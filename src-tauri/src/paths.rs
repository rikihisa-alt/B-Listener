//! 保存先パスの解決とファイル名のサニタイズ。
//!
//! パスはすべて `PathBuf` で扱い、文字列連結は行わない。
//! Windows / macOS で使えないファイル名を作らないことを保証する。

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

/// Windows / macOS の両方で使えないファイル名文字。
const FORBIDDEN: &[char] = &[
    '\\', '/', ':', '*', '?', '"', '<', '>', '|', '\u{0}', '\n', '\r', '\t',
];

/// Windows の予約デバイス名。
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Windows の MAX_PATH(260) を考慮し、フォルダ名は控えめに切り詰める。
const MAX_NAME_CHARS: usize = 60;

/// 会議名などをフォルダ名として安全な文字列へ変換する。
pub fn sanitize_file_name(input: &str) -> String {
    let mut out: String = input
        .chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) || (c as u32) < 0x20 {
                '_'
            } else {
                c
            }
        })
        .collect();

    out = out.trim().trim_matches('.').trim().to_string();

    if out.chars().count() > MAX_NAME_CHARS {
        out = out.chars().take(MAX_NAME_CHARS).collect::<String>();
        out = out.trim().to_string();
    }

    if out.is_empty() {
        return "meeting".to_string();
    }

    let upper = out.to_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    if RESERVED.contains(&stem) {
        out.push('_');
    }

    out
}

/// `2026-09-05_運営会議` 形式のフォルダ名を作る。
pub fn meeting_folder_name(started_at: &DateTime<Local>, title: &str) -> String {
    format!(
        "{}_{}",
        started_at.format("%Y-%m-%d"),
        sanitize_file_name(title)
    )
}

/// 既存フォルダと衝突する場合に `-2`, `-3` … を付けて空きを探す。
pub fn unique_dir(parent: &Path, base_name: &str) -> PathBuf {
    let first = parent.join(base_name);
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = parent.join(format!("{base_name}-{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base_name}-{}", uuid::Uuid::new_v4()))
}

/// 会議フォルダ内の標準ファイル名。
pub struct MeetingFiles;

impl MeetingFiles {
    pub const AUDIO_WAV: &'static str = "audio.wav";
    pub const TRANSCRIPT: &'static str = "transcript.txt";
    pub const TRANSCRIPT_RAW: &'static str = "transcript.raw.txt";
    pub const MINUTES: &'static str = "minutes.md";
    pub const SUMMARY: &'static str = "summary.md";
    pub const METADATA: &'static str = "metadata.json";
}

/// Tauri を使わずにアプリのディレクトリを解決する（ブラウザ版のサーバ用）。
///
/// デスクトップ版と**同じ場所**を指すため、同じPCで両方を使っても
/// 会議データや設定が分かれてしまうことはない。
pub fn default_app_paths() -> Option<(PathBuf, PathBuf)> {
    /// Tauri の `identifier` と一致させること。ずれるとデータが分かれてしまう。
    const APP_ID: &str = "jp.blistener.app";

    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;

    #[cfg(target_os = "macos")]
    let app_data = home.join("Library/Application Support").join(APP_ID);

    #[cfg(target_os = "windows")]
    let app_data = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Roaming"))
        .join(APP_ID);

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let app_data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join(APP_ID);

    let documents = home.join("Documents");
    let meetings = documents.join("B-Listener").join("Meetings");

    Some((app_data, meetings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_forbidden_characters() {
        assert_eq!(sanitize_file_name("9月/運営:会議"), "9月_運営_会議");
    }

    #[test]
    fn handles_empty_and_dots() {
        assert_eq!(sanitize_file_name("   "), "meeting");
        assert_eq!(sanitize_file_name("..."), "meeting");
    }

    #[test]
    fn escapes_windows_reserved_names() {
        assert_eq!(sanitize_file_name("CON"), "CON_");
        assert_eq!(sanitize_file_name("con.txt"), "con.txt_");
    }

    #[test]
    fn truncates_long_names() {
        let long = "あ".repeat(200);
        assert_eq!(sanitize_file_name(&long).chars().count(), MAX_NAME_CHARS);
    }
}
