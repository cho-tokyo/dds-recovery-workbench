//! Chunk 22: 統計結果から症状を自動判定する。
//!
//! 業務シナリオ:
//! - 「健康な HDD（誤発注）」    → `None`
//! - 「ゴミ箱からの削除」        → `Deleted`
//! - 「クイックフォーマット直後」 → `Formatted`
//! - 「MFT 部分損傷」            → `FilesystemError`
//! - 「フォーマット後に追加削除」 → `Mixed`
//!
//! 優先順位:
//! 1. FS 異常 → `FilesystemError`
//! 2. ファイル数が極端に少ない → `Formatted`
//! 3. 削除エントリあり → `Deleted`
//! 4. いずれも該当しなければ `None`
//!
//! 複数該当する場合は `Symptom::Mixed { symptoms }` で全部包含する。
//!
//! 関連 FR: FR-DIAG-02 (症状自動判定)。

use dds_case_manager::Symptom;

use crate::aggregator::{FORMATTED_DIR_THRESHOLD, FORMATTED_FILE_THRESHOLD};
use crate::report::{FileStatistics, FsAnomalyReport};

/// 統計結果から症状を判定する。
///
/// `has_deleted` には [`crate::aggregator::AggregateResult::deleted_file_stats`] の
/// `is_some()` を渡す（削除統計が存在するなら 1 件以上削除エントリあり）。
pub fn detect_symptom(
    file_stats: &FileStatistics,
    anomalies: &FsAnomalyReport,
    has_deleted: bool,
) -> Symptom {
    let mut symptoms = Vec::new();

    if anomalies.has_any_anomaly() {
        symptoms.push(Symptom::FilesystemError {
            anomalies: anomalies.to_anomaly_list(),
        });
    }

    // フォーマット痕跡ヒューリスティック（Phase 1 簡易版）:
    // クイックフォーマット直後は MFT がほぼ初期化されており、ファイル・ディレクトリ
    // 数が極端に小さい。Phase 2 で MFT カービング実装時により正確な判定に差し替え予定。
    if file_stats.total_files < FORMATTED_FILE_THRESHOLD
        && file_stats.directories < FORMATTED_DIR_THRESHOLD
    {
        symptoms.push(Symptom::Formatted {
            current_mft_entries: file_stats.total_files,
            old_mft_recoverability_hint: None,
        });
    }

    if has_deleted {
        symptoms.push(Symptom::Deleted);
    }

    match symptoms.len() {
        0 => Symptom::None,
        1 => symptoms.into_iter().next().expect("len checked"),
        _ => Symptom::Mixed { symptoms },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds_case_manager::FsAnomaly;

    fn many_files() -> FileStatistics {
        FileStatistics {
            total_files: 500,
            live_files: 500,
            deleted_files: 0,
            directories: 50,
            total_size_bytes: 100_000,
        }
    }

    #[test]
    fn detect_none_when_clean() {
        let s = detect_symptom(&many_files(), &FsAnomalyReport::default(), false);
        assert_eq!(s, Symptom::None);
    }

    #[test]
    fn detect_deleted_when_deleted_files_present() {
        let s = detect_symptom(&many_files(), &FsAnomalyReport::default(), true);
        assert_eq!(s, Symptom::Deleted);
    }

    #[test]
    fn detect_filesystem_error_when_anomalies() {
        let anomalies = FsAnomalyReport {
            mft_corrupted_count: 3,
            ..Default::default()
        };
        let s = detect_symptom(&many_files(), &anomalies, false);
        match s {
            Symptom::FilesystemError { anomalies } => {
                assert_eq!(anomalies.len(), 1);
                assert!(matches!(
                    anomalies[0],
                    FsAnomaly::MftEntryCorrupted { count: 3 }
                ));
            }
            other => panic!("expected FilesystemError, got {:?}", other),
        }
    }

    #[test]
    fn detect_formatted_when_very_few_files() {
        let fs = FileStatistics {
            total_files: 20,
            live_files: 20,
            deleted_files: 0,
            directories: 2,
            total_size_bytes: 4096,
        };
        let s = detect_symptom(&fs, &FsAnomalyReport::default(), false);
        match s {
            Symptom::Formatted {
                current_mft_entries: 20,
                ..
            } => {}
            other => panic!("expected Formatted, got {:?}", other),
        }
    }

    #[test]
    fn detect_mixed_when_multiple_conditions() {
        let fs = FileStatistics {
            total_files: 20,
            live_files: 18,
            deleted_files: 2,
            directories: 2,
            total_size_bytes: 4096,
        };
        let anomalies = FsAnomalyReport {
            mft_corrupted_count: 1,
            ..Default::default()
        };
        let s = detect_symptom(&fs, &anomalies, true);
        match s {
            Symptom::Mixed { symptoms } => {
                assert!(symptoms.len() >= 2);
                // 主症状ラベルは FilesystemError が優先される
                assert_eq!(
                    Symptom::Mixed { symptoms }.primary_label(),
                    "ファイルシステム異常 (複合)"
                );
            }
            other => panic!("expected Mixed, got {:?}", other),
        }
    }

    #[test]
    fn detect_does_not_emit_formatted_when_many_files_even_with_deletion() {
        // ファイル数が閾値以上なら Formatted は出ない（誤検知防止）。
        let fs = FileStatistics {
            total_files: 300,
            live_files: 295,
            deleted_files: 5,
            directories: 20,
            total_size_bytes: 1_000_000,
        };
        let s = detect_symptom(&fs, &FsAnomalyReport::default(), true);
        assert_eq!(s, Symptom::Deleted);
    }
}
