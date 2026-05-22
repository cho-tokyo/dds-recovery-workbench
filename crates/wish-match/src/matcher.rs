//! Chunk 15-16: 希望リスト × ファイル群のパターンマッチエンジン。
//!
//! 単一ファイル × 単一希望の `matches_wish` を基底に、複数希望をまとめて
//! 評価する `match_file` / `match_files` を提供。マッチ結果は
//! `MatchResult` に集約され、優先度スコア降順でソートされる。
//! Chunk 17 の復旧パイプラインが「優先抽出順」を決めるために使う。
//!
//! Chunk 16 で Glob（`PathGlob` / `FilenameGlob`）、日付範囲（`ModifiedRange` 等）、
//! 論理結合（`All` / `Any` / `Not`）のマッチング規則を追加。
//! 関連 FR: FR-WISH-02 (パターン突合), FR-REC-01 (目標優先抽出)。

use chrono::{DateTime, Utc};
use globset::GlobBuilder;

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

        // === Chunk 16 ===
        WishItem::PathGlob(pattern) => matches_path_glob(&file.path, pattern),
        WishItem::FilenameGlob(pattern) => matches_filename_glob(&file.name, pattern),
        WishItem::ModifiedRange { after, before } => {
            matches_date_range(file.modified, *after, *before)
        }
        WishItem::CreatedRange { after, before } => {
            matches_date_range(file.created, *after, *before)
        }
        WishItem::AccessedRange { after, before } => {
            matches_date_range(file.accessed, *after, *before)
        }
        WishItem::All(items) => items.iter().all(|i| matches_item(file, i)),
        WishItem::Any(items) => items.iter().any(|i| matches_item(file, i)),
        WishItem::Not(item) => !matches_item(file, item),
    }
}

/// 日付範囲マッチング（両端 inclusive）。
///
/// `field_value` が `None` の場合は常に `false`（日付なしファイルは範囲条件に該当しない）。
/// `after` / `before` が `None` の側は無制限。両方 `None` なら日付ありファイルすべて `true`。
fn matches_date_range(
    field_value: Option<DateTime<Utc>>,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> bool {
    let Some(value) = field_value else {
        return false;
    };
    after.map(|a| value >= a).unwrap_or(true) && before.map(|b| value <= b).unwrap_or(true)
}

/// Glob パターンでパスマッチ。
///
/// NTFS パスは `\` 区切りだが、`globset` は `/` 区切り前提。
/// マッチ前に**パスとパターンの両方**を `/` に正規化することで、
/// ユーザがどちらの区切り文字で glob を書いても動く設計。
///
/// `literal_separator(true)` により `*` がパス区切りを跨がない（`**` だけが跨ぐ）。
/// `case_insensitive(true)` により NTFS の挙動と整合した大文字小文字非区別マッチ。
///
/// 不正な glob パターンは `false` を返す（パニックしない寛容な設計）。
fn matches_path_glob(file_path: &str, glob_pattern: &str) -> bool {
    let normalized_path = file_path.replace('\\', "/");
    let normalized_pattern = glob_pattern.replace('\\', "/");
    let glob = match GlobBuilder::new(&normalized_pattern)
        .case_insensitive(true)
        .literal_separator(true)
        .build()
    {
        Ok(g) => g,
        Err(_) => return false,
    };
    glob.compile_matcher().is_match(&normalized_path)
}

/// Glob パターンでファイル名マッチ（パスは無視、ファイル名のみ対象）。
///
/// `literal_separator` は false（ファイル名内にパス区切りは想定しない）。
/// `case_insensitive(true)` で大文字小文字非区別。不正パターンは `false` を返す。
fn matches_filename_glob(filename: &str, glob_pattern: &str) -> bool {
    let glob = match GlobBuilder::new(glob_pattern)
        .case_insensitive(true)
        .build()
    {
        Ok(g) => g,
        Err(_) => return false,
    };
    glob.compile_matcher().is_match(filename)
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
        let wish = Wish::new(
            WishItem::PathPrefix("\\Users\\Chou".into()),
            "ユーザフォルダ",
        );
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
        let item = WishItem::SizeRange {
            min: Some(1000),
            max: Some(5000),
        };
        assert!(matches_item(&build_file("\\a", 1000), &item));
        assert!(matches_item(&build_file("\\a", 5000), &item));
        assert!(!matches_item(&build_file("\\a", 999), &item));
        assert!(!matches_item(&build_file("\\a", 5001), &item));
    }

    #[test]
    fn size_range_min_only_no_upper_bound() {
        let item = WishItem::SizeRange {
            min: Some(1000),
            max: None,
        };
        assert!(matches_item(&build_file("\\a", 1_000_000_000), &item));
        assert!(!matches_item(&build_file("\\a", 500), &item));
    }

    #[test]
    fn modified_range_after_only_filters_correctly() {
        // Chunk 15 の `ModifiedAfter` 相当を Chunk 16 では `ModifiedRange { after, before: None }` で表現。
        // 機能的に同じ結果になることを確認するマイグレーションテスト。
        let cutoff = Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        let mut newer = build_file("\\a", 1);
        newer.modified = Some(Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap());
        let mut older = build_file("\\b", 1);
        older.modified = Some(Utc.with_ymd_and_hms(2026, 4, 20, 0, 0, 0).unwrap());
        let item = WishItem::ModifiedRange {
            after: Some(cutoff),
            before: None,
        };
        assert!(matches_item(&newer, &item));
        assert!(!matches_item(&older, &item));
    }

    #[test]
    fn match_files_sorts_by_priority_score_descending() {
        let f1 = build_file("\\important.docx", 100);
        let f2 = build_file("\\other.txt", 200);
        let wl = Wishlist::new()
            .add(
                Wish::new(WishItem::Extension("docx".into()), "Word")
                    .with_priority(Priority::Critical),
            )
            .add(Wish::new(WishItem::Extension("txt".into()), "Text").with_priority(Priority::Low));
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

    // === Chunk 16: Glob パターン ===

    #[test]
    fn path_glob_matches_extension_in_root() {
        // `*.docx` は literal_separator により階層を跨がない。
        let root = build_file("\\report.docx", 1);
        let nested = build_file("\\dir\\foo.docx", 1);
        let item = WishItem::PathGlob("*.docx".into());
        // ルートは `/report.docx` に正規化、パターンは `*.docx` → 先頭の `/` がない場合、
        // globset は `*.docx` がパス区切りを跨がないので `/report.docx` にはマッチしない。
        // 業務的観点: 「ルート直下」の表現は `/*.docx` または `\\*.docx` を使うべき。
        // ここでは仕様書通り、literal_separator 挙動を検証する。
        assert!(!matches_item(&nested, &item));
        // ルート直下にマッチさせたい場合は明示的に `/` を先頭につける必要がある。
        let root_item = WishItem::PathGlob("/*.docx".into());
        assert!(matches_item(&root, &root_item));
        assert!(!matches_item(&nested, &root_item));
    }

    #[test]
    fn path_glob_double_star_matches_recursive() {
        // `**` は階層を跨ぐ。`\**\*.pdf` で任意の階層下の .pdf がマッチ。
        let f = build_file("\\dir1\\sub1\\foo.pdf", 1);
        let item = WishItem::PathGlob("\\**\\*.pdf".into());
        assert!(matches_item(&f, &item));
    }

    #[test]
    fn path_glob_single_star_one_level_only() {
        // `\Users\*\Documents` は中間 1 階層のみマッチ（literal_separator）。
        let one_level = build_file("\\Users\\Chou\\Documents", 0);
        let two_levels = build_file("\\Users\\Chou\\Sub\\Documents", 0);
        let item = WishItem::PathGlob("\\Users\\*\\Documents".into());
        assert!(matches_item(&one_level, &item));
        assert!(!matches_item(&two_levels, &item));
    }

    #[test]
    fn path_glob_case_insensitive() {
        let f = build_file("\\report.docx", 1);
        let item = WishItem::PathGlob("/*.DOCX".into());
        assert!(matches_item(&f, &item));
    }

    #[test]
    fn path_glob_invalid_pattern_returns_false_no_panic() {
        // 不正な glob パターン（閉じていない `[`）はパニックせず `false` を返す。
        let f = build_file("\\foo.txt", 1);
        let item = WishItem::PathGlob("[unclosed".into());
        assert!(!matches_item(&f, &item));
    }

    #[test]
    fn filename_glob_question_mark_single_char() {
        // `?` は任意の 1 文字。`invoice_2025-??.xlsx` は 2 文字版だけマッチ。
        let two_char = build_file("\\inv\\invoice_2025-Q1.xlsx", 1);
        let three_char = build_file("\\inv\\invoice_2025-Q12.xlsx", 1);
        let item = WishItem::FilenameGlob("invoice_2025-??.xlsx".into());
        assert!(matches_item(&two_char, &item));
        assert!(!matches_item(&three_char, &item));
    }

    #[test]
    fn filename_glob_character_class() {
        // `[0-9]` は 1 桁数字のみマッチ。アルファベットは外れる。
        let digit = build_file("\\file_3.txt", 1);
        let alpha = build_file("\\file_a.txt", 1);
        let item = WishItem::FilenameGlob("file_[0-9].txt".into());
        assert!(matches_item(&digit, &item));
        assert!(!matches_item(&alpha, &item));
    }

    // === Chunk 16: 日付範囲 ===

    fn build_file_with_dates(
        path: &str,
        modified: Option<DateTime<Utc>>,
        created: Option<DateTime<Utc>>,
        accessed: Option<DateTime<Utc>>,
    ) -> FileInfo {
        let mut f = build_file(path, 1);
        f.modified = modified;
        f.created = created;
        f.accessed = accessed;
        f
    }

    #[test]
    fn modified_range_inclusive_boundaries() {
        // 両端 inclusive: ちょうど after / before に等しいファイルもマッチする。
        let after = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2024, 12, 31, 0, 0, 0).unwrap();
        let item = WishItem::ModifiedRange {
            after: Some(after),
            before: Some(before),
        };
        let on_lower = build_file_with_dates("\\a", Some(after), None, None);
        let on_upper = build_file_with_dates("\\b", Some(before), None, None);
        let outside_lower = build_file_with_dates(
            "\\c",
            Some(Utc.with_ymd_and_hms(2023, 12, 31, 23, 59, 59).unwrap()),
            None,
            None,
        );
        let outside_upper = build_file_with_dates(
            "\\d",
            Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 1).unwrap()),
            None,
            None,
        );
        assert!(matches_item(&on_lower, &item));
        assert!(matches_item(&on_upper, &item));
        assert!(!matches_item(&outside_lower, &item));
        assert!(!matches_item(&outside_upper, &item));
    }

    #[test]
    fn modified_range_after_only_no_upper_bound() {
        let cutoff = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let item = WishItem::ModifiedRange {
            after: Some(cutoff),
            before: None,
        };
        let future = build_file_with_dates(
            "\\a",
            Some(Utc.with_ymd_and_hms(2099, 12, 31, 0, 0, 0).unwrap()),
            None,
            None,
        );
        let past = build_file_with_dates(
            "\\b",
            Some(Utc.with_ymd_and_hms(2023, 6, 15, 0, 0, 0).unwrap()),
            None,
            None,
        );
        assert!(matches_item(&future, &item));
        assert!(!matches_item(&past, &item));
    }

    #[test]
    fn modified_range_returns_false_for_no_modified_date() {
        // 業務的判断: 日付情報が欠落しているファイルは「範囲条件には該当しない」（保守的挙動）。
        let f = build_file_with_dates("\\a", None, None, None);
        let item = WishItem::ModifiedRange {
            after: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            before: None,
        };
        assert!(!matches_item(&f, &item));
    }

    #[test]
    fn created_range_works_on_creation_date() {
        // created と modified は別フィールド。CreatedRange は created のみ参照。
        let created = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let f = build_file_with_dates(
            "\\a",
            Some(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap()), // modified は範囲外
            Some(created),
            None,
        );
        let item = WishItem::CreatedRange {
            after: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            before: Some(Utc.with_ymd_and_hms(2024, 12, 31, 0, 0, 0).unwrap()),
        };
        assert!(matches_item(&f, &item));
    }

    #[test]
    fn accessed_range_works_on_access_date() {
        let accessed = Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
        let f = build_file_with_dates("\\a", None, None, Some(accessed));
        let item = WishItem::AccessedRange {
            after: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            before: None,
        };
        assert!(matches_item(&f, &item));
    }

    // === Chunk 16: 論理結合 ===

    #[test]
    fn all_combinator_requires_every_subcondition() {
        let f = build_file("\\Documents\\report.docx", 1024);
        let matched = WishItem::All(vec![
            WishItem::PathPrefix("\\Documents".into()),
            WishItem::Extension("docx".into()),
        ]);
        assert!(matches_item(&f, &matched));
        // 片方外す → AND 不成立。
        let unmatched = WishItem::All(vec![
            WishItem::PathPrefix("\\Documents".into()),
            WishItem::Extension("pdf".into()),
        ]);
        assert!(!matches_item(&f, &unmatched));
    }

    #[test]
    fn all_combinator_empty_vec_returns_true() {
        // vacuous truth: `All(vec![])` は条件なし → true。
        let f = build_file("\\a", 1);
        assert!(matches_item(&f, &WishItem::All(vec![])));
    }

    #[test]
    fn any_combinator_matches_when_any_matches() {
        let f = build_file("\\report.docx", 1);
        let item = WishItem::Any(vec![
            WishItem::Extension("pdf".into()),
            WishItem::Extension("docx".into()), // ← これがマッチ
            WishItem::Extension("xlsx".into()),
        ]);
        assert!(matches_item(&f, &item));
    }

    #[test]
    fn any_combinator_empty_vec_returns_false() {
        let f = build_file("\\a", 1);
        assert!(!matches_item(&f, &WishItem::Any(vec![])));
    }

    #[test]
    fn not_combinator_inverts_match() {
        let f = build_file("\\report.docx", 1);
        let inner = WishItem::Extension("docx".into());
        assert!(matches_item(&f, &inner));
        assert!(!matches_item(&f, &WishItem::Not(Box::new(inner))));
    }

    #[test]
    fn nested_combinators_compose_correctly() {
        // `All(vec![Any(vec![A, B]), Not(C)])`: A か B にマッチ AND C にマッチしない。
        let f = build_file("\\Documents\\report.docx", 1024);
        let item = WishItem::All(vec![
            WishItem::Any(vec![
                WishItem::Extension("docx".into()),
                WishItem::Extension("pdf".into()),
            ]),
            WishItem::Not(Box::new(WishItem::PathPrefix(
                "\\Documents\\$RECYCLE.BIN".into(),
            ))),
        ]);
        assert!(matches_item(&f, &item));

        let in_recycle = build_file("\\Documents\\$RECYCLE.BIN\\report.docx", 1024);
        assert!(!matches_item(&in_recycle, &item));
    }

    // === Chunk 16: 業務シナリオ ===

    #[test]
    fn business_scenario_documents_only_excluding_recycle_bin() {
        // お客様: 「\Users\Chou\Documents 配下の .docx が欲しい、ただしゴミ箱は除く」。
        let item = WishItem::All(vec![
            WishItem::PathPrefix("\\Users\\Chou\\Documents".into()),
            WishItem::Extension("docx".into()),
            WishItem::Not(Box::new(WishItem::PathPrefix(
                "\\Users\\Chou\\Documents\\$RECYCLE.BIN".into(),
            ))),
        ]);
        let normal = build_file("\\Users\\Chou\\Documents\\report.docx", 1);
        let in_recycle = build_file("\\Users\\Chou\\Documents\\$RECYCLE.BIN\\old.docx", 1);
        let wrong_ext = build_file("\\Users\\Chou\\Documents\\foo.pdf", 1);
        let outside = build_file("\\Users\\Other\\report.docx", 1);
        assert!(matches_item(&normal, &item));
        assert!(!matches_item(&in_recycle, &item));
        assert!(!matches_item(&wrong_ext, &item));
        assert!(!matches_item(&outside, &item));
    }

    #[test]
    fn serializes_complex_wish_to_json_and_back() {
        // 論理結合 + glob + 日付範囲を含む複雑な Wish が JSON ラウンドトリップ成功すること。
        // Tauri UI から JSON で渡される業務シナリオを想定。
        let after = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let item = WishItem::All(vec![
            WishItem::Any(vec![
                WishItem::PathGlob("\\**\\*.docx".into()),
                WishItem::PathGlob("\\**\\*.pdf".into()),
            ]),
            WishItem::ModifiedRange {
                after: Some(after),
                before: None,
            },
            WishItem::Not(Box::new(WishItem::FilenameGlob("~$*".into()))),
        ]);
        let wish = Wish::new(item, "Documents .docx/.pdf 2024 以降、一時ファイル除く")
            .with_priority(Priority::Critical);
        let json = serde_json::to_string(&wish).expect("serialize");
        let restored: Wish = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, wish);
    }
}
