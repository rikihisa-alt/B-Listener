//! 音声認識（Speech To Text）。
//!
//! このモジュールは AI 処理 (`crate::llm`) に依存しない。
//! 文字起こしが失敗しても音声は残り、AI が失敗しても文字起こしは残る、という
//! 独立性を保つための分離である。

pub mod models;
pub mod whisper;

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::audio::resample::TARGET_SAMPLE_RATE;
use crate::audio::wav_reader::WavReader;
use crate::error::{AppError, AppResult};

/// 文字起こしの 1 セグメント。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// 精度重視か速度重視か。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accuracy {
    /// 会議終了後の最終文字起こし。時間がかかっても精度を優先する。
    Final,
    /// 会議中のリアルタイム文字起こし。多少の誤変換は許容する。
    Realtime,
}

#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// 言語コード。`None` なら自動判定。
    pub language: Option<String>,
    /// 固有名詞などの手がかり（事前入力から生成する）。
    pub initial_prompt: String,
    pub accuracy: Accuracy,
    pub n_threads: i32,
}

impl TranscribeOptions {
    pub fn final_japanese(initial_prompt: String) -> Self {
        Self {
            language: Some("ja".to_string()),
            initial_prompt,
            accuracy: Accuracy::Final,
            n_threads: default_thread_count(),
        }
    }
}

/// 物理コア数に近い値を使う。全論理コアを使うと UI が固まりやすいため 1 つ残す。
pub fn default_thread_count() -> i32 {
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (available.saturating_sub(1)).clamp(1, 8) as i32
}

/// 進捗コールバック。`percent` は 0〜100。
pub type ProgressFn = Arc<dyn Fn(u8) + Send + Sync>;

/// 音声認識エンジンの抽象。実装を差し替えられるようにしておく。
pub trait Transcriber: Send {
    /// 16kHz mono の f32 サンプル列を文字起こしする。
    fn transcribe(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
        on_progress: ProgressFn,
    ) -> AppResult<Vec<Segment>>;

    /// ログ表示用の名前。
    fn engine_name(&self) -> String;
}

// ---------------------------------------------------------------- 長時間音声

/// 1 チャンクの目安（秒）。10 分ぶんで約 38MB の f32 バッファになる。
const CHUNK_SECONDS: usize = 600;
/// 分割位置を探す範囲（秒）。この幅の中でいちばん静かな場所を切れ目にする。
const SPLIT_SEARCH_SECONDS: usize = 15;
/// 静けさを測る窓（ミリ秒）。
const SILENCE_WINDOW_MS: usize = 200;

/// 音声ファイル全体を文字起こしする。
///
/// 3 時間の会議は f32 換算で約 690MB になるため、一括では読み込まない。
/// 10 分程度のチャンクに分けて処理し、**発話の切れ目（最も静かな位置）で分割する**ことで
/// 文の途中で切れて情報が落ちるのを防ぐ。
///
/// `on_progress` には (全体の進捗率, 処理済みミリ秒, 総ミリ秒) を渡す。
pub type FileProgressFn = Arc<dyn Fn(u8, i64, i64) + Send + Sync>;

pub fn transcribe_file(
    transcriber: &mut dyn Transcriber,
    audio_path: &Path,
    options: &TranscribeOptions,
    on_progress: FileProgressFn,
) -> AppResult<Vec<Segment>> {
    let mut reader = WavReader::open(audio_path)?;

    if reader.sample_rate != TARGET_SAMPLE_RATE {
        return Err(AppError::Stt(format!(
            "この音声は {}Hz です。{}Hz の音声のみ文字起こしできます。",
            reader.sample_rate, TARGET_SAMPLE_RATE
        )));
    }
    if reader.total_frames == 0 {
        return Err(AppError::Stt(
            "音声データが記録されていないため文字起こしできません。".to_string(),
        ));
    }

    let total_ms = reader.duration_ms();
    let rate = reader.sample_rate as usize;
    let chunk_frames = CHUNK_SECONDS * rate;
    let search_frames = SPLIT_SEARCH_SECONDS * rate;
    let read_frames = chunk_frames + search_frames;

    tracing::info!(
        engine = %transcriber.engine_name(),
        path = %audio_path.display(),
        duration_ms = total_ms,
        "文字起こしを開始します"
    );
    let started = std::time::Instant::now();

    let mut segments: Vec<Segment> = Vec::new();
    let mut buffer: Vec<f32> = Vec::with_capacity(read_frames + rate);
    let mut chunk_start_frame: u64 = 0;
    let mut eof = false;

    while !eof {
        // チャンク 1 つぶん + 探索範囲まで読み込む
        while buffer.len() < read_frames {
            let got = reader.read_mono_f32(read_frames - buffer.len(), &mut buffer)?;
            if got == 0 {
                eof = true;
                break;
            }
        }
        if buffer.is_empty() {
            break;
        }

        let split_at = if eof {
            buffer.len()
        } else {
            find_quietest_split(&buffer, chunk_frames, search_frames, rate)
        };

        let chunk: Vec<f32> = buffer.drain(..split_at).collect();
        let chunk_offset_ms = (chunk_start_frame as i64 * 1000) / rate as i64;
        let chunk_ms = (chunk.len() as i64 * 1000) / rate as i64;

        // チャンク内の進捗を会議全体の進捗へ写す。
        // whisper-rs のコールバックは 'static を要求するため、
        // 借用ではなく Arc を複製して持ち込む。
        let progress: ProgressFn = {
            let outer = on_progress.clone();
            let base_ms = chunk_offset_ms;
            let chunk_len = chunk_ms.max(1);
            let total = total_ms.max(1);
            Arc::new(move |p: u8| {
                let done = (base_ms + (chunk_len * p as i64) / 100).min(total);
                outer(((done * 100) / total) as u8, done, total);
            })
        };

        let mut chunk_segments = transcriber.transcribe(&chunk, options, progress)?;
        for s in &mut chunk_segments {
            s.start_ms += chunk_offset_ms;
            s.end_ms += chunk_offset_ms;
        }
        segments.append(&mut chunk_segments);

        chunk_start_frame += split_at as u64;
        let done_ms = (chunk_start_frame as i64 * 1000) / rate as i64;
        on_progress(
            ((done_ms.min(total_ms) * 100) / total_ms.max(1)) as u8,
            done_ms.min(total_ms),
            total_ms,
        );
    }

    let segments = drop_repeated_segments(segments);

    tracing::info!(
        segments = segments.len(),
        duration_ms = total_ms,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "文字起こしが完了しました"
    );
    Ok(segments)
}

/// `target` 付近で最も静かな位置を探して分割点にする。
/// 文の途中で切ると前後の文脈が失われるため、無音に近い場所で切る。
fn find_quietest_split(buffer: &[f32], target: usize, search: usize, sample_rate: usize) -> usize {
    let window = (SILENCE_WINDOW_MS * sample_rate) / 1000;
    let lo = target.saturating_sub(search);
    let hi = (target + search).min(buffer.len().saturating_sub(window));
    if window == 0 || lo >= hi {
        return target.min(buffer.len());
    }

    let step = (window / 4).max(1);
    let mut best_pos = target.min(buffer.len());
    let mut best_energy = f32::MAX;

    let mut pos = lo;
    while pos < hi {
        let energy: f32 = buffer[pos..pos + window].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best_pos = pos + window / 2;
        }
        pos += step;
    }
    best_pos.min(buffer.len())
}

/// whisper が無音区間で同じ文を繰り返し出力することがあるため、連続する重複を落とす。
fn drop_repeated_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(last) = out.last() {
            if last.text.trim() == text {
                continue;
            }
        }
        out.push(Segment {
            start_ms: seg.start_ms,
            end_ms: seg.end_ms,
            text: text.to_string(),
        });
    }
    out
}

/// セグメント列をタイムスタンプ付きテキストへ整形する。
pub fn segments_to_text(segments: &[Segment]) -> String {
    let mut out = String::new();
    for s in segments {
        out.push_str(&format!(
            "[{}] {}\n",
            crate::stt::format_timestamp(s.start_ms),
            s.text
        ));
    }
    out
}

pub fn format_timestamp(ms: i64) -> String {
    let total = (ms / 1000).max(0);
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// 事前入力の固有名詞から whisper の initial_prompt を組み立てる。
///
/// whisper の prompt は長すぎると効果が落ちるため、上限を設ける。
pub fn build_initial_prompt(terms: &[String]) -> String {
    const MAX_CHARS: usize = 200;

    let mut unique: Vec<&str> = Vec::new();
    for t in terms {
        let t = t.trim();
        if t.is_empty() || unique.contains(&t) {
            continue;
        }
        unique.push(t);
    }
    if unique.is_empty() {
        return String::new();
    }

    let mut prompt = String::from("以下の固有名詞が登場します: ");
    for (i, term) in unique.iter().enumerate() {
        if prompt.chars().count() + term.chars().count() + 2 > MAX_CHARS {
            break;
        }
        if i > 0 {
            prompt.push('、');
        }
        prompt.push_str(term);
    }
    prompt.push('。');
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_timestamps() {
        assert_eq!(format_timestamp(0), "00:00:00");
        assert_eq!(format_timestamp(3_723_000), "01:02:03");
    }

    #[test]
    fn removes_consecutive_duplicates() {
        let input = vec![
            Segment {
                start_ms: 0,
                end_ms: 1000,
                text: "はい".into(),
            },
            Segment {
                start_ms: 1000,
                end_ms: 2000,
                text: " はい ".into(),
            },
            Segment {
                start_ms: 2000,
                end_ms: 3000,
                text: "".into(),
            },
            Segment {
                start_ms: 3000,
                end_ms: 4000,
                text: "次の議題です".into(),
            },
            Segment {
                start_ms: 4000,
                end_ms: 5000,
                text: "はい".into(),
            },
        ];
        let out = drop_repeated_segments(input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].text, "はい");
        assert_eq!(out[1].text, "次の議題です");
        assert_eq!(out[2].text, "はい");
    }

    #[test]
    fn builds_prompt_from_terms() {
        let prompt = build_initial_prompt(&[
            "吉田".to_string(),
            "国保連".to_string(),
            "吉田".to_string(),
            "  ".to_string(),
        ]);
        assert!(prompt.contains("吉田"));
        assert!(prompt.contains("国保連"));
        // 重複は 1 度だけ
        assert_eq!(prompt.matches("吉田").count(), 1);
    }

    #[test]
    fn empty_terms_produce_empty_prompt() {
        assert_eq!(build_initial_prompt(&[]), "");
    }

    #[test]
    fn splits_at_the_quietest_point() {
        let rate = 16_000usize;
        // 大きい音 → 無音 → 大きい音 という並びを作る
        let mut buf = vec![0.5f32; rate * 10];
        let silence_start = rate * 5;
        for s in buf[silence_start..silence_start + rate / 2].iter_mut() {
            *s = 0.0;
        }
        // target=6秒付近、探索±2秒 → 5.0〜5.5秒の無音が選ばれるはず
        let split = find_quietest_split(&buf, rate * 6, rate * 2, rate);
        assert!(
            split >= silence_start && split <= silence_start + rate / 2,
            "無音区間で分割されていません: {split}"
        );
    }
}
