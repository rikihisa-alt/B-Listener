//! B-Listener — AI議事録デスクトップアプリ / ブラウザ版
//!
//! 設計ドキュメントは `docs/` を参照。
//!
//! 設計上の最優先事項は「録音データを絶対に失わない」こと。
//! そのため音声処理 (`audio`) は文字起こし (`stt`) や AI 処理 (`llm`) に依存せず、
//! 上位の `pipeline` だけがそれらを結合する。
//!
//! UI は 2 種類ある。
//! - デスクトップ版: Tauri（このファイルの `run`）
//! - ブラウザ版: HTTP サーバ（`server` モジュール / `b-listener-server` バイナリ）
//!
//! どちらも `state::AppState` を通して同じコア処理を呼び、
//! 差異は「UI への通知の送り先」(`events::EventSink`) にだけ閉じ込めている。

pub mod audio;
pub mod commands;
pub mod db;
pub mod error;
pub mod events;
pub mod logging;
pub mod paths;
pub mod pipeline;
pub mod server;
pub mod settings;
pub mod state;
pub mod storage;
pub mod stt;
pub mod sysutil;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Emitter, Manager, Runtime};

use crate::events::{AppEvent, EventSink};
use crate::state::{AppPaths, AppState};

/// Tauri のイベントとして UI へ通知する送り先。
struct TauriSink<R: Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: Runtime> EventSink for TauriSink<R> {
    fn emit(&self, event: AppEvent) {
        if let Err(e) = self.app.emit(event.name(), event.payload()) {
            // 通知に失敗しても処理は続ける（UI の表示が遅れるだけ）。
            tracing::warn!(event = event.name(), error = %e, "通知の送信に失敗しました");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = resolve_paths(app.handle())?;

            // ログは最初に立ち上げる。以降の失敗を必ず記録するため。
            let log_guard = logging::init(&paths.log_dir);
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                os = std::env::consts::OS,
                "B-Listener を起動します"
            );

            let events: events::SharedEventSink = Arc::new(TauriSink {
                app: app.handle().clone(),
            });
            let state = AppState::bootstrap(paths, events)?;

            app.manage(state.clone());
            spawn_recording_ticker(state);

            // ログのガードは Tauri 側で保持する（AppState は共有されるため入れない）。
            app.manage(LogGuard(log_guard));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::meeting::create_meeting,
            commands::meeting::list_meetings,
            commands::meeting::get_home_stats,
            commands::meeting::get_meeting_detail,
            commands::meeting::save_meeting_context,
            commands::meeting::delete_meeting,
            commands::recording::list_input_devices,
            commands::recording::start_recording,
            commands::recording::pause_recording,
            commands::recording::resume_recording,
            commands::recording::stop_recording,
            commands::recording::get_recording_state,
            commands::recording::get_recoverable_meetings,
            commands::recording::recover_meeting,
            commands::recording::check_disk_status,
            commands::pipeline::run_pipeline,
            commands::pipeline::retry_pipeline,
            commands::pipeline::get_pipeline_status,
            commands::pipeline::get_transcript,
            commands::files::list_meeting_files,
            commands::files::read_meeting_document,
            commands::files::export_meeting_document,
            commands::files::export_meeting_audio,
            commands::stt::list_whisper_models,
            commands::stt::download_whisper_model,
            commands::stt::cancel_model_download,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::reset_settings,
            commands::system::get_system_info,
            commands::system::check_components,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri アプリケーションの起動に失敗しました");
}

/// ログ書き出しスレッドのガードを、アプリの寿命と同じだけ保持するための入れ物。
struct LogGuard(#[allow(dead_code)] Option<tracing_appender::non_blocking::WorkerGuard>);

/// OS ごとの標準的な場所からアプリのディレクトリを解決する。
fn resolve_paths(app: &tauri::AppHandle) -> Result<AppPaths, Box<dyn std::error::Error>> {
    let resolver = app.path();
    let app_data_dir = resolver.app_data_dir()?;

    // 保存先の既定は「書類 / B-Listener / Meetings」。
    // 利用者が Finder / エクスプローラから辿れる場所に置く。
    let default_meetings_dir: PathBuf = resolver
        .document_dir()
        .unwrap_or_else(|_| app_data_dir.clone())
        .join("B-Listener")
        .join("Meetings");

    Ok(AppPaths {
        log_dir: app_data_dir.join("logs"),
        models_dir: app_data_dir.join("models"),
        app_data_dir,
        default_meetings_dir,
    })
}

/// 録音中の経過時間・入力レベルを UI へ通知する常駐スレッド。
///
/// UI からのポーリングではなくイベントで通知することで、
/// 会議中の画面更新が IPC の往復に依存しないようにする。
/// デスクトップ版・ブラウザ版のどちらでも同じ処理を使う。
pub fn spawn_recording_ticker(state: Arc<AppState>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// 画面の経過時間表示に十分で、かつ負荷にならない間隔。
    const TICK_INTERVAL: Duration = Duration::from_millis(500);
    /// 空き容量を確認する間隔。
    const DISK_CHECK_INTERVAL: Duration = Duration::from_secs(60);

    let spawned = std::thread::Builder::new()
        .name("b-listener-recording-ticker".into())
        .spawn(move || {
            let error_notified = AtomicBool::new(false);
            let mut last_disk_check = Instant::now();

            loop {
                std::thread::sleep(TICK_INTERVAL);

                let Some(snapshot) = server::service::get_recording_state(&state) else {
                    error_notified.store(false, Ordering::Relaxed);
                    continue;
                };

                // 録音が継続できないエラーは一度だけ通知する。
                if let Some(message) = snapshot.error.as_ref() {
                    if !error_notified.swap(true, Ordering::Relaxed) {
                        state.events.emit(AppEvent::RecordingError(message.clone()));
                    }
                }

                state.events.emit(AppEvent::RecordingTick(snapshot));

                if last_disk_check.elapsed() >= DISK_CHECK_INTERVAL {
                    last_disk_check = Instant::now();
                    let settings = state.settings.get();
                    if let Some(available) = sysutil::available_space(&settings.meetings_dir) {
                        let required = settings.min_free_disk_mb * 1024 * 1024;
                        if available < required {
                            let message = format!(
                                "保存先の空き容量が残り {} MB です。録音を続けるには空き容量を確保してください。",
                                available / 1024 / 1024
                            );
                            tracing::warn!(
                                available_mb = available / 1024 / 1024,
                                "空き容量が不足しています"
                            );
                            state.events.emit(AppEvent::RecordingDiskWarning(message));
                        }
                    }
                }
            }
        });

    if let Err(e) = spawned {
        tracing::error!(error = %e, "録音状態通知スレッドを起動できませんでした");
    }
}
