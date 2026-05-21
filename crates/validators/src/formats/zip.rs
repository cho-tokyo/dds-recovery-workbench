//! Chunk 19: ZIP validator + 共通 ZIP 構造検査ヘルパ。
//!
//! ZIP (PKWare APPNOTE.TXT) のローカルファイルヘッダ (`PK\x03\x04`) と
//! End of Central Directory (`PK\x05\x06`) の存在チェック。
//!
//! `validate_zip_structure` は OOXML 系 (DOCX/XLSX/PPTX) からも再利用される
//! 共通ヘルパ関数（`pub(crate)` 公開）。
//!
//! 関連 FR: FR-QUAL-01 (品質判定), FR-QUAL-03 (3 値ステータス)。

use crate::registry::Validator;
use crate::result::ValidationResult;

/// ZIP ローカルファイルヘッダのマジック (`PK\x03\x04`)。
pub(crate) const ZIP_LOCAL_HEADER: &[u8] = b"PK\x03\x04";
/// ZIP End of Central Directory のマジック (`PK\x05\x06`)。
pub(crate) const ZIP_EOCD: &[u8] = b"PK\x05\x06";
/// 空 ZIP の最小サイズ（EOCD レコードのみ、22 bytes）。
pub(crate) const ZIP_EMPTY_EOCD_SIZE: usize = 22;
/// EOCD 探索範囲: 22 (EOCD固定部) + 65535 (コメント最大長)。
pub(crate) const EOCD_SEARCH_TAIL: usize = 65557;

/// ZIP コンテナの基本検証。
///
/// 先頭マジック（`PK\x03\x04` または空 ZIP の `PK\x05\x06`）チェックと
/// 末尾 64KB+22B 範囲内の EOCD マーカー検出を行う。
///
/// OOXML 系（DOCX/XLSX/PPTX）の前段検証としても使用される。
///
/// 戻り値:
/// - `Ok(Vec<String>)`: 診断メッセージ（成功時）
/// - `Err(String)`: 失敗理由
pub(crate) fn validate_zip_structure(content: &[u8]) -> Result<Vec<String>, String> {
    if content.len() < ZIP_EMPTY_EOCD_SIZE {
        return Err(format!(
            "File too small ({} bytes, need at least {})",
            content.len(),
            ZIP_EMPTY_EOCD_SIZE
        ));
    }

    let starts_with_local = content.starts_with(ZIP_LOCAL_HEADER);
    let starts_with_eocd = content.starts_with(ZIP_EOCD);

    if !starts_with_local && !starts_with_eocd {
        let preview_end = 4.min(content.len());
        return Err(format!(
            "ZIP magic mismatch: {:02X?}",
            &content[0..preview_end]
        ));
    }

    let mut diagnostics = Vec::new();
    if starts_with_local {
        diagnostics.push("Local file header magic OK".to_string());
    } else {
        diagnostics.push("Empty ZIP (EOCD only)".to_string());
    }

    // EOCD を末尾から探す（コメント最大長を考慮）。
    let tail_start = content.len().saturating_sub(EOCD_SEARCH_TAIL);
    let tail = &content[tail_start..];
    let eocd_found = tail.windows(ZIP_EOCD.len()).any(|w| w == ZIP_EOCD);

    if !eocd_found {
        return Err("EOCD (PK\\x05\\x06) marker not found in last 64KB".to_string());
    }
    diagnostics.push("EOCD marker found".to_string());

    Ok(diagnostics)
}

/// ZIP ファイルのバリデータ。
pub struct ZipValidator;

impl Validator for ZipValidator {
    fn name(&self) -> &str {
        "zip_v1"
    }

    fn format(&self) -> &str {
        "ZIP"
    }

    fn extensions(&self) -> &[&str] {
        &["zip"]
    }

    fn validate(&self, content: &[u8]) -> ValidationResult {
        match validate_zip_structure(content) {
            Ok(diagnostics) => ValidationResult::valid("ZIP", self.name(), diagnostics),
            Err(reason) => ValidationResult::invalid("ZIP", self.name(), reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 標準的な ZIP の最小形（PK\x03\x04 始まり + 中間ダミー + 末尾 EOCD）。
    fn make_minimal_zip_with_local_header() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZIP_LOCAL_HEADER);
        // ダミーローカルヘッダの残り部分 + データ。実 ZIP の体裁はテスト目的では不要。
        bytes.extend_from_slice(&[0u8; 26]);
        bytes.extend_from_slice(b"fake.txt");
        bytes.extend_from_slice(b"DATA");
        // EOCD レコード (22 bytes)
        bytes.extend_from_slice(ZIP_EOCD);
        bytes.extend_from_slice(&[0u8; 18]);
        bytes
    }

    /// 空 ZIP (EOCD のみ、22 bytes)。
    fn make_empty_zip() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZIP_EOCD);
        bytes.extend_from_slice(&[0u8; 18]);
        bytes
    }

    #[test]
    fn validates_minimal_zip_with_local_header_and_eocd() {
        let zip = make_minimal_zip_with_local_header();
        let result = ZipValidator.validate(&zip);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("ZIP"));
        assert_eq!(result.validator_name, "zip_v1");
    }

    #[test]
    fn validates_empty_zip_eocd_only() {
        let zip = make_empty_zip();
        let result = ZipValidator.validate(&zip);
        assert!(result.status.is_valid(), "Empty ZIP should be valid");
        assert!(result.diagnostics.iter().any(|d| d.contains("Empty ZIP")));
    }

    #[test]
    fn invalid_when_no_zip_magic() {
        let bytes = vec![0u8; 100];
        let result = ZipValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("magic mismatch"));
    }

    #[test]
    fn invalid_when_eocd_missing() {
        // PK\x03\x04 始まりだが末尾 EOCD なし。
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ZIP_LOCAL_HEADER);
        bytes.extend_from_slice(&[0u8; 50]); // EOCD 含まないダミー
        let result = ZipValidator.validate(&bytes);
        assert!(result.status.is_invalid());
        assert!(result.diagnostics[0].contains("EOCD"));
    }
}
