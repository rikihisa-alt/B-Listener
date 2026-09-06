//! B-Listener — AI議事録デスクトップアプリ
//!
//! 設計ドキュメントは `docs/` を参照。
//!
//! 設計上の最優先事項は「録音データを絶対に失わない」こと。
//! そのため音声処理 (`audio`) は文字起こし (`stt`) や AI 処理 (`llm`) に依存せず、
//! 上位の `pipeline` だけがそれらを結合する。

pub mod audio;
pub mod commands;
pub mod db;
pub mod error;
pub mod logging;
pub mod paths;
pub mod pipeline;
pub mod settings;
pub mod state;
pub mod storage;
pub mod stt;
pub mod sysutil;

use std::path::PathBuf;

use tauri::Manager;

use crate::audio::recorder::Recorder;
use crate::db::Database;
use crate::settings::SettingsStore;
use crate::state::{AppPaths, AppState};

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

            std::fs::create_dir_all(&paths.app_data_dir)?;
            std::fs::create_dir_all(&paths.models_dir)?;

            let settings =
                SettingsStore::load(&paths.app_data_dir, paths.default_meetings_dir.clone());
            if let Err(e) = settings.ensure_persisted() {
                // 保存先が作れなくてもアプリは起動させ、設定画面で直せるようにする。
                tracing::error!(error = %e, "保存先フォルダの準備に失敗しました");
            }

            let db = Database::open(&paths.app_data_dir.join("app.db"))?;

            // 前回が異常終了だった場合に備え、起動時点で件数だけ確認しておく。
            // 実際の復旧処理は Phase 2（録音）で実装する。
            match db.with_conn(db::repo::list_interrupted_meetings) {
                Ok(list) if !list.is_empty() => {
                    tracing::warn!(count = list.len(), "中断された会議があります");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "中断された会議の確認に失敗しました"),
            }

            app.manage(AppState {
                paths,
                db,
                settings,
                recorder: Recorder::new(),
                running_pipelines: std::sync::Mutex::new(std::collections::HashSet::new()),
                download_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                downloading_model: std::sync::Mutex::new(None),
                _log_guard: log_guard,
            });

            spawn_recording_ticker(app.handle().clone());

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
fn spawn_recording_ticker(app: tauri::AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use tauri::Emitter;

    /// 画面の経過時間表示に十分で、かつ負荷にならない間隔。
    const TICK_INTERVAL: Duration = Duration::from_millis(500);
    /// 空き容量を確認する間隔。
    const DISK_CHECK_INTERVAL: Duration = Duration::from_secs(60);

    std::thread::Builder::new()
        .name("b-listener-recording-ticker".into())
        .spawn(move || {
            let error_notified = AtomicBool::new(false);
            let mut last_disk_check = Instant::now();

            loop {
                std::thread::sleep(TICK_INTERVAL);

                let state = app.state::<AppState>();
                let Some(snapshot) = state.recorder.snapshot() else {
                    error_notified.store(false, Ordering::Relaxed);
                    continue;
                };

                // 録音が継続できないエラーは一度だけ通知する。
                if let Some(message) = snapshot.error.as_ref() {
                    if !error_notified.swap(true, Ordering::Relaxed) {
                        if let Err(e) = app.emit("recording:error", message.clone()) {
                            tracing::warn!(error = %e, "録音エラーの通知に失敗しました");
                        }
                    }
                }

                if let Err(e) = app.emit("recording:tick", &snapshot) {
                    tracing::warn!(error = %e, "録音状態の通知に失敗しました");
                }

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
                            tracing::warn!(available_mb = available / 1024 / 1024, "空き容量が不足しています");
                            if let Err(e) = app.emit("recording:disk-warning", message) {
                                tracing::warn!(error = %e, "空き容量警告の通知に失敗しました");
                            }
                        }
                    }
                }
            }
        })
        .expect("録音状態通知スレッドを起動できません");
}
