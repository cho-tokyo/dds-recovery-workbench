//! `$DATA` 属性パーサの結合テスト。フィクスチャイメージから 30 ファイルを SHA256 検証付きで
//! 復元できることを実証する。関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05, FR-REC-01, FR-REC-04。

mod common;

use dds_fs_ntfs::{
    extract_main_data_stream, find_best_file_name, parse_boot_sector, parse_mft_entry, DataContent,
    MftEntry,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn read_record(img: &[u8], idx: usize) -> Option<MftEntry> {
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 健全イメージから全ユーザファイルの (filename, sha256) を収集
fn collect_recovered_hashes(img: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for idx in 0..256 {
        let Some(entry) = read_record(img, idx) else {
            continue;
        };
        let first = entry.header.first_attribute_offset as usize;
        let Some(name) = find_best_file_name(&entry.data, first) else {
            continue;
        };
        if !name.filename.starts_with("file_") {
            continue;
        }
        let Some(stream) = extract_main_data_stream(&entry.data, first) else {
            continue;
        };
        if let DataContent::Resident { bytes, .. } = &stream.content {
            out.insert(name.filename.clone(), sha256_hex(bytes));
        }
    }
    out
}

#[test]
fn recovers_all_30_files_with_matching_sha256_in_healthy_image() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let truth = common::load_ground_truth("ntfs_healthy_small");
    let recovered = collect_recovered_hashes(&img);
    let truth_files = truth["files"].as_array().expect("files array");
    if truth_files.is_empty() {
        assert!(
            recovered.len() >= 30,
            "expected >=30 recovered files, got {}",
            recovered.len()
        );
        return;
    }
    let mut matched = 0;
    for f in truth_files {
        let path = f["path"].as_str().unwrap().to_string();
        let expected = f["content_hash_sha256"].as_str().unwrap();
        let actual = recovered
            .get(&path)
            .unwrap_or_else(|| panic!("file not recovered: {path}"));
        assert_eq!(actual, expected, "hash mismatch for {path}");
        matched += 1;
    }
    assert!(
        matched >= 30,
        "expected >=30 files matched, got {}",
        matched
    );
}

#[test]
fn recovers_all_5_deleted_files_with_matching_sha256() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let truth = common::load_ground_truth("ntfs_with_5_deletions_small");
    let recovered = collect_recovered_hashes(&img);
    let truth_files = truth["files"].as_array().expect("files array");
    let mut deleted_checked = 0;
    for f in truth_files {
        let path = f["path"].as_str().unwrap().to_string();
        let expected = f["content_hash_sha256"].as_str().unwrap();
        let is_deleted = f["is_deleted"].as_bool().unwrap_or(false);
        let actual = recovered
            .get(&path)
            .unwrap_or_else(|| panic!("file not recovered: {path}"));
        assert_eq!(
            actual, expected,
            "hash mismatch for {path} (deleted={is_deleted})"
        );
        if is_deleted {
            deleted_checked += 1;
        }
    }
    assert!(
        deleted_checked >= 5,
        "expected >=5 deleted files verified, got {}",
        deleted_checked
    );
}

#[test]
fn product_demo_complete_recovery() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let boot = parse_boot_sector(&img[..512]).expect("boot");

    println!("\n=== DDS Recovery Workbench - Phase 1 Demo ===\n");
    println!("Source: ntfs_with_5_deletions_small.img");
    println!("Cluster size: {} bytes", boot.cluster_size_bytes());
    println!("MFT location: byte {}\n", boot.mft_byte_offset());

    let mft_record_size = boot.mft_record_size_bytes() as usize;
    let mft_start = boot.mft_byte_offset() as usize;

    let mut recovered = 0;
    let mut deleted_recovered = 0;

    for entry_idx in 16..150 {
        let entry_offset = mft_start + entry_idx * mft_record_size;
        if entry_offset + mft_record_size > img.len() {
            break;
        }
        let Ok(entry) = parse_mft_entry(&img[entry_offset..entry_offset + mft_record_size]) else {
            continue;
        };
        if entry.header.first_attribute_offset == 0 {
            continue;
        }
        let first = entry.header.first_attribute_offset as usize;
        let Some(name) = find_best_file_name(&entry.data, first) else {
            continue;
        };
        if !name.filename.starts_with("file_") {
            continue;
        }
        let Some(stream) = extract_main_data_stream(&entry.data, first) else {
            continue;
        };
        let status = if entry.header.is_deleted() {
            "[DELETED]"
        } else {
            "[Live]   "
        };
        let size = stream.content.size();
        let suffix = if entry.header.is_deleted() {
            "  <- 完全復元!"
        } else {
            ""
        };
        println!(
            "  {} {:<20} ({} bytes){}",
            status, name.filename, size, suffix
        );
        recovered += 1;
        if entry.header.is_deleted() {
            deleted_recovered += 1;
        }
    }

    println!("\n=== Summary ===");
    println!("Total files recovered:   {}", recovered);
    println!("Deleted files recovered: {}", deleted_recovered);
    println!();
    assert!(
        recovered >= 30,
        "Expected at least 30 files, got {}",
        recovered
    );
    assert!(
        deleted_recovered >= 5,
        "Expected at least 5 deleted files, got {}",
        deleted_recovered
    );
}
