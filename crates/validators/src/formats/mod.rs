//! Chunk 18+19: 各ファイル形式の Validator 実装。
//!
//! - Chunk 18: PNG / JPEG / PDF。
//! - Chunk 19: GIF / BMP / ZIP + OOXML 3 形式 (DOCX / XLSX / PPTX)。

pub mod bmp;
pub mod gif;
pub mod jpeg;
pub mod ooxml;
pub mod pdf;
pub mod png;
pub mod zip;
