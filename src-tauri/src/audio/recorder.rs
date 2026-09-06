//! 録音セッション。アプリの最重要機能。
//!
//! # スレッド構成
//! ```text
//! [cpal コールバック]  OS のオーディオスレッド。確保・ロック・I/O を最小限にする
//!        │ mono へダウンミックスして送るだけ
//!        ▼
//!   bounded channel   容量は数秒ぶん。満杯なら送信側が待つ（＝取りこぼさない）
//!        ▼
//! [Writer スレッド]   リサンプル → WAV へ追記 → 5 秒ごとに flush + ヘッダ更新
//! ```
//!
//! リアルタイム文字起こし用の経路（Phase 7）は Writer から `try_send` で分岐させ、
//! 詰まった場合は破棄する。録音経路は決して詰まらせない。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, StreamTrait};
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::sysutil::SleepBlocker;

use super::devices;
use super::resample::{downmix_to_mono, Resampler16k, TARGET_SAMPLE_RATE};
use super::wav_sink::{FinalizedWav, WavSink};

/// 録音経路のチャンネル容量。48kHz / 480 フレーム換算でおよそ 20 秒ぶん。
/// ここが埋まるほど書き込みが遅延することは通常ないが、
/// 万一詰まった場合は「捨てる」のではなく「待つ」ことでデータを守る。
const RECORD_CHANNEL_CAPACITY: usize = 2048;

/// リアルタイム経路のチャンネル容量。こちらは詰まったら破棄してよい。
const REALTIME_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecorderState {
    Recording,
    Paused,
}

/// UI へ返す録音状態のスナップショット。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSnapshot {
    pub meeting_id: String,
    pub state: RecorderState,
    /// 実際に録音された音声の長さ（一時停止分を含まない）。
    pub elapsed_ms: i64,
    /// 直近の入力レベル（0.0〜1.0）。マイクが拾えているかの確認に使う。
    pub level: f32,
    pub bytes_written: u64,
    pub audio_path: String,
    pub device_name: String,
    /// 録音を継続できない致命的なエラー。発生後も既存の音声は保全される。
    pub error: Option<String>,
}

/// 録音スレッド間で共有する状態。すべてロックフリーで読み書きする。
struct SharedStatus {
    paused: AtomicBool,
    samples_written: AtomicU64,
    /// 直近の入力ピークレベル。f32 のビット表現で保持する。
    level_bits: AtomicU32,
    /// リアルタイム経路で破棄したチャンク数（録音には影響しない）。
    dropped_realtime: AtomicU64,
    fatal_error: Mutex<Option<String>>,
}

impl SharedStatus {
    fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            samples_written: AtomicU64::new(0),
            level_bits: AtomicU32::new(0),
            dropped_realtime: AtomicU64::new(0),
            fatal_error: Mutex::new(None),
        }
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    fn set_level(&self, level: f32) {
        self.level_bits.store(level.to_bits(), Ordering::Relaxed);
    }

    fn set_fatal(&self, message: String) {
        tracing::error!(error = %message, "録音中に致命的なエラーが発生しました");
        match self.fatal_error.lock() {
            Ok(mut g) => {
                if g.is_none() {
                    *g = Some(message);
                }
            }
            Err(poisoned) => {
                let mut g = poisoned.into_inner();
                if g.is_none() {
                    *g = Some(message);
                }
            }
        }
    }

    fn fatal(&self) -> Option<String> {
        match self.fatal_error.lock() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

enum ControlMsg {
    Stop,
}

struct Active {
    meeting_id: String,
    audio_path: PathBuf,
    device_name: String,
    status: Arc<SharedStatus>,
    control_tx: SyncSender<ControlMsg>,
    control_handle: JoinHandle<()>,
    writer_handle: JoinHandle<AppResult<FinalizedWav>>,
}

pub struct RecorderConfig {
    pub meeting_id: String,
    pub audio_path: PathBuf,
    pub device_name: Option<String>,
    /// リアルタイム文字起こしへ 16kHz サンプルを流すか（Phase 7 で使用）。
    pub realtime_output: bool,
}

/// 同時に 1 つだけ存在する録音セッションを管理する。
pub struct Recorder {
    active: Mutex<Option<Active>>,
    /// リアルタイム文字起こしへ渡す 16kHz サンプルの受け口（Phase 7 で使用）。
    realtime_rx: Mutex<Option<Receiver<Vec<f32>>>>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
            realtime_rx: Mutex::new(None),
        }
    }

    fn lock_active(&self) -> AppResult<std::sync::MutexGuard<'_, Option<Active>>> {
        self.active
            .lock()
            .map_err(|_| AppError::Audio("録音状態を取得できませんでした".into()))
    }

    pub fn is_active(&self) -> bool {
        self.lock_active().map(|g| g.is_some()).unwrap_or(false)
    }

    /// 録音を開始する。既に録音中の場合はエラーを返す。
    pub fn start(&self, config: RecorderConfig) -> AppResult<RecordingSnapshot> {
        let mut guard = self.lock_active()?;
        if guard.is_some() {
            return Err(AppError::Audio(
                "すでに別の会議を録音中です。先に会議を終了してください。".to_string(),
            ));
        }

        let device = devices::resolve_input_device(config.device_name.as_deref())?;
        let device_name = device.name().unwrap_or_else(|_| "不明なマイク".to_string());

        let supported = device.default_input_config().map_err(|e| {
            AppError::Audio(format!(
                "マイク「{device_name}」の設定を取得できません: {e}。\
                 OSのプライバシー設定でマイクの使用が許可されているか確認してください。"
            ))
        })?;
        let sample_format = supported.sample_format();
        let stream_config: cpal::StreamConfig = supported.into();
        let source_rate = stream_config.sample_rate.0;
        let channels = stream_config.channels as usize;

        tracing::info!(
            meeting_id = %config.meeting_id,
            device = %device_name,
            source_rate,
            channels,
            format = ?sample_format,
            "録音を開始します"
        );

        // WAV は「これから書く」側のスレッドで開く。開けなければ録音自体を始めない。
        let sink = WavSink::create(&config.audio_path, TARGET_SAMPLE_RATE, 1)?;
        let resampler = Resampler16k::new(source_rate)?;

        let status = Arc::new(SharedStatus::new());
        let (audio_tx, audio_rx) = sync_channel::<Vec<f32>>(RECORD_CHANNEL_CAPACITY);
        let (control_tx, control_rx) = sync_channel::<ControlMsg>(1);
        let (ready_tx, ready_rx) = sync_channel::<Result<(), String>>(1);

        // リアルタイム文字起こし用の分岐は、実際に消費する側がいるときだけ作る。
        // 誰も読まないチャンネルへサンプルを複製するのは無駄なため。
        let (realtime_tx, realtime_rx) = if config.realtime_output {
            let (tx, rx) = sync_channel::<Vec<f32>>(REALTIME_CHANNEL_CAPACITY);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let writer_handle = spawn_writer(sink, resampler, audio_rx, realtime_tx, status.clone());

        let control_handle = spawn_control(
            device,
            stream_config,
            sample_format,
            channels,
            audio_tx,
            status.clone(),
            control_rx,
            ready_tx,
        );

        // ストリームの構築結果を待つ。ここで失敗したら録音を開始しない。
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(message)) => {
                let _ = control_handle.join();
                let _ = writer_handle.join();
                std::fs::remove_file(&config.audio_path).ok();
                return Err(AppError::Audio(message));
            }
            Err(e) => {
                let _ = control_handle.join();
                let _ = writer_handle.join();
                std::fs::remove_file(&config.audio_path).ok();
                return Err(AppError::Audio(format!(
                    "録音スレッドの起動に失敗しました: {e}"
                )));
            }
        }

        let snapshot = RecordingSnapshot {
            meeting_id: config.meeting_id.clone(),
            state: RecorderState::Recording,
            elapsed_ms: 0,
            level: 0.0,
            bytes_written: 0,
            audio_path: config.audio_path.display().to_string(),
            device_name: device_name.clone(),
            error: None,
        };

        *guard = Some(Active {
            meeting_id: config.meeting_id,
            audio_path: config.audio_path,
            device_name,
            status,
            control_tx,
            control_handle,
            writer_handle,
        });

        if let Ok(mut rt) = self.realtime_rx.lock() {
            *rt = realtime_rx;
        }

        Ok(snapshot)
    }

    pub fn pause(&self) -> AppResult<RecordingSnapshot> {
        let guard = self.lock_active()?;
        let active = guard
            .as_ref()
            .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?;
        active.status.paused.store(true, Ordering::SeqCst);
        tracing::info!(meeting_id = %active.meeting_id, "録音を一時停止しました");
        Ok(snapshot_of(active))
    }

    pub fn resume(&self) -> AppResult<RecordingSnapshot> {
        let guard = self.lock_active()?;
        let active = guard
            .as_ref()
            .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?;
        active.status.paused.store(false, Ordering::SeqCst);
        tracing::info!(meeting_id = %active.meeting_id, "録音を再開しました");
        Ok(snapshot_of(active))
    }

    pub fn snapshot(&self) -> Option<RecordingSnapshot> {
        self.lock_active().ok()?.as_ref().map(snapshot_of)
    }

    /// 録音を停止し、WAV を確定させる。
    ///
    /// 途中でエラーが起きていても、書き込み済みの音声は必ずファイルに残る。
    pub fn stop(&self) -> AppResult<FinalizedWav> {
        let active = {
            let mut guard = self.lock_active()?;
            guard
                .take()
                .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?
        };

        if let Ok(mut rt) = self.realtime_rx.lock() {
            *rt = None;
        }

        // 制御スレッドへ停止を伝える。ストリームが drop されると
        // コールバックが持つ送信端も落ち、Writer スレッドが終了処理へ入る。
        let _ = active.control_tx.send(ControlMsg::Stop);
        if active.control_handle.join().is_err() {
            tracing::error!("録音制御スレッドが異常終了しました");
        }

        let finalized = match active.writer_handle.join() {
            Ok(result) => result,
            Err(_) => Err(AppError::Audio(
                "録音書き込みスレッドが異常終了しました。ファイルの復旧を試みてください。"
                    .to_string(),
            )),
        };

        let dropped = active.status.dropped_realtime.load(Ordering::Relaxed);
        if dropped > 0 {
            tracing::warn!(
                dropped,
                "リアルタイム処理向けのチャンクを破棄しました（録音データには影響しません）"
            );
        }
        if let Some(err) = active.status.fatal() {
            tracing::error!(error = %err, "録音中のエラーを検出しました");
        }

        tracing::info!(
            meeting_id = %active.meeting_id,
            path = %active.audio_path.display(),
            device = %active.device_name,
            "録音を終了しました"
        );

        finalized
    }
}

fn snapshot_of(active: &Active) -> RecordingSnapshot {
    let samples = active.status.samples_written.load(Ordering::Relaxed);
    RecordingSnapshot {
        meeting_id: active.meeting_id.clone(),
        state: if active.status.paused.load(Ordering::Relaxed) {
            RecorderState::Paused
        } else {
            RecorderState::Recording
        },
        elapsed_ms: (samples as i64 * 1000) / TARGET_SAMPLE_RATE as i64,
        level: active.status.level(),
        bytes_written: samples * 2,
        audio_path: active.audio_path.display().to_string(),
        device_name: active.device_name.clone(),
        error: active.status.fatal(),
    }
}

/// WAV へ書き込むスレッド。ディスク I/O はすべてここで行う。
fn spawn_writer(
    mut sink: WavSink,
    mut resampler: Resampler16k,
    audio_rx: Receiver<Vec<f32>>,
    realtime_tx: Option<SyncSender<Vec<f32>>>,
    status: Arc<SharedStatus>,
) -> JoinHandle<AppResult<FinalizedWav>> {
    std::thread::Builder::new()
        .name("b-listener-audio-writer".into())
        .spawn(move || {
            let mut converted: Vec<f32> = Vec::with_capacity(4096);
            let mut write_error: Option<AppError> = None;

            while let Ok(chunk) = audio_rx.recv() {
                if write_error.is_some() {
                    // 書き込みに失敗した後も受信は続ける。
                    // ここで止めるとコールバック側が詰まって音が乱れるため。
                    continue;
                }

                converted.clear();
                if let Err(e) = resampler.process(&chunk, &mut converted) {
                    status.set_fatal(e.to_string());
                    write_error = Some(e);
                    continue;
                }
                if converted.is_empty() {
                    continue;
                }

                if let Err(e) = sink.write_samples(&converted) {
                    status.set_fatal(e.to_string());
                    write_error = Some(e);
                    continue;
                }
                status
                    .samples_written
                    .fetch_add(converted.len() as u64, Ordering::Relaxed);
                status.set_level(peak(&converted));

                if let Err(e) = sink.maybe_sync() {
                    // 同期に失敗しても書き込み自体は続ける。次回の同期で回復する可能性がある。
                    tracing::warn!(error = %e, "録音データの定期同期に失敗しました");
                }

                // リアルタイム経路へ分岐。詰まっていたら捨てる（録音は止めない）。
                if let Some(tx) = realtime_tx.as_ref() {
                    match tx.try_send(converted.clone()) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            status.dropped_realtime.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(TrySendError::Disconnected(_)) => {}
                    }
                }
            }

            // 送信側が落ちた = 停止。端数を吐き出してから確定させる。
            converted.clear();
            if write_error.is_none() {
                match resampler.flush(&mut converted) {
                    Ok(()) => {
                        if !converted.is_empty() {
                            if let Err(e) = sink.write_samples(&converted) {
                                tracing::warn!(error = %e, "末尾サンプルを書き込めませんでした");
                            } else {
                                status
                                    .samples_written
                                    .fetch_add(converted.len() as u64, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "末尾サンプルを変換できませんでした"),
                }
            }

            // 書き込みエラーがあっても finalize は必ず行う。
            // ここまでに書けたぶんを再生可能な状態で残すため。
            let finalized = sink.finalize();

            match (write_error, finalized) {
                (Some(err), Ok(out)) => {
                    tracing::error!(
                        error = %err,
                        path = %out.path.display(),
                        duration_ms = out.duration_ms,
                        "録音は途中で失敗しましたが、そこまでの音声は保存されています"
                    );
                    Err(err)
                }
                (Some(err), Err(_)) => Err(err),
                (None, result) => result,
            }
        })
        .expect("録音書き込みスレッドを起動できません")
}

/// cpal のストリームを保持する制御スレッド。
///
/// `cpal::Stream` は `Send` ではないため、生成したスレッド上で保持し、
/// 同じスレッドで drop する必要がある。
#[allow(clippy::too_many_arguments)]
fn spawn_control(
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    channels: usize,
    audio_tx: SyncSender<Vec<f32>>,
    status: Arc<SharedStatus>,
    control_rx: Receiver<ControlMsg>,
    ready_tx: SyncSender<Result<(), String>>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("b-listener-audio-control".into())
        .spawn(move || {
            // Windows ではスリープ抑止がスレッドに紐づくため、このスレッドで保持する。
            let _sleep_blocker = SleepBlocker::activate();

            let stream = match build_stream(
                &device,
                &stream_config,
                sample_format,
                channels,
                audio_tx,
                status.clone(),
            ) {
                Ok(s) => s,
                Err(message) => {
                    let _ = ready_tx.send(Err(message));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(format!(
                    "録音を開始できませんでした: {e}。\
                     マイクが他のアプリで使用されていないか確認してください。"
                )));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            // 停止指示が来るまでストリームを保持し続ける。
            match control_rx.recv() {
                Ok(ControlMsg::Stop) => {}
                Err(_) => tracing::warn!("録音制御チャンネルが切断されました"),
            }

            // stream を drop するとコールバックが停止し、
            // コールバックが持つ送信端も落ちて Writer スレッドが終了処理へ入る。
            drop(stream);
        })
        .expect("録音制御スレッドを起動できません")
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    channels: usize,
    audio_tx: SyncSender<Vec<f32>>,
    status: Arc<SharedStatus>,
) -> Result<cpal::Stream, String> {
    let error_status = status.clone();
    let on_error = move |e: cpal::StreamError| {
        error_status.set_fatal(match e {
            cpal::StreamError::DeviceNotAvailable => {
                "マイクが取り外されたため録音を継続できません。ここまでの音声は保存されています。"
                    .to_string()
            }
            other => format!("録音デバイスでエラーが発生しました: {other}"),
        });
    };

    macro_rules! build {
        ($sample:ty, $to_f32:expr) => {{
            let status = status.clone();
            let tx = audio_tx.clone();
            device.build_input_stream(
                config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    // 一時停止中はサンプルを捨てる。ストリーム自体は止めない
                    // （デバイスの再取得に失敗するリスクを避けるため）。
                    if status.paused.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut mono = Vec::with_capacity(data.len() / channels.max(1) + 1);
                    let converter = $to_f32;
                    // ダウンミックスのみを行う。リサンプルなどの重い処理は Writer 側で行う。
                    if channels <= 1 {
                        mono.extend(data.iter().copied().map(converter));
                    } else {
                        let floats: Vec<f32> = data.iter().copied().map(converter).collect();
                        downmix_to_mono(&floats, channels, &mut mono);
                    }
                    if mono.is_empty() {
                        return;
                    }
                    // 満杯なら待つ。捨てない。録音データの保全を最優先する。
                    if tx.send(mono).is_err() {
                        // 受信側が終了している = 停止処理中。何もしない。
                    }
                },
                on_error,
                None,
            )
        }};
    }

    let result = match sample_format {
        cpal::SampleFormat::F32 => build!(f32, |v: f32| v),
        cpal::SampleFormat::I16 => build!(i16, |v: i16| v as f32 / i16::MAX as f32),
        cpal::SampleFormat::U16 => {
            build!(u16, |v: u16| (v as f32 - 32768.0) / 32768.0)
        }
        cpal::SampleFormat::I32 => {
            build!(i32, |v: i32| v as f32 / i32::MAX as f32)
        }
        other => {
            return Err(format!(
                "このマイクの音声形式 ({other:?}) には対応していません。\
                 別のマイクを選択してください。"
            ))
        }
    };

    result.map_err(|e| {
        format!(
            "録音を開始できませんでした: {e}。\
             マイクの接続と、OSのプライバシー設定でのマイク使用許可を確認してください。"
        )
    })
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |acc, s| acc.max(s.abs()))
        .min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_returns_max_absolute_value() {
        assert_eq!(peak(&[0.1, -0.8, 0.3]), 0.8);
        assert_eq!(peak(&[]), 0.0);
        assert_eq!(peak(&[5.0]), 1.0);
    }
}
