//! 会議終了後の自動処理オーケストレータ。
//!
//! # 設計方針
//! - 各ステップの成否を `job_run` に記録し、失敗しても完了済みステップはやり直さない
//! - **どのステップで失敗しても音声ファイルは必ず残る**
//! - 文字起こしが失敗しても音声は残り、AI が失敗しても文字起こしは残る
//!
//! リアルタイム文字起こしの結果はここでは一切使わない。
//! 必ず保存済みの音声ファイル全体から作り直す（仕様書 10 章）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::audio::recovery;
use crate::audio::resample::TARGET_SAMPLE_RATE;
use crate::db::models::{JobState, MeetingStatus, PipelineStep, TranscriptKind};
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage;
use crate::stt::{self, whisper::WhisperTranscriber, TranscribeOptions};

/// UI へ送る進捗。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineProgress {
    pub meeting_id: String,
    pub step: PipelineStep,
    pub label: String,
    pub step_index: usize,
    pub step_total: usize,
    /// このステップ内の進捗（0〜100）。
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineFailure {
    pub meeting_id: String,
    pub step: PipelineStep,
    pub label: String,
    pub code: String,
    pub message: String,
    /// 失敗しても残っているもの（利用者への安心材料として明示する）。
    pub preserved: Vec<String>,
}

/// 現在このバージョンで実行するステップ。Phase 4 以降で増える。
const STEPS: &[PipelineStep] = &[
    PipelineStep::FinalizeAudio,
    PipelineStep::Transcribe,
    PipelineStep::Persist,
];

/// 会議終了後の処理をバックグラウンドで開始する。
///
/// 同じ会議に対して二重に起動しない。UI をブロックしないよう別スレッドで動かす。
pub fn spawn<R: Runtime>(app: AppHandle<R>, meeting_id: String) {
    {
        let state = app.state::<AppState>();
        let mut running = match state.running_pipelines.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !running.insert(meeting_id.clone()) {
            tracing::info!(meeting_id = %meeting_id, "この会議の処理は既に実行中です");
            return;
        }
    }

    let spawn_result = std::thread::Builder::new()
        .name("b-listener-pipeline".into())
        .spawn({
            let app = app.clone();
            let meeting_id = meeting_id.clone();
            move || {
                let result = run_blocking(&app, &meeting_id);
                finish(&app, &meeting_id, result);
            }
        });

    if let Err(e) = spawn_result {
        tracing::error!(error = %e, "処理スレッドを起動できませんでした");
        let state = app.state::<AppState>();
        if let Ok(mut running) = state.running_pipelines.lock() {
            running.remove(&meeting_id);
        };
    }
}

fn finish<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, result: AppResult<()>) {
    let state = app.state::<AppState>();
    if let Ok(mut running) = state.running_pipelines.lock() {
        running.remove(meeting_id);
    }

    match result {
        Ok(()) => {
            if let Err(e) = state.db.with_conn(|conn| {
                repo::set_meeting_status(conn, meeting_id, MeetingStatus::Completed)
            }) {
                tracing::error!(error = %e, "会議状態を更新できませんでした");
            }
            tracing::info!(meeting_id = %meeting_id, "会議終了後の処理が完了しました");
            let _ = app.emit("pipeline:done", meeting_id.to_string());
        }
        Err(error) => {
            // 失敗しても音声は必ず残る。状態を failed にして UI から再実行できるようにする。
            if let Err(e) = state
                .db
                .with_conn(|conn| repo::set_meeting_status(conn, meeting_id, MeetingStatus::Failed))
            {
                tracing::error!(error = %e, "会議状態を更新できませんでした");
            }
            tracing::error!(meeting_id = %meeting_id, error = %error, "会議終了後の処理が失敗しました");

            let failure = build_failure(app, meeting_id, &error);
            let _ = app.emit("pipeline:failed", failure);
        }
    }
}

/// 失敗時に「何が残っているか」を調べて伝える。
fn build_failure<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    error: &AppError,
) -> PipelineFailure {
    let state = app.state::<AppState>();
    let mut preserved = Vec::new();
    let mut failed_step = PipelineStep::Transcribe;

    if let Ok(meeting) = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, meeting_id))
    {
        if meeting
            .audio_path
            .as_ref()
            .map(|p| Path::new(p).is_file())
            .unwrap_or(false)
        {
            preserved.push("音声ファイル".to_string());
        }
        if meeting
            .transcript_path
            .as_ref()
            .map(|p| Path::new(p).is_file())
            .unwrap_or(false)
        {
            preserved.push("文字起こし".to_string());
        }
    }
    if let Ok(jobs) = state
        .db
        .with_conn(|conn| repo::list_job_runs(conn, meeting_id))
    {
        if let Some(job) = jobs.iter().find(|j| j.state == JobState::Failed) {
            failed_step = job.step;
        }
    }

    PipelineFailure {
        meeting_id: meeting_id.to_string(),
        step: failed_step,
        label: failed_step.label().to_string(),
        code: error.code().to_string(),
        message: error.to_string(),
        preserved,
    }
}

/// 会議終了後の処理を同期実行する。
///
/// 通常は [`spawn`] から別スレッドで呼ばれる。統合テストからも直接呼べるよう公開している。
pub fn run_blocking<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    state
        .db
        .with_conn(|conn| repo::set_meeting_status(conn, meeting_id, MeetingStatus::Processing))?;

    for (index, step) in STEPS.iter().enumerate() {
        let step = *step;

        // 完了済みのステップはやり直さない（再実行時に無駄な処理をしない）。
        let already_done = state
            .db
            .with_conn(|conn| repo::list_job_runs(conn, meeting_id))?
            .into_iter()
            .any(|j| j.step == step && j.state == JobState::Done);
        if already_done {
            tracing::info!(
                meeting_id,
                step = step.as_str(),
                "完了済みのためスキップします"
            );
            continue;
        }

        state.db.with_conn(|conn| {
            repo::set_job_state(conn, meeting_id, step, JobState::Running, None)
        })?;
        emit_progress(app, meeting_id, step, index, 0, step.label());

        let outcome = match step {
            PipelineStep::FinalizeAudio => finalize_audio(app, meeting_id),
            PipelineStep::Transcribe => transcribe(app, meeting_id, index),
            PipelineStep::Persist => persist(app, meeting_id),
            // Phase 4 以降で実装するステップ。現時点では STEPS に含めていない。
            other => Err(AppError::Other(format!(
                "ステップ {} はまだ実装されていません",
                other.label()
            ))),
        };

        match outcome {
            Ok(()) => {
                state.db.with_conn(|conn| {
                    repo::set_job_state(conn, meeting_id, step, JobState::Done, None)
                })?;
                emit_progress(app, meeting_id, step, index, 100, step.label());
            }
            Err(e) => {
                let message = e.to_string();
                if let Err(db_err) = state.db.with_conn(|conn| {
                    repo::set_job_state(conn, meeting_id, step, JobState::Failed, Some(&message))
                }) {
                    tracing::error!(error = %db_err, "処理状態を記録できませんでした");
                }
                return Err(e);
            }
        }
    }

    Ok(())
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    step: PipelineStep,
    index: usize,
    percent: u8,
    message: &str,
) {
    let payload = PipelineProgress {
        meeting_id: meeting_id.to_string(),
        step,
        label: step.label().to_string(),
        step_index: index,
        step_total: STEPS.len(),
        percent,
        message: message.to_string(),
    };
    if let Err(e) = app.emit("pipeline:progress", payload) {
        tracing::warn!(error = %e, "進捗の通知に失敗しました");
    }
}

// ------------------------------------------------------------ 各ステップ

/// 音声ファイルを確定させる。
///
/// このステップが通れば「音声だけは必ずある」状態が保証される。
/// 以降のステップが全て失敗しても、利用者は録音を手に入れられる。
fn finalize_audio<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, meeting_id))?;

    let audio_path = meeting
        .audio_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("録音ファイルのパスが記録されていません".to_string()))?;

    let sample_rate = meeting.sample_rate.unwrap_or(TARGET_SAMPLE_RATE as i64) as u32;
    let inspection = recovery::inspect(&audio_path, sample_rate, 1);
    let confirmed_path = recovery::repair(&audio_path, &inspection)?;

    if confirmed_path != audio_path {
        state.db.with_conn(|conn| {
            repo::set_audio_path(conn, meeting_id, &confirmed_path.display().to_string())
        })?;
    }

    // 実データから会議時間を確定させる（録音停止時の値より確実）。
    let confirmed = recovery::inspect(&confirmed_path, sample_rate, 1);
    if confirmed.duration_ms > 0 && confirmed.duration_ms != meeting.duration_ms {
        state.db.with_conn(|conn| {
            repo::mark_recording_finished(
                conn,
                meeting_id,
                confirmed.duration_ms,
                MeetingStatus::Processing,
            )
        })?;
    }

    tracing::info!(
        meeting_id,
        duration_ms = confirmed.duration_ms,
        "音声ファイルを確定しました"
    );
    Ok(())
}

/// 保存済みの音声全体から、精度優先の設定で文字起こしを作り直す。
fn transcribe<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    step_index: usize,
) -> AppResult<()> {
    let state = app.state::<AppState>();
    let settings = state.settings.get();

    let meeting = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, meeting_id))?;
    let audio_path = meeting
        .audio_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("録音ファイルが見つかりません".to_string()))?;
    let folder = meeting
        .folder_path
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| audio_path.parent().map(Path::to_path_buf))
        .ok_or_else(|| AppError::NotFound("会議フォルダが見つかりません".to_string()))?;

    // 事前入力の固有名詞を whisper のヒントとして使う（未入力でも動作する）。
    let terms = state
        .db
        .with_conn(|conn| repo::list_all_term_strings(conn, meeting_id))?;
    let options = TranscribeOptions::final_japanese(stt::build_initial_prompt(&terms));

    let model_path =
        stt::models::ensure_available(&state.paths.models_dir, &settings.whisper_model)?;
    let mut transcriber = WhisperTranscriber::load(&model_path, &settings.whisper_model)?;

    let progress: stt::FileProgressFn = {
        let app = app.clone();
        let meeting_id = meeting_id.to_string();
        Arc::new(move |percent, done_ms, total_ms| {
            emit_progress(
                &app,
                &meeting_id,
                PipelineStep::Transcribe,
                step_index,
                percent,
                &format!(
                    "文字起こし {} / {}",
                    crate::stt::format_timestamp(done_ms),
                    crate::stt::format_timestamp(total_ms)
                ),
            );
        })
    };

    let segments = stt::transcribe_file(&mut transcriber, &audio_path, &options, progress)?;

    if segments.is_empty() {
        return Err(AppError::Stt(
            "音声から発話を検出できませんでした。マイクが音を拾えていたか確認してください。\
             録音ファイルはそのまま保存されています。"
                .to_string(),
        ));
    }

    // 補正前を先に保存する。補正（Phase 6）で意図しない置換が起きても原文に戻せる。
    storage::write_raw_transcript(&folder, &segments)?;
    let transcript_path = storage::write_transcript(&folder, &segments)?;

    state.db.with_tx(|tx| {
        repo::replace_transcript_segments(tx, meeting_id, TranscriptKind::Final, &segments)
    })?;
    state.db.with_conn(|conn| {
        repo::set_transcript_path(conn, meeting_id, &transcript_path.display().to_string())
    })?;

    tracing::info!(
        meeting_id,
        segments = segments.len(),
        "文字起こしを保存しました"
    );
    Ok(())
}

/// 会議フォルダ内の `metadata.json` を更新する。
fn persist<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let settings = state.settings.get();

    let detail = state
        .db
        .with_conn(|conn| repo::get_meeting_detail(conn, meeting_id))?;

    let folder = detail
        .meeting
        .folder_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::NotFound("会議フォルダが見つかりません".to_string()))?;

    let metadata = storage::metadata::build(
        detail.meeting,
        detail.participants,
        detail.agendas,
        detail.terms,
        Some(settings.whisper_model.clone()),
        None,
    );
    storage::metadata::write(&folder, &metadata)?;
    Ok(())
}
