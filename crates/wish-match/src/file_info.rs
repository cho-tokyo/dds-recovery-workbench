//! Chunk 15: FS 非依存の汎用ファイル情報 `FileInfo`。
//!
//! 各 FS リーダ (fs-ntfs, fs-exfat, fs-fat32) がこの形式に変換して
//! wish-match エンジンに渡す。業務統合層を FS 種別から独立させるための境界型。
//! 関連 FR: FR-WISH-02 (パターン突合), FR-REC-01 (目標優先抽出)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 抽象的なファイル情報。ファイルシステム種別に依存しない汎用表現。
///
/// 各 FS リーダがこの形式に変換して wish-match エンジンに渡す。
/// `source_id` は復旧フェーズで原本特定に使う（例: `"NTFS#67"`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileInfo {
    /// ファイルシステム上のフルパス（NTFS なら `\dir\file.txt` 形式）。
    pub path: String,
    /// ファイル名のみ（拡張子含む）。
    pub name: String,
    /// 拡張子（小文字、ドットなし、例: `"docx"`）。なければ `None`。
    pub extension: Option<String>,
    /// 実データサイズ（バイト）。
    pub size: u64,
    /// 作成日時。
    pub created: Option<DateTime<Utc>>,
    /// 内容更新日時。
    pub modified: Option<DateTime<Utc>>,
    /// アクセス日時。
    pub accessed: Option<DateTime<Utc>>,
    /// 削除済みエントリか。
    pub is_deleted: bool,
    /// ディレクトリか。
    pub is_directory: bool,
    /// 復旧ソース識別子（業務層で使用、例: `"NTFS#67"`）。
    pub source_id: String,
}

impl FileInfo {
    /// FileInfo を作る最小コンストラクタ。`name` と `extension` をパスから自動派生。
    ///
    /// パス区切りは Windows 慣例の `\` を期待する。拡張子は小文字化して保存される。
    pub fn new(path: impl Into<String>, size: u64) -> Self {
        let path = path.into();
        let name = path
            .rsplit_once('\\')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| path.clone());
        let extension = name.rsplit_once('.').map(|(_, ext)| ext.to_lowercase());
        Self {
            path,
            name,
            extension,
            size,
            created: None,
            modified: None,
            accessed: None,
            is_deleted: false,
            is_directory: false,
            source_id: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_info_new_parses_extension() {
        let fi = FileInfo::new("\\foo\\bar.docx", 1234);
        assert_eq!(fi.name, "bar.docx");
        assert_eq!(fi.extension, Some("docx".to_string()));
        assert_eq!(fi.size, 1234);
    }

    #[test]
    fn file_info_new_no_extension_returns_none() {
        let fi = FileInfo::new("\\foo\\Makefile", 100);
        assert_eq!(fi.name, "Makefile");
        assert_eq!(fi.extension, None);
    }

    #[test]
    fn file_info_new_lowercases_extension() {
        let fi = FileInfo::new("\\FOO.PDF", 0);
        assert_eq!(fi.extension, Some("pdf".to_string()));
    }
}
