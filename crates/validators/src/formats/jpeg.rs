//! Chunk 18: JPEG validator。
//!
//! JPEG (ITU-T T.81 / JFIF) の SOI/EOI マーカーと SOI 直後のマーカープレフィックスを確認。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// SOI marker (Start of Image)。
const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
/// EOI marker (End of Image)。
const JPEG_EOI: [u8; 2] = [0xFF, 0xD9];
/// JPEG 最小サイズ: SOI + 任意マーカー + EOI ≥ 4 bytes。
const JPEG_MIN_BYTES: usize = 4;

/// JPEG ファイルのバリデータ。`.jpg` / `.jpeg` 双方を扱う。
pub struct JpegValidator;

impl Validator for JpegValidator {
    fn name(&self) -> &str {
        "jpeg_v1"
    }

    fn format(&self) -> &str {
        "JPEG"
    }

    fn extensions(&self) -> &[&str] {
        &["jpg", "jpeg"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < JPEG_MIN_BYTES {
            return ValidationResult::invalid(
                "JPEG",
                self.name(),
                format!("File too small ({} bytes)", content.len()),
            );
        }

        // SOI check.
        if content[0..2] != JPEG_SOI {
            return ValidationResult::invalid(
                "JPEG",
                self.name(),
                format!("SOI marker missing (got {:02X?})", &content[0..2]),
            );
        }
        let mut diagnostics = vec!["SOI marker OK".to_string()];

        // EOI check (末尾 2 バイト)。
        let end = content.len();
        if content[end - 2..end] != JPEG_EOI {
            return ValidationResult::invalid(
                "JPEG",
                self.name(),
                format!(
                    "EOI marker missing (got {:02X?} at end)",
                    &content[end - 2..end]
                ),
            );
        }
        diagnostics.push("EOI marker OK at end".to_string());

        // 第 3 バイトが 0xFF（マーカープレフィックス）であることを期待。
        // 一般的に SOI 直後は JFIF=FF E0, EXIF=FF E1, DQT=FF DB 等のマーカー。
        if content[2] != 0xFF {
            return ValidationResult::invalid(
                "JPEG",
                self.name(),
                format!(
                    "Expected marker prefix after SOI (got 0x{:02X})",
                    content[2]
                ),
            );
        }
        diagnostics.push(format!("Marker after SOI: 0xFF 0x{:02X}", content[3]));

        ValidationResult::valid("JPEG", self.name(), diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小有効 JPEG (22 バイト)。SOI + APP0(JFIF) + EOI のみ。
    pub(crate) const VALID_JPEG_MINIMAL: &[u8] = &[
        0xFF, 0xD8, // SOI
        0xFF, 0xE0, // APP0 marker
        0x00, 0x10, // APP0 length = 16
        b'J', b'F', b'I', b'F', 0x00, // "JFIF\0"
        0x01, 0x01, // version 1.01
        0x00, // density units
        0x00, 0x01, 0x00, 0x01, // x/y density = 1, 1
        0x00, 0x00, // thumbnail w/h
        0xFF, 0xD9, // EOI
    ];

    #[test]
    fn validates_minimal_jpeg() {
        let result = JpegValidator.validate(VALID_JPEG_MINIMAL);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("JPEG"));
        assert_eq!(result.validator_name, "jpeg_v1");
    }

    #[test]
    fn invalid_when_soi_missing() {
        let mut bytes = VALID_JPEG_MINIMAL.to_vec();
        bytes[0] = 0x00;
        bytes[1] = 0x00;
        let result = JpegValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("SOI"));
    }

    #[test]
    fn invalid_when_eoi_missing() {
        let mut bytes = VALID_JPEG_MINIMAL.to_vec();
        let n = bytes.len();
        bytes[n - 2] = 0x00;
        bytes[n - 1] = 0x00;
        let result = JpegValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("EOI"));
    }

    #[test]
    fn invalid_when_no_marker_after_soi() {
        // SOI 直後が 0xFF でない（マーカープレフィックス不一致）→ Invalid。
        let bytes = [0xFF, 0xD8, 0x12, 0x34, 0xFF, 0xD9];
        let result = JpegValidator.validate(&bytes);
        assert!(result.status.is_invalid());
    }

    #[test]
    fn supports_both_jpg_and_jpeg_extensions() {
        // Validator が 2 拡張子を返すことの回帰テスト。
        assert_eq!(JpegValidator.extensions(), &["jpg", "jpeg"]);
    }
}
