//! OS 依存のユーティリティ（ディスク空き容量・スリープ抑止）。
//!
//! 長時間の録音を守るために必要な最小限の機能だけを扱う。

use std::path::Path;

/// 指定パスが属するボリュームの空き容量（バイト）。取得できない場合は `None`。
pub fn available_space(path: &Path) -> Option<u64> {
    // 保存先がまだ存在しない場合に備え、存在する親までさかのぼる。
    let mut target = path;
    loop {
        if target.exists() {
            break;
        }
        target = target.parent()?;
    }
    platform_available_space(target)
}

#[cfg(unix)]
fn platform_available_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: c_path は有効な NUL 終端文字列。stat は呼び出し前にゼロ初期化する。
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let block = if stat.f_frsize > 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        Some(stat.f_bavail as u64 * block)
    }
}

#[cfg(windows)]
fn platform_available_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_for_caller: u64 = 0;
    // SAFETY: wide は NUL 終端された有効なパス。出力ポインタはローカル変数を指す。
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_for_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        None
    } else {
        Some(free_for_caller)
    }
}

/// 録音中にPCがスリープしないよう抑止する。
///
/// `Drop` で自動的に解除されるため、録音セッションと同じ寿命で保持する。
pub struct SleepBlocker {
    #[cfg(target_os = "macos")]
    child: Option<std::process::Child>,
    #[cfg(not(target_os = "macos"))]
    _private: (),
}

impl SleepBlocker {
    /// スリープ抑止を開始する。失敗しても録音は継続するため、エラーは記録のみ行う。
    pub fn activate() -> Self {
        #[cfg(target_os = "macos")]
        {
            // macOS 標準の caffeinate を使う。外部依存を増やさずに済む。
            // -i: アイドルによるシステムスリープを抑止する
            let child = std::process::Command::new("/usr/bin/caffeinate")
                .arg("-i")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            match child {
                Ok(child) => {
                    tracing::info!("スリープ抑止を開始しました");
                    Self { child: Some(child) }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "スリープ抑止を開始できませんでした");
                    Self { child: None }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::Power::{
                SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
            };
            // 注意: この設定は呼び出したスレッドに紐づく。
            // 録音制御スレッドから呼び出し、同じスレッドで解除すること。
            // SAFETY: 引数はフラグのみで、ポインタを扱わない。
            unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
            tracing::info!("スリープ抑止を開始しました");
            Self { _private: () }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Self { _private: () }
        }
    }
}

impl Drop for SleepBlocker {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if let Some(child) = self.child.as_mut() {
                if let Err(e) = child.kill() {
                    tracing::warn!(error = %e, "スリープ抑止プロセスを終了できませんでした");
                }
                let _ = child.wait();
                tracing::info!("スリープ抑止を解除しました");
            }
        }

        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
            // SAFETY: 引数はフラグのみ。
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            tracing::info!("スリープ抑止を解除しました");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_space_for_temp_dir() {
        let space = available_space(&std::env::temp_dir());
        assert!(space.is_some());
        assert!(space.unwrap() > 0);
    }

    #[test]
    fn walks_up_to_existing_parent() {
        let missing = std::env::temp_dir().join("blistener-does-not-exist-xyz/deeper");
        assert!(available_space(&missing).is_some());
    }
}
