//! アプリ設定。
//!
//! DB ではなく `<app_data>/settings.json` に保存する。
//! 理由: DB が破損した場合でも「保存先フォルダ」を読めなければ復旧できないため。
//!
//! ハードコードを避けるため、モデル名・間隔・閾値などの可変値はすべてここに集約する。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Whisper モデルの既定値（精度優先・最終文字起こし用）。
pub const DEFAULT_WHISPER_MODEL: &str = "large-v3-turbo";
/// Whisper モデルの既定値（速度優先・リアルタイム用）。
pub const DEFAULT_WHISPER_REALTIME_MODEL: &str = "small";
/// Ollama の既定エンドポイント（ローカルのみ）。
pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
/// 既定の LLM モデル。日本語性能とローカル実行性能のバランスで選定。
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5:7b-instruct";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// 会議フォルダを作成する親ディレクトリ。
    pub meetings_dir: PathBuf,

    /// 最終文字起こしに使う Whisper モデル。
    pub whisper_model: String,
    /// リアルタイム文字起こしに使う Whisper モデル。
    pub whisper_realtime_model: String,

    /// ローカル LLM のエンドポイント（localhost のみを想定）。
    pub llm_endpoint: String,
    /// ローカル LLM のモデル名。
    pub llm_model: String,
    /// LLM プロバイダ種別。将来の差し替えのために保持する。
    pub llm_provider: LlmProviderKind,

    /// リアルタイム文字起こしを行うか。
    pub realtime_transcription_enabled: bool,
    /// リアルタイム AI 分析を行うか。
    pub realtime_analysis_enabled: bool,
    /// リアルタイム AI 分析の実行間隔（秒）。
    pub realtime_analysis_interval_secs: u32,
    /// 会議終了後に AI まとめを生成するか。
    pub ai_summary_enabled: bool,

    /// 使用する入力デバイス名。None なら OS の既定デバイス。
    pub input_device: Option<String>,

    /// 録音開始前に確保を要求する空き容量（MB）。
    pub min_free_disk_mb: u64,

    /// m4a へ圧縮したあとも WAV を残すか（Phase 9）。
    pub keep_wav_after_compress: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProviderKind {
    Ollama,
    /// LM Studio / llama.cpp server などの OpenAI 互換ローカルサーバ。
    OpenAiCompatible,
}

impl AppSettings {
    /// 既定値。`default_meetings_dir` は OS ごとに解決した値を渡す。
    pub fn with_defaults(default_meetings_dir: PathBuf) -> Self {
        Self {
            meetings_dir: default_meetings_dir,
            whisper_model: DEFAULT_WHISPER_MODEL.to_string(),
            whisper_realtime_model: DEFAULT_WHISPER_REALTIME_MODEL.to_string(),
            llm_endpoint: DEFAULT_OLLAMA_ENDPOINT.to_string(),
            llm_model: DEFAULT_OLLAMA_MODEL.to_string(),
            llm_provider: LlmProviderKind::Ollama,
            realtime_transcription_enabled: true,
            realtime_analysis_enabled: true,
            realtime_analysis_interval_secs: 45,
            ai_summary_enabled: true,
            input_device: None,
            min_free_disk_mb: 2048,
            keep_wav_after_compress: true,
        }
    }

    fn validate(&mut self) {
        // 極端な値を弾く。UI の入力ミスでリアルタイム分析が暴走しないようにする。
        self.realtime_analysis_interval_secs = self.realtime_analysis_interval_secs.clamp(20, 600);
        self.min_free_disk_mb = self.min_free_disk_mb.clamp(256, 1_000_000);
        if self.whisper_model.trim().is_empty() {
            self.whisper_model = DEFAULT_WHISPER_MODEL.to_string();
        }
        if self.whisper_realtime_model.trim().is_empty() {
            self.whisper_realtime_model = DEFAULT_WHISPER_REALTIME_MODEL.to_string();
        }
        if self.llm_endpoint.trim().is_empty() {
            self.llm_endpoint = DEFAULT_OLLAMA_ENDPOINT.to_string();
        }
        if self.llm_model.trim().is_empty() {
            self.llm_model = DEFAULT_OLLAMA_MODEL.to_string();
        }
    }
}

/// 設定の読み書きを担当する。アプリ全体で 1 インスタンスを共有する。
pub struct SettingsStore {
    file: PathBuf,
    default_meetings_dir: PathBuf,
    current: RwLock<AppSettings>,
}

impl SettingsStore {
    /// 設定ファイルを読み込む。存在しない・壊れている場合は既定値で作り直す。
    pub fn load(app_data_dir: &Path, default_meetings_dir: PathBuf) -> Self {
        let file = app_data_dir.join("settings.json");
        let defaults = AppSettings::with_defaults(default_meetings_dir.clone());

        let loaded = match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<AppSettings>(&text) {
                Ok(mut s) => {
                    s.validate();
                    s
                }
                Err(e) => {
                    // 壊れた設定で起動不能にならないよう、既定値へフォールバックする。
                    tracing::warn!(error = %e, "settings.json を解釈できないため既定値を使用します");
                    defaults.clone()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => defaults.clone(),
            Err(e) => {
                tracing::warn!(error = %e, "settings.json を読み込めないため既定値を使用します");
                defaults.clone()
            }
        };

        Self {
            file,
            default_meetings_dir,
            current: RwLock::new(loaded),
        }
    }

    pub fn get(&self) -> AppSettings {
        match self.current.read() {
            Ok(g) => g.clone(),
            // ロックが毒された場合でも既定値を返して動作を継続する。
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn update(&self, mut next: AppSettings) -> AppResult<AppSettings> {
        next.validate();

        if next.meetings_dir.as_os_str().is_empty() {
            next.meetings_dir = self.default_meetings_dir.clone();
        }
        std::fs::create_dir_all(&next.meetings_dir).map_err(|e| {
            AppError::Settings(format!(
                "保存先フォルダを作成できません ({}): {e}",
                next.meetings_dir.display()
            ))
        })?;

        self.persist(&next)?;

        match self.current.write() {
            Ok(mut g) => *g = next.clone(),
            Err(poisoned) => *poisoned.into_inner() = next.clone(),
        }
        Ok(next)
    }

    /// 起動時に一度だけ呼び、保存先フォルダと設定ファイルの実体を確定させる。
    pub fn ensure_persisted(&self) -> AppResult<()> {
        let current = self.get();
        std::fs::create_dir_all(&current.meetings_dir).map_err(|e| {
            AppError::Settings(format!(
                "保存先フォルダを作成できません ({}): {e}",
                current.meetings_dir.display()
            ))
        })?;
        if !self.file.exists() {
            self.persist(&current)?;
        }
        Ok(())
    }

    fn persist(&self, settings: &AppSettings) -> AppResult<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Settings(format!("設定フォルダを作成できません: {e}")))?;
        }
        let json = serde_json::to_string_pretty(settings)?;

        // 途中でクラッシュしても settings.json が壊れないよう、一時ファイル経由で置き換える。
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|e| AppError::Settings(format!("設定を書き込めません: {e}")))?;
        std::fs::rename(&tmp, &self.file)
            .map_err(|e| AppError::Settings(format!("設定を保存できません: {e}")))?;
        Ok(())
    }
}
