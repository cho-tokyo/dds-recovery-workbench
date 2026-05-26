//! Chunk 20.5 / Chunk 24a: 顧客向け .docx レポート生成。
//!
//! Word で開いて編集 → PDF 化して納品するワークフローを前提とした業務適用版。
//!
//! Chunk 24a の改訂で「業務的に誤解を生む表示」を全削除し、お客様向けに最小限の
//! 情報のみを残す **簡素化レイアウト** に変更。
//!
//! ## レイアウト (Chunk 24a)
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │       データ復旧レポート              │
//! │                                      │
//! │ ■ 復旧結果                            │
//! │   通常ファイル   : N 件                │
//! │   削除ファイル   : N 件                │
//! │   合計           : N 件、合計 NN GB    │
//! │                                      │
//! │ ■ ご指定優先データ (Wishlist 指定時のみ)│
//! │   該当ファイル   : N 件                │
//! │   ご指定条件     : 「写真」、「書類」   │
//! │                                      │
//! │ ■ お問い合わせ先                       │
//! │   Digital Data Solution 株式会社       │
//! └──────────────────────────────────────┘
//! ```
//!
//! ## Chunk 24a で削除した表示 (お客様に誤解を生む)
//!
//! - 復旧実施日時 (日付の意味合いがお客様に伝わりにくい)
//! - 品質保証率 / Valid / Invalid / Uncertain (内部品質判定)
//! - 自動確認対象外の内訳 (CS 用情報)
//! - 形式別ブレイクダウン
//! - 業務メトリクスを誇張する表現
//!
//! ## 設計原則 (最重要、Chunk 20.5 から継続)
//!
//! `internal_note_ja` を**絶対に**含めない。業務的にお客様に共有してはならない内部メモが
//! 漏れることを機械テスト (`zip::ZipArchive` で .docx を解凍して XML 文字列検索) で防ぐ。
//!
//! 関連 FR: FR-REP-01 (顧客向け復旧レポート出力), FR-OUT-05 (納品物簡素化, Chunk 24a).

use docx_rs::{Docx, Paragraph, Run, Table, TableCell, TableRow};

use dds_recovery::RecoveryReport;

use crate::error::ReportError;
use crate::format::format_bytes;

/// 顧客向けに表示する会社名 (お問い合わせ先案内に使用)。
pub const COMPANY_NAME: &str = "Digital Data Solution 株式会社";

/// 顧客向け .docx レポート（簡素化版、Chunk 24a）をバイト列で生成する。
///
/// `Vec<u8>` には完全な OOXML ZIP アーカイブが含まれる。`std::fs::write` で
/// `.docx` 拡張子のファイルとして保存すれば Word / LibreOffice で開ける。
///
/// 含まれない情報（顧客への誤解防止 / 漏洩防止）:
/// - `internal_note_ja` (CS 内部メモ、Chunk 20.5 から継続)
/// - 個別ファイル名 / SHA256 / 出力先パス (Chunk 20.5 から継続)
/// - 品質保証率 / Valid / Invalid / Uncertain (Chunk 24a で追加削除)
/// - 復旧実施日時 (Chunk 24a で追加削除)
/// - 自動確認対象外の内訳 (Chunk 24a で追加削除)
pub fn render_customer_docx(report: &RecoveryReport) -> Result<Vec<u8>, ReportError> {
    let mut docx = Docx::new();

    // ===== タイトル =====
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text("データ復旧レポート").size(40).bold()),
    );
    docx = docx.add_paragraph(Paragraph::new());

    // ===== 復旧結果 =====
    docx = docx
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("■ 復旧結果").size(28).bold()));

    let live_count = report.recovered.iter().filter(|e| !e.is_deleted).count();
    let deleted_count = report.recovered.iter().filter(|e| e.is_deleted).count();
    let total_count = report.recovered.len();

    docx = docx.add_table(Table::new(vec![
        make_kv_row("通常ファイル", &format!("{} 件", live_count)),
        make_kv_row("削除ファイル", &format!("{} 件", deleted_count)),
        make_kv_row(
            "合計",
            &format!(
                "{} 件、{}",
                total_count,
                format_bytes(report.total_bytes_written())
            ),
        ),
    ]));
    docx = docx.add_paragraph(Paragraph::new());

    // ===== ご指定優先データ (Wishlist 指定時のみ) =====
    if report.priority_count() > 0 {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("■ ご指定優先データ").size(28).bold()),
        );
        docx = docx.add_table(Table::new(vec![
            make_kv_row("該当ファイル", &format!("{} 件", report.priority_count())),
            make_kv_row("ご指定条件", &report.wish_labels.join("、")),
        ]));
        docx = docx.add_paragraph(Paragraph::new());
    }

    // ===== お問い合わせ先 =====
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text("■ お問い合わせ先").size(28).bold()),
    );
    docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(format!(
        "復旧データに関するお問い合わせは、{}までご連絡ください。",
        COMPANY_NAME
    ))));

    // ===== パック =====
    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        docx.build()
            .pack(cursor)
            .map_err(|e| ReportError::Template(format!("docx pack error: {}", e)))?;
    }
    Ok(buf)
}

/// `label: value` 形式の 1 行を 2 列テーブル行として組み立てるヘルパー。
fn make_kv_row(label: &str, value: &str) -> TableRow {
    TableRow::new(vec![
        TableCell::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(label).bold())),
        TableCell::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(value))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::{UncertainReason, ValidationResult};
    use std::io::Read;
    use std::path::PathBuf;

    fn build_report(wish_labels: Vec<String>, recovered: Vec<RecoveredEntry>) -> RecoveryReport {
        let now = Utc::now();
        let total_matched = recovered.len();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched,
            recovered,
            failed: vec![],
            skipped: vec![],
            wish_labels,
        }
    }

    fn entry(path: &str, validation: Option<ValidationResult>) -> RecoveredEntry {
        RecoveredEntry {
            source_id: "NTFS#1".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/tmp/out"),
            bytes_written: 4096,
            priority_score: 100,
            is_deleted: false,
            sha256: None,
            validation,
            matched_wish_labels: vec![],
            is_priority: false,
        }
    }

    /// .docx (ZIP) を展開し、全 .xml の中身を文字列連結して返すテストヘルパー。
    fn extract_docx_text(docx_bytes: &[u8]) -> String {
        let cursor = std::io::Cursor::new(docx_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("docx is a ZIP archive");
        let mut all_text = String::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            if file.name().ends_with(".xml") {
                let mut content = String::new();
                file.read_to_string(&mut content).unwrap();
                all_text.push_str(&content);
            }
        }
        all_text
    }

    #[test]
    fn customer_docx_contains_title() {
        // タイトル「データ復旧レポート」と会社名 (お問い合わせ先) が含まれる。
        let bytes = render_customer_docx(&build_report(vec![], vec![])).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let text = extract_docx_text(&bytes);
        assert!(text.contains("データ復旧レポート"));
        assert!(text.contains(COMPANY_NAME));
    }

    #[test]
    fn customer_docx_shows_recovery_counts() {
        // 通常 / 削除 / 合計の件数が表示される。
        let mut live_entry = entry("\\dir\\a.png", None);
        live_entry.is_deleted = false;
        let mut del_entry = entry("\\dir\\b.png", None);
        del_entry.is_deleted = true;
        let report = build_report(vec![], vec![live_entry, del_entry]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(text.contains("通常ファイル"));
        assert!(text.contains("削除ファイル"));
        assert!(text.contains("合計"));
    }

    #[test]
    fn customer_docx_omits_quality_metrics() {
        // Chunk 24a: 品質保証率 / Valid / Invalid / Uncertain 表示なし (お客様向け簡素化)。
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let report = build_report(vec!["写真".into()], vec![entry("\\a.png", Some(v_ok))]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(!text.contains("品質保証率"), "品質保証率は削除されている");
        assert!(!text.contains("品質確認"));
        assert!(!text.contains("正常確認済み"));
        assert!(!text.contains("要ご確認"));
        // 「自動確認対象外」キーワードも本文から消えていること。
        assert!(!text.contains("自動確認対象外"));
    }

    #[test]
    fn customer_docx_omits_recovery_datetime() {
        // Chunk 24a: 復旧実施日時 (「作成日:」等) の表示なし。
        let report = build_report(vec![], vec![]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(!text.contains("作成日"), "作成日は削除されている");
        assert!(!text.contains("復旧実施日時"));
    }

    #[test]
    fn customer_docx_shows_priority_section_when_priority_present() {
        // Wishlist マッチ (is_priority=true) があれば「ご指定優先データ」セクションを表示。
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let mut e = entry("\\photo.png", Some(v_ok));
        e.is_priority = true;
        let report = build_report(vec!["写真".into()], vec![e]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(text.contains("ご指定優先データ"));
        assert!(text.contains("写真"));
    }

    #[test]
    fn customer_docx_hides_priority_section_when_no_priority() {
        // priority_count == 0 のとき「ご指定優先データ」セクションは省略。
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let e = entry("\\photo.png", Some(v_ok));
        let report = build_report(vec![], vec![e]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(!text.contains("ご指定優先データ"));
    }

    #[test]
    fn customer_docx_excludes_internal_note() {
        // 最重要 (Chunk 20.5 から継続): internal_note_ja が顧客 .docx に含まれてはならない。
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG として開けない可能性があります",
            "再復旧推奨 / CS 確認案件",
        );
        let report = build_report(
            vec!["写真".into()],
            vec![entry("\\bad.png", Some(validation))],
        );
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(
            !text.contains("再復旧推奨"),
            "CS 内部メモは顧客 .docx に含まれてはならない"
        );
        assert!(!text.contains("CS 確認案件"));
    }

    #[test]
    fn customer_docx_omits_uncertain_breakdown() {
        // Chunk 24a: 「自動確認対象外について」の理由内訳セクションが完全削除されている。
        let v_unc = ValidationResult::uncertain(
            UncertainReason::NoValidatorAvailable,
            "diag",
            "ユーザー",
            "メモ",
        );
        let report = build_report(vec![], vec![entry("\\x.xyz", Some(v_unc))]);
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(!text.contains("自動確認対象外について"));
        assert!(!text.contains("対応 Validator なし"));
    }
}
