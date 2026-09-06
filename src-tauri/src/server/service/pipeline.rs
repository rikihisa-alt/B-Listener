//! （`commands/pipeline.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 会議終了後の自動処理の起動と状態取得。

use std::sync::Arc;

use serde::Serialize;

use crate::db::models::{JobRun, MeetingStatus, TranscriptKind, TranscriptSegment};
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::pipeline;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStatus {
    pub meeting_id: String,
    pub status: MeetingStatus,
    pub running: bool,
    pub jobs: Vec<JobRun>,
}

/// 会議終了後の処理を開始する。
pub fn run_pipeline(state: Arc<AppState>, meeting_id: String) -> AppResult<()> {
    // 会議の存在確認をここで済ませ、存在しないIDでスレッドを起こさない。
    state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;
    pipeline::spawn(state, meeting_id);
    Ok(())
}

/// 失敗したステップから処理をやり直す。
///
/// 完了済みのステップはスキップされるため、文字起こしが成功していれば
/// AI 処理だけを再実行できる。
/// `from_transcription` が true なら文字起こしからやり直す。
pub fn retry_pipeline(
    state: Arc<AppState>,
    meeting_id: String,
    from_transcription: bool,
) -> AppResult<()> {
    state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;

    if from_transcription {
        state.db.with_conn(|conn| {
            repo::clear_job_runs(conn, &meeting_id, &["transcribe", "correct"])
        })?;
    }

    pipeline::spawn(state, meeting_id);
    Ok(())
}

pub fn get_pipeline_status(state: &AppState, meeting_id: String) -> AppResult<PipelineStatus> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;
    let jobs = state
        .db
        .with_conn(|conn| repo::list_job_runs(conn, &meeting_id))?;

    let running = match state.running_pipelines.lock() {
        Ok(g) => g.contains(&meeting_id),
        Err(poisoned) => poisoned.into_inner().contains(&meeting_id),
    };

    Ok(PipelineStatus {
        meeting_id,
        status: meeting.status,
        running,
        jobs,
    })
}

/// 最終文字起こしを取得する。まだ無い場合は空配列を返す。
pub fn get_transcript(state: &AppState, meeting_id: String) -> AppResult<Vec<TranscriptSegment>> {
    state
        .db
        .with_conn(|conn| repo::list_transcript_segments(conn, &meeting_id, TranscriptKind::Final))
        .map_err(|e| match e {
            AppError::Db(m) => AppError::Db(m),
            other => other,
        })
}
