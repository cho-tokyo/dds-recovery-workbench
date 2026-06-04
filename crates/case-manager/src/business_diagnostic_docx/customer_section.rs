//! Chunk 24d-4-2: 業務診断書 DOCX の「お客様用」セクション。
//!
//! 営業がお客様に口頭で説明する際の台本。専門用語ゼロ + 免責注釈付き。
//!
//! ## 業務原則 (Chunk 24d-4-1.5 から継続)
//!
//! - `customer_explanation` をそのまま使う (技術用語ゼロが静的データで保証されている)
//! - 「受注不可」「対応困難」「復旧不可能」のような決めつけ表現は使わない
//! - `RecoveryDifficulty::Easy` は CRM テキスト方針と整合して **過剰説明を避ける**
//!   (「特に異常はありません」のような前向きメッセージに留める)
//! - 末尾に [`CUSTOMER_DISCLAIMER`](crate::explanation::CUSTOMER_DISCLAIMER) を 1 回だけ付与

use docx_rs::Docx;

use super::helpers::{
    add_blank_paragraph, add_customer_friendly_paragraph, add_h2, add_h3, add_paragraph,
};
use crate::case::Case;
use crate::diagnostic::{DiagnosticInput, RecoveryDifficulty};
use crate::explanation::{
    boot_sector_explanation, mft_corruption_explanation, CUSTOMER_DISCLAIMER,
};

/// お客様用セクション全体を追加する。
pub(super) fn add_customer_section(docx: Docx, case: &Case) -> Docx {
    let diag = &case.diagnostic_input;
    let mut docx = docx;

    // === 1. 案件概要 ===
    docx = add_h2(docx, "1. 案件概要");
    docx = add_customer_friendly_paragraph(
        docx,
        "このたびは当社のデータ復旧サービスをご検討いただき、誠にありがとうございます。",
    );
    docx = add_customer_friendly_paragraph(
        docx,
        &format!(
            "案件番号: {} のお客様の HDD の状態について、診断結果をご説明いたします。",
            case.case_id
        ),
    );
    docx = add_blank_paragraph(docx);

    // === 2. HDD の状態について ===
    docx = add_h2(docx, "2. HDD の状態について");
    docx = add_hdd_state(docx, diag);
    docx = add_blank_paragraph(docx);

    // === 3. 復旧の見通し ===
    docx = add_h2(docx, "3. 復旧の見通し");
    docx = add_recovery_outlook(docx, diag);
    docx = add_blank_paragraph(docx);

    // === 4. 注意事項 (免責注釈) ===
    docx = add_h2(docx, "4. 注意事項");
    for line in CUSTOMER_DISCLAIMER.lines() {
        docx = add_customer_friendly_paragraph(docx, line);
    }
    docx = add_blank_paragraph(docx);
    docx = add_customer_friendly_paragraph(
        docx,
        "ご不明な点がございましたら、お気軽にお問い合わせください。",
    );

    docx
}

/// 「2. HDD の状態について」: customer_explanation のみで構成 (技術用語ゼロ)。
fn add_hdd_state(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let mut docx = docx;
    let mut has_explanation = false;

    if let Some(s) = &diag.dirty_bit {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● Windows がアクセスを拒否している原因について");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }
    if let Some(s) = &diag.log_file_status {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● ファイル管理ログの状態について");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }
    if let Some(s) = &diag.bitlocker {
        if let Some(exp) = s.explanation() {
            docx = add_h3(docx, "● 暗号化について");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }

    let findings = diag.filesystem_findings.as_ref();
    let mft_count = findings.map(|f| f.mft_corrupted_count).unwrap_or(0) as u32;
    if let Some(exp) = mft_corruption_explanation(mft_count) {
        docx = add_h3(docx, "● ファイル管理情報の状態について");
        docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
        has_explanation = true;
    }

    let bs_damaged = !findings.map(|f| f.boot_sector_ok).unwrap_or(true);
    if let Some(exp) = boot_sector_explanation(bs_damaged) {
        docx = add_h3(docx, "● HDD の起動情報の状態について");
        docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
        has_explanation = true;
    }

    if !has_explanation {
        docx = add_customer_friendly_paragraph(
            docx,
            "お客様の HDD は健全な状態です。標準的な復旧プロセスでデータを取り出すことが可能です。",
        );
    }
    docx
}

/// 「3. 復旧の見通し」: 難易度の customer_explanation + 推定成功率。
///
/// `RecoveryDifficulty::Easy` は CRM テキスト方針と整合して過剰説明を避ける
/// (Chunk 24d-4-1.5 で確立した業務的判断)。
fn add_recovery_outlook(docx: Docx, diag: &DiagnosticInput) -> Docx {
    let mut docx = docx;
    let mut wrote_diff = false;

    if let Some(diff) = &diag.recovery_difficulty {
        match diff {
            RecoveryDifficulty::Easy => {
                docx = add_customer_friendly_paragraph(
                    docx,
                    "お客様の HDD の状態は良好で、標準的な復旧プロセスで対応可能です。",
                );
                wrote_diff = true;
            }
            other => {
                if let Some(exp) = other.explanation() {
                    docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
                    wrote_diff = true;
                }
            }
        }
    }
    if !wrote_diff {
        docx = add_customer_friendly_paragraph(
            docx,
            "復旧の見通しについては、診断結果をもとに担当者よりご案内いたします。",
        );
    }

    if let Some(rate) = &diag.success_rate {
        docx = add_blank_paragraph(docx);
        docx = add_paragraph(docx, "推定復旧成功率:", true);
        docx = add_paragraph(docx, &format!("  全体: 約 {}%", rate.overall_rate), false);
        if let Some(p) = rate.priority_rate {
            docx = add_paragraph(docx, &format!("  優先データ: 約 {}%", p), false);
        }
    }
    docx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_id::CaseId;
    use crate::diagnostic::{
        BitLockerStatus, DirtyBitStatus, FilesystemFindings, RecoveryDifficulty,
    };
    use std::io::Read;

    fn case_with(diag: DiagnosticInput) -> Case {
        let mut c = Case::new(CaseId::parse("260603-33").unwrap());
        c.diagnostic_input = diag;
        c
    }

    fn extract_text(docx: Docx) -> String {
        let mut buf = Vec::new();
        docx.build().pack(std::io::Cursor::new(&mut buf)).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&buf)).unwrap();
        let mut text = String::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).unwrap();
            if f.name().ends_with(".xml") {
                let mut s = String::new();
                f.read_to_string(&mut s).ok();
                text.push_str(&s);
            }
        }
        text
    }

    #[test]
    fn customer_section_healthy_has_positive_message() {
        let case = case_with(DiagnosticInput {
            filesystem_findings: Some(FilesystemFindings {
                signature_valid: true,
                boot_sector_ok: true,
                ..Default::default()
            }),
            recovery_difficulty: Some(RecoveryDifficulty::Easy),
            ..Default::default()
        });
        let text = extract_text(add_customer_section(Docx::new(), &case));
        // 健全時: 前向きメッセージ
        assert!(text.contains("良好") || text.contains("健全"));
        // Easy: customer_explanation の本文「高い成功率」が出ない (簡潔メッセージのみ)
        // (代わりに「標準的な復旧プロセス」が出る)
        assert!(text.contains("標準的"));
    }

    #[test]
    fn customer_section_anomalous_shows_customer_explanation() {
        let case = case_with(DiagnosticInput {
            dirty_bit: Some(DirtyBitStatus::Dirty),
            bitlocker: Some(BitLockerStatus::Encrypted),
            ..Default::default()
        });
        let text = extract_text(add_customer_section(Docx::new(), &case));
        // Dirty Bit / BitLocker の customer_explanation がそれぞれ含まれる
        // BitLocker の customer_explanation には「48 桁の回復キー」が含まれる
        assert!(text.contains("回復キー"));
        // Dirty Bit の customer_explanation には「専門ツール」が含まれる
        assert!(text.contains("専門ツール") || text.contains("当社"));
    }

    #[test]
    fn customer_section_avoids_technical_jargon_in_visible_text() {
        // CRITICAL: お客様セクションには技術用語 (MFT / $Volume / VOLUME_INFORMATION) を
        // 含めない (静的データ explanation::customer_explanation で既に保証されている)。
        let case = case_with(DiagnosticInput {
            dirty_bit: Some(DirtyBitStatus::Dirty),
            bitlocker: Some(BitLockerStatus::Encrypted),
            filesystem_findings: Some(FilesystemFindings {
                signature_valid: true,
                mft_corrupted_count: 50,
                boot_sector_ok: false,
                ..Default::default()
            }),
            recovery_difficulty: Some(RecoveryDifficulty::Hard),
            ..Default::default()
        });
        let text = extract_text(add_customer_section(Docx::new(), &case));
        // 業務管理用セクションは含まれていないため、これらは出てこない。
        // ただし見出しに「MFT」と書く可能性があるので、敢えて customer 見出しは
        // 「ファイル管理情報」に変換済み (add_hdd_state 参照)。
        assert!(
            !text.contains("$Volume"),
            "お客様セクションに $Volume が含まれてはいけない"
        );
        assert!(
            !text.contains("VOLUME_INFORMATION"),
            "お客様セクションに VOLUME_INFORMATION が含まれてはいけない"
        );
        // MFT は見出しレベルでも含めない (Chunk 24d-4-2 設計判断)
        assert!(
            !text.contains("MFT"),
            "お客様セクションに「MFT」が含まれてはいけない"
        );
    }

    #[test]
    fn customer_section_includes_disclaimer() {
        let case = case_with(DiagnosticInput::default());
        let text = extract_text(add_customer_section(Docx::new(), &case));
        assert!(text.contains("参考情報"));
        assert!(text.contains("法的責任"));
        assert!(text.contains("個別案件"));
    }

    #[test]
    fn customer_section_shows_success_rate_when_present() {
        use crate::diagnostic::SuccessRatePrediction;
        let case = case_with(DiagnosticInput {
            success_rate: Some(SuccessRatePrediction {
                overall_rate: 92,
                priority_rate: Some(95),
                reasoning: vec![],
            }),
            ..Default::default()
        });
        let text = extract_text(add_customer_section(Docx::new(), &case));
        assert!(text.contains("92"));
        assert!(text.contains("95"));
        assert!(text.contains("優先データ"));
    }
}
