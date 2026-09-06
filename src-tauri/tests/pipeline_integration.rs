//! 会議終了後パイプラインの統合テスト。
//!
//! 実際の音声ファイルと音声認識モデルを使い、
//! 「録音済み音声 → 文字起こし → 保存」までが通ることを確認する。
//!
//! ```sh
//! cargo test --test pipeline_integration -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use b_listener_lib::db::models::{JobState, MeetingStatus, TranscriptKind};
use b_listener_lib::db::repo;
use b_listener_lib::events::NullSink;
use b_listener_lib::state::{AppPaths, AppState};

/// テスト用の共有状態を組み立てる。
///
/// デスクトップ版と同じ `AppState::bootstrap` を使うため、
/// 起動処理そのものもここで検証されることになる。
fn build_state(app_data_dir: PathBuf, meetings_dir: PathBuf, models_dir: PathBuf) -> Arc<AppState> {
    AppState::bootstrap(
        AppPaths {
            log_dir: app_data_dir.join("logs"),
            models_dir,
            app_data_dir,
            default_meetings_dir: meetings_dir,
        },
        Arc::new(NullSink),
    )
    .expect("共有状態を組み立てられること")
}

fn models_dir() -> PathBuf {
    std::env::var("BL_TEST_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME が必要です");
            PathBuf::from(home).join("Library/Application Support/jp.blistener.app/models")
        })
}

fn source_audio() -> PathBuf {
    std::env::var("BL_TEST_AUDIO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/bl_speech.wav"))
}

/// 音声 → 文字起こし → 保存 の一連が通り、成果物が会議フォルダに揃うこと。
#[test]
#[ignore = "音声認識モデルと音声ファイルが必要"]
fn processes_recorded_meeting_end_to_end() {
    let models_dir = models_dir();
    let audio_src = source_audio();
    assert!(
        models_dir.join("ggml-small.bin").is_file(),
        "モデルがありません: {}",
        models_dir.display()
    );
    assert!(
        audio_src.is_file(),
        "音声がありません: {}",
        audio_src.display()
    );

    let root = std::env::temp_dir().join(format!("blistener-pipe-{}", uuid::Uuid::new_v4()));
    let app_data_dir = root.join("appdata");
    let meetings_dir = root.join("Meetings");
    std::fs::create_dir_all(&app_data_dir).unwrap();
    std::fs::create_dir_all(&meetings_dir).unwrap();

    let state = build_state(
        app_data_dir.clone(),
        meetings_dir.clone(),
        models_dir.clone(),
    );
    let mut s = state.settings.get();
    s.whisper_model = "small".to_string();
    state.settings.update(s).expect("設定を保存できること");
    let db = &state.db;

    // 録音済みの会議を再現する
    let folder = meetings_dir.join("2026-09-06_統合テスト会議");
    std::fs::create_dir_all(&folder).unwrap();
    let audio_path = folder.join("audio.wav");
    std::fs::copy(&audio_src, &audio_path).unwrap();

    let meeting = db
        .with_conn(|conn| repo::create_meeting(conn, Some("統合テスト会議".to_string())))
        .unwrap();
    let meeting_id = meeting.id.clone();
    db.with_conn(|conn| {
        repo::mark_recording_started(
            conn,
            &meeting_id,
            &folder.display().to_string(),
            &audio_path.display().to_string(),
            16_000,
        )
    })
    .unwrap();
    db.with_conn(|conn| {
        repo::mark_recording_finished(conn, &meeting_id, 0, MeetingStatus::Processing)
    })
    .unwrap();

    // 事前情報（Phase 6 で UI から入力する内容）を入れておく
    db.with_tx(|tx| {
        repo::save_meeting_context(
            tx,
            &meeting_id,
            &b_listener_lib::db::models::MeetingContextInput {
                participants: Some(vec!["吉田".into(), "生田".into()]),
                ..Default::default()
            },
        )
    })
    .unwrap();

    let started = std::time::Instant::now();
    b_listener_lib::pipeline::run_blocking(&state, &meeting_id).expect("処理が完走すること");
    println!("処理時間: {:?}", started.elapsed());

    // 1. 全ステップが完了していること
    let jobs = state
        .db
        .with_conn(|conn| repo::list_job_runs(conn, &meeting_id))
        .unwrap();
    assert!(!jobs.is_empty(), "ステップが記録されていません");
    for job in &jobs {
        assert_eq!(
            job.state,
            JobState::Done,
            "ステップ {} が完了していません: {:?}",
            job.label,
            job.error
        );
    }

    // 2. 最終文字起こしが DB に入っていること
    let segments = state
        .db
        .with_conn(|conn| repo::list_transcript_segments(conn, &meeting_id, TranscriptKind::Final))
        .unwrap();
    assert!(!segments.is_empty(), "文字起こしが保存されていません");
    let joined: String = segments.iter().map(|s| s.text.as_str()).collect();
    println!("文字起こし: {joined}");
    assert!(
        joined.contains("シフト"),
        "想定の語が含まれません: {joined}"
    );

    // 3. 会議フォルダに成果物が揃っていること
    for name in [
        "audio.wav",
        "transcript.txt",
        "transcript.raw.txt",
        "metadata.json",
    ] {
        assert!(
            folder.join(name).is_file(),
            "{name} が出力されていません ({})",
            folder.display()
        );
    }
    let transcript = std::fs::read_to_string(folder.join("transcript.txt")).unwrap();
    assert!(
        transcript.starts_with("[00:00:00]"),
        "タイムスタンプ形式が想定と違います"
    );

    // 4. 会議情報が更新されていること
    let updated = state
        .db
        .with_conn(|conn| repo::get_meeting(conn, &meeting_id))
        .unwrap();
    assert!(
        updated.transcript_path.is_some(),
        "文字起こしのパスが未設定です"
    );
    assert!(
        updated.duration_ms > 10_000,
        "音声の長さが確定していません: {}",
        updated.duration_ms
    );

    // 5. metadata.json が会議フォルダ単体で内容を復元できる形になっていること
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("metadata.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["meeting"]["title"], "統合テスト会議");
    assert_eq!(metadata["whisperModel"], "small");
    assert_eq!(metadata["participants"][0]["name"], "吉田");

    std::fs::remove_dir_all(&root).ok();
}

/// モデルが無い場合でも音声は失われず、失敗として記録されること。
#[test]
#[ignore = "音声ファイルが必要"]
fn keeps_audio_when_transcription_fails() {
    let audio_src = source_audio();
    assert!(
        audio_src.is_file(),
        "音声がありません: {}",
        audio_src.display()
    );

    let root = std::env::temp_dir().join(format!("blistener-pipe-{}", uuid::Uuid::new_v4()));
    let app_data_dir = root.join("appdata");
    let meetings_dir = root.join("Meetings");
    // モデルが 1 つも無いディレクトリを指す
    let empty_models = root.join("no-models");
    std::fs::create_dir_all(&app_data_dir).unwrap();
    std::fs::create_dir_all(&meetings_dir).unwrap();
    std::fs::create_dir_all(&empty_models).unwrap();

    let state = build_state(
        app_data_dir.clone(),
        meetings_dir.clone(),
        empty_models.clone(),
    );
    let db = &state.db;

    let folder = meetings_dir.join("2026-09-06_モデルなし");
    std::fs::create_dir_all(&folder).unwrap();
    let audio_path = folder.join("audio.wav");
    std::fs::copy(&audio_src, &audio_path).unwrap();

    let meeting = db
        .with_conn(|conn| repo::create_meeting(conn, Some("モデルなし".to_string())))
        .unwrap();
    let meeting_id = meeting.id.clone();
    db.with_conn(|conn| {
        repo::mark_recording_started(
            conn,
            &meeting_id,
            &folder.display().to_string(),
            &audio_path.display().to_string(),
            16_000,
        )
    })
    .unwrap();

    let err = b_listener_lib::pipeline::run_blocking(&state, &meeting_id)
        .expect_err("モデルが無いので失敗すること");
    println!("期待どおり失敗: {err}");
    assert_eq!(err.code(), "MISSING_COMPONENT");

    // 最重要: 音声ファイルは残っていること
    assert!(audio_path.is_file(), "音声ファイルが失われています");
    assert!(
        std::fs::metadata(&audio_path).unwrap().len() > 400_000,
        "音声ファイルが欠損しています"
    );

    // 音声の確定ステップは成功し、文字起こしステップだけが失敗していること
    let jobs = state
        .db
        .with_conn(|conn| repo::list_job_runs(conn, &meeting_id))
        .unwrap();
    let finalize = jobs
        .iter()
        .find(|j| j.step == b_listener_lib::db::models::PipelineStep::FinalizeAudio)
        .expect("音声確定ステップが記録されていること");
    assert_eq!(finalize.state, JobState::Done);
    let transcribe = jobs
        .iter()
        .find(|j| j.step == b_listener_lib::db::models::PipelineStep::Transcribe)
        .expect("文字起こしステップが記録されていること");
    assert_eq!(transcribe.state, JobState::Failed);
    assert!(transcribe.error.is_some(), "失敗理由が記録されていません");

    std::fs::remove_dir_all(&root).ok();
}
