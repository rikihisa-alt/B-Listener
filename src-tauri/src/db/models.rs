//! DB のドメインモデル。IPC の DTO も兼ねる（serde は camelCase）。
//!
//! フロントエンドの `src/types/ipc.ts` と 1:1 で対応させること。

use serde::{Deserialize, Serialize};

/// 会議の状態。文字列としてそのまま DB に保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingStatus {
    /// 作成済み・未開始
    Draft,
    /// 録音中
    Recording,
    /// 一時停止中
    Paused,
    /// 会議終了後の自動処理中
    Processing,
    /// 完了
    Completed,
    /// 処理に失敗（音声は残っている）
    Failed,
}

impl MeetingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MeetingStatus::Draft => "draft",
            MeetingStatus::Recording => "recording",
            MeetingStatus::Paused => "paused",
            MeetingStatus::Processing => "processing",
            MeetingStatus::Completed => "completed",
            MeetingStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "recording" => MeetingStatus::Recording,
            "paused" => MeetingStatus::Paused,
            "processing" => MeetingStatus::Processing,
            "completed" => MeetingStatus::Completed,
            "failed" => MeetingStatus::Failed,
            _ => MeetingStatus::Draft,
        }
    }

    /// アプリ起動時にこの状態だった会議は、クラッシュとみなして復旧対象にする。
    pub fn is_interrupted(self) -> bool {
        matches!(self, MeetingStatus::Recording | MeetingStatus::Paused)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub scheduled_at: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_ms: i64,
    pub status: MeetingStatus,
    pub folder_path: Option<String>,
    pub audio_path: Option<String>,
    pub audio_format: Option<String>,
    pub sample_rate: Option<i64>,
    pub transcript_path: Option<String>,
    pub minutes_path: Option<String>,
    pub summary_path: Option<String>,
    pub goal: String,
    pub carryover: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 一覧表示用の軽量な行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingListItem {
    pub id: String,
    pub title: String,
    pub started_at: Option<String>,
    pub created_at: String,
    pub duration_ms: i64,
    pub status: MeetingStatus,
    pub has_audio: bool,
    pub has_minutes: bool,
    pub has_summary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agenda {
    pub id: String,
    pub text: String,
    pub sort_order: i64,
}

/// AI 補助情報の分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TermCategory {
    /// 人名
    Person,
    /// 利用者名
    User,
    /// 顧客名
    Customer,
    /// 会社名
    Company,
    /// サービス名
    Service,
    /// 専門用語
    Jargon,
    /// 略称
    Abbrev,
    /// その他固有名詞
    Other,
}

impl TermCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            TermCategory::Person => "person",
            TermCategory::User => "user",
            TermCategory::Customer => "customer",
            TermCategory::Company => "company",
            TermCategory::Service => "service",
            TermCategory::Jargon => "jargon",
            TermCategory::Abbrev => "abbrev",
            TermCategory::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "person" => TermCategory::Person,
            "user" => TermCategory::User,
            "customer" => TermCategory::Customer,
            "company" => TermCategory::Company,
            "service" => TermCategory::Service,
            "jargon" => TermCategory::Jargon,
            "abbrev" => TermCategory::Abbrev,
            _ => TermCategory::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextTerm {
    pub id: String,
    pub term: String,
    pub category: TermCategory,
    pub reading: String,
    pub sort_order: i64,
}

/// 会議の全情報（詳細画面と事前入力画面が使う）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    pub meeting: Meeting,
    pub participants: Vec<Participant>,
    pub agendas: Vec<Agenda>,
    pub terms: Vec<ContextTerm>,
}

/// 事前入力の保存リクエスト。すべて任意項目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingContextInput {
    pub title: Option<String>,
    pub scheduled_at: Option<String>,
    pub goal: Option<String>,
    pub carryover: Option<String>,
    pub notes: Option<String>,
    pub participants: Option<Vec<String>>,
    pub agendas: Option<Vec<String>>,
    pub terms: Option<Vec<ContextTermInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextTermInput {
    pub term: String,
    pub category: TermCategory,
    #[serde(default)]
    pub reading: String,
}

// ------------------------------------------------------- 文字起こし・処理状態

/// 文字起こしの種別。
///
/// `Realtime` は会議中の確認用、`Final` は会議終了後に音声全体から作り直したもの。
/// 最終議事録は必ず `Final` のみを使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptKind {
    Realtime,
    Final,
}

impl TranscriptKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TranscriptKind::Realtime => "realtime",
            TranscriptKind::Final => "final",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: String,
    pub kind: TranscriptKind,
    pub seq: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub raw_text: String,
}

/// 会議終了後パイプラインのステップ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStep {
    /// 音声ファイルの確定
    FinalizeAudio,
    /// 高精度文字起こし
    Transcribe,
    /// 事前情報による用語補正
    Correct,
    /// チャンクごとの構造化抽出
    Extract,
    /// 抽出結果の統合
    Merge,
    /// 議事録生成
    Minutes,
    /// AIまとめ生成
    Summary,
    /// 保存
    Persist,
}

impl PipelineStep {
    pub fn as_str(self) -> &'static str {
        match self {
            PipelineStep::FinalizeAudio => "finalize_audio",
            PipelineStep::Transcribe => "transcribe",
            PipelineStep::Correct => "correct",
            PipelineStep::Extract => "extract",
            PipelineStep::Merge => "merge",
            PipelineStep::Minutes => "minutes",
            PipelineStep::Summary => "summary",
            PipelineStep::Persist => "persist",
        }
    }

    /// 画面に出す日本語名。
    pub fn label(self) -> &'static str {
        match self {
            PipelineStep::FinalizeAudio => "音声ファイルの確定",
            PipelineStep::Transcribe => "文字起こし",
            PipelineStep::Correct => "用語の補正",
            PipelineStep::Extract => "内容の抽出",
            PipelineStep::Merge => "抽出結果の統合",
            PipelineStep::Minutes => "議事録の作成",
            PipelineStep::Summary => "まとめの作成",
            PipelineStep::Persist => "保存",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "finalize_audio" => PipelineStep::FinalizeAudio,
            "transcribe" => PipelineStep::Transcribe,
            "correct" => PipelineStep::Correct,
            "extract" => PipelineStep::Extract,
            "merge" => PipelineStep::Merge,
            "minutes" => PipelineStep::Minutes,
            "summary" => PipelineStep::Summary,
            "persist" => PipelineStep::Persist,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "running" => JobState::Running,
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            _ => JobState::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRun {
    pub step: PipelineStep,
    /// 画面表示用のステップ名。
    pub label: String,
    pub state: JobState,
    pub attempt: i64,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
