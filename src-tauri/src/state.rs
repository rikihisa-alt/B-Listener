//! アプリ全体で共有する状態。
//!
//! Tauri の `State<AppState>` として注入し、command 層からのみ触る。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::audio::recorder::Recorder;
use crate::audio::upload_recorder::UploadRecorder;
use crate::db::Database;
use crate::events::SharedEventSink;
use crate::settings::SettingsStore;

/// OS ごとに解決した各種ディレクトリ。
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// 設定・DB・ログを置く場所。
    pub app_data_dir: PathBuf,
    /// ログ出力先。
    pub log_dir: PathBuf,
    /// Whisper モデルの保存先。
    pub models_dir: PathBuf,
    /// 保存先フォルダの既定値（設定で変更可能）。
    pub default_meetings_dir: PathBuf,
}

/// アプリ全体で共有する状態。
///
/// Tauri からもブラウザ版の HTTP サーバからも、同じこの型を通してコア処理を呼ぶ。
/// UI の種類に依存する処理は `events`（通知の送り先）にだけ閉じ込めている。
pub struct AppState {
    pub paths: AppPaths,
    pub db: Database,
    pub settings: SettingsStore,
    /// UI への通知の送り先（Tauri イベント / SSE）。
    pub events: SharedEventSink,
    /// このPCのマイクを使う録音セッション（デスクトップ版）。
    pub recorder: Recorder,
    /// ブラウザから送られてくる音声を受ける録音セッション（ブラウザ版）。
    pub upload_recorder: UploadRecorder,
    /// 実行中の会議終了後パイプライン。同じ会議の二重実行を防ぐ。
    pub running_pipelines: Mutex<HashSet<String>>,
    /// モデルのダウンロードを中止するためのフラグ。
    pub download_cancel: Arc<AtomicBool>,
    /// ダウンロード中のモデルID。多重ダウンロードを防ぐ。
    pub downloading_model: Mutex<Option<String>>,
    /// ログ書き出しスレッドのガード。アプリ終了まで保持する必要がある。
    ///
    /// `bootstrap` では設定せず、起動処理側（`run` / サーバ）が保持する。
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl AppState {
    /// DB・設定・保存先を用意して共有状態を組み立てる。
    ///
    /// デスクトップ版（Tauri）とブラウザ版（HTTPサーバ）の両方がこれを使うことで、
    /// 起動処理の差異が生まれないようにする。
    pub fn bootstrap(
        paths: AppPaths,
        events: SharedEventSink,
    ) -> crate::error::AppResult<Arc<Self>> {
        use crate::error::AppError;

        std::fs::create_dir_all(&paths.app_data_dir)
            .map_err(|e| AppError::Io(format!("データフォルダを作成できません: {e}")))?;
        std::fs::create_dir_all(&paths.models_dir)
            .map_err(|e| AppError::Io(format!("モデルフォルダを作成できません: {e}")))?;

        let settings = SettingsStore::load(&paths.app_data_dir, paths.default_meetings_dir.clone());
        if let Err(e) = settings.ensure_persisted() {
            // 保存先が作れなくてもアプリは起動させ、設定画面で直せるようにする。
            tracing::error!(error = %e, "保存先フォルダの準備に失敗しました");
        }

        let db = Database::open(&paths.app_data_dir.join("app.db"))?;

        // 前回が異常終了だった場合に備え、起動時点で件数だけ確認しておく。
        match db.with_conn(crate::db::repo::list_interrupted_meetings) {
            Ok(list) if !list.is_empty() => {
                tracing::warn!(count = list.len(), "中断された会議があります");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "中断された会議の確認に失敗しました"),
        }

        Ok(Arc::new(Self {
            paths,
            db,
            settings,
            events,
            recorder: Recorder::new(),
            upload_recorder: UploadRecorder::new(),
            running_pipelines: Mutex::new(HashSet::new()),
            download_cancel: Arc::new(AtomicBool::new(false)),
            downloading_model: Mutex::new(None),
            _log_guard: None,
        }))
    }
}
