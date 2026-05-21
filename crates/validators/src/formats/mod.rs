//! Chunk 18: 各ファイル形式の Validator 実装。
//!
//! Phase 1: PNG / JPEG / PDF。
//! 将来 Chunk 19+ で GIF / BMP / DOCX / XLSX 等を追加予定。

pub mod jpeg;
pub mod pdf;
pub mod png;
