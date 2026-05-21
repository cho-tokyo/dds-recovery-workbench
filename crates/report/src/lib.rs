//! # dds-report
//!
//! Chunk 20: DDS 復旧レポート生成。
//!
//! [`dds_recovery::RecoveryReport`] から 3 種類のレポートを生成する:
//!
//! - **顧客向け HTML** ([`render_customer_html`]): お客様に納品する正式レポート。
//!   `user_message_ja` のみ使用、`internal_note_ja` は**絶対に含めない**。
//! - **CS 向け HTML** ([`render_internal_html`]): CS の業務管理用。
//!   `internal_note_ja` + SHA256 + 出力先パス等を含む。「お客様に共有しないでください」警告付き。
//! - **CSV** ([`render_csv`]): 外部システム連携用。13 列全フィールド出力。
//!
//! [`write_all_reports`] で 3 形式を同時にディレクトリへ書き出せる。
//!
//! ## 安全性設計
//!
//! - 顧客 HTML から `internal_note_ja` を排除（テストで機械検証）
//! - ファイル名・メッセージは HTML エスケープ済み（XSS 防止）
//! - 出力 HTML は外部 CSS/JS リンクなし、自己完結
//!
//! 関連 FR: FR-REP-01 / FR-REP-02 / FR-REP-03 / FR-QUAL-04。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod csv;
pub mod error;
pub mod escape;
pub mod html_customer;
pub mod html_internal;

pub use crate::csv::render_csv;
pub use error::ReportError;
pub use html_customer::render_customer_html;
pub use html_internal::render_internal_html;

use std::path::{Path, PathBuf};

use dds_recovery::RecoveryReport;

/// [`write_all_reports`] が生成した 3 つのレポートのファイルパス。
#[derive(Debug, Clone)]
pub struct ReportPaths {
    /// 顧客向け HTML レポートのパス（`report_customer.html`）。
    pub customer_html: PathBuf,
    /// CS 向け HTML レポートのパス（`report_internal.html`）。
    pub internal_html: PathBuf,
    /// 外部連携用 CSV のパス（`report.csv`）。
    pub csv: PathBuf,
}

/// 3 種類のレポート（顧客 HTML / CS HTML / CSV）を `output_dir` に書き出す。
///
/// `output_dir` が存在しない場合は再帰的に作成する。
///
/// 出力ファイル名:
/// - `report_customer.html`
/// - `report_internal.html`
/// - `report.csv`
pub fn write_all_reports(
    report: &RecoveryReport,
    output_dir: &Path,
) -> Result<ReportPaths, ReportError> {
    std::fs::create_dir_all(output_dir)?;

    let customer_path = output_dir.join("report_customer.html");
    let internal_path = output_dir.join("report_internal.html");
    let csv_path = output_dir.join("report.csv");

    std::fs::write(&customer_path, render_customer_html(report)?)?;
    std::fs::write(&internal_path, render_internal_html(report)?)?;
    std::fs::write(&csv_path, render_csv(report)?)?;

    Ok(ReportPaths {
        customer_html: customer_path,
        internal_html: internal_path,
        csv: csv_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::RecoveryReport;
    use tempfile::TempDir;

    fn empty_report() -> RecoveryReport {
        let now = Utc::now();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 0,
            recovered: vec![],
            failed: vec![],
            skipped: vec![],
        }
    }

    #[test]
    fn write_all_reports_creates_three_files() {
        // 3 種レポートが指定ディレクトリに生成され、ファイルが空でないこと。
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("nested").join("reports");
        let paths = write_all_reports(&empty_report(), &dir).expect("write_all_reports");
        assert!(paths.customer_html.exists());
        assert!(paths.internal_html.exists());
        assert!(paths.csv.exists());
        // ファイル名の検証
        assert_eq!(paths.customer_html.file_name().unwrap(), "report_customer.html");
        assert_eq!(paths.internal_html.file_name().unwrap(), "report_internal.html");
        assert_eq!(paths.csv.file_name().unwrap(), "report.csv");
        // 各ファイルの先頭バイトをチェック（空ではない）
        assert!(paths.customer_html.metadata().unwrap().len() > 0);
        assert!(paths.internal_html.metadata().unwrap().len() > 0);
        assert!(paths.csv.metadata().unwrap().len() > 0);
    }
}
