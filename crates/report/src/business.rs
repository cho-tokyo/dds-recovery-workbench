//! Chunk 23 / Chunk 24a: 業務向けレポートライタ（日本語ファイル名版）。
//!
//! Chunk 24a で大きく簡素化:
//! - 納品 HDD には **`復旧レポート.docx` のみ** 出力。お客様が混乱しないシンプル構成。
//! - 社内向け詳細（`業務管理レポート.html` / `復旧詳細.csv`）は社内保存ディレクトリへ。
//! - TXT 系の出力は廃止 (Chunk 23.8 で導入したが Chunk 24a で削除)。
//! - CSV は UTF-8 BOM (0xEF 0xBB 0xBF) を先頭付加して Excel 文字化けを解消
//!   (実機ドライランフィードバック ④)。
//!
//! ## 設計判断: パスを 3 つ受け取る理由
//!
//! ```text
//! ✗ write_business_reports(report, &CaseOutput, &CaseStorage) → report → case-manager 循環依存
//! ○ write_business_reports(report, &Path, &Path, &Path)       → 依存なし
//! ```
//!
//! `CaseOutput` 由来のパス (納品 HDD 用) と `CaseStorage` 由来のパス (社内保存) を
//! 呼び出し側（`dds_case_manager::orchestration`）で展開して渡す。
//!
//! 関連 FR: FR-OUT-03 (日本語名対応), FR-OUT-05 (納品物簡素化, Chunk 24a),
//!         FR-OUT-06 (社内・お客様向けの分離, Chunk 24a), FR-REP-01〜05.

use std::path::{Path, PathBuf};

use dds_recovery::RecoveryReport;

use crate::csv::render_csv;
use crate::docx_customer::render_customer_docx;
use crate::error::ReportError;
use crate::html_internal::render_internal_html;

/// 業務向け 3 ファイルを生成するエントリ関数 (Chunk 24a で 5→3 に簡素化)。
///
/// 配置先:
/// ```text
/// 納品 HDD   {customer_docx_path}     ← G:\{案件番号}\レポート\復旧レポート.docx
/// 社内保存   {internal_html_path}     ← C:\cases\{案件番号}\業務管理レポート.html
/// 社内保存   {csv_path}               ← C:\cases\{案件番号}\復旧詳細.csv (BOM 付き)
/// ```
///
/// 各パスの親ディレクトリが未作成でも自動作成する (`create_dir_all`)。
/// CSV は UTF-8 BOM を先頭付加して Excel 文字化けを解消する。
pub fn write_business_reports(
    report: &RecoveryReport,
    customer_docx: &Path,
    internal_html: &Path,
    csv: &Path,
) -> Result<BusinessReportPaths, ReportError> {
    for p in [customer_docx, internal_html, csv] {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // 納品 HDD: お客様向け .docx のみ。
    std::fs::write(customer_docx, render_customer_docx(report)?)?;

    // 社内保存: 業務管理 HTML。
    std::fs::write(internal_html, render_internal_html(report)?)?;

    // 社内保存: CSV (UTF-8 BOM 先頭付加で Excel 文字化け解消)。
    let csv_body = render_csv(report)?;
    let mut csv_bytes = Vec::with_capacity(3 + csv_body.len());
    csv_bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    csv_bytes.extend_from_slice(csv_body.as_bytes());
    std::fs::write(csv, csv_bytes)?;

    Ok(BusinessReportPaths {
        customer_docx: customer_docx.to_path_buf(),
        internal_html: internal_html.to_path_buf(),
        csv: csv.to_path_buf(),
    })
}

/// [`write_business_reports`] が生成した 3 ファイルの絶対パス (Chunk 24a で 5→3 に簡素化)。
#[derive(Debug, Clone)]
pub struct BusinessReportPaths {
    /// 顧客向け Word レポートのパス（納品 HDD: `{case}/レポート/復旧レポート.docx`）。
    pub customer_docx: PathBuf,
    /// 社内向け業務管理レポートのパス（社内保存: `{storage}/{案件番号}/業務管理レポート.html`）。
    pub internal_html: PathBuf,
    /// 業務管理 CSV のパス（社内保存: `{storage}/{案件番号}/復旧詳細.csv`、UTF-8 BOM 付き）。
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
    fn write_business_reports_creates_three_files_with_japanese_names() {
        // Chunk 24a: 3 ファイル (docx / html / csv) が生成されることの基本確認。
        let delivery = TempDir::new().unwrap();
        let internal = TempDir::new().unwrap();
        let docx = delivery
            .path()
            .join("260522-04")
            .join("レポート")
            .join("復旧レポート.docx");
        let html = internal
            .path()
            .join("260522-04")
            .join("業務管理レポート.html");
        let csv = internal.path().join("260522-04").join("復旧詳細.csv");

        let paths = write_business_reports(&empty_report(), &docx, &html, &csv).unwrap();

        assert!(paths.customer_docx.exists());
        assert!(paths.internal_html.exists());
        assert!(paths.csv.exists());

        // ファイル名がそのまま日本語であること。
        assert!(paths
            .customer_docx
            .to_string_lossy()
            .ends_with("復旧レポート.docx"));
        assert!(paths
            .internal_html
            .to_string_lossy()
            .ends_with("業務管理レポート.html"));
        assert!(paths.csv.to_string_lossy().ends_with("復旧詳細.csv"));

        // 内容が空でない。
        assert!(paths.customer_docx.metadata().unwrap().len() > 0);
        assert!(paths.internal_html.metadata().unwrap().len() > 0);
        assert!(paths.csv.metadata().unwrap().len() > 0);
    }

    #[test]
    fn write_business_reports_creates_parent_dirs_automatically() {
        // 親ディレクトリ未作成でも write_business_reports が作る。
        let tmp = TempDir::new().unwrap();
        let nested_delivery = tmp.path().join("a").join("b").join("レポート");
        let nested_internal = tmp.path().join("x").join("y").join("案件");
        let docx = nested_delivery.join("復旧レポート.docx");
        let html = nested_internal.join("業務管理レポート.html");
        let csv = nested_internal.join("復旧詳細.csv");

        write_business_reports(&empty_report(), &docx, &html, &csv).unwrap();
        assert!(nested_delivery.is_dir());
        assert!(nested_internal.is_dir());
        assert!(docx.exists());
    }

    #[test]
    fn write_business_reports_csv_starts_with_utf8_bom() {
        // Chunk 24a: 復旧詳細.csv の先頭 3 バイトが UTF-8 BOM (0xEF 0xBB 0xBF)。
        // Excel で開いて文字化けしないことの保証 (実機ドライランフィードバック ④)。
        let tmp = TempDir::new().unwrap();
        let docx = tmp.path().join("復旧レポート.docx");
        let html = tmp.path().join("業務管理レポート.html");
        let csv = tmp.path().join("復旧詳細.csv");

        write_business_reports(&empty_report(), &docx, &html, &csv).unwrap();

        let csv_bytes = std::fs::read(&csv).unwrap();
        assert!(csv_bytes.len() >= 3);
        assert_eq!(&csv_bytes[..3], &[0xEF, 0xBB, 0xBF], "UTF-8 BOM 必須");
    }

    #[test]
    fn write_business_reports_docx_is_zip_archive() {
        // 顧客向け .docx は OOXML ZIP として正しく書き出されること (PK\x03\x04 magic)。
        let tmp = TempDir::new().unwrap();
        let docx = tmp.path().join("復旧レポート.docx");
        let html = tmp.path().join("業務管理レポート.html");
        let csv = tmp.path().join("復旧詳細.csv");

        write_business_reports(&empty_report(), &docx, &html, &csv).unwrap();
        let bytes = std::fs::read(&docx).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn write_business_reports_does_not_create_txt_files() {
        // Chunk 24a: 廃止された TXT 系ファイルが生成されないこと (回帰防止)。
        let tmp = TempDir::new().unwrap();
        let docx_dir = tmp.path().join("delivery");
        let storage_dir = tmp.path().join("internal");
        std::fs::create_dir_all(&docx_dir).unwrap();
        std::fs::create_dir_all(&storage_dir).unwrap();

        let docx = docx_dir.join("復旧レポート.docx");
        let html = storage_dir.join("業務管理レポート.html");
        let csv = storage_dir.join("復旧詳細.csv");

        write_business_reports(&empty_report(), &docx, &html, &csv).unwrap();

        // 旧 TXT (Chunk 23.8 で導入、Chunk 24a で削除) が **存在しない**こと。
        assert!(!docx_dir.join("破損疑いファイル一覧.txt").exists());
        assert!(!docx_dir.join("自動確認対象外ファイル一覧.txt").exists());
        // 旧 HTML / CSV が納品 HDD 側にない (社内保存に移動済み)。
        assert!(!docx_dir.join("業務管理レポート.html").exists());
        assert!(!docx_dir.join("report.csv").exists());
    }
}
