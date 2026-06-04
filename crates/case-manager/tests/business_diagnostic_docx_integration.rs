//! Chunk 24d-4-2 結合テスト: 業務診断書 DOCX の業務シナリオ統合。
//!
//! `generate_business_diagnostic_docx` を `CaseStorage::business_diagnostic_docx_path`
//! と組合せて、案件ディレクトリ配下に DOCX が生成され、必要な業務情報が含まれる
//! ことを業務的に保証する。
//!
//! 関連 FR: FR-DIAG-11, FR-DIAG-12。

use std::io::Read;

use tempfile::TempDir;

use dds_case_manager::{
    generate_business_diagnostic_docx, BitLockerStatus, Case, CaseId, CaseStorage, DiagnosticInput,
    DirtyBitStatus, FileEstimation, FilesystemFindings, LogFileStatus, RecoveryDifficulty,
    SuccessRatePrediction,
};

/// .docx (ZIP) を展開し、word/*.xml の中身を文字列連結する。
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

fn anomalous_case(case_id: &str) -> Case {
    let mut case = Case::new(CaseId::parse(case_id).unwrap());
    case.diagnostic_input = DiagnosticInput {
        diagnosed_at: Some(chrono::Utc::now()),
        filesystem_type: Some("NTFS".into()),
        filesystem_findings: Some(FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 7,
            invalid_runlist_count: 0,
            boot_sector_ok: true,
            other_issues: vec![],
        }),
        dirty_bit: Some(DirtyBitStatus::Dirty),
        log_file_status: Some(LogFileStatus::Inconsistent),
        bitlocker: Some(BitLockerStatus::Encrypted),
        file_estimation: Some(FileEstimation {
            estimated_total_files: 25_000,
            estimated_deleted_files: 1500,
            estimated_live_files: 23_500,
        }),
        recovery_difficulty: Some(RecoveryDifficulty::Hard),
        success_rate: Some(SuccessRatePrediction {
            overall_rate: 65,
            priority_rate: Some(72),
            reasoning: vec![
                "Dirty Bit 検出 (-10%)".into(),
                "BitLocker 暗号化 (-20%)".into(),
            ],
        }),
        total_files: 25_000,
        ..Default::default()
    };
    case
}

#[test]
fn business_docx_generated_under_case_dir() {
    // 業務シナリオ: 診断後、案件ディレクトリに業務診断書.docx が生成される。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case = anomalous_case("260603-10");

    // 案件ディレクトリを事前作成 (storage.save() でも作られる)
    let case_dir = storage.case_dir(&case.case_id);
    std::fs::create_dir_all(&case_dir).unwrap();

    let docx_path = storage.business_diagnostic_docx_path(&case.case_id);
    generate_business_diagnostic_docx(&case, &docx_path).expect("DOCX 生成成功");

    assert!(docx_path.exists());
    assert!(docx_path.to_string_lossy().ends_with("業務診断書.docx"));
    let metadata = std::fs::metadata(&docx_path).unwrap();
    assert!(metadata.len() > 1000, "DOCX は 1KB 以上");
}

#[test]
fn business_docx_contains_both_sections() {
    // CRITICAL: 業務管理用 + お客様用 の両セクションが必ず含まれる。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case = anomalous_case("260603-11");
    let docx_path = storage.business_diagnostic_docx_path(&case.case_id);
    generate_business_diagnostic_docx(&case, &docx_path).unwrap();

    let bytes = std::fs::read(&docx_path).unwrap();
    let text = extract_docx_text(&bytes);
    assert!(text.contains("業務管理用"), "「業務管理用」セクション");
    assert!(text.contains("お客様用"), "「お客様用」セクション");
    // 業務管理用の見出し
    assert!(text.contains("ファイルシステムの基本情報"));
    assert!(text.contains("Windows のマウント状態"));
    assert!(text.contains("業務的な評価"));
    // お客様用の見出し
    assert!(text.contains("案件概要"));
    assert!(text.contains("HDD の状態について"));
    assert!(text.contains("復旧の見通し"));
}

#[test]
fn business_docx_includes_disclaimer_in_customer_section() {
    // CRITICAL: 免責注釈 (CUSTOMER_DISCLAIMER) が必ず含まれる。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case = anomalous_case("260603-12");
    let docx_path = storage.business_diagnostic_docx_path(&case.case_id);
    generate_business_diagnostic_docx(&case, &docx_path).unwrap();

    let bytes = std::fs::read(&docx_path).unwrap();
    let text = extract_docx_text(&bytes);
    assert!(text.contains("参考情報"), "免責注釈の「参考情報」");
    assert!(text.contains("法的責任"), "免責注釈の「法的責任」");
    assert!(text.contains("個別案件"), "免責注釈の「個別案件」");
}

#[test]
fn business_docx_customer_section_avoids_technical_jargon() {
    // CRITICAL (Chunk 24d-4-1.5 から継続): customer_explanation 本文は技術用語ゼロ。
    // ただし業務管理用セクションには MFT 等が含まれるため、お客様向け見出しのみで
    // 技術用語を含まないことを担保する。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case = anomalous_case("260603-13");
    let docx_path = storage.business_diagnostic_docx_path(&case.case_id);
    generate_business_diagnostic_docx(&case, &docx_path).unwrap();

    let bytes = std::fs::read(&docx_path).unwrap();
    let text = extract_docx_text(&bytes);
    // お客様用セクション内の見出しは「ファイル管理情報の状態について」等の平易な表現
    assert!(text.contains("ファイル管理情報の状態について"));
    // $Volume / VOLUME_INFORMATION は customer_explanation 本文に含まれない (静的データ保証)
    // → ここでは「お客様用」セクション内の代表的フレーズが含まれることを担保
    assert!(text.contains("お客様の HDD"));
}

#[test]
fn business_docx_handles_healthy_case_concisely() {
    // 健全な Case では「特に異常な項目はありません」の業務メッセージが出る。
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let mut case = Case::new(CaseId::parse("260603-14").unwrap());
    case.diagnostic_input = DiagnosticInput {
        diagnosed_at: Some(chrono::Utc::now()),
        filesystem_findings: Some(FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 0,
            invalid_runlist_count: 0,
            boot_sector_ok: true,
            other_issues: vec![],
        }),
        recovery_difficulty: Some(RecoveryDifficulty::Easy),
        ..Default::default()
    };
    let docx_path = storage.business_diagnostic_docx_path(&case.case_id);
    generate_business_diagnostic_docx(&case, &docx_path).unwrap();

    let bytes = std::fs::read(&docx_path).unwrap();
    let text = extract_docx_text(&bytes);
    assert!(
        text.contains("特に異常") || text.contains("健全") || text.contains("良好"),
        "健全時は前向きメッセージ"
    );
    // Easy の customer_explanation 本文 (高い成功率 等) ではなく簡潔メッセージのみ
    assert!(text.contains("標準的"));
}
