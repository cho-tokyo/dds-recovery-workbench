# Chunk 18 指示: validators 品質判定基盤（ファイル形式バリデータ）

このチャンクで **「復旧したファイルが本当に開けるか」** を検証する品質判定エンジンの基盤を構築します。Chunk 17 で復旧された RecoveredEntry に、「PDF として有効」「PNG として有効」等の判定結果を付与します。

> 🎯 完了時点で「復旧 + 品質判定」の自動化パイプラインが動く状態に。M4「復旧+品質」70% 進捗、Phase 1 NTFS-α リリース直前。

---

## 目的

復旧結果の **業務的信頼性** を担保する:

1. **マジックナンバー検証**: 各形式のバイトシグネチャを確認
2. **基本構造検証**: 開始/終端マーカー、必須チャンク等の存在確認
3. **拡張可能な設計**: 新しい形式を Validator trait の実装で簡単に追加
4. **3 つの初期 Validator**: PNG / JPEG / PDF
5. **RecoveryEngine 統合**: オプションで復旧後に自動検証

## 対象クレート

- **主**: `crates/validators/`（Chunk 1 で空スケルトン作成済み、本実装）
- **副**: `crates/recovery/`（RecoveredEntry に validation 結果追加、engine から呼び出し）

## 重要な設計原則

### 「Valid」と「Invalid」と「Uncertain」を区別

Phase 1 のバリデータは **完全な仕様準拠検証ではなく実用判定**:

| 判定 | 意味 |
|---|---|
| **Valid** | マジック + 基本構造チェック通過。**ほぼ確実に開ける** |
| **Invalid** | マジック不一致 or 致命的構造破損。**開けない or データ破損** |
| **Uncertain** | 該当 Validator なし or 部分破損で判定不能。**人間の判断が必要** |

「Valid だが実は微妙に壊れている」場合は Uncertain になる設計が安全。誤って Valid と判定して CS が「復旧成功」とお客様に伝えてしまうリスクを減らす。

## 仕様参照

### ビジネス要件

- **FR-REC-04**: データ整合性 — 復旧結果が技術的に妥当かを検証可能に
- **FR-QUAL-01**: 品質判定 — 復旧ファイルの「実際に使える度」を可視化

### 各形式の参照仕様

- **PNG**: ISO/IEC 15948 / PNG Specification 1.2
  - Magic: `89 50 4E 47 0D 0A 1A 0A` (8 bytes)
  - 必須チャンク: IHDR, IEND
- **JPEG**: ITU-T T.81 / JFIF
  - SOI marker: `FF D8`
  - EOI marker: `FF D9`
- **PDF**: ISO 32000
  - Header: `%PDF-1.X` (X は 0-7)
  - Trailer: `%%EOF`

## 実装内容

### モジュール構成

```
crates/validators/
├── Cargo.toml
└── src/
    ├── lib.rs           ← re-export
    ├── error.rs         ← ValidatorError
    ├── result.rs        ← ValidationStatus, ValidationResult
    ├── registry.rs      ← Validator trait + ValidatorRegistry
    └── formats/
        ├── mod.rs
        ├── png.rs       ← PNG validator
        ├── jpeg.rs      ← JPEG validator
        └── pdf.rs       ← PDF validator
```

### Cargo.toml

```toml
[package]
name = "dds-validators"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror.workspace = true
serde = { workspace = true, features = ["derive"] }
```

`crates/recovery/Cargo.toml` に追加:
```toml
[dependencies]
# 既存に追加:
dds-validators.workspace = true
```

### 1. `error.rs`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidatorError {
    #[error("Buffer too small for {format}: got {got} bytes, need at least {need}")]
    BufferTooSmall { format: String, got: usize, need: usize },
    
    #[error("No validator registered for extension: {extension:?}")]
    NoValidatorForExtension { extension: String },
}
```

注: バリデータは**エラーを返さない設計**が基本（Invalid/Uncertain は ValidationResult で表現）。`ValidatorError` は限定的なシステムエラー用。

### 2. `result.rs`

```rust
use serde::{Deserialize, Serialize};

/// 検証結果の 3 値ステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// マジック + 基本構造チェック通過。ほぼ確実に開ける
    Valid,
    /// マジック不一致 or 致命的破損。開けない or 中身が壊れている
    Invalid,
    /// 判定不能。Validator なし、または部分破損で判定保留
    Uncertain,
}

impl ValidationStatus {
    pub fn is_valid(self) -> bool { matches!(self, ValidationStatus::Valid) }
    pub fn is_invalid(self) -> bool { matches!(self, ValidationStatus::Invalid) }
    pub fn is_uncertain(self) -> bool { matches!(self, ValidationStatus::Uncertain) }
}

/// 単一ファイルの検証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub status: ValidationStatus,
    /// 検出された形式名（例: "PNG", "JPEG", "PDF"）。Uncertain なら None あり。
    pub format_detected: Option<String>,
    /// 使用された Validator の識別名（例: "png_v1"）
    pub validator_name: String,
    /// 診断メッセージ（成功なら ["magic OK", "IHDR found", "IEND found"] 等）
    pub diagnostics: Vec<String>,
}

impl ValidationResult {
    /// Valid 結果のコンストラクタ
    pub fn valid(format: impl Into<String>, validator: impl Into<String>, diagnostics: Vec<String>) -> Self {
        Self {
            status: ValidationStatus::Valid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics,
        }
    }
    
    /// Invalid 結果のコンストラクタ
    pub fn invalid(format: impl Into<String>, validator: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: ValidationStatus::Invalid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics: vec![reason.into()],
        }
    }
    
    /// Uncertain 結果のコンストラクタ
    pub fn uncertain(reason: impl Into<String>) -> Self {
        Self {
            status: ValidationStatus::Uncertain,
            format_detected: None,
            validator_name: "none".into(),
            diagnostics: vec![reason.into()],
        }
    }
    
    /// CS 向けの短い説明文
    pub fn summary(&self) -> String {
        match self.status {
            ValidationStatus::Valid => format!("✓ {} as Valid", self.format_detected.as_deref().unwrap_or("Unknown")),
            ValidationStatus::Invalid => format!("✗ Invalid: {}", self.diagnostics.first().map(|s| s.as_str()).unwrap_or("unknown")),
            ValidationStatus::Uncertain => format!("? Uncertain: {}", self.diagnostics.first().map(|s| s.as_str()).unwrap_or("no validator")),
        }
    }
}
```

### 3. `registry.rs`

```rust
use std::collections::HashMap;
use crate::result::ValidationResult;

/// ファイル形式バリデータの共通 trait。
///
/// 各実装は特定の 1 形式（PNG, PDF, etc.）に対するチェックを行う。
/// マジックナンバー検証 + 基本構造検証で `ValidationStatus` を返す。
pub trait Validator: Send + Sync {
    /// この Validator の識別名（例: "png_v1"）
    fn name(&self) -> &str;
    
    /// この Validator が扱う形式の表示名（例: "PNG"）
    fn format(&self) -> &str;
    
    /// この Validator が対応する拡張子リスト（小文字、ドットなし）
    fn extensions(&self) -> &[&str];
    
    /// 検証本体。
    /// 戻り値の status:
    /// - Valid: マジック + 基本構造 OK
    /// - Invalid: 構造破損明確
    /// - Uncertain: 切り詰めなど判定不能
    fn validate(&self, content: &[u8]) -> ValidationResult;
}

/// 複数の Validator を保持し、拡張子で適切なものを選ぶレジストリ。
pub struct ValidatorRegistry {
    by_extension: HashMap<String, Box<dyn Validator>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self { by_extension: HashMap::new() }
    }
    
    /// デフォルト Validator 群を登録した Registry を返す。
    /// 現在: PNG, JPEG, PDF
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(crate::formats::png::PngValidator));
        reg.register(Box::new(crate::formats::jpeg::JpegValidator));
        reg.register(Box::new(crate::formats::pdf::PdfValidator));
        reg
    }
    
    pub fn register(&mut self, validator: Box<dyn Validator>) {
        for ext in validator.extensions().iter() {
            self.by_extension.insert(ext.to_lowercase(), 
                // 同じ Box を複数 key で参照できないので、毎回新規 Box を作る代わりに
                // Arc を使うか、別アプローチが必要。
                // Phase 1 では「1 Validator は 1 拡張子限定」または Arc に切り替えで OK
                unimplemented!("see notes"));
        }
    }
    
    /// 拡張子に基づいて適切な Validator で検証する。
    /// 該当する Validator がない場合は Uncertain を返す。
    pub fn validate(&self, content: &[u8], extension: Option<&str>) -> ValidationResult {
        let Some(ext) = extension else {
            return ValidationResult::uncertain("No extension provided");
        };
        
        let lower = ext.to_lowercase();
        let Some(validator) = self.by_extension.get(&lower) else {
            return ValidationResult::uncertain(format!("No validator for extension: .{}", lower));
        };
        
        validator.validate(content)
    }
}

impl Default for ValidatorRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
```

**設計メモ**: `Box<dyn Validator>` を複数キーで HashMap に入れたい場合、`Arc<dyn Validator>` への変更が必要。Phase 1 の簡易実装としては、各 Validator が 1 つの拡張子のみ扱うように制約してもよい（PNG は `.png` のみ、JPEG は `.jpg` と `.jpeg` を別 Validator 化）。

実装側で `Arc<dyn Validator>` に切り替えるか、`extensions() -> &[&str]` を 1 つの拡張子で `&str` 返却に簡略化するかは builder の判断に委ねる。

### 4. `formats/png.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const IHDR_CHUNK_TYPE: [u8; 4] = *b"IHDR";
const IEND_CHUNK_TYPE: [u8; 4] = *b"IEND";

pub struct PngValidator;

impl Validator for PngValidator {
    fn name(&self) -> &str { "png_v1" }
    fn format(&self) -> &str { "PNG" }
    fn extensions(&self) -> &[&str] { &["png"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        // 最小サイズ: signature(8) + IHDR chunk(25) + IEND chunk(12) = 45 bytes
        if content.len() < 45 {
            return ValidationResult::invalid("PNG", self.name(), 
                format!("File too small ({} bytes, need at least 45)", content.len()));
        }
        
        // Magic check
        if content[0..8] != PNG_SIGNATURE {
            return ValidationResult::invalid("PNG", self.name(), 
                format!("Magic signature mismatch (got {:02X?})", &content[0..8]));
        }
        
        let mut diagnostics = vec!["Magic signature OK".to_string()];
        
        // IHDR は signature 直後の最初のチャンクであるべき
        // チャンク構造: length(4) + type(4) + data(length) + crc(4)
        // signature の後 8-11 = length, 12-15 = type
        if content[12..16] != IHDR_CHUNK_TYPE {
            return ValidationResult::invalid("PNG", self.name(),
                format!("First chunk should be IHDR, got {:02X?}", &content[12..16]));
        }
        diagnostics.push("IHDR chunk found at correct position".to_string());
        
        // IEND チャンクの存在確認（ファイル末尾近くにあるはず）
        // IEND は length=0 + type="IEND" + crc(4) = 12 bytes
        // 末尾から 12 bytes を確認
        let end = content.len();
        if &content[end - 8..end - 4] != &IEND_CHUNK_TYPE[..] {
            return ValidationResult::invalid("PNG", self.name(),
                "IEND chunk not found at end of file".to_string());
        }
        diagnostics.push("IEND chunk found at end".to_string());
        
        ValidationResult::valid("PNG", self.name(), diagnostics)
    }
}
```

### 5. `formats/jpeg.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];  // Start of Image
const JPEG_EOI: [u8; 2] = [0xFF, 0xD9];  // End of Image

pub struct JpegValidator;

impl Validator for JpegValidator {
    fn name(&self) -> &str { "jpeg_v1" }
    fn format(&self) -> &str { "JPEG" }
    fn extensions(&self) -> &[&str] { &["jpg", "jpeg"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        // 最小サイズ: SOI(2) + 任意マーカー + EOI(2) ≥ 4 bytes
        if content.len() < 4 {
            return ValidationResult::invalid("JPEG", self.name(),
                format!("File too small ({} bytes)", content.len()));
        }
        
        // SOI check
        if content[0..2] != JPEG_SOI {
            return ValidationResult::invalid("JPEG", self.name(),
                format!("SOI marker missing (got {:02X?})", &content[0..2]));
        }
        let mut diagnostics = vec!["SOI marker OK".to_string()];
        
        // EOI check (末尾 2 バイト)
        let end = content.len();
        if content[end - 2..end] != JPEG_EOI {
            return ValidationResult::invalid("JPEG", self.name(),
                format!("EOI marker missing (got {:02X?} at end)", &content[end - 2..end]));
        }
        diagnostics.push("EOI marker OK at end".to_string());
        
        // 第 3 バイトが 0xFF（マーカープレフィックス）であることを期待
        // JFIF: FF E0, EXIF: FF E1, etc.
        if content[2] != 0xFF {
            return ValidationResult::invalid("JPEG", self.name(),
                format!("Expected marker prefix after SOI (got 0x{:02X})", content[2]));
        }
        diagnostics.push(format!("Marker after SOI: 0xFF 0x{:02X}", content[3]));
        
        ValidationResult::valid("JPEG", self.name(), diagnostics)
    }
}
```

### 6. `formats/pdf.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

const PDF_HEADER_PREFIX: &[u8] = b"%PDF-1.";
const PDF_TRAILER: &[u8] = b"%%EOF";
const TRAILER_SEARCH_TAIL: usize = 1024;  // 末尾の何バイトまで EOF を探すか

pub struct PdfValidator;

impl Validator for PdfValidator {
    fn name(&self) -> &str { "pdf_v1" }
    fn format(&self) -> &str { "PDF" }
    fn extensions(&self) -> &[&str] { &["pdf"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        // 最小サイズ: "%PDF-1.X\n" (9 bytes) + "%%EOF" (5 bytes) = 14 bytes 程度
        if content.len() < 14 {
            return ValidationResult::invalid("PDF", self.name(),
                format!("File too small ({} bytes)", content.len()));
        }
        
        // Header check: %PDF-1.X
        if !content.starts_with(PDF_HEADER_PREFIX) {
            return ValidationResult::invalid("PDF", self.name(),
                format!("PDF header missing (got {:?})", 
                    std::str::from_utf8(&content[..8.min(content.len())]).unwrap_or("<binary>")));
        }
        
        // バージョン番号取得（1.0-1.7 が有効）
        let version_byte = content[7];
        if !(b'0'..=b'7').contains(&version_byte) {
            return ValidationResult::invalid("PDF", self.name(),
                format!("Unsupported PDF version: 1.{}", version_byte as char));
        }
        let mut diagnostics = vec![format!("PDF header OK (version 1.{})", version_byte as char)];
        
        // Trailer check: 末尾 N バイト内に %%EOF を探す
        let tail_start = content.len().saturating_sub(TRAILER_SEARCH_TAIL);
        let tail = &content[tail_start..];
        
        let trailer_found = tail.windows(PDF_TRAILER.len())
            .any(|w| w == PDF_TRAILER);
        
        if !trailer_found {
            return ValidationResult::invalid("PDF", self.name(),
                format!("%%EOF trailer not found in last {} bytes", TRAILER_SEARCH_TAIL));
        }
        diagnostics.push("%%EOF trailer found".to_string());
        
        ValidationResult::valid("PDF", self.name(), diagnostics)
    }
}
```

### 7. `formats/mod.rs`

```rust
pub mod jpeg;
pub mod pdf;
pub mod png;
```

### 8. `lib.rs`

```rust
//! ファイル形式バリデータ。
//!
//! 復旧されたファイルの「マジックナンバー + 基本構造」を検証する。
//! Phase 1 は PNG / JPEG / PDF をサポート。

pub mod error;
pub mod formats;
pub mod registry;
pub mod result;

pub use error::ValidatorError;
pub use registry::{Validator, ValidatorRegistry};
pub use result::{ValidationResult, ValidationStatus};
```

### 9. recovery クレートとの統合

`crates/recovery/src/options.rs` に追加:

```rust
pub struct RecoveryOptions {
    // ... 既存フィールド ...
    
    /// 復旧後に validator で検証するか
    pub validate_after_recovery: bool,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            // ... 既存 ...
            validate_after_recovery: true,  // デフォルト有効
        }
    }
}
```

`crates/recovery/src/report.rs` の `RecoveredEntry` に追加:

```rust
use dds_validators::ValidationResult;

#[derive(Debug, Clone)]
pub struct RecoveredEntry {
    // ... 既存フィールド ...
    
    /// 復旧後の検証結果（RecoveryOptions::validate_after_recovery が true の時のみ Some）
    pub validation: Option<ValidationResult>,
}

impl RecoveryReport {
    /// 検証で Valid 判定されたファイル数
    pub fn validated_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_valid()).unwrap_or(false))
            .count()
    }
    
    /// 検証で Invalid 判定されたファイル数
    pub fn invalid_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_invalid()).unwrap_or(false))
            .count()
    }
}
```

`crates/recovery/src/engine.rs` の `recover_one` 内、書き込み後に追加:

```rust
fn recover_one<F>(&self, /* ... */) -> Result<SingleOutcome, RecoveryError>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // ... 既存の書き込み処理 ...
    fs::write(&final_path, &content)?;
    
    // SHA256 計算（既存）
    let sha256 = if self.options.compute_sha256 {
        Some(sha256_hex(&content))
    } else {
        None
    };
    
    // 検証（新規）
    let validation = if self.options.validate_after_recovery {
        let registry = dds_validators::ValidatorRegistry::with_defaults();
        Some(registry.validate(&content, ntfs_file.extension().as_deref()))
    } else {
        None
    };
    
    Ok(SingleOutcome::Recovered(RecoveredEntry {
        // ... 既存フィールド ...
        validation,
    }))
}
```

## 単体テスト要件（最低 12 件）

各形式の Validator にテスト用バイト列をハードコードで含める:

### PNG (`formats/png.rs` 内)

```rust
// 1x1 透明 PNG (67 バイト) - 動作確認済みの実バイト列
const VALID_PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 
    0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, // IEND
    0x42, 0x60, 0x82,
];

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn validates_minimal_valid_png() {
        let result = PngValidator.validate(VALID_PNG_1X1);
        assert!(result.status.is_valid(), "Result: {:?}", result);
        assert_eq!(result.format_detected.as_deref(), Some("PNG"));
    }
    
    #[test]
    fn invalid_when_magic_wrong() {
        let mut bytes = VALID_PNG_1X1.to_vec();
        bytes[0] = 0xFF;
        let result = PngValidator.validate(&bytes);
        assert!(result.status.is_invalid());
    }
    
    #[test]
    fn invalid_when_iend_missing() {
        let bytes = &VALID_PNG_1X1[..VALID_PNG_1X1.len() - 8];
        let result = PngValidator.validate(bytes);
        assert!(result.status.is_invalid());
    }
    
    #[test]
    fn invalid_when_too_small() {
        let result = PngValidator.validate(&[0x89, 0x50, 0x4E, 0x47]);
        assert!(result.status.is_invalid());
    }
}
```

### JPEG (`formats/jpeg.rs` 内)

```rust
// 最小有効 JPEG (SOI + APP0 + EOI、JFIF コンテナ最小形)
const VALID_JPEG_MINIMAL: &[u8] = &[
    0xFF, 0xD8,                                     // SOI
    0xFF, 0xE0,                                     // APP0 marker
    0x00, 0x10,                                     // APP0 length = 16
    b'J', b'F', b'I', b'F', 0x00,                  // "JFIF\0"
    0x01, 0x01,                                     // version 1.01
    0x00,                                           // density units
    0x00, 0x01, 0x00, 0x01,                        // x/y density
    0x00, 0x00,                                     // thumbnail w/h
    0xFF, 0xD9,                                     // EOI
];

#[cfg(test)]
mod tests {
    #[test]
    fn validates_minimal_jpeg() {
        // VALID_JPEG_MINIMAL で valid
    }
    
    #[test]
    fn invalid_when_soi_missing() {
        // 先頭 2 バイトを変えて invalid
    }
    
    #[test]
    fn invalid_when_eoi_missing() {
        // 末尾 2 バイトを変えて invalid
    }
}
```

### PDF (`formats/pdf.rs` 内)

```rust
// 最小有効 PDF (28 バイト) - 構造的最小サンプル
const VALID_PDF_MINIMAL: &[u8] = b"%PDF-1.4\n\
1 0 obj\n<<>>\nendobj\n\
xref\n0 1\n0000000000 65535 f\n\
trailer\n<</Size 1>>\n\
%%EOF";

#[cfg(test)]
mod tests {
    #[test]
    fn validates_minimal_pdf() {
        // %%EOF と %PDF-1.4 の両方が見つかる
    }
    
    #[test]
    fn invalid_when_header_missing() {
        // %PDF を %xxx に変えて invalid
    }
    
    #[test]
    fn invalid_when_eof_missing() {
        // %%EOF を削って invalid
    }
    
    #[test]
    fn invalid_for_unsupported_version() {
        // %PDF-1.9 など範囲外
    }
}
```

### Registry (`registry.rs` 内)

```rust
#[test]
fn registry_with_defaults_has_three_formats() {
    let reg = ValidatorRegistry::with_defaults();
    assert!(reg.validate(VALID_PNG_1X1, Some("png")).status.is_valid());
    assert!(reg.validate(VALID_JPEG_MINIMAL, Some("jpeg")).status.is_valid());
    assert!(reg.validate(VALID_PDF_MINIMAL, Some("pdf")).status.is_valid());
}

#[test]
fn registry_returns_uncertain_for_unknown_extension() {
    let reg = ValidatorRegistry::with_defaults();
    let result = reg.validate(b"some bytes", Some("xyz"));
    assert!(result.status.is_uncertain());
}

#[test]
fn registry_returns_uncertain_when_no_extension() {
    let reg = ValidatorRegistry::with_defaults();
    let result = reg.validate(b"some bytes", None);
    assert!(result.status.is_uncertain());
}
```

## 結合テスト要件（最低 3 件）

`crates/recovery/tests/recovery_validation_integration.rs` を作成。**ただし**、現在のフィクスチャ (`ntfs_directories` 等) は `.txt` ファイルが中心で PNG/JPEG/PDF がない。

### Option 1: Validator のスタンドアロン結合テスト

`crates/validators/tests/validators_integration.rs`:

```rust
use dds_validators::*;

#[test]
fn registry_dispatches_correct_validator_by_extension() {
    let reg = ValidatorRegistry::with_defaults();
    
    // PNG bytes with .png extension
    let png_result = reg.validate(/* PNG bytes */, Some("png"));
    assert!(png_result.status.is_valid());
    assert_eq!(png_result.format_detected.as_deref(), Some("PNG"));
}

#[test]
fn validator_detects_extension_content_mismatch() {
    let reg = ValidatorRegistry::with_defaults();
    
    // PDF bytes claimed to be PNG (extension lies)
    let mismatch = reg.validate(/* PDF bytes */, Some("png"));
    assert!(mismatch.status.is_invalid(), 
        "Extension claims PNG but bytes are PDF - should be Invalid");
}
```

### Option 2: 新フィクスチャ追加（推奨）

`fixtures/scripts/gen_ntfs_mixed_formats.py` を新規作成（既存スクリプトをベースに）。生成内容:
- `image_001.png` (1x1 PNG、ハードコード) × 3 ファイル
- `photo_001.jpg` (最小 JPEG) × 3 ファイル  
- `report_001.pdf` (最小 PDF) × 2 ファイル
- `corrupt_001.png` (PNG ヘッダだけ正しいが IEND なし) × 1
- `garbage_001.pdf` (拡張子は .pdf だが中身はランダム) × 1
- `unknown_001.xyz` (未知拡張子) × 1

合計 11 ファイル、ground truth に各形式の expected validation 結果を記録。

ただし、新フィクスチャ作成は Chunk のスコープを膨らませる。**Phase 1 では Option 1（Validator スタンドアロン）で割り切り、新フィクスチャは Chunk 19 で扱う**ことを推奨。

### Option 3: 復旧パイプライン統合のシンプルテスト

```rust
#[test]
fn recovery_with_validation_marks_txt_as_uncertain() {
    // ntfs_directories は .txt ファイルなので、Validator なし
    // 検証結果は全 Uncertain になるはず
    let img = decompress_fixture("ntfs_directories");
    // ... volume open ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".into()), "全 txt"));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    // 全部 Uncertain（.txt 用 Validator なし）
    let uncertain_count = report.recovered.iter()
        .filter(|e| e.validation.as_ref().map(|v| v.status.is_uncertain()).unwrap_or(false))
        .count();
    assert!(uncertain_count >= 100);
    assert_eq!(report.validated_count(), 0);  // Valid 0 件
    assert_eq!(report.invalid_count(), 0);    // Invalid 0 件
}

#[test]
fn product_demo_recovery_with_validation() {
    // Chunk 17 の product_demo を validation 結果も表示する形に拡張
    // .txt は全 Uncertain だが、レポート出力ロジックは確認できる
    println!("\n=== Recovery + Validation Demo ===");
    println!("Recovered: {}", report.recovered.len());
    println!("  Valid:     {}", report.validated_count());
    println!("  Invalid:   {}", report.invalid_count());
    println!("  Uncertain: {} (no validator for .txt)", uncertain_count);
}
```

将来 PNG/JPEG/PDF フィクスチャができたら、より意味のあるテストが書けます（Chunk 19 で対応）。

## 制約

- **行数目安**:
  - validators クレート全体: ~500 行（実装 + テスト）
  - recovery クレート追加分: ~30 行
- **単体テスト最低 12 件**
- **結合テスト最低 3 件**（Validator standalone + recovery 統合）
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **依存方向**: recovery → validators の単方向

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-validators` が全パス（≥12 件）
- [ ] `cargo test -p dds-recovery` が全パス（既存 + 新規結合 ≥3 件）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_recovery_with_validation` が pass + 出力が見える
- [ ] `grep -r 'unsafe' crates/validators/src/` で 0 件

## 関連 FR 要件

- **FR-REC-04** (データ整合性) ← 検証で実証
- **FR-QUAL-01** (品質判定) ← 基盤完成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 M4「復旧+品質」70% 進捗達成**
4. 次のステップ候補:
   - **Chunk 19**: validators 拡充 (DOCX/XLSX/PPTX/ZIP/GIF/BMP 等) + 混在フィクスチャ生成
   - **Chunk 20**: 復旧レポート生成 (PDF/Excel/HTML)
   - **Chunk 21**: Tauri UI 着手 (Phase 1 最終)

---

## 注意事項

### Validator は「保守的」が原則

「Valid」を返すなら**ほぼ確実に開ける**ことを保証する。曖昧な場合は **Uncertain** を返す方が安全。誤って Valid 判定すると CS の信頼を失うリスク。

例:
- PNG の IEND が末尾にない → Invalid（致命的）
- PNG の IHDR がない → Invalid
- PNG だが IHDR と IEND の間に未知チャンクが多数 → **Valid のまま**（Phase 1 では深追いしない）
- PDF の xref テーブルが破損 → **Uncertain**（中身は読めるかも、判定保留）

### Box<dyn Validator> の HashMap 配置

`registry.rs` の `register` メソッドで、複数拡張子（`.jpg`, `.jpeg` 等）を同じ Validator にマップする必要があるが、`Box<dyn Validator>` は所有権が 1 つしか取れない。

**解決策**:
- **A**: `Arc<dyn Validator>` に変更（共有所有）
- **B**: 各拡張子に**別の Validator インスタンス**を登録（同じ struct を 2 回 new）
- **C**: Validator を ZST (Zero-Sized Type) として作り、`fn(...) -> ValidationResult` のような関数ポインタを保持

Phase 1 では **A（Arc）が最も素直**。実装上は `Arc<dyn Validator>` に変更し、`HashMap<String, Arc<dyn Validator>>` にする。

### 拡張子と中身の不一致検出

ユーザが `.pdf` という拡張子で実は壊れた DOCX を渡された場合:
1. PdfValidator が `%PDF-` ヘッダで Invalid を返す
2. レポートに「PDF validator on .pdf file → Invalid: header mismatch」と記録

これは復旧結果に対して重要なシグナル。「extension が嘘をついている」「ファイルが完全に壊れている」のどちらかを示唆。

### Phase 1 で意図的に除外した機能

- **DOCX/XLSX/PPTX**: ZIP ベースの OOXML、Chunk 19+ で。圧縮解凍が必要
- **GIF/BMP/WebP/TIFF**: Chunk 19+ で
- **MP4/MOV/AVI**: 動画系、Phase 2 で
- **ZIP/RAR/7Z**: アーカイブ系、Phase 2 で
- **詳細な構造検証**: PDF の xref 解析、PNG の CRC 検証 → Phase 2
- **マジック自動検出（拡張子なし）**: Phase 2 で

### Wikipedia / Public Domain のテスト用バイト列

PNG/JPEG/PDF の最小バイト列は仕様書から導出可能で、著作権の心配なし。テストで使うのは「決定論的に有効と判定される最小サンプル」のみ。

### 性能の懸念

- 大ファイル（100MB の PDF）でも、Phase 1 のチェックは「先頭 8 バイト + 末尾 1024 バイト」程度なので**ほぼ瞬時**
- バイナリ全走査を必要とする検証（CRC 計算、構造完全解析）は Phase 2 で

### serde シリアライズ

`ValidationResult` と `ValidationStatus` は serde 派生。これにより RecoveryReport を JSON 化したとき、検証結果も含めて出力可能。Tauri UI から確認・表示できる素地。

---

## 質問が必要なケース

- Office 系（DOCX/XLSX）バリデータの優先度（業務的にどの形式が最重要か）
- 拡張子なしファイルへの対応（マジック自動検出の優先度）
- ZIP ベース形式の検証深さ（圧縮内容の検証まで含めるか）

---

## 完了報告例

```markdown
## Chunk 18 完了報告

### 新規ファイル
- `crates/validators/src/lib.rs`         (30 行)
- `crates/validators/src/error.rs`        (25 行)
- `crates/validators/src/result.rs`       (75 行 + テスト 20 行)
- `crates/validators/src/registry.rs`     (90 行 + テスト 30 行)
- `crates/validators/src/formats/mod.rs`  (5 行)
- `crates/validators/src/formats/png.rs`  (70 行 + テスト 40 行)
- `crates/validators/src/formats/jpeg.rs` (60 行 + テスト 35 行)
- `crates/validators/src/formats/pdf.rs`  (75 行 + テスト 40 行)
- `crates/validators/Cargo.toml`
- `crates/validators/tests/validators_integration.rs` (60 行)
- `crates/recovery/tests/recovery_validation_integration.rs` (80 行)

### 既存ファイル更新
- `crates/recovery/src/options.rs`: validate_after_recovery 追加
- `crates/recovery/src/report.rs`: validation フィールド + validated_count/invalid_count
- `crates/recovery/src/engine.rs`: 復旧後検証ロジック追加
- `crates/recovery/Cargo.toml`: dds-validators 依存追加

### 公開API
- `Validator` trait
- `ValidatorRegistry` (new / with_defaults / register / validate)
- `ValidationResult`, `ValidationStatus` (Valid / Invalid / Uncertain)
- `PngValidator`, `JpegValidator`, `PdfValidator`
- `RecoveredEntry.validation: Option<ValidationResult>`
- `RecoveryReport.validated_count()`, `invalid_count()`

### テスト統計
- 単体: 既存 255 + 新規 14 = **269 件 pass**
- 結合: 既存 42 + 新規 3 = **45 件 pass**
- 全 workspace: **314+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- recovery → validators の単方向依存維持

### 業務価値の見える化 (`product_demo_recovery_with_validation`)
```
=== Recovery + Validation Demo ===
Source:    ntfs_directories.img.zst
Matched:   109 files
Recovered: 109

Validation breakdown:
  Valid:     0
  Invalid:   0
  Uncertain: 109 (no validator for .txt)

Note: PNG/JPEG/PDF fixtures will be added in Chunk 19
      to demonstrate Valid/Invalid distinction.
```

### 🎉 マイルストーン
- **品質判定基盤完成**: PNG/JPEG/PDF の 3 形式に対応
- **拡張可能設計**: Validator trait で新形式追加が容易
- **M4「復旧+品質」70% 進捗**

- **関連 FR**: FR-REC-04 (拡張), FR-QUAL-01 (基盤)

→ tester エージェントへ引き継ぎお願いします
```
