//! # dds-fs-ntfs
//!
//! NTFS リーダー実装。Chunk 4 でブートセクタ、Chunk 5 で MFT エントリ、Chunk 6 で属性ヘッダを追加。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-05（削除エントリ可視化）、FR-LIVE-06（メタデータ表示）。
//! 詳細は docs/PRD.md と docs/chunk6_ntfs_attribute_header.md を参照。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod attribute;
pub mod boot_sector;
pub mod mft;
pub use attribute::{
    parse_attribute_header, AttributeCommonHeader, AttributeError, AttributeHeader,
    AttributeType, NonResidentInfo, ResidentInfo,
};
pub use boot_sector::{parse_boot_sector, BootSector, BootSectorError};
pub use mft::{parse_mft_entry, MftEntry, MftEntryHeader, MftError};
