//! 音声の取り込みと保存。
//!
//! このモジュールは文字起こし (`crate::stt`) や AI 処理 (`crate::llm`) に依存しない。
//! 後段の処理がすべて失敗しても、録音だけは必ず成立させるための分離である。

pub mod devices;
pub mod recorder;
pub mod recovery;
pub mod resample;
pub mod wav_reader;
pub mod wav_sink;
