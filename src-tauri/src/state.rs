//! アプリ全体で共有する状態。
//!
//! Tauri の `State<AppState>` として注入し、command 層からのみ触る。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::audio::recorder::Recorder;
use crate::db::Database;
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

pub struct AppState {
    pub paths: AppPaths,
    pub db: Database,
    pub settings: SettingsStore,
    /// 同時に 1 件だけ動作する録音セッション。
    pub recorder: Recorder,
    /// 実行中の会議終了後パイプライン。同じ会議の二重実行を防ぐ。
    pub running_pipelines: Mutex<HashSet<String>>,
    /// モデルのダウンロードを中止するためのフラグ。
    pub download_cancel: Arc<AtomicBool>,
    /// ダウンロード中のモデルID。多重ダウンロードを防ぐ。
    pub downloading_model: Mutex<Option<String>>,
    /// ログ書き出しスレッドのガード。アプリ終了まで保持する必要がある。
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}
