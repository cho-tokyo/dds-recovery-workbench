//! Chunk 15-16: 希望リスト型 `Wishlist` / `Wish` / `WishItem` / `Priority`。
//!
//! お客様が「復旧したいファイル」を表現する基本データ構造。
//! Tauri UI からの JSON 受け渡しを見据え、すべて `serde` 派生でシリアライズ可能。
//!
//! - Chunk 15: 基本パターン（ExactPath / PathPrefix / Extension / FilenameContains / SizeRange）。
//! - Chunk 16: Glob (`PathGlob` / `FilenameGlob`)、日付範囲 (`ModifiedRange` 等)、
//!   論理結合 (`All` / `Any` / `Not`) を追加。お客様の複雑な希望
//!   （「Documents の .docx **かつ** 2024 年以降、**ただし** ゴミ箱は除外」など）を表現可能に。
//!
//! 関連 FR: FR-WISH-01 (希望リスト管理), FR-WISH-02 (パターン突合)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 優先度。お客様が「絶対欲しい」から「あったら嬉しい」まで段階表現。
///
/// 数値は `priority_score` の加算用。同一ファイルが複数希望にマッチした場合の
/// ソートに使う（Chunk 17 復旧パイプラインで優先抽出順を決める）。
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Priority {
    /// 補助 (=25): あったら嬉しい。
    Low = 25,
    /// 通常 (=50): デフォルト。
    #[default]
    Normal = 50,
    /// 重要 (=75): 優先したい。
    High = 75,
    /// 必須 (=100): 案件成立条件、お客様が絶対欲しいと指定。
    Critical = 100,
}

impl Priority {
    /// 優先度の数値スコア（マッチ集計用）。
    pub fn score(self) -> u32 {
        self as u32
    }
}

/// 個別の希望アイテム。1 つの `WishItem` は 1 つのマッチ規則を表現する。
///
/// Chunk 16 で 5 バリアント → 13 バリアントに拡張（既存 5 維持 + 新規 8: Glob 2 / 日付範囲 3 /
/// 論理結合 3）。論理結合 (`All` / `Any` / `Not`) で
/// 任意のネストが可能（例: `All(vec![PathPrefix(..), Not(Box::new(PathPrefix(..)))]`)）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WishItem {
    // === 基本パターン（Chunk 15） ===
    /// 完全一致するパス（大文字小文字非区別）。
    ExactPath(String),
    /// 指定パス配下（プレフィックス一致、大文字小文字非区別）。
    /// 末尾の `\` 有無に関わらずディレクトリ境界で判定する。
    PathPrefix(String),
    /// 拡張子一致（小文字比較、ドットなし）。
    Extension(String),
    /// ファイル名に部分一致する文字列（大文字小文字非区別）。
    FilenameContains(String),
    /// ファイルサイズ範囲（バイト）。`min`/`max` どちらも省略可、両端 inclusive。
    SizeRange {
        /// 下限（バイト）。
        min: Option<u64>,
        /// 上限（バイト）。
        max: Option<u64>,
    },

    // === Chunk 16 新規 ===
    /// Glob パターンでパスマッチ（大文字小文字非区別、`\` と `/` を同等扱い）。
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
    ///
    /// 不正なパターンはマッチ時に `false` を返す（パニックしない寛容な設計）。
    PathGlob(String),

    /// Glob パターンでファイル名マッチ（パスは無視、ファイル名のみ対象）。
    ///
    /// 例: `"invoice_2025-??.xlsx"` で `invoice_2025-Q1.xlsx` にマッチ。
    FilenameGlob(String),

    /// 内容更新日時の範囲指定。`after` 以降かつ `before` 以前にマッチ（両端 inclusive）。
    /// どちらも省略可（片方だけ指定で「以降のみ」「以前のみ」を表現）。
    /// ファイル側の日付が `None` の場合は `false`（範囲条件には該当しない）。
    ModifiedRange {
        /// 下限（この日時以降）。
        after: Option<DateTime<Utc>>,
        /// 上限（この日時以前）。
        before: Option<DateTime<Utc>>,
    },

    /// 作成日時の範囲指定。`ModifiedRange` と同様のセマンティクス。
    CreatedRange {
        /// 下限（この日時以降）。
        after: Option<DateTime<Utc>>,
        /// 上限（この日時以前）。
        before: Option<DateTime<Utc>>,
    },

    /// アクセス日時の範囲指定。`ModifiedRange` と同様のセマンティクス。
    AccessedRange {
        /// 下限（この日時以降）。
        after: Option<DateTime<Utc>>,
        /// 上限（この日時以前）。
        before: Option<DateTime<Utc>>,
    },

    /// 論理 AND: すべての子条件にマッチ。空 `Vec` は vacuous truth で `true` を返す。
    All(Vec<WishItem>),

    /// 論理 OR: いずれかの子条件にマッチ。空 `Vec` は `false` を返す。
    Any(Vec<WishItem>),

    /// 論理 NOT: 子条件にマッチ**しない**ことを要求。
    Not(Box<WishItem>),
}

/// 単一の希望（マッチ規則 + 優先度 + 人間可読ラベル）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Wish {
    /// マッチパターン。
    pub item: WishItem,
    /// 優先度（マッチ集計に使う）。
    pub priority: Priority,
    /// 人間が読むラベル（例: "クライアント A の請求書"）。レポート出力に使用。
    pub label: String,
}

impl Wish {
    /// 通常優先度で `Wish` を生成。`with_priority` でチェーンして変更可能。
    pub fn new(item: WishItem, label: impl Into<String>) -> Self {
        Self {
            item,
            priority: Priority::Normal,
            label: label.into(),
        }
    }

    /// 優先度を変更したコピーを返す（builder pattern）。
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// お客様の希望リスト全体。複数の `Wish` をまとめて保持。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Wishlist {
    /// 含まれる希望（順序は UI 表示順）。
    pub wishes: Vec<Wish>,
}

impl Wishlist {
    /// 空の希望リストを生成。
    pub fn new() -> Self {
        Self::default()
    }
    /// `Wish` を追加した自身を返す（builder pattern）。
    ///
    /// `std::ops::Add::add` とは無関係。希望リストはマッチ集合の構築ステップとして
    /// `Wishlist::new().add(w1).add(w2)` のチェーンを推奨スタイルとする。
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, wish: Wish) -> Self {
        self.wishes.push(wish);
        self
    }
    /// 希望が 1 件も登録されていないか。
    pub fn is_empty(&self) -> bool {
        self.wishes.is_empty()
    }
    /// 登録済み希望の件数。
    pub fn len(&self) -> usize {
        self.wishes.len()
    }

    /// AND 結合の希望を簡便に追加するヘルパー（Chunk 16）。
    ///
    /// 例:
    /// ```ignore
    /// Wishlist::new().add_all(Priority::High, "重要書類", vec![
    ///     WishItem::PathPrefix("\\Documents".into()),
    ///     WishItem::Extension("docx".into()),
    /// ]);
    /// ```
    pub fn add_all(
        self,
        priority: Priority,
        label: impl Into<String>,
        items: Vec<WishItem>,
    ) -> Self {
        self.add(Wish::new(WishItem::All(items), label).with_priority(priority))
    }

    /// OR 結合の希望を簡便に追加するヘルパー（Chunk 16）。
    ///
    /// `add_all` の OR 版。複数のいずれかにマッチすれば良い場合に使う。
    pub fn add_any(
        self,
        priority: Priority,
        label: impl Into<String>,
        items: Vec<WishItem>,
    ) -> Self {
        self.add(Wish::new(WishItem::Any(items), label).with_priority(priority))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wish_can_be_built_with_priority() {
        let w = Wish::new(WishItem::Extension("docx".into()), "Word 文書")
            .with_priority(Priority::Critical);
        assert_eq!(w.priority, Priority::Critical);
        assert_eq!(w.label, "Word 文書");
    }

    #[test]
    fn wishlist_builder_pattern_chains() {
        let wl = Wishlist::new()
            .add(Wish::new(WishItem::Extension("txt".into()), "テキスト"))
            .add(Wish::new(WishItem::Extension("pdf".into()), "PDF"));
        assert_eq!(wl.len(), 2);
        assert!(!wl.is_empty());
    }

    #[test]
    fn wishlist_serializes_to_json() {
        // Tauri UI 連携に必要: 希望リストが JSON でラウンドトリップ可能なこと。
        let wl = Wishlist::new()
            .add(
                Wish::new(WishItem::PathPrefix("\\Users\\Chou".into()), "ユーザフォルダ")
                    .with_priority(Priority::Critical),
            )
            .add(Wish::new(
                WishItem::SizeRange { min: Some(100), max: Some(10_000) },
                "中サイズファイル",
            ));
        let json = serde_json::to_string(&wl).expect("serialize");
        let restored: Wishlist = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, wl);
        assert_eq!(restored.len(), 2);
    }

    #[test]
    fn priority_ordering_correct() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
        assert_eq!(Priority::Critical.score(), 100);
        assert_eq!(Priority::High.score(), 75);
        assert_eq!(Priority::Normal.score(), 50);
        assert_eq!(Priority::Low.score(), 25);
        assert_eq!(Priority::default(), Priority::Normal);
    }
}
