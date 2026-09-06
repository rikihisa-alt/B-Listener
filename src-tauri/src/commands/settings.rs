//! 設定の取得・更新。

use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::server::service;
use crate::settings::AppSettings;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> AppSettings {
    service::get_settings(&state)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    service::update_settings(&state, settings)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, Arc<AppState>>) -> AppResult<AppSettings> {
    service::reset_settings(&state)
}
