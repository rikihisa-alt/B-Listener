//! 音声認識モデルの管理。

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::stt::models::{self, ModelStatus};

#[tauri::command]
pub fn list_whisper_models(state: State<'_, AppState>) -> Vec<ModelStatus> {
    models::list_models(&state.paths.models_dir)
}

/// モデルのダウンロードをバックグラウンドで開始する。
///
/// 数百MB〜数GBあるため、UI をブロックしないよう別スレッドで実行し、
/// 進捗は `model:download-progress` イベントで通知する。
#[tauri::command]
pub fn download_whisper_model(app: AppHandle, model_id: String) -> AppResult<()> {
    let state = app.state::<AppState>();

    if models::find_spec(&model_id).is_none() {
        return Err(AppError::Invalid(format!(
            "未知の音声認識モデルです: {model_id}"
        )));
    }

    {
        let mut current = match state.downloading_model.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(running) = current.as_ref() {
            return Err(AppError::Invalid(format!(
                "モデル「{running}」をダウンロード中です。完了するまでお待ちください。"
            )));
        }
        *current = Some(model_id.clone());
    }
    state.download_cancel.store(false, Ordering::SeqCst);

    let models_dir = state.paths.models_dir.clone();
    let cancel = state.download_cancel.clone();
    let app_for_thread = app.clone();

    let spawned = std::thread::Builder::new()
        .name("b-listener-model-download".into())
        .spawn(move || {
            let progress_app = app_for_thread.clone();
            let on_progress = move |p: models::DownloadProgress| {
                if let Err(e) = progress_app.emit("model:download-progress", p) {
                    tracing::warn!(error = %e, "ダウンロード進捗の通知に失敗しました");
                }
            };
            let should_cancel = || cancel.load(Ordering::SeqCst);

            let result = models::download(&models_dir, &model_id, &on_progress, &should_cancel);

            let state = app_for_thread.state::<AppState>();
            if let Ok(mut current) = state.downloading_model.lock() {
                *current = None;
            }

            match result {
                Ok(path) => {
                    tracing::info!(model_id, path = %path.display(), "モデルを配置しました");
                    let _ = app_for_thread.emit("model:download-done", model_id);
                }
                Err(e) => {
                    tracing::error!(model_id, error = %e, "モデルのダウンロードに失敗しました");
                    let _ = app_for_thread.emit("model:download-failed", e.to_string());
                }
            }
        });

    if let Err(e) = spawned {
        if let Ok(mut current) = state.downloading_model.lock() {
            *current = None;
        }
        return Err(AppError::Other(format!(
            "ダウンロードを開始できませんでした: {e}"
        )));
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_model_download(state: State<'_, AppState>) {
    state.download_cancel.store(true, Ordering::SeqCst);
}
