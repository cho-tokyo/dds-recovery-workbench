//! Chunk 20.5: 顧客向け .docx レポート生成。
//!
//! Word で開いて編集 → PDF 化して納品するワークフローを前提とした業務適用版。
//!
//! **設計原則（最重要）**: `internal_note_ja` を**絶対に**含めない。
//! 業務的にお客様に共有してはならない内部メモが漏れることを機械テスト
//! （`zip::ZipArchive` で .docx を解凍して XML 文字列検索）で防ぐ。
//!
//! 構造:
//! - 会社名（右寄せ） + タイトル（中央寄せ） + 作成日
//! - ご指定条件（`wish_labels`）
//! - 復旧結果サマリ / 品質確認テーブル
//! - 要確認ファイル概要（上位 5 グループ、件数 + 主な理由）
//! - 復旧データ量（人間可読バイト）
//! - フッター（会社名）
//!
//! 関連 FR: FR-REP-01 (顧客向け復旧レポート出力), FR-REP-04 (業務指標可視化)。

use chrono::Local;
use docx_rs::{AlignmentType, Docx, Paragraph, Run, Table, TableCell, TableRow};

use dds_recovery::RecoveryReport;

use crate::error::ReportError;
use crate::format::format_bytes;

/// 顧客向けに表示する会社名（業務適用版で必須）。
pub const COMPANY_NAME: &str = "デジタルデータソリューション株式会社";

/// 顧客向け .docx レポートをバイト列で生成する。
///
/// `Vec<u8>` には完全な OOXML ZIP アーカイブが含まれる。`std::fs::write` で
/// `.docx` 拡張子のファイルとして保存すれば Word / LibreOffice で開ける。
///
/// 含まれない情報（顧客への漏洩防止）:
/// - `internal_note_ja` (CS 内部メモ)
/// - 個別ファイル名（recovered_files.txt に分離）
/// - SHA256 / 出力先パス
/// - 技術的 diagnostics
pub fn render_customer_docx(report: &RecoveryReport) -> Result<Vec<u8>, ReportError> {
    let date = Local::now().format("%Y年%m月%d日").to_string();

    let mut docx = Docx::new();

    // === ヘッダー: 会社名（右寄せ、小さめ） + タイトル ===
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Right)
            .add_run(Run::new().add_text(COMPANY_NAME).size(20)),
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text("データ復旧レポート").size(40).bold()),
    );
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(format!("作成日: {}", date))),
    );
    docx = docx.add_paragraph(Paragraph::new());

    // === ご指定条件（Wish::label のリスト） ===
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text("■ ご指定条件").size(28).bold()),
    );
    if report.wish_labels.is_empty() {
        docx = docx
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("  (条件指定なし)")));
    } else {
        for label in &report.wish_labels {
            docx = docx.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(format!("  「{}」", label))),
            );
        }
    }
    docx = docx.add_paragraph(Paragraph::new());

    // === 復旧結果サマリ ===
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 復旧結果サマリ").size(28).bold()),
    );
    let summary_rows = vec![
        make_kv_row("該当ファイル数", &format!("{} 件", report.total_matched)),
        make_kv_row(
            "復旧成功",
            &format!(
                "{} 件 ({:.1}%)",
                report.recovered.len(),
                report.recovery_success_rate()
            ),
        ),
    ];
    docx = docx.add_table(Table::new(summary_rows));
    docx = docx.add_paragraph(Paragraph::new());

    // === 品質確認 ===
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text("■ 品質確認").size(28).bold()),
    );
    let valid = report.validated_count();
    let invalid = report.invalid_count();
    let uncertain = report.uncertain_count();
    let quality_rows = vec![
        make_kv_row(
            "正常確認済み",
            &format!("{} 件 ({:.1}%)", valid, report.quality_assurance_rate()),
        ),
        make_kv_row("要ご確認", &format!("{} 件", invalid)),
        make_kv_row("自動確認対象外", &format!("{} 件", uncertain)),
    ];
    docx = docx.add_table(Table::new(quality_rows));
    docx = docx.add_paragraph(Paragraph::new());

    // === 要ご確認のファイル概要（Invalid グループのトップ 5）===
    if invalid > 0 {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("■ 要ご確認のファイルについて").size(28).bold()),
        );
        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(format!(
            "合計 {} 件のファイルに品質上の懸念があります。",
            invalid
        ))));
        docx = docx.add_paragraph(Paragraph::new());
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("主な内訳:").bold()),
        );

        let grouped = report.invalid_grouped_by_reason();
        for (reason, entries) in grouped.iter().take(5) {
            docx = docx.add_paragraph(Paragraph::new().add_run(
                Run::new().add_text(format!("  ・{}: {} 件", reason, entries.len())),
            ));
        }
        docx = docx.add_paragraph(Paragraph::new());
        docx = docx.add_paragraph(Paragraph::new().add_run(
            Run::new()
                .add_text("詳細なファイル一覧は、別添「recovered_files.txt」をご参照ください。")
                .italic(),
        ));
        docx = docx.add_paragraph(Paragraph::new());
    }

    // === 復旧データ量 ===
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 復旧データ量").size(28).bold()),
    );
    docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(format!(
        "  合計: {}",
        format_bytes(report.total_bytes_written())
    ))));

    // === フッター ===
    docx = docx.add_paragraph(Paragraph::new());
    docx = docx.add_paragraph(Paragraph::new());
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(
                Run::new()
                    .add_text("ご不明な点がございましたら、担当者までお問い合わせください。")
                    .size(18),
            ),
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text(COMPANY_NAME).size(20).bold()),
    );

    // === パック ===
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
        TableCell::new()
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text(label).bold())),
        TableCell::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(value))),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::ValidationResult;
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
    fn customer_docx_contains_company_name() {
        // 会社名「デジタルデータソリューション株式会社」が必ず含まれること。
        let bytes = render_customer_docx(&build_report(vec![], vec![])).unwrap();
        // .docx は ZIP (OOXML)
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let text = extract_docx_text(&bytes);
        assert!(text.contains(COMPANY_NAME), "会社名が含まれること");
    }

    #[test]
    fn customer_docx_contains_wish_labels() {
        let report = build_report(
            vec!["お客様の写真".into(), "重要な書類".into()],
            vec![],
        );
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(text.contains("お客様の写真"));
        assert!(text.contains("重要な書類"));
    }

    #[test]
    fn customer_docx_excludes_internal_note() {
        // 最重要: internal_note_ja の文言が顧客 .docx に含まれてはならない。
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG として開けない可能性があります",
            "再復旧推奨 / CS 確認案件", // ← 内部メモ
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
    fn customer_docx_contains_summary_metrics() {
        // 業務指標（該当件数、復旧成功率、品質保証率の表現）が含まれること。
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let report = build_report(
            vec!["写真".into()],
            vec![entry("\\a.png", Some(v_ok))],
        );
        let bytes = render_customer_docx(&report).unwrap();
        let text = extract_docx_text(&bytes);
        // 該当ファイル数 / 復旧成功 / 正常確認済み / 復旧データ量 のラベルが含まれる。
        assert!(text.contains("該当ファイル数"));
        assert!(text.contains("復旧成功"));
        assert!(text.contains("正常確認済み"));
        assert!(text.contains("復旧データ量"));
    }
}
