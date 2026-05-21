//! # dds-fs-ntfs
//!
//! NTFS リーダー実装。Chunk 4 でブートセクタ、Chunk 5 で MFT エントリ、Chunk 6 で属性ヘッダ、
//! Chunk 7 で属性イテレータと `$STANDARD_INFORMATION` パーサ、Chunk 8 で `$FILE_NAME` パーサ、
//! Chunk 9 で `$DATA` 常駐属性パーサを追加。Chunk 14 で全パーサを 1 つの owned 型
//! [`NtfsFile`] に統合し、Phase 1 の NTFS リーダー実装完成形に到達。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-04（ファイルツリー）、FR-LIVE-05（削除エントリ可視化）、
//! FR-LIVE-06（メタデータ表示）、FR-REC-01（目標優先抽出）、FR-REC-04（データ整合性）。
//! 詳細は docs/PRD.md と docs/chunk14_ntfs_file_integrated_type.md を参照。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod attribute;
pub mod attributes;
pub mod boot_sector;
pub mod file;
pub mod fixup;
pub mod mft;
pub mod path;
pub mod volume;

pub use attribute::{
    parse_attribute_header, AttributeCommonHeader, AttributeError, AttributeHeader, AttributeType,
    NonResidentInfo, ResidentInfo,
};
pub use attributes::{
    extract_all_data_streams, extract_main_data_stream, find_all_file_names, find_attribute,
    find_best_file_name, parse_data_stream, parse_entries_in_node, parse_file_name,
    parse_index_root, parse_indx_block, parse_runlist, parse_standard_information, read_runs_with,
    AttributeIterator, AttributeRef, DataContent, DataError, DataStream, FileAttributes, FileName,
    FileNameError, FileNameNamespace, FileTime, IndexEntry, IndexError, IndexNodeHeader, IndexRoot,
    IndxBlock, MftReference, Run, RunlistError, SiError, StandardInformation,
};
pub use boot_sector::{parse_boot_sector, BootSector, BootSectorError};
pub use file::{FileContentRef, NtfsFile, NtfsFileIterator};
pub use fixup::{apply_fixup, FixupError};
pub use mft::{parse_mft_entry, MftEntry, MftEntryHeader, MftError};
pub use path::{PathResolver, MAX_PATH_DEPTH, NTFS_ROOT_RECORD};
pub use volume::{DirectoryListing, NtfsMftIterator, NtfsVolume, VolumeError, MAX_BTREE_DEPTH};
