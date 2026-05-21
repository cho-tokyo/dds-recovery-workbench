//! フルパス再構築モジュール（Chunk 13）。
//!
//! 各 MFT エントリの `$FILE_NAME` 属性に含まれる親ディレクトリ参照を辿り、NTFS ルート (`\`) からの
//! 絶対パス文字列を構築する。ディレクトリパス（中間ノード）を `HashMap` でキャッシュし、N ファイル全
//! パス解決の計算量を実用上 O(N) に近づける。書籍『File System Forensic Analysis』Ch.12「LINKS TO
//! FILES AND DIRECTORIES」の再帰アルゴリズム準拠。関連 FR: FR-LIVE-04, FR-LIVE-05, FR-LIVE-06。
use crate::attributes::file_name::find_best_file_name;
use crate::volume::{NtfsVolume, VolumeError};
use std::collections::HashMap;

/// NTFS のルートディレクトリ MFT エントリ番号（固定値）。書籍 Ch.13 system files。
pub const NTFS_ROOT_RECORD: u64 = 5;
/// パス再構築の最大深さ。破損データ / 循環参照防護（NTFS 実用上 32 階層程度が上限）。
pub const MAX_PATH_DEPTH: u32 = 64;
const PATH_SEPARATOR: &str = "\\";

/// MFT エントリ番号 → フルパス文字列のキャッシュ付き解決器。
///
/// 大量ファイルの全パス解決時に中間ディレクトリパスを再利用するため、`HashMap` で
/// キャッシュする。`new()` でルート（entry 5 → `"\\"`）を投入済み。
#[derive(Debug)]
pub struct PathResolver {
    /// MFT エントリ番号 → 解決済みフルパス。`clear()` で消去（ルートは復元）。
    cache: HashMap<u64, String>,
}

impl PathResolver {
    /// 新規 `PathResolver` を生成。NTFS ルート（entry 5）→ `"\\"` をキャッシュ初期投入。
    pub fn new() -> Self {
        let mut cache = HashMap::new();
        cache.insert(NTFS_ROOT_RECORD, PATH_SEPARATOR.to_string());
        Self { cache }
    }

    /// 指定 MFT エントリのフルパスを解決する。同一 record を 2 回目以降に呼ぶと
    /// キャッシュ即返却。中間ディレクトリも自動キャッシュ。
    pub fn resolve<F>(
        &mut self,
        record_index: u64,
        volume: &mut NtfsVolume<F>,
    ) -> Result<String, VolumeError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        self.resolve_inner(record_index, volume, 0)
    }

    /// 再帰本体。`depth` は循環防護用、`MAX_PATH_DEPTH` 超過で `PathDepthExceeded`。
    fn resolve_inner<F>(
        &mut self,
        record_index: u64,
        volume: &mut NtfsVolume<F>,
        depth: u32,
    ) -> Result<String, VolumeError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        if let Some(cached) = self.cache.get(&record_index) {
            return Ok(cached.clone());
        }
        if depth > MAX_PATH_DEPTH {
            return Err(VolumeError::PathDepthExceeded {
                record_index,
                depth,
            });
        }
        let entry = volume.read_record(record_index)?;
        let file_name =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
                .ok_or(VolumeError::NoFileName { record_index })?;
        let parent_index = file_name.parent_directory.entry_number;
        // 自己参照（自分の親が自分）も `depth` チェックで止まるが、明示的に弾く
        if parent_index == record_index {
            return Err(VolumeError::PathDepthExceeded {
                record_index,
                depth,
            });
        }
        let parent_path = self.resolve_inner(parent_index, volume, depth + 1)?;
        let my_path = if parent_path == PATH_SEPARATOR {
            format!("{}{}", PATH_SEPARATOR, file_name.filename)
        } else {
            format!("{}{}{}", parent_path, PATH_SEPARATOR, file_name.filename)
        };
        self.cache.insert(record_index, my_path.clone());
        Ok(my_path)
    }

    /// キャッシュをクリアし、ルート（entry 5 → `"\\"`）のみ復元する。
    /// ボリュームを再オープンした際は本メソッドを呼ぶか、新しい `PathResolver` を生成すること。
    pub fn clear(&mut self) {
        self.cache.clear();
        self.cache
            .insert(NTFS_ROOT_RECORD, PATH_SEPARATOR.to_string());
    }

    /// 現在のキャッシュエントリ数（テスト・診断用）。
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// キャッシュに記録済みのフルパス参照を返す（解決処理を行わない、ピーク用）。
    pub fn cached(&self, record_index: u64) -> Option<&str> {
        self.cache.get(&record_index).map(String::as_str)
    }
}

impl Default for PathResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_resolver_root_returns_backslash() {
        let resolver = PathResolver::new();
        assert_eq!(resolver.cached(NTFS_ROOT_RECORD), Some("\\"));
        assert_eq!(resolver.cache_size(), 1);
    }

    #[test]
    fn path_resolver_default_matches_new() {
        let a = PathResolver::new();
        let b = PathResolver::default();
        assert_eq!(a.cache_size(), b.cache_size());
        assert_eq!(a.cached(NTFS_ROOT_RECORD), b.cached(NTFS_ROOT_RECORD));
    }

    #[test]
    fn path_resolver_clear_removes_cache_but_restores_root() {
        let mut resolver = PathResolver::new();
        // 手動で他レコードを差し込み（実際は resolve 経由でしか入らないが、API 検証目的）
        resolver.cache.insert(100, "\\foo\\bar".to_string());
        resolver.cache.insert(200, "\\baz".to_string());
        assert_eq!(resolver.cache_size(), 3);
        resolver.clear();
        assert_eq!(resolver.cache_size(), 1);
        assert_eq!(resolver.cached(NTFS_ROOT_RECORD), Some("\\"));
        assert_eq!(resolver.cached(100), None);
        assert_eq!(resolver.cached(200), None);
    }

    #[test]
    fn path_resolver_cached_returns_none_for_unknown_record() {
        let resolver = PathResolver::new();
        assert_eq!(resolver.cached(42), None);
        assert_eq!(resolver.cached(999_999), None);
    }

    #[test]
    fn max_path_depth_is_within_documented_bound() {
        // 仕様: 64 階層で打ち切り（NTFS 実用上の安全マージン）
        assert_eq!(MAX_PATH_DEPTH, 64);
        assert_eq!(NTFS_ROOT_RECORD, 5);
    }
}
