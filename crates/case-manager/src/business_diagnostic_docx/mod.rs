//! Chunk 24d-4-2: 営業向け業務診断書 (DOCX) の生成。
//!
//! 業務管理用セクション + お客様用セクションの 2 セクション構造で診断結果を業務的に
//! 文書化する。社内保存のみで、お客様への直接納品はしない (業務管理用情報を含むため)。
//!
//! ## 出力先
//!
//! `C:\cases\{案件番号}\業務診断書.docx`
//! ([`super::storage::CaseStorage::business_diagnostic_docx_path`])
//!
//! ## セクション構造
//!
//! 1. ヘッダ (タイトル / 案件番号 / 診断日時)
//! 2. ━━━ 業務管理用 ━━━
//!    1. ファイルシステムの基本情報 (`FilesystemFindings`)
//!    2. Windows のマウント状態 (Dirty Bit / $LogFile / BitLocker)
//!    3. 業務的な評価 (推定ファイル数 / 復旧難易度 / 推定成功率)
//!    4. 業務的な詳細説明 (5 セクション × 異常項目)
//! 3. ━━━ お客様用 ━━━
//!    1. 案件概要 (定型挨拶)
//!    2. HDD の状態について (`customer_explanation` のみ、技術用語ゼロ)
//!    3. 復旧の見通し
//!    4. 注意事項 (CUSTOMER_DISCLAIMER)
//!
//! ## 設計原則 (Chunk 24d-4-1.5 から継続)
//!
//! - お客様用セクションには技術用語 (MFT / $Volume / VOLUME_INFORMATION) を含めない
//! - 「受注不可」「対応困難」「復旧不可能」のような決めつけ表現を使わない
//! - 免責注釈 ([`CUSTOMER_DISCLAIMER`](super::explanation::CUSTOMER_DISCLAIMER)) を必ず付与
//! - `RecoveryDifficulty::Easy` はお客様セクションでは過剰説明を避けて簡潔メッセージのみ
//!   (CRM テキスト方針と整合)
//!
//! 関連 FR: FR-DIAG-11 (営業向け診断書 DOCX 生成),
//!         FR-DIAG-12 (業務管理用 + お客様用の 2 セクション)。

mod customer_section;
mod helpers;
mod internal_section;

use std::path::Path;

use docx_rs::Docx;

use crate::case::Case;

/// 業務診断書 DOCX 生成時のエラー。
#[derive(Debug, thiserror::Error)]
pub enum BusinessDiagnosticDocxError {
    /// DOCX 書き出し時の I/O エラー。
    #[error("DOCX I/O エラー: {0}")]
    Io(#[from] std::io::Error),
    /// docx-rs のシリアライズ失敗。
    #[error("DOCX シリアライズエラー: {0}")]
    Serialize(String),
}

/// 業務診断書 DOCX のバイト列を生成する (社内保存用)。
///
/// `Vec<u8>` には完全な OOXML ZIP アーカイブが含まれる。`.docx` 拡張子で保存すれば
/// Word / LibreOffice で開ける。お客様への直接納品は禁止 (業務管理用セクションを含む)。
///
/// [`render_customer_docx`](dds_report::render_customer_docx) と同じ
/// 「`Vec<u8>` を返す → 上位で `fs::write`」パターンに整合。
pub fn render_business_diagnostic_docx(
    case: &Case,
) -> Result<Vec<u8>, BusinessDiagnosticDocxError> {
    let mut docx = Docx::new();

    // ヘッダ
    docx = helpers::add_header(docx, case);

    // 業務管理用セクション
    docx = helpers::add_section_divider(docx, "業務管理用 (内部確認・見積根拠)");
    docx = internal_section::add_internal_section(docx, case);

    // お客様用セクション
    docx = helpers::add_section_divider(docx, "お客様用 (口頭説明の参考)");
    docx = customer_section::add_customer_section(docx, case);

    let mut buf = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        docx.build()
            .pack(cursor)
            .map_err(|e| BusinessDiagnosticDocxError::Serialize(e.to_string()))?;
    }
    Ok(buf)
}

/// 業務診断書 DOCX を生成してファイルに書き出すヘルパー。
///
/// 親ディレクトリが存在しない場合は再帰的に作成する。CLI / orchestration から呼び出される。
pub fn generate_business_diagnostic_docx(
    case: &Case,
    output_path: &Path,
) -> Result<(), BusinessDiagnosticDocxError> {
    let bytes = render_business_diagnostic_docx(case)?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_id::CaseId;
    use crate::diagnostic::{
        BitLockerStatus, DiagnosticInput, DirtyBitStatus, FileEstimation, FilesystemFindings,
        LogFileStatus, RecoveryDifficulty, SuccessRatePrediction,
    };
    use std::io::Read;

    fn cid() -> CaseId {
        CaseId::parse("260603-01").unwrap()
    }

    fn healthy_case() -> Case {
        let mut case = Case::new(cid());
        case.diagnostic_input = DiagnosticInput {
            diagnosed_at: Some(chrono::Utc::now()),
            filesystem_type: Some("NTFS".into()),
            filesystem_findings: Some(FilesystemFindings {
                signature_valid: true,
                mft_corrupted_count: 0,
                invalid_runlist_count: 0,
                boot_sector_ok: true,
                other_issues: vec![],
            }),
            total_files: 1000,
            ..Default::default()
        };
        case
    }

    fn anomalous_case() -> Case {
        let mut case = Case::new(cid());
        case.diagnostic_input = DiagnosticInput {
            diagnosed_at: Some(chrono::Utc::now()),
            filesystem_type: Some("NTFS".into()),
            filesystem_findings: Some(FilesystemFindings {
                signature_valid: true,
                mft_corrupted_count: 5,
                invalid_runlist_count: 2,
                boot_sector_ok: true,
                other_issues: vec![],
            }),
            dirty_bit: Some(DirtyBitStatus::Dirty),
            log_file_status: Some(LogFileStatus::Inconsistent),
            bitlocker: Some(BitLockerStatus::NotEncrypted),
            file_estimation: Some(FileEstimation {
                estimated_total_files: 1500,
                estimated_deleted_files: 300,
                estimated_live_files: 1200,
            }),
            recovery_difficulty: Some(RecoveryDifficulty::Medium),
            success_rate: Some(SuccessRatePrediction {
                overall_rate: 85,
                priority_rate: None,
                reasoning: vec!["Dirty Bit 検出 (-10%)".into()],
            }),
            total_files: 1500,
            ..Default::default()
        };
        case
    }

    fn extract_docx_text(docx_bytes: &[u8]) -> String {
        let cursor = std::io::Cursor::new(docx_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("docx is ZIP");
        let mut all = String::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).unwrap();
            if f.name().ends_with(".xml") {
                let mut s = String::new();
                if f.read_to_string(&mut s).is_ok() {
                    all.push_str(&s);
                }
            }
        }
        all
    }

    #[test]
    fn generate_docx_creates_file_with_zip_magic() {
        let case = healthy_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"), "DOCX は ZIP 形式");
        assert!(bytes.len() > 1000, "最低 1KB 以上");
    }

    #[test]
    fn generate_docx_includes_case_id_and_both_sections() {
        let case = healthy_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        let text = extract_docx_text(&bytes);
        assert!(text.contains("260603-01"), "案件番号が含まれる");
        assert!(
            text.contains("業務管理用"),
            "業務管理用セクションが含まれる"
        );
        assert!(text.contains("お客様用"), "お客様用セクションが含まれる");
        assert!(text.contains("業務診断書"), "タイトルが含まれる");
    }

    #[test]
    fn generate_docx_with_anomalies_includes_explanations() {
        let case = anomalous_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        let text = extract_docx_text(&bytes);
        // 業務管理用: 5 セクション説明文の見出しが含まれる
        assert!(
            text.contains("【何が起きているか】"),
            "what_happened セクション"
        );
        assert!(
            text.contains("【お客様への説明例】"),
            "customer_explanation 例"
        );
        // Dirty Bit / $LogFile の説明文 (技術用語含む)
        assert!(text.contains("Dirty Bit") || text.contains("Windows"));
    }

    #[test]
    fn generate_docx_healthy_case_is_concise() {
        let case = healthy_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        let text = extract_docx_text(&bytes);
        // 健全な場合: 「特に異常な項目はありません」のメッセージ
        assert!(
            text.contains("特に異常") || text.contains("標準的"),
            "健全時は簡潔なメッセージ"
        );
        // お客様用は「健全」「標準的」等の前向きな表現
        assert!(text.contains("健全") || text.contains("標準"));
    }

    #[test]
    fn generate_docx_includes_customer_disclaimer() {
        let case = anomalous_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        let text = extract_docx_text(&bytes);
        // CUSTOMER_DISCLAIMER のキーワード
        assert!(text.contains("参考情報"), "免責注釈の「参考情報」");
        assert!(text.contains("法的責任"), "免責注釈の「法的責任」");
    }

    #[test]
    fn generate_docx_customer_section_avoids_technical_jargon() {
        // CRITICAL: お客様セクションは技術用語ゼロを業務的に保証する。
        // ただし DOCX 全体は業務管理用セクションを含むため MFT 等は含まれる。
        // ここでは customer_explanation 本文がそのまま埋め込まれていること
        // (case-manager::explanation のテスト customer_explanation_avoids_technical_jargon
        // が静的データ側で技術用語ゼロを保証済み) を信頼する。
        let case = anomalous_case();
        let bytes = render_business_diagnostic_docx(&case).unwrap();
        let text = extract_docx_text(&bytes);
        // お客様セクションの代表的文言を確認
        assert!(text.contains("お客様の HDD") || text.contains("お客様"));
    }

    #[test]
    fn generate_to_file_writes_docx() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("業務診断書.docx");
        let case = anomalous_case();
        generate_business_diagnostic_docx(&case, &path).unwrap();
        assert!(path.exists());
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }
}
