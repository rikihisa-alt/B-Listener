//! 会議終了後の自動処理の起動と状態取得。

use std::sync::Arc;

use tauri::State;

use crate::db::models::TranscriptSegment;
use crate::error::AppResult;
use crate::server::service;
use crate::server::service::PipelineStatus;
use crate::state::AppState;

#[tauri::command]
pub fn run_pipeline(state: State<'_, Arc<AppState>>, meeting_id: String) -> AppResult<()> {
    service::run_pipeline((*state).clone(), meeting_id)
}

#[tauri::command]
pub fn retry_pipeline(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
    from_transcription: bool,
) -> AppResult<()> {
    service::retry_pipeline((*state).clone(), meeting_id, from_transcription)
}

#[tauri::command]
pub fn get_pipeline_status(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
) -> AppResult<PipelineStatus> {
    service::get_pipeline_status(&state, meeting_id)
}

#[tauri::command]
pub fn get_transcript(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
) -> AppResult<Vec<TranscriptSegment>> {
    service::get_transcript(&state, meeting_id)
}
