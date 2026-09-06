//! クラッシュした録音ファイルの復旧。
//!
//! [`super::wav_sink::WavSink`] は 5 秒ごとにヘッダを更新するため、強制終了時は
//! 「ヘッダのサイズ欄 < 実ファイルサイズ」という状態で残る。
//! 余剰バイトはすべて有効な PCM なので、ヘッダを実サイズへ書き直せば全て復元できる。
//!
//! ヘッダ自体が壊れている場合も、内容を捨てずに別ファイルへ救出する。

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

use super::wav_sink::{build_header, WAV_HEADER_LEN};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WavCondition {
    /// ヘッダと実サイズが一致している。修復不要。
    Intact,
    /// ヘッダが実サイズより小さい。ヘッダを書き直せば全て復元できる。
    HeaderOutdated,
    /// RIFF ヘッダが壊れている。生 PCM として救出する。
    HeaderBroken,
    /// ヘッダ分のバイトすら無い。音声データは存在しない。
    Empty,
    /// ファイルが存在しない。
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WavInspection {
    pub path: String,
    pub condition: WavCondition,
    /// ヘッダに記録されている data チャンク長。
    pub header_data_bytes: u64,
    /// 実際に存在する PCM のバイト数。
    pub actual_data_bytes: u64,
    pub sample_rate: u32,
    pub channels: u16,
    /// 実データから算出した音声長。
    pub duration_ms: i64,
}

/// WAV を読み取り専用で検査する。ファイルは変更しない。
pub fn inspect(path: &Path, fallback_sample_rate: u32, fallback_channels: u16) -> WavInspection {
    let mut result = WavInspection {
        path: path.display().to_string(),
        condition: WavCondition::Missing,
        header_data_bytes: 0,
        actual_data_bytes: 0,
        sample_rate: fallback_sample_rate,
        channels: fallback_channels,
        duration_ms: 0,
    };

    let Ok(meta) = std::fs::metadata(path) else {
        return result;
    };
    let file_len = meta.len();

    if file_len < WAV_HEADER_LEN {
        result.condition = WavCondition::Empty;
        return result;
    }

    let mut header = [0u8; WAV_HEADER_LEN as usize];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut header)) {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "WAVヘッダを読めません");
            result.condition = WavCondition::HeaderBroken;
            result.actual_data_bytes = file_len;
            return result;
        }
    }

    let riff_ok = &header[0..4] == b"RIFF" && &header[8..12] == b"WAVE";
    if !riff_ok {
        result.condition = WavCondition::HeaderBroken;
        result.actual_data_bytes = file_len;
        result.duration_ms = pcm_duration_ms(file_len, fallback_sample_rate, fallback_channels);
        return result;
    }

    let channels = u16::from_le_bytes([header[22], header[23]]).max(1);
    let sample_rate = u32::from_le_bytes(header[24..28].try_into().unwrap_or([0; 4]));
    let header_data_bytes = u32::from_le_bytes(header[40..44].try_into().unwrap_or([0; 4])) as u64;
    let actual = file_len - WAV_HEADER_LEN;

    result.channels = channels;
    result.sample_rate = if sample_rate == 0 {
        fallback_sample_rate
    } else {
        sample_rate
    };
    result.header_data_bytes = header_data_bytes;
    result.actual_data_bytes = actual;
    result.duration_ms = pcm_duration_ms(actual, result.sample_rate, channels);
    result.condition = if actual == 0 {
        WavCondition::Empty
    } else if header_data_bytes == actual {
        WavCondition::Intact
    } else {
        // ヘッダが実サイズと違う場合は、実データを正とする。
        WavCondition::HeaderOutdated
    };

    result
}

fn pcm_duration_ms(data_bytes: u64, sample_rate: u32, channels: u16) -> i64 {
    let block = 2 * channels.max(1) as u64;
    let frames = data_bytes / block;
    (frames as i64 * 1000) / sample_rate.max(1) as i64
}

/// 検査結果に応じて復旧する。復旧後の音声ファイルのパスを返す。
///
/// 元のデータは決して削除しない。破損ヘッダの場合は別名で救出ファイルを作る。
pub fn repair(path: &Path, inspection: &WavInspection) -> AppResult<PathBuf> {
    match inspection.condition {
        WavCondition::Intact => Ok(path.to_path_buf()),

        WavCondition::HeaderOutdated => {
            let header = build_header(
                inspection.sample_rate,
                inspection.channels,
                inspection.actual_data_bytes,
            );
            let mut file = OpenOptions::new().write(true).open(path).map_err(|e| {
                AppError::Audio(format!(
                    "録音ファイルを開けません ({}): {e}",
                    path.display()
                ))
            })?;
            file.seek(SeekFrom::Start(0))
                .map_err(|e| AppError::Audio(format!("ヘッダ位置へ移動できません: {e}")))?;
            file.write_all(&header)
                .map_err(|e| AppError::Audio(format!("ヘッダを修復できません: {e}")))?;
            file.sync_all()
                .map_err(|e| AppError::Audio(format!("修復内容を確定できません: {e}")))?;

            tracing::info!(
                path = %path.display(),
                bytes = inspection.actual_data_bytes,
                duration_ms = inspection.duration_ms,
                "WAVヘッダを修復しました"
            );
            Ok(path.to_path_buf())
        }

        WavCondition::HeaderBroken => {
            // ヘッダが壊れている場合、中身は生 PCM とみなして別ファイルへ救出する。
            // 元ファイルには一切手を触れない。
            let rescued = path.with_extension("recovered.wav");
            let mut src = std::fs::File::open(path).map_err(|e| {
                AppError::Audio(format!(
                    "録音ファイルを開けません ({}): {e}",
                    path.display()
                ))
            })?;
            let mut pcm = Vec::new();
            src.read_to_end(&mut pcm)
                .map_err(|e| AppError::Audio(format!("録音データを読み取れません: {e}")))?;

            let mut out = std::fs::File::create(&rescued).map_err(|e| {
                AppError::Audio(format!(
                    "救出ファイルを作成できません ({}): {e}",
                    rescued.display()
                ))
            })?;
            out.write_all(&build_header(
                inspection.sample_rate,
                inspection.channels,
                pcm.len() as u64,
            ))
            .and_then(|()| out.write_all(&pcm))
            .and_then(|()| out.sync_all())
            .map_err(|e| AppError::Audio(format!("救出ファイルを書き出せません: {e}")))?;

            tracing::warn!(
                original = %path.display(),
                rescued = %rescued.display(),
                "WAVヘッダが壊れていたため生PCMとして救出しました"
            );
            Ok(rescued)
        }

        WavCondition::Empty => Err(AppError::Audio(
            "録音データが記録されていません。".to_string(),
        )),
        WavCondition::Missing => Err(AppError::NotFound(format!(
            "録音ファイル {}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::wav_sink::WavSink;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("blistener-rec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn detects_intact_file() {
        let dir = temp_dir();
        let path = dir.join("audio.wav");
        let mut sink = WavSink::create(&path, 16_000, 1).unwrap();
        sink.write_samples(&vec![0.0; 1600]).unwrap();
        sink.finalize().unwrap();

        let got = inspect(&path, 16_000, 1);
        assert_eq!(got.condition, WavCondition::Intact);
        assert_eq!(got.duration_ms, 100);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 「録音中にアプリが強制終了した」状況を再現する。
    /// ヘッダ更新前に落ちた場合でも、全サンプルが復元できることを保証する。
    #[test]
    fn repairs_outdated_header_without_losing_audio() {
        let dir = temp_dir();
        let path = dir.join("audio.wav");

        // ヘッダは 0 のまま、PCM だけが追記された状態を作る。
        let mut sink = WavSink::create(&path, 16_000, 1).unwrap();
        sink.write_samples(&vec![0.5; 16_000]).unwrap();
        drop(sink); // finalize せずに破棄 = クラッシュ相当

        let before = inspect(&path, 16_000, 1);
        assert_eq!(before.condition, WavCondition::HeaderOutdated);
        assert_eq!(before.actual_data_bytes, 32_000);
        assert_eq!(before.duration_ms, 1000);

        let repaired = repair(&path, &before).unwrap();
        assert_eq!(repaired, path);

        let after = inspect(&path, 16_000, 1);
        assert_eq!(after.condition, WavCondition::Intact);
        assert_eq!(after.header_data_bytes, 32_000);
        assert_eq!(after.duration_ms, 1000);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rescues_broken_header_to_new_file() {
        let dir = temp_dir();
        let path = dir.join("audio.wav");
        // RIFF ヘッダを持たない生 PCM
        std::fs::write(&path, vec![7u8; 32_000]).unwrap();

        let got = inspect(&path, 16_000, 1);
        assert_eq!(got.condition, WavCondition::HeaderBroken);

        let rescued = repair(&path, &got).unwrap();
        assert!(rescued.to_string_lossy().ends_with("recovered.wav"));
        // 元ファイルは無傷であること
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 32_000);

        let checked = inspect(&rescued, 16_000, 1);
        assert_eq!(checked.condition, WavCondition::Intact);
        assert_eq!(checked.actual_data_bytes, 32_000);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reports_empty_file() {
        let dir = temp_dir();
        let path = dir.join("audio.wav");
        let sink = WavSink::create(&path, 16_000, 1).unwrap();
        sink.finalize().unwrap();

        let got = inspect(&path, 16_000, 1);
        assert_eq!(got.condition, WavCondition::Empty);
        std::fs::remove_dir_all(&dir).ok();
    }
}
