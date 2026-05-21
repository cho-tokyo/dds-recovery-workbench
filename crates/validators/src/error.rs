//! Chunk 18: validators クレートのエラー型。
//!
//! バリデータは原則として「Invalid / Uncertain」を `ValidationResult` で表現するため、
//! このエラー型は限定的なシステムエラーのみを表す。

use thiserror::Error;

/// validators クレート固有のエラー。
///
/// 通常の検証結果は `ValidationResult` で表現するため、このエラーは
/// 限定的なシステム的失敗（例えば登録ルックアップ失敗など）のみを扱う。
#[derive(Error, Debug)]
pub enum ValidatorError {
    /// 検証対象のバッファが、その形式の最小サイズに満たない。
    #[error("Buffer too small for {format}: got {got} bytes, need at least {need}")]
    BufferTooSmall {
        /// 対象形式名（例: "PNG"）。
        format: String,
        /// 実際のサイズ。
        got: usize,
        /// 必要な最小サイズ。
        need: usize,
    },

    /// 指定された拡張子に対応する Validator が登録されていない。
    #[error("No validator registered for extension: {extension:?}")]
    NoValidatorForExtension {
        /// 該当しなかった拡張子。
        extension: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_too_small_message_is_descriptive() {
        let e = ValidatorError::BufferTooSmall {
            format: "PNG".into(),
            got: 10,
            need: 45,
        };
        let msg = format!("{}", e);
        assert!(msg.contains("PNG"));
        assert!(msg.contains("10"));
        assert!(msg.contains("45"));
    }

    #[test]
    fn no_validator_for_extension_message_includes_extension() {
        let e = ValidatorError::NoValidatorForExtension {
            extension: "xyz".into(),
        };
        let msg = format!("{}", e);
        assert!(msg.contains("xyz"));
    }

    #[test]
    fn error_is_debug_and_display() {
        // CS にエラー報告できるよう Debug / Display 両方使えること。
        let e = ValidatorError::BufferTooSmall {
            format: "PDF".into(),
            got: 0,
            need: 14,
        };
        let _ = format!("{:?}", e);
        let _ = format!("{}", e);
    }
}
