//! Chunk 19: BMP validator。
//!
//! BMP (Microsoft Bitmap File Format) のマジック署名 (`BM`) と
//! ヘッダのファイルサイズフィールド整合性をチェック。
//! ピクセルデータの詳細解析は Phase 2 以降。
//!
//! 関連 FR: FR-QUAL-01 (品質判定), FR-QUAL-03 (3 値ステータス)。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// BMP signature: `BM` (2 bytes)。
const BMP_SIGNATURE: [u8; 2] = [b'B', b'M'];
/// BMP FILE HEADER の最小サイズ（14 bytes）。
const BMP_HEADER_MIN_SIZE: usize = 14;

/// BMP ファイルのバリデータ。
pub struct BmpValidator;

impl Validator for BmpValidator {
    fn name(&self) -> &str {
        "bmp_v1"
    }

    fn format(&self) -> &str {
        "BMP"
    }

    fn extensions(&self) -> &[&str] {
        &["bmp"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < BMP_HEADER_MIN_SIZE {
            return ValidationResult::invalid(
                "BMP",
                self.name(),
                format!(
                    "File too small ({} bytes, need at least {})",
                    content.len(),
                    BMP_HEADER_MIN_SIZE
                ),
                "ファイルが小さすぎて BMP として認識できません",
                format!("{} バイトしかない。disk-io 層を確認", content.len()),
            );
        }

        if content[0..2] != BMP_SIGNATURE {
            return ValidationResult::invalid(
                "BMP",
                self.name(),
                format!("BMP signature mismatch: {:02X?}", &content[0..2]),
                "BMP として保存されていますが、BMP ファイルではないようです（別の形式の可能性）",
                "拡張子と中身の不一致。実形式を判定して正しい拡張子で再復旧推奨",
            );
        }

        // ヘッダのファイルサイズフィールド (offset 2-5, little-endian u32)。
        // try_into は 4 byte スライス→[u8; 4] への変換で、length 保証済みなので
        // unwrap は安全（content.len() >= 14 のチェック後）。
        let size_bytes: [u8; 4] = content[2..6]
            .try_into()
            .expect("4-byte slice always converts to [u8; 4]");
        let declared_size = u32::from_le_bytes(size_bytes);
        let actual_size = content.len() as u32;

        if declared_size != actual_size {
            return ValidationResult::invalid(
                "BMP",
                self.name(),
                format!(
                    "Size mismatch: header declares {} bytes, actual is {} bytes",
                    declared_size, actual_size
                ),
                "BMP 画像のサイズ情報が一致しません。画像が破損している可能性があります",
                format!(
                    "ヘッダー宣言 {} バイト vs 実 {} バイト。途中切り詰めの可能性、元データから再復旧推奨",
                    declared_size, actual_size
                ),
            );
        }

        ValidationResult::valid(
            "BMP",
            self.name(),
            vec![
                "BM signature OK".to_string(),
                format!("File size matches header: {} bytes", actual_size),
            ],
            "BMP 画像として正常です",
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x2 24bit BMP（70 バイト、fixtures の VALID_BMP と同一）。
    fn make_valid_bmp() -> Vec<u8> {
        let pixels: Vec<u8> = vec![
            0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, // 行 1
            0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, // 行 2
        ];
        let pixel_offset: u32 = 54;
        let file_size: u32 = pixel_offset + pixels.len() as u32;

        let mut bytes = Vec::with_capacity(file_size as usize);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&file_size.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 4]); // reserved
        bytes.extend_from_slice(&pixel_offset.to_le_bytes());
        // DIB header (40 bytes)
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes()); // width
        bytes.extend_from_slice(&2u32.to_le_bytes()); // height
        bytes.extend_from_slice(&1u16.to_le_bytes()); // planes
        bytes.extend_from_slice(&24u16.to_le_bytes()); // bpp
        bytes.extend_from_slice(&[0u8; 24]); // compression, etc.
        bytes.extend_from_slice(&pixels);
        bytes
    }

    #[test]
    fn validates_minimal_bmp() {
        let bmp = make_valid_bmp();
        let result = BmpValidator.validate(&bmp);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("BMP"));
        assert_eq!(result.validator_name, "bmp_v1");
    }

    #[test]
    fn invalid_when_signature_wrong() {
        let mut bmp = make_valid_bmp();
        bmp[0] = b'X';
        let result = BmpValidator.validate(&bmp);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("signature mismatch"));
    }

    #[test]
    fn invalid_when_size_mismatch() {
        // ヘッダのサイズフィールドを実サイズと違う値に書き換え。
        let mut bmp = make_valid_bmp();
        bmp[2] = 0xFF;
        bmp[3] = 0xFF;
        let result = BmpValidator.validate(&bmp);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("Size mismatch"));
    }
}
