//! デバイスのサンプルレートから 16kHz mono への変換。
//!
//! whisper.cpp の入力要件が 16kHz mono のため、録音の時点で合わせておく。
//! こうすることで会議終了後に変換工程が不要になり、失敗しうる処理段が 1 つ減る。
//!
//! 変換はオーディオコールバックではなく書き込みスレッドで行う。
//! コールバックは OS のリアルタイムスレッドであり、そこで重い処理をすると
//! 音が途切れる（＝録音データが欠ける）ため。

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::error::{AppError, AppResult};

/// whisper.cpp が要求するサンプルレート。
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// 一度にリサンプラへ渡す入力フレーム数。
const CHUNK_FRAMES: usize = 1024;

/// mono f32 を任意のレートから 16kHz へ変換する。
///
/// 入力チャンク長が可変でも扱えるよう、内部で固定長にバッファリングする。
pub struct Resampler16k {
    inner: Option<SincFixedIn<f32>>,
    /// 未処理の入力サンプル。
    pending: Vec<f32>,
    source_rate: u32,
}

impl Resampler16k {
    pub fn new(source_rate: u32) -> AppResult<Self> {
        if source_rate == 0 {
            return Err(AppError::Audio(
                "マイクのサンプルレートを取得できませんでした。".to_string(),
            ));
        }

        // 既に 16kHz ならリサンプル不要。そのまま通す。
        if source_rate == TARGET_SAMPLE_RATE {
            return Ok(Self {
                inner: None,
                pending: Vec::new(),
                source_rate,
            });
        }

        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };

        let ratio = TARGET_SAMPLE_RATE as f64 / source_rate as f64;
        let inner = SincFixedIn::<f32>::new(ratio, 1.0, params, CHUNK_FRAMES, 1)
            .map_err(|e| AppError::Audio(format!("リサンプラを初期化できません: {e}")))?;

        tracing::info!(
            source_rate,
            target = TARGET_SAMPLE_RATE,
            "リサンプラを構成しました"
        );
        Ok(Self {
            inner: Some(inner),
            pending: Vec::with_capacity(CHUNK_FRAMES * 2),
            source_rate,
        })
    }

    pub fn source_rate(&self) -> u32 {
        self.source_rate
    }

    /// 入力サンプルを与え、変換済みの 16kHz サンプルを `out` へ追記する。
    ///
    /// 固定長に満たない端数は内部に保持され、次回の呼び出しで処理される。
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> AppResult<()> {
        let Some(resampler) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return Ok(());
        };

        self.pending.extend_from_slice(input);

        while self.pending.len() >= CHUNK_FRAMES {
            let chunk: Vec<f32> = self.pending.drain(..CHUNK_FRAMES).collect();
            let converted = resampler
                .process(&[chunk], None)
                .map_err(|e| AppError::Audio(format!("リサンプルに失敗しました: {e}")))?;
            if let Some(channel) = converted.first() {
                out.extend_from_slice(channel);
            }
        }
        Ok(())
    }

    /// 録音終了時に、内部に残った端数を 0 埋めして吐き出す。
    ///
    /// 端数は最大でも 1024 サンプル（48kHz で約 21ms）なので、
    /// 末尾に無音が数十ミリ秒付く程度の影響しかない。
    pub fn flush(&mut self, out: &mut Vec<f32>) -> AppResult<()> {
        let Some(resampler) = self.inner.as_mut() else {
            out.append(&mut self.pending);
            return Ok(());
        };
        if self.pending.is_empty() {
            return Ok(());
        }

        let mut chunk = std::mem::take(&mut self.pending);
        chunk.resize(CHUNK_FRAMES, 0.0);
        let converted = resampler
            .process(&[chunk], None)
            .map_err(|e| AppError::Audio(format!("リサンプルに失敗しました: {e}")))?;
        if let Some(channel) = converted.first() {
            out.extend_from_slice(channel);
        }
        Ok(())
    }
}

/// インターリーブされた多チャンネル音声を mono へダウンミックスする。
///
/// この処理はオーディオコールバック内で行うため、確保もロックも行わない。
pub fn downmix_to_mono(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let inv = 1.0 / channels as f32;
    for frame in interleaved.chunks_exact(channels) {
        out.push(frame.iter().sum::<f32>() * inv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_already_16k() {
        let mut r = Resampler16k::new(16_000).unwrap();
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3], &mut out).unwrap();
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn downsamples_48k_to_roughly_one_third() {
        let mut r = Resampler16k::new(48_000).unwrap();
        // 48000 サンプル = 1 秒
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let mut out = Vec::new();
        r.process(&input, &mut out).unwrap();
        r.flush(&mut out).unwrap();

        // 1 秒ぶんなので 16000 サンプル前後になる（フィルタ遅延ぶんの誤差を許容）
        assert!(
            (out.len() as i64 - 16_000).abs() < 500,
            "期待値から離れすぎています: {}",
            out.len()
        );
    }

    #[test]
    fn handles_variable_input_lengths() {
        let mut r = Resampler16k::new(44_100).unwrap();
        let mut out = Vec::new();
        for len in [37usize, 512, 1, 2048, 999] {
            r.process(&vec![0.0; len], &mut out).unwrap();
        }
        r.flush(&mut out).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }
}
