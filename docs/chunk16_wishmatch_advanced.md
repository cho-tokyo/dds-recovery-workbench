# Chunk 16 指示: wish-match 高度なマッチング（Glob / 日付範囲 / 論理結合）

Chunk 15 で構築した基本パターンを拡張し、**実業務で本当に必要な希望表現**を可能にします。お客様の「Documents 配下の .docx か .pdf で 2024 年以降、ただしゴミ箱は除く」のような複雑な希望が、wish-match の API として表現可能になります。

> 🎯 完了時点で **wish-match v1.0 が業務本番運用に耐える形に到達**。M3「希望突合エンジン」が 70% 進捗。

---

## 目的

3 つの機能を追加する:

### A. Glob パターン

- `PathGlob("*.docx")` — 拡張子だけのシンプルなパターン
- `PathGlob("\\**\\*.pdf")` — 任意の階層配下のすべて
- `PathGlob("\\dir1\\*\\file_*.txt")` — 1 階層のワイルドカード
- `FilenameGlob("invoice_2025-??.xlsx")` — 文字数指定 (`?`) 含む

### B. 日付範囲（既存の Before/After を整理）

- `ModifiedRange { after, before }` — 内容更新日時の範囲
- `CreatedRange { after, before }` — 作成日時の範囲
- `AccessedRange { after, before }` — アクセス日時の範囲

既存の `ModifiedAfter` / `ModifiedBefore` バリアントは **削除** し、`ModifiedRange` に統合。

### C. 論理結合

- `All(Vec<WishItem>)` — すべてマッチ（AND）
- `Any(Vec<WishItem>)` — いずれかマッチ（OR）
- `Not(Box<WishItem>)` — マッチしない（NOT）

これらで「Documents の .docx **かつ** 2024 年以降、**ただし** ゴミ箱は除外」のような表現が可能に。

## 対象クレート

`crates/wish-match/`（既存ファイル拡張のみ、新規ファイルなし）

## 仕様参照

### ビジネス要件

- **FR-WISH-02**: パターン突合 — 複雑なパス・日付・論理組合せに対応
- **FR-REC-01**: 目標優先抽出 — 「除外」も含む詳細な希望表現

### 既存の参照

- `crates/wish-match/` Chunk 15 実装
- `globset` クレートのドキュメント: https://docs.rs/globset/latest/globset/

## 実装内容

### 1. workspace 依存追加

ワークスペースルート `Cargo.toml` の `[workspace.dependencies]` に追加:

```toml
globset = "0.4"
```

`crates/wish-match/Cargo.toml` の `[dependencies]` に追加:

```toml
globset.workspace = true
```

### 2. `wishlist.rs` の WishItem enum 拡張

既存の `ModifiedAfter` / `ModifiedBefore` バリアントを**削除**して、以下を追加:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WishItem {
    // === 基本パターン（Chunk 15 から維持） ===
    ExactPath(String),
    PathPrefix(String),
    Extension(String),
    FilenameContains(String),
    SizeRange { min: Option<u64>, max: Option<u64> },
    
    // === Chunk 16 新規 ===
    
    /// Glob パターンでパスマッチ（大文字小文字非区別、`\` と `/` を同等扱い）
    /// 
    /// 構文:
    /// - `*` ... 任意の文字列（パス区切りを除く）
    /// - `**` ... 任意の文字列（パス区切り含む、再帰的）
    /// - `?` ... 任意の 1 文字
    /// - `[abc]` ... a/b/c のいずれか
    /// 
    /// 例:
    /// - `"*.docx"` ... ルート直下の .docx
    /// - `"\\**\\*.pdf"` ... 任意の階層下の .pdf すべて
    /// - `"\\Users\\*\\Documents\\*.xlsx"` ... 中間 1 階層が任意
    PathGlob(String),
    
    /// Glob パターンでファイル名マッチ（パスは無視、ファイル名のみ対象）
    /// 
    /// 例: `"invoice_2025-??.xlsx"` で `invoice_2025-Q1.xlsx` にマッチ
    FilenameGlob(String),
    
    /// 内容更新日時の範囲指定。`after` 以降かつ `before` 以前にマッチ。
    /// どちらも省略可（片方だけ指定で「以降のみ」「以前のみ」を表現）。
    ModifiedRange {
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    },
    
    /// 作成日時の範囲指定
    CreatedRange {
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    },
    
    /// アクセス日時の範囲指定
    AccessedRange {
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    },
    
    /// 論理 AND: すべての子条件にマッチ。空 Vec は vacuous truth で true を返す
    All(Vec<WishItem>),
    
    /// 論理 OR: いずれかの子条件にマッチ。空 Vec は false を返す
    Any(Vec<WishItem>),
    
    /// 論理 NOT: 子条件にマッチ**しない**ことを要求
    Not(Box<WishItem>),
}
```

### 3. `matcher.rs` 拡張

既存の `matches_item` を拡張。新規ヘルパー関数 `matches_path_glob` / `matches_filename_glob` を private で追加:

```rust
use globset::{Glob, GlobBuilder};

pub fn matches_item(file: &FileInfo, item: &WishItem) -> bool {
    match item {
        // === Chunk 15 既存 ===
        WishItem::ExactPath(target) => {
            file.path.eq_ignore_ascii_case(target)
        }
        WishItem::PathPrefix(prefix) => {
            // 既存実装維持
            // ...
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
        
        // === Chunk 16 新規 ===
        WishItem::PathGlob(pattern) => {
            matches_path_glob(&file.path, pattern)
        }
        WishItem::FilenameGlob(pattern) => {
            matches_filename_glob(&file.name, pattern)
        }
        WishItem::ModifiedRange { after, before } => {
            matches_date_range(file.modified, *after, *before)
        }
        WishItem::CreatedRange { after, before } => {
            matches_date_range(file.created, *after, *before)
        }
        WishItem::AccessedRange { after, before } => {
            matches_date_range(file.accessed, *after, *before)
        }
        WishItem::All(items) => {
            items.iter().all(|i| matches_item(file, i))
        }
        WishItem::Any(items) => {
            items.iter().any(|i| matches_item(file, i))
        }
        WishItem::Not(item) => {
            !matches_item(file, item)
        }
    }
}

/// 日付範囲マッチング。`field_value` が None なら false（日付なしファイルは範囲条件に該当しない）。
fn matches_date_range(
    field_value: Option<DateTime<Utc>>,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> bool {
    let Some(value) = field_value else { return false };
    after.map(|a| value >= a).unwrap_or(true)
        && before.map(|b| value <= b).unwrap_or(true)
}

/// Glob パターンでパスマッチ（NTFS 形式の `\` を `/` に正規化してから globset を使用）。
fn matches_path_glob(file_path: &str, glob_pattern: &str) -> bool {
    // NTFS の `\` を `/` に正規化（globset は `/` 区切り前提）
    let normalized_path = file_path.replace('\\', "/");
    let normalized_pattern = glob_pattern.replace('\\', "/");
    
    let glob = match GlobBuilder::new(&normalized_pattern)
        .case_insensitive(true)
        .literal_separator(true)  // * がパス区切りを跨がない（** だけが跨ぐ）
        .build()
    {
        Ok(g) => g,
        Err(_) => return false,  // 不正な glob パターンは false（エラー化しない設計）
    };
    
    glob.compile_matcher().is_match(&normalized_path)
}

/// Glob パターンでファイル名マッチ（パス区切りなし、純粋にファイル名のみ対象）。
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
```

### 4. Wishlist の builder 補助メソッド（任意、利便性のため）

```rust
impl Wishlist {
    /// 業務シナリオでよく使う「AND 結合」の希望を簡単に追加
    /// 例: `Wishlist::new().add_all(Priority::High, "重要", vec![
    ///         WishItem::PathPrefix("\\Documents".into()),
    ///         WishItem::Extension("docx".into()),
    ///     ])`
    pub fn add_all(self, priority: Priority, label: impl Into<String>, items: Vec<WishItem>) -> Self {
        self.add(Wish::new(WishItem::All(items), label).with_priority(priority))
    }
    
    /// 同様に「OR 結合」も簡便メソッド
    pub fn add_any(self, priority: Priority, label: impl Into<String>, items: Vec<WishItem>) -> Self {
        self.add(Wish::new(WishItem::Any(items), label).with_priority(priority))
    }
}
```

これは品質向上のための糖衣構文。実装ボリュームが増えるなら省略可。

## 既存テストのマイグレーション

Chunk 15 で書いた以下テストを更新する必要があります:

```rust
// Chunk 15 (削除予定):
fn modified_after_correctly_filters_by_date() {
    let item = WishItem::ModifiedAfter(some_date);
    // ...
}

// Chunk 16 (新しい書き方):
fn modified_range_after_only_filters_correctly() {
    let item = WishItem::ModifiedRange { 
        after: Some(some_date), 
        before: None 
    };
    // ...
}
```

`grep -rn "ModifiedAfter\|ModifiedBefore" crates/` で参照箇所を全部洗い出して、機械的に置換。

**注意**: Chunk 15 のテストがすべて pass している状態を維持。マイグレーション後も既存の業務シナリオは同じ結果を出すこと。

## 単体テスト要件（最低 15 件）

`matcher.rs` 内 `#[cfg(test)] mod tests`:

### Glob パターン

1. **`path_glob_matches_extension_in_root`**: `"*.docx"` が `\report.docx` にマッチ、`\dir\foo.docx` にはマッチしない（literal_separator）
2. **`path_glob_double_star_matches_recursive`**: `"\\**\\*.pdf"` が `\dir1\sub1\foo.pdf` にマッチ
3. **`path_glob_single_star_one_level_only`**: `"\\Users\\*\\Documents"` が `\Users\Chou\Documents` にマッチ、`\Users\Chou\Sub\Documents` にはマッチしない
4. **`path_glob_case_insensitive`**: `"*.DOCX"` が `report.docx` にマッチ
5. **`path_glob_invalid_pattern_returns_false_no_panic`**: `"[unclosed"` で false 返却（パニックしない）
6. **`filename_glob_question_mark_single_char`**: `"invoice_2025-??.xlsx"` が `invoice_2025-Q1.xlsx` にマッチ、`invoice_2025-Q12.xlsx` にはマッチしない
7. **`filename_glob_character_class`**: `"file_[0-9].txt"` が `file_3.txt` にマッチ、`file_a.txt` にはマッチしない

### 日付範囲

8. **`modified_range_inclusive_boundaries`**: `after=2024-01-01, before=2024-12-31` でちょうど境界値にマッチ
9. **`modified_range_after_only_no_upper_bound`**: `after=Some, before=None` で未来のファイルも OK
10. **`modified_range_returns_false_for_no_modified_date`**: `file.modified=None` でマッチしない
11. **`created_range_works_on_creation_date`**: created と modified の区別が正しい
12. **`accessed_range_works_on_access_date`**: 同上

### 論理結合

13. **`all_combinator_requires_every_subcondition`**: 2 つの条件 AND で両方マッチ要求
14. **`all_combinator_empty_vec_returns_true`**: `All(vec![])` は vacuous truth で true
15. **`any_combinator_matches_when_any_matches`**: OR で少なくとも 1 つマッチで true
16. **`any_combinator_empty_vec_returns_false`**: `Any(vec![])` は false
17. **`not_combinator_inverts_match`**: マッチする条件を否定するとマッチしない
18. **`nested_combinators_compose_correctly`**: `All(vec![Or(...), Not(...)])` のネストが正しく動作

### 業務シナリオ

19. **`business_scenario_documents_only_excluding_recycle_bin`**:
   ```
   All(vec![
       PathPrefix("\\Users\\Chou\\Documents"),
       Extension("docx"),
       Not(Box::new(PathPrefix("\\Users\\Chou\\Documents\\$RECYCLE.BIN"))),
   ])
   ```

20. **`serializes_complex_wish_to_json_and_back`**: 論理結合 + glob + 日付範囲の複雑な Wish が JSON ラウンドトリップ成功

## 結合テスト要件（最低 3 件）

既存 `crates/fs-ntfs/tests/wish_match_integration.rs` を拡張:

### 1. **Glob による多階層マッチ**

```rust
#[test]
fn many_files_glob_matches_all_100_files() {
    let img = decompress_fixture("ntfs_directories");
    // ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::PathGlob("\\many\\file_0??.txt".to_string()),
            "many 配下の 3 桁数字ファイル"
        ).with_priority(Priority::High));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    // file_000 〜 file_099 の 100 個マッチ
    assert_eq!(matches.len(), 100);
}
```

### 2. **論理結合: dir1 配下から sub2 を除外**

```rust
#[test]
fn business_scenario_dir1_txt_excluding_sub2() {
    let img = decompress_fixture("ntfs_directories");
    // ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::All(vec![
                WishItem::PathPrefix("\\dir1".to_string()),
                WishItem::Extension("txt".to_string()),
                WishItem::Not(Box::new(
                    WishItem::PathPrefix("\\dir1\\sub1\\sub2".to_string())
                )),
            ]),
            "dir1 配下の .txt (sub2 を除く)"
        ).with_priority(Priority::Critical));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    // \dir1\file_001.txt と \dir1\sub1\file_002.txt の 2 ファイル
    // (\dir1\sub1\sub2\file_deeply.txt は除外される)
    assert_eq!(matches.len(), 2);
    
    let paths: Vec<&str> = matches.iter().map(|m| m.source_id.as_str()).collect();
    println!("Matched: {:?}", paths);
}
```

### 3. **総合デモテスト（複合シナリオ）**

```rust
#[test]
fn product_demo_complex_wish_with_combinators() {
    let img = decompress_fixture("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // お客様の架空シナリオ:
    // - 最重要(Critical): \dir1 配下 OR ファイル名に "root" を含むもの (ただし many/ は除く)
    // - 高(High): file_0?? の glob にマッチ (3 桁数字)
    // - 低(Low): 残りの .txt 全部
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::All(vec![
                WishItem::Any(vec![
                    WishItem::PathPrefix("\\dir1".to_string()),
                    WishItem::FilenameContains("root".to_string()),
                ]),
                WishItem::Not(Box::new(
                    WishItem::PathPrefix("\\many".to_string())
                )),
            ]),
            "重要書類 (dir1 配下 OR root 命名、many は除外)"
        ).with_priority(Priority::Critical))
        .add(Wish::new(
            WishItem::PathGlob("\\many\\file_0??.txt".to_string()),
            "many 配下の 3 桁数字ファイル"
        ).with_priority(Priority::High))
        .add(Wish::new(
            WishItem::Extension("txt".to_string()),
            "テキスト全般"
        ).with_priority(Priority::Low));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    println!("\n=== Complex Wish Match Demo ===\n");
    println!("Wishlist:");
    for w in &wishlist.wishes {
        println!("  {:?}({}): {}", w.priority, w.priority.score(), w.label);
    }
    println!("\nTop 10 matches:");
    for (i, m) in matches.iter().enumerate().take(10) {
        let labels: Vec<&str> = m.matched_wishes.iter().map(|w| w.label.as_str()).collect();
        println!("  {:2}. [{:3}] {} (matched: {})", i + 1, m.priority_score, m.source_id, labels.join(", "));
    }
    println!("\nTotal matches: {}", matches.len());
    
    // 最高スコアは Critical + Low = 125
    // (例: \dir1\file_001.txt は Critical(dir1 配下) + Low(.txt) = 125)
    assert!(matches[0].priority_score >= 125);
}
```

### 4. **日付範囲（既存フィクスチャは特定日付なので軽めのテスト）**

```rust
#[test]
fn modified_range_filters_by_recent_date() {
    let img = decompress_fixture("ntfs_directories");
    // ...
    
    // フィクスチャは 2026 年生成想定なので、2020 年以降全部マッチするはず
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::ModifiedRange {
                after: Some("2020-01-01T00:00:00Z".parse().unwrap()),
                before: None,
            },
            "2020 年以降"
        ).with_priority(Priority::Normal));
    
    let file_infos: Vec<FileInfo> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .map(|f| FileInfo::from(&f))
        .collect();
    
    let matches = match_files(&file_infos, &wishlist);
    
    // 全 109 ファイルが modified を持っている前提（NTFS 仕様）
    assert!(matches.len() >= 100);
}
```

## Cargo.toml 設定（再掲）

ワークスペースルート `Cargo.toml`:
```toml
[workspace.dependencies]
# 既存に追加:
globset = "0.4"
```

`crates/wish-match/Cargo.toml`:
```toml
[dependencies]
# 既存に追加:
globset.workspace = true
```

## 制約

- **行数目安**:
  - `crates/wish-match/src/wishlist.rs` 拡張: +60 行（新バリアント追加・既存削除込み）
  - `crates/wish-match/src/matcher.rs` 拡張: +100 行（新マッチング + helpers）
  - 単体テスト追加: +120 行
- **単体テスト最低 15 件**（既存 17 + 新規 15 = 32 件以上）
- **結合テスト最低 3 件**（既存 4 + 新規 3 = 7 件以上）
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、書き込み API 0 件**
- **既存テスト破壊禁止**（`ModifiedAfter` → `ModifiedRange` マイグレーション後も同じ結果）
- **globset エラー時はパニックせず false 返却**（不正パターンへの寛容な対応）

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-wish-match` が全パス（≥32 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥7 件）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_complex_wish_with_combinators` が pass + 出力が見える
- [ ] 既存 `ModifiedAfter`/`ModifiedBefore` の参照が 0 件（grep で確認）
- [ ] WishItem の JSON ラウンドトリップが論理結合含めて動作
- [ ] `grep -r 'unsafe\|fn write' crates/wish-match/src/` で 0 件

## 関連 FR 要件

- **FR-WISH-02** (パターン突合) ← **拡張完了**
- **FR-REC-01** (目標優先抽出) ← 詳細表現に対応

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 wish-match v1.0 完成、M3「希望突合エンジン」70% 進捗達成**
4. 次のステップ候補:
   - **Chunk 17**: 復旧パイプライン基盤 (`recovery` クレート)
   - **Chunk 18**: 品質判定基盤 (`validators` クレート)
   - または M3 完了として復旧フェーズ M4 着手

---

## 注意事項

### globset の挙動

`globset` クレートの重要な挙動:

- `literal_separator(true)`: `*` がパス区切り (`/`) を**跨がない**。`*` がディレクトリ内のみマッチ、`**` がディレクトリ階層を跨ぐ。**この設定は必須**。
- `case_insensitive(true)`: 大文字小文字非区別マッチ。NTFS の挙動と整合。
- 不正な glob パターンは `Glob::new` がエラー返却。実装では `match` で受けて false にする。

### NTFS パスの `\` 正規化

NTFS パスは `\` 区切りだが、globset は `/` 区切り前提。マッチ前に両方を `/` に正規化する設計。**ユーザがどちらの区切り文字で glob を書いても動く**ようにする。

```rust
// 両方 OK:
PathGlob("\\dir1\\*.txt")  // NTFS 風
PathGlob("/dir1/*.txt")    // Unix 風
```

### 既存テストのマイグレーション

`ModifiedAfter` / `ModifiedBefore` を削除する際、以下の手順:

1. 全プロジェクトで grep して全参照箇所をリストアップ
2. 単体テストを 1 件ずつ `ModifiedRange { after: Some(date), before: None }` 等に書き換え
3. 結合テスト（あれば）も同様に書き換え
4. `cargo build` でコンパイルエラーがゼロになることを確認
5. `cargo test` で全テスト pass を確認

破壊的変更だが Phase 1 開発中なので OK。実 UI 連携前なので影響範囲は限定。

### `All(vec![])` の挙動

数学的 vacuous truth に従い `true` を返す。これは「条件がないなら何でもマッチ」という直感的な挙動と一致。
ただし業務的には**意図しないマッチ拡大**を引き起こす可能性があるので、空 Vec は警告ログを出すか、`Wishlist::validate()` で弾く設計余地を残す（Chunk 17 以降で検討）。

### 性能の懸念

- 各 `matches_item` 呼び出しで `globset::Glob` を新規コンパイル → 100 万ファイル × 10 希望で 1000 万回のコンパイル
- 性能要求が明確化されたら、コンパイル済み glob を Wishlist 内にキャッシュする設計に変更
- **Phase 1 では性能最適化は範囲外**。動くことを優先。

### serde シリアライズの注意

`Box<WishItem>` は serde で問題なく動作。`Vec<WishItem>` も同様。ネストした複雑な Wish も JSON ラウンドトリップ可能。

### 日付なしファイルの扱い

`file.modified == None` の場合、`ModifiedRange` は `false` を返す（マッチしない）。これは「日付情報が破損または欠落しているファイルは日付範囲条件には該当しない」という保守的な挙動。
業務的に「日付不明も含めたい」場合は `Or(ModifiedRange{...}, ...)` で別条件を足す設計とする。

---

## 質問が必要なケース

- `[abc]` や `[!abc]` のような文字クラスの Phase 1 サポート範囲
- 日付なしファイルに対する Range の挙動（false にするか、None ファイルは無視するか）
- 大規模ファイル数（100 万件）での性能要件

---

## 完了報告例

```markdown
## Chunk 16 完了報告

### 拡張内容
- **WishItem**: 5 → 11 バリアント
  - 削除: `ModifiedAfter`, `ModifiedBefore`
  - 追加: `PathGlob`, `FilenameGlob`, `ModifiedRange`, `CreatedRange`, `AccessedRange`, `All`, `Any`, `Not`
- **matcher**: glob マッチング、日付範囲、論理結合の実装追加

### ファイル変更
- `crates/wish-match/src/wishlist.rs`: +55 行（バリアント追加・削除）
- `crates/wish-match/src/matcher.rs`: +95 行（マッチング + ヘルパー）
- 単体テスト追加: +130 行
- `Cargo.toml`: globset 依存追加
- 結合テスト: +3 件追加

### テスト統計
- 単体: 既存 17 + 新規 18 = **35 件 pass**
- 結合: 既存 4 + 新規 3 = **7 件 pass**
- 全 workspace: **240+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0, 書き込み API 0
- ModifiedAfter/Before の残存参照 0 件 (grep 検証済み)

### 業務価値の見える化

#### 複合シナリオデモ出力 (`product_demo_complex_wish_with_combinators`)
```
=== Complex Wish Match Demo ===

Wishlist:
  Critical(100): 重要書類 (dir1 配下 OR root 命名、many は除外)
  High(75):      many 配下の 3 桁数字ファイル
  Low(25):       テキスト全般

Top 10 matches:
   1. [125] NTFS#XX (matched: 重要書類, テキスト全般)  ← \dir1\file_001.txt
   2. [125] NTFS#XX (matched: 重要書類, テキスト全般)  ← \dir1\sub1\file_002.txt
   3. [125] NTFS#XX (matched: 重要書類, テキスト全般)  ← \dir1\sub1\sub2\file_deeply.txt
   4. [125] NTFS#XX (matched: 重要書類, テキスト全般)  ← \file_root_001.txt
   ...
  10. [100] NTFS#XX (matched: many 3 桁数字, テキスト全般)  ← \many\file_000.txt

Total matches: 109
```

### 🎉 マイルストーン達成
- **wish-match v1.0 完成**: お客様の複雑な希望表現が業務本番運用レベルに到達
- **M3「希望突合エンジン」70% 進捗**

- **関連 FR**: FR-WISH-02 (完了), FR-REC-01 (詳細対応)

→ tester エージェントへ引き継ぎお願いします
```
