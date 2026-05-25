//! Chunk 20.5: 顧客向け要確認ファイル一覧 TXT 生成。
//!
//! `recovered_files.txt` ファイルとして納品物の一部に含める想定。
//! Notepad（Windows 10/11 デフォルト）で問題なく開ける UTF-8（BOM なし）。
//!
//! 構造:
//! - ヘッダー（タイトル + 作成日） + 案内文
//! - Invalid なファイルのみ収集、フォルダ単位（`\` 区切り）で `BTreeMap` グルーピング
//! - 各フォルダで `==== {folder} ====` の見出し + 各ファイル名のリスト
//! - 合計件数 + フッター（会社名）
//!
//! 業務シナリオ: 万件規模の Invalid でも Notepad で読める粒度に集約する。
//!
//! 関連 FR: FR-REP-01 (顧客向け復旧レポート出力), FR-REP-05 (大規模ファイル対応)。

use std::collections::BTreeMap;

use chrono::Local;

use dds_recovery::RecoveryReport;

use crate::docx_customer::COMPANY_NAME;

/// 顧客向け要確認ファイル一覧 TXT を生成する。
///
/// 戻り値は UTF-8 文字列。`std::fs::write` で `recovered_files.txt` として保存する。
/// Invalid なファイルが 1 件も無い場合は「(要確認ファイルはありません)」の旨を出力。
pub fn render_invalid_files_txt(report: &RecoveryReport) -> String {
    let mut content = String::new();

    content.push_str("要確認ファイル一覧\n");
    content.push_str("====================\n\n");
    content.push_str(&format!(
        "作成日: {}\n\n",
        Local::now().format("%Y年%m月%d日")
    ));

    content.push_str("このリストは、復旧したものの破損の可能性があるファイルです。\n");
    content.push_str("お手元のディスクで実際に開いて、内容をご確認ください。\n\n");

    // Invalid のみ抽出。
    let invalid_entries: Vec<_> = report
        .recovered
        .iter()
        .filter(|e| {
            e.validation
                .as_ref()
                .map(|v| v.status.is_invalid())
                .unwrap_or(false)
        })
        .collect();

    if invalid_entries.is_empty() {
        content.push_str("(要確認ファイルはありません)\n\n");
        content.push_str(&format!("{}\n", COMPANY_NAME));
        return content;
    }

    // フォルダ単位グルーピング: 最後の '\' で分割。'\' 無しのケースもケア。
    let mut by_folder: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for entry in &invalid_entries {
        let path = entry.original_path.as_str();
        let (folder, filename) = split_folder_filename(path);
        by_folder.entry(folder).or_default().push(filename);
    }

    for (folder, files) in &by_folder {
        let folder_display = if folder.is_empty() || folder == "\\" {
            "(ルート)"
        } else {
            folder.as_str()
        };
        content.push_str(&format!("==== {} ====\n", folder_display));
        for filename in files {
            content.push_str(&format!("  {}\n", filename));
        }
        content.push('\n');
    }

    content.push_str(&format!("合計: {} ファイル\n\n", invalid_entries.len()));
    content.push_str("ご不明な点は、担当者までお問い合わせください。\n");
    content.push_str(&format!("{}\n", COMPANY_NAME));

    content
}

/// `\dir1\dir2\file.txt` → (`"\dir1\dir2"`, `"file.txt"`) に分割。
/// 区切り文字を含まないパスは `("", path)` を返す。
/// 先頭 `\` 直下のファイルは `("", "file.txt")` を返す（ルート扱い）。
fn split_folder_filename(path: &str) -> (String, &str) {
    match path.rfind('\\') {
        Some(0) => (String::new(), &path[1..]),
        Some(pos) => (path[..pos].to_string(), &path[pos + 1..]),
        None => (String::new(), path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::ValidationResult;
    use std::path::PathBuf;

    fn build_report(recovered: Vec<RecoveredEntry>) -> RecoveryReport {
        let now = Utc::now();
        let total_matched = recovered.len();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched,
            recovered,
            failed: vec![],
            skipped: vec![],
            wish_labels: vec![],
        }
    }

    fn invalid_entry(path: &str) -> RecoveredEntry {
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG として開けない可能性があります",
            "再復旧推奨",
        );
        RecoveredEntry {
            source_id: "NTFS#1".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/tmp/out"),
            bytes_written: 100,
            priority_score: 50,
            is_deleted: false,
            sha256: None,
            validation: Some(validation),
            matched_wish_labels: vec![],
            is_priority: false,
        }
    }

    fn valid_entry(path: &str) -> RecoveredEntry {
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        RecoveredEntry {
            source_id: "NTFS#2".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/tmp/out"),
            bytes_written: 100,
            priority_score: 50,
            is_deleted: false,
            sha256: None,
            validation: Some(validation),
            matched_wish_labels: vec![],
            is_priority: false,
        }
    }

    #[test]
    fn txt_groups_by_folder() {
        // 同じフォルダのファイルが連続 1 ブロックとして出力されること。
        let report = build_report(vec![
            invalid_entry("\\photos\\a.png"),
            invalid_entry("\\photos\\b.png"),
            invalid_entry("\\docs\\report.docx"),
        ]);
        let text = render_invalid_files_txt(&report);
        // \docs (BTreeMap でアルファベット順 → docs が先) の見出しが出る。
        assert!(text.contains("==== \\docs ===="));
        assert!(text.contains("==== \\photos ===="));
        // 同じフォルダ内の 2 ファイルが連続して並ぶ。
        let photos_block = text.split("==== \\photos ====").nth(1).unwrap();
        let head: String = photos_block.lines().take(3).collect::<Vec<_>>().join("\n");
        assert!(head.contains("a.png"));
        assert!(head.contains("b.png"));
    }

    #[test]
    fn txt_only_includes_invalid_entries() {
        // Valid は出力されない。
        let report = build_report(vec![
            invalid_entry("\\photos\\bad.png"),
            valid_entry("\\photos\\good.png"),
        ]);
        let text = render_invalid_files_txt(&report);
        assert!(text.contains("bad.png"));
        assert!(!text.contains("good.png"), "Valid は含まれない");
    }

    #[test]
    fn txt_includes_summary_line() {
        let report = build_report(vec![
            invalid_entry("\\a.png"),
            invalid_entry("\\b.png"),
            invalid_entry("\\c.png"),
        ]);
        let text = render_invalid_files_txt(&report);
        assert!(text.contains("合計: 3 ファイル"));
        // 会社名フッターも含まれる。
        assert!(text.contains(COMPANY_NAME));
    }

    #[test]
    fn txt_handles_root_files_correctly() {
        // 先頭 `\` 直下のファイル → (ルート) 扱い。
        let report = build_report(vec![invalid_entry("\\root_only.png")]);
        let text = render_invalid_files_txt(&report);
        assert!(text.contains("==== (ルート) ===="), "got: {}", text);
        assert!(text.contains("root_only.png"));
    }

    #[test]
    fn txt_zero_invalid_emits_friendly_message() {
        // Invalid 0 件のとき安心メッセージのみ出力。
        let report = build_report(vec![valid_entry("\\a.png")]);
        let text = render_invalid_files_txt(&report);
        assert!(text.contains("要確認ファイルはありません"));
        // 合計行は出ない。
        assert!(!text.contains("合計:"));
    }
}
