//! 音声認識モデルの管理。

use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;
use crate::stt::models::ModelStatus;

#[tauri::command]
pub fn list_whisper_models(state: State<'_, Arc<AppState>>) -> Vec<ModelStatus> {
    crate::server::service::list_whisper_models(&state)
}

/// モデルのダウンロードをバックグラウンドで開始する。
///
/// 数百MB〜数GBあるため、UI をブロックしないよう別スレッドで実行し、
/// 進捗は `model:download-progress` イベントで通知する。
#[tauri::command]
pub fn download_whisper_model(state: State<'_, Arc<AppState>>, model_id: String) -> AppResult<()> {
    crate::server::service::start_model_download((*state).clone(), model_id)
}

#[tauri::command]
pub fn cancel_model_download(state: State<'_, Arc<AppState>>) {
    crate::server::service::cancel_model_download(&state);
}
