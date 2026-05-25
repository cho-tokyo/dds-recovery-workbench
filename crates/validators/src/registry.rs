//! Chunk 18: Validator trait と ValidatorRegistry。
//!
//! 各形式バリデータが共通実装する trait と、拡張子→Validator の
//! ルックアップを担うレジストリ。`Arc<dyn Validator>` 共有所有で
//! 「1 つの Validator を複数拡張子（例: jpg / jpeg）」にマップ可能。

use std::collections::HashMap;
use std::sync::Arc;

use crate::result::ValidationResult;
use crate::uncertain::UncertainReason;

/// 検証実施のサイズ上限 (バイト)。これを超えるファイルは
/// `UncertainReason::TooLargeForValidation` として検証スキップする（Chunk 23.8）。
///
/// 100 MB 暫定値。Phase 2 で業務観測に基づき調整予定。
pub const VALIDATION_SIZE_THRESHOLD: u64 = 100 * 1024 * 1024;

/// ファイル形式バリデータの共通 trait。
///
/// 各実装は特定の 1 形式（PNG, PDF, etc.）に対するチェックを行う。
/// マジックナンバー検証 + 基本構造検証で `ValidationResult` を返す。
pub trait Validator: Send + Sync {
    /// この Validator の識別名（例: `"png_v1"`）。
    fn name(&self) -> &str;

    /// この Validator が扱う形式の表示名（例: `"PNG"`）。
    fn format(&self) -> &str;

    /// この Validator が対応する拡張子リスト（小文字、ドットなし）。
    fn extensions(&self) -> &[&str];

    /// 検証本体。
    ///
    /// 戻り値の status:
    /// - `Valid`: マジック + 基本構造 OK
    /// - `Invalid`: 構造破損明確
    /// - `Uncertain(UncertainReason)`: 判定不能。Chunk 23.8 で理由分類が必須化
    fn validate(&self, content: &[u8]) -> ValidationResult;
}

/// 複数の Validator を保持し、拡張子で適切なものを選ぶレジストリ。
///
/// `Arc<dyn Validator>` を使うことで、1 つの Validator インスタンスを
/// 複数の拡張子キー（`jpg`, `jpeg` 等）にマップできる。
pub struct ValidatorRegistry {
    by_extension: HashMap<String, Arc<dyn Validator>>,
}

impl ValidatorRegistry {
    /// 空の Registry を生成する。
    pub fn new() -> Self {
        Self {
            by_extension: HashMap::new(),
        }
    }

    /// デフォルト Validator 群を登録した Registry を返す。
    ///
    /// Phase 1 でサポートする 9 種:
    /// - 画像系: PNG / JPEG / GIF / BMP
    /// - 文書系: PDF / DOCX / XLSX / PPTX
    /// - アーカイブ系: ZIP
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        // Chunk 18: PNG / JPEG / PDF
        reg.register(Arc::new(crate::formats::png::PngValidator));
        reg.register(Arc::new(crate::formats::jpeg::JpegValidator));
        reg.register(Arc::new(crate::formats::pdf::PdfValidator));
        // Chunk 19: GIF / BMP / ZIP
        reg.register(Arc::new(crate::formats::gif::GifValidator));
        reg.register(Arc::new(crate::formats::bmp::BmpValidator));
        reg.register(Arc::new(crate::formats::zip::ZipValidator));
        // Chunk 19: OOXML (DOCX / XLSX / PPTX)
        reg.register(Arc::new(crate::formats::ooxml::DocxValidator));
        reg.register(Arc::new(crate::formats::ooxml::XlsxValidator));
        reg.register(Arc::new(crate::formats::ooxml::PptxValidator));
        reg
    }

    /// Validator を登録する。`extensions()` が返す全拡張子に対して同じ
    /// インスタンスをマップする（Arc クローンで共有）。
    pub fn register(&mut self, validator: Arc<dyn Validator>) {
        for ext in validator.extensions().iter() {
            self.by_extension
                .insert(ext.to_lowercase(), Arc::clone(&validator));
        }
    }

    /// 登録された拡張子の総数（テスト用メトリクス）。
    pub fn registered_extension_count(&self) -> usize {
        self.by_extension.len()
    }

    /// 拡張子に基づいて適切な Validator で検証する。
    ///
    /// 該当する Validator がない、または拡張子が `None` の場合は
    /// `Uncertain` を返す。
    pub fn validate(&self, content: &[u8], extension: Option<&str>) -> ValidationResult {
        let Some(ext) = extension else {
            return ValidationResult::uncertain(
                UncertainReason::NoValidatorAvailable,
                "No extension provided",
                "拡張子が指定されていないため、自動検証できません",
                "拡張子なしファイル。マジック自動検出は Phase 2 対応予定。CS で内容確認",
            );
        };

        let lower = ext.to_lowercase();
        let Some(validator) = self.by_extension.get(&lower) else {
            return ValidationResult::uncertain(
                UncertainReason::NoValidatorAvailable,
                format!("No validator for extension: .{}", lower),
                format!(".{} 形式の自動検証は現在対応していません", lower),
                format!(
                    ".{} は未実装。CS で実際にファイルを開いて確認推奨。複数件発生する場合は validator 追加検討",
                    lower
                ),
            );
        };

        // Chunk 23.8: サイズ超過チェック。100 MB 超は自動検証スキップ。
        if (content.len() as u64) > VALIDATION_SIZE_THRESHOLD {
            return ValidationResult::uncertain(
                UncertainReason::TooLargeForValidation {
                    size: content.len() as u64,
                    threshold: VALIDATION_SIZE_THRESHOLD,
                },
                format!(
                    "Size {} exceeds threshold {}",
                    content.len(),
                    VALIDATION_SIZE_THRESHOLD
                ),
                format!(
                    "ファイルサイズが大きすぎるため自動検証できません（{} 超）",
                    crate::format_bytes_helper::format_bytes(VALIDATION_SIZE_THRESHOLD)
                ),
                format!(
                    "サイズ {} > {} (100MB)、CS で手動確認推奨",
                    content.len(),
                    VALIDATION_SIZE_THRESHOLD
                ),
            );
        }

        validator.validate(content)
    }
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 各 format テスト用バイト列の最小サンプル。
    // PNG / JPEG / PDF それぞれの formats モジュールで定義した最小サンプル相当。
    const VALID_PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    const VALID_JPEG_MINIMAL: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
    ];

    const VALID_PDF_MINIMAL: &[u8] = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nxref\n0 1\n0000000000 65535 f\ntrailer\n<</Size 1>>\n%%EOF";

    #[test]
    fn registry_with_defaults_validates_chunk18_formats() {
        // 業務観測: 既存の PNG/JPEG/PDF が全て Valid 判定される回帰。
        // Chunk 18 までの動作が Chunk 19 拡張後も維持されることを保証。
        let reg = ValidatorRegistry::with_defaults();
        assert!(reg.validate(VALID_PNG_1X1, Some("png")).status.is_valid());
        assert!(reg
            .validate(VALID_JPEG_MINIMAL, Some("jpeg"))
            .status
            .is_valid());
        assert!(reg
            .validate(VALID_JPEG_MINIMAL, Some("jpg"))
            .status
            .is_valid());
        assert!(reg
            .validate(VALID_PDF_MINIMAL, Some("pdf"))
            .status
            .is_valid());
    }

    #[test]
    fn with_defaults_registers_all_9_validators() {
        // Chunk 19: 9 種 validator が拡張子マップに登録されること。
        // jpg/jpeg = JpegValidator 共有なので拡張子数は 10。
        // 拡張子: png, jpg, jpeg, pdf, gif, bmp, zip, docx, xlsx, pptx
        let reg = ValidatorRegistry::with_defaults();
        assert_eq!(reg.registered_extension_count(), 10);
        for ext in [
            "png", "jpg", "jpeg", "pdf", "gif", "bmp", "zip", "docx", "xlsx", "pptx",
        ] {
            let result = reg.validate(b"", Some(ext));
            // 空バイト列だが、Validator が登録されていれば Uncertain ではなく Invalid を返す。
            // 未登録なら Uncertain になるので、業務的にはこれで判定可能。
            assert!(
                !result.status.is_uncertain(),
                "extension .{} should have a registered validator",
                ext
            );
        }
    }

    #[test]
    fn registry_returns_uncertain_for_unknown_extension() {
        // 業務観測: .xyz など Validator なしの拡張子は Uncertain を返す。
        let reg = ValidatorRegistry::with_defaults();
        let result = reg.validate(b"some bytes", Some("xyz"));
        assert!(result.status.is_uncertain());
        assert!(result.diagnostics[0].contains("xyz"));
    }

    #[test]
    fn registry_returns_uncertain_when_no_extension() {
        // 業務観測: 拡張子なしファイル（NTFS の MFT エントリ等）は Uncertain。
        let reg = ValidatorRegistry::with_defaults();
        let result = reg.validate(b"some bytes", None);
        assert!(result.status.is_uncertain());
    }

    #[test]
    fn uncertain_unknown_extension_includes_internal_action() {
        // Chunk 20: 未知拡張子で internal_note_ja に「CS 確認」等の指示が含まれる。
        let reg = ValidatorRegistry::with_defaults();
        let result = reg.validate(b"some bytes", Some("xyz"));
        assert!(result.status.is_uncertain());
        let cust = result.customer_message();
        assert!(
            cust.contains("xyz"),
            "顧客メッセージは拡張子を含む: {}",
            cust
        );
        let note = result
            .internal_note()
            .expect("Uncertain でも internal_note は必須");
        assert!(
            note.contains("CS") || note.contains("確認"),
            "internal_note は CS 業務指示を含むべき: {}",
            note
        );

        let no_ext = reg.validate(b"bytes", None);
        assert!(no_ext.status.is_uncertain());
        let no_ext_note = no_ext
            .internal_note()
            .expect("None でも internal_note 必須");
        assert!(no_ext_note.contains("CS") || no_ext_note.contains("確認"));
    }

    #[test]
    fn registry_is_case_insensitive_for_extension() {
        // 業務観測: 大文字拡張子（.PNG）でも同じ Validator にマップされる。
        let reg = ValidatorRegistry::with_defaults();
        assert!(reg.validate(VALID_PNG_1X1, Some("PNG")).status.is_valid());
        assert!(reg.validate(VALID_PNG_1X1, Some("Png")).status.is_valid());
    }

    #[test]
    fn registry_returns_no_validator_for_unknown_extension() {
        // Chunk 23.8: 未対応拡張子は UncertainReason::NoValidatorAvailable で分類される。
        let reg = ValidatorRegistry::with_defaults();
        let result = reg.validate(b"some bytes", Some("xyz"));
        assert!(result.status.is_uncertain());
        assert_eq!(
            result.status.uncertain_reason(),
            Some(&UncertainReason::NoValidatorAvailable)
        );

        // 拡張子なしも同じ分類。
        let no_ext = reg.validate(b"x", None);
        assert_eq!(
            no_ext.status.uncertain_reason(),
            Some(&UncertainReason::NoValidatorAvailable)
        );
    }

    #[test]
    fn registry_returns_too_large_for_oversized_content() {
        // Chunk 23.8: VALIDATION_SIZE_THRESHOLD (100 MB) 超は TooLargeForValidation。
        let reg = ValidatorRegistry::with_defaults();
        // 101 MB のダミーデータ。PNG 拡張子だが中身は random byte。
        let oversized = vec![0u8; (VALIDATION_SIZE_THRESHOLD + 1024) as usize];
        let result = reg.validate(&oversized, Some("png"));
        assert!(result.status.is_uncertain());
        match result.status.uncertain_reason() {
            Some(UncertainReason::TooLargeForValidation { size, threshold }) => {
                assert!(*size > *threshold);
                assert_eq!(*threshold, VALIDATION_SIZE_THRESHOLD);
            }
            other => panic!("Expected TooLargeForValidation, got {:?}", other),
        }
    }
}
