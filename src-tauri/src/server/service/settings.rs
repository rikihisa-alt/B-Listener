//! （`commands/settings.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 設定の取得・更新。

use crate::error::AppResult;
use crate::settings::AppSettings;
use crate::state::AppState;

pub fn get_settings(state: &AppState) -> AppSettings {
    state.settings.get()
}

pub fn update_settings(state: &AppState, settings: AppSettings) -> AppResult<AppSettings> {
    let saved = state.settings.update(settings)?;
    tracing::info!("設定を更新しました");
    Ok(saved)
}

/// 既定値へ戻す。
pub fn reset_settings(state: &AppState) -> AppResult<AppSettings> {
    let defaults = AppSettings::with_defaults(state.paths.default_meetings_dir.clone());
    state.settings.update(defaults)
}
