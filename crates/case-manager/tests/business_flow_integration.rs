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
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};

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

/// Chunk 23.7.1: 業務的に説得力のある demo 用 Wishlist。
///
/// `ntfs_mixed_formats` フィクスチャ (15 件: PNG 4 / JPEG 3 / PDF 4 / GIF 1 /
/// BMP 1 / DOCX 1 / xyz 1) に対し、PNG のみを優先データに指定する設計。
///
/// 業務的意図:
/// - 全 15 件が「全体」、PNG 4 件のみが「優先データ」となり、
///   レポート上で「全体 ≠ 優先データ」の二重表示の意味が明確になる
/// - Wishlist は復旧範囲ではなく「お客様優先データのラベリング」だと業務メンバー
///   に伝わる業務シナリオ
fn make_business_demo_wishlist() -> Wishlist {
    Wishlist::new().add(
        Wish::new(
            WishItem::Extension("png".into()),
            "お客様優先: PNG 画像",
        )
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

    // 4. 業務復旧実行。Chunk 23.7: 全件復旧 + デフォルト除外パターン。
    let exclusions = ExclusionList::default_system_exclusions();
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
    )
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
    let exclusions = ExclusionList::default_system_exclusions();

    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
    )
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

#[test]
fn full_business_flow_recovers_all_files_with_priority() {
    // Chunk 23.7: R-STUDIO 風業務フロー (全件復旧 + Wishlist は優先データ)。
    // Wishlist には .txt のみ指定。フィクスチャ ntfs_with_5_deletions_small は
    // 30 件全て .txt なので全 user file が priority になる業務シナリオ。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = make_wishlist(); // .txt 全部
    let exclusions = ExclusionList::default_system_exclusions();

    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
    )
    .expect("execute_business_recovery");

    // 全 30 件復旧（全件復旧設計）。
    assert_eq!(result.report.recovered.len(), 30);
    // 全 30 件が .txt にマッチ → priority。
    assert_eq!(result.report.priority_count(), 30);
    // 優先データ全件の original_path が .txt 拡張子。
    let priority_paths: Vec<_> = result
        .report
        .recovered
        .iter()
        .filter(|e| e.is_priority)
        .map(|e| e.original_path.clone())
        .collect();
    assert!(priority_paths
        .iter()
        .all(|p| p.to_lowercase().ends_with(".txt")));
}

#[test]
fn product_demo_phase_1_5_business_aligned() {
    // Chunk 23.7 完成デモ: R-STUDIO 風業務フロー対応。
    // Chunk 23.7.1: フィクスチャを ntfs_mixed_formats に、Wishlist を PNG のみに変更し、
    // 「全体 (15 件) ≠ 優先データ (4 件)」「validator が機能して品質保証率 66.7%」が
    // 業務メンバーに伝わる demo に改善。
    // `cargo test -p dds-case-manager -- --nocapture` で確認。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_mixed_formats");
    let wishlist = make_business_demo_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
    )
    .expect("execute_business_recovery");
    storage.save(&case).unwrap();

    println!("\n=== Phase 1.5 Business-Aligned Demo (Chunk 23.7.1) ===\n");
    println!("[業務フロー]");
    println!("  Workbench は R-STUDIO 風の全件復旧を実施");
    println!("  Wishlist は『お客様優先データ』としてレポートで強調");
    println!();
    println!("案件番号: {}", case.case_id);
    println!("Wishlist: お客様優先: PNG 画像 (Extension=\"png\")");
    println!();
    println!("[復旧結果 - 全体]");
    println!("  該当ファイル: {} 件", result.report.total_matched);
    println!(
        "  復旧成功率:   {:.1}%",
        result.report.recovery_success_rate()
    );
    println!(
        "  品質保証率:   {:.1}%",
        result.report.quality_assurance_rate()
    );
    println!();
    println!("[復旧結果 - お客様優先データ]");
    println!("  該当ファイル: {} 件", result.report.priority_count());
    println!(
        "  品質保証率:   {:.1}%",
        result.report.priority_quality_assurance_rate()
    );
    println!();
    println!("[除外パターン]");
    println!("  Windows / Program Files");
    println!("  $Recycle.Bin / System Volume Information");
    println!("  $ で始まるシステムファイル");
    println!();
    println!("=== R-STUDIO 風業務フロー対応完成 ===");

    // 業務的アサーション: 全件復旧 + 優先データは PNG 4 件のみ。
    // ntfs_mixed_formats は 15 件 (PNG 4 / JPEG 3 / PDF 4 / GIF 1 / BMP 1 / DOCX 1 / xyz 1)。
    assert_eq!(result.report.recovered.len(), 15);
    assert_eq!(result.report.priority_count(), 4);

    // 「全体 ≠ 優先データ」を機械的に保証 (Wishlist が "全体" の filter ではないことの担保)。
    assert!(result.report.priority_count() < result.report.recovered.len());

    // 品質保証率 > 0%: ntfs_mixed_formats は PNG/JPEG/PDF/GIF/BMP/DOCX を含み、
    // Chunk 18-19 で実装した 9 validator が機能していることの回帰防止。
    assert!(
        result.report.quality_assurance_rate() > 0.0,
        "validator が機能していれば品質保証率は 0% 超 (Chunk 19 で Valid 10/15 実証済み)"
    );
}

/// Chunk 23.7.1: 業務メンバー向けの永続化デモ。
///
/// `product_demo_phase_1_5_business_aligned` と同じ業務シナリオ
/// (ntfs_mixed_formats + PNG 優先) を実行し、生成物を workspace ルートの
/// `target/chunk23_7-samples/` に永続化する。`TempDir` を使う通常テストと違い、
/// 実行後もディレクトリが残るので Word / Notepad / ブラウザ / Excel で
/// 実視覚確認できる。
///
/// 出力構造:
/// ```text
/// target/chunk23_7-samples/
///   ├ delivery/                ← 納品 HDD 相当 (CaseOutput が下を作る)
///   │   └ 260522-04/
///   │       ├ 復旧データ/
///   │       │   ├ 通常ファイル/   ← 生存 user file
///   │       │   └ 削除ファイル/   ← 削除エントリ
///   │       └ レポート/
///   │           ├ 復旧レポート.docx
///   │           ├ 要確認ファイル一覧.txt
///   │           ├ 業務管理レポート.html
///   │           └ report.csv
///   └ internal/                ← 社内保存 (CaseStorage が下を作る)
///       └ 260522-04/case.json
/// ```
///
/// 実行: `cargo test -p dds-case-manager --test business_flow_integration \
///        persist_chunk23_7_demo_reports -- --ignored --nocapture`
///
/// CI からは除外 (`#[ignore]`)。Chunk 20.5 の `persist_chunk20_5_demo_reports` と
/// 同じパターン。
#[test]
#[ignore]
fn persist_chunk23_7_demo_reports() {
    // workspace ルートの target/chunk23_7-samples/ に永続化。
    let mut sample_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    sample_root.push("../../target/chunk23_7-samples");

    // 再実行可能性: 既存ディレクトリは丸ごと削除してから作り直す。
    if sample_root.exists() {
        std::fs::remove_dir_all(&sample_root).expect("remove existing sample dir");
    }
    let internal_root = sample_root.join("internal");
    let delivery_root = sample_root.join("delivery");
    std::fs::create_dir_all(&internal_root).expect("create internal dir");
    std::fs::create_dir_all(&delivery_root).expect("create delivery dir");

    let storage = CaseStorage::with_base_dir(&internal_root);
    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_mixed_formats");
    let wishlist = make_business_demo_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    let result = execute_business_recovery(
        &mut case,
        &delivery_root,
        &mut volume,
        &wishlist,
        &exclusions,
    )
    .expect("execute_business_recovery");
    storage.save(&case).unwrap();

    let sample_root_display = sample_root
        .canonicalize()
        .unwrap_or_else(|_| sample_root.clone());

    println!(
        "\n=== Phase 1.5 Persistent Demo (Chunk 23.7.1) ===\n"
    );
    println!("永続化ルート: {:?}", sample_root_display);
    println!();
    println!("├─ delivery/");
    println!("│   └─ 260522-04/");
    println!(
        "│       ├─ 復旧データ/通常ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.live_files_dir())
    );
    println!(
        "│       ├─ 復旧データ/削除ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.deleted_files_dir())
    );
    println!("│       └─ レポート/");
    println!(
        "│           ├─ 復旧レポート.docx ({} bytes)",
        result.report_paths.customer_docx.metadata().unwrap().len()
    );
    println!(
        "│           ├─ 要確認ファイル一覧.txt ({} bytes)",
        result.report_paths.customer_txt.metadata().unwrap().len()
    );
    println!(
        "│           ├─ 業務管理レポート.html ({} bytes)",
        result.report_paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "│           └─ report.csv ({} bytes)",
        result.report_paths.csv.metadata().unwrap().len()
    );
    println!("└─ internal/");
    let case_json = storage.case_file_path(&case_id);
    println!(
        "    └─ 260522-04/case.json ({} bytes)",
        case_json.metadata().map(|m| m.len()).unwrap_or(0)
    );
    println!();
    println!("業務指標:");
    println!("  全体 該当数:        {} 件", result.report.total_matched);
    println!(
        "  全体 品質保証率:    {:.1}%",
        result.report.quality_assurance_rate()
    );
    println!(
        "  優先データ 該当数:  {} 件",
        result.report.priority_count()
    );
    println!(
        "  優先データ 品質保証率: {:.1}%",
        result.report.priority_quality_assurance_rate()
    );
    println!();
    println!("→ Word / Notepad / ブラウザ / Excel で実際に開いて業務適用性を確認");

    // 業務シナリオが意図通り再現されていることの最低限の防御。
    assert!(result.report_paths.customer_docx.is_file());
    assert!(result.report_paths.customer_txt.is_file());
    assert!(result.report_paths.internal_html.is_file());
    assert!(result.report_paths.csv.is_file());
    assert!(case_json.is_file());
}
