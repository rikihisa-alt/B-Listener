//! 会議の作成・取得・更新・削除。

use std::sync::Arc;

use tauri::State;

use crate::db::models::{Meeting, MeetingContextInput, MeetingDetail, MeetingListItem};
use crate::db::repo::HomeStats;
use crate::error::AppResult;
use crate::server::service;
use crate::state::AppState;

#[tauri::command]
pub fn create_meeting(
    state: State<'_, Arc<AppState>>,
    title: Option<String>,
) -> AppResult<Meeting> {
    service::create_meeting(&state, title)
}

#[tauri::command]
pub fn get_home_stats(state: State<'_, Arc<AppState>>) -> AppResult<HomeStats> {
    service::get_home_stats(&state)
}

#[tauri::command]
pub fn list_meetings(state: State<'_, Arc<AppState>>) -> AppResult<Vec<MeetingListItem>> {
    service::list_meetings(&state)
}

#[tauri::command]
pub fn get_meeting_detail(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
) -> AppResult<MeetingDetail> {
    service::get_meeting_detail(&state, meeting_id)
}

#[tauri::command]
pub fn save_meeting_context(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
    input: MeetingContextInput,
) -> AppResult<MeetingDetail> {
    service::save_meeting_context(&state, meeting_id, input)
}

#[tauri::command]
pub fn delete_meeting(state: State<'_, Arc<AppState>>, meeting_id: String) -> AppResult<()> {
    service::delete_meeting(&state, meeting_id)
}
