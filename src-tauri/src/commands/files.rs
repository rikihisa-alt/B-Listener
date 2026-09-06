//! 会議フォルダ内の成果物（音声・文字起こし・議事録・まとめ）の参照と書き出し。

use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::server::service;
use crate::server::service::{MeetingDocument, MeetingFile};
use crate::state::AppState;

#[tauri::command]
pub fn list_meeting_files(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
) -> AppResult<Vec<MeetingFile>> {
    service::list_meeting_files(&state, meeting_id)
}

#[tauri::command]
pub fn read_meeting_document(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
    document: MeetingDocument,
) -> AppResult<Option<String>> {
    service::read_meeting_document(&state, meeting_id, document)
}

#[tauri::command]
pub fn export_meeting_document(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
    document: MeetingDocument,
    target_path: String,
) -> AppResult<String> {
    service::export_meeting_document(&state, meeting_id, document, target_path)
}

#[tauri::command]
pub fn export_meeting_audio(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
    target_path: String,
) -> AppResult<String> {
    service::export_meeting_audio(&state, meeting_id, target_path)
}
