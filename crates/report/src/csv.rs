//! Chunk 20: 外部システム連携用 CSV レポート生成。
//!
//! 13 列の固定スキーマで全フィールドを出力する。Excel 等での詳細分析や
//! 案件管理 DB への取り込みを想定。Phase 1 は BOM なし UTF-8（シンプル優先）。
//!
//! 列順序:
//! `source_id, original_path, output_path, bytes_written, is_deleted,
//! priority_score, sha256, validation_status, format_detected, validator_name,
//! customer_message, internal_note, diagnostics`
//!
//! 関連 FR: FR-REP-03 (外部システム連携用 CSV)。

use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;

/// CSV ヘッダー（13 列、列順序の単一の真実）。
const CSV_HEADER: &[&str; 13] = &[
    "source_id",
    "original_path",
    "output_path",
    "bytes_written",
    "is_deleted",
    "priority_score",
    "sha256",
    "validation_status",
    "format_detected",
    "validator_name",
    "customer_message",
    "internal_note",
    "diagnostics",
];

/// CSV レポートを生成する（全 13 フィールド含む）。
///
/// `csv` crate がカンマ・ダブルクオート・改行を自動でエスケープする。
/// 文字エンコーディングは UTF-8 (BOM なし)。
pub fn render_csv(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());

    wtr.write_record(CSV_HEADER)?;

    for entry in &report.recovered {
        let (status, format, validator_name, customer_msg, internal_note, diag) =
            match entry.validation.as_ref() {
                Some(v) => (
                    match v.status {
                        ValidationStatus::Valid => "valid",
                        ValidationStatus::Invalid => "invalid",
                        ValidationStatus::Uncertain => "uncertain",
                    },
                    v.format_detected.clone().unwrap_or_default(),
                    v.validator_name.clone(),
                    v.customer_message(),
                    v.internal_note().unwrap_or("").to_string(),
                    v.diagnostics.join("; "),
                ),
                None => (
                    "",
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            };

        wtr.write_record([
            entry.source_id.as_str(),
            entry.original_path.as_str(),
            &entry.output_path.display().to_string(),
            &entry.bytes_written.to_string(),
            &entry.is_deleted.to_string(),
            &entry.priority_score.to_string(),
            entry.sha256.as_deref().unwrap_or(""),
            status,
            format.as_str(),
            validator_name.as_str(),
            customer_msg.as_str(),
            internal_note.as_str(),
            diag.as_str(),
        ])?;
    }

    let data = wtr
        .into_inner()
        .map_err(|e| ReportError::Template(e.to_string()))?;
    String::from_utf8(data).map_err(|e| ReportError::Template(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::ValidationResult;
    use std::path::PathBuf;

    fn build_report(entries: Vec<RecoveredEntry>) -> RecoveryReport {
        let now = Utc::now();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: entries.len(),
            recovered: entries,
            failed: vec![],
            skipped: vec![],
        }
    }

    fn entry(path: &str, validation: Option<ValidationResult>) -> RecoveredEntry {
        RecoveredEntry {
            source_id: "NTFS#7".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/out/x"),
            bytes_written: 50,
            priority_score: 30,
            is_deleted: false,
            sha256: Some("ff".repeat(32)),
            validation,
        }
    }

    #[test]
    fn csv_has_all_13_fields_in_header() {
        // 13 列ヘッダーが先頭行に並ぶこと。
        let csv = render_csv(&build_report(vec![])).unwrap();
        let first_line = csv.lines().next().unwrap();
        let cols: Vec<_> = first_line.split(',').collect();
        assert_eq!(cols.len(), 13, "ヘッダーは 13 列: {:?}", cols);
        for col in CSV_HEADER {
            assert!(first_line.contains(col), "{} が含まれるべき", col);
        }
        // 専用カラム: internal_note と customer_message が独立して存在
        assert!(first_line.contains("internal_note"));
        assert!(first_line.contains("customer_message"));
    }

    #[test]
    fn csv_handles_commas_and_quotes_in_paths() {
        // パスにカンマやダブルクオートを含むケース。csv crate が自動でエスケープすること。
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let report = build_report(vec![entry(
            "\\dir,with,comma\\and\"quote.png",
            Some(validation),
        )]);
        let csv = render_csv(&report).unwrap();
        // クオートで囲まれた中にカンマが含まれていれば正しくエスケープされている
        assert!(
            csv.contains("\"\\dir,with,comma\\and\"\"quote.png\""),
            "csv crate がカンマとクオートを正しくエスケープすること: {}",
            csv
        );
    }

    #[test]
    fn csv_writes_internal_note_in_dedicated_column() {
        // internal_note が独立カラムに書かれ、customer_message と分離されていること。
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "顧客向け文言",
            "内部メモ:再復旧推奨",
        );
        let report = build_report(vec![entry("\\bad.png", Some(validation))]);
        let csv = render_csv(&report).unwrap();
        assert!(csv.contains("顧客向け文言"));
        assert!(csv.contains("内部メモ:再復旧推奨"));
        // 2 行目（データ行）に 13 列存在
        let mut rdr = csv::Reader::from_reader(csv.as_bytes());
        let record = rdr.records().next().expect("少なくとも 1 レコード").unwrap();
        assert_eq!(record.len(), 13);
        // customer_message 列 (index 10) と internal_note 列 (index 11) が分離
        assert_eq!(record.get(10), Some("顧客向け文言"));
        assert_eq!(record.get(11), Some("内部メモ:再復旧推奨"));
    }
}
