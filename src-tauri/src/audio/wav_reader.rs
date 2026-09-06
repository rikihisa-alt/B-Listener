//! WAV の読み出し。
//!
//! 3時間の会議は 16kHz mono でも f32 換算で約 690MB になる。
//! 一括で読み込むとメモリを圧迫するため、フレーム単位で少しずつ読めるようにする。
//!
//! 自分で書き出したファイルは常に 44 バイトヘッダの単純な PCM だが、
//! 復旧ファイルや他アプリで加工されたファイルも扱えるよう、チャンクを走査して探す。

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::{AppError, AppResult};

pub struct WavReader {
    reader: BufReader<File>,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    /// data チャンクに含まれるフレーム数。
    pub total_frames: u64,
    frames_read: u64,
}

impl WavReader {
    pub fn open(path: &Path) -> AppResult<Self> {
        let file = File::open(path).map_err(|e| {
            AppError::Stt(format!(
                "音声ファイルを開けません ({}): {e}",
                path.display()
            ))
        })?;
        let file_len = file
            .metadata()
            .map_err(|e| AppError::Stt(format!("音声ファイルの情報を取得できません: {e}")))?
            .len();
        let mut reader = BufReader::new(file);

        let mut riff = [0u8; 12];
        read_exact(&mut reader, &mut riff, "RIFFヘッダ")?;
        if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
            return Err(AppError::Stt(format!(
                "WAV形式ではないファイルです: {}",
                path.display()
            )));
        }

        let mut sample_rate = 0u32;
        let mut channels = 0u16;
        let mut bits_per_sample = 0u16;
        let mut format_tag = 0u16;
        let mut data_len: Option<u64> = None;
        let mut pos = 12u64;

        // fmt と data が見つかるまでチャンクを走査する。
        while pos + 8 <= file_len {
            let mut head = [0u8; 8];
            read_exact(&mut reader, &mut head, "チャンクヘッダ")?;
            let id = [head[0], head[1], head[2], head[3]];
            let declared = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as u64;
            pos += 8;

            // 宣言された長さがファイルを超える場合（＝書き込み途中）は実サイズを採用する。
            let available = file_len.saturating_sub(pos);
            let size = declared.min(available);

            match &id {
                b"fmt " => {
                    let mut fmt = vec![0u8; size as usize];
                    read_exact(&mut reader, &mut fmt, "fmtチャンク")?;
                    if fmt.len() < 16 {
                        return Err(AppError::Stt("WAVのfmt情報が壊れています".to_string()));
                    }
                    format_tag = u16::from_le_bytes([fmt[0], fmt[1]]);
                    channels = u16::from_le_bytes([fmt[2], fmt[3]]);
                    sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
                    bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
                }
                b"data" => {
                    data_len = Some(size);
                    break; // 読み取り位置は data の先頭にある
                }
                _ => {
                    reader
                        .seek(SeekFrom::Current(size as i64))
                        .map_err(|e| AppError::Stt(format!("WAVの読み取りに失敗しました: {e}")))?;
                }
            }

            pos += size;
            // チャンクは偶数境界に揃う
            if size % 2 == 1 && pos < file_len {
                reader
                    .seek(SeekFrom::Current(1))
                    .map_err(|e| AppError::Stt(format!("WAVの読み取りに失敗しました: {e}")))?;
                pos += 1;
            }
        }

        let data_len = data_len
            .ok_or_else(|| AppError::Stt("WAVに音声データが含まれていません".to_string()))?;

        if format_tag != 1 {
            return Err(AppError::Stt(format!(
                "対応していない音声形式です (format={format_tag})。16bit PCM のWAVのみ扱えます。"
            )));
        }
        if bits_per_sample != 16 {
            return Err(AppError::Stt(format!(
                "対応していないビット深度です ({bits_per_sample}bit)。16bit PCM のWAVのみ扱えます。"
            )));
        }
        if channels == 0 || sample_rate == 0 {
            return Err(AppError::Stt("WAVのfmt情報が壊れています".to_string()));
        }

        let block_align = (bits_per_sample / 8) as u64 * channels as u64;
        Ok(Self {
            reader,
            sample_rate,
            channels,
            bits_per_sample,
            total_frames: data_len / block_align,
            frames_read: 0,
        })
    }

    pub fn duration_ms(&self) -> i64 {
        (self.total_frames as i64 * 1000) / self.sample_rate.max(1) as i64
    }

    pub fn frames_read(&self) -> u64 {
        self.frames_read
    }

    /// 最大 `max_frames` フレームを mono の f32 として `out` へ追記する。
    /// 返り値は実際に読めたフレーム数。0 なら終端。
    pub fn read_mono_f32(&mut self, max_frames: usize, out: &mut Vec<f32>) -> AppResult<usize> {
        let remaining = (self.total_frames - self.frames_read) as usize;
        let want = max_frames.min(remaining);
        if want == 0 {
            return Ok(0);
        }

        let channels = self.channels as usize;
        let mut raw = vec![0u8; want * channels * 2];
        let read = fill(&mut self.reader, &mut raw)?;
        let frames = read / (channels * 2);
        if frames == 0 {
            return Ok(0);
        }

        let inv = 1.0 / channels as f32;
        for frame in raw[..frames * channels * 2].chunks_exact(channels * 2) {
            let mut sum = 0.0f32;
            for sample in frame.chunks_exact(2) {
                sum += i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0;
            }
            out.push(sum * inv);
        }

        self.frames_read += frames as u64;
        Ok(frames)
    }
}

fn read_exact(reader: &mut impl Read, buf: &mut [u8], what: &str) -> AppResult<()> {
    reader
        .read_exact(buf)
        .map_err(|e| AppError::Stt(format!("{what}を読み取れません: {e}")))
}

/// 可能な限り読み込む（末尾に達したら短く返す）。
fn fill(reader: &mut impl Read, buf: &mut [u8]) -> AppResult<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(AppError::Stt(format!("音声データを読み取れません: {e}"))),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::wav_sink::WavSink;

    #[test]
    fn reads_back_what_was_written() {
        let dir = std::env::temp_dir().join(format!("blistener-read-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audio.wav");

        let mut sink = WavSink::create(&path, 16_000, 1).unwrap();
        let written: Vec<f32> = (0..16_000)
            .map(|i| ((i % 100) as f32 / 200.0) - 0.25)
            .collect();
        sink.write_samples(&written).unwrap();
        sink.finalize().unwrap();

        let mut reader = WavReader::open(&path).unwrap();
        assert_eq!(reader.sample_rate, 16_000);
        assert_eq!(reader.channels, 1);
        assert_eq!(reader.total_frames, 16_000);
        assert_eq!(reader.duration_ms(), 1000);

        let mut got = Vec::new();
        while reader.read_mono_f32(3000, &mut got).unwrap() > 0 {}
        assert_eq!(got.len(), 16_000);
        // 16bit 量子化の誤差の範囲で一致すること
        for (a, b) in written.iter().zip(got.iter()) {
            assert!((a - b).abs() < 1.0 / 32768.0 * 2.0, "{a} vs {b}");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_non_wav() {
        let dir = std::env::temp_dir().join(format!("blistener-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.wav");
        std::fs::write(&path, b"not a wav file at all........").unwrap();
        assert!(WavReader::open(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
