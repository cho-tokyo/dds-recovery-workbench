//! Chunk 21 結合テスト: 案件管理の業務シナリオシミュレーション。
//!
//! Chunk 22.6 で旧症状判定型を削除し、`FilesystemFindings` (事実のみ) に置換。
//!
//! 1. 案件のライフサイクル完全シミュレーション
//!    create → 診断データ追加 → Wishlist 追加 → RecoveryReportSummary 追加 →
//!    save → load → 全フィールド保持確認
//! 2. プロダクトデモテスト
//!    1 日分の業務（3 案件を順次受領）→ list_all → println フォーマット出力
//!
//! 関連 FR: FR-CASE-01 ~ FR-CASE-04, FR-DIAG-06。

use std::path::PathBuf;

use chrono::Utc;
use tempfile::TempDir;

use dds_case_manager::{Case, CaseId, CaseStorage, FilesystemFindings, RecoveryReportSummary};
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};

#[test]
fn full_case_lifecycle_create_diagnose_recover() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case_id = CaseId::parse("260522-04").unwrap();

    // Step 1: 案件作成
    let mut case = storage.create_new(case_id.clone()).unwrap();
    assert_eq!(case.case_id, case_id);

    // Step 2: 診断結果反映（Chunk 22 を想定して手動で埋める）
    case.diagnostic_input.diagnosed_at = Some(Utc::now());
    case.diagnostic_input.duration_secs = Some(42);
    case.diagnostic_input.filesystem_type = Some("NTFS".into());
    case.diagnostic_input.filesystem_findings = Some(FilesystemFindings {
        signature_valid: true,
        mft_corrupted_count: 0,
        invalid_runlist_count: 0,
        boot_sector_ok: true,
        other_issues: vec![],
    });
    case.diagnostic_input.total_files = 12847;
    case.diagnostic_input.deleted_files = 234;
    case.diagnostic_input.total_size_bytes = 100_000_000_000;
    case.diagnostic_input.notes = "Shift+Delete による削除と推定".into();
    storage.save(&case).unwrap();

    // Step 3: Wishlist 追加
    case.wishlist = Some(
        Wishlist::new().add(
            Wish::new(WishItem::Extension("docx".into()), "Word ファイル全部")
                .with_priority(Priority::High),
        ),
    );
    storage.save(&case).unwrap();

    // Step 4: 復旧結果サマリ追加
    let now = Utc::now();
    case.recovery_report_summary = Some(RecoveryReportSummary {
        started_at: now,
        finished_at: now,
        duration_ms: 8_500,
        total_matched: 230,
        recovered_count: 225,
        failed_count: 3,
        skipped_count: 2,
        validated_count: 220,
        invalid_count: 4,
        uncertain_count: 1,
        total_bytes_written: 850_000_000,
        recovery_success_rate: 0.978,
        quality_assurance_rate: 0.978,
    });
    case.output_dir = Some(PathBuf::from("G:\\260522-04"));
    storage.save(&case).unwrap();

    // Step 5: 再読み込みで全情報保持
    let reloaded = storage.load(&case_id).unwrap();
    assert_eq!(reloaded.case_id, case_id);
    assert_eq!(
        reloaded.diagnostic_input.filesystem_type,
        Some("NTFS".into())
    );
    assert_eq!(reloaded.diagnostic_input.total_files, 12847);
    assert_eq!(reloaded.diagnostic_input.deleted_files, 234);
    let findings = reloaded
        .diagnostic_input
        .filesystem_findings
        .expect("findings present");
    assert!(findings.signature_valid);
    assert!(findings.boot_sector_ok);
    assert!(!findings.has_any_issue());
    assert!(reloaded.wishlist.is_some());
    assert_eq!(reloaded.wishlist.as_ref().unwrap().len(), 1);
    assert!(reloaded.recovery_report_summary.is_some());
    let summary = reloaded.recovery_report_summary.as_ref().unwrap();
    assert_eq!(summary.recovered_count, 225);
    assert_eq!(summary.total_bytes_written, 850_000_000);
    assert_eq!(reloaded.output_dir, Some(PathBuf::from("G:\\260522-04")));
}

#[test]
fn product_demo_case_management_basics() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());

    // 1 日分の業務シミュレーション: 3 案件を順次受領
    for seq in 1..=3 {
        let case_id = CaseId::parse(&format!("260522-{:02}", seq)).unwrap();
        let case: Case = storage.create_new(case_id).unwrap();
        println!("案件 {} 作成", case.case_id);
    }

    let list = storage.list_all().unwrap();

    println!();
    println!("=== Phase 1.5 Case Management Demo (Chunk 21) ===");
    println!();
    println!("保存先: {:?}", temp.path());
    println!("登録案件数: {}", list.len());
    println!();
    println!("案件一覧:");
    for case_id in &list {
        let case = storage.load(case_id).unwrap();
        println!(
            "  {} (作成: {})",
            case.case_id,
            case.created_at.format("%Y-%m-%d %H:%M")
        );
    }
    println!();
    println!("=== Case Manager 基盤完成 ===");

    assert_eq!(list.len(), 3);
    assert_eq!(list[0].as_str(), "260522-01");
    assert_eq!(list[1].as_str(), "260522-02");
    assert_eq!(list[2].as_str(), "260522-03");
}
