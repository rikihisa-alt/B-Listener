//! ブラウザ版（HTTP サーバ）と、両UIが共有するコア処理レイヤ。
//!
//! `service` はデスクトップ版（Tauri コマンド）からも呼ばれる。
//! `http` / `sink` はブラウザ版でのみ使う（feature = "server"）。

pub mod service;

#[cfg(feature = "server")]
pub mod http;
#[cfg(feature = "server")]
pub mod sink;
