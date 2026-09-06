//! `metadata.json` の入出力。
//!
//! 会議フォルダを単体で完結させ、DB が失われても内容を復元できるようにする。
//! 将来の外部連携（Phase 9 以降）でも、再解析せずにこのファイルを読めば済む。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::models::{Agenda, ContextTerm, Meeting, Participant};
use crate::error::AppResult;
use crate::paths::MeetingFiles;

/// `metadata.json` の形式バージョン。読み込み側の互換判定に使う。
const METADATA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingMetadata {
    pub version: u32,
    pub app_version: String,
    pub meeting: Meeting,
    pub participants: Vec<Participant>,
    pub agendas: Vec<Agenda>,
    pub terms: Vec<ContextTerm>,
    /// 使用した音声認識モデル。
    pub whisper_model: Option<String>,
    /// 使用したローカル LLM モデル。
    pub llm_model: Option<String>,
    pub written_at: String,
}

pub fn write(folder: &Path, metadata: &MeetingMetadata) -> AppResult<PathBuf> {
    let path = folder.join(MeetingFiles::METADATA);
    let json = serde_json::to_string_pretty(metadata)?;
    super::write_text_atomic(&path, &json)?;
    Ok(path)
}

pub fn build(
    meeting: Meeting,
    participants: Vec<Participant>,
    agendas: Vec<Agenda>,
    terms: Vec<ContextTerm>,
    whisper_model: Option<String>,
    llm_model: Option<String>,
) -> MeetingMetadata {
    MeetingMetadata {
        version: METADATA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        meeting,
        participants,
        agendas,
        terms,
        whisper_model,
        llm_model,
        written_at: crate::db::now_iso8601(),
    }
}
