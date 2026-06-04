//! Chunk 22 / 22.6 結合テスト: 実フィクスチャを用いた診断エンジン E2E。
//!
//! Chunk 22.6 で症状判定ロジックを完全削除し、事実報告型に再設計済み。
//!
//! ここでカバーする業務シナリオ:
//! 1. 削除フィクスチャ → DeletedFileStats が 5 件で構築される
//! 2. 健康フィクスチャ → 削除 0 件、FS 異常 0 件
//! 3. プロダクトデモ → CRM 貼り付けテキスト全文を println で出力
//! 4. case.json 統合 → 診断結果を CaseStorage 経由で往復保存
//! 5. 業務 CRITICAL: 削除案件で「フォーマット (複合)」誤判定が出ないこと

mod common;

use common::{decompress_fixture, make_image_reader};

use dds_case_manager::{CaseId, CaseStorage};
use dds_diagnostic::DiagnosticEngine;
use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};
use tempfile::TempDir;

fn open_volume(
    fixture: &str,
) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = decompress_fixture(fixture);
    let cluster_size = u64::from(
        parse_boot_sector(&img[..512])
            .expect("parse boot sector")
            .cluster_size_bytes(),
    );
    NtfsVolume::open(make_image_reader(img, cluster_size)).expect("open volume")
}

#[test]
fn diagnose_deleted_fixture_detects_5_deleted_entries() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    // 業務的事実: 削除エントリが 5 件検出されること。
    assert_eq!(report.file_stats.deleted_files, 5);
    let stats = report
        .deleted_file_stats
        .as_ref()
        .expect("deleted stats present");
    assert_eq!(stats.total_count, 5);
}

#[test]
fn diagnose_healthy_fixture_has_no_deletions_or_anomalies() {
    let mut volume = open_volume("ntfs_healthy_small");
    let case_id = CaseId::parse("260522-01").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    assert_eq!(report.file_stats.deleted_files, 0);
    assert!(report.deleted_file_stats.is_none());
    assert!(
        !report.filesystem_findings.has_any_issue(),
        "healthy fixture should have no fs findings issue, got: {:?}",
        report.filesystem_findings
    );
}

#[test]
fn diagnose_populates_filesystem_findings() {
    // DiagnosticEngine 実行後、filesystem_findings が正しく埋まることを確認。
    let mut volume = open_volume("ntfs_healthy_small");
    let case_id = CaseId::parse("260522-01").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    // 健康なフィクスチャでは signature_valid + boot_sector_ok が立ち、件数は 0。
    let findings = &report.filesystem_findings;
    assert!(findings.signature_valid);
    assert!(findings.boot_sector_ok);
    assert_eq!(findings.mft_corrupted_count, 0);
    assert_eq!(findings.invalid_runlist_count, 0);
}

#[test]
fn product_demo_diagnose_with_crm_text() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let crm_text = report.to_crm_text();

    println!("\n=== Phase 1.5 Diagnostic Engine Demo (Chunk 22.6) ===\n");
    println!("案件: 260522-04");
    println!("診断時間: {} 秒", report.duration_secs);
    println!("削除エントリ数: {} 件", report.file_stats.deleted_files);
    println!();
    println!("--- CRM 貼り付けテキスト ---");
    println!("{}", crm_text);
    println!("--- ここまで ---");
    println!();
    println!("=== 診断エンジン完成 ===");

    // 基本検証
    assert!(crm_text.contains("260522-04"));
    assert!(crm_text.contains("【ファイル統計】"));
    assert!(crm_text.contains("【ファイルシステムの破損】"));
    assert!(crm_text.contains("【MFT エントリ統計】"));
    assert!(crm_text.contains("=== 診断完了 ==="));
    assert!(report.duration_secs < 60);
}

#[test]
fn product_demo_deleted_case_no_format_misdetection() {
    // 業務 CRITICAL: 削除案件で「フォーマット (複合)」のような誤判定が出ないこと。
    // Chunk 22 で実発生した回帰を機械的に防止する。
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let crm = report.to_crm_text();

    assert!(
        !crm.contains("フォーマット (複合)"),
        "Chunk 22 で誤判定された 'フォーマット (複合)' が再発: {}",
        crm
    );
    assert!(!crm.contains("主症状: フォーマット"));
    assert!(!crm.contains("主症状:"));
    assert!(!crm.contains("【症状判定】"));

    // 新セクションが期待通り存在する。
    assert!(crm.contains("【ファイルシステムの破損】"));
    assert!(crm.contains("【MFT エントリ統計】"));
}

#[test]
fn diagnose_result_can_be_saved_to_case() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());

    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).expect("create_new");

    // 診断実行
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let report = DiagnosticEngine::diagnose(&mut volume, case_id.clone()).expect("diagnose");

    // case に反映
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(&case).expect("save");

    // 再読み込みで保持されている
    let reloaded = storage.load(&case_id).expect("load");
    assert_eq!(
        reloaded.diagnostic_input.filesystem_type,
        Some("NTFS".into())
    );
    let findings = reloaded
        .diagnostic_input
        .filesystem_findings
        .expect("findings present");
    assert!(findings.signature_valid);
    assert!(findings.boot_sector_ok);
    assert_eq!(reloaded.diagnostic_input.deleted_files, 5);
}

// ---- Chunk 24d-4-1: 業務的診断指標の結合テスト ----

#[test]
fn diagnose_healthy_ntfs_returns_easy_difficulty() {
    // 業務シナリオ: 健康な NTFS ボリュームでは復旧難易度 = 易、成功率 >= 90%。
    let mut volume = open_volume("ntfs_healthy_small");
    let case_id = CaseId::parse("260522-01").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let difficulty = report.recovery_difficulty.expect("difficulty present");
    assert_eq!(
        difficulty,
        dds_case_manager::RecoveryDifficulty::Easy,
        "healthy fixture should be Easy, got {:?}",
        difficulty
    );

    let rate = report.success_rate.as_ref().expect("rate present");
    assert!(
        rate.overall_rate >= 90,
        "healthy fixture should have >= 90% success rate, got {}%",
        rate.overall_rate
    );

    // 業務指標がすべて埋まっていることを確認
    assert!(report.dirty_bit.is_some());
    assert!(report.log_file_status.is_some());
    assert!(report.bitlocker.is_some());
    assert!(report.file_estimation.is_some());
}

#[test]
fn crm_text_includes_business_diagnostic_info() {
    // 業務シナリオ: 営業が CRM に貼り付けるテキストに業務サマリと技術詳細が含まれる。
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let crm_text = report.to_crm_text();

    // 業務サマリセクション
    assert!(
        crm_text.contains("【診断結果 - 業務サマリ】"),
        "業務サマリ section missing: {}",
        crm_text
    );
    assert!(crm_text.contains("復旧難易度"), "difficulty missing");
    assert!(crm_text.contains("成功率"), "success rate missing");

    // 技術詳細セクション
    assert!(
        crm_text.contains("【診断結果 - 技術詳細】"),
        "技術詳細 section missing"
    );
    assert!(crm_text.contains("Dirty Bit"), "Dirty Bit row missing");
    assert!(crm_text.contains("$LogFile"), "$LogFile row missing");
    assert!(crm_text.contains("BitLocker"), "BitLocker row missing");

    // 業務原則: 「受注不可」のような決めつけ表現は含まれないこと
    assert!(
        !crm_text.contains("受注不可"),
        "業務原則違反: 「受注不可」表現が含まれる"
    );
    assert!(
        !crm_text.contains("対応困難"),
        "業務原則違反: 「対応困難」表現が含まれる"
    );
    assert!(
        !crm_text.contains("復旧不可能"),
        "業務原則違反: 「復旧不可能」表現が含まれる"
    );
}

#[test]
fn diagnose_persists_business_indicators_to_case_json() {
    // 業務シナリオ: 業務指標が case.json (DiagnosticInput) に保存され、再 load で復元される。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());

    let case_id = CaseId::parse("260522-01").unwrap();
    let mut case = storage.create_new(case_id.clone()).expect("create_new");

    let mut volume = open_volume("ntfs_healthy_small");
    let report = DiagnosticEngine::diagnose(&mut volume, case_id.clone()).expect("diagnose");
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(&case).expect("save");

    let reloaded = storage.load(&case_id).expect("load");
    let di = &reloaded.diagnostic_input;
    assert!(di.dirty_bit.is_some());
    assert!(di.log_file_status.is_some());
    assert!(di.bitlocker.is_some());
    assert!(di.file_estimation.is_some());
    assert!(di.recovery_difficulty.is_some());
    assert!(di.success_rate.is_some());
}
