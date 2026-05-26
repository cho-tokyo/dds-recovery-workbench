//! Chunk 22: 単一パスで全統計を集計する核心ロジック。
//!
//! `NtfsVolume::iter_files()` を **1 回だけ** 呼び、ファイル統計・形式別ブレイクダウン・
//! フォルダ別ブレイクダウン・削除ファイル統計・FS 異常を並行集計する。
//!
//! 業務的に MFT スキャンは I/O が重い（健康な 2TB HDD で 30-60 秒見込み）。
//! 集計ロジックを分割して複数回走査するのは厳禁。
//!
//! 関連 FR: FR-DIAG-01 (NTFS 論理診断), FR-DIAG-03 (削除ファイル統計),
//!         FR-DIAG-05 (1 分以内)。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dds_case_manager::DeletedFileStats;
use dds_fs_ntfs::{ClusterRange, NtfsVolume};

use crate::error::DiagnosticError;
use crate::report::{FileStatistics, FolderCount, FormatCount, FsAnomalyReport};

/// 単一パス走査結果の集約構造体。
pub struct AggregateResult {
    /// 全ファイル統計。
    pub file_stats: FileStatistics,
    /// 拡張子別ブレイクダウン（小文字キーで昇順ソート）。
    pub format_breakdown: BTreeMap<String, FormatCount>,
    /// 件数降順 上位 10 フォルダ。
    pub folder_breakdown: Vec<FolderCount>,
    /// 削除ファイル統計（削除 0 件時は `None`）。
    pub deleted_file_stats: Option<DeletedFileStats>,
    /// 集計中に検出した FS 異常レポート。
    pub anomalies: FsAnomalyReport,
    /// 生存ファイル群のクラスタ占有マップ（Chunk 22.5、復旧可能性推定用）。
    pub cluster_occupancy: ClusterOccupancyMap,
    /// 削除ファイル群の判定用メタデータ（Chunk 22.5、復旧可能性推定用）。
    pub deleted_file_metadata: Vec<DeletedFileMetadata>,
}

/// 生存ファイル群のクラスタ占有マップ。
///
/// 削除ファイルが占有していたクラスタが、現在生存ファイルに割り当てられているかを
/// 判定するために使う。Phase 1.5 では `BTreeSet<u64>` で実装（小規模フィクスチャ
/// ~1300 クラスタで実用範囲）。Phase 2 で大規模 HDD 対応する際は Roaring Bitmap や
/// ビットマップ実装を検討。
/// 関連 FR: FR-DIAG-07（削除ファイル復旧可能性推定）。
#[derive(Debug, Default, Clone)]
pub struct ClusterOccupancyMap {
    /// 占有されているクラスタの集合。
    occupied: BTreeSet<u64>,
}

impl ClusterOccupancyMap {
    /// 空の占有マップを生成する。
    pub fn new() -> Self {
        Self::default()
    }
    /// 指定範囲 `[start, start + length)` のクラスタ群を「占有済み」として登録する。
    pub fn mark_range(&mut self, start: u64, length: u64) {
        for lcn in start..start.saturating_add(length) {
            self.occupied.insert(lcn);
        }
    }
    /// 指定 LCN が占有されているかを返す。
    pub fn is_occupied(&self, lcn: u64) -> bool {
        self.occupied.contains(&lcn)
    }
    /// 範囲 `[start, start + length)` のうち占有されているクラスタ数を返す。
    pub fn count_overlapping(&self, start: u64, length: u64) -> u64 {
        (start..start.saturating_add(length))
            .filter(|lcn| self.occupied.contains(lcn))
            .count() as u64
    }
    /// 占有マップに登録されているクラスタ数の総数（業務観測用）。
    pub fn occupied_cluster_count(&self) -> usize {
        self.occupied.len()
    }
}

/// 削除ファイル 1 件分の復旧可能性判定用メタデータ。
///
/// Chunk 22.5 で導入。aggregator が単一パス走査中に各削除エントリから収集し、
/// `recoverability::estimate` がこの配列を受け取って High/Medium/Low に分類する。
/// 関連 FR: FR-DIAG-07（削除ファイル復旧可能性推定）。
#[derive(Debug, Clone)]
pub struct DeletedFileMetadata {
    /// MFT エントリ番号（このファイルの一意 ID）。
    pub record_index: u64,
    /// メイン `$DATA` 属性が resident かどうか。
    pub is_resident: bool,
    /// run-list が完全に解析できているか。`build_file` で成功した時点で `true`。
    pub run_list_valid: bool,
    /// 占有していたクラスタ範囲（sparse は除外、resident は空）。
    pub cluster_ranges: Vec<ClusterRange>,
}

/// 削除統計の上位フォルダ件数（業務的に「主要フォルダ 5 つ」が目安）。
const DELETED_TOP_FOLDERS: usize = 5;
/// フォルダブレイクダウンの上位件数。
const FOLDER_TOP_N: usize = 10;

/// volume の全 MFT エントリを 1 回だけ走査し、全統計を集計する。
///
/// `iter_files` の `Result` ストリーム中、エラーは内部の `classify_error` で分類して
/// `FsAnomalyReport` に加算、走査は継続（破損耐性）。
pub fn aggregate_all<F>(volume: &mut NtfsVolume<F>) -> Result<AggregateResult, DiagnosticError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let mut file_stats = FileStatistics::default();
    let mut format_breakdown: BTreeMap<String, FormatCount> = BTreeMap::new();
    let mut all_folders: HashMap<String, (usize, u64)> = HashMap::new();

    let mut deleted_by_ext: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleted_by_folder: HashMap<String, usize> = HashMap::new();
    let mut deleted_total_size: u64 = 0;
    let mut deleted_count: usize = 0;

    let mut anomalies = FsAnomalyReport::default();

    // Chunk 22.5: 復旧可能性推定用の中間データ。
    let mut cluster_occupancy = ClusterOccupancyMap::new();
    let mut deleted_metadata: Vec<DeletedFileMetadata> = Vec::new();

    for result in volume.iter_files() {
        match result {
            Ok(file) => {
                // ユーザファイル + 削除済みユーザディレクトリ含むが、システムメタファイルは除外。
                // is_user_file は「!directory && !system」なので、ディレクトリも含めて
                // 統計を取りたい場合は手動で system メタファイルだけ除外する。
                if file.is_system_metafile() {
                    continue;
                }

                file_stats.total_files += 1;
                file_stats.total_size_bytes = file_stats.total_size_bytes.saturating_add(file.size);
                if file.is_deleted {
                    file_stats.deleted_files += 1;
                } else {
                    file_stats.live_files += 1;
                }
                if file.is_directory {
                    file_stats.directories += 1;
                    continue;
                }

                // 形式別集計
                let ext = file
                    .extension()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(なし)".to_string());
                let entry = format_breakdown.entry(ext.clone()).or_default();
                entry.count += 1;
                entry.total_size_bytes = entry.total_size_bytes.saturating_add(file.size);

                // フォルダ別集計
                let folder = extract_folder(&file.path);
                let f_entry = all_folders.entry(folder.clone()).or_insert((0, 0));
                f_entry.0 += 1;
                f_entry.1 = f_entry.1.saturating_add(file.size);

                // 削除専用統計
                if file.is_deleted {
                    deleted_count += 1;
                    deleted_total_size = deleted_total_size.saturating_add(file.size);
                    if ext != "(なし)" {
                        *deleted_by_ext.entry(ext).or_insert(0) += 1;
                    }
                    *deleted_by_folder.entry(folder).or_insert(0) += 1;

                    // Chunk 22.5: 削除ファイルのメタデータを収集。run-list が解析できた
                    // 時点（iter_files の Ok 分岐）で run_list_valid = true とみなす。
                    deleted_metadata.push(DeletedFileMetadata {
                        record_index: file.record_index,
                        is_resident: file.is_resident(),
                        run_list_valid: true,
                        cluster_ranges: file.occupied_cluster_ranges(),
                    });
                } else {
                    // Chunk 22.5: 生存ファイルが占有しているクラスタを占有マップに登録。
                    for range in file.occupied_cluster_ranges() {
                        cluster_occupancy.mark_range(range.start_lcn, range.length);
                    }
                }
            }
            Err(e) => classify_error(&e, &mut anomalies),
        }
    }

    let mut folder_vec: Vec<FolderCount> = all_folders
        .into_iter()
        .map(|(path, (count, size))| FolderCount {
            path,
            file_count: count,
            total_size_bytes: size,
        })
        .collect();
    folder_vec.sort_by(|a, b| b.file_count.cmp(&a.file_count).then(a.path.cmp(&b.path)));
    folder_vec.truncate(FOLDER_TOP_N);

    let deleted_file_stats = if deleted_count > 0 {
        let mut df_vec: Vec<(String, usize)> = deleted_by_folder.into_iter().collect();
        df_vec.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        df_vec.truncate(DELETED_TOP_FOLDERS);

        Some(DeletedFileStats {
            total_count: deleted_count,
            by_extension: deleted_by_ext,
            by_folder: df_vec,
            estimated_total_size: deleted_total_size,
            recoverability_estimate: None,
        })
    } else {
        None
    };

    Ok(AggregateResult {
        file_stats,
        format_breakdown,
        folder_breakdown: folder_vec,
        deleted_file_stats,
        anomalies,
        cluster_occupancy,
        deleted_file_metadata: deleted_metadata,
    })
}

/// パスからフォルダ部分を抽出する。
///
/// 例:
/// - `"\Users\Chou\file.txt"` → `"\Users\Chou"`
/// - `"\file.txt"`             → `"\"`
/// - `"file.txt"`              → `"(root)"`
pub fn extract_folder(path: &str) -> String {
    match path.rfind('\\') {
        Some(0) => "\\".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => "(root)".to_string(),
    }
}

/// エラーメッセージを分類して [`FsAnomalyReport`] に集計する。
///
/// 「未使用 MFT スロット（magic ≠ "FILE" かつ ≠ "BAAD"、典型的にはオールゼロ）」
/// は NTFS 仕様上正常な状態であり、`InvalidMagic` で yield されるためここで除外する。
/// これを除外しないと健康な NTFS ボリュームでも常に多数の「MFT 破損」が立ち、
/// 業務的に誤検知となる。
///
/// `BAAD` シグネチャ（NTFS が破損を明示マーキング）は真の破損として `mft_corrupted_count`
/// に計上する。
///
/// `dds-fs-ntfs::VolumeError` のバリアントを直接 match できれば理想だが、
/// 拡張に追従しやすいよう文字列マッチで実装している（Phase 2 で構造化に検討）。
fn classify_error(e: &dds_fs_ntfs::VolumeError, anomalies: &mut FsAnomalyReport) {
    let msg = e.to_string();
    let lower = msg.to_lowercase();

    // 未使用 MFT スロットは健康なボリュームでも常に多数存在するためスキップ。
    if lower.contains("invalid mft entry magic") {
        return;
    }

    if lower.contains("runlist") || lower.contains("run-list") || lower.contains("data run") {
        anomalies.invalid_runlist_count += 1;
    } else if lower.contains("baad")
        || lower.contains("mft")
        || lower.contains("entry")
        || lower.contains("record")
    {
        anomalies.mft_corrupted_count += 1;
    } else {
        anomalies.other_issues.push(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_folder_handles_root_files() {
        // "\file.txt" → "\"
        assert_eq!(extract_folder("\\file.txt"), "\\");
    }

    #[test]
    fn extract_folder_handles_root_slash() {
        // 単独のルートスラッシュ → "\"
        assert_eq!(extract_folder("\\"), "\\");
    }

    #[test]
    fn extract_folder_handles_deep_path() {
        assert_eq!(
            extract_folder("\\Users\\Chou\\Docs\\note.txt"),
            "\\Users\\Chou\\Docs"
        );
    }

    #[test]
    fn extract_folder_handles_path_without_separator() {
        // バックスラッシュ無し → "(root)"
        assert_eq!(extract_folder("file.txt"), "(root)");
        assert_eq!(extract_folder(""), "(root)");
    }

    #[test]
    fn classify_error_categorizes_runlist_first() {
        // VolumeError::Runlist を生成しにくいので fake な io::Error 経由で
        // メッセージ「Runlist error: ...」が runlist 分岐に入ることを確認。
        let io = std::io::Error::other("runlist out of range");
        let ve = dds_fs_ntfs::VolumeError::from(io);
        let mut a = FsAnomalyReport::default();
        classify_error(&ve, &mut a);
        assert_eq!(a.invalid_runlist_count, 1);
        assert_eq!(a.mft_corrupted_count, 0);
    }

    #[test]
    fn classify_error_categorizes_mft_corruption() {
        let io = std::io::Error::other("mft record header bad");
        let ve = dds_fs_ntfs::VolumeError::from(io);
        let mut a = FsAnomalyReport::default();
        classify_error(&ve, &mut a);
        assert_eq!(a.mft_corrupted_count, 1);
    }

    #[test]
    fn classify_error_falls_back_to_other() {
        let io = std::io::Error::other("some bizarre failure");
        let ve = dds_fs_ntfs::VolumeError::from(io);
        let mut a = FsAnomalyReport::default();
        classify_error(&ve, &mut a);
        assert_eq!(a.mft_corrupted_count, 0);
        assert_eq!(a.invalid_runlist_count, 0);
        assert_eq!(a.other_issues.len(), 1);
        assert!(a.other_issues[0].contains("bizarre"));
    }

    // Chunk 22.5: ClusterOccupancyMap のテスト ------------------------------

    #[test]
    fn occupancy_mark_and_check_range() {
        let mut occ = ClusterOccupancyMap::new();
        occ.mark_range(100, 5);
        // 範囲内
        assert!(occ.is_occupied(100));
        assert!(occ.is_occupied(104));
        // 範囲外
        assert!(!occ.is_occupied(99));
        assert!(!occ.is_occupied(105));
        assert_eq!(occ.occupied_cluster_count(), 5);
    }

    #[test]
    fn occupancy_count_overlapping_partial() {
        let mut occ = ClusterOccupancyMap::new();
        occ.mark_range(105, 3); // [105, 108) を占有
                                // [100, 110) のうち占有されているのは 105, 106, 107 の 3 つ
        assert_eq!(occ.count_overlapping(100, 10), 3);
        // 重なりなし
        assert_eq!(occ.count_overlapping(200, 10), 0);
        // 完全包含
        assert_eq!(occ.count_overlapping(105, 3), 3);
    }

    #[test]
    fn occupancy_default_is_empty() {
        let occ = ClusterOccupancyMap::default();
        assert_eq!(occ.occupied_cluster_count(), 0);
        assert!(!occ.is_occupied(0));
        assert_eq!(occ.count_overlapping(0, 100), 0);
    }
}
