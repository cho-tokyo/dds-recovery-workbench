//! Chunk 23.7: 復旧から除外するパターンのリスト `ExclusionList`。
//!
//! Phase 1.5 で「全件復旧 + 除外」設計（R-STUDIO 風）に切り替えた際、
//! システムファイル等の「絶対に復旧したくないもの」を表現するために導入。
//!
//! [`Wishlist`](crate::Wishlist) は「お客様優先データ」のラベリング、
//! `ExclusionList` は「業務的にユーザに渡したくないもの」のフィルタという
//! 役割分担になる。
//!
//! 関連 FR: FR-REC-05 (全件復旧、業務適用), FR-REC-06 (システムファイル除外)。

use serde::{Deserialize, Serialize};

/// 復旧から除外するパターンのリスト。
///
/// 業務的に「絶対に復旧しない」システムファイルを排除するために使う。
/// デフォルトは [`ExclusionList::default_system_exclusions`] で
/// Windows / NTFS のシステム系を網羅。
///
/// マッチ判定は [`ExclusionList::matches`] で行い、case-insensitive で
/// 比較する（Windows ファイルシステムは大文字小文字を区別しないため）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExclusionList {
    /// 除外パターン群（先頭マッチで除外確定）。
    pub patterns: Vec<ExclusionPattern>,
}

/// 個別の除外パターン。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value")]
pub enum ExclusionPattern {
    /// パスがこの接頭辞で始まる場合に除外（case-insensitive）。
    /// 例: `"\\Windows\\"`, `"\\Program Files\\"`
    PathPrefix(String),

    /// ファイル名（パスの末尾要素）がこの文字列で始まる場合に除外（case-insensitive）。
    /// 例: `"$"` で NTFS の `$Boot` / `$MFT` 等を一括除外。
    NameStartsWith(String),

    /// 拡張子による除外（case-insensitive、ドットなしで指定）。
    /// 例: `"tmp"`, `"bak"`
    Extension(String),
}

impl ExclusionList {
    /// DDS 業務標準の除外パターン。
    ///
    /// Windows システム、NTFS メタデータ、ゴミ箱、`$` 始まりのシステムファイルを除外。
    /// 「お客様の私的データを全件復旧する」シナリオで「お客様が見たくないもの」を
    /// 自動的に除外する業務デフォルト。
    pub fn default_system_exclusions() -> Self {
        Self {
            patterns: vec![
                // Windows システム
                ExclusionPattern::PathPrefix("\\Windows\\".into()),
                ExclusionPattern::PathPrefix("\\Program Files\\".into()),
                ExclusionPattern::PathPrefix("\\Program Files (x86)\\".into()),
                // NTFS メタデータ / ゴミ箱
                ExclusionPattern::PathPrefix("\\$Recycle.Bin\\".into()),
                ExclusionPattern::PathPrefix("\\System Volume Information\\".into()),
                ExclusionPattern::PathPrefix("\\$Extend\\".into()),
                // NTFS システムファイル ($MFT, $Bitmap 等)
                ExclusionPattern::NameStartsWith("$".into()),
            ],
        }
    }

    /// 何も除外しない空リスト（テスト・特殊シナリオ用）。
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// パターン追加（builder pattern）。
    ///
    /// `Wishlist::add` と同じく builder スタイルを優先するため、`std::ops::Add`
    /// 実装は意図的に避けている（業務コードの読みやすさ優先）。
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, pattern: ExclusionPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    /// 指定パスがいずれかの除外パターンにマッチするか判定（case-insensitive）。
    ///
    /// パスは NTFS 慣例の `\` 区切り。`path` は完全パス（例: `\Users\foo\bar.txt`）。
    /// パターンが 1 つでもマッチすれば `true` を返す（短絡評価）。
    pub fn matches(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        for pattern in &self.patterns {
            match pattern {
                ExclusionPattern::PathPrefix(prefix) => {
                    if lower.starts_with(&prefix.to_lowercase()) {
                        return true;
                    }
                }
                ExclusionPattern::NameStartsWith(prefix) => {
                    let filename = filename_from_path(&lower);
                    if filename.starts_with(&prefix.to_lowercase()) {
                        return true;
                    }
                }
                ExclusionPattern::Extension(ext) => {
                    if lower.ends_with(&format!(".{}", ext.to_lowercase())) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// パスから最後の `\` 以降を切り出してファイル名部分を返す。
/// 区切りが無ければパス全体をファイル名とみなす。
fn filename_from_path(path: &str) -> &str {
    match path.rfind('\\') {
        Some(pos) => &path[pos + 1..],
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_path_prefix_matches_windows_folder() {
        let ex = ExclusionList::default_system_exclusions();
        assert!(ex.matches("\\Windows\\System32\\drivers\\foo.sys"));
        assert!(ex.matches("\\Program Files\\Notepad++\\notepad++.exe"));
        // 関係ないパスはマッチしない。
        assert!(!ex.matches("\\Users\\Chou\\Documents\\report.docx"));
    }

    #[test]
    fn exclusion_path_prefix_case_insensitive() {
        // Windows ファイルシステムは大文字小文字非区別なので、
        // パターン側 / パス側どちらの casing でもマッチすること。
        let ex = ExclusionList::empty().add(ExclusionPattern::PathPrefix("\\Windows\\".into()));
        assert!(ex.matches("\\WINDOWS\\foo.txt"));
        assert!(ex.matches("\\windows\\foo.txt"));
        assert!(ex.matches("\\WiNdOwS\\foo.txt"));
    }

    #[test]
    fn exclusion_name_starts_with_dollar_sign() {
        // `$` 始まりのファイル名は NTFS システムファイル → 除外。
        let ex = ExclusionList::empty().add(ExclusionPattern::NameStartsWith("$".into()));
        assert!(ex.matches("\\$MFT"));
        assert!(ex.matches("\\$Boot"));
        assert!(ex.matches("\\subdir\\$Secure"));
        // 普通のファイルはマッチしない。
        assert!(!ex.matches("\\subdir\\report.docx"));
    }

    #[test]
    fn exclusion_default_includes_windows_system() {
        // 業務デフォルトに必要なパターンが含まれていることを保証（回帰テスト）。
        let ex = ExclusionList::default_system_exclusions();
        assert!(ex.matches("\\Windows\\notepad.exe"));
        assert!(ex.matches("\\Program Files\\foo.exe"));
        assert!(ex.matches("\\Program Files (x86)\\bar.exe"));
        assert!(ex.matches("\\$Recycle.Bin\\S-1-5\\$IXY.dat"));
        assert!(ex.matches("\\System Volume Information\\tracking.log"));
        assert!(ex.matches("\\$Extend\\$UsnJrnl"));
        assert!(ex.matches("\\$MFT"));
    }

    #[test]
    fn exclusion_empty_matches_nothing() {
        let ex = ExclusionList::empty();
        assert!(!ex.matches("\\Windows\\foo.exe"));
        assert!(!ex.matches("\\$MFT"));
        assert!(!ex.matches("\\anything\\at\\all.bin"));
    }

    #[test]
    fn exclusion_add_chain_pattern() {
        // builder pattern で複数パターンを連鎖追加できる。
        let ex = ExclusionList::empty()
            .add(ExclusionPattern::PathPrefix("\\foo\\".into()))
            .add(ExclusionPattern::Extension("tmp".into()))
            .add(ExclusionPattern::NameStartsWith("~".into()));
        assert_eq!(ex.patterns.len(), 3);
        assert!(ex.matches("\\foo\\bar.docx"));
        assert!(ex.matches("\\other\\file.tmp"));
        assert!(ex.matches("\\dir\\~temp.lock"));
        assert!(!ex.matches("\\dir\\normal.docx"));
    }
}
