//! 実機のマイクを使った録音の統合テスト。
//!
//! CI や マイクの無い環境では失敗するため `#[ignore]` を付けている。
//! 実行するには:
//!
//! ```sh
//! cargo test --test recording_integration -- --ignored --nocapture
//! ```
//!
//! macOS ではこのテストを実行するプロセス（ターミナル等）にマイク権限が必要。

use std::time::Duration;

use b_listener_lib::audio::recorder::{Recorder, RecorderConfig, RecorderState};
use b_listener_lib::audio::recovery::{self, WavCondition};
use b_listener_lib::audio::resample::TARGET_SAMPLE_RATE;

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("blistener-it-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// 開始 → 一時停止 → 再開 → 終了 の一連が成立し、
/// 一時停止中の時間が録音長に含まれないことを確認する。
#[test]
#[ignore = "実機のマイクが必要"]
fn records_pauses_resumes_and_finalizes() {
    let path = temp_path("audio.wav");
    let recorder = Recorder::new();

    let started = recorder
        .start(RecorderConfig {
            meeting_id: "it-meeting".to_string(),
            audio_path: path.clone(),
            device_name: None,
            realtime_output: false,
        })
        .expect("録音を開始できること");
    assert_eq!(started.state, RecorderState::Recording);
    println!("使用デバイス: {}", started.device_name);

    std::thread::sleep(Duration::from_secs(2));
    let while_recording = recorder.snapshot().expect("録音中の状態が取れること");
    assert!(
        while_recording.elapsed_ms >= 1500,
        "2秒待った時点で録音長が伸びているはず: {}ms",
        while_recording.elapsed_ms
    );

    recorder.pause().expect("一時停止できること");
    let paused_at = recorder.snapshot().unwrap().elapsed_ms;
    std::thread::sleep(Duration::from_secs(2));
    let after_pause = recorder.snapshot().unwrap();
    assert_eq!(after_pause.state, RecorderState::Paused);
    assert!(
        after_pause.elapsed_ms - paused_at < 300,
        "一時停止中に録音長が伸びている: {} -> {}",
        paused_at,
        after_pause.elapsed_ms
    );

    recorder.resume().expect("再開できること");
    std::thread::sleep(Duration::from_secs(2));

    let finalized = recorder.stop().expect("録音を確定できること");
    println!(
        "録音結果: {} ({} bytes / {} ms)",
        finalized.path.display(),
        finalized.data_bytes,
        finalized.duration_ms
    );

    // 実録音は約4秒（2秒 + 2秒）。一時停止の2秒は含まれない。
    assert!(
        (3_000..5_500).contains(&finalized.duration_ms),
        "録音長が想定外です: {}ms",
        finalized.duration_ms
    );
    assert_eq!(finalized.sample_rate, TARGET_SAMPLE_RATE);

    // 再生可能な WAV として確定していること。
    let inspection = recovery::inspect(&path, TARGET_SAMPLE_RATE, 1);
    assert_eq!(inspection.condition, WavCondition::Intact);
    assert_eq!(inspection.actual_data_bytes, finalized.data_bytes);

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// 二重に録音を開始できないこと（音声ファイルが二重に開かれないことの保証）。
#[test]
#[ignore = "実機のマイクが必要"]
fn rejects_second_concurrent_recording() {
    let recorder = Recorder::new();
    let first = temp_path("first.wav");
    let second = temp_path("second.wav");

    recorder
        .start(RecorderConfig {
            meeting_id: "a".into(),
            audio_path: first.clone(),
            device_name: None,
            realtime_output: false,
        })
        .expect("1件目は開始できる");

    let err = recorder
        .start(RecorderConfig {
            meeting_id: "b".into(),
            audio_path: second.clone(),
            device_name: None,
            realtime_output: false,
        })
        .expect_err("2件目は拒否される");
    println!("期待どおり拒否: {err}");

    recorder.stop().expect("停止できること");
    assert!(!second.exists(), "2件目のファイルは作られない");

    std::fs::remove_dir_all(first.parent().unwrap()).ok();
    std::fs::remove_dir_all(second.parent().unwrap()).ok();
}
