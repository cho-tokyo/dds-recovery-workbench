//! Chunk 22 結合テスト: 実フィクスチャ（NTFS イメージ）を用いた診断エンジン E2E。
//!
//! ここでカバーする業務シナリオ:
//! 1. 削除フィクスチャ → `Symptom::Deleted` + DeletedFileStats 構築
//! 2. 健康フィクスチャ → `Symptom::None` (Phase 1 ヒューリスティック)
//! 3. プロダクトデモ → CRM 貼り付けテキスト全文を println で出力
//! 4. case.json 統合 → 診断結果を CaseStorage 経由で往復保存

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
fn diagnose_deleted_fixture_produces_deleted_symptom() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    // 主症状: 削除が検出されること（フィクスチャ規模では Formatted との Mixed も許容）
    let label = report.symptom.primary_label();
    assert!(
        label.contains("削除") || label.contains("複合"),
        "expected 削除 label, got: {}",
        label
    );
    assert_eq!(report.file_stats.deleted_files, 5);
    assert!(report.deleted_file_stats.is_some());
    assert_eq!(report.deleted_file_stats.as_ref().unwrap().total_count, 5);
}

#[test]
fn diagnose_healthy_fixture_produces_no_deletions() {
    let mut volume = open_volume("ntfs_healthy_small");
    let case_id = CaseId::parse("260522-01").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    // 健康フィクスチャ: 削除エントリ 0 件
    assert_eq!(report.file_stats.deleted_files, 0);
    assert!(report.deleted_file_stats.is_none());
    assert!(
        !report.anomalies.has_any_anomaly(),
        "healthy fixture should have no fs anomaly, got: {:?}",
        report.anomalies
    );
}

#[test]
fn product_demo_diagnose_with_crm_text() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let crm_text = report.to_crm_text();

    println!("\n=== Phase 1.5 Diagnostic Engine Demo (Chunk 22) ===\n");
    println!("案件: 260522-04");
    println!("診断時間: {} 秒", report.duration_secs);
    println!("主症状: {}", report.symptom.primary_label());
    println!();
    println!("--- CRM 貼り付けテキスト ---");
    println!("{}", crm_text);
    println!("--- ここまで ---");
    println!();
    println!("=== 診断エンジン完成 ===");

    // 基本検証
    assert!(crm_text.contains("260522-04"));
    assert!(crm_text.contains("削除"));
    assert!(crm_text.contains("【ファイル統計】"));
    assert!(crm_text.contains("=== 診断完了 ==="));
    assert!(report.duration_secs < 60);
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
    let symptom = reloaded.diagnostic_input.symptom.expect("symptom present");
    // 主症状ラベルに「削除」を含むことだけ確認（厳密なバリアント比較はフィクスチャ依存）
    assert!(
        symptom.primary_label().contains("削除") || symptom.primary_label().contains("複合"),
        "expected 削除 in reloaded symptom, got: {}",
        symptom.primary_label()
    );
    assert_eq!(reloaded.diagnostic_input.deleted_files, 5);
}
