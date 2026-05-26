//! Chunk 22 / 22.6: CRM 貼り付け用の業務日本語テキスト生成。
//!
//! 業務シナリオ:
//! DDS の CS 担当者が CRM の備考欄に「論理診断結果」を貼り付けるためのテキスト。
//! HDD 接続 → 1 コマンド実行 → 数十秒後に画面表示されたテキストをそのままコピペできる。
//!
//! Chunk 22.6: 「【症状判定】」セクションを完全削除し、業務フローに整合させた。
//! Workbench は事実報告のみで、症状判定は CS / CRM の責務。
//!
//! 生成セクション（出力順、Chunk 22.6 改訂後）:
//! 1. 案件番号 + 診断日時 + 所要時間
//! 2. 【ハードウェア】(モデル / シリアル / 容量)
//! 3. 【ファイルシステム】(種別 / シリアル / クラスタサイズ / 使用率)
//! 4. 【ファイルシステムの破損】 ← 上位へ移動
//! 5. 【MFT エントリ統計】 ← 新規 (フォーマット案件参考)
//! 6. 【ファイル統計】
//! 7. 【削除エントリの詳細】(削除あり時のみ)
//! 8. 【生存ファイル統計】(参考)
//! 9. 【主なフォルダ】(上位 10)
//! 10. 【物理不良チェック】
//!
//! 関連 FR: FR-DIAG-04 (CRM 貼り付け用テキスト), FR-DIAG-06 (事実ベースの報告)。

use std::fmt::Write;

use dds_core::format::format_bytes;

use crate::report::{DiagnosticReport, FormatCount};

/// [`DiagnosticReport`] から CRM 貼り付け用業務テキストを生成する。
///
/// 戻り値はそのままクリップボードコピー / .txt 保存が可能な完全独立な文字列。
/// 改行は LF（Windows でも CRM 上で問題なく表示される業務実績あり）。
pub fn render(report: &DiagnosticReport) -> String {
    let mut s = String::with_capacity(2048);

    // 1. ヘッダ
    let _ = writeln!(s, "=== 論理診断結果 (案件 {}) ===", report.case_id);
    let _ = writeln!(
        s,
        "診断日時: {}",
        report.diagnosed_at.format("%Y-%m-%d %H:%M")
    );
    let _ = writeln!(s, "診断時間: {} 秒", report.duration_secs);
    let _ = writeln!(s, "※物理診断は別途実施済み");
    let _ = writeln!(s);

    // 2. ハードウェア
    let _ = writeln!(s, "【ハードウェア】");
    if let Some(model) = &report.hardware.model {
        let _ = writeln!(s, "HDD: {}", model);
    }
    if let Some(serial) = &report.hardware.serial {
        let _ = writeln!(s, "シリアル: {}", serial);
    }
    let _ = writeln!(s, "容量: {}", format_bytes(report.hardware.size_bytes));
    let _ = writeln!(s);

    // 3. ファイルシステム
    let _ = writeln!(s, "【ファイルシステム】");
    let _ = writeln!(s, "種類: {}", report.filesystem.fs_type);
    if let Some(vsn) = &report.filesystem.volume_serial {
        let _ = writeln!(s, "ボリュームシリアル: {}", vsn);
    }
    let _ = writeln!(
        s,
        "クラスタサイズ: {} bytes",
        report.filesystem.cluster_size_bytes
    );
    let used_bytes = report
        .filesystem
        .used_clusters
        .saturating_mul(u64::from(report.filesystem.cluster_size_bytes));
    let total_bytes = report
        .filesystem
        .total_clusters
        .saturating_mul(u64::from(report.filesystem.cluster_size_bytes));
    if total_bytes > 0 {
        let usage_pct = (used_bytes as f64) / (total_bytes as f64) * 100.0;
        let _ = writeln!(
            s,
            "使用率: {} / {} ({:.1}%)",
            format_bytes(used_bytes),
            format_bytes(total_bytes),
            usage_pct
        );
    } else {
        let _ = writeln!(s, "使用率: 未計測");
    }
    let _ = writeln!(s);

    // 4. ファイルシステムの破損 (上位へ移動)
    render_filesystem_findings(&mut s, report);

    // 5. MFT エントリ統計 (新規、フォーマット案件の参考)
    let _ = writeln!(s, "【MFT エントリ統計】");
    let _ = writeln!(s, "全エントリ数: {} 件", report.file_stats.total_files);
    let _ = writeln!(
        s,
        "※ フォーマット案件の場合、エントリ数の極端な少なさが参考になります"
    );
    let _ = writeln!(s, "※ 旧 MFT 残存度の計測は Phase 2 で対応予定");
    let _ = writeln!(s);

    // 6. ファイル統計
    let _ = writeln!(s, "【ファイル統計】");
    let _ = writeln!(
        s,
        "全ファイル: {} 件 ({})",
        report.file_stats.total_files,
        format_bytes(report.file_stats.total_size_bytes)
    );
    let _ = writeln!(s, "  - 通常 (生存): {} 件", report.file_stats.live_files);
    let _ = writeln!(s, "  - 削除済み: {} 件", report.file_stats.deleted_files);
    let _ = writeln!(s, "ディレクトリ: {} 件", report.file_stats.directories);
    let _ = writeln!(s);

    // 7. 削除エントリの詳細 (削除あり時のみ)
    if let Some(deleted) = &report.deleted_file_stats {
        let _ = writeln!(s, "【削除エントリの詳細】");
        if !deleted.by_extension.is_empty() {
            let _ = writeln!(s, "形式別:");
            let mut ext_vec: Vec<(&String, &usize)> = deleted.by_extension.iter().collect();
            ext_vec.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            for (ext, count) in ext_vec.iter().take(10) {
                let _ = writeln!(s, "  {}: {} 件", ext.to_uppercase(), count);
            }
        }
        if !deleted.by_folder.is_empty() {
            let _ = writeln!(s);
            let _ = writeln!(s, "フォルダ別:");
            for (folder, count) in deleted.by_folder.iter().take(5) {
                let _ = writeln!(s, "  {}: {} 件", folder, count);
            }
        }
        let _ = writeln!(
            s,
            "推定合計サイズ: {}",
            format_bytes(deleted.estimated_total_size)
        );

        // Chunk 22.5: 復旧可能性 (推定) セクション
        if let Some(est) = &deleted.recoverability_estimate {
            let _ = writeln!(s);
            let _ = writeln!(s, "復旧可能性 (推定):");
            let _ = writeln!(s, "  高 (確実復旧可能): {} 件", est.high_confidence);
            let _ = writeln!(s, "  中 (部分復旧の可能性): {} 件", est.medium_confidence);
            let _ = writeln!(s, "  低 (メタデータのみ): {} 件", est.low_confidence);
            let _ = writeln!(s, "  ※ 判定基準:");
            let _ = writeln!(
                s,
                "    高: ファイル内容が MFT 内に完結、または占有クラスタが上書きされていない"
            );
            let _ = writeln!(
                s,
                "    中: 占有クラスタの一部が他のファイルで上書きされている"
            );
            let _ = writeln!(s, "    低: run-list 解析失敗、または全クラスタが上書き済み");
        }
        let _ = writeln!(s);
    }

    // 8. 生存ファイル統計
    let _ = writeln!(s, "【生存ファイル統計】(参考、主要形式)");
    let mut formats: Vec<(&String, &FormatCount)> = report.format_breakdown.iter().collect();
    formats.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
    if formats.is_empty() {
        let _ = writeln!(s, "  （該当なし）");
    } else {
        for (ext, count) in formats.iter().take(10) {
            let _ = writeln!(
                s,
                "  {}: {} 件 / {}",
                ext.to_uppercase(),
                count.count,
                format_bytes(count.total_size_bytes)
            );
        }
    }
    let _ = writeln!(s);

    // 9. 主なフォルダ
    if !report.folder_breakdown.is_empty() {
        let _ = writeln!(s, "【主なフォルダ】(上位 10)");
        for folder in report.folder_breakdown.iter().take(10) {
            let _ = writeln!(
                s,
                "  {}: {} 件 / {}",
                folder.path,
                folder.file_count,
                format_bytes(folder.total_size_bytes)
            );
        }
        let _ = writeln!(s);
    }

    // 10. 物理不良チェック
    let _ = writeln!(s, "【物理不良チェック】");
    let _ = writeln!(s, "未実施 (Phase 2 で対応予定)");
    let _ = writeln!(s);

    let _ = writeln!(s, "=== 診断完了 ===");

    s
}

/// 【ファイルシステムの破損】セクションを描画する。
///
/// `FilesystemFindings` の各フィールドを業務日本語で書き下す。
/// Chunk 22.6 で導入。
fn render_filesystem_findings(s: &mut String, report: &DiagnosticReport) {
    let findings = &report.filesystem_findings;
    let _ = writeln!(s, "【ファイルシステムの破損】");
    if findings.signature_valid {
        let _ = writeln!(s, "ファイルシステム署名: 正常 (NTFS 認識成功)");
    } else {
        let _ = writeln!(s, "ファイルシステム署名: 異常");
    }
    let _ = writeln!(s, "MFT エントリ破損: {} 件", findings.mft_corrupted_count);
    let _ = writeln!(s, "不正な run-list: {} 件", findings.invalid_runlist_count);
    if findings.boot_sector_ok {
        let _ = writeln!(s, "Boot sector: 正常");
    } else {
        let _ = writeln!(s, "Boot sector: 異常");
    }
    if !findings.other_issues.is_empty() {
        let _ = writeln!(s, "その他の異常: {} 件", findings.other_issues.len());
    }
    let _ = writeln!(s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        FileStatistics, FilesystemInfo, FolderCount, FsAnomalyReport, HardwareInfo,
    };
    use chrono::TimeZone;
    use dds_case_manager::{CaseId, DeletedFileStats, FilesystemFindings};
    use std::collections::BTreeMap;

    fn base_report(with_deleted: bool, findings: FilesystemFindings) -> DiagnosticReport {
        let mut format_breakdown: BTreeMap<String, FormatCount> = BTreeMap::new();
        format_breakdown.insert(
            "txt".into(),
            FormatCount {
                count: 25,
                total_size_bytes: 1_250,
            },
        );
        let deleted_file_stats = if with_deleted {
            let mut by_extension = BTreeMap::new();
            by_extension.insert("txt".to_string(), 5);
            Some(DeletedFileStats {
                total_count: 5,
                by_extension,
                by_folder: vec![("\\".to_string(), 5)],
                estimated_total_size: 250,
                recoverability_estimate: None,
            })
        } else {
            None
        };
        DiagnosticReport {
            case_id: CaseId::parse("260522-04").unwrap(),
            diagnosed_at: chrono::Utc
                .with_ymd_and_hms(2026, 5, 22, 14, 30, 0)
                .unwrap(),
            duration_secs: 0,
            hardware: HardwareInfo {
                model: None,
                serial: None,
                size_bytes: 5_000_000,
            },
            filesystem: FilesystemInfo {
                fs_type: "NTFS".into(),
                volume_serial: Some("A1B2C3D4".into()),
                cluster_size_bytes: 4096,
                total_clusters: 1220,
                used_clusters: 0,
            },
            filesystem_findings: findings,
            file_stats: FileStatistics {
                total_files: 30,
                live_files: 25,
                deleted_files: if with_deleted { 5 } else { 0 },
                directories: 0,
                total_size_bytes: 1_500,
            },
            format_breakdown,
            folder_breakdown: vec![FolderCount {
                path: "\\".into(),
                file_count: 30,
                total_size_bytes: 1_500,
            }],
            deleted_file_stats,
            anomalies: FsAnomalyReport::default(),
        }
    }

    fn healthy_findings() -> FilesystemFindings {
        FilesystemFindings {
            signature_valid: true,
            boot_sector_ok: true,
            ..Default::default()
        }
    }

    #[test]
    fn crm_text_contains_case_id() {
        let r = base_report(true, healthy_findings());
        let text = render(&r);
        assert!(text.contains("260522-04"), "case id missing: {}", text);
    }

    #[test]
    fn crm_text_no_longer_contains_symptom_section() {
        // Chunk 22.6 回帰防止: 旧「症状判定」セクション・「主症状:」見出しが
        // 一切残っていないことを機械的に確認する。
        let r = base_report(true, healthy_findings());
        let text = render(&r);
        assert!(
            !text.contains("【症状判定】"),
            "症状判定 section must be removed: {}",
            text
        );
        assert!(
            !text.contains("主症状:"),
            "主症状 prefix must be removed: {}",
            text
        );
        assert!(
            !text.contains("フォーマット (複合)"),
            "Mixed format mislabel must not appear: {}",
            text
        );
    }

    #[test]
    fn crm_text_shows_filesystem_findings_above_file_stats() {
        // 業務的に「破損があれば真っ先に見たい」ため、【ファイルシステムの破損】が
        // 【ファイル統計】より前 (= インデックスが小さい) に出ることを確認。
        let r = base_report(true, healthy_findings());
        let text = render(&r);
        let findings_idx = text
            .find("【ファイルシステムの破損】")
            .expect("findings section present");
        let stats_idx = text
            .find("【ファイル統計】")
            .expect("file stats section present");
        assert!(
            findings_idx < stats_idx,
            "破損 must appear before ファイル統計: findings={} stats={}",
            findings_idx,
            stats_idx
        );
    }

    #[test]
    fn crm_text_contains_mft_entry_statistics_section() {
        let r = base_report(false, healthy_findings());
        let text = render(&r);
        assert!(text.contains("【MFT エントリ統計】"), "MFT 統計 missing");
        assert!(text.contains("全エントリ数: 30 件"));
        assert!(text.contains("フォーマット案件"));
    }

    #[test]
    fn crm_text_includes_format_breakdown() {
        let r = base_report(true, healthy_findings());
        let text = render(&r);
        assert!(text.contains("TXT: 25 件"), "format breakdown missing");
    }

    #[test]
    fn crm_text_omits_deleted_section_when_no_deletions() {
        let r = base_report(false, healthy_findings());
        let text = render(&r);
        assert!(
            !text.contains("【削除エントリの詳細】"),
            "should omit deleted block: {}",
            text
        );
    }

    #[test]
    fn crm_text_renders_size_in_human_readable_format() {
        let r = base_report(true, healthy_findings());
        let text = render(&r);
        // 5_000_000 B = 4.77 MB
        assert!(
            text.contains("4.77 MB"),
            "human readable size missing: {}",
            text
        );
        assert!(text.contains("1.22 KB"));
    }

    // Chunk 22.5: 復旧可能性 (推定) セクションのテスト ---------------------

    fn report_with_recoverability(
        est: Option<dds_case_manager::RecoverabilityEstimate>,
    ) -> DiagnosticReport {
        let mut r = base_report(true, healthy_findings());
        if let Some(stats) = r.deleted_file_stats.as_mut() {
            stats.recoverability_estimate = est;
        }
        r
    }

    #[test]
    fn crm_text_includes_recoverability_section_when_estimate_present() {
        let est = dds_case_manager::RecoverabilityEstimate {
            high_confidence: 5,
            medium_confidence: 0,
            low_confidence: 0,
        };
        let r = report_with_recoverability(Some(est));
        let text = render(&r);
        assert!(
            text.contains("復旧可能性 (推定):"),
            "missing header: {}",
            text
        );
        assert!(text.contains("高 (確実復旧可能): 5 件"));
        assert!(text.contains("中 (部分復旧の可能性): 0 件"));
        assert!(text.contains("低 (メタデータのみ): 0 件"));
        assert!(text.contains("判定基準:"));
    }

    #[test]
    fn crm_text_omits_recoverability_when_no_estimate() {
        // recoverability_estimate = None の場合は判定基準セクションが含まれない。
        let r = report_with_recoverability(None);
        let text = render(&r);
        assert!(!text.contains("復旧可能性 (推定):"));
        assert!(!text.contains("高 (確実復旧可能)"));
    }

    #[test]
    fn crm_text_renders_anomaly_counts_in_findings() {
        // findings に破損件数があれば本文に反映される。
        let findings = FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 2,
            invalid_runlist_count: 1,
            boot_sector_ok: false,
            other_issues: vec!["unknown thing".into()],
        };
        let r = base_report(false, findings);
        let text = render(&r);
        assert!(text.contains("MFT エントリ破損: 2 件"));
        assert!(text.contains("不正な run-list: 1 件"));
        assert!(text.contains("Boot sector: 異常"));
        assert!(text.contains("その他の異常: 1 件"));
    }
}
