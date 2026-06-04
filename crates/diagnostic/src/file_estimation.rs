//! Chunk 24d-4-1: MFT ベースのファイル数推定。
//!
//! aggregator が既に 1-pass で `$MFT` を走査して `FileStatistics` を構築するため、
//! 本モジュールはその統計を業務指標型 [`FileEstimation`] へ変換する thin wrapper。
//! 重複走査を避けるための設計判断 (方針 B)。
//!
//! 関連 FR: FR-DIAG-06 (ファイル数推定)。

use dds_case_manager::FileEstimation;

use crate::report::FileStatistics;

/// aggregator の `FileStatistics` から業務指標 [`FileEstimation`] に変換する。
///
/// 既存集計結果 (生存 / 削除 / 総) をそのままビジネス型に詰め替える。
///
/// 関連 FR: FR-DIAG-06。
pub fn estimate_from_file_stats(stats: &FileStatistics) -> FileEstimation {
    FileEstimation {
        estimated_total_files: stats.total_files as u64,
        estimated_deleted_files: stats.deleted_files as u64,
        estimated_live_files: stats.live_files as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds_case_manager::format_estimation_number;

    #[test]
    fn estimate_from_file_stats_copies_counts() {
        let stats = FileStatistics {
            total_files: 1500,
            live_files: 1450,
            deleted_files: 50,
            directories: 30,
            total_size_bytes: 100_000,
        };
        let est = estimate_from_file_stats(&stats);
        assert_eq!(est.estimated_total_files, 1500);
        assert_eq!(est.estimated_live_files, 1450);
        assert_eq!(est.estimated_deleted_files, 50);
    }

    #[test]
    fn business_summary_format() {
        let est = FileEstimation {
            estimated_total_files: 1500,
            estimated_deleted_files: 50,
            estimated_live_files: 1450,
        };
        let summary = est.business_summary();
        assert!(summary.contains("1,500"), "summary: {}", summary);
        assert!(summary.contains("1,450"), "summary: {}", summary);
        assert!(summary.contains("50"), "summary: {}", summary);
    }

    #[test]
    fn format_number_thousands() {
        // 業務向け短縮表記の境界値テスト (case-manager 側関数の再エクスポート確認)。
        assert_eq!(format_estimation_number(500), "500");
        assert_eq!(format_estimation_number(1500), "1,500");
        assert_eq!(format_estimation_number(25_000), "2.5万");
    }

    #[test]
    fn estimate_from_empty_stats_is_zero() {
        let stats = FileStatistics::default();
        let est = estimate_from_file_stats(&stats);
        assert_eq!(est.estimated_total_files, 0);
        assert_eq!(est.estimated_live_files, 0);
        assert_eq!(est.estimated_deleted_files, 0);
    }
}
