//! whisper.cpp による文字起こしの統合テスト。
//!
//! モデルファイル（数百MB）が必要なため `#[ignore]` を付けている。
//!
//! ```sh
//! BL_TEST_MODEL=~/Library/Application\ Support/jp.blistener.app/models/ggml-small.bin \
//! BL_TEST_AUDIO=/tmp/bl_speech.wav \
//! cargo test --test stt_integration -- --ignored --nocapture
//! ```
//!
//! 検証音声は macOS の `say` コマンドで作れる:
//! ```sh
//! say -v Kyoko -o /tmp/bl_speech.aiff "……"
//! afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/bl_speech.aiff /tmp/bl_speech.wav
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use b_listener_lib::stt::{self, whisper::WhisperTranscriber, TranscribeOptions};

fn model_path() -> PathBuf {
    std::env::var("BL_TEST_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME が必要です");
            PathBuf::from(home)
                .join("Library/Application Support/jp.blistener.app/models/ggml-small.bin")
        })
}

fn audio_path() -> PathBuf {
    std::env::var("BL_TEST_AUDIO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/bl_speech.wav"))
}

#[test]
#[ignore = "音声認識モデルが必要"]
fn transcribes_japanese_meeting_audio() {
    let model = model_path();
    let audio = audio_path();
    assert!(model.is_file(), "モデルがありません: {}", model.display());
    assert!(audio.is_file(), "音声がありません: {}", audio.display());

    let mut transcriber =
        WhisperTranscriber::load(&model, "small").expect("モデルを読み込めること");

    // 事前入力の固有名詞をヒントとして渡す（Phase 6 で UI から入力できるようになる）
    let terms = vec![
        "吉田".to_string(),
        "訪問介護".to_string(),
        "訪問看護".to_string(),
    ];
    let options = TranscribeOptions::final_japanese(stt::build_initial_prompt(&terms));
    println!("initial_prompt: {}", options.initial_prompt);

    let last_percent = Arc::new(AtomicU8::new(0));
    let progress: stt::FileProgressFn = {
        let last_percent = last_percent.clone();
        Arc::new(move |percent, done_ms, total_ms| {
            last_percent.store(percent, Ordering::Relaxed);
            println!(
                "  進捗 {percent}% ({} / {})",
                stt::format_timestamp(done_ms),
                stt::format_timestamp(total_ms)
            );
        })
    };

    let started = std::time::Instant::now();
    let segments = stt::transcribe_file(&mut transcriber, &audio, &options, progress)
        .expect("文字起こしできること");
    let elapsed = started.elapsed();

    println!(
        "--- 文字起こし結果 ({} 件, {:?}) ---",
        segments.len(),
        elapsed
    );
    for s in &segments {
        println!("[{}] {}", stt::format_timestamp(s.start_ms), s.text);
    }

    assert!(!segments.is_empty(), "セグメントが 1 件も得られていません");
    assert_eq!(
        last_percent.load(Ordering::Relaxed),
        100,
        "進捗が 100% になっていません"
    );

    let joined: String = segments.iter().map(|s| s.text.as_str()).collect();
    // 会議で実際に発話された固有名詞が拾えていること
    for expected in ["シフト", "吉田", "火曜"] {
        assert!(
            joined.contains(expected),
            "「{expected}」が文字起こしに含まれていません:\n{joined}"
        );
    }

    // タイムスタンプが単調増加であること（議事録の並び順の前提）
    for pair in segments.windows(2) {
        assert!(
            pair[1].start_ms >= pair[0].start_ms,
            "タイムスタンプが逆転しています: {} -> {}",
            pair[0].start_ms,
            pair[1].start_ms
        );
    }
}

/// モデルが無い場合に「必要なコンポーネントがありません」と伝わること。
#[test]
fn reports_missing_model_clearly() {
    let dir = std::env::temp_dir().join("blistener-no-models-xyz");
    std::fs::create_dir_all(&dir).unwrap();
    let err = b_listener_lib::stt::models::ensure_available(&dir, "small").unwrap_err();
    assert_eq!(err.code(), "MISSING_COMPONENT");
    assert!(err.to_string().contains("設定画面"));
    std::fs::remove_dir_all(&dir).ok();
}
