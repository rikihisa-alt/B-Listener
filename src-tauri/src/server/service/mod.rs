//! コア処理の共有レイヤ。
//!
//! デスクトップ版（Tauri コマンド）とブラウザ版（HTTP ハンドラ）の両方が、
//! この層の関数だけを呼ぶ。ここには Tauri にも axum にも依存する型を持ち込まない。

pub mod files;
pub mod meeting;
pub mod pipeline;
pub mod recording;
pub mod settings;
pub mod stt;
pub mod system;

pub use files::*;
pub use meeting::*;
pub use pipeline::*;
pub use recording::*;
pub use settings::*;
pub use stt::*;
pub use system::*;
