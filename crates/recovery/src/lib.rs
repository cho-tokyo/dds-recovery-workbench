//! # dds-recovery
//!
//! Chunk 17: DDS 復旧パイプライン基盤。
//!
//! `dds-wish-match` がマッチさせた結果を、実ファイルとして出力ディレクトリに
//! 書き出す end-to-end のパイプラインを提供する。Chunks 4-14 で構築した NTFS
//! リーダ + Chunk 15-16 で構築した wish-match の上に乗る業務統合層の最終段。
//!
//! ## read / write 境界
//!
//! このクレートは初めて `std::fs::write` / `std::fs::create_dir_all` を使う
//! 書き込み API 利用クレートだが、書き込み先は `RecoveryEngine::output_dir`
//! 配下のみ。ソースディスク（`NtfsVolume`）への書き込みは絶対禁止のまま、
//! fs-ntfs / wish-match / disk-io / core / fs-common の書き込み API 0 件原則を維持。
//!
//! ## 使い方
//!
//! ```no_run
//! use dds_recovery::{RecoveryEngine, RecoveryOptions};
//! use dds_wish_match::{Wishlist, Wish, WishItem, Priority};
//! # use dds_fs_ntfs::NtfsVolume;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let mut volume: NtfsVolume<Box<dyn FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>> =
//! #     unimplemented!();
//!
//! let wishlist = Wishlist::new().add(
//!     Wish::new(WishItem::Extension("docx".into()), "全ての Word 文書")
//!         .with_priority(Priority::High),
//! );
//!
//! let engine = RecoveryEngine::new("./recovered_files");
//! let report = engine.recover_files(&mut volume, &wishlist)?;
//! println!("Recovered: {}", report.recovered.len());
//! # Ok(())
//! # }
//! ```
//!
//! 関連 FR: FR-REC-01 (目標優先抽出), FR-REC-02 (出力先指定),
//! FR-REC-03 (衝突解決), FR-REC-04 (データ整合性)。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod engine;
pub mod error;
pub mod options;
pub mod report;
pub mod sanitize;

pub use engine::{RecoveryConfig, RecoveryEngine};
pub use error::RecoveryError;
pub use options::{ConflictStrategy, RecoveryOptions};
pub use report::{FailedEntry, FormatStats, RecoveredEntry, RecoveryReport, SkippedEntry};
pub use sanitize::{insert_deleted_marker, sanitize_filename};
