//! 結合テスト: 実 NTFS イメージから `$STANDARD_INFORMATION` を取得し、タイムスタンプを読む。
//! 関連 FR: FR-LIVE-01, FR-LIVE-06。

mod common;

use dds_fs_ntfs::{
    find_attribute, parse_boot_sector, parse_mft_entry, parse_standard_information,
    AttributeHeader, AttributeType,
};

fn read_record(img: &[u8], idx: usize) -> Option<dds_fs_ntfs::MftEntry> {
    let bs = parse_boot_sector(&img[..512]).ok()?;
    let off = bs.mft_byte_offset() as usize + idx * bs.mft_record_size_bytes() as usize;
    let size = bs.mft_record_size_bytes() as usize;
    if off + size > img.len() {
        return None;
    }
    if &img[off..off + 4] != b"FILE" {
        return None;
    }
    parse_mft_entry(&img[off..off + size]).ok()
}

#[test]
fn reads_standard_information_from_healthy_records() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let mut si_found = 0usize;
    let mut sample_created: Option<chrono::DateTime<chrono::Utc>> = None;
    for idx in 0..50 {
        let Some(entry) = read_record(&img, idx) else {
            continue;
        };
        let first = entry.header.first_attribute_offset as usize;
        let Some(attr) = find_attribute(&entry.data, first, AttributeType::StandardInformation)
        else {
            continue;
        };
        if let AttributeHeader::Resident { resident, .. } = &attr.header {
            let co = resident.content_offset as usize;
            let cs = resident.content_size as usize;
            let si = parse_standard_information(&attr.raw[co..co + cs]).expect("parse SI");
            if sample_created.is_none() {
                sample_created = si.created.to_datetime();
            }
            si_found += 1;
        }
    }
    assert!(si_found >= 1, "expected at least one $STANDARD_INFORMATION");
    println!("healthy: si_found={si_found}, sample_created={sample_created:?}");
}

#[test]
fn reads_standard_information_from_deleted_records() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let mut deleted_si = 0usize;
    let mut sample_created: Option<chrono::DateTime<chrono::Utc>> = None;
    for idx in 0..100 {
        let Some(entry) = read_record(&img, idx) else {
            continue;
        };
        if !entry.header.is_deleted() {
            continue;
        }
        let first = entry.header.first_attribute_offset as usize;
        let Some(attr) = find_attribute(&entry.data, first, AttributeType::StandardInformation)
        else {
            continue;
        };
        if let AttributeHeader::Resident { resident, .. } = &attr.header {
            let co = resident.content_offset as usize;
            let cs = resident.content_size as usize;
            if let Ok(si) = parse_standard_information(&attr.raw[co..co + cs]) {
                if sample_created.is_none() {
                    sample_created = si.created.to_datetime();
                }
                deleted_si += 1;
            }
        }
    }
    assert!(
        deleted_si >= 1,
        "expected $SI readable from at least one deleted entry"
    );
    println!("deleted: deleted_si={deleted_si}, sample_created={sample_created:?}");
}
