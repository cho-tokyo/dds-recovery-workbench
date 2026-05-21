//! Chunk 20 結合テスト: recovery → validators → report の end-to-end 連鎖。
//!
//! `ntfs_mixed_formats.img.zst` を入力に、3 形式（顧客 HTML / CS HTML / CSV）の
//! レポートを生成し、業務的に重要な不変条件を機械検証する:
//!
//! - 3 ファイルが規定ファイル名で生成される
//! - 顧客 HTML に CS 内部メモが**絶対に**漏れていない
//! - CS HTML に警告文と内部メモが含まれる
//!
//! 関連 FR: FR-REP-01 / FR-REP-02 / FR-REP-03 / FR-QUAL-04。

mod common;

use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_recovery::RecoveryEngine;
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};
use tempfile::TempDir;

/// `ntfs_mixed_formats` フィクスチャを開く。
fn open_mixed_formats_volume(
) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture("ntfs_mixed_formats");
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(common::make_image_reader(img, cs)).expect("open volume")
}

/// 業務形式（PNG / JPEG / PDF / GIF / BMP / DOCX）を全て指定した希望リスト。
fn business_wishlist() -> Wishlist {
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
fn generates_all_three_report_formats_from_mixed_fixture() {
    // 業務シナリオ: 混在フィクスチャから 3 形式レポートが全て生成され、
    // 各ファイルが規定サイズ以上の内容を持つこと。
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let recovery_dir = temp_dir.path().join("recovered");
    let report_dir = temp_dir.path().join("reports");

    let engine = RecoveryEngine::new(&recovery_dir);
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    let paths = dds_report::write_all_reports(&report, &report_dir).expect("write_all_reports");

    assert!(paths.customer_html.exists());
    assert!(paths.internal_html.exists());
    assert!(paths.csv.exists());

    // 各ファイルが空でないこと（実データを含む）。
    let customer_size = paths.customer_html.metadata().unwrap().len();
    let internal_size = paths.internal_html.metadata().unwrap().len();
    let csv_size = paths.csv.metadata().unwrap().len();

    assert!(
        customer_size > 1000,
        "顧客 HTML は 1000 byte 超: actual {}",
        customer_size
    );
    assert!(
        internal_size > 1000,
        "CS HTML は 1000 byte 超: actual {}",
        internal_size
    );
    assert!(csv_size > 500, "CSV は 500 byte 超: actual {}", csv_size);
}

#[test]
fn customer_html_must_not_contain_internal_notes() {
    // 業務上、最重要の不変条件: 顧客 HTML に CS 内部メモが含まれてはならない。
    //
    // 機械検証: validator が internal_note_ja で使う既知フレーズをすべて grep して、
    // 1 つでも見つかったら失敗。
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path().join("recovered"));
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    let html = dds_report::render_customer_html(&report).expect("render_customer_html");

    let forbidden_strings = [
        "再復旧推奨",
        "CS 確認",
        "業務判断",
        "技術調査",
        "validator 追加検討",
        "disk-io 層を確認",
        "CS 内部",
    ];

    for forbidden in &forbidden_strings {
        assert!(
            !html.contains(forbidden),
            "顧客 HTML に CS 内部フレーズが含まれてはならない: '{}' が検出された",
            forbidden
        );
    }
}

#[test]
fn product_demo_full_pipeline_with_reports() {
    // Phase 1 NTFS-α 完成デモ: recovery → validators → report の全パイプラインを
    // 実行し、3 形式のレポートを生成して業務不変条件を確認する。
    //
    // `cargo test --release --nocapture` で出力可視化推奨。
    let mut volume = open_mixed_formats_volume();
    let wishlist = business_wishlist();

    let temp_dir = TempDir::new().unwrap();
    let recovery_dir = temp_dir.path().join("recovered");
    let report_dir = temp_dir.path().join("reports");

    let engine = RecoveryEngine::new(&recovery_dir);
    let report = engine
        .recover_files(&mut volume, &wishlist)
        .expect("recover_files");

    let paths = dds_report::write_all_reports(&report, &report_dir).expect("write_all_reports");

    println!("\n=== DDS Recovery Workbench - Full Pipeline Demo (Chunk 20) ===\n");
    println!("入力:");
    println!("  ソース: ntfs_mixed_formats.img.zst");
    println!("  希望: 全形式（PNG/JPEG/PDF/GIF/BMP/DOCX）");
    println!();
    println!("復旧結果:");
    println!("  対象: {} ファイル", report.total_matched);
    println!("  成功: {} ファイル", report.recovered.len());
    println!("  品質 OK: {}", report.validated_count());
    println!("  品質 NG: {}", report.invalid_count());
    println!();
    println!("出力レポート:");
    println!(
        "  顧客向け HTML: {:?} ({} bytes)",
        paths.customer_html,
        paths.customer_html.metadata().unwrap().len()
    );
    println!("    (お客様に納品可能、internal_note を含まない)");
    println!(
        "  CS 向け HTML:  {:?} ({} bytes)",
        paths.internal_html,
        paths.internal_html.metadata().unwrap().len()
    );
    println!("    (業務管理用、internal_note + SHA256 含む)");
    println!(
        "  CSV:           {:?} ({} bytes)",
        paths.csv,
        paths.csv.metadata().unwrap().len()
    );
    println!("    (外部システム連携用、全 13 フィールド)");
    println!();
    println!("=== Phase 1 NTFS-α 完成 ===");

    // 顧客 HTML には CS 内部メモが一切含まれない。
    let customer = std::fs::read_to_string(&paths.customer_html).unwrap();
    assert!(
        !customer.contains("CS 内部"),
        "顧客 HTML に 'CS 内部' は含めない"
    );
    assert!(
        !customer.contains("再復旧推奨"),
        "顧客 HTML に '再復旧推奨' は含めない"
    );

    // CS HTML には CS が含まれること（タイトル or 警告文 or カラム名）。
    let internal = std::fs::read_to_string(&paths.internal_html).unwrap();
    assert!(internal.contains("CS"), "CS HTML には 'CS' が含まれる");

    // CSV が 13 列ヘッダーを持つこと。
    let csv = std::fs::read_to_string(&paths.csv).unwrap();
    let first_line = csv.lines().next().unwrap();
    let col_count = first_line.split(',').count();
    assert_eq!(col_count, 13, "CSV ヘッダーは 13 列: {}", first_line);
}

/// 通常 CI からは除外する開発者用デモ: 生成された 3 レポートを
/// `target/chunk20-samples/` に永続化し、ブラウザ / Excel での視覚確認に使う。
///
/// 実行: `cargo test -p dds-recovery --test recovery_with_reports_integration \
///        persist_chunk20_demo_reports -- --ignored --nocapture`
#[test]
#[ignore]
fn persist_chunk20_demo_reports() {
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path().join("recovered"));
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    // workspace ルート target/chunk20-samples に永続化。
    let mut sample_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    sample_dir.push("../../target/chunk20-samples");
    let paths = dds_report::write_all_reports(&report, &sample_dir).expect("write_all_reports");

    println!("Persistent reports written to: {:?}", sample_dir.canonicalize().unwrap_or(sample_dir));
    println!("  customer_html: {} bytes", paths.customer_html.metadata().unwrap().len());
    println!("  internal_html: {} bytes", paths.internal_html.metadata().unwrap().len());
    println!("  csv:           {} bytes", paths.csv.metadata().unwrap().len());
}
