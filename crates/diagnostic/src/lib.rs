//! # dds-diagnostic
//!
//! Chunk 22: NTFS 論理診断エンジン（Phase 1.5 の業務的中核チャンク）。
//! Chunk 22.6: 症状判定ロジックを完全削除し、事実報告型に再設計。
//!
//! 単一パスで `$MFT` を走査し、以下を一気に集計する:
//! - ファイル統計（生存 / 削除 / ディレクトリ件数、総バイト）
//! - 形式別ブレイクダウン（拡張子ごとの件数 + サイズ）
//! - フォルダ別ブレイクダウン（件数順 上位 10）
//! - 削除ファイル統計（形式別 + フォルダ別）
//! - FS 異常（MFT 破損件数 / 不正 run-list 件数 / その他）
//!
//! その上で CRM 貼り付け用業務日本語テキスト、および case.json 永続化用
//! [`DiagnosticInput`](dds_case_manager::DiagnosticInput) を生成する。
//!
//! ## 設計原則: 「判定者」ではなく「事実提供者」
//!
//! Workbench は事実 (件数・破損状態) のみ報告し、業務判断 (CS の「これは
//! フォーマット案件」「これは削除案件」等) は外部 (CS / CRM) に委ねる。
//! Chunk 22 では複合判定が小規模フィクスチャで「フォーマット (複合)」と
//! 誤判定される事故が発生したため、Chunk 22.6 で症状判定ロジックを排除した。
//!
//! ## エントリーポイント
//!
//! ```ignore
//! use dds_diagnostic::DiagnosticEngine;
//! use dds_case_manager::CaseId;
//!
//! let case_id = CaseId::parse("260522-04").unwrap();
//! let report = DiagnosticEngine::diagnose(&mut volume, case_id)?;
//!
//! // CRM 貼り付け用テキスト
//! let crm_text = report.to_crm_text();
//!
//! // case.json 保存用 slim 版
//! let input = report.to_diagnostic_input();
//! ```
//!
//! ## 依存方向
//! `dds-diagnostic → dds-fs-ntfs + dds-case-manager + dds-core` のみ。
//! `dds-recovery` / `dds-report` / `dds-wish-match` / `dds-validators` には依存しない
//! （診断 → 復旧の業務上向き依存を避け、診断単独で動くクレートに保つ）。
//!
//! 関連 FR: FR-DIAG-01〜05, FR-DIAG-06 (事実ベースの報告)。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod aggregator;
pub mod crm_text;
pub mod error;
pub mod recoverability;
pub mod report;

pub use aggregator::{ClusterOccupancyMap, DeletedFileMetadata};
pub use error::DiagnosticError;
pub use report::{
    DiagnosticReport, FileStatistics, FilesystemInfo, FolderCount, FormatCount, FsAnomalyReport,
    HardwareInfo,
};

use chrono::Utc;

use dds_case_manager::CaseId;
use dds_fs_ntfs::NtfsVolume;

/// 診断エンジン本体（unit struct）。
///
/// 状態を持たないため `DiagnosticEngine::diagnose(...)` のように関連関数として呼び出す。
pub struct DiagnosticEngine;

impl DiagnosticEngine {
    /// `NtfsVolume` を診断し、[`DiagnosticReport`] を返す。
    ///
    /// 内部で `$MFT` を **1 回だけ** 走査して全統計を集計するため、
    /// 健康な 2TB HDD でも数十秒で完了する想定（FR-DIAG-05）。
    ///
    /// Chunk 22.6: 旧症状判定処理は削除済み。代わりに
    /// `FsAnomalyReport::to_findings()` で `FilesystemFindings` を構築する。
    pub fn diagnose<F>(
        volume: &mut NtfsVolume<F>,
        case_id: CaseId,
    ) -> Result<DiagnosticReport, DiagnosticError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();

        // ハードウェア情報は Phase 1.5 ではモデル/シリアル未取得。容量だけ
        // FilesystemInfo から後で計算する。
        let mut hardware = HardwareInfo::default();

        let filesystem = gather_filesystem_info(volume);

        let aggregate = aggregator::aggregate_all(volume)?;

        // 症状判定は行わない (Chunk 22.6)。事実のみ FilesystemFindings に詰める。
        let filesystem_findings = aggregate.anomalies.to_findings();

        // Chunk 22.5: 削除ファイル群の復旧可能性を推定し、DeletedFileStats に反映。
        let recoverability =
            recoverability::estimate(&aggregate.deleted_file_metadata, &aggregate.cluster_occupancy);
        let mut deleted_file_stats = aggregate.deleted_file_stats;
        if let Some(stats) = &mut deleted_file_stats {
            stats.recoverability_estimate = Some(recoverability);
        }

        let finished_at = Utc::now();
        let duration_secs = (finished_at - started_at).num_seconds().max(0) as u64;

        // パーティション容量 = total_clusters * cluster_size_bytes
        hardware.size_bytes = filesystem
            .total_clusters
            .saturating_mul(u64::from(filesystem.cluster_size_bytes));

        Ok(DiagnosticReport {
            case_id,
            diagnosed_at: started_at,
            duration_secs,
            hardware,
            filesystem,
            filesystem_findings,
            file_stats: aggregate.file_stats,
            format_breakdown: aggregate.format_breakdown,
            folder_breakdown: aggregate.folder_breakdown,
            deleted_file_stats,
            anomalies: aggregate.anomalies,
        })
    }
}

/// ファイルシステム情報を [`BootSector`](dds_fs_ntfs::BootSector) 経由で組み立てる。
///
/// `NtfsVolume` には直接 `cluster_size_bytes` / `volume_serial` getter が無いため、
/// `boot_sector()` 経由でアクセスする。`used_clusters` は `$Bitmap` 解析が必要だが、
/// Phase 1.5 では未実装で 0 フォールバック（CRM テキストは「使用率: 未計測」表示）。
//
// TODO: Phase 2 で `$Bitmap` を解析して `used_clusters` を埋め、より精緻な使用率を出す。
fn gather_filesystem_info<F>(volume: &NtfsVolume<F>) -> FilesystemInfo
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let bs = volume.boot_sector();
    // ボリュームシリアルが 0 の場合は「未設定」と扱い、CRM 表示から省略。
    let volume_serial = if bs.volume_serial == 0 {
        None
    } else {
        // 16 進大文字 8 桁。NTFS 慣習で 64bit シリアルを上位 32bit と下位 32bit
        // でハイフン区切る表記もあるが、Phase 1.5 では単純表示で十分。
        Some(format!("{:016X}", bs.volume_serial))
    };
    // パーティション総クラスタ数 = total_sectors * bytes_per_sector / cluster_size_bytes
    let cluster_size = u64::from(bs.cluster_size_bytes());
    let total_bytes = bs
        .total_sectors
        .saturating_mul(u64::from(bs.bytes_per_sector));
    let total_clusters = total_bytes.checked_div(cluster_size).unwrap_or(0);
    FilesystemInfo {
        fs_type: "NTFS".to_string(),
        volume_serial,
        cluster_size_bytes: bs.cluster_size_bytes(),
        total_clusters,
        used_clusters: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::FileStatistics;
    use chrono::Utc;
    use dds_case_manager::FilesystemFindings;
    use std::collections::BTreeMap;

    fn sample_report(findings: FilesystemFindings) -> DiagnosticReport {
        DiagnosticReport {
            case_id: CaseId::parse("260522-04").unwrap(),
            diagnosed_at: Utc::now(),
            duration_secs: 1,
            hardware: HardwareInfo::default(),
            filesystem: FilesystemInfo {
                fs_type: "NTFS".into(),
                volume_serial: Some("A1B2C3D4".into()),
                cluster_size_bytes: 4096,
                total_clusters: 100,
                used_clusters: 0,
            },
            filesystem_findings: findings,
            file_stats: FileStatistics {
                total_files: 30,
                live_files: 25,
                deleted_files: 5,
                directories: 0,
                total_size_bytes: 1_500,
            },
            format_breakdown: BTreeMap::new(),
            folder_breakdown: vec![],
            deleted_file_stats: None,
            anomalies: FsAnomalyReport::default(),
        }
    }

    #[test]
    fn diagnostic_report_to_input_round_trip_via_json() {
        // 業務的に case.json 経由で DiagnosticInput が完全復元できることを確認。
        let report = sample_report(FilesystemFindings {
            signature_valid: true,
            boot_sector_ok: true,
            ..Default::default()
        });
        let input = report.to_diagnostic_input();
        let json = serde_json::to_string(&input).unwrap();
        let restored: dds_case_manager::DiagnosticInput = serde_json::from_str(&json).unwrap();
        let findings = restored
            .filesystem_findings
            .expect("filesystem_findings preserved");
        assert!(findings.signature_valid);
        assert!(findings.boot_sector_ok);
        assert_eq!(restored.total_files, 30);
        assert_eq!(restored.deleted_files, 5);
        assert_eq!(restored.filesystem_type, Some("NTFS".to_string()));
    }

    #[test]
    fn diagnostic_engine_is_unit_struct_with_static_method() {
        // コンパイル時の存在確認のみ（実 volume なしで diagnose は呼べないので smoke test）。
        let _ = DiagnosticEngine;
    }

    #[test]
    fn to_diagnostic_input_includes_filesystem_findings() {
        // case.json 保存形式に findings が含まれることを確認。
        let findings = FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 2,
            invalid_runlist_count: 1,
            boot_sector_ok: true,
            other_issues: vec![],
        };
        let report = sample_report(findings.clone());
        let input = report.to_diagnostic_input();
        assert_eq!(input.filesystem_findings, Some(findings));
    }
}
