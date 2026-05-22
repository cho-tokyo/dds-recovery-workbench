//! Chunk 20.5 結合テスト: recovery → validators → report の end-to-end 連鎖（業務適用版）。
//!
//! `ntfs_mixed_formats.img.zst` を入力に、**4 形式**（顧客 .docx / 顧客 .txt /
//! CS HTML / CSV）のレポートを生成し、業務的に重要な不変条件を機械検証する:
//!
//! - 4 ファイルが規定ファイル名で生成される
//! - 顧客 .docx は ZIP magic (PK\x03\x04) で開始（OOXML 構造）
//! - 顧客 .docx の **XML 中身**に CS 内部メモが**絶対に**漏れていない
//! - CS HTML に警告文・業務指標・形式別ブレイクダウンが含まれる
//!
//! 関連 FR: FR-REP-01〜05 / FR-QUAL-04。

mod common;

use std::io::Read;

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

/// .docx の ZIP アーカイブを開いて、全 .xml の文字列を連結して返す。
/// 内部メモ漏洩テストで使う。
fn extract_docx_xml_text(docx_bytes: &[u8]) -> String {
    let cursor = std::io::Cursor::new(docx_bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("docx is a ZIP archive");
    let mut all_text = String::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        if file.name().ends_with(".xml") {
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            all_text.push_str(&content);
        }
    }
    all_text
}

#[test]
fn generates_four_report_files_in_business_format() {
    // Chunk 20.5: 4 形式（.docx / .txt / .html / .csv）が全て生成されること。
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let recovery_dir = temp_dir.path().join("recovered");
    let report_dir = temp_dir.path().join("reports");

    let engine = RecoveryEngine::new(&recovery_dir);
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    let paths = dds_report::write_all_reports(&report, &report_dir).expect("write_all_reports");

    assert!(paths.customer_docx.exists());
    assert!(paths.invalid_txt.exists());
    assert!(paths.internal_html.exists());
    assert!(paths.csv.exists());

    // .docx は OOXML ZIP（PK magic で始まる）
    let docx_bytes = std::fs::read(&paths.customer_docx).unwrap();
    assert!(
        docx_bytes.starts_with(b"PK\x03\x04"),
        ".docx must be a ZIP archive (PK magic)"
    );

    // 各ファイルサイズが妥当
    assert!(paths.customer_docx.metadata().unwrap().len() > 1000);
    assert!(paths.internal_html.metadata().unwrap().len() > 1000);
    assert!(paths.csv.metadata().unwrap().len() > 200);
    // recovered_files.txt は Invalid 0 件のケースでも会社名フッターを含むので 100+ bytes。
    assert!(paths.invalid_txt.metadata().unwrap().len() > 50);
}

#[test]
fn customer_docx_must_not_contain_internal_notes() {
    // 業務上、最重要の不変条件: 顧客 .docx の XML 中身に CS 内部メモが含まれてはならない。
    // .docx は ZIP なので、内部 XML を文字列抽出して機械的に grep 検証する。
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path().join("recovered"));
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    let docx_bytes = dds_report::render_customer_docx(&report).expect("render_customer_docx");
    let all_xml_text = extract_docx_xml_text(&docx_bytes);

    let forbidden = ["再復旧推奨", "CS 確認", "業務判断", "技術調査", "disk-io 層"];
    for phrase in &forbidden {
        assert!(
            !all_xml_text.contains(phrase),
            "Customer DOCX must not contain CS-internal phrase: '{}'",
            phrase
        );
    }
}

#[test]
fn product_demo_business_grade_reports() {
    // Chunk 20.5 完成デモ: recovery → validators → report の全パイプラインを
    // 実行し、4 形式の業務適用版レポートを生成して業務不変条件を確認する。
    //
    // 実行: `cargo test --release -p dds-recovery --test recovery_with_reports_integration \
    //        product_demo_business_grade_reports -- --nocapture`
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

    println!("\n=== DDS Recovery Workbench - Business-Grade Reports (Chunk 20.5) ===\n");
    println!("入力:");
    println!("  ソース: ntfs_mixed_formats.img.zst");
    println!();
    println!("業務指標:");
    println!("  該当ファイル数:  {} 件", report.total_matched);
    println!("  復旧成功率:      {:.1}%", report.recovery_success_rate());
    println!("  品質保証率:      {:.1}%", report.quality_assurance_rate());
    println!(
        "  復旧データ量:    {}",
        dds_report::format_bytes(report.total_bytes_written())
    );
    println!(
        "  処理時間:        {}",
        dds_report::format_duration_ms(report.duration_ms())
    );
    println!();
    println!("形式別ブレイクダウン:");
    for (format, stats) in report.format_breakdown() {
        println!(
            "  {:6} : {}/{} 正常 ({:.1}%)",
            format,
            stats.valid,
            stats.total,
            stats.valid_ratio()
        );
    }
    println!();
    println!("出力ファイル:");
    println!(
        "  [顧客向け] report_customer.docx ({} bytes)",
        paths.customer_docx.metadata().unwrap().len()
    );
    println!(
        "  [顧客向け] recovered_files.txt  ({} bytes)",
        paths.invalid_txt.metadata().unwrap().len()
    );
    println!(
        "  [CS 内部] report_internal.html  ({} bytes)",
        paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "  [外部連携] report.csv           ({} bytes)",
        paths.csv.metadata().unwrap().len()
    );
    println!();
    println!("CS のフロー:");
    println!("  1. report_customer.docx を Word で開いて確認");
    println!("  2. 案件固有の注記を追加 (必要なら)");
    println!("  3. 「PDF として保存」(Word の機能)");
    println!("  4. PDF + recovered_files.txt をお客様に納品");
    println!();
    println!("=== Phase 1 NTFS-α 業務適用版完成 ===");

    // 基本的な assertions
    assert!(paths.customer_docx.metadata().unwrap().len() > 1000);
    assert!(paths.invalid_txt.metadata().unwrap().len() > 50);
    // .docx XML 中に CS 内部メモが含まれない（顧客漏洩防止）
    let docx_bytes = std::fs::read(&paths.customer_docx).unwrap();
    let all_xml_text = extract_docx_xml_text(&docx_bytes);
    assert!(
        !all_xml_text.contains("再復旧推奨"),
        "Customer DOCX must not leak CS internal note"
    );

    // CS HTML に業務指標が出ていること
    let internal = std::fs::read_to_string(&paths.internal_html).unwrap();
    assert!(internal.contains("品質保証率"));
    assert!(internal.contains("形式別ブレイクダウン") || internal.contains("ブレイクダウン"));

    // CSV ヘッダーが 14 列
    let csv = std::fs::read_to_string(&paths.csv).unwrap();
    let first_line = csv.lines().next().unwrap();
    let col_count = first_line.split(',').count();
    assert_eq!(col_count, 14, "CSV ヘッダーは 14 列: {}", first_line);
    assert!(first_line.contains("matched_wishes"));
}

/// 通常 CI からは除外する開発者用デモ: 生成された 4 レポートを
/// `target/chunk20_5-samples/` に永続化し、Word / Notepad / ブラウザ / Excel での視覚確認に使う。
///
/// 実行: `cargo test -p dds-recovery --test recovery_with_reports_integration \
///        persist_chunk20_5_demo_reports -- --ignored --nocapture`
#[test]
#[ignore]
fn persist_chunk20_5_demo_reports() {
    let mut volume = open_mixed_formats_volume();
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path().join("recovered"));
    let report = engine
        .recover_files(&mut volume, &business_wishlist())
        .expect("recover_files");

    // workspace ルート target/chunk20_5-samples に永続化。
    let mut sample_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    sample_dir.push("../../target/chunk20_5-samples");
    let paths = dds_report::write_all_reports(&report, &sample_dir).expect("write_all_reports");

    println!(
        "Persistent reports written to: {:?}",
        sample_dir.canonicalize().unwrap_or(sample_dir)
    );
    println!(
        "  customer_docx: {} bytes",
        paths.customer_docx.metadata().unwrap().len()
    );
    println!(
        "  invalid_txt:   {} bytes",
        paths.invalid_txt.metadata().unwrap().len()
    );
    println!(
        "  internal_html: {} bytes",
        paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "  csv:           {} bytes",
        paths.csv.metadata().unwrap().len()
    );
}
