//! Chunk 24d-4-2: 業務診断書 DOCX の「業務管理用」セクション。
//!
//! 営業の内部確認 / 見積根拠 / 業務的判断のための技術情報を含む。
//! お客様には直接共有しない (情報量が業務的に多すぎる + 技術用語含む)。

use docx_rs::Docx;

use super::helpers::{add_blank_paragraph, add_h2, add_h3, add_paragraph, add_table_row};
use crate::case::Case;
use crate::diagnostic::DiagnosticInput;
use crate::explanation::{
    boot_sector_explanation, mft_corruption_explanation, BusinessExplanation,
};

/// 業務管理用セクション全体を追加する。
pub(super) fn add_internal_section(docx: Docx, case: &Case) -> Docx {
    let diag = &case.diagnostic_input;
    let mut docx = docx;

    // === 1. ファイルシステムの基本情報 ===
    docx = add_h2(docx, "1. ファイルシステムの基本情報");
    docx = add_filesystem_info(docx, diag);
    docx = add_blank_paragraph(docx);

    // === 2. Windows のマウント状態 ===
    docx = add_h2(docx, "2. Windows のマウント状態");
    docx = add_mount_state(docx, diag);
    docx = add_blank_paragraph(docx);

    // === 3. 業務的な評価 ===
    docx = add_h2(docx, "3. 業務的な評価");
    docx = add_business_evaluation(docx, diag);
    docx = add_blank_paragraph(docx);

    // === 4. 業務的な詳細説明 ===
    docx = add_h2(docx, "4. 業務的な詳細説明 (営業の判断材料)");
    docx = add_business_explanations(docx, diag);
    docx = add_blank_paragraph(docx);

    docx
}

/// 「1. ファイルシステムの基本情報」: FilesystemFindings から技術詳細。
fn add_filesystem_info(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let findings = diag.filesystem_findings.as_ref();
    let sig_text = if findings.map(|f| f.signature_valid).unwrap_or(false) {
        "正常 (NTFS 認識成功)"
    } else {
        "異常"
    };
    let mft_count = findings.map(|f| f.mft_corrupted_count).unwrap_or(0);
    let runlist_count = findings.map(|f| f.invalid_runlist_count).unwrap_or(0);
    let bs_text = if findings.map(|f| f.boot_sector_ok).unwrap_or(false) {
        "正常"
    } else {
        "異常"
    };
    let fs_type = diag.filesystem_type.as_deref().unwrap_or("(未検出)");

    let docx = add_table_row(docx, "ファイルシステム種別", fs_type);
    let docx = add_table_row(docx, "ファイルシステム署名", sig_text);
    let docx = add_table_row(docx, "MFT エントリ破損", &format!("{} 件", mft_count));
    let docx = add_table_row(docx, "不正な run-list", &format!("{} 件", runlist_count));
    add_table_row(docx, "Boot sector", bs_text)
}

/// 「2. Windows のマウント状態」: Dirty Bit / $LogFile / BitLocker。
fn add_mount_state(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let mut docx = docx;
    if let Some(s) = &diag.dirty_bit {
        docx = add_table_row(docx, "Dirty Bit", s.business_message());
    }
    if let Some(s) = &diag.log_file_status {
        docx = add_table_row(docx, "$LogFile 整合性", s.business_message());
    }
    if let Some(s) = &diag.bitlocker {
        docx = add_table_row(docx, "BitLocker 暗号化", s.business_message());
    }
    if diag.dirty_bit.is_none() && diag.log_file_status.is_none() && diag.bitlocker.is_none() {
        docx = add_paragraph(docx, "  (マウント状態情報なし)", false);
    }
    docx
}

/// 「3. 業務的な評価」: 推定ファイル数 / 復旧難易度 / 推定成功率 + 計算根拠。
fn add_business_evaluation(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let mut docx = docx;
    let mut any = false;

    if let Some(est) = &diag.file_estimation {
        docx = add_table_row(docx, "推定ファイル数", &est.business_summary());
        any = true;
    }
    if let Some(diff) = &diag.recovery_difficulty {
        docx = add_table_row(
            docx,
            "復旧難易度",
            &format!("{} ({})", diff.display_name(), diff.business_explanation()),
        );
        any = true;
    }
    if let Some(rate) = &diag.success_rate {
        docx = add_table_row(docx, "推定成功率", &rate.business_summary());
        if !rate.reasoning.is_empty() {
            docx = add_paragraph(docx, "  計算根拠:", true);
            for r in &rate.reasoning {
                docx = add_paragraph(docx, &format!("    ・{}", r), false);
            }
        }
        any = true;
    }

    if !any {
        docx = add_paragraph(docx, "  (業務指標が計算されていません)", false);
    }
    docx
}

/// 「4. 業務的な詳細説明」: 異常項目ごとに 5 セクションフル展開。
fn add_business_explanations(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let mut docx = docx;
    let mut count = 0;

    if let Some(s) = &diag.dirty_bit {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● Dirty Bit について");
            docx = render_explanation_full(docx, exp);
            count += 1;
        }
    }
    if let Some(s) = &diag.log_file_status {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● $LogFile 整合性について");
            docx = render_explanation_full(docx, exp);
            count += 1;
        }
    }
    if let Some(s) = &diag.bitlocker {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● BitLocker 暗号化について");
            docx = render_explanation_full(docx, exp);
            count += 1;
        }
    }

    let findings = diag.filesystem_findings.as_ref();
    let mft_count = findings.map(|f| f.mft_corrupted_count).unwrap_or(0) as u32;
    if let Some(exp) = mft_corruption_explanation(mft_count) {
        docx = add_h3(docx, "● MFT エントリ破損について");
        docx = render_explanation_full(docx, exp);
        count += 1;
    }

    let bs_damaged = !findings.map(|f| f.boot_sector_ok).unwrap_or(true);
    if let Some(exp) = boot_sector_explanation(bs_damaged) {
        docx = add_h3(docx, "● Boot sector について");
        docx = render_explanation_full(docx, exp);
        count += 1;
    }

    if let Some(diff) = &diag.recovery_difficulty {
        // 業務管理用は Easy も含めて全段階表示 (営業の判断材料)
        if let Some(exp) = diff.explanation() {
            docx = add_h3(docx, "● 復旧難易度について");
            docx = render_explanation_full(docx, exp);
            count += 1;
        }
    }

    if count == 0 {
        docx = add_paragraph(
            docx,
            "  特に異常な項目はありません。標準的な業務ケースとして処理可能です。",
            false,
        );
    }
    docx
}

/// 5 セクションフル展開 (業務管理用)。
fn render_explanation_full(docx: Docx, exp: &BusinessExplanation) -> Docx {
    let docx = add_paragraph(docx, "  【何が起きているか】", true);
    let docx = add_paragraph(docx, &format!("    {}", exp.what_happened), false);
    let docx = add_blank_paragraph(docx);

    let mut docx = add_paragraph(docx, "  【考えられる原因】", true);
    for cause in exp.causes {
        docx = add_paragraph(docx, &format!("    ・{}", cause), false);
    }
    let docx = add_blank_paragraph(docx);

    let docx = add_paragraph(docx, "  【Windows の挙動】", true);
    let docx = add_paragraph(docx, &format!("    {}", exp.windows_behavior), false);
    let docx = add_blank_paragraph(docx);

    let docx = add_paragraph(docx, "  【業務的な意味】", true);
    let docx = add_paragraph(docx, &format!("    {}", exp.business_meaning), false);
    let docx = add_blank_paragraph(docx);

    let docx = add_paragraph(docx, "  【お客様への説明例】", true);
    let docx = add_paragraph(
        docx,
        &format!("    「{}」", exp.customer_explanation),
        false,
    );
    add_blank_paragraph(docx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_id::CaseId;
    use crate::diagnostic::{
        DirtyBitStatus, FilesystemFindings, RecoveryDifficulty, SuccessRatePrediction,
    };

    fn case_with(diag: DiagnosticInput) -> Case {
        let mut c = Case::new(CaseId::parse("260603-22").unwrap());
        c.diagnostic_input = diag;
        c
    }

    #[test]
    fn internal_section_with_empty_diag_does_not_panic() {
        let case = case_with(DiagnosticInput::default());
        let docx = add_internal_section(Docx::new(), &case);
        let mut buf = Vec::new();
        docx.build().pack(std::io::Cursor::new(&mut buf)).unwrap();
        assert!(buf.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn internal_section_renders_full_explanation_for_dirty_bit() {
        let diag = DiagnosticInput {
            dirty_bit: Some(DirtyBitStatus::Dirty),
            ..Default::default()
        };
        let case = case_with(diag);
        let docx = add_internal_section(Docx::new(), &case);
        let mut buf = Vec::new();
        docx.build().pack(std::io::Cursor::new(&mut buf)).unwrap();

        // ZIP 展開して XML 内に 5 セクション見出しが入っているか
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
        let mut text = String::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).unwrap();
            if f.name().ends_with(".xml") {
                use std::io::Read;
                let mut s = String::new();
                f.read_to_string(&mut s).ok();
                text.push_str(&s);
            }
        }
        assert!(text.contains("【何が起きているか】"));
        assert!(text.contains("【お客様への説明例】"));
    }

    #[test]
    fn internal_section_shows_mft_explanation_when_corrupted() {
        let diag = DiagnosticInput {
            filesystem_findings: Some(FilesystemFindings {
                signature_valid: true,
                mft_corrupted_count: 7,
                invalid_runlist_count: 0,
                boot_sector_ok: true,
                other_issues: vec![],
            }),
            ..Default::default()
        };
        let case = case_with(diag);
        let docx = add_internal_section(Docx::new(), &case);
        let mut buf = Vec::new();
        docx.build().pack(std::io::Cursor::new(&mut buf)).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
        let mut text = String::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).unwrap();
            if f.name().ends_with(".xml") {
                use std::io::Read;
                let mut s = String::new();
                f.read_to_string(&mut s).ok();
                text.push_str(&s);
            }
        }
        // MFT 7 件 → MFT_CORRUPTION_LIGHT セクションが表示される
        assert!(text.contains("MFT エントリ破損について"));
    }

    #[test]
    fn internal_section_evaluation_block_has_calc_reasoning() {
        let diag = DiagnosticInput {
            recovery_difficulty: Some(RecoveryDifficulty::Medium),
            success_rate: Some(SuccessRatePrediction {
                overall_rate: 70,
                priority_rate: Some(80),
                reasoning: vec!["減点要因 A".into(), "減点要因 B".into()],
            }),
            ..Default::default()
        };
        let case = case_with(diag);
        let docx = add_internal_section(Docx::new(), &case);
        let mut buf = Vec::new();
        docx.build().pack(std::io::Cursor::new(&mut buf)).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
        let mut text = String::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).unwrap();
            if f.name().ends_with(".xml") {
                use std::io::Read;
                let mut s = String::new();
                f.read_to_string(&mut s).ok();
                text.push_str(&s);
            }
        }
        assert!(text.contains("計算根拠"));
        assert!(text.contains("減点要因 A"));
        assert!(text.contains("減点要因 B"));
    }
}
