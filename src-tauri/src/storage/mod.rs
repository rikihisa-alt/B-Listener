//! 会議フォルダへのファイル出力。
//!
//! DB ではなくここに出力されたファイル群がデータの唯一の正である。
//! DB が壊れてもフォルダから会議を再構築できるようにする。

pub mod metadata;

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::paths::MeetingFiles;
use crate::stt::Segment;

/// テキストファイルを安全に書き出す。
///
/// 書き込み途中でクラッシュしても、既存の内容を壊さないよう
/// 一時ファイルへ書いてから置き換える。文字コードは UTF-8、改行は LF。
pub fn write_text_atomic(path: &Path, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Io(format!(
                "フォルダを作成できません ({}): {e}",
                parent.display()
            ))
        })?;
    }

    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("txt")
    ));

    std::fs::write(&tmp, content.as_bytes())
        .map_err(|e| AppError::Io(format!("ファイルを書き込めません ({}): {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        AppError::Io(format!(
            "ファイルを保存できません ({}): {e}",
            path.display()
        ))
    })?;
    Ok(())
}

/// 最終文字起こしを `transcript.txt` として保存する。
pub fn write_transcript(folder: &Path, segments: &[Segment]) -> AppResult<PathBuf> {
    let path = folder.join(MeetingFiles::TRANSCRIPT);
    write_text_atomic(&path, &crate::stt::segments_to_text(segments))?;
    Ok(path)
}

/// 補正前の文字起こしを `transcript.raw.txt` として保存する。
///
/// 補正が意図しない置き換えをしていないか、後から検証できるようにするため。
pub fn write_raw_transcript(folder: &Path, segments: &[Segment]) -> AppResult<PathBuf> {
    let path = folder.join(MeetingFiles::TRANSCRIPT_RAW);
    write_text_atomic(&path, &crate::stt::segments_to_text(segments))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_text_and_replaces_existing() {
        let dir = std::env::temp_dir().join(format!("blistener-store-{}", uuid::Uuid::new_v4()));
        let path = dir.join("out.txt");

        write_text_atomic(&path, "1回目").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1回目");

        write_text_atomic(&path, "2回目").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2回目");

        // 一時ファイルが残っていないこと
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "一時ファイルが残っています: {leftovers:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writes_transcript_with_timestamps() {
        let dir = std::env::temp_dir().join(format!("blistener-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let segments = vec![
            Segment {
                start_ms: 0,
                end_ms: 2000,
                text: "おはようございます".into(),
            },
            Segment {
                start_ms: 65_000,
                end_ms: 70_000,
                text: "シフトの件です".into(),
            },
        ];
        let path = write_transcript(&dir, &segments).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[00:00:00] おはようございます"));
        assert!(text.contains("[00:01:05] シフトの件です"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
