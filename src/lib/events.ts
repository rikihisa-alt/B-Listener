/**
 * Rust から届くイベントの型付き購読。
 *
 * デスクトップ版は Tauri のイベント、ブラウザ版は SSE（Server-Sent Events）で
 * まったく同じイベント名・同じ payload が届く。差はこのモジュールで吸収する。
 *
 * UI コンポーネントは `listen` や `EventSource` を直接使わない。
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { isDesktop } from "./runtime";
import type {
  ModelDownloadProgress,
  PipelineFailure,
  PipelineProgress,
  RecordingSnapshot,
} from "@/types/ipc";

// ------------------------------------------------------- SSE（ブラウザ版）

/** SSE 接続は 1 本だけ張り、購読者で共有する。 */
let sharedSource: EventSource | null = null;
let subscriberCount = 0;

function acquireSource(): EventSource {
  if (!sharedSource) {
    sharedSource = new EventSource("/api/events");
    sharedSource.onerror = () => {
      // EventSource は自動で再接続する。切断のたびに画面へ出すと
      // うるさいので、記録だけ残す。
      console.warn("サーバとの接続が切れました。自動的に再接続します。");
    };
  }
  subscriberCount += 1;
  return sharedSource;
}

function releaseSource() {
  subscriberCount -= 1;
  if (subscriberCount <= 0 && sharedSource) {
    sharedSource.close();
    sharedSource = null;
    subscriberCount = 0;
  }
}

/** イベント名を指定して購読する。戻り値を呼ぶと購読を解除する。 */
function subscribe<T>(name: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (isDesktop) {
    return listen<T>(name, (event) => handler(event.payload));
  }

  const source = acquireSource();
  const listener = (event: MessageEvent<string>) => {
    try {
      handler(JSON.parse(event.data) as T);
    } catch (e) {
      // 1 件壊れていても購読は続ける
      console.error(`イベント ${name} を解釈できませんでした`, e);
    }
  };
  source.addEventListener(name, listener as EventListener);

  return Promise.resolve(() => {
    source.removeEventListener(name, listener as EventListener);
    releaseSource();
  });
}

// ---------------------------------------------------------------- recording

/** 録音中の経過時間・入力レベル（約0.5秒間隔） */
export function onRecordingTick(
  handler: (snapshot: RecordingSnapshot) => void,
): Promise<UnlistenFn> {
  return subscribe<RecordingSnapshot>("recording:tick", handler);
}

/** 録音を継続できないエラー（デバイス切断など）。音声はここまでのぶんが残る。 */
export function onRecordingError(handler: (message: string) => void): Promise<UnlistenFn> {
  return subscribe<string>("recording:error", handler);
}

/** 保存先の空き容量が不足しはじめた警告 */
export function onDiskWarning(handler: (message: string) => void): Promise<UnlistenFn> {
  return subscribe<string>("recording:disk-warning", handler);
}

// ---------------------------------------------------------------- pipeline

/** 会議終了後の処理の進捗 */
export function onPipelineProgress(
  handler: (progress: PipelineProgress) => void,
): Promise<UnlistenFn> {
  return subscribe<PipelineProgress>("pipeline:progress", handler);
}

export function onPipelineDone(handler: (meetingId: string) => void): Promise<UnlistenFn> {
  return subscribe<string>("pipeline:done", handler);
}

/** 処理が失敗したときの通知。`preserved` に「残っているもの」が入る。 */
export function onPipelineFailed(handler: (failure: PipelineFailure) => void): Promise<UnlistenFn> {
  return subscribe<PipelineFailure>("pipeline:failed", handler);
}

// ---------------------------------------------------------------- model

export function onModelDownloadProgress(
  handler: (progress: ModelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return subscribe<ModelDownloadProgress>("model:download-progress", handler);
}

export function onModelDownloadDone(handler: (modelId: string) => void): Promise<UnlistenFn> {
  return subscribe<string>("model:download-done", handler);
}

export function onModelDownloadFailed(handler: (message: string) => void): Promise<UnlistenFn> {
  return subscribe<string>("model:download-failed", handler);
}
