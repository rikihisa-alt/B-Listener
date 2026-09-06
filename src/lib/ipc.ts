/**
 * Tauri command の型付きラッパ。
 *
 * UI コンポーネントは `invoke` を直接呼ばず、必ずこのモジュール経由で呼ぶ。
 * これにより IPC の契約が 1 箇所に集まり、型が崩れない。
 */

import { invoke } from "@tauri-apps/api/core";

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
  return invoke<Meeting>("create_meeting", { title: title ?? null });
}

export function getHomeStats(): Promise<HomeStats> {
  return invoke<HomeStats>("get_home_stats");
}

export function listMeetings(): Promise<MeetingListItem[]> {
  return invoke<MeetingListItem[]>("list_meetings");
}

export function getMeetingDetail(meetingId: string): Promise<MeetingDetail> {
  return invoke<MeetingDetail>("get_meeting_detail", { meetingId });
}

export function saveMeetingContext(
  meetingId: string,
  input: MeetingContextInput,
): Promise<MeetingDetail> {
  return invoke<MeetingDetail>("save_meeting_context", { meetingId, input });
}

export function deleteMeeting(meetingId: string): Promise<void> {
  return invoke<void>("delete_meeting", { meetingId });
}

// ---------------------------------------------------------------- settings

export function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export function updateSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { settings });
}

export function resetSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("reset_settings");
}

// ---------------------------------------------------------------- system

export function getSystemInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("get_system_info");
}

export function checkComponents(): Promise<ComponentStatus[]> {
  return invoke<ComponentStatus[]>("check_components");
}

// ---------------------------------------------------------------- recording

export function listInputDevices(): Promise<AudioInputDevice[]> {
  return invoke<AudioInputDevice[]>("list_input_devices");
}

export function startRecording(meetingId: string): Promise<RecordingSnapshot> {
  return invoke<RecordingSnapshot>("start_recording", { meetingId });
}

export function pauseRecording(): Promise<RecordingSnapshot> {
  return invoke<RecordingSnapshot>("pause_recording");
}

export function resumeRecording(): Promise<RecordingSnapshot> {
  return invoke<RecordingSnapshot>("resume_recording");
}

export function stopRecording(): Promise<Meeting> {
  return invoke<Meeting>("stop_recording");
}

export function getRecordingState(): Promise<RecordingSnapshot | null> {
  return invoke<RecordingSnapshot | null>("get_recording_state");
}

export function checkDiskStatus(): Promise<DiskStatus> {
  return invoke<DiskStatus>("check_disk_status");
}

// ---------------------------------------------------------------- recovery

export function getRecoverableMeetings(): Promise<RecoverableMeeting[]> {
  return invoke<RecoverableMeeting[]>("get_recoverable_meetings");
}

export function recoverMeeting(meetingId: string): Promise<Meeting> {
  return invoke<Meeting>("recover_meeting", { meetingId });
}

// ---------------------------------------------------------------- whisper

export function listWhisperModels(): Promise<ModelStatus[]> {
  return invoke<ModelStatus[]>("list_whisper_models");
}

/** ダウンロードはバックグラウンドで進む。進捗は model:download-progress イベントで届く。 */
export function downloadWhisperModel(modelId: string): Promise<void> {
  return invoke<void>("download_whisper_model", { modelId });
}

export function cancelModelDownload(): Promise<void> {
  return invoke<void>("cancel_model_download");
}

// ---------------------------------------------------------------- pipeline

export function runPipeline(meetingId: string): Promise<void> {
  return invoke<void>("run_pipeline", { meetingId });
}

export function retryPipeline(meetingId: string, fromTranscription: boolean): Promise<void> {
  return invoke<void>("retry_pipeline", { meetingId, fromTranscription });
}

export function getPipelineStatus(meetingId: string): Promise<PipelineStatus> {
  return invoke<PipelineStatus>("get_pipeline_status", { meetingId });
}

export function getTranscript(meetingId: string): Promise<TranscriptSegment[]> {
  return invoke<TranscriptSegment[]>("get_transcript", { meetingId });
}

// ---------------------------------------------------------------- files

export function listMeetingFiles(meetingId: string): Promise<MeetingFile[]> {
  return invoke<MeetingFile[]>("list_meeting_files", { meetingId });
}

/** 議事録・まとめ・文字起こしの本文。未作成なら null。 */
export function readMeetingDocument(
  meetingId: string,
  document: MeetingDocument,
): Promise<string | null> {
  return invoke<string | null>("read_meeting_document", { meetingId, document });
}

/** 成果物を任意の場所へ書き出す。保存先の選択は UI 側のダイアログで行う。 */
export function exportMeetingDocument(
  meetingId: string,
  document: MeetingDocument,
  targetPath: string,
): Promise<string> {
  return invoke<string>("export_meeting_document", { meetingId, document, targetPath });
}

export function exportMeetingAudio(meetingId: string, targetPath: string): Promise<string> {
  return invoke<string>("export_meeting_audio", { meetingId, targetPath });
}
