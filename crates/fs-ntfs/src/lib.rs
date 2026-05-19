//! # dds-fs-ntfs
//!
//! NTFS リーダー実装。Chunk 4 でブートセクタパーサを追加。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）。
//! 詳細は docs/PRD.md と docs/chunk4_ntfs_boot_sector.md を参照。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod boot_sector;
pub use boot_sector::{parse_boot_sector, BootSector, BootSectorError};
