/**
 * Rust 側の serde 定義と 1:1 で対応する IPC 型。
 * ここが IPC の型の「正」であり、UI からはこの型のみを使う（any 禁止）。
 *
 * 対応する Rust: src-tauri/src/db/models.rs, settings.rs, commands/system.rs, error.rs
 */

// ---------------------------------------------------------------- error

/** Rust の AppError。invoke が reject したときにこの形で返る。 */
export interface AppErrorPayload {
  code:
    | "DB"
    | "IO"
    | "SETTINGS"
    | "NOT_FOUND"
    | "INVALID"
    | "AUDIO"
    | "STT"
    | "LLM"
    | "DISK_SPACE"
    | "MISSING_COMPONENT"
    | "OTHER";
  message: string;
}

// ---------------------------------------------------------------- meeting

export type MeetingStatus =
  | "draft"
  | "recording"
  | "paused"
  | "processing"
  | "completed"
  | "failed";

export interface Meeting {
  id: string;
  title: string;
  scheduledAt: string | null;
  startedAt: string | null;
  endedAt: string | null;
  durationMs: number;
  status: MeetingStatus;
  folderPath: string | null;
  audioPath: string | null;
  audioFormat: string | null;
  sampleRate: number | null;
  transcriptPath: string | null;
  minutesPath: string | null;
  summaryPath: string | null;
  goal: string;
  carryover: string;
  notes: string;
  createdAt: string;
  updatedAt: string;
}

export interface MeetingListItem {
  id: string;
  title: string;
  startedAt: string | null;
  createdAt: string;
  durationMs: number;
  status: MeetingStatus;
  hasAudio: boolean;
  hasMinutes: boolean;
  hasSummary: boolean;
}

export interface Participant {
  id: string;
  name: string;
  sortOrder: number;
}

export interface Agenda {
  id: string;
  text: string;
  sortOrder: number;
}

/** 事前入力の AI 補助情報の分類 */
export type TermCategory =
  | "person"
  | "user"
  | "customer"
  | "company"
  | "service"
  | "jargon"
  | "abbrev"
  | "other";

export interface ContextTerm {
  id: string;
  term: string;
  category: TermCategory;
  reading: string;
  sortOrder: number;
}

export interface MeetingDetail {
  meeting: Meeting;
  participants: Participant[];
  agendas: Agenda[];
  terms: ContextTerm[];
}

export interface ContextTermInput {
  term: string;
  category: TermCategory;
  reading: string;
}

/** 事前入力。すべて任意。未指定 (undefined) の項目は変更されない。 */
export interface MeetingContextInput {
  title?: string;
  scheduledAt?: string;
  goal?: string;
  carryover?: string;
  notes?: string;
  participants?: string[];
  agendas?: string[];
  terms?: ContextTermInput[];
}

// ---------------------------------------------------------------- settings

export type LlmProviderKind = "ollama" | "open-ai-compatible";

export interface AppSettings {
  meetingsDir: string;
  whisperModel: string;
  whisperRealtimeModel: string;
  llmEndpoint: string;
  llmModel: string;
  llmProvider: LlmProviderKind;
  realtimeTranscriptionEnabled: boolean;
  realtimeAnalysisEnabled: boolean;
  realtimeAnalysisIntervalSecs: number;
  aiSummaryEnabled: boolean;
  inputDevice: string | null;
  minFreeDiskMb: number;
  keepWavAfterCompress: boolean;
}

// ---------------------------------------------------------------- system

export type ComponentState = "ready" | "missing" | "not-checked";

export interface ComponentStatus {
  name: string;
  state: ComponentState;
  message: string;
  setupHint: string;
  expectedPath: string | null;
}

export interface SystemInfo {
  appVersion: string;
  os: string;
  appDataDir: string;
  logDir: string;
  modelsDir: string;
  meetingsDir: string;
}

// ---------------------------------------------------------------- recording

export interface AudioInputDevice {
  name: string;
  isDefault: boolean;
}

export type RecorderState = "recording" | "paused";

/** 録音状態のスナップショット。`recording:tick` イベントでも同じ形が届く。 */
export interface RecordingSnapshot {
  meetingId: string;
  state: RecorderState;
  /** 実際に録音された長さ。一時停止分は含まない。 */
  elapsedMs: number;
  /** 直近の入力レベル (0.0〜1.0) */
  level: number;
  bytesWritten: number;
  audioPath: string;
  deviceName: string;
  /** 録音を継続できないエラー。発生後も既存の音声は保全される。 */
  error: string | null;
}

export type WavCondition =
  | "intact"
  | "header-outdated"
  | "header-broken"
  | "empty"
  | "missing";

export interface WavInspection {
  path: string;
  condition: WavCondition;
  headerDataBytes: number;
  actualDataBytes: number;
  sampleRate: number;
  channels: number;
  durationMs: number;
}

export interface RecoverableMeeting {
  meeting: Meeting;
  inspection: WavInspection;
  canRecover: boolean;
}

export interface DiskStatus {
  availableBytes: number;
  requiredBytes: number;
  sufficient: boolean;
  estimatedMinutes: number;
}

// ---------------------------------------------------------------- whisper

export interface ModelStatus {
  id: string;
  label: string;
  approxSizeMb: number;
  suitableForRealtime: boolean;
  installed: boolean;
  path: string;
  sizeBytes: number;
}

export interface ModelDownloadProgress {
  modelId: string;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
}

// ---------------------------------------------------------------- pipeline

/** 会議終了後の処理ステップ */
export type PipelineStep =
  | "finalize_audio"
  | "transcribe"
  | "correct"
  | "extract"
  | "merge"
  | "minutes"
  | "summary"
  | "persist";

export type JobState = "pending" | "running" | "done" | "failed";

export interface JobRun {
  step: PipelineStep;
  label: string;
  state: JobState;
  attempt: number;
  error: string | null;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface PipelineStatus {
  meetingId: string;
  status: MeetingStatus;
  running: boolean;
  jobs: JobRun[];
}

export interface PipelineProgress {
  meetingId: string;
  step: PipelineStep;
  label: string;
  stepIndex: number;
  stepTotal: number;
  percent: number;
  message: string;
}

/** 処理が失敗しても残っているものを `preserved` で伝える。 */
export interface PipelineFailure {
  meetingId: string;
  step: PipelineStep;
  label: string;
  code: string;
  message: string;
  preserved: string[];
}

export type TranscriptKind = "realtime" | "final";

export interface TranscriptSegment {
  id: string;
  kind: TranscriptKind;
  seq: number;
  startMs: number;
  endMs: number;
  text: string;
  rawText: string;
}

// ---------------------------------------------------------------- stats

export interface HomeStats {
  meetingsThisMonth: number;
  unprocessed: number;
  savedAudio: number;
}

// ---------------------------------------------------------------- files

export type MeetingDocument = "transcript" | "minutes" | "summary";

export interface MeetingFile {
  name: string;
  path: string;
  sizeBytes: number;
  isAudio: boolean;
}
