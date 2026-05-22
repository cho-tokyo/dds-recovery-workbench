//! # dds-fs-common
//!
//! 各 FS リーダー（NTFS / exFAT / FAT32）が実装する共通インタフェースと共通データ型。
//! 書き込み API は意図的に未定義（read-only 強制を型レベルで担保）。
//! 関連 FR: FR-LIVE-01〜FR-LIVE-07（ライブモードでの FS 列挙・メタデータ抽出基盤）。
#![warn(missing_docs, rust_2018_idioms)]
use dds_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// 取り扱う FS 種別。関連 FR: FR-LIVE-01（FS 自動判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FsType {
    /// Microsoft NTFS。
    Ntfs,
    /// Microsoft exFAT。
    ExFat,
    /// Microsoft FAT32。
    Fat32,
    /// 判定不能・未対応。
    Unknown,
}
impl FsType {
    /// UI・レポート用ラベル。Unknown のみ "不明" と和訳します。
    pub fn label_ja(&self) -> &'static str {
        match self {
            Self::Ntfs => "NTFS",
            Self::ExFat => "exFAT",
            Self::Fat32 => "FAT32",
            Self::Unknown => "不明",
        }
    }
}
impl fmt::Display for FsType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ntfs => "NTFS",
            Self::ExFat => "exFAT",
            Self::Fat32 => "FAT32",
            Self::Unknown => "Unknown",
        })
    }
}
impl FromStr for FsType {
    type Err = CoreError;
    /// 大文字小文字を区別せず構築。未知文字列は `InvalidArgument` を返します。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "ntfs" => Ok(Self::Ntfs),
            "exfat" => Ok(Self::ExFat),
            "fat32" => Ok(Self::Fat32),
            "unknown" | "不明" => Ok(Self::Unknown),
            o => Err(CoreError::InvalidArgument(format!("未知の FS 種別: {}", o))),
        }
    }
}
/// FS エントリ種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    /// 通常ファイル。
    File,
    /// ディレクトリ。
    Directory,
    /// シンボリックリンク／ジャンクション等。
    Symlink,
    /// 上記以外（デバイスファイル等）。
    Other,
}
impl EntryKind {
    /// ディレクトリかどうか。
    pub fn is_directory(&self) -> bool {
        matches!(self, Self::Directory)
    }
    /// 通常ファイルかどうか。
    pub fn is_regular_file(&self) -> bool {
        matches!(self, Self::File)
    }
}
/// FS エントリのタイムスタンプ群（Unix epoch ミリ秒、未取得時は `None`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsTimestamps {
    /// 作成時刻（Unix epoch ミリ秒）。
    pub created: Option<i64>,
    /// 最終更新時刻。
    pub modified: Option<i64>,
    /// 最終アクセス時刻。
    pub accessed: Option<i64>,
}
impl FsTimestamps {
    /// 全フィールド `None` の空タイムスタンプ。
    pub fn empty() -> Self {
        Self::default()
    }
}
/// FS 内の単一エントリ（ファイル／ディレクトリ）のメタデータ。
/// 関連 FR: FR-LIVE-02（エントリ列挙）, FR-LIVE-05（削除済みフラグ取得）, FR-LIVE-06（メタデータ抽出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    /// FS 固有のレコード ID（NTFS なら MFT エントリ番号等）。
    pub record_id: u64,
    /// 親ディレクトリのレコード ID。ルート・孤児の場合は `None`。
    pub parent_record_id: Option<u64>,
    /// 単体ファイル名（パスではない）。
    pub name: String,
    /// 親階層解決後のフルパス。未解決時は `None`。
    pub full_path: Option<String>,
    /// ファイルサイズ（バイト）。ディレクトリは `None`。
    pub size_bytes: Option<u64>,
    /// エントリ種別。
    pub kind: EntryKind,
    /// 削除済みフラグ（FR-LIVE-05）。
    pub is_deleted: bool,
    /// タイムスタンプ群。
    pub timestamps: FsTimestamps,
    /// 抽出元 FS 種別。
    pub fs_type: FsType,
}
impl FsEntry {
    /// 削除済みかどうかを返します（公開フィールドの意図明示用 getter）。
    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }
    /// ディレクトリかどうか（`kind.is_directory()` 委譲）。
    pub fn is_directory(&self) -> bool {
        self.kind.is_directory()
    }
}
/// FS リーダーの read-only 共通インタフェース。書き込み系メソッドは **意図的に未定義**。
/// 関連 FR: FR-LIVE-01（FS 判定）, FR-LIVE-02（エントリ列挙）, FR-LIVE-03（ルート取得）,
/// FR-LIVE-04（個別取得）, FR-LIVE-05（削除済み取得）, FR-LIVE-06（メタデータ抽出）。
pub trait FsReader {
    /// このリーダが解釈する FS 種別。
    fn fs_type(&self) -> FsType;
    /// ルートディレクトリのレコード ID（NTFS なら 5 など）。
    fn root_record_id(&self) -> CoreResult<u64>;
    /// 指定レコード ID からエントリを 1 件取得。
    fn read_entry(&mut self, record_id: u64) -> CoreResult<FsEntry>;
    /// 全エントリ（削除済み含む）列挙。後続チャンクで Iterator 化を検討。
    fn list_all_entries(&mut self) -> CoreResult<Vec<FsEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fs_type_display_outputs_correct_labels() {
        assert_eq!(format!("{}", FsType::Ntfs), "NTFS");
        assert_eq!(format!("{}", FsType::ExFat), "exFAT");
        assert_eq!(format!("{}", FsType::Fat32), "FAT32");
        assert_eq!(format!("{}", FsType::Unknown), "Unknown");
        assert_eq!(FsType::Unknown.label_ja(), "不明");
    }
    #[test]
    fn fs_type_from_str_accepts_case_insensitive() {
        assert_eq!(FsType::from_str("ntfs").unwrap(), FsType::Ntfs);
        assert_eq!(FsType::from_str("NTFS").unwrap(), FsType::Ntfs);
        assert_eq!(FsType::from_str("Ntfs").unwrap(), FsType::Ntfs);
        assert_eq!(FsType::from_str("exfat").unwrap(), FsType::ExFat);
        assert_eq!(FsType::from_str("FAT32").unwrap(), FsType::Fat32);
        let err = FsType::from_str("zfs").unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidArgument(_)),
            "actual: {:?}",
            err
        );
    }
    #[test]
    fn entry_kind_helpers() {
        for (k, dir, file) in [
            (EntryKind::Directory, true, false),
            (EntryKind::File, false, true),
            (EntryKind::Symlink, false, false),
            (EntryKind::Other, false, false),
        ] {
            assert_eq!(k.is_directory(), dir, "kind={:?}", k);
            assert_eq!(k.is_regular_file(), file, "kind={:?}", k);
        }
    }
    fn make_entry(record_id: u64, kind: EntryKind, deleted: bool) -> FsEntry {
        FsEntry {
            record_id,
            parent_record_id: None,
            name: String::new(),
            full_path: None,
            size_bytes: None,
            kind,
            is_deleted: deleted,
            timestamps: FsTimestamps::empty(),
            fs_type: FsType::Ntfs,
        }
    }
    #[test]
    fn fs_entry_default_is_alive_and_anonymous() {
        // FsEntry は Default 派生しない設計のため、手動構築で初期状態を検証する。
        let entry = make_entry(0, EntryKind::File, false);
        assert!(!entry.is_deleted());
        assert!(!entry.is_directory());
        assert_eq!(entry.timestamps, FsTimestamps::default());
        assert!(entry.name.is_empty());
    }
    struct StubReader {
        entries: Vec<FsEntry>,
    }
    impl FsReader for StubReader {
        fn fs_type(&self) -> FsType {
            FsType::Ntfs
        }
        fn root_record_id(&self) -> CoreResult<u64> {
            Ok(5)
        }
        fn read_entry(&mut self, id: u64) -> CoreResult<FsEntry> {
            self.entries
                .iter()
                .find(|e| e.record_id == id)
                .cloned()
                .ok_or_else(|| CoreError::InvalidArgument(format!("no entry: {}", id)))
        }
        fn list_all_entries(&mut self) -> CoreResult<Vec<FsEntry>> {
            Ok(self.entries.clone())
        }
    }
    #[test]
    fn fs_reader_trait_via_stub() {
        let entry = make_entry(5, EntryKind::Directory, false);
        let mut reader = StubReader {
            entries: vec![entry.clone()],
        };
        assert_eq!(reader.fs_type(), FsType::Ntfs);
        assert_eq!(reader.root_record_id().unwrap(), 5);
        assert_eq!(reader.read_entry(5).unwrap(), entry);
        assert!(reader.read_entry(999).is_err());
        assert_eq!(reader.list_all_entries().unwrap().len(), 1);
    }
}
