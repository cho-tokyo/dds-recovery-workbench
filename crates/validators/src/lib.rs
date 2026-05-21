//! # dds-validators
//!
//! Chunk 18: ファイル形式バリデータ基盤。
//!
//! 復旧されたファイルが本当に「開けるか」を検証するための軽量品質判定エンジン。
//! 各形式ごとに `Validator` trait を実装し、マジックナンバー + 基本構造チェックで
//! 3 値ステータス（`Valid` / `Invalid` / `Uncertain`）を返す。
//!
//! ## 設計原則: 保守的 (conservative) 判定
//!
//! 「Valid」を返すなら**ほぼ確実に開ける**ことを保証する。曖昧な場合は
//! `Uncertain` を返し、誤って Valid 判定して CS の信頼を失うリスクを避ける。
//!
//! ## サポート形式（Phase 1）
//!
//! - **PNG**: signature + IHDR + IEND チェック (Chunk 18)
//! - **JPEG**: SOI + EOI + マーカープレフィックス (Chunk 18)
//! - **PDF**: `%PDF-1.X` ヘッダ (X=0-7) + 末尾 `%%EOF` (Chunk 18)
//! - **GIF**: `GIF87a` / `GIF89a` signature + trailer `0x3B` (Chunk 19)
//! - **BMP**: `BM` signature + ヘッダのファイルサイズ整合性 (Chunk 19)
//! - **ZIP**: `PK\x03\x04` / `PK\x05\x06` + EOCD 検出 (Chunk 19)
//! - **DOCX / XLSX / PPTX**: ZIP + `[Content_Types].xml` + format marker (Chunk 19)
//!
//! ## 使い方
//!
//! ```
//! use dds_validators::ValidatorRegistry;
//!
//! let registry = ValidatorRegistry::with_defaults();
//! let bytes = std::fs::read("recovered/photo.jpg").unwrap_or_default();
//! let result = registry.validate(&bytes, Some("jpg"));
//! println!("{}", result.summary());
//! ```
//!
//! ## 依存関係
//!
//! このクレートは他クレートに依存しない（`thiserror` + `serde` のみ）。
//! 上位の `dds-recovery` から呼び出される単方向依存設計。
//!
//! 関連 FR: FR-REC-04 (データ整合性), FR-QUAL-01 (品質判定)。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod error;
pub mod formats;
pub mod registry;
pub mod result;

pub use error::ValidatorError;
pub use registry::{Validator, ValidatorRegistry};
pub use result::{ValidationResult, ValidationStatus};
