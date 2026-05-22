//! Chunk 22: CRM 貼り付け用の業務日本語テキスト生成。
//!
//! 業務シナリオ:
//! DDS の CS 担当者が CRM の備考欄に「論理診断結果」を貼り付けるためのテキスト。
//! HDD 接続 → 1 コマンド実行 → 数十秒後に画面表示されたテキストをそのままコピペできる。
//!
//! 生成セクション（出力順）:
//! 1. 案件番号 + 診断日時 + 所要時間
//! 2. ハードウェア（モデル / シリアル / 容量）
//! 3. ファイルシステム（種別 / シリアル / クラスタサイズ / 使用率）
//! 4. 症状判定（主症状 + 詳細）
//! 5. ファイル統計（生存 / 削除 / ディレクトリ件数）
//! 6. 削除ファイル内訳（削除あり時のみ）
//! 7. 生存ファイル統計（拡張子別 上位 10）
//! 8. 主なフォルダ（件数順 上位 10）
//! 9. FS 破損
//! 10. 物理不良チェック（Phase 2 で対応予定）
//!
//! 関連 FR: FR-DIAG-04 (CRM 貼り付け用テキスト)。

use std::fmt::Write;

use dds_case_manager::Symptom;
use dds_core::format::format_bytes;

use crate::report::{DiagnosticReport, FormatCount};

/// [`DiagnosticReport`] から CRM 貼り付け用業務テキストを生成する。
///
/// 戻り値はそのままクリップボードコピー / .txt 保存が可能な完全独立な文字列。
/// 改行は LF（Windows でも CRM 上で問題なく表示される業務実績あり）。
pub fn render(report: &DiagnosticReport) -> String {
    let mut s = String::with_capacity(2048);

    // ヘッダ
    let _ = writeln!(s, "=== 論理診断結果 (案件 {}) ===", report.case_id);
    let _ = writeln!(
        s,
        "診断日時: {}",
        report.diagnosed_at.format("%Y-%m-%d %H:%M")
    );
    let _ = writeln!(s, "診断時間: {} 秒", report.duration_secs);
    let _ = writeln!(s, "※物理診断は別途実施済み");
    let _ = writeln!(s);

    // ハードウェア
    let _ = writeln!(s, "【ハードウェア】");
    if let Some(model) = &report.hardware.model {
        let _ = writeln!(s, "HDD: {}", model);
    }
    if let Some(serial) = &report.hardware.serial {
        let _ = writeln!(s, "シリアル: {}", serial);
    }
    let _ = writeln!(s, "容量: {}", format_bytes(report.hardware.size_bytes));
    let _ = writeln!(s);

    // ファイルシステム
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

    // 症状判定
    let _ = writeln!(s, "【症状判定】");
    let _ = writeln!(s, "主症状: {}", report.symptom.primary_label());
    render_symptom_details(&mut s, &report.symptom);
    let _ = writeln!(s);

    // ファイル統計
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

    // 削除ファイル内訳
    if let Some(deleted) = &report.deleted_file_stats {
        let _ = writeln!(s, "【削除ファイルの内訳】");
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
        let _ = writeln!(s);
    }

    // 生存ファイル統計
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

    // 主なフォルダ
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

    // FS 異常
    let _ = writeln!(s, "【ファイルシステムの破損】");
    let _ = writeln!(
        s,
        "MFT エントリ破損: {} 件",
        report.anomalies.mft_corrupted_count
    );
    let _ = writeln!(
        s,
        "不正な run-list: {} 件",
        report.anomalies.invalid_runlist_count
    );
    if report.anomalies.boot_sector_issues.is_empty() {
        let _ = writeln!(s, "Boot sector: 正常");
    } else {
        let _ = writeln!(
            s,
            "Boot sector の異常: {} 件",
            report.anomalies.boot_sector_issues.len()
        );
    }
    let _ = writeln!(s);

    // 物理不良チェック
    let _ = writeln!(s, "【物理不良チェック】");
    let _ = writeln!(s, "未実施 (Phase 2 で対応予定)");
    let _ = writeln!(s);

    let _ = writeln!(s, "=== 診断完了 ===");

    s
}

/// 症状別の詳細セクションを描画する。
fn render_symptom_details(s: &mut String, symptom: &Symptom) {
    match symptom {
        Symptom::None => {
            let _ = writeln!(s, "- ファイルシステム署名: 正常 (NTFS 認識成功)");
            let _ = writeln!(s, "- MFT 構造: 正常");
            let _ = writeln!(s, "- 削除エントリ: なし");
            let _ = writeln!(s, "- フォーマット痕跡: なし");
        }
        Symptom::Deleted => {
            let _ = writeln!(s, "- ファイルシステム署名: 正常");
            let _ = writeln!(s, "- MFT 構造: 正常");
            let _ = writeln!(s, "- フォーマット痕跡: なし");
            let _ = writeln!(s, "  ※削除エントリ検出 (件数は下記「削除ファイル」参照)");
        }
        Symptom::Formatted {
            current_mft_entries,
            old_mft_recoverability_hint,
        } => {
            let _ = writeln!(
                s,
                "- 新 MFT エントリ数: {} 件 (初期化された MFT と推定)",
                current_mft_entries
            );
            if let Some(hint) = old_mft_recoverability_hint {
                let _ = writeln!(s, "- 旧 MFT 残存度: {:.1}%", hint * 100.0);
            } else {
                let _ = writeln!(s, "- 旧 MFT 残存度: 未計測 (Phase 2 で対応予定)");
            }
            let _ = writeln!(
                s,
                "  ※フォーマット前ファイルの復旧には MFT カービング機能が必要 (Phase 2)"
            );
        }
        Symptom::FilesystemError { anomalies } => {
            let _ = writeln!(s, "- 検出された異常:");
            for a in anomalies {
                let _ = writeln!(s, "  ・{}", anomaly_label(a));
            }
        }
        Symptom::Mixed { symptoms } => {
            let _ = writeln!(s, "- 複合症状:");
            for sub in symptoms {
                let _ = writeln!(s, "  ・{}", sub.primary_label());
            }
        }
    }
}

/// 個別 FS 異常 ([`dds_case_manager::FsAnomaly`]) の業務日本語ラベルを返す。
fn anomaly_label(a: &dds_case_manager::FsAnomaly) -> String {
    use dds_case_manager::FsAnomaly::*;
    match a {
        MftEntryCorrupted { count } => format!("MFT エントリ破損 {} 件", count),
        InvalidRunList { count } => format!("不正な run-list {} 件", count),
        BootSectorAnomaly { description } => format!("Boot sector: {}", description),
        InvalidVolumeSerial => "Volume Serial Number 異常".to_string(),
        Other { description } => description.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        FileStatistics, FilesystemInfo, FolderCount, FsAnomalyReport, HardwareInfo,
    };
    use chrono::TimeZone;
    use dds_case_manager::{CaseId, DeletedFileStats};
    use std::collections::BTreeMap;

    fn base_report(symptom: Symptom, with_deleted: bool) -> DiagnosticReport {
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
            symptom,
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

    #[test]
    fn crm_text_contains_case_id() {
        let r = base_report(Symptom::Deleted, true);
        let text = render(&r);
        assert!(text.contains("260522-04"), "case id missing: {}", text);
    }

    #[test]
    fn crm_text_uses_japanese_symptom_label() {
        let r = base_report(Symptom::Deleted, true);
        let text = render(&r);
        assert!(text.contains("主症状: 削除"), "expected 主症状: 削除");
        assert!(text.contains("【症状判定】"));
        assert!(text.contains("【ファイル統計】"));
    }

    #[test]
    fn crm_text_includes_format_breakdown() {
        let r = base_report(Symptom::Deleted, true);
        let text = render(&r);
        assert!(text.contains("TXT: 25 件"), "format breakdown missing");
    }

    #[test]
    fn crm_text_omits_deleted_section_when_no_deletions() {
        let r = base_report(Symptom::None, false);
        let text = render(&r);
        assert!(
            !text.contains("【削除ファイルの内訳】"),
            "should omit deleted block: {}",
            text
        );
        assert!(text.contains("削除エントリ: なし"));
    }

    #[test]
    fn crm_text_renders_size_in_human_readable_format() {
        let r = base_report(Symptom::Deleted, true);
        let text = render(&r);
        // 5_000_000 B = 4.77 MB
        assert!(
            text.contains("4.77 MB"),
            "human readable size missing: {}",
            text
        );
        assert!(text.contains("1.22 KB"));
    }
}
