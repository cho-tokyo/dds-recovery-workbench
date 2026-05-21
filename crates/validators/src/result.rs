//! Chunk 18 / Chunk 20: 検証結果の 3 値ステータスと結果構造体。
//!
//! 「Valid / Invalid / Uncertain」の保守的設計で、誤って Valid 判定するリスクを下げる。
//!
//! Chunk 20 で **3 層メッセージ構造**を導入:
//! - `diagnostics`: 英語、開発者向け、デバッグ用（CSV のみに出る）
//! - `user_message_ja`: 日本語、顧客向け（顧客 HTML / CS HTML / CSV すべて）
//! - `internal_note_ja`: 日本語、CS 業務メモ（CS HTML / CSV のみ、**顧客には絶対公開しない**）
//!
//! 関連 FR: FR-QUAL-04 (多言語サポート / 日本語)。

use serde::{Deserialize, Serialize};

/// 検証結果の 3 値ステータス。
///
/// 保守的設計: 曖昧な場合は `Uncertain` を返し、誤って `Valid` 判定するリスクを下げる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// マジック + 基本構造チェック通過。ほぼ確実に開ける。
    Valid,
    /// マジック不一致 or 致命的破損。開けない or 中身が壊れている。
    Invalid,
    /// 判定不能。Validator なし、または部分破損で判定保留。
    Uncertain,
}

impl ValidationStatus {
    /// `Valid` の場合のみ `true`。
    pub fn is_valid(self) -> bool {
        matches!(self, ValidationStatus::Valid)
    }
    /// `Invalid` の場合のみ `true`。
    pub fn is_invalid(self) -> bool {
        matches!(self, ValidationStatus::Invalid)
    }
    /// `Uncertain` の場合のみ `true`。
    pub fn is_uncertain(self) -> bool {
        matches!(self, ValidationStatus::Uncertain)
    }
}

/// 単一ファイルの検証結果（3 層メッセージ構造）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// 検証ステータス。
    pub status: ValidationStatus,
    /// 検出された形式名（例: "PNG", "JPEG", "PDF"）。Uncertain なら None あり。
    pub format_detected: Option<String>,
    /// 使用された Validator の識別名（例: "png_v1"）。
    pub validator_name: String,
    /// 技術詳細（英語、開発者向け、CSV のみ表示）。
    pub diagnostics: Vec<String>,
    /// 顧客向け日本語サマリ（顧客 HTML / CS HTML / CSV に表示）。
    /// `None` の場合 [`customer_message`](ValidationResult::customer_message) はデフォルト文言を返す。
    pub user_message_ja: Option<String>,
    /// CS 内部メモ（CS HTML / CSV のみ表示、**顧客 HTML には絶対載せない**）。
    /// 「次にこうしてください」等の業務判断補助。Invalid / Uncertain では基本 `Some(...)`、Valid では `None`。
    pub internal_note_ja: Option<String>,
}

impl ValidationResult {
    /// Valid 結果のコンストラクタ（3 層メッセージ付き）。
    ///
    /// 正常結果のため `internal_note_ja` は `Option<String>` で受け取り、通常は `None` を渡す。
    pub fn valid(
        format: impl Into<String>,
        validator: impl Into<String>,
        diagnostics: Vec<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: Option<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Valid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics,
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja,
        }
    }

    /// Invalid 結果のコンストラクタ（3 層メッセージ付き）。
    ///
    /// 業務上、Invalid 時は CS への業務指示（「再復旧推奨」等）が必要なため
    /// `internal_note_ja` は必須引数。
    pub fn invalid(
        format: impl Into<String>,
        validator: impl Into<String>,
        diagnostic: impl Into<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: impl Into<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Invalid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics: vec![diagnostic.into()],
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja: Some(internal_note_ja.into()),
        }
    }

    /// Uncertain 結果のコンストラクタ。Validator なしの場合などに使う。
    ///
    /// CS で内容確認が必要なため `internal_note_ja` は必須引数。
    pub fn uncertain(
        reason: impl Into<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: impl Into<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Uncertain,
            format_detected: None,
            validator_name: "none".into(),
            diagnostics: vec![reason.into()],
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja: Some(internal_note_ja.into()),
        }
    }

    /// 顧客向けに公開可能なメッセージ（`user_message_ja` がなければデフォルト）。
    ///
    /// `internal_note_ja` は**絶対に**返さない。顧客 HTML 生成で使用される。
    pub fn customer_message(&self) -> String {
        self.user_message_ja.clone().unwrap_or_else(|| match self.status {
            ValidationStatus::Valid => format!(
                "{}として正常です",
                self.format_detected.as_deref().unwrap_or("ファイル")
            ),
            ValidationStatus::Invalid => "ファイルに問題があります".to_string(),
            ValidationStatus::Uncertain => "自動検証の対象外です".to_string(),
        })
    }

    /// CS 向け内部メモへの参照（顧客には絶対公開しない）。
    pub fn internal_note(&self) -> Option<&str> {
        self.internal_note_ja.as_deref()
    }

    /// CS / レポート向けの短い説明文（英語、デバッグ用）。
    pub fn summary(&self) -> String {
        match self.status {
            ValidationStatus::Valid => format!(
                "[OK] {} as Valid",
                self.format_detected.as_deref().unwrap_or("Unknown")
            ),
            ValidationStatus::Invalid => format!(
                "[NG] Invalid: {}",
                self.diagnostics.first().map(|s| s.as_str()).unwrap_or("unknown")
            ),
            ValidationStatus::Uncertain => format!(
                "[?] Uncertain: {}",
                self.diagnostics.first().map(|s| s.as_str()).unwrap_or("no validator")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_predicates_are_mutually_exclusive() {
        assert!(ValidationStatus::Valid.is_valid());
        assert!(!ValidationStatus::Valid.is_invalid());
        assert!(!ValidationStatus::Valid.is_uncertain());
        assert!(ValidationStatus::Invalid.is_invalid());
        assert!(ValidationStatus::Uncertain.is_uncertain());
    }

    #[test]
    fn valid_constructor_sets_fields() {
        let r = ValidationResult::valid(
            "PNG",
            "png_v1",
            vec!["magic OK".into()],
            "PNG として正常です",
            None,
        );
        assert!(r.status.is_valid());
        assert_eq!(r.format_detected.as_deref(), Some("PNG"));
        assert_eq!(r.validator_name, "png_v1");
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.user_message_ja.as_deref(), Some("PNG として正常です"));
        assert!(r.internal_note_ja.is_none(), "Valid 時の internal_note は None");
    }

    #[test]
    fn invalid_constructor_sets_fields() {
        let r = ValidationResult::invalid(
            "JPEG",
            "jpeg_v1",
            "SOI missing",
            "JPEG として保存されていますが破損しています",
            "ヘッダー破損。再復旧推奨",
        );
        assert!(r.status.is_invalid());
        assert_eq!(r.format_detected.as_deref(), Some("JPEG"));
        assert_eq!(r.diagnostics[0], "SOI missing");
        assert!(r.user_message_ja.is_some());
        assert_eq!(r.internal_note().unwrap(), "ヘッダー破損。再復旧推奨");
    }

    #[test]
    fn uncertain_constructor_has_no_format() {
        let r = ValidationResult::uncertain(
            "no validator for .txt",
            ".txt 形式の自動検証は対応していません",
            ".txt は未実装。CS で確認推奨",
        );
        assert!(r.status.is_uncertain());
        assert!(r.format_detected.is_none());
        assert_eq!(r.validator_name, "none");
        assert!(r.internal_note().unwrap().contains("CS で確認"));
    }

    #[test]
    fn customer_message_returns_user_message_ja() {
        let r = ValidationResult::valid(
            "PNG",
            "png_v1",
            vec![],
            "PNG 画像として正常です",
            None,
        );
        assert_eq!(r.customer_message(), "PNG 画像として正常です");
    }

    #[test]
    fn customer_message_fallback_when_no_user_message() {
        // 構造体直接構築で user_message_ja = None。fallback で format_detected を使った文言。
        let r = ValidationResult {
            status: ValidationStatus::Valid,
            format_detected: Some("PNG".into()),
            validator_name: "png_v1".into(),
            diagnostics: vec![],
            user_message_ja: None,
            internal_note_ja: None,
        };
        assert_eq!(r.customer_message(), "PNGとして正常です");

        let invalid = ValidationResult {
            status: ValidationStatus::Invalid,
            format_detected: None,
            validator_name: "none".into(),
            diagnostics: vec![],
            user_message_ja: None,
            internal_note_ja: None,
        };
        assert_eq!(invalid.customer_message(), "ファイルに問題があります");
    }

    #[test]
    fn internal_note_accessor_returns_none_for_valid() {
        let r = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        assert!(r.internal_note().is_none());
    }

    #[test]
    fn summary_uses_status_specific_prefix() {
        assert!(
            ValidationResult::valid("PNG", "png_v1", vec!["m".into()], "OK", None)
                .summary()
                .starts_with("[OK]")
        );
        assert!(
            ValidationResult::invalid("PNG", "png_v1", "bad", "x", "y")
                .summary()
                .starts_with("[NG]")
        );
        assert!(
            ValidationResult::uncertain("none", "x", "y")
                .summary()
                .starts_with("[?]")
        );
    }
}
