//! 音声認識モデルの一覧とダウンロード。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::events::AppEvent;
use crate::state::AppState;
use crate::stt::models::{self, ModelStatus};

pub fn list_whisper_models(state: &AppState) -> Vec<ModelStatus> {
    models::list_models(&state.paths.models_dir)
}

/// モデルのダウンロードをバックグラウンドで開始する。
///
/// 数百MB〜数GBあるため UI をブロックしないよう別スレッドで実行し、
/// 進捗は `model:download-progress` として通知する。
pub fn start_model_download(state: Arc<AppState>, model_id: String) -> AppResult<()> {
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

    let spawned = std::thread::Builder::new()
        .name("b-listener-model-download".into())
        .spawn({
            let state = state.clone();
            move || {
                let events = state.events.clone();
                let on_progress = move |p: models::DownloadProgress| {
                    events.emit(AppEvent::ModelDownloadProgress(p));
                };
                let cancel = state.download_cancel.clone();
                let should_cancel = move || cancel.load(Ordering::SeqCst);

                let result = models::download(
                    &state.paths.models_dir,
                    &model_id,
                    &on_progress,
                    &should_cancel,
                );

                if let Ok(mut current) = state.downloading_model.lock() {
                    *current = None;
                }

                match result {
                    Ok(path) => {
                        tracing::info!(model_id, path = %path.display(), "モデルを配置しました");
                        state.events.emit(AppEvent::ModelDownloadDone(model_id));
                    }
                    Err(e) => {
                        tracing::error!(model_id, error = %e, "モデルのダウンロードに失敗しました");
                        state
                            .events
                            .emit(AppEvent::ModelDownloadFailed(e.to_string()));
                    }
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

pub fn cancel_model_download(state: &AppState) {
    state.download_cancel.store(true, Ordering::SeqCst);
}
