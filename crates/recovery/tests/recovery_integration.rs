//! Chunk 17 結合テスト。実フィクスチャを使った復旧パイプライン end-to-end 検証。
//!
//! - `recovers_all_5_deleted_txt_files`: 削除済み + 生存ファイル全件復旧、サブディレクトリ分離。
//! - `recovered_files_match_ground_truth_sha256`: ground truth SHA256 と整合性検証。
//! - `product_demo_end_to_end_recovery`: お客様シナリオ。`--nocapture` で見える形に。
//!
//! 関連 FR: FR-REC-01〜04。

mod common;

use std::collections::HashMap;
use std::fs;

use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_recovery::{RecoveredEntry, RecoveryEngine};
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};
use tempfile::TempDir;

fn open_fixture(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(common::make_image_reader(img, cs)).expect("open volume")
}

/// ground truth の path フィールド ("file_000.txt" or "\\file_root_001.txt") を
/// `NtfsFile::path` 形式（先頭 `\` 必須）に正規化する。
fn normalize_gt_path(s: &str) -> String {
    if s.starts_with('\\') {
        s.to_string()
    } else {
        format!("\\{}", s)
    }
}

#[test]
fn recovers_all_5_deleted_txt_files() {
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = Wishlist::new().add(
        Wish::new(WishItem::Extension("txt".into()), "全 .txt ファイル")
            .with_priority(Priority::High),
    );

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist)
        .expect("recover_files");

    // 30 ファイル全件（live 25 + deleted 5）が wish-match でマッチ、全件復旧成功。
    assert_eq!(report.total_matched, 30, "total_matched");
    assert_eq!(report.recovered.len(), 30, "recovered count");
    assert_eq!(report.failed.len(), 0, "no failures");
    assert_eq!(report.skipped.len(), 0, "no skips");

    // 削除済み 5 件は `deleted/` サブディレクトリに。
    let deleted_dir = tmp.path().join("deleted");
    assert!(deleted_dir.exists(), "deleted/ should exist");
    let deleted_files: Vec<_> = fs::read_dir(&deleted_dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(deleted_files.len(), 5, "5 files under deleted/");

    // 生存 25 件は `live/` サブディレクトリに。
    let live_dir = tmp.path().join("live");
    assert!(live_dir.exists(), "live/ should exist");
    let live_files: Vec<_> = fs::read_dir(&live_dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(live_files.len(), 25, "25 files under live/");

    // 削除ファイルは deleted-marker 入りファイル名であること。
    let deleted_recovered: Vec<&RecoveredEntry> =
        report.recovered.iter().filter(|e| e.is_deleted).collect();
    assert_eq!(deleted_recovered.len(), 5);
    for entry in &deleted_recovered {
        let fname = entry.output_path.file_name().unwrap().to_string_lossy();
        assert!(
            fname.contains("(deleted-#"),
            "deleted marker missing in {:?}",
            fname
        );
    }
}

#[test]
fn recovered_files_match_ground_truth_sha256() {
    let mut volume = open_fixture("ntfs_directories");
    let gt = common::load_ground_truth("ntfs_directories");
    let expected: HashMap<String, String> = gt["files"]
        .as_array()
        .expect("files[]")
        .iter()
        .map(|f| {
            (
                normalize_gt_path(f["path"].as_str().expect("path")),
                f["content_hash_sha256"].as_str().expect("hash").to_string(),
            )
        })
        .collect();

    let wishlist = Wishlist::new().add(Wish::new(WishItem::Extension("txt".into()), "全 .txt"));

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist)
        .expect("recover_files");

    // 109 ファイル全件復旧（全て live）。
    assert!(
        report.recovered.len() >= 100,
        "expected >= 100 recovered, got {}",
        report.recovered.len()
    );

    // RecoveredEntry の sha256 を ground truth と突合。
    let mut matched = 0usize;
    let mut mismatched: Vec<(String, String, String)> = Vec::new();
    for entry in &report.recovered {
        if let Some(expected_hash) = expected.get(&entry.original_path) {
            match &entry.sha256 {
                Some(actual) if actual == expected_hash => matched += 1,
                Some(actual) => mismatched.push((
                    entry.original_path.clone(),
                    expected_hash.clone(),
                    actual.clone(),
                )),
                None => panic!("sha256 expected to be Some for {}", entry.original_path),
            }
        }
    }
    assert!(
        mismatched.is_empty(),
        "{} sha256 mismatches; first: {:?}",
        mismatched.len(),
        mismatched.first()
    );
    assert!(
        matched >= 100,
        "expected >= 100 ground truth matches, got {} (out of {} expected)",
        matched,
        expected.len()
    );
    println!(
        "[ground truth] {} / {} files matched SHA256 successfully",
        matched,
        expected.len()
    );
}

#[test]
fn product_demo_end_to_end_recovery() {
    // お客様シナリオ: 削除されたテキスト全般を Critical 優先度で復旧。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = Wishlist::new().add(
        Wish::new(
            WishItem::All(vec![WishItem::Extension("txt".into())]),
            "テキスト全般",
        )
        .with_priority(Priority::Critical),
    );

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist)
        .expect("recover_files");

    println!("\n=== DDS Recovery Workbench - Phase 1 End-to-End Demo ===\n");
    println!("Source:    ntfs_with_5_deletions_small.img.zst");
    println!("Output:    {:?}", tmp.path());
    println!("Wishlist:  {} 希望", wishlist.wishes.len());
    println!();
    println!("Matched:   {}", report.total_matched);
    println!(
        "Recovered: {} (success rate: {:.1}%)",
        report.recovered.len(),
        report.success_rate()
    );
    println!("Failed:    {}", report.failed.len());
    println!("Skipped:   {}", report.skipped.len());
    println!("Duration:  {} ms", report.duration_ms());
    println!();

    let mut deleted_recovered: Vec<&RecoveredEntry> =
        report.recovered.iter().filter(|e| e.is_deleted).collect();
    deleted_recovered.sort_by_key(|e| e.source_id.clone());

    println!("Deleted files recovered:");
    for entry in &deleted_recovered {
        println!(
            "  [OK] {} -> {}",
            entry.original_path,
            entry.output_path.display()
        );
        let sha = entry.sha256.as_deref().unwrap_or("(none)");
        let prefix: String = sha.chars().take(16).collect();
        println!("       sha256: {}...", prefix);
    }
    println!();

    println!("=== Summary ===");
    println!(
        "Total recovered:    {} files ({} bytes)",
        report.recovered.len(),
        report.total_bytes_written()
    );
    println!("Deleted recovered:  {} files", deleted_recovered.len());

    assert_eq!(
        deleted_recovered.len(),
        5,
        "Should recover all 5 deleted files"
    );
    assert_eq!(report.failed.len(), 0, "No failures expected");
    assert_eq!(report.recovered.len(), 30, "30 total recovered");
}
