/**
 * ブラウザのマイク入力を、そのままメインスレッドへ渡すだけの AudioWorklet。
 *
 * AudioContext を 16kHz で作っているため、ここに届く時点で
 * whisper.cpp が要求する 16kHz mono になっている。
 * 重い処理はhere行わず、コピーして送るだけにする（音が途切れると録音が欠けるため）。
 */
class PcmRecorderProcessor extends AudioWorkletProcessor {
  process(inputs) {
    const channel = inputs[0] && inputs[0][0];
    if (channel && channel.length > 0) {
      // process() に渡されるバッファは使い回されるため、必ずコピーして送る
      this.port.postMessage(new Float32Array(channel));
    }
    // false を返すとノードが破棄される。録音中は常に true。
    return true;
  }
}

registerProcessor("pcm-recorder", PcmRecorderProcessor);
