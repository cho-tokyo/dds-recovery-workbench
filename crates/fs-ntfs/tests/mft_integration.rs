//! 結合テスト: 実 NTFS フィクスチャイメージから MFT エントリを解析する。
//! 関連 FR: FR-LIVE-01, FR-LIVE-05。
mod common;
use dds_fs_ntfs::{parse_boot_sector, parse_mft_entry};

#[test]
fn parses_first_mft_record_from_healthy_image() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let bs = parse_boot_sector(&img[..512]).expect("parse boot");
    let mft_offset = bs.mft_byte_offset() as usize;
    let mft_size = bs.mft_record_size_bytes() as usize;
    let entry_bytes = &img[mft_offset..mft_offset + mft_size];
    let entry = parse_mft_entry(entry_bytes).expect("parse mft entry");
    assert!(entry.header.is_in_use(), "$MFT entry 0 must be in use");
    assert!(
        !entry.header.is_directory(),
        "$MFT itself is not a directory"
    );
    if let Some(n) = entry.header.mft_record_number {
        assert_eq!(n, 0);
    }
}

#[test]
fn counts_deleted_entries_in_deletions_fixture() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let bs = parse_boot_sector(&img[..512]).expect("parse boot");
    let mft_offset = bs.mft_byte_offset() as usize;
    let mft_size = bs.mft_record_size_bytes() as usize;
    let mut deleted = 0usize;
    for i in 0..100 {
        let off = mft_offset + i * mft_size;
        if off + mft_size > img.len() {
            break;
        }
        let bytes = &img[off..off + mft_size];
        if &bytes[..4] != b"FILE" {
            continue;
        }
        if let Ok(entry) = parse_mft_entry(bytes) {
            if entry.header.is_deleted() {
                deleted += 1;
            }
        }
    }
    assert!(
        deleted >= 5,
        "expected at least 5 deleted entries in fixture, got {deleted}"
    );
}
