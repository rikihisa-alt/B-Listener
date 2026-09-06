/**
 * Tauri command の型付きラッパ。
 *
 * UI コンポーネントは `invoke` を直接呼ばず、必ずこのモジュール経由で呼ぶ。
 * これにより IPC の契約が 1 箇所に集まり、型が崩れない。
 */

import { invoke } from "@tauri-apps/api/core";

import { isDesktop } from "./runtime";

/**
 * コマンド呼び出しの共通口。
 *
 * デスクトップ版は Tauri の `invoke`、ブラウザ版は `POST /api/command/<name>` を使う。
 * どちらもコマンド名と引数はまったく同じなので、この 1 箇所だけで吸収できる。
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isDesktop) {
    return invoke<T>(command, args);
  }

  const response = await fetch(`/api/command/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });

  // エラーは Tauri 版と同じ {code, message} の形で返る
  if (!response.ok) {
    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      payload = { code: "OTHER", message: `サーバとの通信に失敗しました (${response.status})` };
    }
    throw payload;
  }

  return (await response.json()) as T;
}

import type {
  AppSettings,
  AudioInputDevice,
  ComponentStatus,
  DiskStatus,
  HomeStats,
  Meeting,
  MeetingContextInput,
  MeetingDetail,
  MeetingDocument,
  MeetingFile,
  MeetingListItem,
  ModelStatus,
  PipelineStatus,
  RecordingSnapshot,
  RecoverableMeeting,
  SystemInfo,
  TranscriptSegment,
} from "@/types/ipc";

// ---------------------------------------------------------------- meeting

export function createMeeting(title?: string): Promise<Meeting> {
  return call<Meeting>("create_meeting", { title: title ?? null });
}

export function getHomeStats(): Promise<HomeStats> {
  return call<HomeStats>("get_home_stats");
}

export function listMeetings(): Promise<MeetingListItem[]> {
  return call<MeetingListItem[]>("list_meetings");
}

export function getMeetingDetail(meetingId: string): Promise<MeetingDetail> {
  return call<MeetingDetail>("get_meeting_detail", { meetingId });
}

export function saveMeetingContext(
  meetingId: string,
  input: MeetingContextInput,
): Promise<MeetingDetail> {
  return call<MeetingDetail>("save_meeting_context", { meetingId, input });
}

export function deleteMeeting(meetingId: string): Promise<void> {
  return call<void>("delete_meeting", { meetingId });
}

// ---------------------------------------------------------------- settings

export function getSettings(): Promise<AppSettings> {
  return call<AppSettings>("get_settings");
}

export function updateSettings(settings: AppSettings): Promise<AppSettings> {
  return call<AppSettings>("update_settings", { settings });
}

export function resetSettings(): Promise<AppSettings> {
  return call<AppSettings>("reset_settings");
}

// ---------------------------------------------------------------- system

export function getSystemInfo(): Promise<SystemInfo> {
  return call<SystemInfo>("get_system_info");
}

export function checkComponents(): Promise<ComponentStatus[]> {
  return call<ComponentStatus[]>("check_components");
}

// ---------------------------------------------------------------- recording

export function listInputDevices(): Promise<AudioInputDevice[]> {
  return call<AudioInputDevice[]>("list_input_devices");
}

/** `clientLabel` はブラウザ版で「どの端末から録音しているか」を表示するために使う。 */
export function startRecording(
  meetingId: string,
  clientLabel?: string,
): Promise<RecordingSnapshot> {
  return call<RecordingSnapshot>("start_recording", { meetingId, clientLabel });
}

export function pauseRecording(): Promise<RecordingSnapshot> {
  return call<RecordingSnapshot>("pause_recording");
}

export function resumeRecording(): Promise<RecordingSnapshot> {
  return call<RecordingSnapshot>("resume_recording");
}

export function stopRecording(): Promise<Meeting> {
  return call<Meeting>("stop_recording");
}

export function getRecordingState(): Promise<RecordingSnapshot | null> {
  return call<RecordingSnapshot | null>("get_recording_state");
}

export function checkDiskStatus(): Promise<DiskStatus> {
  return call<DiskStatus>("check_disk_status");
}

// ---------------------------------------------------------------- recovery

export function getRecoverableMeetings(): Promise<RecoverableMeeting[]> {
  return call<RecoverableMeeting[]>("get_recoverable_meetings");
}

export function recoverMeeting(meetingId: string): Promise<Meeting> {
  return call<Meeting>("recover_meeting", { meetingId });
}

// ---------------------------------------------------------------- whisper

export function listWhisperModels(): Promise<ModelStatus[]> {
  return call<ModelStatus[]>("list_whisper_models");
}

/** ダウンロードはバックグラウンドで進む。進捗は model:download-progress イベントで届く。 */
export function downloadWhisperModel(modelId: string): Promise<void> {
  return call<void>("download_whisper_model", { modelId });
}

export function cancelModelDownload(): Promise<void> {
  return call<void>("cancel_model_download");
}

// ---------------------------------------------------------------- pipeline

export function runPipeline(meetingId: string): Promise<void> {
  return call<void>("run_pipeline", { meetingId });
}

export function retryPipeline(meetingId: string, fromTranscription: boolean): Promise<void> {
  return call<void>("retry_pipeline", { meetingId, fromTranscription });
}

export function getPipelineStatus(meetingId: string): Promise<PipelineStatus> {
  return call<PipelineStatus>("get_pipeline_status", { meetingId });
}

export function getTranscript(meetingId: string): Promise<TranscriptSegment[]> {
  return call<TranscriptSegment[]>("get_transcript", { meetingId });
}

// ---------------------------------------------------------------- files

export function listMeetingFiles(meetingId: string): Promise<MeetingFile[]> {
  return call<MeetingFile[]>("list_meeting_files", { meetingId });
}

/** 議事録・まとめ・文字起こしの本文。未作成なら null。 */
export function readMeetingDocument(
  meetingId: string,
  document: MeetingDocument,
): Promise<string | null> {
  return call<string | null>("read_meeting_document", { meetingId, document });
}

/** 成果物を任意の場所へ書き出す。保存先の選択は UI 側のダイアログで行う。 */
export function exportMeetingDocument(
  meetingId: string,
  document: MeetingDocument,
  targetPath: string,
): Promise<string> {
  return call<string>("export_meeting_document", { meetingId, document, targetPath });
}

export function exportMeetingAudio(meetingId: string, targetPath: string): Promise<string> {
  return call<string>("export_meeting_audio", { meetingId, targetPath });
}
