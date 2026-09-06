//! 動作環境の確認とパス情報の提供。

use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::server::service;
use crate::server::service::{ComponentStatus, SystemInfo};
use crate::state::AppState;

#[tauri::command]
pub fn get_system_info(state: State<'_, Arc<AppState>>) -> SystemInfo {
    service::get_system_info(&state)
}

#[tauri::command]
pub fn check_components(state: State<'_, Arc<AppState>>) -> AppResult<Vec<ComponentStatus>> {
    service::check_components(&state)
}
