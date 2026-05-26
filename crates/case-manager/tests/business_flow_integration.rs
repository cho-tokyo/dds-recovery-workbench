//! Chunk 23 / Chunk 24a 結合テスト: 業務フロー end-to-end + Phase 1.5 プロダクトデモ。
//!
//! Chunk 24a で「お客様向け納品物の簡素化」「タイムスタンプ保持」を反映:
//!
//! - 納品 HDD: 復旧レポート.docx のみ (TXT / HTML / CSV は社内保存に移動)
//! - 社内保存: 業務管理レポート.html + 復旧詳細.csv (UTF-8 BOM 付き)
//! - 復旧ファイルのタイムスタンプを R-STUDIO 並みに保持 (Windows のみ)
//!
//! 関連 FR: FR-OUT-01〜06, FR-CASE-01〜04, FR-REC-01〜07, FR-REP-01〜05。

mod common;

use tempfile::TempDir;

use dds_case_manager::{execute_business_recovery, CaseId, CaseStorage};
use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use dds_recovery::NoopProgressReporter;
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
fn make_business_demo_wishlist() -> Wishlist {
    Wishlist::new().add(
        Wish::new(WishItem::Extension("png".into()), "お客様優先: PNG 画像")
            .with_priority(Priority::High),
    )
}

#[test]
fn full_business_flow_from_case_creation_to_delivery() {
    // Chunk 24a 改訂版: 納品 HDD には .docx のみ、社内保存に HTML / CSV。
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
        &storage,
        &NoopProgressReporter,
    )
    .expect("execute_business_recovery");

    storage.save(&case).unwrap();

    // 納品 HDD 側のツリーチェック (Chunk 24a 改訂版)。
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

    // Chunk 24a: 旧ファイル群は納品 HDD のレポートディレクトリには存在しない。
    assert!(
        !reports_dir.join("破損疑いファイル一覧.txt").exists(),
        "破損疑いファイル一覧.txt は納品 HDD から削除済み (Chunk 24a)"
    );
    assert!(
        !reports_dir.join("自動確認対象外ファイル一覧.txt").exists(),
        "自動確認対象外ファイル一覧.txt は納品 HDD から削除済み (Chunk 24a)"
    );
    assert!(
        !reports_dir.join("業務管理レポート.html").exists(),
        "業務管理レポート.html は社内保存へ移動済み (Chunk 24a)"
    );
    assert!(
        !reports_dir.join("report.csv").exists(),
        "report.csv は社内保存に移動 (Chunk 24a で 復旧詳細.csv にリネーム)"
    );

    // 社内保存側のツリーチェック (Chunk 24a 新規)。
    let internal_case_dir = internal_storage.path().join("260522-04");
    assert!(
        internal_case_dir.join("業務管理レポート.html").is_file(),
        "業務管理レポート.html は社内保存に作成される"
    );
    assert!(
        internal_case_dir.join("復旧詳細.csv").is_file(),
        "復旧詳細.csv は社内保存に作成される"
    );

    // CSV BOM 確認 (実機ドライランフィードバック ④ 対応)。
    let csv_bytes = std::fs::read(internal_case_dir.join("復旧詳細.csv")).unwrap();
    assert!(csv_bytes.len() >= 3);
    assert_eq!(
        &csv_bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "復旧詳細.csv 先頭 3 バイトは UTF-8 BOM"
    );

    // 復旧件数 / 振り分けチェック。
    assert_eq!(result.report.total_matched, 30);
    assert_eq!(result.report.recovered.len(), 30);
    assert_eq!(result.report.failed.len(), 0);

    let live_count = count_files_recursive(&result.case_output.live_files_dir());
    let deleted_count = count_files_recursive(&result.case_output.deleted_files_dir());
    assert_eq!(live_count, 25, "通常ファイル/ count");
    assert_eq!(deleted_count, 5, "削除ファイル/ count");

    // case.json 再読込で全フィールド保持。
    let loaded = storage.load(&case_id).unwrap();
    assert!(loaded.output_dir.is_some());
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
    assert!(loaded.wishlist.is_some());

    // BusinessReportPaths が CaseOutput / CaseStorage と一致。
    assert_eq!(
        result.report_paths.customer_docx,
        result.case_output.customer_docx_path()
    );
    assert_eq!(
        result.report_paths.internal_html,
        result
            .case_output
            .internal_html_path_in_storage(storage.base_dir())
    );
    assert_eq!(
        result.report_paths.csv,
        result.case_output.csv_path_in_storage(storage.base_dir())
    );
}

#[test]
fn business_reports_separated_between_delivery_and_internal() {
    // Chunk 24a: 納品 HDD と社内保存の分離を機械検証する結合テスト。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_mixed_formats");
    let wishlist = make_business_demo_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    let _ = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &NoopProgressReporter,
    )
    .expect("execute_business_recovery");

    // 納品 HDD: 復旧レポート.docx のみ存在。
    let delivery_reports = delivery_drive.path().join("260522-04").join("レポート");
    assert!(delivery_reports.join("復旧レポート.docx").is_file());
    // 旧 4 ファイル不在を機械検証。
    assert!(!delivery_reports.join("業務管理レポート.html").exists());
    assert!(!delivery_reports.join("report.csv").exists());
    assert!(!delivery_reports.join("復旧詳細.csv").exists());
    assert!(!delivery_reports.join("破損疑いファイル一覧.txt").exists());
    assert!(!delivery_reports
        .join("自動確認対象外ファイル一覧.txt")
        .exists());

    // 社内保存: HTML + CSV のみ。
    let internal_case = internal_storage.path().join("260522-04");
    assert!(internal_case.join("業務管理レポート.html").is_file());
    assert!(internal_case.join("復旧詳細.csv").is_file());

    // CSV 先頭 3 バイトが UTF-8 BOM。
    let csv_bytes = std::fs::read(internal_case.join("復旧詳細.csv")).unwrap();
    assert_eq!(&csv_bytes[..3], &[0xEF, 0xBB, 0xBF]);
}

#[cfg(windows)]
#[test]
fn recovered_files_preserve_original_timestamps() {
    // Chunk 24a Part D: タイムスタンプ保持の end-to-end 検証 (R-STUDIO 並み)。
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    // ntfs_with_5_deletions_small: $STANDARD_INFORMATION 由来のタイムスタンプを持つ
    // user file が複数件含まれる業務代表フィクスチャ。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = make_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    // 復旧前に NtfsFile の modified タイムスタンプを記録 (apply_timestamps の前後の整合確認)。
    use std::collections::HashMap;
    let mut expected_modified: HashMap<u64, chrono::DateTime<chrono::Utc>> = HashMap::new();
    let files: Vec<_> = volume.iter_files().filter_map(Result::ok).collect();
    for f in &files {
        if f.is_user_file() && !f.is_directory {
            if let Some(m) = f.modified {
                expected_modified.insert(f.record_index, m);
            }
        }
    }

    // 復旧実行。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &NoopProgressReporter,
    )
    .expect("execute_business_recovery");

    // 復旧後のファイルの modified time が、元 NtfsFile の modified と (秒精度で) 一致。
    let mut matched = 0;
    for entry in &result.report.recovered {
        // source_id "NTFS#N" から record_index を抽出。
        let rec_index: u64 = entry
            .source_id
            .strip_prefix("NTFS#")
            .and_then(|s| s.parse().ok())
            .expect("source_id should be NTFS#<index>");

        let expected = match expected_modified.get(&rec_index) {
            Some(m) => *m,
            None => continue, // modified が None だったエントリは skip 対象 (タイムスタンプ未適用)。
        };

        let metadata = std::fs::metadata(&entry.output_path).expect("recovered file exists");
        let actual_mod = metadata.modified().expect("modified time");
        let actual_dt: chrono::DateTime<chrono::Utc> = actual_mod.into();

        // 秒精度で比較 (Windows / NTFS 100ns 精度のまるめ吸収)。
        assert_eq!(
            actual_dt.timestamp(),
            expected.timestamp(),
            "recovered file modified time mismatch for {}: expected={}, actual={}",
            entry.output_path.display(),
            expected,
            actual_dt
        );
        matched += 1;
    }

    // 少なくとも 1 件は modified が Some で、タイムスタンプ書き戻し検証が実施されたこと。
    assert!(
        matched > 0,
        "at least one file must have modified timestamp preserved"
    );
}

#[test]
fn product_demo_phase_1_5_final() {
    // Chunk 24a 改訂版: Phase 1.5 完成最終デモ。
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
        &storage,
        &NoopProgressReporter,
    )
    .expect("execute_business_recovery");
    storage.save(&case).unwrap();

    println!("\n=== Phase 1.5 完成 Demo (Chunk 24a) ===\n");
    println!("案件番号: {}", case.case_id);
    println!("Wishlist: お客様優先: PNG 画像");
    println!();
    println!("[復旧結果 - 全体]");
    println!("  該当ファイル: {} 件", result.report.total_matched);
    println!("  復旧成功:     {} 件", result.report.recovered.len());
    println!(
        "  復旧データ量: {}",
        dds_core::format::format_bytes(result.report.total_bytes_written())
    );
    println!();
    println!("[復旧結果 - お客様優先データ]");
    println!("  該当ファイル: {} 件", result.report.priority_count());
    println!();
    println!(
        "[納品 HDD ({}) - お客様向け]",
        delivery_drive.path().display()
    );
    println!("  └─ 260522-04/");
    println!(
        "      ├─ 復旧データ/通常ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.live_files_dir())
    );
    println!(
        "      ├─ 復旧データ/削除ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.deleted_files_dir())
    );
    println!(
        "      └─ レポート/復旧レポート.docx ({} bytes)",
        result.report_paths.customer_docx.metadata().unwrap().len()
    );
    println!();
    println!(
        "[社内保存 ({}) - CS 業務管理用]",
        internal_storage.path().display()
    );
    println!(
        "  └─ 260522-04/業務管理レポート.html ({} bytes)",
        result.report_paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "  └─ 260522-04/復旧詳細.csv ({} bytes、UTF-8 BOM 付)",
        result.report_paths.csv.metadata().unwrap().len()
    );
    println!();
    println!("=== Phase 1.5 完成 (Chunk 24a 実機ドライランフィードバック反映) ===");

    // 機械検証: 3 ファイル全て生成。
    assert!(result.report_paths.customer_docx.is_file());
    assert!(result.report_paths.internal_html.is_file());
    assert!(result.report_paths.csv.is_file());
    // 納品 HDD には docx のみ。
    let reports_dir = delivery_drive.path().join("260522-04").join("レポート");
    assert!(reports_dir.join("復旧レポート.docx").is_file());
    assert!(!reports_dir.join("業務管理レポート.html").exists());
    assert!(!reports_dir.join("復旧詳細.csv").exists());
}

#[test]
#[ignore]
fn persist_chunk24a_demo_reports() {
    // Chunk 24a: 業務メンバー向けの永続化デモ。
    // `cargo test -p dds-case-manager --test business_flow_integration \
    //  persist_chunk24a_demo_reports -- --ignored --nocapture`
    let mut sample_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    sample_root.push("../../target/chunk24a-samples");

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
        &storage,
        &NoopProgressReporter,
    )
    .expect("execute_business_recovery");
    storage.save(&case).unwrap();

    let sample_root_display = sample_root
        .canonicalize()
        .unwrap_or_else(|_| sample_root.clone());

    println!("\n=== Phase 1.5 Final Persistent Demo (Chunk 24a) ===\n");
    println!("永続化ルート: {:?}", sample_root_display);
    println!();
    println!("├─ delivery/ (納品 HDD 相当)");
    println!("│   └─ 260522-04/");
    println!(
        "│       ├─ 復旧データ/通常ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.live_files_dir())
    );
    println!(
        "│       ├─ 復旧データ/削除ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.deleted_files_dir())
    );
    println!(
        "│       └─ レポート/復旧レポート.docx ({} bytes)",
        result.report_paths.customer_docx.metadata().unwrap().len()
    );
    println!("└─ internal/ (社内保存相当)");
    let case_json = storage.case_file_path(&case_id);
    println!(
        "    └─ 260522-04/case.json ({} bytes)",
        case_json.metadata().map(|m| m.len()).unwrap_or(0)
    );
    println!(
        "    └─ 260522-04/業務管理レポート.html ({} bytes)",
        result.report_paths.internal_html.metadata().unwrap().len()
    );
    println!(
        "    └─ 260522-04/復旧詳細.csv ({} bytes, UTF-8 BOM 付き)",
        result.report_paths.csv.metadata().unwrap().len()
    );
    println!();
    println!("→ Word / ブラウザ / Excel で開いて業務適用性を確認 (BOM で日本語化け解消)");

    assert!(result.report_paths.customer_docx.is_file());
    assert!(result.report_paths.internal_html.is_file());
    assert!(result.report_paths.csv.is_file());
    assert!(case_json.is_file());
}

/// Chunk 24b: ConsoleProgressReporter の stderr 出力サンプルを業務メンバーが確認するための demo。
///
/// 実行: `cargo test -p dds-case-manager --test business_flow_integration \
///        demo_chunk24b_console_progress_output -- --ignored --nocapture 2>&1`
#[test]
#[ignore]
fn demo_chunk24b_console_progress_output() {
    use std::time::Duration;
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = make_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    // テスト用に間隔を 1ms に短縮して、フィクスチャでも毎ファイル出力させる。
    let progress = dds_recovery::ConsoleProgressReporter::with_interval(Duration::from_millis(1));

    eprintln!("\n=== Chunk 24b ConsoleProgressReporter Sample Output ===");
    let _ = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &progress,
    )
    .expect("execute_business_recovery");
    eprintln!("=== Sample End ===\n");
}

/// Chunk 24b: パフォーマンス計測 demo。`--ignored --nocapture` で実行。
///
/// フィクスチャ `ntfs_mixed_formats` は 15 ファイル / 数 KB と非常に小さく、並列化
/// オーバーヘッドが目立つ可能性が高い。**ここでは絶対速度の到達は目的ではなく**、
/// 「並列化された recover_files が動作している」「MB/s 計測が出る」ことを担保する。
/// 実機ベンチマークは Chouさんが Chunk 24b 完了後に手動で実施する。
///
/// 実行:
/// ```text
/// cargo test --release -p dds-case-manager --test business_flow_integration \
///   perf_demo_chunk24b_recovery_speed -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn perf_demo_chunk24b_recovery_speed() {
    use std::time::Instant;

    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();

    let mut volume = open_fixture("ntfs_mixed_formats");
    let wishlist = make_business_demo_wishlist();
    let exclusions = ExclusionList::default_system_exclusions();

    let progress = NoopProgressReporter; // 進捗表示は別 demo で確認
    let start = Instant::now();
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &progress,
    )
    .expect("execute_business_recovery");
    let elapsed = start.elapsed();

    let total_bytes = result.report.total_bytes_written();
    let mb_per_sec = (total_bytes as f64 / 1_048_576.0) / elapsed.as_secs_f64().max(0.001);
    let worker_count = num_cpus::get().clamp(1, 4);

    println!("\n=== Chunk 24b Performance Demo ===");
    println!(
        "ワーカー数:   {} (CPU コア数 = {})",
        worker_count,
        num_cpus::get()
    );
    println!("ファイル数:   {} 件", result.report.recovered.len());
    println!(
        "データ量:    {} bytes ({:.2} MB)",
        total_bytes,
        total_bytes as f64 / 1_048_576.0
    );
    println!("経過時間:    {:.3} 秒", elapsed.as_secs_f64());
    println!("速度:        {:.1} MB/s", mb_per_sec);
    println!();
    println!("注: ベースライン (Chunk 24a 実機): 約 4 MB/s");
    println!("    目標 (Chunk 24b):              50-100 MB/s (実機で検証)");
    println!("    本デモはフィクスチャが小さいため絶対値は参考程度。");
    println!("=== Performance Demo End ===\n");

    assert!(
        !result.report.recovered.is_empty(),
        "should recover at least one file"
    );
}
