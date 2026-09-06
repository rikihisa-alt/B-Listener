//! （`commands/system.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 動作環境の確認とパス情報の提供。
//!
//! 「必要なコンポーネントがありません」を利用者に明確に伝えるための情報源。

use serde::Serialize;

use crate::error::AppResult;
use crate::state::AppState;

/// コンポーネントの導入状態。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentState {
    /// 使用できる
    Ready,
    /// 導入されていない
    Missing,
    /// このフェーズではまだ確認処理を実装していない
    NotChecked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    /// 画面に出す名称
    pub name: String,
    pub state: ComponentState,
    /// 利用者向けの説明（そのまま表示できる日本語）
    pub message: String,
    /// 導入方法の案内
    pub setup_hint: String,
    /// 期待している配置先（あれば）
    pub expected_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub app_data_dir: String,
    pub log_dir: String,
    pub models_dir: String,
    pub meetings_dir: String,
}

pub fn get_system_info(state: &AppState) -> SystemInfo {
    SystemInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        app_data_dir: state.paths.app_data_dir.display().to_string(),
        log_dir: state.paths.log_dir.display().to_string(),
        models_dir: state.paths.models_dir.display().to_string(),
        meetings_dir: state.settings.get().meetings_dir.display().to_string(),
    }
}

/// Whisper モデルと Ollama の導入状況を返す。
///
/// Whisper モデルの有無はファイルの実在で判定できるためこの段階で実装する。
/// Ollama の疎通確認は HTTP クライアントを導入する Phase 4 で実装する。
pub fn check_components(state: &AppState) -> AppResult<Vec<ComponentStatus>> {
    let settings = state.settings.get();
    let models_dir = &state.paths.models_dir;

    let mut out = Vec::new();

    for (label, model) in [
        (
            "音声認識モデル（最終文字起こし）",
            settings.whisper_model.clone(),
        ),
        (
            "音声認識モデル（リアルタイム）",
            settings.whisper_realtime_model.clone(),
        ),
    ] {
        // 判定は stt::models に一本化する（ここで独自判定すると齟齬が出るため）。
        let available = crate::stt::models::ensure_available(models_dir, &model);
        let path = crate::stt::models::model_path(models_dir, &model);
        let installed = available.is_ok();
        out.push(ComponentStatus {
            name: format!("{label}: {model}"),
            state: if installed {
                ComponentState::Ready
            } else {
                ComponentState::Missing
            },
            message: match &available {
                Ok(_) => "使用できます。".to_string(),
                Err(e) => e.to_string(),
            },
            setup_hint: if installed {
                String::new()
            } else {
                "設定画面の「モデルをダウンロード」から取得できます（初回のみ・数分かかります）。"
                    .to_string()
            },
            expected_path: Some(path.display().to_string()),
        });
    }

    out.push(ComponentStatus {
        name: format!("ローカルLLM: {}", settings.llm_model),
        state: ComponentState::NotChecked,
        message: format!(
            "接続先 {} の確認は未実装です（Phase 4 で対応）。",
            settings.llm_endpoint
        ),
        setup_hint: "Ollama を https://ollama.com からインストールし、`ollama pull qwen2.5:7b-instruct` を実行してください。".to_string(),
        expected_path: None,
    });

    Ok(out)
}
