//! アプリからUIへ送る通知の抽象化。
//!
//! デスクトップ版は Tauri のイベント、ブラウザ版は SSE（Server-Sent Events）で
//! 同じ内容を配信する。コア処理（`pipeline` や録音）はどちらで動いているかを
//! 知る必要がないため、`EventSink` 越しにだけ通知する。

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::audio::recorder::RecordingSnapshot;
use crate::pipeline::{PipelineFailure, PipelineProgress};
use crate::stt::models::DownloadProgress;

/// UI へ送る通知。イベント名と JSON ペイロードは
/// デスクトップ版・ブラウザ版で完全に同じものを使う。
#[derive(Debug, Clone, Serialize)]
pub enum AppEvent {
    /// 録音中の経過時間・入力レベル（約0.5秒間隔）
    RecordingTick(RecordingSnapshot),
    /// 録音を継続できないエラー（デバイス切断など）
    RecordingError(String),
    /// 保存先の空き容量が不足しはじめた警告
    RecordingDiskWarning(String),

    /// 会議終了後の処理の進捗
    PipelineProgress(PipelineProgress),
    /// 会議終了後の処理が完了した（値は会議ID）
    PipelineDone(String),
    /// 会議終了後の処理が失敗した
    PipelineFailed(PipelineFailure),

    /// 音声認識モデルのダウンロード進捗
    ModelDownloadProgress(DownloadProgress),
    /// モデルのダウンロード完了（値はモデルID）
    ModelDownloadDone(String),
    /// モデルのダウンロード失敗（値はメッセージ）
    ModelDownloadFailed(String),
}

impl AppEvent {
    /// フロントエンドが購読する際の名前。
    pub fn name(&self) -> &'static str {
        match self {
            AppEvent::RecordingTick(_) => "recording:tick",
            AppEvent::RecordingError(_) => "recording:error",
            AppEvent::RecordingDiskWarning(_) => "recording:disk-warning",
            AppEvent::PipelineProgress(_) => "pipeline:progress",
            AppEvent::PipelineDone(_) => "pipeline:done",
            AppEvent::PipelineFailed(_) => "pipeline:failed",
            AppEvent::ModelDownloadProgress(_) => "model:download-progress",
            AppEvent::ModelDownloadDone(_) => "model:download-done",
            AppEvent::ModelDownloadFailed(_) => "model:download-failed",
        }
    }

    /// 通知の中身。失敗しても通知が止まらないよう、
    /// シリアライズできない場合は null を返して記録だけ残す。
    pub fn payload(&self) -> Value {
        let result = match self {
            AppEvent::RecordingTick(v) => serde_json::to_value(v),
            AppEvent::RecordingError(v) => serde_json::to_value(v),
            AppEvent::RecordingDiskWarning(v) => serde_json::to_value(v),
            AppEvent::PipelineProgress(v) => serde_json::to_value(v),
            AppEvent::PipelineDone(v) => serde_json::to_value(v),
            AppEvent::PipelineFailed(v) => serde_json::to_value(v),
            AppEvent::ModelDownloadProgress(v) => serde_json::to_value(v),
            AppEvent::ModelDownloadDone(v) => serde_json::to_value(v),
            AppEvent::ModelDownloadFailed(v) => serde_json::to_value(v),
        };
        match result {
            Ok(value) => value,
            Err(e) => {
                tracing::error!(event = self.name(), error = %e, "通知の変換に失敗しました");
                Value::Null
            }
        }
    }
}

/// 通知の送り先。デスクトップ版・ブラウザ版でそれぞれ実装する。
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: AppEvent);
}

/// 通知を捨てる実装。テストや、UI を持たない実行時に使う。
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: AppEvent) {}
}

pub type SharedEventSink = Arc<dyn EventSink>;
