//! Chunk 18: PDF validator。
//!
//! PDF (ISO 32000) のヘッダ `%PDF-1.X` (X=0-7) と末尾 1024 バイト内の `%%EOF` を確認。
//! xref / objects の構造解析は Phase 2 以降に予定。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// PDF ヘッダプレフィックス: `%PDF-1.`（バージョン数値はこの直後 1 バイト）。
const PDF_HEADER_PREFIX: &[u8] = b"%PDF-1.";
/// PDF トレイラ。
const PDF_TRAILER: &[u8] = b"%%EOF";
/// 末尾の何バイトまで `%%EOF` を探すか。仕様では「ファイル末尾近く」だが
/// 改行・linearization 等の余地を含めて 1024 バイト確保。
const TRAILER_SEARCH_TAIL: usize = 1024;
/// PDF 最小サイズ: ヘッダ 9 バイト + EOF 5 バイト ≥ 14 バイト。
const PDF_MIN_BYTES: usize = 14;
/// ヘッダ内のバージョン数値の位置（`%PDF-1.` の直後）。
const PDF_VERSION_OFFSET: usize = 7;

/// PDF ファイルのバリデータ。
pub struct PdfValidator;

impl Validator for PdfValidator {
    fn name(&self) -> &str {
        "pdf_v1"
    }

    fn format(&self) -> &str {
        "PDF"
    }

    fn extensions(&self) -> &[&str] {
        &["pdf"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < PDF_MIN_BYTES {
            return ValidationResult::invalid(
                "PDF",
                self.name(),
                format!("File too small ({} bytes)", content.len()),
            );
        }

        if !content.starts_with(PDF_HEADER_PREFIX) {
            let preview = std::str::from_utf8(&content[..8.min(content.len())])
                .unwrap_or("<binary>")
                .to_string();
            return ValidationResult::invalid(
                "PDF",
                self.name(),
                format!("PDF header missing (got {:?})", preview),
            );
        }

        // バージョン番号: 1.0〜1.7 のみ Phase 1 でサポート。
        let version_byte = content[PDF_VERSION_OFFSET];
        if !(b'0'..=b'7').contains(&version_byte) {
            return ValidationResult::invalid(
                "PDF",
                self.name(),
                format!("Unsupported PDF version: 1.{}", version_byte as char),
            );
        }
        let mut diagnostics = vec![format!("PDF header OK (version 1.{})", version_byte as char)];

        // Trailer: 末尾 N バイト内に %%EOF を探す。
        let tail_start = content.len().saturating_sub(TRAILER_SEARCH_TAIL);
        let tail = &content[tail_start..];
        let trailer_found = tail
            .windows(PDF_TRAILER.len())
            .any(|w| w == PDF_TRAILER);
        if !trailer_found {
            return ValidationResult::invalid(
                "PDF",
                self.name(),
                format!(
                    "%%EOF trailer not found in last {} bytes",
                    TRAILER_SEARCH_TAIL
                ),
            );
        }
        diagnostics.push("%%EOF trailer found".to_string());

        ValidationResult::valid("PDF", self.name(), diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小有効 PDF。`%PDF-1.4` ヘッダ + 単一 obj + xref + trailer + `%%EOF`。
    pub(crate) const VALID_PDF_MINIMAL: &[u8] = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nxref\n0 1\n0000000000 65535 f\ntrailer\n<</Size 1>>\n%%EOF";

    #[test]
    fn validates_minimal_pdf() {
        let result = PdfValidator.validate(VALID_PDF_MINIMAL);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("PDF"));
        assert!(result.diagnostics[0].contains("1.4"));
    }

    #[test]
    fn invalid_when_header_missing() {
        let mut bytes = VALID_PDF_MINIMAL.to_vec();
        bytes[0] = b'X';
        bytes[1] = b'X';
        let result = PdfValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("header"));
    }

    #[test]
    fn invalid_when_eof_missing() {
        // `%%EOF` を含まないバイト列。
        let bytes = b"%PDF-1.4\nsome content without trailer marker";
        let result = PdfValidator.validate(bytes.as_slice());
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("EOF"));
    }

    #[test]
    fn invalid_for_unsupported_version() {
        // 1.9 はサポート外（PDF 仕様は 1.0〜1.7 + 2.0 系。Phase 1 は 1.0-1.7）。
        let mut bytes = VALID_PDF_MINIMAL.to_vec();
        bytes[PDF_VERSION_OFFSET] = b'9';
        let result = PdfValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("version"));
    }

    #[test]
    fn validates_all_supported_versions_1_0_to_1_7() {
        // 業務観測: PDF 1.0〜1.7 全てが Valid 判定される回帰。
        for v in b'0'..=b'7' {
            let mut bytes = VALID_PDF_MINIMAL.to_vec();
            bytes[PDF_VERSION_OFFSET] = v;
            let result = PdfValidator.validate(&bytes);
            assert!(
                result.status.is_valid(),
                "PDF 1.{} should be valid",
                v as char
            );
        }
    }
}
