//! # dds-fs-ntfs
//!
//! NTFS リーダー実装。Chunk 4 でブートセクタ、Chunk 5 で MFT エントリ、Chunk 6 で属性ヘッダ、
//! Chunk 7 で属性イテレータと `$STANDARD_INFORMATION` パーサ、Chunk 8 で `$FILE_NAME` パーサ、
//! Chunk 9 で `$DATA` 常駐属性パーサを追加。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-04（ファイルツリー）、FR-LIVE-05（削除エントリ可視化）、
//! FR-LIVE-06（メタデータ表示）、FR-REC-01（目標優先抽出）、FR-REC-04（データ整合性）。
//! 詳細は docs/PRD.md と docs/chunk9_ntfs_data_resident.md を参照。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod attribute;
pub mod attributes;
pub mod boot_sector;
pub mod mft;
pub mod volume;

pub use attribute::{
    parse_attribute_header, AttributeCommonHeader, AttributeError, AttributeHeader,
    AttributeType, NonResidentInfo, ResidentInfo,
};
pub use attributes::{
    extract_all_data_streams, extract_main_data_stream, find_all_file_names, find_attribute,
    find_best_file_name, parse_data_stream, parse_file_name, parse_runlist,
    parse_standard_information, read_runs_with, AttributeIterator, AttributeRef, DataContent,
    DataError, DataStream, FileAttributes, FileName, FileNameError, FileNameNamespace, FileTime,
    MftReference, Run, RunlistError, SiError, StandardInformation,
};
pub use boot_sector::{parse_boot_sector, BootSector, BootSectorError};
pub use mft::{parse_mft_entry, MftEntry, MftEntryHeader, MftError};
pub use volume::{NtfsMftIterator, NtfsVolume, VolumeError};
