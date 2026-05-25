//! Chunk 23: 業務向けレポートライタ（日本語ファイル名版）。
//!
//! [`crate::write_all_reports`] (Chunk 20.5) は 4 ファイルを `report_customer.docx`
//! など **英名** で出力していたが、業務的にはお客様に渡す納品 HDD へは日本語名で
//! 配布する必要がある。本モジュールはレポートディレクトリではなく **個別の 4 つの
//! パス** を直接受け取って書き出す薄いラッパ。
//!
//! ## 設計判断: パスを 4 つ受け取る理由
//!
//! ```text
//! ✗ write_business_reports(report, &CaseOutput) → report → case-manager の循環依存
//! ○ write_business_reports(report, &Path, &Path, &Path, &Path) → 依存なし
//! ```
//!
//! `CaseOutput` から派生したパスを呼び出し側（`dds_case_manager::orchestration`）
//! で 4 つ展開して渡す。これにより `report` クレートは `case-manager` を知らずに済む。
//!
//! 関連 FR: FR-OUT-03 (日本語名対応), FR-REP-01〜05 (各レポート責務)。

use std::path::{Path, PathBuf};

use dds_recovery::RecoveryReport;

use crate::csv::render_csv;
use crate::docx_customer::render_customer_docx;
use crate::error::ReportError;
use crate::html_internal::render_internal_html;
use crate::txt_customer::{render_invalid_files_txt, render_uncertain_files_txt};

/// 業務向けレポート 5 ファイルを指定された各パスへ書き出す（Chunk 23.8 で 4→5 に拡張）。
///
/// 各パスの親ディレクトリは事前に作成されているか、または同じ親ディレクトリで
/// あることが想定される（同じ親なら 1 度だけ `create_dir_all` を呼ぶ）。
/// 安全側で 5 つすべての親をそれぞれ `create_dir_all` する。
///
/// 業務シナリオ:
/// ```text
/// G:\260522-04\レポート\
///   ├ 復旧レポート.docx              ← customer_docx
///   ├ 破損疑いファイル一覧.txt       ← customer_invalid_txt (Chunk 23.8 で rename)
///   ├ 自動確認対象外ファイル一覧.txt ← customer_uncertain_txt (Chunk 23.8 新規)
///   ├ 業務管理レポート.html          ← internal_html
///   └ report.csv                     ← csv
/// ```
pub fn write_business_reports(
    report: &RecoveryReport,
    customer_docx: &Path,
    customer_invalid_txt: &Path,
    customer_uncertain_txt: &Path,
    internal_html: &Path,
    csv: &Path,
) -> Result<BusinessReportPaths, ReportError> {
    for p in [
        customer_docx,
        customer_invalid_txt,
        customer_uncertain_txt,
        internal_html,
        csv,
    ] {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    std::fs::write(customer_docx, render_customer_docx(report)?)?;
    std::fs::write(customer_invalid_txt, render_invalid_files_txt(report))?;
    std::fs::write(customer_uncertain_txt, render_uncertain_files_txt(report))?;
    std::fs::write(internal_html, render_internal_html(report)?)?;
    std::fs::write(csv, render_csv(report)?)?;

    Ok(BusinessReportPaths {
        customer_docx: customer_docx.to_path_buf(),
        customer_invalid_txt: customer_invalid_txt.to_path_buf(),
        customer_uncertain_txt: customer_uncertain_txt.to_path_buf(),
        internal_html: internal_html.to_path_buf(),
        csv: csv.to_path_buf(),
    })
}

/// [`write_business_reports`] が生成した 5 ファイルの絶対パス（Chunk 23.8 で 4→5）。
#[derive(Debug, Clone)]
pub struct BusinessReportPaths {
    /// 顧客向け Word レポートのパス（`復旧レポート.docx`）。
    pub customer_docx: PathBuf,
    /// 顧客向け破損疑いファイル一覧のパス（`破損疑いファイル一覧.txt`、Chunk 23.8 で rename）。
    pub customer_invalid_txt: PathBuf,
    /// 顧客向け自動確認対象外ファイル一覧のパス（`自動確認対象外ファイル一覧.txt`、Chunk 23.8 新規）。
    pub customer_uncertain_txt: PathBuf,
    /// 社内向け業務管理レポートのパス（`業務管理レポート.html`）。
    pub internal_html: PathBuf,
    /// 外部システム連携用 CSV のパス（`report.csv`）。
    pub csv: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
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
    fn write_business_reports_creates_japanese_filename_files() {
        let tmp = TempDir::new().unwrap();
        let reports_dir = tmp.path().join("260522-04").join("レポート");
        let docx = reports_dir.join("復旧レポート.docx");
        let invalid_txt = reports_dir.join("破損疑いファイル一覧.txt");
        let uncertain_txt = reports_dir.join("自動確認対象外ファイル一覧.txt");
        let html = reports_dir.join("業務管理レポート.html");
        let csv = reports_dir.join("report.csv");

        let report = empty_report();
        let paths =
            write_business_reports(&report, &docx, &invalid_txt, &uncertain_txt, &html, &csv)
                .unwrap();

        assert!(paths.customer_docx.exists());
        assert!(paths.customer_invalid_txt.exists());
        assert!(paths.customer_uncertain_txt.exists());
        assert!(paths.internal_html.exists());
        assert!(paths.csv.exists());

        // ファイル名がそのまま日本語であること。
        assert!(paths
            .customer_docx
            .to_string_lossy()
            .ends_with("復旧レポート.docx"));
        assert!(paths
            .customer_invalid_txt
            .to_string_lossy()
            .ends_with("破損疑いファイル一覧.txt"));
        assert!(paths
            .customer_uncertain_txt
            .to_string_lossy()
            .ends_with("自動確認対象外ファイル一覧.txt"));
        assert!(paths
            .internal_html
            .to_string_lossy()
            .ends_with("業務管理レポート.html"));
        // 内容も空ではない（最低 1 バイト書き出されている）。
        assert!(paths.customer_docx.metadata().unwrap().len() > 0);
        assert!(paths.customer_invalid_txt.metadata().unwrap().len() > 0);
        assert!(paths.customer_uncertain_txt.metadata().unwrap().len() > 0);
        assert!(paths.internal_html.metadata().unwrap().len() > 0);
        assert!(paths.csv.metadata().unwrap().len() > 0);
    }

    #[test]
    fn write_business_reports_creates_parent_dirs_automatically() {
        // 親ディレクトリがまだ存在しなくても、自動で作って書き出すこと。
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a").join("b").join("レポート");
        let docx = nested.join("復旧レポート.docx");
        let invalid_txt = nested.join("破損疑いファイル一覧.txt");
        let uncertain_txt = nested.join("自動確認対象外ファイル一覧.txt");
        let html = nested.join("業務管理レポート.html");
        let csv = nested.join("report.csv");

        write_business_reports(
            &empty_report(),
            &docx,
            &invalid_txt,
            &uncertain_txt,
            &html,
            &csv,
        )
        .unwrap();
        assert!(nested.is_dir());
        assert!(docx.exists());
    }

    #[test]
    fn write_business_reports_docx_is_zip_archive() {
        // 顧客向け .docx は OOXML ZIP として正しく書き出されること（PK\x03\x04 magic）。
        let tmp = TempDir::new().unwrap();
        let docx = tmp.path().join("復旧レポート.docx");
        let invalid_txt = tmp.path().join("破損疑いファイル一覧.txt");
        let uncertain_txt = tmp.path().join("自動確認対象外ファイル一覧.txt");
        let html = tmp.path().join("業務管理レポート.html");
        let csv = tmp.path().join("report.csv");

        write_business_reports(
            &empty_report(),
            &docx,
            &invalid_txt,
            &uncertain_txt,
            &html,
            &csv,
        )
        .unwrap();
        let bytes = std::fs::read(&docx).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }
}
