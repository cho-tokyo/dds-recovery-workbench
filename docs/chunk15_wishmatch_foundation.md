# Chunk 15 指示: wish-match 業務統合基盤 + NtfsFile 拡張

このチャンクで **NTFS 実装層から業務統合層への第一歩** を踏み出します。お客様の希望リスト（wishlist）に基づいた優先復旧の中核となる、データ構造とパターンマッチエンジンの基盤を構築します。

> 🎯 完了時点で「NTFS イメージから希望に合うファイルを抽出」が end-to-end で動く状態に。Phase 1 PRD のマイルストーン **M3「希望突合エンジン」着手**。

---

## 目的（2 部構成）

### Part A: NtfsFile 小幅拡張（5 分作業）

- `NtfsFile::has_system_name_prefix()` メソッド追加
- 業務統合層から「`$RECYCLE.BIN` 配下のゴミ箱ファイル」をオプトインで識別可能に

### Part B: wish-match 業務統合基盤（メイン作業）

- `crates/wish-match/` クレート本実装（Chunk 1 で空スケルトン作成済み）
- 「希望リスト」のデータ構造を定義（パス・拡張子・サイズ・日付パターン）
- パターンマッチエンジン（基本パターン: ExactPath / PathPrefix / Extension / FilenameContains / SizeRange）
- `NtfsFile` → 抽象的な `FileInfo` への変換
- 結合テスト: 実 NTFS イメージから希望ファイル抽出

> ⚠️ **重要**: このチャンクは Chunks 4-14 の NTFS 技術実装とは**質的に異なる**「業務ロジック」層です。書籍参照は不要、ビジネス要件の正確な表現が中心になります。

## 対象クレート

- **主**: `crates/wish-match/` (本実装)
- **副**: `crates/fs-ntfs/` (NtfsFile に追加 + FileInfo 変換実装)

## 仕様参照

### ビジネス要件（書籍ではなく PRD ベース）

- **FR-REC-01**: 目標優先抽出 — 希望リストの優先度順に復旧
- **FR-WISH-01**: 希望リスト管理 — お客様が「欲しいファイル」を表現できる UI（基盤）
- **FR-WISH-02**: パターン突合 — パス・拡張子・名前部分文字列・サイズ・日付での照合

### 既存の参照

- 既存実装: `crates/fs-ntfs/src/file.rs` の `NtfsFile`（Chunk 14）
- `docs/PRD.md` の wish-match 関連セクション

---

## 実装内容

### Part A: NtfsFile::has_system_name_prefix（5 分）

`crates/fs-ntfs/src/file.rs` の `impl NtfsFile` ブロックに追加:

```rust
impl NtfsFile {
    // ... 既存メソッド ...
    
    /// 名前が `$` で始まるか（システムファイルまたはゴミ箱内ファイル）。
    ///
    /// 注意: `$RECYCLE.BIN` 配下の削除ファイル（`$I*` / `$R*` 命名）も該当する。
    /// このメソッドだけで「ユーザファイル除外」してはいけない。
    /// 業務統合層がオプトインで使うフィルタ。
    pub fn has_system_name_prefix(&self) -> bool {
        self.name.starts_with('$')
    }
}
```

単体テスト追加（既存 `#[cfg(test)] mod tests` 内）:

```rust
#[test]
fn has_system_name_prefix_true_for_dollar_files() {
    let file = build_test_ntfs_file_with_name("$MFT");
    assert!(file.has_system_name_prefix());
}

#[test]
fn has_system_name_prefix_true_for_recycle_bin_entries() {
    let file = build_test_ntfs_file_with_name("$IABC123.docx");
    assert!(file.has_system_name_prefix());
    // でも record_index > 24 なら is_user_file は true
    // （業務層がパスで $RECYCLE.BIN を区別する想定）
}

#[test]
fn has_system_name_prefix_false_for_normal_files() {
    let file = build_test_ntfs_file_with_name("report.docx");
    assert!(!file.has_system_name_prefix());
}
```

これで Part A 完了。Part B に進みます。

---

### Part B: wish-match クレート本実装

#### モジュール構成

```
crates/wish-match/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-export
    ├── error.rs        ← WishMatchError
    ├── file_info.rs    ← FileInfo（FS非依存の汎用ファイル情報）
    ├── wishlist.rs     ← Wishlist / Wish / WishItem 型
    └── matcher.rs      ← パターンマッチエンジン
```

#### 1. `Cargo.toml`

`crates/wish-match/Cargo.toml`:

```toml
[package]
name = "dds-wish-match"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
```

このチャンク段階では fs-ntfs に依存しない（業務層を FS から独立に保つ）。  
`From<&NtfsFile> for FileInfo` は fs-ntfs 側に実装するので、fs-ntfs → wish-match の単方向依存になる。

`crates/fs-ntfs/Cargo.toml` に追加:

```toml
[dependencies]
# ... 既存 ...
dds-wish-match.workspace = true
```

ワークスペースルートの `Cargo.toml` で `dds-wish-match` のパス指定済みと仮定（Chunk 1 でスケルトン作成時に登録済み）。

#### 2. `error.rs`

確立されたエラー命名規約に準拠:

```rust
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum WishMatchError {
    #[error("Invalid path pattern: {pattern} ({reason})")]
    InvalidPathPattern { pattern: String, reason: String },
    
    #[error("Invalid size range: min={min:?}, max={max:?} ({reason})")]
    InvalidSizeRange { min: Option<u64>, max: Option<u64>, reason: String },
    
    #[error("Invalid date range: after={after:?}, before={before:?} ({reason})")]
    InvalidDateRange { after: Option<String>, before: Option<String>, reason: String },
    
    #[error("Empty wishlist (must contain at least one wish)")]
    EmptyWishlist,
}
```

#### 3. `file_info.rs`

抽象的なファイル情報。FS非依存:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 抽象的なファイル情報。ファイルシステム種別に依存しない汎用表現。
///
/// 各 FS リーダ（fs-ntfs, fs-exfat 等）がこの形式に変換して
/// wish-match エンジンに渡す。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    /// ファイルシステム上のフルパス（NTFS なら `\dir\file.txt` 形式）
    pub path: String,
    
    /// ファイル名のみ（拡張子含む）
    pub name: String,
    
    /// 拡張子（小文字、ドットなし、例: `"docx"`）。なければ None。
    pub extension: Option<String>,
    
    /// 実データサイズ（バイト）
    pub size: u64,
    
    /// 作成日時
    pub created: Option<DateTime<Utc>>,
    
    /// 内容更新日時
    pub modified: Option<DateTime<Utc>>,
    
    /// アクセス日時
    pub accessed: Option<DateTime<Utc>>,
    
    /// 削除済みエントリか
    pub is_deleted: bool,
    
    /// ディレクトリか
    pub is_directory: bool,
    
    /// 復旧ソース識別子（業務層で使用、例: `"NTFS#67"` など）
    pub source_id: String,
}

impl FileInfo {
    /// FileInfo を作る最小コンストラクタ。
    pub fn new(path: impl Into<String>, size: u64) -> Self {
        let path = path.into();
        let name = path.rsplit_once('\\')
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
```

#### 4. `wishlist.rs`

希望リストとそのアイテムの定義:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 優先度。お客様が「絶対欲しい」から「あったら嬉しい」まで段階表現。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 100,
    High = 75,
    Normal = 50,
    Low = 25,
}

impl Priority {
    pub fn score(self) -> u32 {
        self as u32
    }
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// 個別の希望アイテム。1 つの WishItem は 1 つのマッチ規則を表現する。
///
/// Phase 1 では基本パターンのみ。glob (* / **) や論理結合 (And/Or/Not) は Chunk 16+ で追加予定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WishItem {
    /// 完全一致するパス（大文字小文字非区別）
    ExactPath(String),
    
    /// 指定パス配下（プレフィックス一致、大文字小文字非区別）
    /// 例: `"\\Users\\Chou\\Documents"` → その配下の全ファイル
    PathPrefix(String),
    
    /// 拡張子一致（小文字比較、ドットなし）
    Extension(String),
    
    /// ファイル名に部分一致する文字列（大文字小文字非区別）
    /// 例: `"invoice"` → "Invoice_2025.pdf", "INVOICE-Q4.xlsx" 等
    FilenameContains(String),
    
    /// ファイルサイズ範囲（バイト）。`min`/`max` どちらも省略可。
    SizeRange { min: Option<u64>, max: Option<u64> },
    
    /// 内容更新日時の範囲
    ModifiedAfter(DateTime<Utc>),
    ModifiedBefore(DateTime<Utc>),
}

/// 単一の希望（マッチ規則 + 優先度 + ラベル）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wish {
    pub item: WishItem,
    pub priority: Priority,
    /// 人間が読むラベル（例: "クライアントAの請求書"）
    pub label: String,
}

impl Wish {
    pub fn new(item: WishItem, label: impl Into<String>) -> Self {
        Self {
            item,
            priority: Priority::Normal,
            label: label.into(),
        }
    }
    
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// お客様の希望リスト全体。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Wishlist {
    pub wishes: Vec<Wish>,
}

impl Wishlist {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add(mut self, wish: Wish) -> Self {
        self.wishes.push(wish);
        self
    }
    
    pub fn is_empty(&self) -> bool {
        self.wishes.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.wishes.len()
    }
}
```

#### 5. `matcher.rs`

パターンマッチエンジン本体:

```rust
use crate::file_info::FileInfo;
use crate::wishlist::{Wish, WishItem, Wishlist, Priority};

/// 1 つのファイルが 1 つの希望にマッチするかを判定する。
pub fn matches_wish(file: &FileInfo, wish: &Wish) -> bool {
    matches_item(file, &wish.item)
}

/// 1 つのファイルが WishItem パターンにマッチするか。
pub fn matches_item(file: &FileInfo, item: &WishItem) -> bool {
    match item {
        WishItem::ExactPath(target) => {
            file.path.eq_ignore_ascii_case(target)
        }
        WishItem::PathPrefix(prefix) => {
            let normalized_prefix = if prefix.ends_with('\\') { 
                prefix.clone() 
            } else { 
                format!("{}\\", prefix) 
            };
            file.path.to_ascii_lowercase().starts_with(&normalized_prefix.to_ascii_lowercase())
            // または exact prefix match
            || file.path.eq_ignore_ascii_case(prefix)
        }
        WishItem::Extension(ext) => {
            file.extension.as_deref().map(|e| e.eq_ignore_ascii_case(ext)).unwrap_or(false)
        }
        WishItem::FilenameContains(needle) => {
            file.name.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
        }
        WishItem::SizeRange { min, max } => {
            min.map(|m| file.size >= m).unwrap_or(true)
                && max.map(|m| file.size <= m).unwrap_or(true)
        }
        WishItem::ModifiedAfter(date) => {
            file.modified.map(|m| m >= *date).unwrap_or(false)
        }
        WishItem::ModifiedBefore(date) => {
            file.modified.map(|m| m <= *date).unwrap_or(false)
        }
    }
}

/// マッチ結果。1 ファイルにつき複数の希望がマッチし得る。
#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult<'a> {
    /// マッチした FileInfo の source_id
    pub source_id: String,
    /// マッチした希望のリスト
    pub matched_wishes: Vec<&'a Wish>,
    /// 優先度の合計スコア（マッチ希望の Priority::score() の合計）
    pub priority_score: u32,
}

/// 1 つの FileInfo について、Wishlist 全体とマッチを取り、結果を返す。
/// マッチが 1 つもなければ None。
pub fn match_file<'a>(file: &FileInfo, wishlist: &'a Wishlist) -> Option<MatchResult<'a>> {
    let matched: Vec<&Wish> = wishlist.wishes.iter()
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

/// 複数の FileInfo を Wishlist でフィルタし、マッチしたファイルを優先度順にソートして返す。
pub fn match_files<'a>(
    files: &[FileInfo],
    wishlist: &'a Wishlist,
) -> Vec<MatchResult<'a>> {
    let mut results: Vec<MatchResult> = files.iter()
        .filter_map(|f| match_file(f, wishlist))
        .collect();
    
    // 優先度スコア降順でソート
    results.sort_by(|a, b| b.priority_score.cmp(&a.priority_score));
    results
}
```

#### 6. `lib.rs`

```rust
//! DDS 希望リスト突合エンジン。
//!
//! お客様の「復旧したいファイル」希望を構造化し、
//! ファイルシステムから取得した FileInfo 群とマッチを取る。

pub mod error;
pub mod file_info;
pub mod matcher;
pub mod wishlist;

pub use error::WishMatchError;
pub use file_info::FileInfo;
pub use matcher::{match_file, match_files, matches_item, matches_wish, MatchResult};
pub use wishlist::{Priority, Wish, WishItem, Wishlist};
```

### Part B 連携: fs-ntfs に変換実装

`crates/fs-ntfs/src/file.rs` の末尾に追加:

```rust
use dds_wish_match::FileInfo;

impl From<&NtfsFile> for FileInfo {
    fn from(file: &NtfsFile) -> Self {
        FileInfo {
            path: file.path.clone(),
            name: file.name.clone(),
            extension: file.extension(),
            size: file.size,
            created: file.created,
            modified: file.modified,
            accessed: file.accessed,
            is_deleted: file.is_deleted,
            is_directory: file.is_directory,
            source_id: format!("NTFS#{}", file.record_index),
        }
    }
}
```

## 単体テスト要件

### wish-match クレート（最低 12 件）

`wishlist.rs` 内:

1. **`wish_can_be_built_with_priority`**: `Wish::new(...).with_priority(Priority::Critical)` で構築
2. **`wishlist_builder_pattern_chains`**: `Wishlist::new().add(w1).add(w2)` で 2 件
3. **`wishlist_serializes_to_json`**: serde_json でラウンドトリップ成功
4. **`priority_ordering_correct`**: `Critical > High > Normal > Low`

`matcher.rs` 内:

5. **`exact_path_case_insensitive`**: `\Foo\Bar.txt` vs `\foo\BAR.txt` でマッチ
6. **`path_prefix_matches_subdirectory_files`**: `PathPrefix("\Users\Chou")` が `\Users\Chou\Documents\report.docx` にマッチ
7. **`path_prefix_does_not_match_partial_directory_name`**: `PathPrefix("\Users")` は `\UsersOther\foo.txt` にマッチしない
8. **`extension_case_insensitive`**: `Extension("docx")` が `.DOCX` ファイルにマッチ
9. **`filename_contains_case_insensitive`**: `FilenameContains("invoice")` が `INVOICE_2025.pdf` にマッチ
10. **`size_range_min_and_max_inclusive`**: 境界値 = 1000, max = 5000 で 1000 と 5000 両方マッチ
11. **`size_range_min_only_no_upper_bound`**: max=None で巨大ファイルも OK
12. **`modified_after_correctly_filters_by_date`**: 2026-05-01 以降の updated にマッチ
13. **`match_files_sorts_by_priority_score_descending`**: 3 ファイル × 2 希望で、合計スコア降順
14. **`match_file_returns_none_when_no_match`**: マッチなしで None

`file_info.rs` 内:

15. **`file_info_new_parses_extension`**: `\foo\bar.docx` から `extension = Some("docx")`
16. **`file_info_new_no_extension_returns_none`**: `\foo\Makefile` で `extension = None`
17. **`file_info_new_lowercases_extension`**: `\FOO.PDF` で `extension = Some("pdf")`

### fs-ntfs（最低 4 件、has_system_name_prefix + From 変換）

18. **`has_system_name_prefix_true_for_mft`**: 名前 "$MFT" で true
19. **`has_system_name_prefix_true_for_recycle_bin_entries`**: 名前 "$IABC123.docx" で true
20. **`has_system_name_prefix_false_for_normal_files`**: 名前 "report.docx" で false
21. **`from_ntfs_file_to_file_info_preserves_all_fields`**: NtfsFile → FileInfo 変換で path, name, size, timestamps が一致
22. **`from_ntfs_file_sets_correct_source_id`**: `source_id = "NTFS#67"` 形式

## 結合テスト要件

`crates/fs-ntfs/tests/wish_match_integration.rs` を作成（fs-ntfs 側にテストを置く理由: fs-ntfs と wish-match の橋渡しを検証するため）:

### 1. **NTFS イメージから .txt ファイルを抽出**

```rust
use dds_fs_ntfs::*;
use dds_wish_match::*;

#[test]
fn matches_all_txt_files_in_directories_fixture() {
    let img = decompress_fixture("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // 希望リスト: .txt ファイル全部
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".to_string()), "テキストファイル全部")
             .with_priority(Priority::High));
    
    // iter_files で NtfsFile を取得して FileInfo に変換
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    // マッチング
    let matches = match_files(&file_infos, &wishlist);
    
    // ntfs_directories は 109 個の .txt ファイルを持つ
    let unique_paths: std::collections::HashSet<_> = matches.iter()
        .map(|m| &m.source_id)
        .collect();
    
    assert_eq!(unique_paths.len(), 109);
}
```

### 2. **多階層パスでの PathPrefix マッチ**

```rust
#[test]
fn matches_files_in_dir1_subdirectory_only() {
    let img = decompress_fixture("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::PathPrefix("\\dir1".to_string()), "dir1 配下")
             .with_priority(Priority::Critical));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    // dir1 配下: file_001.txt + sub1/file_002.txt + sub1/sub2/file_deeply.txt = 3 ファイル
    let paths: Vec<&str> = matches.iter().map(|m| m.source_id.as_str()).collect();
    assert_eq!(matches.len(), 3, "Expected 3 files under \\dir1, got {:?}", paths);
}
```

### 3. **削除ファイル + 拡張子フィルタの組合せ**

```rust
#[test]
fn matches_deleted_files_with_txt_extension() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".to_string()), "復旧したい .txt"));
    
    let deleted_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && f.is_deleted)
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&deleted_infos, &wishlist);
    
    // 削除された .txt は 5 個
    assert_eq!(matches.len(), 5);
}
```

### 4. **プロダクトデモテスト（業務価値の実証）**

```rust
#[test]
fn product_demo_wish_match_with_priority() {
    let img = decompress_fixture("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // お客様が出した希望（架空のシナリオ）
    let wishlist = Wishlist::new()
        // 最重要: dir1/sub1/sub2 配下のファイル（重要な深い階層）
        .add(Wish::new(WishItem::PathPrefix("\\dir1\\sub1\\sub2".to_string()), 
                       "最深部の重要書類")
             .with_priority(Priority::Critical))
        // 重要: file_root_* の名前を含むファイル
        .add(Wish::new(WishItem::FilenameContains("file_root".to_string()),
                       "ルート直下の root_ プレフィックスファイル")
             .with_priority(Priority::High))
        // 通常: .txt 全般
        .add(Wish::new(WishItem::Extension("txt".to_string()), "テキスト全般")
             .with_priority(Priority::Low));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    println!("\n=== Wishlist Match Results (Priority-Sorted) ===\n");
    for (i, m) in matches.iter().enumerate().take(15) {
        println!("  {:2}. [{}] {} (matched: {})",
            i + 1,
            m.priority_score,
            m.source_id,
            m.matched_wishes.iter().map(|w| w.label.as_str()).collect::<Vec<_>>().join(", "));
    }
    
    // file_deeply.txt は Critical(100) + Low(25) = 125 で最高スコア
    assert_eq!(matches[0].priority_score, 125);
    assert!(matches[0].matched_wishes.iter().any(|w| w.label.contains("最深部")));
}
```

## Cargo.toml 設定（再掲）

`crates/wish-match/Cargo.toml`:
```toml
[dependencies]
chrono.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true
```

`crates/fs-ntfs/Cargo.toml`:
```toml
[dependencies]
# 既存に追加:
dds-wish-match.workspace = true
```

## 制約

- **行数目安**:
  - `crates/wish-match/src/` 合計: 300 行以内（実装 + 単体テスト）
  - `crates/fs-ntfs/src/file.rs` 追加分: 30 行以内
- **単体テスト最低 17 件**（wish-match 12 + fs-ntfs 4 + file_info 3）
- **結合テスト最低 4 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、書き込み API 0 件**
- **エラー型は thiserror、命名規約準拠**
- **wish-match は fs-ntfs に依存しない**（依存方向: fs-ntfs → wish-match の単方向）
- **serde 派生で Wishlist/Wish/WishItem が JSON シリアライズ可能**（将来の UI 連携用）

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-wish-match` が全パス（≥17 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥4 件）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_wish_match_with_priority` が pass + 出力が見える
- [ ] Wishlist の JSON シリアライズ・デシリアライズが動作
- [ ] fs-ntfs → wish-match の単方向依存（逆方向の依存はなし）

## 関連 FR 要件

- **FR-REC-01** (目標優先抽出) ← **基盤完成**
- **FR-WISH-01** (希望リスト管理) ← データ構造完成
- **FR-WISH-02** (パターン突合) ← 基本パターン完成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次のステップ:
   - **Chunk 16**: 高度なマッチング（glob `*`/`**`、日付範囲、論理結合 `And`/`Or`/`Not`）
   - **Chunk 17**: 復旧パイプライン（マッチ結果 → 実ファイル抽出 → 品質判定）

---

## 注意事項

### 業務ロジック層は「正確な要件表現」が中心

NTFS 実装は「書籍の仕様通りに実装」が主眼でしたが、業務層は「お客様の要件を**正確に**表現する」が主眼。テストケースが**業務シナリオを物語る形**で書かれているか確認すること。

例: `matches_files_in_dir1_subdirectory_only` は単なる技術的検証ではなく、「お客様が `D:\dir1` 配下を欲しいと言ったとき、サブディレクトリも全部マッチする」という業務要件の検証。

### 大文字小文字の扱い

NTFS は**大文字小文字を保持するが比較は非区別**。Phase 1 では `eq_ignore_ascii_case` と `to_ascii_lowercase` を使用する。

注意: Unicode の大文字小文字（日本語の全角/半角等）は ASCII の範囲外なので、`eq_ignore_ascii_case` では完全カバーできない。日本語ファイル名は**完全一致でしか動作しない**。これは Phase 1 の制約として明示し、将来チャンクで Unicode 対応を検討。

### `PathPrefix` の境界

`PathPrefix("\\dir1")` は `\\dir1\\file.txt` にマッチしてほしいが `\\dir1other\\foo.txt` にはマッチしてほしくない。実装では末尾に `\` を補ってからプレフィックス比較する設計。**テスト 7（`path_prefix_does_not_match_partial_directory_name`）が境界条件の防衛線**。

### serde シリアライズの目的

`Wishlist` を JSON 化できる理由は、将来の UI (Tauri) から JSON ファイルとして希望リストを受け渡しするため。Phase 1 ではテストでラウンドトリップを確認するだけで OK。実 UI 統合は M4 で。

### MatchResult のライフタイム

`MatchResult<'a>` は `Wishlist<'a>` を借りる設計。これにより同じ Wishlist に対する複数のマッチ結果をメモリ効率良く保持できる。業務統合層から呼ぶ際は **Wishlist を関数の外で保持して、`match_files` を呼ぶ間生かしておく**ことが必要。

### 拡張子の小文字化

`FileInfo::new` で拡張子を自動的に小文字化。これにより `WishItem::Extension("docx")` が `.DOCX` `.Docx` `.docX` 全てにマッチ。NtfsFile::extension() も同じ挙動（Chunk 14 で実装済み）。

### Phase 1 で意図的に除外した機能

- **glob パターン** (`*.docx`, `\**\*.pdf`): Chunk 16 で
- **論理結合** (`And` / `Or` / `Not`): Chunk 16 で
- **正規表現**: Phase 1 範囲外、Phase 2 検討
- **タグ・カテゴリベース**: Phase 2 で
- **AI による意味的マッチ**: Phase 3 で
- **重複検出**: 別チャンクで

これらは指示書では実装しないが、`WishItem` enum の設計が将来拡張に開かれていることを意識する（バリアント追加で実装可能）。

---

## 質問が必要なケース

- 日本語ファイル名の大文字小文字非区別マッチ（ローマ字大小と全角半角を区別するか）
- 同名希望が複数ある場合（同じパターンで違う優先度）の挙動
- マッチ結果の上限（10,000 件超のマッチ時のメモリ管理）

---

## 完了報告例

```markdown
## Chunk 15 完了報告

### Part A: NtfsFile 拡張
- `crates/fs-ntfs/src/file.rs` に `has_system_name_prefix` 追加 (+5 行)
- 単体テスト 3 件追加

### Part B: wish-match クレート本実装
- 新規ファイル:
  - `crates/wish-match/src/lib.rs` (20 行)
  - `crates/wish-match/src/error.rs` (30 行)
  - `crates/wish-match/src/file_info.rs` (70 行 + テスト 30 行)
  - `crates/wish-match/src/wishlist.rs` (100 行 + テスト 40 行)
  - `crates/wish-match/src/matcher.rs` (110 行 + テスト 50 行)
- `crates/fs-ntfs/src/file.rs` に `From<&NtfsFile> for FileInfo` 追加 (+15 行)
- `crates/wish-match/Cargo.toml` 設定
- `crates/fs-ntfs/Cargo.toml` に dds-wish-match 依存追加

### 公開API追加
- `Wishlist`, `Wish`, `WishItem`, `Priority`
- `FileInfo`
- `match_file`, `match_files`, `matches_item`, `matches_wish`, `MatchResult`
- `WishMatchError`
- `From<&NtfsFile> for FileInfo`
- `NtfsFile::has_system_name_prefix`

### テスト統計
- 単体: 既存 133 + wish-match 新規 17 + fs-ntfs 拡張 3 = **153 件 pass**
- 結合: 既存 35 + 新規 4 = **39 件 pass**

### プロダクトデモ出力例
```
=== Wishlist Match Results (Priority-Sorted) ===

   1. [125] NTFS#... (matched: 最深部の重要書類, テキスト全般)  ← file_deeply.txt
   2. [100] NTFS#... (matched: 最深部の重要書類)
   3. [ 75] NTFS#... (matched: ルート直下の root_ プレフィックスファイル)
   ...
```

### 🎉 マイルストーン
- **Phase 1 業務統合層着手**
- お客様の希望リスト → NTFS ファイル抽出が end-to-end で動作
- 削除ファイル + 希望マッチの組合せが実証

- **関連 FR**: FR-REC-01 (基盤), FR-WISH-01, FR-WISH-02

→ tester エージェントへ引き継ぎお願いします
```
