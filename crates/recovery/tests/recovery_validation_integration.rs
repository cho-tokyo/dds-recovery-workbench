//! Chunk 18 結合テスト: 復旧パイプライン × validators 統合の end-to-end 検証。
//!
//! - `recovery_with_validation_marks_txt_as_uncertain`: .txt は Validator なしで全 Uncertain。
//! - `product_demo_recovery_with_validation`: お客様向けデモ。`--nocapture` で見える形に。
//!
//! Chunk 19 で PNG/JPEG/PDF フィクスチャを追加すれば Valid/Invalid の区別ができるが、
//! Chunk 18 時点では「品質判定がパイプライン統合された」ことの可視化に注力する。
//!
//! 関連 FR: FR-REC-04 (データ整合性), FR-QUAL-01 (品質判定)。

mod common;

use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_recovery::RecoveryEngine;
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};
use tempfile::TempDir;

fn open_fixture(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(common::make_image_reader(img, cs)).expect("open volume")
}

#[test]
fn recovery_with_validation_marks_txt_as_uncertain() {
    // ntfs_directories は .txt ファイルのみ。.txt 用 Validator は登録されていないので、
    // 全ファイルが Uncertain 判定される業務的に正しい挙動を検証。
    let mut volume = open_fixture("ntfs_directories");
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".into()), "全 .txt").with_priority(Priority::High));
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions)
        .expect("recover_files");

    // .txt は Validator なしなので Valid / Invalid は 0 件。
    assert_eq!(report.validated_count(), 0, "no .txt validator → Valid 0");
    assert_eq!(report.invalid_count(), 0, "no .txt validator → Invalid 0");

    // Uncertain >= 100 件（109 件全て Uncertain になる想定）。
    assert!(
        report.uncertain_count() >= 100,
        "expected >= 100 Uncertain, got {}",
        report.uncertain_count()
    );

    // 全 recovered エントリに validation フィールドが Some であること
    // （validate_after_recovery = true がデフォルト）。
    for entry in &report.recovered {
        assert!(
            entry.validation.is_some(),
            "validation should be Some for {}",
            entry.original_path
        );
        let v = entry.validation.as_ref().unwrap();
        assert!(
            v.status.is_uncertain(),
            ".txt should be Uncertain, got {:?} for {}",
            v.status,
            entry.original_path
        );
    }
}

#[test]
fn product_demo_recovery_with_validation() {
    // Chunk 17 の product_demo を validation 結果も表示する形に拡張。
    // 現フィクスチャは .txt のみだが、レポート出力ロジックは全形式で同じ。
    let mut volume = open_fixture("ntfs_directories");
    let wishlist = Wishlist::new().add(
        Wish::new(WishItem::Extension("txt".into()), "テキスト全般")
            .with_priority(Priority::Critical),
    );
    let exclusions = ExclusionList::default_system_exclusions();

    let tmp = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(tmp.path());
    let report = engine
        .recover_files(&mut volume, &wishlist, &exclusions)
        .expect("recover_files");

    println!("\n=== DDS Recovery Workbench - Recovery + Validation Demo (Chunk 18) ===\n");
    println!("Source:    ntfs_directories.img.zst");
    println!("Output:    {:?}", tmp.path());
    println!("Wishlist:  {} wish(es)", wishlist.wishes.len());
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

    println!("Validation breakdown:");
    println!("  Valid:     {}", report.validated_count());
    println!("  Invalid:   {}", report.invalid_count());
    println!(
        "  Uncertain: {} (no validator for .txt)",
        report.uncertain_count()
    );
    println!();

    // 業務観測サンプル: 最初の数件で summary を表示。
    println!("Per-file validation samples (first 3):");
    for entry in report.recovered.iter().take(3) {
        let summary = entry
            .validation
            .as_ref()
            .map(|v| v.summary())
            .unwrap_or_else(|| "<no validation>".to_string());
        println!("  {} -> {}", entry.original_path, summary);
    }
    println!();

    println!("Note: PNG/JPEG/PDF fixtures will be added in Chunk 19");
    println!("      to demonstrate Valid/Invalid distinction in CS-facing reports.");
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

    // 検証ロジック整合性。
    assert!(report.recovered.len() >= 100);
    assert_eq!(
        report.validated_count() + report.invalid_count() + report.uncertain_count(),
        report.recovered.len(),
        "validation counts should sum to recovered count"
    );
    assert_eq!(report.validated_count(), 0);
    assert_eq!(report.invalid_count(), 0);
}
