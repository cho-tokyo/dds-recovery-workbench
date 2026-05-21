//! `NtfsVolume` 統合テスト。実フィクスチャイメージで `NtfsVolume::open()` → 全エントリ列挙
//! のエンドツーエンド動作を検証。Chunk 11 で NTFS リーダの実用形完成を実証する。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04（部分）, FR-LIVE-05, FR-LIVE-06。

mod common;

use dds_fs_ntfs::{find_best_file_name, parse_boot_sector, NtfsVolume};

/// `image` バイト列をクラスタ単位で読むクロージャを生成。`disk-io` 未統合の暫定 reader。
fn make_image_reader(
    img: Vec<u8>,
    cluster_size: u64,
) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
    move |lcn, count| {
        let start = (lcn * cluster_size) as usize;
        let end = start + (count * cluster_size) as usize;
        if end > img.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "oob"));
        }
        Ok(img[start..end].to_vec())
    }
}

fn open_fixture_volume(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(make_image_reader(img, cs)).expect("open volume")
}

#[test]
fn ntfs_healthy_small_enumerates_all_records_and_finds_30_user_files() {
    let mut volume = open_fixture_volume("ntfs_healthy_small");
    assert!(volume.total_records() > 23, "MFT should have system records 0-23 plus users");

    let mut user_file_count = 0;
    for (_idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_in_use() {
            continue;
        }
        if let Some(name) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        {
            if name.filename.starts_with("file_") {
                user_file_count += 1;
            }
        }
    }
    assert!(user_file_count >= 30, "Expected >=30 user files in healthy fixture, got {}", user_file_count);
}

#[test]
fn ntfs_with_deletions_finds_5_deleted_user_files() {
    let mut volume = open_fixture_volume("ntfs_with_5_deletions_small");
    let mut deleted = 0;
    for (_idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_deleted() {
            continue;
        }
        if let Some(name) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        {
            if name.filename.starts_with("file_") {
                deleted += 1;
            }
        }
    }
    assert!(deleted >= 5, "Expected >=5 deleted user files, got {}", deleted);
}

/// Chunk 9 `product_demo_complete_recovery` の `NtfsVolume::iter_records` ベース書き換え版。
/// `--nocapture` で人間可読の復旧結果が得られる。
#[test]
fn product_demo_with_volume_api() {
    let mut volume = open_fixture_volume("ntfs_with_5_deletions_small");

    println!("\n=== DDS Recovery Workbench - Phase 1 (post-Chunk 11) ===\n");
    println!("Total MFT records: {}", volume.total_records());
    println!("Cluster size: {} bytes", volume.cluster_size());
    println!("MFT record size: {} bytes\n", volume.mft_record_size());

    let mut recovered = 0;
    let mut deleted_recovered = 0;
    let mut parse_errors = 0;

    for (idx, result) in volume.iter_records() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => {
                parse_errors += 1;
                continue;
            }
        };
        let Some(name) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if !name.filename.starts_with("file_") {
            continue;
        }
        let status = if entry.header.is_deleted() { "[DELETED]" } else { "[Live]   " };
        let suffix = if entry.header.is_deleted() { "  <- 完全復元!" } else { "" };
        println!("  {} #{:<4} {}{}", status, idx, name.filename, suffix);
        recovered += 1;
        if entry.header.is_deleted() {
            deleted_recovered += 1;
        }
    }

    println!("\n=== Summary ===");
    println!("Total user files recovered: {}", recovered);
    println!("Deleted files recovered:    {}", deleted_recovered);
    println!("Per-record parse errors:    {} (tolerated, iteration continued)\n", parse_errors);

    assert!(recovered >= 30, "Expected >=30 files, got {}", recovered);
    assert!(deleted_recovered >= 5, "Expected >=5 deleted files, got {}", deleted_recovered);
}
