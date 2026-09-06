/**
 * ブラウザ版の録音。
 *
 * マイクの取得はブラウザで行い、16kHz / mono / 16bit PCM をサーバへ送る。
 * 書き込みはサーバの `WavSink` が行うため、デスクトップ版と同じ
 * 「逐次ディスク書き込み・落ちてもそこまでは残る」性質が保たれる。
 *
 * # 音声を失わないための工夫
 * - ブラウザ側に溜めるのは最大でも数秒ぶん（`CHUNK_SECONDS`）
 * - 送信に失敗したチャンクは捨てずにキューへ戻し、次の周期で再送する
 * - 停止時は未送信のチャンクを送り切ってから完了とする
 */

/** whisper.cpp が要求するサンプルレート。サーバ側と一致させること。 */
const SAMPLE_RATE = 16_000;
/** 1 回の送信にまとめる秒数。短いほど失う可能性が減り、通信回数は増える。 */
const CHUNK_SECONDS = 1;
/** 送信に失敗したチャンクを保持する上限（これを超えたら古いものから諦める）。 */
const MAX_PENDING_CHUNKS = 120;

export interface BrowserRecorderHandle {
  /** 録音を停止し、未送信ぶんを送り切る。 */
  stop: () => Promise<void>;
  /** 送信できずに保留になっているチャンク数（0 なら遅延なし）。 */
  pendingChunks: () => number;
}

export interface BrowserRecorderOptions {
  meetingId: string;
  /** 送信が滞っている・失敗しているときに通知する。 */
  onWarning?: (message: string) => void;
}

/**
 * マイクを開いて録音を始める。
 *
 * 呼び出し前にサーバ側の `start_recording` を済ませておくこと
 * （サーバが WAV を作ってから音声が届く必要があるため）。
 */
export async function startBrowserRecording(
  options: BrowserRecorderOptions,
): Promise<BrowserRecorderHandle> {
  const { meetingId, onWarning } = options;

  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      channelCount: 1,
      // 会議録音では自動ゲイン・エコー除去が効きすぎると聞き取りにくくなるが、
      // 一般的な会議室のノイズ対策としては有効なので既定で有効にする。
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    },
    video: false,
  });

  // AudioContext を 16kHz で作ると、ブラウザ側がリサンプルまで行ってくれる。
  const context = new AudioContext({ sampleRate: SAMPLE_RATE });
  try {
    await context.audioWorklet.addModule("/pcm-recorder-worklet.js");
  } catch (e) {
    stream.getTracks().forEach((t) => t.stop());
    await context.close();
    throw new Error(
      `録音モジュールを読み込めませんでした: ${e instanceof Error ? e.message : String(e)}`,
    );
  }

  const source = context.createMediaStreamSource(stream);
  const worklet = new AudioWorkletNode(context, "pcm-recorder");
  // ノードをグラフに繋がないと処理が走らないため、音量 0 で出力へ繋ぐ
  // （そのまま destination へ繋ぐとスピーカーから自分の声が返ってしまう）。
  const silence = context.createGain();
  silence.gain.value = 0;
  source.connect(worklet);
  worklet.connect(silence);
  silence.connect(context.destination);

  const chunkSamples = SAMPLE_RATE * CHUNK_SECONDS;
  let buffer: number[] = [];
  /** 送信待ち（未送信・再送待ち）のチャンク。 */
  const pending: Uint8Array[] = [];
  let sending = false;
  let stopped = false;
  let warnedAboutBacklog = false;

  worklet.port.onmessage = (event: MessageEvent<Float32Array>) => {
    if (stopped) return;
    const samples = event.data;
    for (let i = 0; i < samples.length; i += 1) {
      buffer.push(samples[i]);
    }
    while (buffer.length >= chunkSamples) {
      const slice = buffer.slice(0, chunkSamples);
      buffer = buffer.slice(chunkSamples);
      enqueue(toPcm16(slice));
    }
  };

  function enqueue(bytes: Uint8Array) {
    pending.push(bytes);
    if (pending.length > MAX_PENDING_CHUNKS) {
      // ここに来るのはサーバが長時間応答しない場合のみ。
      // 無制限に溜めるとブラウザが落ちるため、やむを得ず古いものから捨てる。
      pending.shift();
      onWarning?.(
        "サーバへの送信が追いつかず、一部の音声を送れませんでした。ネットワークの状態を確認してください。",
      );
    }
    void flush();
  }

  async function flush(): Promise<void> {
    if (sending) return;
    sending = true;
    try {
      while (pending.length > 0) {
        const chunk = pending[0];
        try {
          const response = await fetch(
            `/api/recording/chunk?meetingId=${encodeURIComponent(meetingId)}`,
            {
              method: "POST",
              headers: { "Content-Type": "application/octet-stream" },
              body: chunk as BodyInit,
            },
          );
          if (!response.ok) {
            throw new Error(`サーバがエラーを返しました (${response.status})`);
          }
          pending.shift();
          warnedAboutBacklog = false;
        } catch (e) {
          // 送れなかったチャンクは捨てずに残し、次の周期で再送する。
          if (!warnedAboutBacklog) {
            warnedAboutBacklog = true;
            onWarning?.(
              `録音データの送信に失敗しました。再送を試みています（${
                e instanceof Error ? e.message : String(e)
              }）`,
            );
          }
          break;
        }
      }
    } finally {
      sending = false;
    }
  }

  async function stop(): Promise<void> {
    if (stopped) return;
    stopped = true;
    worklet.port.onmessage = null;

    // 端数も送る
    if (buffer.length > 0) {
      pending.push(toPcm16(buffer));
      buffer = [];
    }

    source.disconnect();
    worklet.disconnect();
    silence.disconnect();
    stream.getTracks().forEach((track) => track.stop());
    await context.close().catch(() => undefined);

    // 未送信ぶんを送り切る。数回リトライしても駄目なら諦めるが、
    // サーバ側には既に送信済みのぶんが WAV として残っている。
    for (let attempt = 0; attempt < 5 && pending.length > 0; attempt += 1) {
      await flush();
      if (pending.length > 0) {
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
    }
    if (pending.length > 0) {
      onWarning?.(
        `${pending.length}秒ぶんの音声をサーバへ送れませんでした。それ以外の録音は保存されています。`,
      );
    }
  }

  return { stop, pendingChunks: () => pending.length };
}

/** f32 (-1.0〜1.0) を 16bit PCM のリトルエンディアン列へ変換する。 */
function toPcm16(samples: readonly number[]): Uint8Array {
  const out = new Uint8Array(samples.length * 2);
  const view = new DataView(out.buffer);
  for (let i = 0; i < samples.length; i += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(i * 2, Math.round(clamped * 32767), true);
  }
  return out;
}
