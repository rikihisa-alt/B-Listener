//! WAV への逐次書き込み。
//!
//! # なぜ WAV なのか
//! 最優先要件は「録音データを絶対に失わない」こと。
//! WAV はヘッダ以降が生の PCM が並ぶだけなので、途中で強制終了しても
//! 「ファイル末尾までのバイト列」がそのまま有効な音声として残る。
//! m4a / Opus はコンテナのインデックスをファイル末尾に書くため、
//! 書き終わる前に落ちると全損する。
//!
//! # 失わないための書き込み手順
//!
//! 1. サンプルは受け取り次第 `BufWriter` に流す（メモリに溜め込まない）
//! 2. 一定間隔で `flush` → ヘッダのサイズ欄を実長に更新 → `sync_data`
//!
//! これにより、どのタイミングで落ちても直前の更新時点までは
//! 「正しいヘッダを持つ再生可能なファイル」が残る。
//! さらにヘッダが古い場合でも、余剰バイトは有効な PCM なので
//! [`crate::audio::recovery`] がヘッダを書き直すだけで全て復元できる。

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// 標準的な PCM WAV のヘッダ長。
pub const WAV_HEADER_LEN: u64 = 44;

/// ディスクへ確実に書き出す間隔。
/// 短すぎると I/O が増え、長すぎるとクラッシュ時の損失が増えるため 5 秒とする。
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// 16bit PCM の WAV を逐次書き出すシンク。
pub struct WavSink {
    path: PathBuf,
    writer: BufWriter<File>,
    sample_rate: u32,
    channels: u16,
    /// data チャンクに書き込んだバイト数。
    data_bytes: u64,
    /// ヘッダへ反映済みのバイト数。
    synced_bytes: u64,
    last_flush: Instant,
}

impl WavSink {
    /// 新規作成し、サイズ欄が 0 の仮ヘッダを書き込む。
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Audio(format!(
                    "録音フォルダを作成できません ({}): {e}",
                    parent.display()
                ))
            })?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| {
                AppError::Audio(format!(
                    "録音ファイルを作成できません ({}): {e}",
                    path.display()
                ))
            })?;

        let mut sink = Self {
            path: path.to_path_buf(),
            writer: BufWriter::with_capacity(64 * 1024, file),
            sample_rate,
            channels,
            data_bytes: 0,
            synced_bytes: 0,
            last_flush: Instant::now(),
        };

        sink.write_header(0)?;
        // 仮ヘッダの時点で一度ディスクへ落としておく。
        // 開始直後にクラッシュしてもファイルの体裁が保たれる。
        sink.flush_and_sync()?;

        tracing::info!(path = %path.display(), sample_rate, channels, "録音ファイルを作成しました");
        Ok(sink)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 書き込み済みの音声長（ミリ秒）。
    pub fn duration_ms(&self) -> i64 {
        let frames = self.data_bytes / (2 * self.channels as u64);
        (frames as i64 * 1000) / self.sample_rate.max(1) as i64
    }

    pub fn data_bytes(&self) -> u64 {
        self.data_bytes
    }

    /// f32 サンプル (-1.0..1.0) を 16bit PCM として追記する。
    pub fn write_samples(&mut self, samples: &[f32]) -> AppResult<()> {
        // スタック上の固定バッファで変換し、ヒープ確保を避ける。
        let mut buf = [0u8; 4096];
        let mut filled = 0usize;

        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            let bytes = v.to_le_bytes();
            buf[filled] = bytes[0];
            buf[filled + 1] = bytes[1];
            filled += 2;

            if filled == buf.len() {
                self.write_all(&buf)?;
                filled = 0;
            }
        }
        if filled > 0 {
            self.write_all(&buf[..filled])?;
        }
        Ok(())
    }

    /// 既に 16bit PCM になっているサンプルを追記する。
    ///
    /// ブラウザから送られてくる録音データはこの形式のため、
    /// f32 への往復変換を挟まずそのまま書き込む。
    pub fn write_i16_samples(&mut self, samples: &[i16]) -> AppResult<()> {
        let mut buf = [0u8; 4096];
        let mut filled = 0usize;

        for &v in samples {
            let bytes = v.to_le_bytes();
            buf[filled] = bytes[0];
            buf[filled + 1] = bytes[1];
            filled += 2;

            if filled == buf.len() {
                self.write_all(&buf)?;
                filled = 0;
            }
        }
        if filled > 0 {
            self.write_all(&buf[..filled])?;
        }
        Ok(())
    }

    fn write_all(&mut self, bytes: &[u8]) -> AppResult<()> {
        self.writer
            .write_all(bytes)
            .map_err(|e| AppError::Audio(format!("録音データを書き込めません: {e}")))?;
        self.data_bytes += bytes.len() as u64;
        Ok(())
    }

    /// 一定時間が経過していればディスクへ確定させる。書き込みループから毎回呼んでよい。
    pub fn maybe_sync(&mut self) -> AppResult<()> {
        if self.last_flush.elapsed() < FLUSH_INTERVAL {
            return Ok(());
        }
        self.sync()
    }

    /// ヘッダを実長に更新し、ディスクへ確定させる。
    pub fn sync(&mut self) -> AppResult<()> {
        if self.data_bytes != self.synced_bytes {
            let data_bytes = self.data_bytes;
            self.write_header(data_bytes)?;
            self.synced_bytes = data_bytes;
        }
        self.flush_and_sync()?;
        self.last_flush = Instant::now();
        Ok(())
    }

    /// 録音終了時に呼ぶ。ヘッダを確定し、メタデータまで含めて同期する。
    pub fn finalize(mut self) -> AppResult<FinalizedWav> {
        self.sync()?;
        self.writer
            .get_ref()
            .sync_all()
            .map_err(|e| AppError::Audio(format!("録音ファイルを確定できません: {e}")))?;

        tracing::info!(
            path = %self.path.display(),
            bytes = self.data_bytes,
            duration_ms = self.duration_ms(),
            "録音ファイルを確定しました"
        );

        Ok(FinalizedWav {
            path: self.path.clone(),
            duration_ms: self.duration_ms(),
            data_bytes: self.data_bytes,
            sample_rate: self.sample_rate,
        })
    }

    fn flush_and_sync(&mut self) -> AppResult<()> {
        self.writer
            .flush()
            .map_err(|e| AppError::Audio(format!("録音データを書き出せません: {e}")))?;
        // sync_data はメタデータを同期しない分 sync_all より軽い。
        // 録音中はこちらで十分（ファイルサイズはデータ同期に含まれる）。
        self.writer
            .get_ref()
            .sync_data()
            .map_err(|e| AppError::Audio(format!("録音データを同期できません: {e}")))?;
        Ok(())
    }

    /// 44 バイトの PCM WAV ヘッダを先頭へ書き込み、書き込み位置を末尾へ戻す。
    fn write_header(&mut self, data_bytes: u64) -> AppResult<()> {
        let header = build_header(self.sample_rate, self.channels, data_bytes);

        self.writer
            .seek(SeekFrom::Start(0))
            .map_err(|e| AppError::Audio(format!("ヘッダ位置へ移動できません: {e}")))?;
        self.writer
            .write_all(&header)
            .map_err(|e| AppError::Audio(format!("ヘッダを書き込めません: {e}")))?;
        self.writer
            .seek(SeekFrom::Start(WAV_HEADER_LEN + data_bytes))
            .map_err(|e| AppError::Audio(format!("書き込み位置へ戻れません: {e}")))?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FinalizedWav {
    pub path: PathBuf,
    pub duration_ms: i64,
    pub data_bytes: u64,
    pub sample_rate: u32,
}

/// 16bit PCM WAV の 44 バイトヘッダを組み立てる。
pub fn build_header(sample_rate: u32, channels: u16, data_bytes: u64) -> [u8; 44] {
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;
    // 4GB を超える WAV は表現できない。3時間の 16kHz mono は約 346MB なので実用上問題ない。
    let data_len = data_bytes.min(u32::MAX as u64 - WAV_HEADER_LEN) as u32;

    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt チャンク長
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_has_expected_layout() {
        let h = build_header(16_000, 1, 32_000);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(&h[8..12], b"WAVE");
        assert_eq!(&h[36..40], b"data");
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 32_000);
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 32_036);
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 16_000);
        // byte rate = 16000 * 2
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 32_000);
    }

    #[test]
    fn writes_and_finalizes_playable_file() {
        let dir = std::env::temp_dir().join(format!("blistener-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audio.wav");

        let mut sink = WavSink::create(&path, 16_000, 1).unwrap();
        // 1 秒ぶんの無音 + 1 サンプルの最大振幅
        let mut samples = vec![0.0f32; 16_000];
        samples.push(1.0);
        sink.write_samples(&samples).unwrap();
        let out = sink.finalize().unwrap();

        assert_eq!(out.data_bytes, 16_001 * 2);
        assert_eq!(out.duration_ms, 1000);

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, WAV_HEADER_LEN + out.data_bytes);
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as u64,
            out.data_bytes
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
