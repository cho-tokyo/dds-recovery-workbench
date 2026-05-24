//! Chunk 22.5 結合テスト: 削除フィクスチャに対する復旧可能性推定 (High/Medium/Low)。
//!
//! 業務シナリオ:
//! 1. 削除フィクスチャ → DiagnosticEngine が削除エントリ 5 件に対し
//!    `RecoverabilityEstimate { high, medium, low }` を埋める
//! 2. プロダクトデモ → CRM 貼り付けテキスト「復旧可能性 (推定)」セクションを println で出力
//!
//! 関連 FR: FR-DIAG-07, FR-DIAG-08。

mod common;

use common::{decompress_fixture, make_image_reader};

use dds_case_manager::CaseId;
use dds_diagnostic::DiagnosticEngine;
use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};

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
fn diagnose_5_deletions_estimates_recoverability() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let deleted_stats = report
        .deleted_file_stats
        .as_ref()
        .expect("deleted stats present");
    let est = deleted_stats
        .recoverability_estimate
        .as_ref()
        .expect("recoverability estimate should be present");

    // 5 件すべて何らかのカテゴリに分類されている (build_file が全件成功する前提)。
    let total = est.high_confidence + est.medium_confidence + est.low_confidence;
    assert_eq!(
        total, 5,
        "all 5 deleted entries must be categorized, got H={} M={} L={}",
        est.high_confidence, est.medium_confidence, est.low_confidence
    );

    // 小 TXT (~50 B) は確実に resident → 全件 High と想定。
    assert_eq!(
        est.high_confidence, 5,
        "small TXT fixtures should all be resident -> High, got H={} M={} L={}",
        est.high_confidence, est.medium_confidence, est.low_confidence
    );
}

#[test]
fn product_demo_diagnose_with_recoverability_estimate() {
    let mut volume = open_volume("ntfs_with_5_deletions_small");
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).expect("diagnose");

    let crm_text = report.to_crm_text();

    println!("\n=== Phase 1.5 Recoverability Estimate Demo (Chunk 22.5) ===\n");
    println!("案件: 260522-04");
    println!();
    if let Some(deleted) = &report.deleted_file_stats {
        if let Some(est) = &deleted.recoverability_estimate {
            println!("削除エントリ数: {}", deleted.total_count);
            println!("復旧可能性:");
            println!("  高: {} 件", est.high_confidence);
            println!("  中: {} 件", est.medium_confidence);
            println!("  低: {} 件", est.low_confidence);
        }
    }
    println!();
    println!("--- CRM 貼り付けテキスト (抜粋) ---");
    if let Some(idx) = crm_text.find("復旧可能性 (推定)") {
        // 抜粋は最大 500 文字 (バイトオフセットでなくバイト末端で char_indices を見る方が
        // 安全だが、出力済みテキストは ASCII + JP 多言語混在で boundary 越えに弱い。
        // 単純に find のあとは "\n=== 診断完了" まで切り出す)。
        let end = crm_text[idx..]
            .find("=== 診断完了 ===")
            .map(|e| idx + e)
            .unwrap_or(crm_text.len());
        println!("{}", &crm_text[idx..end]);
    }
    println!("--- ここまで ---");
    println!();
    println!("=== 復旧可能性推定機能完成 ===");

    assert!(
        crm_text.contains("復旧可能性 (推定):"),
        "CRM text missing recoverability section"
    );
    assert!(crm_text.contains("高 (確実復旧可能)"));
    assert!(crm_text.contains("中 (部分復旧の可能性)"));
    assert!(crm_text.contains("低 (メタデータのみ)"));
}
