//! 録音の開始・一時停止・再開・終了と、クラッシュ復旧。

use std::sync::Arc;

use tauri::State;

use crate::audio::devices::AudioInputDevice;
use crate::audio::recorder::RecordingSnapshot;
use crate::db::models::Meeting;
use crate::error::AppResult;
use crate::server::service;
use crate::server::service::{DiskStatus, RecoverableMeeting};
use crate::state::AppState;

#[tauri::command]
pub fn list_input_devices() -> AppResult<Vec<AudioInputDevice>> {
    service::list_input_devices()
}

#[tauri::command]
pub fn start_recording(
    state: State<'_, Arc<AppState>>,
    meeting_id: String,
) -> AppResult<RecordingSnapshot> {
    service::start_recording(&state, meeting_id, service::RecordingSource::NativeMic)
}

#[tauri::command]
pub fn pause_recording(state: State<'_, Arc<AppState>>) -> AppResult<RecordingSnapshot> {
    service::pause_recording(&state)
}

#[tauri::command]
pub fn resume_recording(state: State<'_, Arc<AppState>>) -> AppResult<RecordingSnapshot> {
    service::resume_recording(&state)
}

#[tauri::command]
pub fn get_recording_state(state: State<'_, Arc<AppState>>) -> Option<RecordingSnapshot> {
    service::get_recording_state(&state)
}

#[tauri::command]
pub fn stop_recording(state: State<'_, Arc<AppState>>) -> AppResult<Meeting> {
    service::stop_recording(&state)
}

#[tauri::command]
pub fn get_recoverable_meetings(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<RecoverableMeeting>> {
    service::get_recoverable_meetings(&state)
}

#[tauri::command]
pub fn recover_meeting(state: State<'_, Arc<AppState>>, meeting_id: String) -> AppResult<Meeting> {
    service::recover_meeting(&state, meeting_id)
}

#[tauri::command]
pub fn check_disk_status(state: State<'_, Arc<AppState>>) -> DiskStatus {
    service::check_disk_status(&state)
}
