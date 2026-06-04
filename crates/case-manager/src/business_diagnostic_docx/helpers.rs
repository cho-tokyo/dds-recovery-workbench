//! Chunk 24d-4-2: 業務診断書 DOCX のヘルパー関数群。
//!
//! 段落 / 見出し / セクション区切り / KV 行を組み立てる pure 関数。
//! 既存 [`docx_customer`](dds_report::docx_customer) のフォント方針 (Yu Gothic) を継続。

use docx_rs::{AlignmentType, Docx, LineSpacing, Paragraph, Run, RunFonts};

use crate::case::Case;

/// 日本語フォント (Yu Gothic) を east_asia に指定した `RunFonts` を返す。
pub(super) fn ja_fonts() -> RunFonts {
    RunFonts::new().east_asia("Yu Gothic")
}

/// ヘッダブロック (タイトル + 案件番号 + 診断日時) を追加する。
///
/// `case.diagnostic_input.diagnosed_at` が None の場合は `case.updated_at` を
/// フォールバックとして使用する (Chunk 24d-4-2 設計判断)。
pub(super) fn add_header(docx: Docx, case: &Case) -> Docx {
    let title = Paragraph::new()
        .add_run(
            Run::new()
                .add_text("業務診断書")
                .size(36)
                .bold()
                .fonts(ja_fonts()),
        )
        .align(AlignmentType::Center);

    let case_info = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(format!("案件番号: {}", case.case_id))
                .size(24)
                .fonts(ja_fonts()),
        )
        .align(AlignmentType::Center);

    // diagnosed_at が None なら updated_at をフォールバック表示
    let diag_time = case
        .diagnostic_input
        .diagnosed_at
        .unwrap_or(case.updated_at);
    let date_info = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(format!(
                    "診断日時: {}",
                    diag_time.format("%Y年%m月%d日 %H:%M:%S")
                ))
                .size(20)
                .fonts(ja_fonts()),
        )
        .align(AlignmentType::Center);

    docx.add_paragraph(title)
        .add_paragraph(case_info)
        .add_paragraph(date_info)
        .add_paragraph(Paragraph::new())
}

/// セクション区切り (例: ━━━ 業務管理用 ━━━)。中央寄せ・太字。
pub(super) fn add_section_divider(docx: Docx, title: &str) -> Docx {
    let divider = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(format!("━━━━━━━━━━ {} ━━━━━━━━━━", title))
                .size(24)
                .bold()
                .fonts(ja_fonts()),
        )
        .align(AlignmentType::Center);
    docx.add_paragraph(divider).add_paragraph(Paragraph::new())
}

/// 見出し H2 (28pt 太字)。
pub(super) fn add_h2(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(text).size(28).bold().fonts(ja_fonts())),
    )
}

/// 見出し H3 (24pt 太字)。
pub(super) fn add_h3(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(text).size(24).bold().fonts(ja_fonts())),
    )
}

/// 通常段落 (20pt)。`bold=true` で太字。
pub(super) fn add_paragraph(docx: Docx, text: &str, bold: bool) -> Docx {
    let mut run = Run::new().add_text(text).size(20).fonts(ja_fonts());
    if bold {
        run = run.bold();
    }
    docx.add_paragraph(Paragraph::new().add_run(run))
}

/// 空段落 (改行用)。
pub(super) fn add_blank_paragraph(docx: Docx) -> Docx {
    docx.add_paragraph(Paragraph::new())
}

/// 「ラベル: 値」形式の 1 行 (ラベル太字 + 値)。
pub(super) fn add_table_row(docx: Docx, label: &str, value: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text(format!("  {}: ", label))
                    .size(20)
                    .bold()
                    .fonts(ja_fonts()),
            )
            .add_run(Run::new().add_text(value).size(20).fonts(ja_fonts())),
    )
}

/// お客様向けの読みやすい段落 (22pt + 1.5 倍行間)。
pub(super) fn add_customer_friendly_paragraph(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(text).size(22).fonts(ja_fonts()))
            .line_spacing(LineSpacing::new().line(360)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_id::CaseId;

    #[test]
    fn ja_fonts_yu_gothic_set() {
        // RunFonts は内部 struct なので Debug 経由でフォント名確認 (脆いがフォント設定の事実を担保)。
        let f = ja_fonts();
        let dbg = format!("{:?}", f);
        assert!(dbg.contains("Yu Gothic"));
    }

    #[test]
    fn add_header_does_not_panic() {
        let case = Case::new(CaseId::parse("260601-99").unwrap());
        let docx = add_header(Docx::new(), &case);
        // build まで通れば成功
        let mut buf = Vec::new();
        docx.build()
            .pack(std::io::Cursor::new(&mut buf))
            .expect("pack ok");
        assert!(buf.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn helpers_chain_compose() {
        // 各ヘルパが Docx を返してチェイン可能であることの担保。
        let docx = Docx::new();
        let docx = add_section_divider(docx, "セクション X");
        let docx = add_h2(docx, "見出し H2");
        let docx = add_h3(docx, "見出し H3");
        let docx = add_paragraph(docx, "通常段落", false);
        let docx = add_paragraph(docx, "太字段落", true);
        let docx = add_blank_paragraph(docx);
        let docx = add_table_row(docx, "ラベル", "値");
        let docx = add_customer_friendly_paragraph(docx, "お客様向け本文");

        let mut buf = Vec::new();
        docx.build()
            .pack(std::io::Cursor::new(&mut buf))
            .expect("pack ok");
        assert!(buf.starts_with(b"PK\x03\x04"));
    }
}
