//! Chunk 23: 案件単位の業務オーケストレーション `execute_business_recovery`。
//!
//! Phase 1.5 の集大成。1 つの関数呼び出しで以下を順に実行する:
//!
//! 1. [`CaseOutput`] を構築し、納品ディレクトリ 3 つを `create_dir_all`
//! 2. [`RecoveryConfig`] を 業務向けパスで構築
//! 3. [`RecoveryEngine`] で実復旧 → `RecoveryReport`
//! 4. [`dds_report::write_business_reports`] で 4 ファイルを日本語名で生成
//! 5. `Case.output_dir` / `recovery_report_summary` / `wishlist` を更新
//!
//! `case.json` への永続化は呼び出し元の責務（`CaseStorage::save`）。これは
//! 「DB トランザクションは UI / CLI 側で管理する」原則に従う。
//!
//! ## 依存方向（Chunk 23 で意図的に拡大）
//!
//! ```text
//! 変更前 (Chunk 22.6):
//!   case-manager → wish-match → core
//!
//! 変更後 (Chunk 23):
//!   case-manager → wish-match
//!   case-manager → recovery   ← 追加
//!   case-manager → report     ← 追加
//!   case-manager → fs-ntfs    ← 追加（NtfsVolume を受け取るため）
//! ```
//!
//! recovery / report / fs-ntfs から case-manager への逆向き依存は **なし**
//! （循環依存を回避するため `write_business_reports` は `CaseOutput` を知らない設計）。
//!
//! 関連 FR: FR-OUT-01〜04（業務向け出力構造）, FR-REC-01〜04, FR-REP-01〜05。

use std::path::Path;

use dds_fs_ntfs::NtfsVolume;
use dds_recovery::{RecoveryConfig, RecoveryEngine, RecoveryError, RecoveryReport};
use dds_wish_match::{ExclusionList, Wishlist};

use crate::case::{Case, RecoveryReportSummary};
use crate::output::CaseOutput;

/// 案件単位で業務復旧フローを一括実行する。
///
/// `case` は内部状態が更新される（`output_dir`, `recovery_report_summary`,
/// `wishlist` が `Some` で埋まる）。永続化したい場合は呼び出し元で
/// `CaseStorage::save(&case)` を呼ぶこと。
///
/// `drive_root` は納品 HDD のルート (`"G:\\"`) または検証用 `TempDir::path()`。
///
/// `exclusions` は [`ExclusionList::default_system_exclusions`] を渡すのが業務標準
/// （Chunk 23.7 で追加。Windows / NTFS のシステム系を除外する）。
///
/// # エラー
///
/// 内部の `create_dir_all` / `RecoveryEngine` / `write_business_reports` で
/// 発生したエラーを [`BusinessRecoveryError`] にまとめて返す。
pub fn execute_business_recovery<F>(
    case: &mut Case,
    drive_root: impl AsRef<Path>,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
    exclusions: &ExclusionList,
) -> Result<BusinessRecoveryResult, BusinessRecoveryError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // Step 1: 納品レイアウト構築 + 3 ディレクトリ作成。
    let case_output = CaseOutput::new(case.case_id.clone(), drive_root.as_ref().to_path_buf());
    case_output.create_all_dirs()?;

    // Step 2: 復旧設定を業務向け CaseOutput パスで構築。
    let config = RecoveryConfig::with_paths(
        case_output.live_files_dir(),
        case_output.deleted_files_dir(),
    );
    let engine = RecoveryEngine::with_config(config);

    // Step 3: 復旧実行。Chunk 23.7 で全件復旧 + ExclusionList で除外する設計に変更。
    let report = engine.recover_files(volume, wishlist, exclusions)?;

    // Step 4: レポート 4 ファイル生成（日本語名）。
    let report_paths = dds_report::write_business_reports(
        &report,
        &case_output.customer_docx_path(),
        &case_output.customer_txt_path(),
        &case_output.internal_html_path(),
        &case_output.csv_path(),
    )?;

    // Step 5: Case を更新（永続化は呼び出し元）。
    case.output_dir = Some(case_output.root());
    case.recovery_report_summary = Some(summarize_report(&report));
    case.wishlist = Some(wishlist.clone());

    Ok(BusinessRecoveryResult {
        case_output,
        report,
        report_paths,
    })
}

/// [`RecoveryReport`] から軽量サマリ [`RecoveryReportSummary`] を抽出する。
///
/// `RecoveryReport` 本体は重い（`recovered: Vec<RecoveredEntry>` 等）ため
/// `case.json` には保存せず、サマリのみ保存する設計。
fn summarize_report(report: &RecoveryReport) -> RecoveryReportSummary {
    RecoveryReportSummary {
        started_at: report.started_at,
        finished_at: report.finished_at,
        duration_ms: report.duration_ms(),
        total_matched: report.total_matched,
        recovered_count: report.recovered.len(),
        failed_count: report.failed.len(),
        skipped_count: report.skipped.len(),
        validated_count: report.validated_count(),
        invalid_count: report.invalid_count(),
        uncertain_count: report.uncertain_count(),
        total_bytes_written: report.total_bytes_written(),
        recovery_success_rate: report.recovery_success_rate(),
        quality_assurance_rate: report.quality_assurance_rate(),
    }
}

/// [`execute_business_recovery`] の戻り値。
///
/// 復旧パイプラインで生成された生 [`RecoveryReport`] と納品物のパス情報を
/// 呼び出し元に返す。CLI / UI 側で進捗表示やログ出力に使える。
#[derive(Debug)]
pub struct BusinessRecoveryResult {
    /// 構築された納品レイアウト。
    pub case_output: CaseOutput,
    /// 復旧の生レポート（per-file 詳細を含む）。
    pub report: RecoveryReport,
    /// 生成されたレポート 4 ファイルのパス。
    pub report_paths: dds_report::BusinessReportPaths,
}

/// 業務復旧オーケストレーションのエラー集約型。
#[derive(Debug, thiserror::Error)]
pub enum BusinessRecoveryError {
    /// ディレクトリ作成等の I/O エラー。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// 復旧パイプライン由来のエラー。
    #[error("Recovery error: {0}")]
    Recovery(#[from] RecoveryError),

    /// レポート生成由来のエラー。
    #[error("Report error: {0}")]
    Report(#[from] dds_report::ReportError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_recovery_error_displays_io_variant() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err: BusinessRecoveryError = io.into();
        assert!(matches!(err, BusinessRecoveryError::Io(_)));
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn business_recovery_error_displays_report_variant() {
        let rep = dds_report::ReportError::Template("oops".into());
        let err: BusinessRecoveryError = rep.into();
        assert!(matches!(err, BusinessRecoveryError::Report(_)));
        assert!(err.to_string().contains("Report error"));
    }

    #[test]
    fn summarize_report_extracts_expected_metrics() {
        use chrono::Utc;
        let now = Utc::now();
        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 10,
            recovered: vec![],
            failed: vec![],
            skipped: vec![],
            wish_labels: vec![],
        };
        let summary = summarize_report(&report);
        assert_eq!(summary.total_matched, 10);
        assert_eq!(summary.recovered_count, 0);
        // recovery_success_rate と quality_assurance_rate は recovered 0 件のとき 0.0。
        assert_eq!(summary.recovery_success_rate, 0.0);
        assert_eq!(summary.quality_assurance_rate, 0.0);
        assert_eq!(summary.total_bytes_written, 0);
    }
}
