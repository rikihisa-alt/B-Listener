//! （`commands/files.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 会議フォルダ内の成果物（音声・文字起こし・議事録・まとめ）の参照。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::paths::MeetingFiles;
use crate::state::AppState;

/// テキスト成果物の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingDocument {
    Transcript,
    Minutes,
    Summary,
}

/// 1 ファイルあたりの読み込み上限。
/// 3 時間の文字起こしでも 1MB 程度なので、これを超えるのは異常とみなす。
const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingFile {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    /// 音声ファイルかどうか（UI のアイコン切り替え用）。
    pub is_audio: bool,
}

/// 会議フォルダに実際に存在するファイルを列挙する。
pub fn list_meeting_files(state: &AppState, meeting_id: String) -> AppResult<Vec<MeetingFile>> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;

    let Some(folder) = meeting.folder_path.as_ref().map(PathBuf::from) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    let candidates = [
        (MeetingFiles::AUDIO_WAV, true),
        ("audio.m4a", true),
        (MeetingFiles::TRANSCRIPT, false),
        (MeetingFiles::MINUTES, false),
        (MeetingFiles::SUMMARY, false),
        (MeetingFiles::METADATA, false),
    ];

    for (name, is_audio) in candidates {
        let path = folder.join(name);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        out.push(MeetingFile {
            name: name.to_string(),
            path: path.display().to_string(),
            size_bytes: meta.len(),
            is_audio,
        });
    }
    Ok(out)
}

/// 議事録・まとめ・文字起こしの本文を読む。まだ存在しない場合は `None`。
pub fn read_meeting_document(
    state: &AppState,
    meeting_id: String,
    document: MeetingDocument,
) -> AppResult<Option<String>> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;

    // DB に記録されたパスを優先し、無ければ会議フォルダの既定のファイル名を見る。
    let recorded = match document {
        MeetingDocument::Transcript => meeting.transcript_path.as_ref(),
        MeetingDocument::Minutes => meeting.minutes_path.as_ref(),
        MeetingDocument::Summary => meeting.summary_path.as_ref(),
    };

    let path = match recorded {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(folder) = meeting.folder_path.as_ref().map(PathBuf::from) else {
                return Ok(None);
            };
            folder.join(match document {
                MeetingDocument::Transcript => MeetingFiles::TRANSCRIPT,
                MeetingDocument::Minutes => MeetingFiles::MINUTES,
                MeetingDocument::Summary => MeetingFiles::SUMMARY,
            })
        }
    };

    read_text_if_exists(&path)
}

fn read_text_if_exists(path: &Path) -> AppResult<Option<String>> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Ok(None);
    }
    if meta.len() > MAX_DOCUMENT_BYTES {
        return Err(AppError::Io(format!(
            "ファイルが大きすぎて表示できません（{} MB）。保存フォルダから直接開いてください。",
            meta.len() / 1024 / 1024
        )));
    }

    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) => Err(AppError::Io(format!(
            "ファイルを読み込めません ({}): {e}",
            path.display()
        ))),
    }
}

/// 成果物を任意の場所へ書き出す。
///
/// 保存先の選択は UI 側のダイアログで行い、実際の書き込みはここで行う。
/// こうすることで WebView にファイルシステムの権限を渡さずに済む。
pub fn export_meeting_document(
    state: &AppState,
    meeting_id: String,
    document: MeetingDocument,
    target_path: String,
) -> AppResult<String> {
    let content = read_meeting_document(state, meeting_id, document)?
        .ok_or_else(|| AppError::NotFound("この成果物はまだ作成されていません".to_string()))?;

    let target = PathBuf::from(&target_path);
    if target.as_os_str().is_empty() {
        return Err(AppError::Invalid("保存先が指定されていません".to_string()));
    }

    crate::storage::write_text_atomic(&target, &content)?;
    tracing::info!(path = %target.display(), "成果物を書き出しました");
    Ok(target.display().to_string())
}

/// 音声ファイルを任意の場所へコピーする。
pub fn export_meeting_audio(
    state: &AppState,
    meeting_id: String,
    target_path: String,
) -> AppResult<String> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;
    let source = meeting
        .audio_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("録音ファイルがありません".to_string()))?;

    let target = PathBuf::from(&target_path);
    std::fs::copy(&source, &target).map_err(|e| {
        AppError::Io(format!(
            "音声ファイルを書き出せません ({}): {e}",
            target.display()
        ))
    })?;
    tracing::info!(path = %target.display(), "音声ファイルを書き出しました");
    Ok(target.display().to_string())
}

/// 会議の録音ファイルの実体パス。ブラウザ版の再生・ダウンロードで使う。
pub fn audio_path_of(state: &AppState, meeting_id: &str) -> AppResult<PathBuf> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, meeting_id))?;

    let path = meeting
        .audio_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("録音ファイルがありません".to_string()))?;

    if !path.is_file() {
        return Err(AppError::NotFound(format!(
            "録音ファイルが見つかりません: {}",
            path.display()
        )));
    }
    Ok(path)
}
