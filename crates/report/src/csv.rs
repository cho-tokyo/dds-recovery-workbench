//! Chunk 20 / 20.5 / 23.7: 外部システム連携用 CSV レポート生成。
//!
//! 15 列（Chunk 23.7 で `is_priority` 列を追加）の固定スキーマで全フィールドを出力する。
//! Excel 等での詳細分析や案件管理 DB への取り込みを想定。
//! Phase 1 は BOM なし UTF-8（シンプル優先）。
//!
//! 列順序:
//! `source_id, original_path, output_path, bytes_written, is_deleted,
//! is_priority, priority_score, matched_wishes, sha256, validation_status,
//! format_detected, validator_name, customer_message, internal_note, diagnostics`
//!
//! 関連 FR: FR-REP-03 (外部システム連携用 CSV), FR-REP-04 (優先データ強調)。

use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;

/// CSV ヘッダー（15 列、Chunk 23.7 で `is_priority` 列を追加）。
const CSV_HEADER: &[&str; 15] = &[
    "source_id",
    "original_path",
    "output_path",
    "bytes_written",
    "is_deleted",
    "is_priority",
    "priority_score",
    "matched_wishes",
    "sha256",
    "validation_status",
    "format_detected",
    "validator_name",
    "customer_message",
    "internal_note",
    "diagnostics",
];

/// CSV レポートを生成する（全 14 フィールド含む）。
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
                    match &v.status {
                        ValidationStatus::Valid => "valid",
                        ValidationStatus::Invalid => "invalid",
                        ValidationStatus::Uncertain(_) => "uncertain",
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

        // Chunk 20.5: matched_wish_labels を "; " 区切りで一つの CSV セルに連結。
        let matched_wishes = entry.matched_wish_labels.join("; ");

        wtr.write_record([
            entry.source_id.as_str(),
            entry.original_path.as_str(),
            &entry.output_path.display().to_string(),
            &entry.bytes_written.to_string(),
            &entry.is_deleted.to_string(),
            &entry.is_priority.to_string(),
            &entry.priority_score.to_string(),
            matched_wishes.as_str(),
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
            wish_labels: vec![],
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
            matched_wish_labels: Vec::new(),
            is_priority: false,
        }
    }

    #[test]
    fn csv_has_all_15_fields_in_header() {
        // 15 列ヘッダー（Chunk 23.7 で is_priority 列を追加）。
        let csv = render_csv(&build_report(vec![])).unwrap();
        let first_line = csv.lines().next().unwrap();
        let cols: Vec<_> = first_line.split(',').collect();
        assert_eq!(cols.len(), 15, "ヘッダーは 15 列: {:?}", cols);
        for col in CSV_HEADER {
            assert!(first_line.contains(col), "{} が含まれるべき", col);
        }
        // 専用カラム
        assert!(first_line.contains("is_priority"));
        assert!(first_line.contains("matched_wishes"));
        assert!(first_line.contains("internal_note"));
        assert!(first_line.contains("customer_message"));
    }

    #[test]
    fn csv_handles_commas_and_quotes_in_paths() {
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let report = build_report(vec![entry(
            "\\dir,with,comma\\and\"quote.png",
            Some(validation),
        )]);
        let csv = render_csv(&report).unwrap();
        assert!(
            csv.contains("\"\\dir,with,comma\\and\"\"quote.png\""),
            "csv crate がカンマとクオートを正しくエスケープすること: {}",
            csv
        );
    }

    #[test]
    fn csv_writes_internal_note_in_dedicated_column() {
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
        let mut rdr = csv::Reader::from_reader(csv.as_bytes());
        let record = rdr
            .records()
            .next()
            .expect("少なくとも 1 レコード")
            .unwrap();
        assert_eq!(record.len(), 15);
        // Chunk 23.7 で is_priority 列が index 5 に挿入されたため、各 index が +1 シフト。
        // matched_wishes (index 7), sha256 (index 8), customer_message (index 12), internal_note (index 13)
        assert_eq!(record.get(5), Some("false")); // is_priority
        assert_eq!(record.get(7), Some("")); // matched_wishes
        assert_eq!(record.get(12), Some("顧客向け文言"));
        assert_eq!(record.get(13), Some("内部メモ:再復旧推奨"));
    }

    #[test]
    fn csv_emits_matched_wishes_column() {
        // Chunk 20.5 追加: matched_wish_labels が "; " 区切りで matched_wishes 列に出ること。
        // Chunk 23.7 で is_priority 列追加に伴い matched_wishes は index 7 に移動。
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let mut e = entry("\\img\\a.png", Some(validation));
        e.matched_wish_labels = vec!["写真".into(), "重要書類".into()];
        e.is_priority = true;
        let csv = render_csv(&build_report(vec![e])).unwrap();
        let mut rdr = csv::Reader::from_reader(csv.as_bytes());
        let record = rdr.records().next().unwrap().unwrap();
        assert_eq!(record.get(5), Some("true")); // is_priority
        assert_eq!(record.get(7), Some("写真; 重要書類"));
    }

    #[test]
    fn csv_is_priority_column_default_false() {
        // Chunk 23.7: 通常エントリ（is_priority=false）が "false" として出力されること。
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let e = entry("\\img\\a.png", Some(validation));
        let csv = render_csv(&build_report(vec![e])).unwrap();
        let mut rdr = csv::Reader::from_reader(csv.as_bytes());
        let record = rdr.records().next().unwrap().unwrap();
        assert_eq!(record.get(5), Some("false"));
    }
}
