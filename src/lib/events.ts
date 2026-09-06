/**
 * Rust から届くイベントの型付き購読。
 *
 * 会議中の画面更新をポーリングに頼らず、イベント駆動にするためのラッパ。
 * UI コンポーネントは `listen` を直接呼ばず、ここで定義した関数を使う。
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ModelDownloadProgress,
  PipelineFailure,
  PipelineProgress,
  RecordingSnapshot,
} from "@/types/ipc";

/** 録音中の経過時間・入力レベル（約0.5秒間隔） */
export function onRecordingTick(
  handler: (snapshot: RecordingSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<RecordingSnapshot>("recording:tick", (event) => handler(event.payload));
}

/** 録音を継続できないエラー（デバイス切断など）。音声はここまでのぶんが残る。 */
export function onRecordingError(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("recording:error", (event) => handler(event.payload));
}

/** 保存先の空き容量が不足しはじめた警告 */
export function onDiskWarning(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("recording:disk-warning", (event) => handler(event.payload));
}

// ---------------------------------------------------------------- pipeline

/** 会議終了後の処理の進捗 */
export function onPipelineProgress(
  handler: (progress: PipelineProgress) => void,
): Promise<UnlistenFn> {
  return listen<PipelineProgress>("pipeline:progress", (e) => handler(e.payload));
}

export function onPipelineDone(handler: (meetingId: string) => void): Promise<UnlistenFn> {
  return listen<string>("pipeline:done", (e) => handler(e.payload));
}

/** 処理が失敗したときの通知。`preserved` に「残っているもの」が入る。 */
export function onPipelineFailed(
  handler: (failure: PipelineFailure) => void,
): Promise<UnlistenFn> {
  return listen<PipelineFailure>("pipeline:failed", (e) => handler(e.payload));
}

// ---------------------------------------------------------------- model

export function onModelDownloadProgress(
  handler: (progress: ModelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<ModelDownloadProgress>("model:download-progress", (e) => handler(e.payload));
}

export function onModelDownloadDone(handler: (modelId: string) => void): Promise<UnlistenFn> {
  return listen<string>("model:download-done", (e) => handler(e.payload));
}

export function onModelDownloadFailed(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("model:download-failed", (e) => handler(e.payload));
}
