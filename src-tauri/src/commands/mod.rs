//! Tauri command 層。
//!
//! ここは薄いアダプタに徹する。実装は `crate::server::service` にあり、
//! ブラウザ版の HTTP ハンドラと同じ関数を呼ぶ。
//! こうすることで、2 つの UI で挙動が食い違わないようにする。

pub mod files;
pub mod meeting;
pub mod pipeline;
pub mod recording;
pub mod settings;
pub mod stt;
pub mod system;
