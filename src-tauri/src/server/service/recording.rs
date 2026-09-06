//! （`commands/recording.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 録音の開始・一時停止・再開・終了と、クラッシュ復旧。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::audio::devices::{self, AudioInputDevice};
use crate::audio::recorder::{RecorderConfig, RecordingSnapshot};
use crate::audio::recovery::{self, WavCondition, WavInspection};
use crate::audio::resample::TARGET_SAMPLE_RATE;
use crate::db::models::{Meeting, MeetingStatus};
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::paths::{meeting_folder_name, unique_dir, MeetingFiles};
use crate::state::AppState;
use crate::sysutil;

/// 16kHz / mono / 16bit の 1 秒あたりバイト数。
const BYTES_PER_SECOND: u64 = TARGET_SAMPLE_RATE as u64 * 2;
/// 空き容量チェックで想定する最長会議時間。
const ASSUMED_MAX_HOURS: u64 = 3;

/// 録音の入力元。
///
/// デスクトップ版はこのPCのマイクを直接使い、ブラウザ版はブラウザから
/// 16kHz mono PCM を送ってもらう。どちらも同じ `WavSink` でディスクへ書くため、
/// 「録音データを絶対に失わない」性質は変わらない。
#[derive(Debug, Clone)]
pub enum RecordingSource {
    /// このPCに接続されたマイク（デスクトップ版）
    NativeMic,
    /// ブラウザから送信される音声（ブラウザ版）。値は表示用のクライアント名。
    BrowserUpload { client_label: String },
}

pub fn list_input_devices() -> AppResult<Vec<AudioInputDevice>> {
    devices::list_input_devices()
}

/// 録音を開始する。
///
/// 手順の順序が重要:
/// 1. 空き容量を確認する（途中で足りなくなるのを避ける）
/// 2. 会議フォルダを作る
/// 3. **DB に録音開始を記録する**（この直後に落ちても復旧対象として検出できる）
/// 4. 録音を開始する
pub fn start_recording(
    state: &AppState,
    meeting_id: String,
    source: RecordingSource,
) -> AppResult<RecordingSnapshot> {
    let settings = state.settings.get();

    if state.recorder.is_active() || state.upload_recorder.is_active() {
        return Err(AppError::Audio(
            "すでに別の会議を録音中です。先に会議を終了してください。".to_string(),
        ));
    }

    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;

    if meeting.status != MeetingStatus::Draft {
        return Err(AppError::Invalid(format!(
            "この会議は既に開始済みです（状態: {}）。",
            meeting.status.as_str()
        )));
    }

    check_disk_space(&settings.meetings_dir, settings.min_free_disk_mb)?;

    let folder = unique_dir(
        &settings.meetings_dir,
        &meeting_folder_name(&chrono::Local::now(), &meeting.title),
    );
    std::fs::create_dir_all(&folder).map_err(|e| {
        AppError::Io(format!(
            "会議フォルダを作成できません ({}): {e}",
            folder.display()
        ))
    })?;
    let audio_path = folder.join(MeetingFiles::AUDIO_WAV);

    state.db.with_conn(|conn| {
        repo::mark_recording_started(
            conn,
            &meeting_id,
            &folder.display().to_string(),
            &audio_path.display().to_string(),
            TARGET_SAMPLE_RATE as i64,
        )
    })?;

    let started = match &source {
        RecordingSource::NativeMic => state.recorder.start(RecorderConfig {
            meeting_id: meeting_id.clone(),
            audio_path,
            device_name: settings.input_device.clone(),
            // Phase 7 でリアルタイム文字起こしを実装する際に
            // settings.realtime_transcription_enabled をここへ渡す。
            realtime_output: false,
        }),
        RecordingSource::BrowserUpload { client_label } => {
            state
                .upload_recorder
                .start(meeting_id.clone(), &audio_path, client_label.clone())
        }
    };

    match started {
        Ok(snapshot) => Ok(snapshot),
        Err(e) => {
            // 録音を開始できなかった場合は draft に戻す。
            // 中途半端な recording 状態のまま残すと、次回起動で誤って復旧対象になる。
            if let Err(revert) = state
                .db
                .with_conn(|conn| repo::set_meeting_status(conn, &meeting_id, MeetingStatus::Draft))
            {
                tracing::error!(error = %revert, "会議状態を戻せませんでした");
            }
            Err(e)
        }
    }
}

pub fn pause_recording(state: &AppState) -> AppResult<RecordingSnapshot> {
    let snapshot = if state.upload_recorder.is_active() {
        state.upload_recorder.set_paused(true)?
    } else {
        state.recorder.pause()?
    };
    state.db.with_conn(|conn| {
        repo::set_meeting_status(conn, &snapshot.meeting_id, MeetingStatus::Paused)
    })?;
    Ok(snapshot)
}

pub fn resume_recording(state: &AppState) -> AppResult<RecordingSnapshot> {
    let snapshot = if state.upload_recorder.is_active() {
        state.upload_recorder.set_paused(false)?
    } else {
        state.recorder.resume()?
    };
    state.db.with_conn(|conn| {
        repo::set_meeting_status(conn, &snapshot.meeting_id, MeetingStatus::Recording)
    })?;
    Ok(snapshot)
}

/// 現在進行中の録音（マイク / ブラウザのどちらか）の状態。
pub fn get_recording_state(state: &AppState) -> Option<RecordingSnapshot> {
    state
        .recorder
        .snapshot()
        .or_else(|| state.upload_recorder.snapshot())
}

/// ブラウザから送られてきた 16bit PCM チャンクを書き込む。
///
/// 受け取り次第ディスクへ流すため、通信が途切れてもそこまでの音声は残る。
pub fn append_recording_chunk(
    state: &AppState,
    meeting_id: &str,
    pcm: &[u8],
) -> AppResult<RecordingSnapshot> {
    state.upload_recorder.append_pcm(meeting_id, pcm)?;
    state
        .upload_recorder
        .snapshot()
        .ok_or_else(|| AppError::Audio("録音していません。".to_string()))
}

/// 録音を終了し、音声ファイルを確定させる。
///
/// 書き込み中にエラーが起きていた場合でも、そこまでの音声はファイルに残る。
/// その事実を UI へ伝えるため、エラーは会議レコードを更新したうえで返す。
pub fn stop_recording(state: &Arc<AppState>) -> AppResult<Meeting> {
    let snapshot = get_recording_state(state)
        .ok_or_else(|| AppError::Audio("録音していません。".to_string()))?;
    let meeting_id = snapshot.meeting_id.clone();

    let stop_result = if state.upload_recorder.is_active() {
        state.upload_recorder.stop()
    } else {
        state.recorder.stop()
    };

    // 停止処理が失敗しても、DB の状態は必ず更新する。
    // recording のまま残すと、次回起動で「中断された会議」として二重に扱われてしまう。
    match stop_result {
        Ok(finalized) => {
            // 音声は確定した。ここから先（文字起こし・AI分析）が失敗しても
            // 音声ファイルは残るため、状態を Processing にして処理へ引き渡す。
            let meeting = state.db.with_conn(|conn| {
                repo::mark_recording_finished(
                    conn,
                    &meeting_id,
                    finalized.duration_ms,
                    MeetingStatus::Processing,
                )
            })?;
            crate::pipeline::spawn(state.clone(), meeting_id);
            Ok(meeting)
        }
        Err(e) => {
            // 失敗しても実ファイルは残っているので、実データから長さを測り直して記録する。
            let inspection =
                recovery::inspect(Path::new(&snapshot.audio_path), TARGET_SAMPLE_RATE, 1);
            if let Err(db_err) = state.db.with_conn(|conn| {
                repo::mark_recording_finished(
                    conn,
                    &meeting_id,
                    inspection.duration_ms,
                    MeetingStatus::Failed,
                )
            }) {
                tracing::error!(error = %db_err, "録音終了の記録に失敗しました");
            }
            Err(e)
        }
    }
}

// -------------------------------------------------------------- 復旧

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableMeeting {
    pub meeting: Meeting,
    pub inspection: WavInspection,
    /// 録音データが残っており復旧できるか。
    pub can_recover: bool,
}

/// 前回の異常終了で `recording` / `paused` のまま残っている会議を返す。
pub fn get_recoverable_meetings(state: &AppState) -> AppResult<Vec<RecoverableMeeting>> {
    let meetings = state.db.with_conn(repo::list_interrupted_meetings)?;

    // 録音中の会議は復旧対象ではない（今まさに動いている）。
    let active_id = get_recording_state(state).map(|s| s.meeting_id);

    let mut out = Vec::new();
    for meeting in meetings {
        if Some(&meeting.id) == active_id.as_ref() {
            continue;
        }
        let Some(audio_path) = meeting.audio_path.as_ref() else {
            continue;
        };
        let inspection = recovery::inspect(
            Path::new(audio_path),
            meeting.sample_rate.unwrap_or(TARGET_SAMPLE_RATE as i64) as u32,
            1,
        );
        let can_recover = matches!(
            inspection.condition,
            WavCondition::Intact | WavCondition::HeaderOutdated | WavCondition::HeaderBroken
        );
        out.push(RecoverableMeeting {
            meeting,
            inspection,
            can_recover,
        });
    }
    Ok(out)
}

/// 中断された会議の音声を復旧し、会議を終了状態にする。
pub fn recover_meeting(state: &Arc<AppState>, meeting_id: String) -> AppResult<Meeting> {
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))?;

    let audio_path = meeting
        .audio_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("録音ファイルのパスが記録されていません".to_string()))?;

    let sample_rate = meeting.sample_rate.unwrap_or(TARGET_SAMPLE_RATE as i64) as u32;
    let inspection = recovery::inspect(&audio_path, sample_rate, 1);
    let recovered_path = recovery::repair(&audio_path, &inspection)?;

    if recovered_path != audio_path {
        state.db.with_conn(|conn| {
            repo::set_audio_path(conn, &meeting_id, &recovered_path.display().to_string())
        })?;
    }

    // 復旧後に改めて検査し、実データから会議時間を確定させる。
    let confirmed = recovery::inspect(&recovered_path, sample_rate, 1);

    let updated = state.db.with_conn(|conn| {
        repo::mark_recording_finished(
            conn,
            &meeting_id,
            confirmed.duration_ms,
            MeetingStatus::Processing,
        )
    })?;

    tracing::info!(
        meeting_id = %meeting_id,
        duration_ms = confirmed.duration_ms,
        "中断された会議を復旧しました"
    );

    // 音声が確保できたので、通常の会議終了と同じ処理へ流す。
    crate::pipeline::spawn(state.clone(), meeting_id);
    Ok(updated)
}

// -------------------------------------------------------------- 空き容量

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStatus {
    pub available_bytes: u64,
    pub required_bytes: u64,
    pub sufficient: bool,
    /// この空き容量で録音できるおよその時間（分）。
    pub estimated_minutes: u64,
}

pub fn check_disk_status(state: &AppState) -> DiskStatus {
    let settings = state.settings.get();
    let available = sysutil::available_space(&settings.meetings_dir).unwrap_or(0);
    let required = settings.min_free_disk_mb * 1024 * 1024;
    DiskStatus {
        available_bytes: available,
        required_bytes: required,
        sufficient: available >= required,
        estimated_minutes: available / BYTES_PER_SECOND / 60,
    }
}

fn check_disk_space(dir: &Path, min_free_mb: u64) -> AppResult<()> {
    let Some(available) = sysutil::available_space(dir) else {
        // 取得できない環境でも録音は始められるようにする（記録だけ残す）。
        tracing::warn!(dir = %dir.display(), "空き容量を確認できませんでした");
        return Ok(());
    };

    let required = min_free_mb * 1024 * 1024;
    if available < required {
        let assumed = BYTES_PER_SECOND * 3600 * ASSUMED_MAX_HOURS;
        return Err(AppError::DiskSpace(format!(
            "保存先の空き容量は {} MB ですが、{} MB 以上が必要です。\
             {}時間の会議でおよそ {} MB を使用します。不要なファイルを削除するか、\
             設定で保存先を変更してください。",
            available / 1024 / 1024,
            min_free_mb,
            ASSUMED_MAX_HOURS,
            assumed / 1024 / 1024,
        )));
    }
    Ok(())
}
