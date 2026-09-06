//! whisper.cpp（whisper-rs）による文字起こし。
//!
//! アプリに内蔵しているため、利用者が別途 whisper.cpp を導入する必要はない。
//! macOS では Metal による GPU 実行が有効になる。

use std::path::Path;
use std::sync::Once;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::{AppError, AppResult};

use super::{Accuracy, ProgressFn, Segment, TranscribeOptions, Transcriber};

pub struct WhisperTranscriber {
    context: WhisperContext,
    model_id: String,
}

/// whisper.cpp / GGML のログを標準出力ではなく tracing へ流す。
///
/// 既定では大量のデコーダログが標準エラーへ出るため、必ず最初に呼ぶ。
/// アプリのログ設定（`B_LISTENER_LOG`）で表示レベルを制御できるようになる。
fn install_log_hooks_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        whisper_rs::install_logging_hooks();
        tracing::debug!(
            version = whisper_rs::WHISPER_CPP_VERSION,
            "whisper.cpp を初期化しました"
        );
    });
}

impl WhisperTranscriber {
    /// モデルを読み込む。数百MB〜数GBを読むため時間がかかる。
    pub fn load(model_path: &Path, model_id: &str) -> AppResult<Self> {
        install_log_hooks_once();

        let path = model_path.to_str().ok_or_else(|| {
            AppError::Stt(format!(
                "モデルのパスに扱えない文字が含まれています: {}",
                model_path.display()
            ))
        })?;

        let started = std::time::Instant::now();
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| {
                AppError::Stt(format!(
                    "音声認識モデル「{model_id}」を読み込めません: {e}。\
                     設定画面から再ダウンロードしてください。"
                ))
            })?;

        tracing::info!(
            model_id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "音声認識モデルを読み込みました"
        );
        Ok(Self {
            context,
            model_id: model_id.to_string(),
        })
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
        on_progress: ProgressFn,
    ) -> AppResult<Vec<Segment>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| AppError::Stt(format!("音声認識を準備できません: {e}")))?;

        let strategy = match options.accuracy {
            // 精度優先。ビームサーチで候補を広げる。
            Accuracy::Final => SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            },
            // 速度優先。会議中の確認用なので誤変換は許容する。
            Accuracy::Realtime => SamplingStrategy::Greedy { best_of: 1 },
        };
        let mut params = FullParams::new(strategy);

        params.set_n_threads(options.n_threads);
        params.set_translate(false);
        params.set_language(options.language.as_deref());

        // whisper.cpp 自身の標準出力は使わない（ログはアプリ側で統一する）。
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // 無音や雑音で同じ文を繰り返す既知の挙動を抑える設定。
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        params.set_entropy_thold(2.4);
        params.set_logprob_thold(-1.0);

        match options.accuracy {
            Accuracy::Final => {
                // 温度 0 から始め、失敗時のみ段階的に上げる（whisper.cpp の既定の挙動）。
                params.set_temperature(0.0);
                params.set_temperature_inc(0.2);
            }
            Accuracy::Realtime => {
                params.set_temperature(0.0);
                params.set_temperature_inc(0.0);
                params.set_single_segment(false);
            }
        }

        if !options.initial_prompt.is_empty() {
            params.set_initial_prompt(&options.initial_prompt);
        }

        params.set_progress_callback_safe(move |percent: i32| {
            on_progress(percent.clamp(0, 100) as u8);
        });

        state
            .full(params, samples)
            .map_err(|e| AppError::Stt(format!("文字起こしに失敗しました: {e}")))?;

        let count = state
            .full_n_segments()
            .map_err(|e| AppError::Stt(format!("文字起こし結果を取得できません: {e}")))?;

        let mut out = Vec::with_capacity(count.max(0) as usize);
        for i in 0..count {
            // 個別セグメントの取得失敗で全体を捨てない。
            // 1 行読めなくても、他の発言は議事録として価値があるため。
            let text = match state.full_get_segment_text_lossy(i) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(segment = i, error = %e, "セグメントを取得できませんでした");
                    continue;
                }
            };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            // whisper のタイムスタンプは 10ms 単位。
            let start_ms = state.full_get_segment_t0(i).unwrap_or(0) * 10;
            let end_ms = state.full_get_segment_t1(i).unwrap_or(0) * 10;
            out.push(Segment {
                start_ms,
                end_ms,
                text,
            });
        }

        Ok(out)
    }

    fn engine_name(&self) -> String {
        format!("whisper.cpp ({})", self.model_id)
    }
}
