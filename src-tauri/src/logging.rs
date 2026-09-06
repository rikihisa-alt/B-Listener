//! ロギング初期化。
//!
//! 重要: 会議の本文（文字起こし・議事録）はログに出力しない。
//! 出力するのは会議ID・処理名・件数・所要時間・エラー内容のみとする。

use std::path::Path;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// ログを標準出力とファイル (`<app_data>/logs/b-listener.log`) の両方へ出す。
/// 戻り値のガードはアプリ終了まで保持する必要がある。
pub fn init(log_dir: &Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!("ログディレクトリを作成できませんでした: {e}");
    }

    // whisper.cpp / GGML は Metal カーネルの対応状況を大量に WARN で出す。
    // 実害がなくログが読めなくなるため、既定では error のみに絞る。
    // 詳細を見たいときは B_LISTENER_LOG=whisper_rs=debug などで上書きできる。
    let filter = EnvFilter::try_from_env("B_LISTENER_LOG")
        .unwrap_or_else(|_| EnvFilter::new("b_listener_lib=info,whisper_rs=error,warn"));

    let appender = tracing_appender::rolling::daily(log_dir, "b-listener.log");
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let result = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_ansi(true))
        .with(
            fmt::layer()
                .with_target(true)
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .try_init();

    if let Err(e) = result {
        eprintln!("ロガーの初期化に失敗しました: {e}");
        return None;
    }

    Some(guard)
}
