//! 完了報告用: case.json のサンプル出力を生成して標準出力に表示する。
//!
//! 実行: `cargo run -p dds-case-manager --example dump_case_json`

use std::path::PathBuf;

use chrono::Utc;
use dds_case_manager::{CaseId, CaseStorage, RecoveryReportSummary, Symptom};
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};
use tempfile::TempDir;

fn main() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case_id = CaseId::parse("260522-04").unwrap();

    let mut case = storage.create_new(case_id.clone()).unwrap();
    case.diagnostic_input.diagnosed_at = Some(Utc::now());
    case.diagnostic_input.duration_secs = Some(42);
    case.diagnostic_input.filesystem_type = Some("NTFS".into());
    case.diagnostic_input.symptom = Some(Symptom::Deleted);
    case.diagnostic_input.total_files = 12847;
    case.diagnostic_input.deleted_files = 234;
    case.diagnostic_input.total_size_bytes = 100_000_000_000;
    case.diagnostic_input.notes = "Shift+Delete による削除と推定".into();
    case.wishlist = Some(
        Wishlist::new().add(
            Wish::new(WishItem::Extension("docx".into()), "Word ファイル全部")
                .with_priority(Priority::High),
        ),
    );
    let now = Utc::now();
    case.recovery_report_summary = Some(RecoveryReportSummary {
        started_at: now,
        finished_at: now,
        duration_ms: 8_500,
        total_matched: 230,
        recovered_count: 225,
        failed_count: 3,
        skipped_count: 2,
        validated_count: 220,
        invalid_count: 4,
        uncertain_count: 1,
        total_bytes_written: 850_000_000,
        recovery_success_rate: 0.978,
        quality_assurance_rate: 0.978,
    });
    case.output_dir = Some(PathBuf::from("G:\\260522-04"));
    storage.save(&case).unwrap();

    let path = storage.case_file_path(&case_id);
    let json = std::fs::read_to_string(&path).unwrap();
    println!("=== case.json sample ({}) ===", path.display());
    println!("{}", json);
}
