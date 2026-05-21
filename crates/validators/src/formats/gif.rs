//! Chunk 19: GIF validator。
//!
//! GIF (W3C GIF89a Specification) のマジック署名 (`GIF87a` / `GIF89a`) と
//! トレーラバイト (`0x3B`) のチェック。LZW 圧縮データやカラーパレットの
//! 詳細解析は Phase 2 以降。
//!
//! 関連 FR: FR-QUAL-01 (品質判定), FR-QUAL-03 (3 値ステータス)。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// GIF87a signature (6 bytes ASCII)。
const GIF87A: &[u8] = b"GIF87a";
/// GIF89a signature (6 bytes ASCII)。
const GIF89A: &[u8] = b"GIF89a";
/// GIF ファイル末尾のトレーラバイト。
const GIF_TRAILER: u8 = 0x3B;
/// GIF 最小サイズ: signature(6) + 論理画面記述子(7) ≥ 13 bytes。
/// 加えて trailer 1 byte を要求するため実質 14 bytes 以上。
const GIF_MIN_BYTES: usize = 14;

/// GIF ファイルのバリデータ。
pub struct GifValidator;

impl Validator for GifValidator {
    fn name(&self) -> &str {
        "gif_v1"
    }

    fn format(&self) -> &str {
        "GIF"
    }

    fn extensions(&self) -> &[&str] {
        &["gif"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < GIF_MIN_BYTES {
            return ValidationResult::invalid(
                "GIF",
                self.name(),
                format!(
                    "File too small ({} bytes, need at least {})",
                    content.len(),
                    GIF_MIN_BYTES
                ),
            );
        }

        let header = &content[0..6];
        let version = if header == GIF87A {
            "GIF87a"
        } else if header == GIF89A {
            "GIF89a"
        } else {
            return ValidationResult::invalid(
                "GIF",
                self.name(),
                format!("GIF signature mismatch: {:02X?}", header),
            );
        };

        // Trailer check（末尾 0x3B）。
        if content.last() != Some(&GIF_TRAILER) {
            return ValidationResult::invalid(
                "GIF",
                self.name(),
                format!(
                    "GIF trailer (0x3B) missing (got 0x{:02X})",
                    content.last().copied().unwrap_or(0)
                ),
            );
        }

        ValidationResult::valid(
            "GIF",
            self.name(),
            vec![
                format!("Signature: {}", version),
                "Trailer (0x3B) found".to_string(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小 GIF89a (43 バイト、fixtures の VALID_GIF と同一)。
    pub(crate) const VALID_GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, // GIF89a
        0x01, 0x00, 0x01, 0x00, // 1x1
        0x80, 0x00, 0x00, // color table info
        0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, // palette
        0x21, 0xF9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, // GCE
        0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, // image desc
        0x02, 0x02, 0x44, 0x01, 0x00, // LZW data
        0x3B, // trailer
    ];

    #[test]
    fn validates_minimal_gif89a() {
        let result = GifValidator.validate(VALID_GIF_1X1);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("GIF"));
        assert_eq!(result.validator_name, "gif_v1");
    }

    #[test]
    fn validates_gif87a_signature() {
        // 同じ構造で magic だけ GIF87a 版に差し替え。
        let mut bytes = VALID_GIF_1X1.to_vec();
        bytes[4] = 0x37; // '9' -> '7'
        let result = GifValidator.validate(&bytes);
        assert!(result.status.is_valid(), "GIF87a should be valid");
        assert!(result.diagnostics[0].contains("GIF87a"));
    }

    #[test]
    fn invalid_when_signature_wrong() {
        let mut bytes = VALID_GIF_1X1.to_vec();
        bytes[0] = 0x00;
        let result = GifValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("signature mismatch"));
    }

    #[test]
    fn invalid_when_trailer_missing() {
        // 末尾 0x3B を 0x00 に変更 → Invalid。
        let mut bytes = VALID_GIF_1X1.to_vec();
        let n = bytes.len();
        bytes[n - 1] = 0x00;
        let result = GifValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("trailer"));
    }
}
