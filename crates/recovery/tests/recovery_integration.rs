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
use dds_recovery::{NoopProgressReporter, ProgressReporter, RecoveredEntry, RecoveryEngine};
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};
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
    // Chunk 23.7: フィクスチャは user files のみで構成。デフォルト exclusions だと
    // $ プレフィックスファイルが除外されるが、このフィクスチャには存在しない想定。
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions, &NoopProgressReporter)
        .expect("recover_files");

    // Chunk 23.7: 全 user file が復旧対象（フィクスチャは 30 件全て .txt）。
    // 30 ファイル全件（live 25 + deleted 5）が復旧成功 + 全件 priority。
    assert_eq!(report.total_matched, 30, "total_matched");
    assert_eq!(report.recovered.len(), 30, "recovered count");
    assert_eq!(report.failed.len(), 0, "no failures");
    assert_eq!(report.skipped.len(), 0, "no skips");
    // Wishlist マッチ = 全 30 件（拡張子 .txt のため）。
    assert_eq!(report.priority_count(), 30, "all 30 are wishlist matches");

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
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions, &NoopProgressReporter)
        .expect("recover_files");

    // 109 ファイル全件復旧（全て live、全て .txt なので全件 priority）。
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
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions, &NoopProgressReporter)
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

// === Chunk 24b 結合テスト: 並列化 + 進捗表示 ===

/// Mock ProgressReporter。`Send + Sync` 制約を満たし、call の履歴を記録する。
struct MockProgressReporter {
    calls: std::sync::Mutex<Vec<(usize, usize, String)>>,
}

impl MockProgressReporter {
    fn new() -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    fn last_current(&self) -> Option<usize> {
        self.calls.lock().unwrap().last().map(|(c, _, _)| *c)
    }
    fn last_total(&self) -> Option<usize> {
        self.calls.lock().unwrap().last().map(|(_, t, _)| *t)
    }
}

impl ProgressReporter for MockProgressReporter {
    fn report(&self, current: usize, total: usize, current_path: &str) {
        self.calls
            .lock()
            .unwrap()
            .push((current, total, current_path.to_string()));
    }
}

#[test]
fn parallel_recovery_processes_all_files() {
    // Chunk 24b: 並列化された recover_files で全ファイル復旧されること。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = Wishlist::new();
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions, &NoopProgressReporter)
        .expect("recover_files");

    // 並列化前と同じく 30 件全件復旧。
    assert_eq!(report.recovered.len(), 30, "all 30 recovered");
    assert_eq!(report.failed.len(), 0, "no failures");
    assert_eq!(report.skipped.len(), 0, "no skips");
}

#[test]
fn parallel_recovery_progress_called_for_each_file() {
    // Chunk 24b: ProgressReporter::report が各ファイル分呼ばれ、最終は (total, total) に達すること。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = Wishlist::new();
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let progress = MockProgressReporter::new();

    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions, &progress)
        .expect("recover_files");

    // 30 ファイル + 最終の (total, total) 報告。プロデューサが各 ntfs_file 投入時に
    // 1 回ずつ呼び、最後に 100% で 1 回呼ぶため、call_count >= 30 + 1。
    assert!(
        progress.call_count() >= 30,
        "expected at least 30 progress calls, got {}",
        progress.call_count()
    );
    // 最終呼び出しは (total, total) すなわち 100%。
    assert_eq!(
        progress.last_current(),
        Some(30),
        "last current should be 30"
    );
    assert_eq!(progress.last_total(), Some(30), "last total should be 30");

    // 復旧件数とも一致。
    assert_eq!(report.recovered.len(), 30);
}
