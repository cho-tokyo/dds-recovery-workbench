//! 結合テスト: 実 NTFS イメージから `$FILE_NAME` 属性を取り出し、削除ファイルを含む実名を取得。
//! 関連 FR: FR-LIVE-01, FR-LIVE-05, FR-LIVE-06。

mod common;
use dds_fs_ntfs::{find_best_file_name, parse_boot_sector, parse_mft_entry};

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

fn collect_user_files(img: &[u8]) -> Vec<(usize, bool, String)> {
    let mut out = Vec::new();
    for idx in 0..256 {
        let Some(entry) = read_record(img, idx) else { continue };
        let first = entry.header.first_attribute_offset as usize;
        let Some(name) = find_best_file_name(&entry.data, first) else { continue };
        if name.filename.starts_with('$') {
            continue;
        }
        if !name.filename.starts_with("file_") {
            continue;
        }
        out.push((idx, entry.header.is_deleted(), name.filename));
    }
    out
}

#[test]
fn discovers_all_user_files_in_healthy_image() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let files = collect_user_files(&img);
    let names: std::collections::BTreeSet<String> =
        files.iter().map(|(_, _, n)| n.clone()).collect();
    assert!(
        names.len() >= 30,
        "expected at least 30 user files, got {}: {:?}",
        names.len(),
        names
    );
    assert!(names.contains("file_000.txt"));
    assert!(names.contains("file_029.txt"));
}

#[test]
fn recovers_deleted_file_names_with_timestamps() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let files = collect_user_files(&img);
    let deleted: std::collections::BTreeSet<String> = files
        .iter()
        .filter(|(_, deleted, _)| *deleted)
        .map(|(_, _, n)| n.clone())
        .collect();
    for expected in [
        "file_003.txt",
        "file_007.txt",
        "file_015.txt",
        "file_022.txt",
        "file_028.txt",
    ] {
        assert!(
            deleted.contains(expected),
            "deleted name not recovered: {expected}, recovered set: {deleted:?}"
        );
    }
}

#[test]
fn prints_live_and_deleted_file_listing_for_human_review() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let files = collect_user_files(&img);
    println!("\n=== File listing from ntfs_with_5_deletions_small ===");
    for (entry, deleted, name) in &files {
        let tag = if *deleted { "[DELETED]" } else { "[Live]   " };
        println!("{tag} {name:<20} (entry #{entry})");
    }
    println!(
        "=== Total: {} files ({} deleted) ===\n",
        files.len(),
        files.iter().filter(|(_, d, _)| *d).count()
    );
    assert!(files.iter().any(|(_, d, _)| *d));
}
