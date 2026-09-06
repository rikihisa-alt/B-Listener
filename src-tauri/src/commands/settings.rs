//! 設定の取得・更新。

use tauri::State;

use crate::error::AppResult;
use crate::settings::AppSettings;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.get()
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    let saved = state.settings.update(settings)?;
    tracing::info!("設定を更新しました");
    Ok(saved)
}

/// 既定値へ戻す。
#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    let defaults = AppSettings::with_defaults(state.paths.default_meetings_dir.clone());
    state.settings.update(defaults)
}
