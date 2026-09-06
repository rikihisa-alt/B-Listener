//! Whisper モデルの管理とダウンロード。
//!
//! 利用者に「モデル」を意識させないため、必要なときにアプリ側から取得する。
//! 外部通信はこのモジュールとローカルLLMへの接続だけに限られる。
//! 会議データが外部へ出ることは一切ない。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// ggml 形式モデルの配布元（whisper.cpp 公式リポジトリ）。
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// ggml ファイルの先頭に入るマジックナンバー ("lmgg" のリトルエンディアン)。
/// HTML のエラーページなどを掴んでいないことを確認するために使う。
const GGML_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

/// ダウンロードの読み取り単位。
const DOWNLOAD_CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    /// 設定に保存する識別子。
    pub id: &'static str,
    /// 画面に出す説明。
    pub label: &'static str,
    /// おおよそのファイルサイズ（MB）。ダウンロード前の案内に使う。
    pub approx_size_mb: u64,
    /// リアルタイム用途に適するか。
    pub suitable_for_realtime: bool,
}

/// 選択できるモデルの一覧。ハードコードを 1 箇所に集約する。
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny",
        label: "tiny — 最速・精度は低い",
        approx_size_mb: 75,
        suitable_for_realtime: true,
    },
    ModelSpec {
        id: "base",
        label: "base — 速い",
        approx_size_mb: 142,
        suitable_for_realtime: true,
    },
    ModelSpec {
        id: "small",
        label: "small — バランス型（リアルタイム向け）",
        approx_size_mb: 466,
        suitable_for_realtime: true,
    },
    ModelSpec {
        id: "medium",
        label: "medium — 高精度・やや遅い",
        approx_size_mb: 1500,
        suitable_for_realtime: false,
    },
    ModelSpec {
        id: "large-v3-turbo",
        label: "large-v3-turbo — 高精度で比較的速い（推奨）",
        approx_size_mb: 1600,
        suitable_for_realtime: false,
    },
    ModelSpec {
        id: "large-v3",
        label: "large-v3 — 最高精度・最も遅い",
        approx_size_mb: 3100,
        suitable_for_realtime: false,
    },
];

pub fn find_spec(model_id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == model_id)
}

/// モデルファイルの配置先。
pub fn model_path(models_dir: &Path, model_id: &str) -> PathBuf {
    models_dir.join(format!("ggml-{model_id}.bin"))
}

fn model_url(model_id: &str) -> String {
    format!("{BASE_URL}/ggml-{model_id}.bin")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub label: String,
    pub approx_size_mb: u64,
    pub suitable_for_realtime: bool,
    pub installed: bool,
    pub path: String,
    /// 実際のファイルサイズ（未導入なら 0）。
    pub size_bytes: u64,
}

pub fn list_models(models_dir: &Path) -> Vec<ModelStatus> {
    MODELS
        .iter()
        .map(|spec| {
            let path = model_path(models_dir, spec.id);
            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            ModelStatus {
                id: spec.id.to_string(),
                label: spec.label.to_string(),
                approx_size_mb: spec.approx_size_mb,
                suitable_for_realtime: spec.suitable_for_realtime,
                installed: size_bytes > 0 && is_ggml(&path),
                path: path.display().to_string(),
                size_bytes,
            }
        })
        .collect()
}

/// モデルが使用可能な状態かを確認し、パスを返す。
pub fn ensure_available(models_dir: &Path, model_id: &str) -> AppResult<PathBuf> {
    let path = model_path(models_dir, model_id);
    if !path.is_file() {
        return Err(AppError::MissingComponent(format!(
            "音声認識モデル「{model_id}」がインストールされていません。\
             設定画面からダウンロードしてください。"
        )));
    }
    if !is_ggml(&path) {
        return Err(AppError::MissingComponent(format!(
            "音声認識モデル「{model_id}」のファイルが壊れています。\
             設定画面から再ダウンロードしてください。"
        )));
    }
    Ok(path)
}

/// ggml モデルファイルらしいかをマジックナンバーで判定する。
fn is_ggml(path: &Path) -> bool {
    let mut head = [0u8; 4];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)) {
        Ok(()) => head == GGML_MAGIC,
        Err(_) => false,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: u8,
}

/// モデルをダウンロードする。
///
/// 途中でクラッシュしても壊れたファイルを残さないよう、`.part` へ書いてから置き換える。
/// 完了時に「サイズが Content-Length と一致するか」と「ggml のマジックナンバー」を検証する。
pub fn download(
    models_dir: &Path,
    model_id: &str,
    on_progress: &dyn Fn(DownloadProgress),
    should_cancel: &dyn Fn() -> bool,
) -> AppResult<PathBuf> {
    let spec = find_spec(model_id)
        .ok_or_else(|| AppError::Invalid(format!("未知の音声認識モデルです: {model_id}")))?;

    std::fs::create_dir_all(models_dir)
        .map_err(|e| AppError::Io(format!("モデル保存先を作成できません: {e}")))?;

    // 空き容量を先に確認する。途中で失敗すると数百MBを無駄にするため。
    let required = spec.approx_size_mb * 1024 * 1024 + 200 * 1024 * 1024;
    if let Some(available) = crate::sysutil::available_space(models_dir) {
        if available < required {
            return Err(AppError::DiskSpace(format!(
                "モデルの保存に約 {} MB が必要ですが、空き容量は {} MB です。",
                spec.approx_size_mb,
                available / 1024 / 1024
            )));
        }
    }

    let final_path = model_path(models_dir, model_id);
    let part_path = final_path.with_extension("bin.part");
    let url = model_url(model_id);

    tracing::info!(model_id, url = %url, "モデルのダウンロードを開始します");

    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(60 * 60))
        .call()
        .map_err(|e| {
            AppError::Io(format!(
                "モデルをダウンロードできません: {e}。\
                 インターネット接続を確認してください。"
            ))
        })?;

    let total_bytes: u64 = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&part_path)
        .map_err(|e| AppError::Io(format!("モデルファイルを作成できません: {e}")))?;

    let mut buf = vec![0u8; DOWNLOAD_CHUNK];
    let mut downloaded: u64 = 0;
    let mut last_reported_percent = u8::MAX;

    loop {
        if should_cancel() {
            drop(file);
            std::fs::remove_file(&part_path).ok();
            return Err(AppError::Other(
                "モデルのダウンロードを中止しました。".to_string(),
            ));
        }

        let n = reader
            .read(&mut buf)
            .map_err(|e| AppError::Io(format!("モデルの受信に失敗しました: {e}")))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| AppError::Io(format!("モデルを書き込めません: {e}")))?;
        downloaded += n as u64;

        let percent = downloaded
            .checked_mul(100)
            .and_then(|v| v.checked_div(total_bytes))
            .map(|v| v.min(100) as u8)
            .unwrap_or(0);
        if percent != last_reported_percent {
            last_reported_percent = percent;
            on_progress(DownloadProgress {
                model_id: model_id.to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                percent,
            });
        }
    }

    file.sync_all()
        .map_err(|e| AppError::Io(format!("モデルを保存できません: {e}")))?;
    drop(file);

    // 検証: 途中で切れていないか、そもそもモデルファイルなのか。
    if total_bytes > 0 && downloaded != total_bytes {
        std::fs::remove_file(&part_path).ok();
        return Err(AppError::Io(format!(
            "モデルのダウンロードが途中で終了しました（{downloaded} / {total_bytes} バイト）。\
             もう一度お試しください。"
        )));
    }
    if !is_ggml(&part_path) {
        std::fs::remove_file(&part_path).ok();
        return Err(AppError::Io(
            "ダウンロードしたファイルが音声認識モデルではありません。\
             もう一度お試しください。"
                .to_string(),
        ));
    }

    std::fs::rename(&part_path, &final_path)
        .map_err(|e| AppError::Io(format!("モデルを配置できません: {e}")))?;

    tracing::info!(
        model_id,
        bytes = downloaded,
        "モデルのダウンロードが完了しました"
    );
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_unique_id() {
        let mut ids: Vec<&str> = MODELS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn default_models_are_known() {
        assert!(find_spec(crate::settings::DEFAULT_WHISPER_MODEL).is_some());
        assert!(find_spec(crate::settings::DEFAULT_WHISPER_REALTIME_MODEL).is_some());
    }

    #[test]
    fn missing_model_reports_missing_component() {
        let dir = std::env::temp_dir().join(format!("blistener-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = ensure_available(&dir, "small").unwrap_err();
        assert_eq!(err.code(), "MISSING_COMPONENT");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_file_without_ggml_magic() {
        let dir = std::env::temp_dir().join(format!("blistener-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(model_path(&dir, "tiny"), b"<html>error</html>").unwrap();
        let err = ensure_available(&dir, "tiny").unwrap_err();
        assert_eq!(err.code(), "MISSING_COMPONENT");
        std::fs::remove_dir_all(&dir).ok();
    }
}
