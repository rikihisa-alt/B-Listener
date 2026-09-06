//! ブラウザから送られてくる録音データの受け口。
//!
//! ブラウザ版では、マイクの取得はブラウザ側（AudioWorklet）で行い、
//! 16kHz / mono / 16bit PCM のチャンクを HTTP で送ってもらう。
//! サーバ側の書き込みは [`WavSink`] を通すため、
//! **デスクトップ版とまったく同じクラッシュ耐性**（逐次追記・定期 flush・
//! ヘッダ更新・落ちてもヘッダ修復で全復元）が得られる。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{AppError, AppResult};

use super::recorder::{RecorderState, RecordingSnapshot};
use super::resample::TARGET_SAMPLE_RATE;
use super::wav_sink::{FinalizedWav, WavSink};

struct Session {
    meeting_id: String,
    audio_path: PathBuf,
    sink: WavSink,
    /// 書き込んだサンプル数（16kHz mono）。
    samples: u64,
    /// 直近チャンクのピークレベル。
    level: f32,
    paused: bool,
    /// 送信元の表示名（UI に出す）。
    client_label: String,
}

/// 同時に 1 つだけ存在するアップロード録音セッション。
pub struct UploadRecorder {
    session: Mutex<Option<Session>>,
}

impl Default for UploadRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadRecorder {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Option<Session>>> {
        self.session
            .lock()
            .map_err(|_| AppError::Audio("録音状態を取得できませんでした".into()))
    }

    pub fn is_active(&self) -> bool {
        self.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// 録音を開始し、WAV を作成する。
    pub fn start(
        &self,
        meeting_id: String,
        audio_path: &Path,
        client_label: String,
    ) -> AppResult<RecordingSnapshot> {
        let mut guard = self.lock()?;
        if guard.is_some() {
            return Err(AppError::Audio(
                "すでに別の会議を録音中です。先に会議を終了してください。".to_string(),
            ));
        }

        let sink = WavSink::create(audio_path, TARGET_SAMPLE_RATE, 1)?;
        tracing::info!(
            meeting_id = %meeting_id,
            client = %client_label,
            path = %audio_path.display(),
            "ブラウザからの録音を開始します"
        );

        *guard = Some(Session {
            meeting_id: meeting_id.clone(),
            audio_path: audio_path.to_path_buf(),
            sink,
            samples: 0,
            level: 0.0,
            paused: false,
            client_label: client_label.clone(),
        });

        Ok(RecordingSnapshot {
            meeting_id,
            state: RecorderState::Recording,
            elapsed_ms: 0,
            level: 0.0,
            bytes_written: 0,
            audio_path: audio_path.display().to_string(),
            device_name: client_label,
            error: None,
        })
    }

    /// 16bit PCM のチャンクを追記する。
    ///
    /// 受け取り次第ディスクへ流し、[`WavSink`] の定期同期に任せる。
    /// メモリに溜め込まないため、通信が途切れてもそこまでの音声は残る。
    pub fn append_pcm(&self, meeting_id: &str, pcm: &[u8]) -> AppResult<u64> {
        let mut guard = self.lock()?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?;

        if session.meeting_id != meeting_id {
            return Err(AppError::Invalid(
                "別の会議の録音データが送信されました。".to_string(),
            ));
        }
        if session.paused {
            // 一時停止中のデータは捨てる（デスクトップ版と同じ挙動）。
            return Ok(session.samples);
        }
        if pcm.len() < 2 {
            return Ok(session.samples);
        }

        // 奇数バイトで途切れていても、残りは次のチャンクの先頭になるため切り捨てる。
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();

        session.sink.write_i16_samples(&samples)?;
        session.samples += samples.len() as u64;
        session.level = samples
            .iter()
            .fold(0.0f32, |acc, &s| acc.max((s as f32 / 32768.0).abs()))
            .min(1.0);

        if let Err(e) = session.sink.maybe_sync() {
            // 同期に失敗しても受信は続ける。次回の同期で回復する可能性がある。
            tracing::warn!(error = %e, "録音データの定期同期に失敗しました");
        }
        Ok(session.samples)
    }

    pub fn set_paused(&self, paused: bool) -> AppResult<RecordingSnapshot> {
        let mut guard = self.lock()?;
        let session = guard
            .as_mut()
            .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?;
        session.paused = paused;
        if paused {
            // 一時停止のタイミングで確実にディスクへ落としておく。
            if let Err(e) = session.sink.sync() {
                tracing::warn!(error = %e, "一時停止時の同期に失敗しました");
            }
        }
        Ok(snapshot_of(session))
    }

    pub fn snapshot(&self) -> Option<RecordingSnapshot> {
        self.lock().ok()?.as_ref().map(snapshot_of)
    }

    /// 録音を終了し、WAV を確定させる。
    pub fn stop(&self) -> AppResult<FinalizedWav> {
        let session = {
            let mut guard = self.lock()?;
            guard
                .take()
                .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?
        };

        tracing::info!(
            meeting_id = %session.meeting_id,
            path = %session.audio_path.display(),
            client = %session.client_label,
            "ブラウザからの録音を終了しました"
        );
        session.sink.finalize()
    }
}

fn snapshot_of(session: &Session) -> RecordingSnapshot {
    RecordingSnapshot {
        meeting_id: session.meeting_id.clone(),
        state: if session.paused {
            RecorderState::Paused
        } else {
            RecorderState::Recording
        },
        elapsed_ms: (session.samples as i64 * 1000) / TARGET_SAMPLE_RATE as i64,
        level: session.level,
        bytes_written: session.samples * 2,
        audio_path: session.audio_path.display().to_string(),
        device_name: session.client_label.clone(),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::recovery::{self, WavCondition};

    #[test]
    fn writes_uploaded_pcm_and_finalizes() {
        let dir = std::env::temp_dir().join(format!("blistener-upload-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audio.wav");
        let recorder = UploadRecorder::new();

        recorder
            .start("m1".into(), &path, "ブラウザ".into())
            .unwrap();

        // 1 秒ぶん（16000 サンプル）を 4 チャンクに分けて送る
        let chunk: Vec<u8> = (0..4000)
            .flat_map(|i| ((i % 1000) as i16).to_le_bytes())
            .collect();
        for _ in 0..4 {
            recorder.append_pcm("m1", &chunk).unwrap();
        }

        let snapshot = recorder.snapshot().unwrap();
        assert_eq!(snapshot.elapsed_ms, 1000);

        let out = recorder.stop().unwrap();
        assert_eq!(out.duration_ms, 1000);
        assert_eq!(out.data_bytes, 32_000);

        let inspection = recovery::inspect(&path, TARGET_SAMPLE_RATE, 1);
        assert_eq!(inspection.condition, WavCondition::Intact);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ignores_chunks_while_paused() {
        let dir = std::env::temp_dir().join(format!("blistener-upload-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audio.wav");
        let recorder = UploadRecorder::new();
        recorder
            .start("m1".into(), &path, "ブラウザ".into())
            .unwrap();

        let chunk: Vec<u8> = vec![0u8; 3200]; // 1600 サンプル = 100ms
        recorder.append_pcm("m1", &chunk).unwrap();
        recorder.set_paused(true).unwrap();
        recorder.append_pcm("m1", &chunk).unwrap();
        recorder.set_paused(false).unwrap();
        recorder.append_pcm("m1", &chunk).unwrap();

        // 一時停止中のぶんは含まれない
        assert_eq!(recorder.snapshot().unwrap().elapsed_ms, 200);
        recorder.stop().unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_chunk_for_other_meeting() {
        let dir = std::env::temp_dir().join(format!("blistener-upload-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audio.wav");
        let recorder = UploadRecorder::new();
        recorder
            .start("m1".into(), &path, "ブラウザ".into())
            .unwrap();
        assert!(recorder.append_pcm("m2", &[0u8; 4]).is_err());
        recorder.stop().unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
