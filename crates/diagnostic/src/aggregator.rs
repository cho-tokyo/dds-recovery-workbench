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

use std::collections::{BTreeMap, HashMap};

use dds_case_manager::DeletedFileStats;
use dds_fs_ntfs::NtfsVolume;

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
}
