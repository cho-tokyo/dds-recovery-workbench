//! Chunk 18 結合テスト: ValidatorRegistry の dispatch 動作を実バイト列で検証。
//!
//! 拡張子に応じた Validator 選択と、「拡張子が中身を偽る」業務シナリオ
//! （セキュリティ・データ破損検出）の挙動を end-to-end で確認する。

use dds_validators::ValidatorRegistry;

/// 1x1 透明 PNG（67 バイト）。
const PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// 最小有効 JPEG (22 バイト)。
const JPEG_BYTES: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
];

/// 最小有効 PDF。
const PDF_BYTES: &[u8] = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\nxref\n0 1\n0000000000 65535 f\ntrailer\n<</Size 1>>\n%%EOF";

#[test]
fn registry_dispatches_correct_validator_by_extension() {
    // 業務観測: 拡張子ごとに正しい Validator が選ばれ、format_detected が一致する。
    let reg = ValidatorRegistry::with_defaults();

    let png_result = reg.validate(PNG_BYTES, Some("png"));
    assert!(png_result.status.is_valid(), "PNG should be valid");
    assert_eq!(png_result.format_detected.as_deref(), Some("PNG"));
    assert_eq!(png_result.validator_name, "png_v1");

    let jpeg_result = reg.validate(JPEG_BYTES, Some("jpeg"));
    assert!(jpeg_result.status.is_valid(), "JPEG should be valid");
    assert_eq!(jpeg_result.format_detected.as_deref(), Some("JPEG"));
    assert_eq!(jpeg_result.validator_name, "jpeg_v1");

    let jpg_result = reg.validate(JPEG_BYTES, Some("jpg"));
    assert!(jpg_result.status.is_valid(), ".jpg alias should also be valid");
    assert_eq!(jpg_result.validator_name, "jpeg_v1");

    let pdf_result = reg.validate(PDF_BYTES, Some("pdf"));
    assert!(pdf_result.status.is_valid(), "PDF should be valid");
    assert_eq!(pdf_result.format_detected.as_deref(), Some("PDF"));
    assert_eq!(pdf_result.validator_name, "pdf_v1");
}

#[test]
fn validator_detects_extension_content_mismatch() {
    // セキュリティ・データ破損検出: 拡張子が中身を偽るケース。
    //
    // - .png 拡張子だが中身は PDF バイト → PNG validator が選ばれて Invalid 判定
    // - .pdf 拡張子だが中身は PNG バイト → PDF validator が選ばれて Invalid 判定
    //
    // これは「extension が嘘をついている」「ファイルが完全に壊れている」のどちらかを
    // 示唆する重要なシグナルで、復旧結果に対する CS の判断材料になる。
    let reg = ValidatorRegistry::with_defaults();

    let lying_png = reg.validate(PDF_BYTES, Some("png"));
    assert!(
        lying_png.status.is_invalid(),
        "Extension says PNG but bytes are PDF - must be Invalid (got {:?})",
        lying_png
    );
    assert_eq!(lying_png.format_detected.as_deref(), Some("PNG"));

    let lying_pdf = reg.validate(PNG_BYTES, Some("pdf"));
    assert!(
        lying_pdf.status.is_invalid(),
        "Extension says PDF but bytes are PNG - must be Invalid"
    );
    assert_eq!(lying_pdf.format_detected.as_deref(), Some("PDF"));

    let lying_jpeg = reg.validate(PNG_BYTES, Some("jpg"));
    assert!(
        lying_jpeg.status.is_invalid(),
        "Extension says JPG but bytes are PNG - must be Invalid"
    );
}
