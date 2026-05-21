//! Chunk 18: 検証結果の 3 値ステータスと結果構造体。
//!
//! 「Valid / Invalid / Uncertain」の保守的設計で、誤って Valid 判定するリスクを下げる。

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

/// 単一ファイルの検証結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// 検証ステータス。
    pub status: ValidationStatus,
    /// 検出された形式名（例: "PNG", "JPEG", "PDF"）。Uncertain なら None あり。
    pub format_detected: Option<String>,
    /// 使用された Validator の識別名（例: "png_v1"）。
    pub validator_name: String,
    /// 診断メッセージ（成功なら ["magic OK", "IHDR found", "IEND found"] 等）。
    pub diagnostics: Vec<String>,
}

impl ValidationResult {
    /// Valid 結果のコンストラクタ。
    pub fn valid(
        format: impl Into<String>,
        validator: impl Into<String>,
        diagnostics: Vec<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Valid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics,
        }
    }

    /// Invalid 結果のコンストラクタ。
    pub fn invalid(
        format: impl Into<String>,
        validator: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Invalid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics: vec![reason.into()],
        }
    }

    /// Uncertain 結果のコンストラクタ。Validator なしの場合などに使う。
    pub fn uncertain(reason: impl Into<String>) -> Self {
        Self {
            status: ValidationStatus::Uncertain,
            format_detected: None,
            validator_name: "none".into(),
            diagnostics: vec![reason.into()],
        }
    }

    /// CS / レポート向けの短い説明文。
    pub fn summary(&self) -> String {
        match self.status {
            ValidationStatus::Valid => format!(
                "[OK] {} as Valid",
                self.format_detected.as_deref().unwrap_or("Unknown")
            ),
            ValidationStatus::Invalid => format!(
                "[NG] Invalid: {}",
                self.diagnostics
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("unknown")
            ),
            ValidationStatus::Uncertain => format!(
                "[?] Uncertain: {}",
                self.diagnostics
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("no validator")
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
        let r = ValidationResult::valid("PNG", "png_v1", vec!["magic OK".into()]);
        assert!(r.status.is_valid());
        assert_eq!(r.format_detected.as_deref(), Some("PNG"));
        assert_eq!(r.validator_name, "png_v1");
        assert_eq!(r.diagnostics.len(), 1);
    }

    #[test]
    fn invalid_constructor_sets_fields() {
        let r = ValidationResult::invalid("JPEG", "jpeg_v1", "SOI missing");
        assert!(r.status.is_invalid());
        assert_eq!(r.format_detected.as_deref(), Some("JPEG"));
        assert_eq!(r.diagnostics[0], "SOI missing");
    }

    #[test]
    fn uncertain_constructor_has_no_format() {
        let r = ValidationResult::uncertain("no validator for .txt");
        assert!(r.status.is_uncertain());
        assert!(r.format_detected.is_none());
        assert_eq!(r.validator_name, "none");
    }

    #[test]
    fn summary_uses_status_specific_prefix() {
        assert!(ValidationResult::valid("PNG", "png_v1", vec!["m".into()])
            .summary()
            .starts_with("[OK]"));
        assert!(ValidationResult::invalid("PNG", "png_v1", "bad")
            .summary()
            .starts_with("[NG]"));
        assert!(ValidationResult::uncertain("none")
            .summary()
            .starts_with("[?]"));
    }
}
