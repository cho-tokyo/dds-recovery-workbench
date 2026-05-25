//! Chunk 19 結合テスト: 混在形式フィクスチャでの復旧 + 検証 end-to-end 検証。
//!
//! `ntfs_mixed_formats.img.zst` (15 files: PNG/JPEG/PDF/GIF/BMP/DOCX + 破損 + 拡張子不一致)
//! を実際に復旧パイプラインに通し、ground truth の `expected_validation_status` と
//! `expected_format` を全件照合する業務シナリオ E2E。
//!
//! 関連 FR: FR-QUAL-01 / FR-QUAL-02 / FR-QUAL-03 (品質判定実証完了)。

mod common;

use std::collections::HashMap;

use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_recovery::RecoveryEngine;
use dds_validators::ValidationStatus;
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};
use tempfile::TempDir;

/// `ntfs_mixed_formats` フィクスチャを開く。
fn open_mixed_formats_volume() -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>
{
    let img = common::decompress_fixture("ntfs_mixed_formats");
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(common::make_image_reader(img, cs)).expect("open volume")
}

/// 全 15 件分の希望リスト（拡張子 OR 結合、xyz も含む）。
fn wishlist_all_15() -> Wishlist {
    Wishlist::new().add(
        Wish::new(
            WishItem::Any(vec![
                WishItem::Extension("png".into()),
                WishItem::Extension("jpg".into()),
                WishItem::Extension("pdf".into()),
                WishItem::Extension("gif".into()),
                WishItem::Extension("bmp".into()),
                WishItem::Extension("docx".into()),
                WishItem::Extension("xyz".into()),
            ]),
            "全形式テスト（15 件全件）",
        )
        .with_priority(Priority::High),
    )
}

/// CS デモ用希望リスト（業務形式のみ、xyz 除外で 14 件）。
fn wishlist_business_14() -> Wishlist {
    Wishlist::new().add(
        Wish::new(
            WishItem::Any(vec![
                WishItem::Extension("png".into()),
                WishItem::Extension("jpg".into()),
                WishItem::Extension("pdf".into()),
                WishItem::Extension("gif".into()),
                WishItem::Extension("bmp".into()),
                WishItem::Extension("docx".into()),
            ]),
            "顧客指定: 画像と書類すべて",
        )
        .with_priority(Priority::Critical),
    )
}

#[test]
fn recovers_mixed_formats_with_correct_validation_status() {
    let mut volume = open_mixed_formats_volume();
    let ground_truth = common::load_ground_truth("ntfs_mixed_formats");

    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine
        .recover_files(
            &mut volume,
            &wishlist_all_15(),
            &ExclusionList::default_system_exclusions(),
        )
        .expect("recover_files");

    // Chunk 23.7: 全 user file が復旧対象（15 件、全て root 直下で除外対象外）。
    assert_eq!(
        report.recovered.len(),
        15,
        "All 15 user files should be recovered (got {})",
        report.recovered.len()
    );
    // Wishlist マッチも 15 件全件（xyz 含めて全拡張子指定）。
    assert_eq!(report.priority_count(), 15, "all 15 are wishlist matches");

    // ground truth の expected_validation_status / expected_format を実結果と照合。
    let expected: HashMap<String, (String, Option<String>)> = ground_truth["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_string(),
                (
                    f["expected_validation_status"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    f["expected_format"].as_str().map(|s| s.to_string()),
                ),
            )
        })
        .collect();

    let mut matched = 0;
    for entry in &report.recovered {
        let Some((expected_status, expected_format)) = expected.get(&entry.original_path) else {
            panic!(
                "Recovered path not in ground truth: {}",
                entry.original_path
            );
        };
        let actual = entry
            .validation
            .as_ref()
            .expect("validation should be Some");

        let actual_status = match actual.status {
            ValidationStatus::Valid => "valid",
            ValidationStatus::Invalid => "invalid",
            ValidationStatus::Uncertain => "uncertain",
        };

        assert_eq!(
            actual_status, expected_status,
            "Status mismatch for {}: expected {}, got {} (diagnostics: {:?})",
            entry.original_path, expected_status, actual_status, actual.diagnostics
        );

        if let Some(expected_fmt) = expected_format {
            assert_eq!(
                actual.format_detected.as_deref(),
                Some(expected_fmt.as_str()),
                "Format mismatch for {}: expected {}, got {:?}",
                entry.original_path,
                expected_fmt,
                actual.format_detected
            );
        }

        matched += 1;
    }
    assert_eq!(matched, 15, "All 15 files should match ground truth");
}

#[test]
fn extension_content_mismatch_detected_as_invalid() {
    let mut volume = open_mixed_formats_volume();

    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine
        .recover_files(
            &mut volume,
            &wishlist_all_15(),
            &ExclusionList::default_system_exclusions(),
        )
        .expect("recover_files");

    let mismatch_entry = report
        .recovered
        .iter()
        .find(|e| e.original_path == "\\mismatch_001.pdf")
        .expect("mismatch_001.pdf not found in recovered files");

    let validation = mismatch_entry
        .validation
        .as_ref()
        .expect("validation should be Some");

    assert!(
        validation.status.is_invalid(),
        "Extension-content mismatch (.pdf with PNG content) should be Invalid (got {:?})",
        validation.status
    );
    assert_eq!(
        validation.format_detected.as_deref(),
        Some("PDF"),
        "Should be detected by PDF validator (extension-based dispatch)"
    );
}

#[test]
fn corrupted_samples_marked_as_invalid() {
    let mut volume = open_mixed_formats_volume();

    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine
        .recover_files(
            &mut volume,
            &wishlist_all_15(),
            &ExclusionList::default_system_exclusions(),
        )
        .expect("recover_files");

    let broken_paths = ["\\broken_001.png", "\\broken_002.jpg", "\\broken_003.pdf"];
    for path in broken_paths {
        let entry = report
            .recovered
            .iter()
            .find(|e| e.original_path == path)
            .unwrap_or_else(|| panic!("{} not found in recovered files", path));
        let validation = entry
            .validation
            .as_ref()
            .expect("validation should be Some");
        assert!(
            validation.status.is_invalid(),
            "{} should be Invalid, got {:?} (diagnostics: {:?})",
            path,
            validation.status,
            validation.diagnostics
        );
    }
}

#[test]
fn product_demo_recovery_with_quality_breakdown() {
    let mut volume = open_mixed_formats_volume();
    let wishlist = wishlist_business_14();
    let exclusions = ExclusionList::default_system_exclusions();

    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions)
        .expect("recover_files");

    println!("\n=== DDS Recovery Workbench - Quality Breakdown Demo ===\n");
    println!("Source:    ntfs_mixed_formats.img.zst");
    println!("Output:    {:?}", temp_dir.path());
    println!("Matched:   {}", report.total_matched);
    println!("Recovered: {}", report.recovered.len());
    println!();
    println!("Validation breakdown:");
    println!("  [OK] Valid:     {}", report.validated_count());
    println!("  [NG] Invalid:   {}", report.invalid_count());
    println!("  [?]  Uncertain: {}", report.uncertain_count());
    println!();

    // フォーマット別集計: (valid, invalid, total)。
    let mut by_format: HashMap<String, (u32, u32, u32)> = HashMap::new();
    for entry in &report.recovered {
        let Some(v) = &entry.validation else { continue };
        let format = v
            .format_detected
            .clone()
            .unwrap_or_else(|| "Unknown".into());
        let counters = by_format.entry(format).or_insert((0, 0, 0));
        counters.2 += 1;
        match v.status {
            ValidationStatus::Valid => counters.0 += 1,
            ValidationStatus::Invalid => counters.1 += 1,
            ValidationStatus::Uncertain => {}
        }
    }

    println!("Format breakdown:");
    let mut formats: Vec<_> = by_format.into_iter().collect();
    formats.sort_by_key(|entry| std::cmp::Reverse(entry.1 .2));
    for (format, (valid, invalid, total)) in formats {
        println!(
            "  {:6} : {}/{} valid ({} invalid)",
            format, valid, total, invalid
        );
    }
    println!();

    println!("Invalid files (要 CS 確認):");
    for entry in report.recovered.iter().filter(|e| {
        e.validation
            .as_ref()
            .map(|v| v.status.is_invalid())
            .unwrap_or(false)
    }) {
        let reason = entry
            .validation
            .as_ref()
            .and_then(|v| v.diagnostics.first())
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        println!("  [NG] {} -> {}", entry.original_path, reason);
    }
    println!();
    println!("=== Summary ===");
    println!(
        "Total recovered:    {} files ({} bytes)",
        report.recovered.len(),
        report.total_bytes_written()
    );
    println!(
        "Quality breakdown:  Valid={} / Invalid={} / Uncertain={}",
        report.validated_count(),
        report.invalid_count(),
        report.uncertain_count()
    );

    // Chunk 23.7: 全件復旧設計に変更。xyz ファイルも復旧されるが、Wishlist
    // (business_14) には含まれないので priority_count から外れる。
    // 期待値:
    //   recovered: 15 (xyz も含む全 user file)
    //   priority_count: 14 (business_14 ヒット = xyz 以外)
    //   valid 10 (PNG×3, JPEG×2, PDF×2, GIF×1, BMP×1, DOCX×1)
    //   invalid 4 (3 corrupted + 1 mismatch)
    //   uncertain 1 (xyz は Validator 未対応)
    assert_eq!(
        report.recovered.len(),
        15,
        "all 15 user files recovered (Chunk 23.7 全件復旧)"
    );
    assert_eq!(report.priority_count(), 14, "14 wishlist matches (xyz 除外)");
    assert_eq!(report.validated_count(), 10, "10 Valid expected");
    assert_eq!(report.invalid_count(), 4, "4 Invalid expected");
    assert_eq!(report.uncertain_count(), 1, "1 Uncertain (xyz)");
}
