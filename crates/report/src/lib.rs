//! # dds-report
//!
//! Chunk 20 / 20.5 / Chunk 24a: DDS 復旧レポート生成（業務適用版）。
//!
//! [`dds_recovery::RecoveryReport`] から 3 種類のレポートを生成する (Chunk 24a で 4→3 に簡素化):
//!
//! - **顧客向け .docx** ([`render_customer_docx`]): Word で開いて編集 → PDF 化して納品。
//!   `user_message_ja` のみ使用、`internal_note_ja` は**絶対に含めない**。
//!   Chunk 24a で「品質保証率」「Valid/Invalid/Uncertain」「復旧実施日時」表示を削除。
//! - **CS 向け HTML** ([`render_internal_html`]): 業務管理用。`internal_note_ja` +
//!   SHA256 + 出力先パスを含む。「お客様に共有しないでください」警告付き。
//!   Chunk 24a で「品質保証率」パーセンテージ表示削除 (件数表示は維持)。
//! - **CSV** ([`render_csv`]): 外部システム連携用。15 列。書き出し時に UTF-8 BOM 付加で
//!   Excel 文字化け解消 ([`write_business_reports`] 経由)。
//!
//! [`write_business_reports`] で 3 形式を同時に書き出す (Chunk 24a 主 API)。
//! [`write_all_reports`] は Chunk 20.5 の旧 API として残置 (4 ファイル `report_customer.docx`
//! 等の英名版、レガシテスト用)。
//!
//! ## 安全性設計
//!
//! - 顧客 .docx / .txt から `internal_note_ja` を排除（テストで機械検証）
//! - HTML はファイル名・メッセージを HTML エスケープ済み（XSS 防止）
//! - 出力 HTML は外部 CSS/JS リンクなし、自己完結
//!
//! 関連 FR: FR-REP-01 / FR-REP-02 / FR-REP-03 / FR-REP-04 / FR-REP-05 / FR-QUAL-04。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod business;
pub mod csv;
pub mod docx_customer;
pub mod error;
pub mod escape;
pub mod format;
pub mod html_internal;
pub mod txt_customer;

pub use crate::business::{write_business_reports, BusinessReportPaths};
pub use crate::csv::render_csv;
pub use crate::docx_customer::{render_customer_docx, COMPANY_NAME};
pub use crate::format::{format_bytes, format_duration_ms};
pub use crate::html_internal::render_internal_html;
pub use crate::txt_customer::{render_invalid_files_txt, render_uncertain_files_txt};
pub use error::ReportError;

use std::path::{Path, PathBuf};

use dds_recovery::RecoveryReport;

/// [`write_all_reports`] が生成した 4 つのレポートのファイルパス。
#[derive(Debug, Clone)]
pub struct ReportPaths {
    /// 顧客向け .docx レポートのパス（`report_customer.docx`）。
    pub customer_docx: PathBuf,
    /// 顧客向け要確認 .txt（`recovered_files.txt`）。
    pub invalid_txt: PathBuf,
    /// CS 向け HTML レポートのパス（`report_internal.html`）。
    pub internal_html: PathBuf,
    /// 外部連携用 CSV のパス（`report.csv`）。
    pub csv: PathBuf,
}

/// 4 種類のレポート（顧客 .docx / 顧客 .txt / CS HTML / CSV）を `output_dir` に書き出す。
///
/// `output_dir` が存在しない場合は再帰的に作成する。
///
/// 出力ファイル名:
/// - `report_customer.docx`
/// - `recovered_files.txt`
/// - `report_internal.html`
/// - `report.csv`
pub fn write_all_reports(
    report: &RecoveryReport,
    output_dir: &Path,
) -> Result<ReportPaths, ReportError> {
    std::fs::create_dir_all(output_dir)?;

    let customer_docx = output_dir.join("report_customer.docx");
    let invalid_txt = output_dir.join("recovered_files.txt");
    let internal_html = output_dir.join("report_internal.html");
    let csv_path = output_dir.join("report.csv");

    std::fs::write(&customer_docx, render_customer_docx(report)?)?;
    std::fs::write(&invalid_txt, render_invalid_files_txt(report))?;
    std::fs::write(&internal_html, render_internal_html(report)?)?;
    std::fs::write(&csv_path, render_csv(report)?)?;

    Ok(ReportPaths {
        customer_docx,
        invalid_txt,
        internal_html,
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
            wish_labels: vec![],
        }
    }

    #[test]
    fn write_all_reports_creates_four_files() {
        // 4 種レポート（Chunk 20.5）が指定ディレクトリに生成され、空でないこと。
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("nested").join("reports");
        let paths = write_all_reports(&empty_report(), &dir).expect("write_all_reports");
        assert!(paths.customer_docx.exists());
        assert!(paths.invalid_txt.exists());
        assert!(paths.internal_html.exists());
        assert!(paths.csv.exists());
        // ファイル名
        assert_eq!(
            paths.customer_docx.file_name().unwrap(),
            "report_customer.docx"
        );
        assert_eq!(
            paths.invalid_txt.file_name().unwrap(),
            "recovered_files.txt"
        );
        assert_eq!(
            paths.internal_html.file_name().unwrap(),
            "report_internal.html"
        );
        assert_eq!(paths.csv.file_name().unwrap(), "report.csv");
        // 各ファイルが空でない
        assert!(paths.customer_docx.metadata().unwrap().len() > 0);
        assert!(paths.invalid_txt.metadata().unwrap().len() > 0);
        assert!(paths.internal_html.metadata().unwrap().len() > 0);
        assert!(paths.csv.metadata().unwrap().len() > 0);
    }

    #[test]
    fn write_all_reports_customer_docx_is_zip_archive() {
        // .docx は OOXML ZIP アーカイブとして書き出されること（PK magic で開始）。
        let tmp = TempDir::new().unwrap();
        let paths = write_all_reports(&empty_report(), tmp.path()).expect("write_all_reports");
        let bytes = std::fs::read(&paths.customer_docx).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }
}
