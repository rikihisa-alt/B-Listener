//! Tauri command 層。
//!
//! ここは薄いアダプタに徹する。引数の検証と Core 呼び出し、DTO 変換のみを行い、
//! ビジネスロジックは持たない。

pub mod files;
pub mod meeting;
pub mod pipeline;
pub mod recording;
pub mod settings;
pub mod stt;
pub mod system;
