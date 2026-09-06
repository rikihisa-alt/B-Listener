//! B-Listener ブラウザ版のサーバ。
//!
//! 社内のPC/サーバでこれを起動し、他のPCのブラウザから使う。
//! デスクトップ版とまったく同じコア処理・同じ保存先を使うため、
//! 同じPCで両方を使っても会議データは 1 か所にまとまる。
//!
//! ```sh
//! cargo run --features server --bin b-listener-server
//! ```
//!
//! 環境変数:
//! - `B_LISTENER_HOST` 待ち受けアドレス（既定 `0.0.0.0`）
//! - `B_LISTENER_PORT` 待ち受けポート（既定 `8787`）
//! - `B_LISTENER_WEB_ROOT` フロントエンドの `dist` の場所
//!
//! **重要**: 会議データを外部へ出さないため、インターネットへ直接公開しないこと。

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use b_listener_lib::events::SharedEventSink;
use b_listener_lib::server::http::{serve, ServerContext};
use b_listener_lib::server::sink::BroadcastSink;
use b_listener_lib::state::{AppPaths, AppState};
use b_listener_lib::{logging, paths, spawn_recording_ticker};

/// 既定の待ち受けポート。
const DEFAULT_PORT: u16 = 8787;
/// SSE のバッファ。0.5秒間隔の tick に対して十分な余裕を取る。
const EVENT_BUFFER: usize = 256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (app_data_dir, default_meetings_dir) = paths::default_app_paths()
        .ok_or("ホームディレクトリを特定できませんでした。HOME 環境変数を確認してください。")?;

    let app_paths = AppPaths {
        log_dir: app_data_dir.join("logs"),
        models_dir: app_data_dir.join("models"),
        app_data_dir,
        default_meetings_dir,
    };

    // ログのガードは main が終わるまで保持する。
    let _log_guard = logging::init(&app_paths.log_dir);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        "B-Listener（ブラウザ版サーバ）を起動します"
    );

    let sink = Arc::new(BroadcastSink::new(EVENT_BUFFER));
    let events: SharedEventSink = sink.clone();
    let state = AppState::bootstrap(app_paths, events)?;

    spawn_recording_ticker(state.clone());

    let web_root = resolve_web_root()?;
    tracing::info!(web_root = %web_root.display(), "フロントエンドの配信元");

    let addr = resolve_address()?;
    serve(
        ServerContext {
            state,
            sink,
            web_root,
        },
        addr,
    )
    .await?;

    tracing::info!("サーバを終了しました");
    Ok(())
}

fn resolve_address() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    // 既定で 0.0.0.0 にするのは、社内の他PCのブラウザから使うため。
    let host: IpAddr = std::env::var("B_LISTENER_HOST")
        .unwrap_or_else(|_| "0.0.0.0".to_string())
        .parse()?;
    let port: u16 = match std::env::var("B_LISTENER_PORT") {
        Ok(v) => v.parse()?,
        Err(_) => DEFAULT_PORT,
    };
    Ok(SocketAddr::new(host, port))
}

/// フロントエンドのビルド成果物 (`dist`) を探す。
fn resolve_web_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(explicit) = std::env::var_os("B_LISTENER_WEB_ROOT") {
        let path = PathBuf::from(explicit);
        if path.join("index.html").is_file() {
            return Ok(path);
        }
        return Err(format!(
            "B_LISTENER_WEB_ROOT に index.html がありません: {}",
            path.display()
        )
        .into());
    }

    let mut candidates = vec![PathBuf::from("dist"), PathBuf::from("../dist")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("dist"));
            candidates.push(dir.join("../dist"));
        }
    }

    for candidate in candidates {
        if candidate.join("index.html").is_file() {
            return Ok(candidate);
        }
    }

    Err("フロントエンドのビルド結果 (dist) が見つかりません。\
         先に `npm run build` を実行するか、B_LISTENER_WEB_ROOT を指定してください。"
        .into())
}
