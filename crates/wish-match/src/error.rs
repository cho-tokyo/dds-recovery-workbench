//! Chunk 15: wish-match クレートのエラー型。
//!
//! 業務統合層では希望リスト構築時のバリデーション失敗を表現する。
//! NTFS 実装層 (Chunks 4-14) のエラー型と同じく `thiserror` を使い、
//! 構造化メッセージで「何が・なぜ」失敗したかを呼び出し側に伝える。
//! 関連 FR: FR-WISH-01 (希望リスト管理), FR-WISH-02 (パターン突合)。

use thiserror::Error;

/// wish-match クレート全体のエラー型。
///
/// Phase 1 段階では希望リストの構築時バリデーションのみを扱う。
/// マッチ評価自体は失敗しない設計（パターン照合は副作用なし）。
#[derive(Error, Debug, PartialEq, Eq)]
pub enum WishMatchError {
    /// パスパターンが不正（空文字列、無効な形式など）。
    #[error("Invalid path pattern: {pattern} ({reason})")]
    InvalidPathPattern {
        /// 検証対象パターン文字列。
        pattern: String,
        /// 失敗理由。
        reason: String,
    },

    /// サイズ範囲が不正（min > max など）。
    #[error("Invalid size range: min={min:?}, max={max:?} ({reason})")]
    InvalidSizeRange {
        /// 下限（バイト）。
        min: Option<u64>,
        /// 上限（バイト）。
        max: Option<u64>,
        /// 失敗理由。
        reason: String,
    },

    /// 日付範囲が不正（after > before など）。
    #[error("Invalid date range: after={after:?}, before={before:?} ({reason})")]
    InvalidDateRange {
        /// 開始日（RFC3339 文字列）。
        after: Option<String>,
        /// 終了日（RFC3339 文字列）。
        before: Option<String>,
        /// 失敗理由。
        reason: String,
    },

    /// 希望リストが空（最低 1 件の希望が必要）。
    #[error("Empty wishlist (must contain at least one wish)")]
    EmptyWishlist,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_path_pattern_message_contains_pattern_and_reason() {
        let err = WishMatchError::InvalidPathPattern {
            pattern: "".to_string(),
            reason: "empty pattern".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid path pattern"));
        assert!(msg.contains("empty pattern"));
    }

    #[test]
    fn empty_wishlist_error_is_distinct() {
        let a = WishMatchError::EmptyWishlist;
        let b = WishMatchError::EmptyWishlist;
        assert_eq!(a, b);
    }

    #[test]
    fn invalid_size_range_carries_bounds() {
        let err = WishMatchError::InvalidSizeRange {
            min: Some(1000),
            max: Some(100),
            reason: "min > max".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("1000"));
        assert!(msg.contains("min > max"));
    }
}
