//! Chunk 23 結合テスト: 業務フロー end-to-end + Phase 1.5 プロダクトデモ。
//!
//! 1. `full_business_flow_from_case_creation_to_delivery`:
//!    案件作成 → 業務復旧実行 → case.json 保存 → 再読込で全フィールド保持確認。
//!    納品ディレクトリ (`{drive}/{案件番号}/復旧データ/{通常,削除}ファイル/`、
//!    `{drive}/{案件番号}/レポート/{復旧レポート.docx,要確認ファイル一覧.txt,
//!    業務管理レポート.html,report.csv}`) の生成を機械検証。
//!
//! 2. `product_demo_phase_1_5_complete`:
//!    Phase 1.5 完成版デモ。`--nocapture` で業務メンバー提示用の出力を確認。
//!
//! 関連 FR: FR-OUT-01〜04, FR-CASE-01〜04, FR-REC-01〜04, FR-REP-01〜05。

mod common;

use tempfile::TempDir;

use dds_case_manager::{execute_business_recovery, CaseId, CaseStorage};
use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};

use common::{count_files_recursive, decompress_fixture, make_image_reader};

fn open_fixture(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(make_image_reader(img, cs)).expect("open volume")
}

fn make_wishlist() -> Wishlist {
    // 全 .txt ファイルを High 優先で対象に。
    // ntfs_with_5_deletions_small は 30 件 (live 25 + deleted 5) すべて .txt。
    Wishlist::new().add(
        Wish::new(WishItem::Extension("txt".into()), "全 .txt ファイル")
            .with_priority(Priority::High),
    )
}

#[test]
fn full_business_flow_from_case_creation_to_delivery() {
    // 検証 PC の C:\cases\ を tempfile で代用。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());

    // 納品 HDD (G:\) を tempfile で代用。
    let delivery_drive = TempDir::new().unwrap();

    // 1. 案件作成。
    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    // 2. NTFS ボリュームをセットアップ。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");

    // 3. Wishlist 作成。
    let wishlist = make_wishlist();

    // 4. 業務復旧実行。
    let result =
        execute_business_recovery(&mut case, delivery_drive.path(), &mut volume, &wishlist)
            .expect("execute_business_recovery");

    // 5. case.json 永続化。
    storage.save(&case).unwrap();

    // 6. 検証: 出力ディレクトリ構造（全 6 パス）。
    let case_root = delivery_drive.path().join("260522-04");
    assert!(case_root.is_dir(), "case root missing");
    assert!(
        case_root.join("復旧データ").join("通常ファイル").is_dir(),
        "通常ファイル/ missing"
    );
    assert!(
        case_root.join("復旧データ").join("削除ファイル").is_dir(),
        "削除ファイル/ missing"
    );
    let reports_dir = case_root.join("レポート");
    assert!(reports_dir.is_dir(), "レポート/ missing");
    assert!(
        reports_dir.join("復旧レポート.docx").is_file(),
        "復旧レポート.docx missing"
    );
    assert!(
        reports_dir.join("要確認ファイル一覧.txt").is_file(),
        "要確認ファイル一覧.txt missing"
    );
    assert!(
        reports_dir.join("業務管理レポート.html").is_file(),
        "業務管理レポート.html missing"
    );
    assert!(
        reports_dir.join("report.csv").is_file(),
        "report.csv missing"
    );

    // 7. 検証: 復旧件数（30 件全件成功）。
    assert_eq!(result.report.total_matched, 30);
    assert_eq!(result.report.recovered.len(), 30);
    assert_eq!(result.report.failed.len(), 0);

    // 8. 検証: 通常 25 件 / 削除 5 件の振り分け。
    let live_count = count_files_recursive(&result.case_output.live_files_dir());
    let deleted_count = count_files_recursive(&result.case_output.deleted_files_dir());
    assert_eq!(live_count, 25, "通常ファイル/ count");
    assert_eq!(deleted_count, 5, "削除ファイル/ count");

    // 9. 検証: case.json 再読込で全フィールド保持。
    let loaded = storage.load(&case_id).unwrap();
    assert!(loaded.output_dir.is_some(), "output_dir persisted");
    assert_eq!(
        loaded.output_dir.as_ref().unwrap(),
        &result.case_output.root()
    );
    let summary = loaded
        .recovery_report_summary
        .as_ref()
        .expect("summary persisted");
    assert_eq!(summary.total_matched, 30);
    assert_eq!(summary.recovered_count, 30);
    assert!(loaded.wishlist.is_some(), "wishlist persisted");
    assert_eq!(loaded.wishlist.as_ref().unwrap().wishes.len(), 1);

    // 10. BusinessReportPaths が CaseOutput と一致。
    assert_eq!(
        result.report_paths.customer_docx,
        result.case_output.customer_docx_path()
    );
    assert_eq!(result.report_paths.csv, result.case_output.csv_path());
}

#[test]
fn product_demo_phase_1_5_complete() {
    // Phase 1.5 集大成。`cargo test -p dds-case-manager -- --nocapture` で
    // 業務メンバー向けの納品ツリー / 業務指標を確認できる。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = make_wishlist();

    let result =
        execute_business_recovery(&mut case, delivery_drive.path(), &mut volume, &wishlist)
            .expect("execute_business_recovery");
    storage.save(&case).unwrap();

    println!("\n=== Phase 1.5 Complete Demo (Chunk 23) ===\n");
    println!("案件番号: {}", case.case_id);
    println!();
    println!("[納品 HDD] {:?}", delivery_drive.path());
    println!("  └─ 260522-04/");
    println!("      ├─ 復旧データ/");
    println!(
        "      │   ├─ 通常ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.live_files_dir())
    );
    println!(
        "      │   └─ 削除ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.deleted_files_dir())
    );
    println!("      └─ レポート/");
    println!(
        "          ├─ 復旧レポート.docx ({} bytes)",
        result.report_paths.customer_docx.metadata().unwrap().len()
    );
    println!(
        "          ├─ 要確認ファイル一覧.txt ({} bytes)",
        result.report_paths.customer_txt.metadata().unwrap().len()
    );
    println!(
        "          ├─ 業務管理レポート.html ({} bytes)",
        result.report_paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "          └─ report.csv ({} bytes)",
        result.report_paths.csv.metadata().unwrap().len()
    );
    println!();
    println!("[社内保存] {:?}", internal_storage.path());
    println!("  └─ 260522-04/case.json (案件情報、お客様には見せない)");
    println!();
    println!("業務指標:");
    println!("  該当ファイル:      {} 件", result.report.total_matched);
    println!(
        "  復旧成功率:        {:.1}%",
        result.report.recovery_success_rate()
    );
    println!(
        "  品質保証率:        {:.1}%",
        result.report.quality_assurance_rate()
    );
    println!();
    println!("CS のフロー:");
    println!("  1. 納品 HDD を取り出す → G:\\");
    println!("  2. お客様に G:\\ を送付");
    println!("     → お客様は G:\\260522-04\\ を開くだけで全部見える");
    println!("  3. 社内には案件情報が残る (再復旧依頼に備えて)");
    println!();
    println!("=== Phase 1.5 業務統合層完成 ===");
    println!("=== Phase 2.1 (Tauri UI) への準備完了 ===");

    assert!(case.output_dir.is_some());
    assert!(case.recovery_report_summary.is_some());
    assert!(case.wishlist.is_some());
}
