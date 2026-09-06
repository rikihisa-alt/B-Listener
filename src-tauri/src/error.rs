//! アプリ共通のエラー型。
//!
//! 方針: エラーを握り潰さない。すべてのエラーは `AppError` に変換し、
//! ログに記録したうえで UI へ「コード + 日本語メッセージ」として返す。
//! UI 側はコードで分岐し、メッセージをそのまま表示できる。

use serde::ser::{Serialize, SerializeStruct, Serializer};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("データベース処理に失敗しました: {0}")]
    Db(String),

    #[error("ファイルの読み書きに失敗しました: {0}")]
    Io(String),

    #[error("設定の読み書きに失敗しました: {0}")]
    Settings(String),

    #[error("対象が見つかりません: {0}")]
    NotFound(String),

    #[error("入力内容が正しくありません: {0}")]
    Invalid(String),

    #[error("録音処理でエラーが発生しました: {0}")]
    Audio(String),

    #[error("文字起こしでエラーが発生しました: {0}")]
    Stt(String),

    #[error("AI処理でエラーが発生しました: {0}")]
    Llm(String),

    #[error("ディスクの空き容量が不足しています: {0}")]
    DiskSpace(String),

    #[error("必要なコンポーネントがありません: {0}")]
    MissingComponent(String),

    #[error("処理中にエラーが発生しました: {0}")]
    Other(String),
}

impl AppError {
    /// UI 側で分岐するための安定したコード。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Db(_) => "DB",
            AppError::Io(_) => "IO",
            AppError::Settings(_) => "SETTINGS",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Invalid(_) => "INVALID",
            AppError::Audio(_) => "AUDIO",
            AppError::Stt(_) => "STT",
            AppError::Llm(_) => "LLM",
            AppError::DiskSpace(_) => "DISK_SPACE",
            AppError::MissingComponent(_) => "MISSING_COMPONENT",
            AppError::Other(_) => "OTHER",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(format!("JSONの処理に失敗しました: {e}"))
    }
}
