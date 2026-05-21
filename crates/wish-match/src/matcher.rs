//! Chunk 15: 希望リスト × ファイル群のパターンマッチエンジン。
//!
//! 単一ファイル × 単一希望の `matches_wish` を基底に、複数希望をまとめて
//! 評価する `match_file` / `match_files` を提供。マッチ結果は
//! `MatchResult` に集約され、優先度スコア降順でソートされる。
//! Chunk 17 の復旧パイプラインが「優先抽出順」を決めるために使う。
//! 関連 FR: FR-WISH-02 (パターン突合), FR-REC-01 (目標優先抽出)。

use crate::file_info::FileInfo;
use crate::wishlist::{Wish, WishItem, Wishlist};

/// 1 つのファイルが 1 つの希望にマッチするかを判定する。
pub fn matches_wish(file: &FileInfo, wish: &Wish) -> bool {
    matches_item(file, &wish.item)
}

/// 1 つのファイルが `WishItem` パターン単体にマッチするか。
///
/// ASCII 範囲の大文字小文字は非区別。日本語など非 ASCII は完全一致のみ
/// （Phase 1 の制約として明示。Unicode 対応は将来チャンクで検討）。
pub fn matches_item(file: &FileInfo, item: &WishItem) -> bool {
    match item {
        WishItem::ExactPath(target) => file.path.eq_ignore_ascii_case(target),
        WishItem::PathPrefix(prefix) => {
            // ディレクトリ境界に `\` を補ってからプレフィックス比較。
            // これで `PathPrefix("\\dir1")` は `\\dir1\\file.txt` にマッチするが、
            // `\\dir1other\\foo.txt` にはマッチしない（境界条件の防衛線）。
            let normalized = if prefix.ends_with('\\') {
                prefix.clone()
            } else {
                format!("{}\\", prefix)
            };
            file.path
                .to_ascii_lowercase()
                .starts_with(&normalized.to_ascii_lowercase())
                || file.path.eq_ignore_ascii_case(prefix)
        }
        WishItem::Extension(ext) => file
            .extension
            .as_deref()
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false),
        WishItem::FilenameContains(needle) => file
            .name
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase()),
        WishItem::SizeRange { min, max } => {
            min.map(|m| file.size >= m).unwrap_or(true)
                && max.map(|m| file.size <= m).unwrap_or(true)
        }
        WishItem::ModifiedAfter(date) => file.modified.map(|m| m >= *date).unwrap_or(false),
        WishItem::ModifiedBefore(date) => file.modified.map(|m| m <= *date).unwrap_or(false),
    }
}

/// マッチ結果。1 ファイルにつき複数の希望がマッチし得る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult<'a> {
    /// マッチした FileInfo の `source_id`（復旧ソース特定用）。
    pub source_id: String,
    /// マッチした希望のリスト（`Wishlist` を借用）。
    pub matched_wishes: Vec<&'a Wish>,
    /// 優先度スコアの合計（マッチ希望の `Priority::score()` の総和）。
    pub priority_score: u32,
}

/// 1 つの `FileInfo` について Wishlist 全体とマッチを取り、結果を返す。
/// マッチが 1 つもなければ `None`。
pub fn match_file<'a>(file: &FileInfo, wishlist: &'a Wishlist) -> Option<MatchResult<'a>> {
    let matched: Vec<&Wish> = wishlist
        .wishes
        .iter()
        .filter(|w| matches_wish(file, w))
        .collect();
    if matched.is_empty() {
        return None;
    }
    let priority_score = matched.iter().map(|w| w.priority.score()).sum();
    Some(MatchResult {
        source_id: file.source_id.clone(),
        matched_wishes: matched,
        priority_score,
    })
}

/// 複数の `FileInfo` を `Wishlist` でフィルタし、マッチしたファイルを
/// 優先度スコア降順でソートして返す（同点は入力順を保つ stable sort）。
pub fn match_files<'a>(files: &[FileInfo], wishlist: &'a Wishlist) -> Vec<MatchResult<'a>> {
    let mut results: Vec<MatchResult<'_>> = files
        .iter()
        .filter_map(|f| match_file(f, wishlist))
        .collect();
    results.sort_by_key(|r| std::cmp::Reverse(r.priority_score));
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wishlist::Priority;
    use chrono::{TimeZone, Utc};

    fn build_file(path: &str, size: u64) -> FileInfo {
        let mut f = FileInfo::new(path, size);
        f.source_id = format!("TEST#{}", path);
        f
    }

    #[test]
    fn exact_path_case_insensitive() {
        let f = build_file("\\Foo\\Bar.txt", 10);
        let wish = Wish::new(WishItem::ExactPath("\\foo\\BAR.txt".into()), "exact");
        assert!(matches_wish(&f, &wish));
    }

    #[test]
    fn path_prefix_matches_subdirectory_files() {
        // 業務シナリオ: お客様が「Users\Chou 配下のファイルが欲しい」と希望。
        let f = build_file("\\Users\\Chou\\Documents\\report.docx", 1024);
        let wish = Wish::new(WishItem::PathPrefix("\\Users\\Chou".into()), "ユーザフォルダ");
        assert!(matches_wish(&f, &wish));
    }

    #[test]
    fn path_prefix_does_not_match_partial_directory_name() {
        // 境界条件の防衛線: `\Users` は `\UsersOther\...` にマッチしてはいけない。
        let f = build_file("\\UsersOther\\foo.txt", 10);
        let wish = Wish::new(WishItem::PathPrefix("\\Users".into()), "Users 配下");
        assert!(!matches_wish(&f, &wish));
    }

    #[test]
    fn extension_case_insensitive() {
        let f = build_file("\\report.DOCX", 1);
        assert!(matches_item(&f, &WishItem::Extension("docx".into())));
    }

    #[test]
    fn filename_contains_case_insensitive() {
        let f = build_file("\\INVOICE_2025.pdf", 1);
        assert!(matches_item(
            &f,
            &WishItem::FilenameContains("invoice".into())
        ));
    }

    #[test]
    fn size_range_min_and_max_inclusive() {
        let item = WishItem::SizeRange { min: Some(1000), max: Some(5000) };
        assert!(matches_item(&build_file("\\a", 1000), &item));
        assert!(matches_item(&build_file("\\a", 5000), &item));
        assert!(!matches_item(&build_file("\\a", 999), &item));
        assert!(!matches_item(&build_file("\\a", 5001), &item));
    }

    #[test]
    fn size_range_min_only_no_upper_bound() {
        let item = WishItem::SizeRange { min: Some(1000), max: None };
        assert!(matches_item(&build_file("\\a", 1_000_000_000), &item));
        assert!(!matches_item(&build_file("\\a", 500), &item));
    }

    #[test]
    fn modified_after_correctly_filters_by_date() {
        let cutoff = Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        let mut newer = build_file("\\a", 1);
        newer.modified = Some(Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap());
        let mut older = build_file("\\b", 1);
        older.modified = Some(Utc.with_ymd_and_hms(2026, 4, 20, 0, 0, 0).unwrap());
        let item = WishItem::ModifiedAfter(cutoff);
        assert!(matches_item(&newer, &item));
        assert!(!matches_item(&older, &item));
    }

    #[test]
    fn match_files_sorts_by_priority_score_descending() {
        let f1 = build_file("\\important.docx", 100);
        let f2 = build_file("\\other.txt", 200);
        let wl = Wishlist::new()
            .add(Wish::new(WishItem::Extension("docx".into()), "Word")
                .with_priority(Priority::Critical))
            .add(Wish::new(WishItem::Extension("txt".into()), "Text")
                .with_priority(Priority::Low));
        let results = match_files(&[f1, f2], &wl);
        assert_eq!(results.len(), 2);
        assert!(results[0].priority_score > results[1].priority_score);
        assert_eq!(results[0].priority_score, 100); // Critical
        assert_eq!(results[1].priority_score, 25); // Low
    }

    #[test]
    fn match_file_returns_none_when_no_match() {
        let f = build_file("\\foo.bin", 1);
        let wl = Wishlist::new().add(Wish::new(WishItem::Extension("docx".into()), "Word"));
        assert!(match_file(&f, &wl).is_none());
    }
}
