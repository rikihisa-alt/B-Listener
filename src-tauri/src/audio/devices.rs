//! 入力デバイスの列挙と選択。

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub name: String,
    /// OS の既定デバイスかどうか。
    pub is_default: bool,
}

/// 利用可能な入力デバイスを列挙する。
pub fn list_input_devices() -> AppResult<Vec<AudioInputDevice>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());

    let devices = host
        .input_devices()
        .map_err(|e| AppError::Audio(format!("マイクの一覧を取得できません: {e}")))?;

    let mut out = Vec::new();
    for device in devices {
        match device.name() {
            Ok(name) => {
                let is_default = Some(&name) == default_name.as_ref();
                out.push(AudioInputDevice { name, is_default });
            }
            Err(e) => {
                // 1 台読めなくても他は使えるので、記録だけして続行する。
                tracing::warn!(error = %e, "マイク名を取得できませんでした");
            }
        }
    }
    Ok(out)
}

/// 設定で指定された名前のデバイス、なければ既定デバイスを返す。
///
/// 指定デバイスが見つからない場合は既定へフォールバックし、警告を記録する。
/// 「指定が古いせいで録音できない」状態を作らないため。
pub fn resolve_input_device(preferred: Option<&str>) -> AppResult<cpal::Device> {
    let host = cpal::default_host();

    if let Some(name) = preferred.filter(|n| !n.trim().is_empty()) {
        match host.input_devices() {
            Ok(devices) => {
                for device in devices {
                    if device.name().ok().as_deref() == Some(name) {
                        return Ok(device);
                    }
                }
                tracing::warn!(
                    device = name,
                    "指定のマイクが見つからないため既定のマイクを使用します"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "マイクの一覧を取得できないため既定のマイクを使用します");
            }
        }
    }

    host.default_input_device().ok_or_else(|| {
        AppError::Audio(
            "マイクが見つかりません。マイクが接続されているか、OSのプライバシー設定で\
             このアプリにマイクの使用が許可されているかを確認してください。"
                .to_string(),
        )
    })
}
