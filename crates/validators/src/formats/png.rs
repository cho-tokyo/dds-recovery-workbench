//! Chunk 18: PNG validator。
//!
//! PNG (ISO/IEC 15948) のマジック + 必須チャンク（IHDR / IEND）位置チェック。
//! CRC 検証や個別チャンクの内容妥当性は Phase 2 以降の対応。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// PNG signature (8 bytes): \x89 P N G \r \n \x1A \n
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
/// 最初の chunk は IHDR（ASCII）。
const IHDR_CHUNK_TYPE: [u8; 4] = *b"IHDR";
/// 最後の chunk は IEND（ASCII）。
const IEND_CHUNK_TYPE: [u8; 4] = *b"IEND";
/// PNG 最小サイズ: signature(8) + IHDR chunk(25) + IDAT chunk + IEND chunk(12)。
/// 1x1 透明 PNG で実測 67 バイト程度。安全側に最小 45 バイトを敷く。
const PNG_MIN_BYTES: usize = 45;

/// PNG ファイルのバリデータ。
pub struct PngValidator;

impl Validator for PngValidator {
    fn name(&self) -> &str {
        "png_v1"
    }

    fn format(&self) -> &str {
        "PNG"
    }

    fn extensions(&self) -> &[&str] {
        &["png"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < PNG_MIN_BYTES {
            return ValidationResult::invalid(
                "PNG",
                self.name(),
                format!(
                    "File too small ({} bytes, need at least {})",
                    content.len(),
                    PNG_MIN_BYTES
                ),
                "ファイルが小さすぎて PNG として認識できません",
                format!(
                    "{} バイトしかない。MFT クラスタ単位の不整合の可能性、disk-io 層を確認",
                    content.len()
                ),
            );
        }

        if content[0..8] != PNG_SIGNATURE {
            return ValidationResult::invalid(
                "PNG",
                self.name(),
                format!("Magic signature mismatch (got {:02X?})", &content[0..8]),
                "PNG として保存されていますが、PNG ファイルではないようです（別の形式の可能性）",
                "拡張子と中身の不一致。バイト列冒頭から実形式を判定し、正しい拡張子で再復旧推奨",
            );
        }
        let mut diagnostics = vec!["Magic signature OK".to_string()];

        // PNG 構造: signature(8) | length(4) | "IHDR"(4) | data | crc(4) | ...
        // signature 直後の type フィールドは offset 12..16。
        if content[12..16] != IHDR_CHUNK_TYPE {
            return ValidationResult::invalid(
                "PNG",
                self.name(),
                format!("First chunk should be IHDR, got {:02X?}", &content[12..16]),
                "PNG 画像のヘッダー情報が破損しています。表示できない可能性があります",
                "IHDR チャンク欠落。深い構造破損のため再復旧は困難の可能性。サンプル復旧を推奨",
            );
        }
        diagnostics.push("IHDR chunk found at correct position".to_string());

        // IEND chunk: length(4)=0 | "IEND"(4) | crc(4) = 末尾 12 バイト。
        // 末尾 8..4 が "IEND" であるかを確認（length と crc を除いた type 位置）。
        let end = content.len();
        if content[end - 8..end - 4] != IEND_CHUNK_TYPE[..] {
            return ValidationResult::invalid(
                "PNG",
                self.name(),
                "IEND chunk not found at end of file".to_string(),
                "PNG 画像の末尾が欠けています。画像の一部または全体が表示できない可能性があります",
                "末尾チャンク欠損のため部分復旧。可能なら元データから再復旧を試行",
            );
        }
        diagnostics.push("IEND chunk found at end".to_string());

        ValidationResult::valid(
            "PNG",
            self.name(),
            diagnostics,
            "PNG 画像として正常です",
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1x1 透明 PNG（67 バイト）。インターネット上で広く流通する最小有効 PNG。
    pub(crate) const VALID_PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, // bit depth etc + crc
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, // IDAT length + type
        0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, // IDAT data
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, // IDAT data + crc
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, // IEND length + type + crc
        0x42, 0x60, 0x82,
    ];

    #[test]
    fn validates_minimal_valid_png() {
        let result = PngValidator.validate(VALID_PNG_1X1);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("PNG"));
        assert_eq!(result.validator_name, "png_v1");
        assert!(result.diagnostics.len() >= 3);
    }

    #[test]
    fn invalid_when_magic_wrong() {
        let mut bytes = VALID_PNG_1X1.to_vec();
        bytes[0] = 0xFF;
        let result = PngValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("Magic"));
    }

    #[test]
    fn invalid_when_iend_missing() {
        // 末尾 8 バイトを切り詰めて IEND chunk type を失わせる。
        let bytes = &VALID_PNG_1X1[..VALID_PNG_1X1.len() - 8];
        let result = PngValidator.validate(bytes);
        assert!(result.status.is_invalid());
    }

    #[test]
    fn invalid_when_too_small() {
        // signature だけの 4 バイト → 最小サイズ未満で Invalid。
        let result = PngValidator.validate(&[0x89, 0x50, 0x4E, 0x47]);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("too small"));
    }

    #[test]
    fn validates_minimal_valid_png_with_japanese_message() {
        // Chunk 20: Valid 時に user_message_ja が日本語、internal_note_ja が None。
        let result = PngValidator.validate(VALID_PNG_1X1);
        assert!(result.status.is_valid());
        assert_eq!(
            result.user_message_ja.as_deref(),
            Some("PNG 画像として正常です")
        );
        assert!(
            result.internal_note_ja.is_none(),
            "Valid 時は internal_note は None"
        );
    }

    #[test]
    fn invalid_png_includes_actionable_internal_note() {
        // Chunk 20: Invalid 時に internal_note_ja が業務指示を含む（「再復旧」「CS」等）。
        let mut bytes = VALID_PNG_1X1.to_vec();
        bytes[0] = 0xFF;
        let result = PngValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        let note = result.internal_note().expect("Invalid 時は internal_note 必須");
        assert!(
            note.contains("再復旧") || note.contains("CS") || note.contains("確認"),
            "internal_note には業務指示が必要: {}",
            note
        );
        // 顧客向けメッセージは攻撃的でないこと（「破損」などのみで責任追及調にしない）。
        let cust = result.customer_message();
        assert!(cust.contains("PNG"));
    }
}
