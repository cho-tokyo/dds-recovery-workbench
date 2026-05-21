//! Chunk 19: OOXML (Office Open XML) validators - DOCX / XLSX / PPTX。
//!
//! ISO/IEC 29500 (Office Open XML) のサブセット検証。3 形式とも ZIP コンテナで
//! あり、`[Content_Types].xml` の中に「どの Office アプリのドキュメントか」を
//! 示す content type が記録されている。
//!
//! Phase 1 簡易実装: ZIP コンテナを実際に解凍せず、生バイト列内で
//! `[Content_Types].xml` と format-specific marker（例:
//! `wordprocessingml.document`）を `windows().any()` でスキャン。3 つの条件
//! 全てが揃って初めて Valid。
//!
//! Phase 2 で実 ZIP 解凍 + XML 解析に置き換える余地を残す。
//!
//! 関連 FR: FR-QUAL-01 (品質判定), FR-QUAL-03 (3 値ステータス)。

use crate::formats::zip::validate_zip_structure;
use crate::registry::Validator;
use crate::result::ValidationResult;

/// DOCX content type marker (`wordprocessingml.document`)。
const DOCX_CONTENT_MARKER: &[u8] = b"wordprocessingml.document";
/// XLSX content type marker (`spreadsheetml.sheet`)。
const XLSX_CONTENT_MARKER: &[u8] = b"spreadsheetml.sheet";
/// PPTX content type marker (`presentationml.presentation`)。
const PPTX_CONTENT_MARKER: &[u8] = b"presentationml.presentation";

/// OOXML 必須エントリのファイル名（ZIP 内）。
const CONTENT_TYPES_FILENAME: &[u8] = b"[Content_Types].xml";

/// OOXML 検証の共通ロジック。
///
/// 3 段検証:
/// 1. ZIP として有効か（`validate_zip_structure`）
/// 2. `[Content_Types].xml` の存在（バイト列スキャン）
/// 3. format-specific marker の存在（バイト列スキャン）
///
/// いずれかの段階で失敗したら Invalid を返す。
fn validate_ooxml(
    content: &[u8],
    format: &str,
    validator_name: &str,
    content_marker: &[u8],
) -> ValidationResult {
    // Step 1: ZIP として有効か。
    let zip_diagnostics = match validate_zip_structure(content) {
        Ok(d) => d,
        Err(reason) => {
            return ValidationResult::invalid(
                format,
                validator_name,
                format!("ZIP container invalid: {}", reason),
            );
        }
    };

    // Step 2: [Content_Types].xml の存在。
    let has_content_types = content
        .windows(CONTENT_TYPES_FILENAME.len())
        .any(|w| w == CONTENT_TYPES_FILENAME);
    if !has_content_types {
        return ValidationResult::invalid(
            format,
            validator_name,
            "[Content_Types].xml not found in archive".to_string(),
        );
    }

    // Step 3: フォーマット固有マーカー。
    let has_format_marker = content
        .windows(content_marker.len())
        .any(|w| w == content_marker);
    if !has_format_marker {
        let marker_str = std::str::from_utf8(content_marker).unwrap_or("<binary>");
        return ValidationResult::invalid(
            format,
            validator_name,
            format!("Content type marker not found: {}", marker_str),
        );
    }

    let mut diagnostics = zip_diagnostics;
    diagnostics.push("[Content_Types].xml found".to_string());
    diagnostics.push(format!(
        "Format marker found: {}",
        std::str::from_utf8(content_marker).unwrap_or("?")
    ));

    ValidationResult::valid(format, validator_name, diagnostics)
}

/// DOCX ファイルのバリデータ。
pub struct DocxValidator;

impl Validator for DocxValidator {
    fn name(&self) -> &str {
        "docx_v1"
    }
    fn format(&self) -> &str {
        "DOCX"
    }
    fn extensions(&self) -> &[&str] {
        &["docx"]
    }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "DOCX", self.name(), DOCX_CONTENT_MARKER)
    }
}

/// XLSX ファイルのバリデータ。
pub struct XlsxValidator;

impl Validator for XlsxValidator {
    fn name(&self) -> &str {
        "xlsx_v1"
    }
    fn format(&self) -> &str {
        "XLSX"
    }
    fn extensions(&self) -> &[&str] {
        &["xlsx"]
    }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "XLSX", self.name(), XLSX_CONTENT_MARKER)
    }
}

/// PPTX ファイルのバリデータ。
pub struct PptxValidator;

impl Validator for PptxValidator {
    fn name(&self) -> &str {
        "pptx_v1"
    }
    fn format(&self) -> &str {
        "PPTX"
    }
    fn extensions(&self) -> &[&str] {
        &["pptx"]
    }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "PPTX", self.name(), PPTX_CONTENT_MARKER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::zip::{ZIP_EOCD, ZIP_LOCAL_HEADER};

    /// `[Content_Types].xml` と指定マーカーを含む synthetic OOXML バイト列。
    /// ZIP 構造は最小限（実 ZIP の正確な体裁は Phase 1 では不要）。
    fn make_synthetic_ooxml(format_marker: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZIP_LOCAL_HEADER); // ローカルヘッダ magic
        bytes.extend_from_slice(&[0u8; 26]); // ローカルヘッダ残り
        bytes.extend_from_slice(b"[Content_Types].xml"); // ファイル名
        bytes.extend_from_slice(b"<?xml version=\"1.0\"?><Types>");
        bytes.extend_from_slice(b"<Override ContentType=\"application/vnd.openxmlformats-officedocument.");
        bytes.extend_from_slice(format_marker);
        bytes.extend_from_slice(b".main+xml\"/></Types>");
        // EOCD
        bytes.extend_from_slice(ZIP_EOCD);
        bytes.extend_from_slice(&[0u8; 18]);
        bytes
    }

    #[test]
    fn validates_minimal_docx() {
        let docx = make_synthetic_ooxml(DOCX_CONTENT_MARKER);
        let result = DocxValidator.validate(&docx);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("DOCX"));
        assert_eq!(result.validator_name, "docx_v1");
    }

    #[test]
    fn invalid_docx_when_zip_broken() {
        // ZIP magic を破壊。
        let mut bytes = make_synthetic_ooxml(DOCX_CONTENT_MARKER);
        bytes[0] = 0x00;
        bytes[1] = 0x00;
        let result = DocxValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("ZIP container invalid"));
    }

    #[test]
    fn invalid_docx_when_no_content_types_xml() {
        // ZIP として有効だが [Content_Types].xml を含まない。
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZIP_LOCAL_HEADER);
        bytes.extend_from_slice(&[0u8; 26]);
        bytes.extend_from_slice(b"random_file.bin");
        bytes.extend_from_slice(b"wordprocessingml.document"); // marker 単独はあるが
        bytes.extend_from_slice(ZIP_EOCD);
        bytes.extend_from_slice(&[0u8; 18]);
        let result = DocxValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("[Content_Types].xml not found"));
    }

    #[test]
    fn invalid_docx_when_wrong_format_marker() {
        // XLSX 用ファイルを DOCX validator で検証 → Invalid。
        let xlsx = make_synthetic_ooxml(XLSX_CONTENT_MARKER);
        let result = DocxValidator.validate(&xlsx);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("Content type marker"));
    }

    #[test]
    fn validates_xlsx_independently_from_docx() {
        let xlsx = make_synthetic_ooxml(XLSX_CONTENT_MARKER);
        let result = XlsxValidator.validate(&xlsx);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("XLSX"));
    }

    #[test]
    fn validates_pptx_independently() {
        let pptx = make_synthetic_ooxml(PPTX_CONTENT_MARKER);
        let result = PptxValidator.validate(&pptx);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("PPTX"));
    }
}
