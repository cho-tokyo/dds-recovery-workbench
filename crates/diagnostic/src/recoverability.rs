//! Chunk 22.5: 削除ファイルの復旧可能性推定。
//!
//! NTFS の技術的事実（resident attribute、run-list の完全性、クラスタの上書き状態）
//! に基づき、各削除エントリを **High / Medium / Low** に分類する。
//!
//! ## 判定基準
//!
//! - **High（確実復旧可能）**:
//!   - resident attribute（MFT 内データ完結、小さいファイル）
//!   - OR run-list 完全 + 全クラスタ未上書き
//!   - OR 0 バイトファイル（クラスタ占有なし、ファイル存在情報あり）
//! - **Medium（部分復旧の可能性）**:
//!   - non-resident + run-list 完全 + 部分上書き（1 クラスタ以上残存）
//! - **Low（メタデータのみ）**:
//!   - run-list 破損
//!   - OR 全クラスタ上書き
//!
//! ## ヒューリスティック禁止
//!
//! ファイル名や拡張子に依存した推定は行わない。NTFS の生データから確定的に
//! 導出できる事実のみで判定する。
//!
//! 関連 FR: FR-DIAG-07（削除ファイル復旧可能性推定）、FR-DIAG-08（業務見積もりへの活用）。

use dds_case_manager::RecoverabilityEstimate;

use crate::aggregator::{ClusterOccupancyMap, DeletedFileMetadata};

/// 削除ファイル群の復旧可能性を一括推定する。
///
/// 入力の `deleted_files` 各エントリを High / Medium / Low に分類し、
/// 件数を集計した [`RecoverabilityEstimate`] を返す。
///
/// 入力が空の場合は全カウント 0 の estimate を返す。
///
/// 関連 FR: FR-DIAG-07。
pub fn estimate(
    deleted_files: &[DeletedFileMetadata],
    occupancy: &ClusterOccupancyMap,
) -> RecoverabilityEstimate {
    let mut high = 0usize;
    let mut medium = 0usize;
    let mut low = 0usize;

    for file in deleted_files {
        match categorize(file, occupancy) {
            Category::High => high += 1,
            Category::Medium => medium += 1,
            Category::Low => low += 1,
        }
    }

    RecoverabilityEstimate {
        high_confidence: high,
        medium_confidence: medium,
        low_confidence: low,
    }
}

/// 単一削除エントリの復旧可能性分類。内部用列挙型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    /// 確実復旧可能。
    High,
    /// 部分復旧の可能性。
    Medium,
    /// メタデータのみ。
    Low,
}

/// 単一の削除ファイルメタデータを判定する。仕様書の判定基準（Chunk 22.5）通り。
fn categorize(file: &DeletedFileMetadata, occupancy: &ClusterOccupancyMap) -> Category {
    // resident: MFT 内データ完結、ほぼ確実に復旧可能。
    if file.is_resident {
        return Category::High;
    }

    // run-list 破損: メタデータのみ。
    if !file.run_list_valid {
        return Category::Low;
    }

    // run-list 完全 + クラスタ占有状態をチェック。
    let total_clusters: u64 = file.cluster_ranges.iter().map(|r| r.length).sum();

    if total_clusters == 0 {
        // 占有クラスタなし（0 バイトファイル等）。ファイル存在情報が確実に得られるので High 扱い。
        return Category::High;
    }

    let overwritten: u64 = file
        .cluster_ranges
        .iter()
        .map(|r| occupancy.count_overlapping(r.start_lcn, r.length))
        .sum();

    if overwritten == 0 {
        Category::High
    } else if overwritten >= total_clusters {
        Category::Low
    } else {
        Category::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds_fs_ntfs::ClusterRange;

    /// テスト用ヘルパ: フィールドから [`DeletedFileMetadata`] を組み立てる。
    fn make_metadata(
        is_resident: bool,
        run_list_valid: bool,
        ranges: Vec<(u64, u64)>,
    ) -> DeletedFileMetadata {
        DeletedFileMetadata {
            record_index: 0,
            is_resident,
            run_list_valid,
            cluster_ranges: ranges
                .into_iter()
                .map(|(start, length)| ClusterRange {
                    start_lcn: start,
                    length,
                })
                .collect(),
        }
    }

    #[test]
    fn resident_file_is_high() {
        let files = vec![make_metadata(true, true, vec![])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
        assert_eq!(est.medium_confidence, 0);
        assert_eq!(est.low_confidence, 0);
    }

    #[test]
    fn invalid_runlist_is_low() {
        // resident=false + run_list_valid=false → Low
        let files = vec![make_metadata(false, false, vec![])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.low_confidence, 1);
    }

    #[test]
    fn non_overwritten_clusters_is_high() {
        // non-resident + 全クラスタ未上書き → High
        let files = vec![make_metadata(false, true, vec![(100, 5)])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
    }

    #[test]
    fn fully_overwritten_clusters_is_low() {
        // non-resident + 全クラスタ上書き済み → Low
        let files = vec![make_metadata(false, true, vec![(100, 5)])];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(100, 5);
        let est = estimate(&files, &occupancy);
        assert_eq!(est.low_confidence, 1);
    }

    #[test]
    fn partially_overwritten_clusters_is_medium() {
        // non-resident + 部分上書き（10 のうち 3 が上書き）→ Medium
        let files = vec![make_metadata(false, true, vec![(100, 10)])];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(105, 3);
        let est = estimate(&files, &occupancy);
        assert_eq!(est.medium_confidence, 1);
        assert_eq!(est.high_confidence, 0);
        assert_eq!(est.low_confidence, 0);
    }

    #[test]
    fn zero_byte_file_is_high() {
        // non-resident + run-list 完全 + クラスタ占有なし（0 バイト）→ High
        let files = vec![make_metadata(false, true, vec![])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
    }

    #[test]
    fn mixed_categories_counted_separately() {
        let files = vec![
            make_metadata(true, true, vec![]),          // High (resident)
            make_metadata(false, false, vec![]),        // Low (run-list 破損)
            make_metadata(false, true, vec![(200, 5)]), // High (未上書き)
            make_metadata(false, true, vec![(300, 5)]), // Medium (部分上書き)
        ];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(302, 2); // 4 番目のファイルの一部を占有

        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 2);
        assert_eq!(est.medium_confidence, 1);
        assert_eq!(est.low_confidence, 1);
    }

    #[test]
    fn empty_input_yields_zero_estimate() {
        let files: Vec<DeletedFileMetadata> = vec![];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 0);
        assert_eq!(est.medium_confidence, 0);
        assert_eq!(est.low_confidence, 0);
    }
}
