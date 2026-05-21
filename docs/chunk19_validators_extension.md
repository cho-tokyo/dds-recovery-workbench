# Chunk 19 指示: validators 拡充 + 混在形式フィクスチャ

このチャンクで Chunk 18 で構築した validator 基盤が **業務的に意味のある形** で動き始めます。実際に PNG/JPEG/PDF ファイルを含む NTFS イメージを生成し、`Valid` / `Invalid` / `Uncertain` の判定が業務報告に出るようになります。

> 🎯 完了時点で「PNG 復旧して破損していたら CS が気づける」が **end-to-end で実証**。M4「復旧+品質」90% 進捗。

---

## 目的

3 つの追加要素を統合する:

### A. 混在形式フィクスチャ生成

- PNG / JPEG / PDF / GIF / BMP / OOXML 等を含む NTFS イメージ
- **意図的に破損したサンプル**も含む（Invalid 判定の検証用）
- **拡張子と中身が不一致のサンプル**も含む（業務的に重要）
- ground truth に各ファイルの `expected_validation_status` を記録

### B. 新規 Validator 追加

- **GIF**: マジック (`GIF87a` / `GIF89a`) + トレーラ (`0x3B`)
- **BMP**: マジック (`BM`) + ヘッダサイズ整合性
- **ZIP**: マジック (`PK\003\004`) + EOCD (`PK\005\006`)

### C. OOXML Validator (DOCX/XLSX/PPTX)

- ZIP ベースの 3 形式（Office 文書）
- Office 系は復旧業務で **最重要** な形式の 1 つ
- 共通の OOXML ヘルパー + 3 形式ごとの content type 検査

> 💡 **時間が押した場合**: Part C (OOXML) は Chunk 20 に持ち越し可。Part A + B だけでも業務価値は出る。

## 対象クレート

- **主**: `crates/validators/`（formats/ に新規追加）
- **副**: `fixtures/scripts/`（新フィクスチャ生成スクリプト）
- **テスト**: `crates/recovery/tests/`（end-to-end 統合テスト）

## 仕様参照

### 各形式の参照

- **GIF**: W3C GIF89a Specification
  - Magic: `47 49 46 38 [37|39] 61` (GIF87a or GIF89a)
  - Trailer: `0x3B`
- **BMP**: Microsoft BMP File Format
  - Magic: `42 4D` (BM)
  - File size at offset 2-5 (4 bytes, little-endian)
- **ZIP**: PKWare APPNOTE.TXT
  - Local file header: `50 4B 03 04` (PK\003\004)
  - End of Central Directory: `50 4B 05 06` (PK\005\006)
- **OOXML**: ISO/IEC 29500 (Office Open XML)
  - ZIP container with `[Content_Types].xml`
  - DOCX: `wordprocessingml.document.main+xml`
  - XLSX: `spreadsheetml.sheet.main+xml`
  - PPTX: `presentationml.presentation.main+xml`

## 実装内容

### Part A: 混在フィクスチャ生成スクリプト

`fixtures/scripts/gen_ntfs_mixed_formats.py`:

```python
#!/usr/bin/env python3
"""
ntfs_mixed_formats.img.zst を生成するスクリプト。

混在形式（PNG/JPEG/PDF/GIF/BMP/DOCX）+ 破損サンプル + 拡張子不一致サンプルを含む
NTFS テストイメージを作成する。Chunk 19（validators 拡充）用。
"""

import hashlib
import io
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path

# === 定数 ===

SCRIPT_DIR = Path(__file__).resolve().parent
FIXTURES_DIR = SCRIPT_DIR.parent / "images"

IMAGE_NAME = "ntfs_mixed_formats"
IMAGE_SIZE_MB = 30
MOUNT_POINT = "/tmp/ntfs_mixed_formats_mount"

IMAGE_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img"
COMPRESSED_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img.zst"
GROUND_TRUTH_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.json"

# === 形式別の最小有効バイト列 ===

# 1x1 透明 PNG (67 bytes) - Chunk 18 の Validator テストで使用したものと同じ
VALID_PNG = bytes([
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
    0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
])

# 最小 JPEG (JFIF コンテナ、SOI + APP0 + EOI)
VALID_JPEG = bytes([
    0xFF, 0xD8,                                    # SOI
    0xFF, 0xE0, 0x00, 0x10,                        # APP0 marker + length
    0x4A, 0x46, 0x49, 0x46, 0x00,                  # "JFIF\0"
    0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01,      # version + density
    0x00, 0x00,                                    # thumbnail w/h
    0xFF, 0xD9,                                    # EOI
])

# 最小 PDF
VALID_PDF = (b'%PDF-1.4\n'
             b'1 0 obj\n<<>>\nendobj\n'
             b'xref\n0 1\n0000000000 65535 f\n'
             b'trailer\n<</Size 1>>\n'
             b'%%EOF')

# 最小 GIF (1x1 ピクセル、GIF89a)
VALID_GIF = bytes([
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61,            # GIF89a
    0x01, 0x00, 0x01, 0x00,                        # 1x1
    0x80, 0x00, 0x00,                              # color table info
    0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,            # palette (black, white)
    0x21, 0xF9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,  # GCE
    0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,  # image descriptor
    0x02, 0x02, 0x44, 0x01, 0x00,                  # LZW data
    0x3B,                                          # trailer
])

# 最小 BMP (2x2 24bit、26 + 12 = 38 bytes)
def make_valid_bmp():
    # ヘッダのファイルサイズフィールドを正確に書く必要あり
    pixels = bytes([
        # 行 1 (BMP は下から上)
        0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,  # 白白 + padding
        # 行 2
        0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00,  # 緑緑 + padding
    ])
    pixel_size = len(pixels)
    file_size = 14 + 40 + pixel_size  # FILE_HEADER + DIB_HEADER + pixels
    return (
        b'BM' +
        file_size.to_bytes(4, 'little') +
        b'\x00\x00\x00\x00' +  # reserved
        (54).to_bytes(4, 'little') +  # pixel data offset
        # DIB header (40 bytes)
        (40).to_bytes(4, 'little') +  # DIB header size
        (2).to_bytes(4, 'little') +   # width
        (2).to_bytes(4, 'little') +   # height
        (1).to_bytes(2, 'little') +   # planes
        (24).to_bytes(2, 'little') +  # bpp
        b'\x00' * 24 +                # compression, image size, etc.
        pixels
    )

VALID_BMP = make_valid_bmp()

# 最小 DOCX (synthetic ZIP with [Content_Types].xml containing wordprocessingml)
def make_minimal_docx():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w', zipfile.ZIP_STORED) as zf:
        zf.writestr('[Content_Types].xml',
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '</Types>')
        zf.writestr('word/document.xml', '<?xml version="1.0"?><document/>')
    return buf.getvalue()

VALID_DOCX = make_minimal_docx()

# === 破損サンプル ===

# PNG: IEND チャンクを削除（structurally broken）
CORRUPT_PNG_NO_IEND = VALID_PNG[:-12]

# JPEG: EOI marker を削除
CORRUPT_JPEG_NO_EOI = VALID_JPEG[:-2]

# PDF: %%EOF を削除
CORRUPT_PDF_NO_EOF = VALID_PDF[:-5]

# === ファイル定義 ===

def define_files():
    """生成するファイルのリストを返す: (path, content, expected_status, expected_format)"""
    return [
        # ---- Valid samples ----
        ("image_001.png",  VALID_PNG,    "valid",   "PNG"),
        ("image_002.png",  VALID_PNG,    "valid",   "PNG"),
        ("image_003.png",  VALID_PNG,    "valid",   "PNG"),
        ("photo_001.jpg",  VALID_JPEG,   "valid",   "JPEG"),
        ("photo_002.jpg",  VALID_JPEG,   "valid",   "JPEG"),
        ("report_001.pdf", VALID_PDF,    "valid",   "PDF"),
        ("report_002.pdf", VALID_PDF,    "valid",   "PDF"),
        ("anim_001.gif",   VALID_GIF,    "valid",   "GIF"),
        ("bitmap_001.bmp", VALID_BMP,    "valid",   "BMP"),
        ("doc_001.docx",   VALID_DOCX,   "valid",   "DOCX"),
        
        # ---- Corrupted samples ----
        ("broken_001.png", CORRUPT_PNG_NO_IEND,  "invalid", "PNG"),
        ("broken_002.jpg", CORRUPT_JPEG_NO_EOI,  "invalid", "JPEG"),
        ("broken_003.pdf", CORRUPT_PDF_NO_EOF,   "invalid", "PDF"),
        
        # ---- Extension mismatch ----
        # .pdf 拡張子だが中身は PNG → PdfValidator が Invalid 判定すべき
        ("mismatch_001.pdf", VALID_PNG, "invalid", "PDF"),
        
        # ---- Unknown extension ----
        # .xyz は Validator なし → Uncertain
        ("unknown_001.xyz", b"some random bytes here", "uncertain", None),
    ]

# === 以下、Chunk 13 の gen_ntfs_directories.py と同じパターン ===

def sha256_of_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def run(cmd: str, check: bool = True):
    print(f"  [run] {cmd}")
    return subprocess.run(cmd, shell=True, check=check)

def umount_only():
    subprocess.run(f"umount {MOUNT_POINT} 2>/dev/null", shell=True, check=False)

def full_cleanup():
    umount_only()
    subprocess.run(f"rmdir {MOUNT_POINT} 2>/dev/null", shell=True, check=False)

def check_environment():
    if os.geteuid() != 0:
        print("ERROR: sudo で実行してください")
        sys.exit(1)
    for tool in ["mkntfs", "zstd"]:
        if subprocess.run(f"which {tool}", shell=True, capture_output=True).returncode != 0:
            print(f"ERROR: {tool} が見つかりません。 sudo apt install ntfs-3g zstd")
            sys.exit(1)

def main():
    check_environment()
    full_cleanup()
    if IMAGE_PATH.exists():
        IMAGE_PATH.unlink()
    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    os.makedirs(MOUNT_POINT, exist_ok=True)
    
    print(f"\n=== Generating {IMAGE_NAME}.img ===\n")
    run(f"dd if=/dev/zero of={IMAGE_PATH} bs=1M count={IMAGE_SIZE_MB} status=progress")
    run(f"mkntfs -F -Q -L {IMAGE_NAME} {IMAGE_PATH}")
    run(f"mount -o loop,rw {IMAGE_PATH} {MOUNT_POINT}")
    
    mount = Path(MOUNT_POINT)
    files_info = []
    
    try:
        for path, content, expected_status, expected_format in define_files():
            (mount / path).write_bytes(content)
            files_info.append({
                "path": f"\\{path}",
                "size_bytes": len(content),
                "content_hash_sha256": sha256_of_bytes(content),
                "is_deleted": False,
                "expected_validation_status": expected_status,
                "expected_format": expected_format,
            })
    finally:
        umount_only()
    
    ground_truth = {
        "fixture_name": IMAGE_NAME,
        "fs_type": "NTFS",
        "purpose": "Chunk 19: validators 拡充 - 混在形式 + 破損 + 拡張子不一致",
        "structure_summary": {
            "valid_samples": 10,
            "corrupted_samples": 3,
            "mismatch_samples": 1,
            "unknown_extension": 1,
            "total": len(files_info),
        },
        "files": files_info,
    }
    GROUND_TRUTH_PATH.write_text(json.dumps(ground_truth, indent=2, ensure_ascii=False))
    
    if COMPRESSED_PATH.exists():
        COMPRESSED_PATH.unlink()
    run(f"zstd -19 {IMAGE_PATH} -o {COMPRESSED_PATH}")
    IMAGE_PATH.unlink()
    full_cleanup()
    
    print(f"\n=== Done ===")
    print(f"  Image:        {COMPRESSED_PATH} ({COMPRESSED_PATH.stat().st_size // 1024} KB)")
    print(f"  Ground truth: {GROUND_TRUTH_PATH}")
    print(f"  Total files:  {len(files_info)}")
    print(f"    - Valid:       10")
    print(f"    - Corrupted:    3")
    print(f"    - Mismatch:     1")
    print(f"    - Unknown:      1")

if __name__ == "__main__":
    try:
        main()
    except (KeyboardInterrupt, subprocess.CalledProcessError) as e:
        print(f"\nERROR / Interrupted: {e}")
        full_cleanup()
        sys.exit(1)
```

### Part B: GIF / BMP / ZIP Validator

#### `crates/validators/src/formats/gif.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

const GIF87A: &[u8] = b"GIF87a";
const GIF89A: &[u8] = b"GIF89a";
const GIF_TRAILER: u8 = 0x3B;

pub struct GifValidator;

impl Validator for GifValidator {
    fn name(&self) -> &str { "gif_v1" }
    fn format(&self) -> &str { "GIF" }
    fn extensions(&self) -> &[&str] { &["gif"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < 7 {
            return ValidationResult::invalid("GIF", self.name(),
                format!("File too small ({} bytes)", content.len()));
        }
        
        let header = &content[0..6];
        let version = if header == GIF87A {
            "GIF87a"
        } else if header == GIF89A {
            "GIF89a"
        } else {
            return ValidationResult::invalid("GIF", self.name(),
                format!("GIF signature mismatch: {:02X?}", header));
        };
        
        // Trailer check
        if content.last() != Some(&GIF_TRAILER) {
            return ValidationResult::invalid("GIF", self.name(),
                format!("GIF trailer (0x3B) missing (got 0x{:02X})", 
                    content.last().copied().unwrap_or(0)));
        }
        
        ValidationResult::valid("GIF", self.name(), vec![
            format!("Signature: {}", version),
            "Trailer (0x3B) found".to_string(),
        ])
    }
}
```

#### `crates/validators/src/formats/bmp.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

const BMP_SIGNATURE: [u8; 2] = [b'B', b'M'];
const BMP_HEADER_MIN_SIZE: usize = 14;  // FILE HEADER のサイズ

pub struct BmpValidator;

impl Validator for BmpValidator {
    fn name(&self) -> &str { "bmp_v1" }
    fn format(&self) -> &str { "BMP" }
    fn extensions(&self) -> &[&str] { &["bmp"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < BMP_HEADER_MIN_SIZE {
            return ValidationResult::invalid("BMP", self.name(),
                format!("File too small ({} bytes, need {})", 
                    content.len(), BMP_HEADER_MIN_SIZE));
        }
        
        if content[0..2] != BMP_SIGNATURE {
            return ValidationResult::invalid("BMP", self.name(),
                format!("BMP signature mismatch: {:02X?}", &content[0..2]));
        }
        
        // ヘッダのファイルサイズフィールド (offset 2-5)
        let declared_size = u32::from_le_bytes(content[2..6].try_into().unwrap());
        let actual_size = content.len() as u32;
        
        if declared_size != actual_size {
            return ValidationResult::invalid("BMP", self.name(),
                format!("Size mismatch: header declares {} bytes, actual is {} bytes", 
                    declared_size, actual_size));
        }
        
        ValidationResult::valid("BMP", self.name(), vec![
            "BM signature OK".to_string(),
            format!("File size matches header: {} bytes", actual_size),
        ])
    }
}
```

#### `crates/validators/src/formats/zip.rs`

```rust
use crate::registry::Validator;
use crate::result::ValidationResult;

pub(crate) const ZIP_LOCAL_HEADER: &[u8] = b"PK\x03\x04";
pub(crate) const ZIP_EOCD: &[u8] = b"PK\x05\x06";
pub(crate) const ZIP_EMPTY_EOCD_SIZE: usize = 22;
pub(crate) const EOCD_SEARCH_TAIL: usize = 65557;  // 22 + 65535 (comment max)

/// ZIP コンテナの基本検証。EOCD と Local Header をチェック。
/// OOXML 系（DOCX/XLSX/PPTX）の基盤としても使用される。
pub fn validate_zip_structure(content: &[u8]) -> Result<Vec<String>, String> {
    if content.len() < ZIP_EMPTY_EOCD_SIZE {
        return Err(format!("File too small ({} bytes)", content.len()));
    }
    
    // 先頭 4 バイトは Local file header または EOCD（空 ZIP の場合）
    let starts_with_local = content.starts_with(ZIP_LOCAL_HEADER);
    let starts_with_eocd = content.starts_with(ZIP_EOCD);
    
    if !starts_with_local && !starts_with_eocd {
        return Err(format!("ZIP magic mismatch: {:02X?}", 
            &content[0..4.min(content.len())]));
    }
    
    let mut diagnostics = vec![];
    if starts_with_local {
        diagnostics.push("Local file header magic OK".to_string());
    } else {
        diagnostics.push("Empty ZIP (EOCD only)".to_string());
    }
    
    // EOCD を末尾から探す
    let tail_start = content.len().saturating_sub(EOCD_SEARCH_TAIL);
    let tail = &content[tail_start..];
    let eocd_found = tail.windows(ZIP_EOCD.len()).any(|w| w == ZIP_EOCD);
    
    if !eocd_found {
        return Err("EOCD (PK\\x05\\x06) marker not found in last 64KB".to_string());
    }
    diagnostics.push("EOCD marker found".to_string());
    
    Ok(diagnostics)
}

pub struct ZipValidator;

impl Validator for ZipValidator {
    fn name(&self) -> &str { "zip_v1" }
    fn format(&self) -> &str { "ZIP" }
    fn extensions(&self) -> &[&str] { &["zip"] }
    
    fn validate(&self, content: &[u8]) -> ValidationResult {
        match validate_zip_structure(content) {
            Ok(diagnostics) => ValidationResult::valid("ZIP", self.name(), diagnostics),
            Err(reason) => ValidationResult::invalid("ZIP", self.name(), reason),
        }
    }
}
```

### Part C: OOXML Validator (DOCX/XLSX/PPTX)

#### `crates/validators/src/formats/ooxml.rs`

```rust
use crate::formats::zip::validate_zip_structure;
use crate::registry::Validator;
use crate::result::ValidationResult;

/// `[Content_Types].xml` 内の content type 検索 markers
const DOCX_CONTENT_MARKER: &[u8] = b"wordprocessingml.document";
const XLSX_CONTENT_MARKER: &[u8] = b"spreadsheetml.sheet";
const PPTX_CONTENT_MARKER: &[u8] = b"presentationml.presentation";

const CONTENT_TYPES_FILENAME: &[u8] = b"[Content_Types].xml";

/// OOXML 検証の共通ロジック。
///
/// ZIP として有効か確認 + Content_Types.xml の存在確認 + フォーマット固有マーカーの検索。
/// 検索は ZIP コンテナを解凍せずバイト列スキャンで行う（Phase 1 簡易実装）。
fn validate_ooxml(
    content: &[u8],
    format: &str,
    validator_name: &str,
    content_marker: &[u8],
) -> ValidationResult {
    // Step 1: ZIP として有効か
    let zip_diagnostics = match validate_zip_structure(content) {
        Ok(d) => d,
        Err(reason) => {
            return ValidationResult::invalid(format, validator_name,
                format!("ZIP container invalid: {}", reason));
        }
    };
    
    // Step 2: [Content_Types].xml の存在
    let has_content_types = content.windows(CONTENT_TYPES_FILENAME.len())
        .any(|w| w == CONTENT_TYPES_FILENAME);
    if !has_content_types {
        return ValidationResult::invalid(format, validator_name,
            "[Content_Types].xml not found in archive".to_string());
    }
    
    // Step 3: フォーマット固有マーカー
    let has_format_marker = content.windows(content_marker.len())
        .any(|w| w == content_marker);
    if !has_format_marker {
        return ValidationResult::invalid(format, validator_name,
            format!("Content type marker not found: {:?}",
                std::str::from_utf8(content_marker).unwrap_or("<binary>")));
    }
    
    let mut diagnostics = zip_diagnostics;
    diagnostics.push("[Content_Types].xml found".to_string());
    diagnostics.push(format!("Format marker found: {}", 
        std::str::from_utf8(content_marker).unwrap_or("?")));
    
    ValidationResult::valid(format, validator_name, diagnostics)
}

pub struct DocxValidator;
impl Validator for DocxValidator {
    fn name(&self) -> &str { "docx_v1" }
    fn format(&self) -> &str { "DOCX" }
    fn extensions(&self) -> &[&str] { &["docx"] }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "DOCX", self.name(), DOCX_CONTENT_MARKER)
    }
}

pub struct XlsxValidator;
impl Validator for XlsxValidator {
    fn name(&self) -> &str { "xlsx_v1" }
    fn format(&self) -> &str { "XLSX" }
    fn extensions(&self) -> &[&str] { &["xlsx"] }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "XLSX", self.name(), XLSX_CONTENT_MARKER)
    }
}

pub struct PptxValidator;
impl Validator for PptxValidator {
    fn name(&self) -> &str { "pptx_v1" }
    fn format(&self) -> &str { "PPTX" }
    fn extensions(&self) -> &[&str] { &["pptx"] }
    fn validate(&self, content: &[u8]) -> ValidationResult {
        validate_ooxml(content, "PPTX", self.name(), PPTX_CONTENT_MARKER)
    }
}
```

### Part D: Registry 拡張

`crates/validators/src/registry.rs` の `with_defaults` を更新:

```rust
impl ValidatorRegistry {
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(crate::formats::png::PngValidator));
        reg.register(Arc::new(crate::formats::jpeg::JpegValidator));
        reg.register(Arc::new(crate::formats::pdf::PdfValidator));
        reg.register(Arc::new(crate::formats::gif::GifValidator));
        reg.register(Arc::new(crate::formats::bmp::BmpValidator));
        reg.register(Arc::new(crate::formats::zip::ZipValidator));
        reg.register(Arc::new(crate::formats::ooxml::DocxValidator));
        reg.register(Arc::new(crate::formats::ooxml::XlsxValidator));
        reg.register(Arc::new(crate::formats::ooxml::PptxValidator));
        reg
    }
}
```

`crates/validators/src/formats/mod.rs`:

```rust
pub mod bmp;
pub mod gif;
pub mod jpeg;
pub mod ooxml;
pub mod pdf;
pub mod png;
pub mod zip;
```

## 単体テスト要件（最低 18 件）

各新規 validator にテスト追加:

### `gif.rs`
1. `validates_minimal_gif89a`: 上記 VALID_GIF で Valid
2. `validates_gif87a_signature`: `GIF87a` も Valid
3. `invalid_when_signature_wrong`: 別のマジック → Invalid
4. `invalid_when_trailer_missing`: 末尾 0x3B 削除 → Invalid

### `bmp.rs`
5. `validates_minimal_bmp`: 上記 VALID_BMP で Valid
6. `invalid_when_signature_wrong`: 先頭 BM 以外 → Invalid
7. `invalid_when_size_mismatch`: ヘッダのサイズと実サイズ不一致 → Invalid

### `zip.rs`
8. `validates_minimal_zip_with_local_header_and_eocd`: 標準的 ZIP → Valid
9. `validates_empty_zip_eocd_only`: EOCD のみの空 ZIP → Valid
10. `invalid_when_no_zip_magic`: 別のマジック → Invalid
11. `invalid_when_eocd_missing`: EOCD 削除 → Invalid

### `ooxml.rs`
12. `validates_minimal_docx`: 上記 VALID_DOCX で Valid
13. `invalid_docx_when_zip_broken`: ZIP 構造破損 → Invalid
14. `invalid_docx_when_no_content_types_xml`: `[Content_Types].xml` なし → Invalid
15. `invalid_docx_when_wrong_format_marker`: XLSX を DOCX として検証 → Invalid
16. `validates_xlsx_independently_from_docx`: XLSX 専用マーカーで Valid
17. `validates_pptx_independently`: PPTX 専用マーカーで Valid

### `registry.rs`
18. `with_defaults_registers_all_9_validators`: 全 9 種が拡張子マップに登録される

## 結合テスト要件（最低 4 件）

`crates/recovery/tests/recovery_mixed_formats_integration.rs` を新規作成:

```rust
use std::collections::HashMap;
use tempfile::TempDir;

use dds_fs_ntfs::*;
use dds_recovery::*;
use dds_validators::ValidationStatus;
use dds_wish_match::*;

mod common;
use common::*;

/// 全形式の混在フィクスチャから復旧 + 検証
#[test]
fn recovers_mixed_formats_with_correct_validation_status() {
    let img = decompress_fixture("ntfs_mixed_formats");
    let ground_truth = load_ground_truth("ntfs_mixed_formats");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // 全ファイルを希望（拡張子フィルタなし）
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::Any(vec![
                WishItem::Extension("png".into()),
                WishItem::Extension("jpg".into()),
                WishItem::Extension("pdf".into()),
                WishItem::Extension("gif".into()),
                WishItem::Extension("bmp".into()),
                WishItem::Extension("docx".into()),
                WishItem::Extension("xyz".into()),
            ]),
            "全形式テスト"
        ).with_priority(Priority::High));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    assert_eq!(report.recovered.len(), 15);  // ground truth の全 15 ファイル
    
    // ground truth の expected_validation_status と実結果を照合
    let expected: HashMap<String, (String, Option<String>)> = ground_truth["files"]
        .as_array().unwrap().iter()
        .map(|f| (
            f["path"].as_str().unwrap().to_string(),
            (
                f["expected_validation_status"].as_str().unwrap().to_string(),
                f["expected_format"].as_str().map(|s| s.to_string()),
            ),
        ))
        .collect();
    
    let mut matched = 0;
    for entry in &report.recovered {
        let Some((expected_status, expected_format)) = expected.get(&entry.original_path) else {
            continue;
        };
        let actual = entry.validation.as_ref().unwrap();
        
        let actual_status = match actual.status {
            ValidationStatus::Valid => "valid",
            ValidationStatus::Invalid => "invalid",
            ValidationStatus::Uncertain => "uncertain",
        };
        
        assert_eq!(actual_status, expected_status,
            "Status mismatch for {}: expected {}, got {}",
            entry.original_path, expected_status, actual_status);
        
        if let Some(expected_fmt) = expected_format {
            assert_eq!(actual.format_detected.as_deref(), Some(expected_fmt.as_str()),
                "Format mismatch for {}", entry.original_path);
        }
        
        matched += 1;
    }
    assert_eq!(matched, 15, "All 15 files should match ground truth");
}

/// 拡張子と中身の不一致検出
#[test]
fn extension_content_mismatch_detected_as_invalid() {
    // mismatch_001.pdf は中身が PNG → PdfValidator が Invalid 判定
    // ... 同様のセットアップ ...
    
    let mismatch_entry = report.recovered.iter()
        .find(|e| e.original_path == "\\mismatch_001.pdf")
        .expect("mismatch sample not found");
    
    let validation = mismatch_entry.validation.as_ref().unwrap();
    assert!(validation.status.is_invalid(), 
        "Extension-content mismatch should be Invalid");
    assert_eq!(validation.format_detected.as_deref(), Some("PDF"));
    // 診断メッセージに「PDF header missing」等の理由が含まれる
}

/// 破損ファイル検出
#[test]
fn corrupted_samples_marked_as_invalid() {
    // broken_001.png (IEND なし) / broken_002.jpg (EOI なし) / broken_003.pdf (EOF なし)
    // すべて Invalid 判定であること
    // ...
    let broken_paths = ["\\broken_001.png", "\\broken_002.jpg", "\\broken_003.pdf"];
    for path in broken_paths {
        let entry = report.recovered.iter()
            .find(|e| e.original_path == path)
            .expect(&format!("{} not found", path));
        let validation = entry.validation.as_ref().unwrap();
        assert!(validation.status.is_invalid(),
            "{} should be Invalid, got {:?}", path, validation.status);
    }
}

/// プロダクトデモテスト（CS 視点の業務報告）
#[test]
fn product_demo_recovery_with_quality_breakdown() {
    let img = decompress_fixture("ntfs_mixed_formats");
    // ... setup ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::Any(vec![
                WishItem::Extension("png".into()),
                WishItem::Extension("jpg".into()),
                WishItem::Extension("pdf".into()),
                WishItem::Extension("gif".into()),
                WishItem::Extension("bmp".into()),
                WishItem::Extension("docx".into()),
            ]),
            "顧客指定: 画像と書類すべて"
        ).with_priority(Priority::High));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Quality Breakdown Demo ===\n");
    println!("Source:    ntfs_mixed_formats.img.zst");
    println!("Matched:   {}", report.total_matched);
    println!("Recovered: {}", report.recovered.len());
    println!();
    println!("Validation breakdown:");
    println!("  ✓ Valid:     {}", report.validated_count());
    println!("  ✗ Invalid:   {}", report.invalid_count());
    println!("  ? Uncertain: {}", report.recovered.len() - report.validated_count() - report.invalid_count());
    println!();
    
    // フォーマット別集計
    let mut by_format: HashMap<String, (u32, u32, u32)> = HashMap::new();  // (valid, invalid, total)
    for entry in &report.recovered {
        let Some(v) = &entry.validation else { continue };
        let format = v.format_detected.clone().unwrap_or_else(|| "Unknown".into());
        let counters = by_format.entry(format).or_insert((0, 0, 0));
        counters.2 += 1;
        match v.status {
            ValidationStatus::Valid => counters.0 += 1,
            ValidationStatus::Invalid => counters.1 += 1,
            ValidationStatus::Uncertain => {}
        }
    }
    
    println!("Format breakdown:");
    let mut formats: Vec<_> = by_format.into_iter().collect();
    formats.sort_by(|a, b| b.1.2.cmp(&a.1.2));  // 件数降順
    for (format, (valid, invalid, total)) in formats {
        println!("  {} : {}/{} valid ({} invalid)", format, valid, total, invalid);
    }
    
    println!();
    println!("Invalid files (要 CS 確認):");
    for entry in report.recovered.iter().filter(|e| e.validation.as_ref().map(|v| v.status.is_invalid()).unwrap_or(false)) {
        let reason = entry.validation.as_ref()
            .and_then(|v| v.diagnostics.first())
            .map(|s| s.as_str()).unwrap_or("unknown");
        println!("  ✗ {} → {}", entry.original_path, reason);
    }
    
    // 期待値: valid 10, invalid 4 (3 corrupted + 1 mismatch), uncertain 1 (xyz)
    assert_eq!(report.validated_count(), 10);
    assert_eq!(report.invalid_count(), 4);
}
```

期待される出力:

```
=== DDS Recovery Workbench - Quality Breakdown Demo ===

Source:    ntfs_mixed_formats.img.zst
Matched:   14  (xyz は wishlist のマッチ対象外)
Recovered: 14

Validation breakdown:
  ✓ Valid:     10
  ✗ Invalid:    4
  ? Uncertain:  0

Format breakdown:
  PNG  : 3/4 valid (1 invalid)
  JPEG : 2/3 valid (1 invalid)
  PDF  : 2/4 valid (2 invalid)   ← 破損1 + 不一致1
  GIF  : 1/1 valid
  BMP  : 1/1 valid
  DOCX : 1/1 valid

Invalid files (要 CS 確認):
  ✗ \broken_001.png → IEND chunk not found at end of file
  ✗ \broken_002.jpg → EOI marker missing
  ✗ \broken_003.pdf → %%EOF trailer not found
  ✗ \mismatch_001.pdf → PDF header missing (got "\x89PNG")
```

## Cargo.toml 設定

変更不要（既存 thiserror, serde で足りる）。

## 制約

- **行数目安**:
  - validators 新規ファイル合計: ~400 行 + テスト 130 行
  - 結合テスト: ~200 行
  - Python フィクスチャ生成: ~250 行
- **単体テスト最低 18 件**（既存 14 + 新規 18 = 32 件以上）
- **結合テスト最低 4 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **既存テスト破壊禁止**

## 完了条件チェックリスト

- [ ] `fixtures/scripts/gen_ntfs_mixed_formats.py` 作成 + WSL で実行成功
- [ ] `fixtures/images/ntfs_mixed_formats.img.zst` 生成 + ground truth JSON 同梱
- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-validators` が全パス（≥32 件）
- [ ] `cargo test -p dds-recovery` が全パス（既存 + 新規結合 ≥4 件）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_recovery_with_quality_breakdown` が pass + 出力が見える
- [ ] 拡張子不一致サンプル (`mismatch_001.pdf`) が Invalid 判定
- [ ] 破損サンプル (`broken_*`) すべて Invalid 判定
- [ ] `grep -r 'unsafe' crates/validators/src/` で 0 件

## 関連 FR 要件

- **FR-QUAL-01** (品質判定基盤) ← **拡充完了**
- **FR-QUAL-02** (検証結果統合) ← フォーマット別集計対応
- **FR-QUAL-03** (3 値ステータス) ← 業務シナリオで実証

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 M4「復旧+品質」90% 進捗、Phase 1 NTFS-α リリース直前**
4. 次のステップ候補:
   - **Chunk 20**: 復旧結果レポート生成 (PDF/Excel/HTML/CSV、FR-REP-01〜05)
   - **Chunk 21**: 案件管理 (case-manager、FR-CASE-01〜05)
   - **Chunk 22**: Tauri UI 着手
   - **実機検証**: 並行作業

---

## 注意事項

### フィクスチャ生成の WSL 実行

Chunk 13 の `gen_ntfs_directories.py` と同じ手順:

```bash
sudo mkdir -p /tmp/ntfs_mixed_formats_mount  # 念のため
cd /mnt/c/Users/dds.r8d/Documents/Claude/Projects/dds-recovery-workbench/fixtures/scripts
sudo python3 gen_ntfs_mixed_formats.py
```

cleanup 順序のバグは v2 で修正済みなので、新スクリプトでは正しい順序を踏襲。

### OOXML 検証のバイト列スキャン

Phase 1 簡易実装として、ZIP コンテナを実際に解凍せず、生バイト列内で `[Content_Types].xml` と `wordprocessingml.document` 等の文字列を `windows().any()` でスキャン。

これは技術的には**誤検出の余地**があるが、Phase 1 のユースケース（破損検出と拡張子嘘検出）には十分。Phase 2 で実際の ZIP 解凍 + XML 解析に置き換える余地を残す。

### ZIP の EOCD 検索範囲

ZIP の EOCD は最大 64KB + 22 バイト範囲内に存在し得る（コメントフィールドが最大 65535 バイト）。実装の `EOCD_SEARCH_TAIL = 65557` がこの仕様に従う。

### BMP のファイルサイズ整合性

`mkntfs` でファイルが書かれた後の実サイズと、BMP ヘッダ内に記録された宣言サイズは**通常一致するはず**。一致しない = ファイル切り詰めや改変の証拠。

ただし、一部の BMP エクスポータが正しくサイズを書かない（0 を入れる等）ことがある。Phase 1 では厳密に一致を要求するが、誤検出が業務的に問題になるなら Chunk 20+ で「サイズ 0 なら警告のみ」のような寛容モード追加を検討。

### OOXML の文字列スキャンの脆弱性

`wordprocessingml.document` という文字列を含むテキストファイルを `.docx` として復旧すると、誤って Valid 判定される可能性。

Phase 1 では:
1. **ZIP 構造**（PK\003\004 と EOCD）
2. **[Content_Types].xml 文字列**
3. **format-specific marker**

の 3 つ全部が揃って初めて Valid。3 つ揃った非 OOXML ファイルは現実的にはほぼ無いので OK。

### Phase 1 で意図的に除外した機能

- **TIFF / WebP / AVIF**: 画像系の拡張、Chunk 20+ で
- **MP4 / MOV / WebM**: 動画系、Phase 2 で
- **MP3 / WAV / FLAC**: 音声系、Phase 2 で
- **ZIP コンテナの完全解凍検証**: 圧縮データの CRC まで見る、Phase 2
- **PDF の xref テーブル解析**: Phase 2

---

## 質問が必要なケース

- BMP ファイルサイズフィールドのゼロ許容（業務優先度）
- OOXML 内の XML スキーマ詳細検証の必要性
- 拡張子なしファイル（無拡張）のフォールバック挙動

---

## 完了報告例

```markdown
## Chunk 19 完了報告

### 新規ファイル
- `fixtures/scripts/gen_ntfs_mixed_formats.py` (250 行)
- `fixtures/images/ntfs_mixed_formats.img.zst` (生成済み)
- `fixtures/images/ntfs_mixed_formats.json` (ground truth)
- `crates/validators/src/formats/gif.rs` (60 行 + テスト 30 行)
- `crates/validators/src/formats/bmp.rs` (70 行 + テスト 35 行)
- `crates/validators/src/formats/zip.rs` (80 行 + テスト 35 行)
- `crates/validators/src/formats/ooxml.rs` (130 行 + テスト 55 行)
- `crates/recovery/tests/recovery_mixed_formats_integration.rs` (250 行)

### 既存ファイル更新
- `crates/validators/src/formats/mod.rs`: 新 module 4 件追加
- `crates/validators/src/registry.rs::with_defaults`: 6 つの新 validator 登録

### 公開 API
- `GifValidator`, `BmpValidator`, `ZipValidator`
- `DocxValidator`, `XlsxValidator`, `PptxValidator`
- `validate_zip_structure` (OOXML から再利用される共通関数、pub(crate))

### テスト統計
- 単体: 既存 269 + 新規 18 = **287 件 pass**
- 結合: 既存 45 + 新規 4 = **49 件 pass**
- 全 workspace: **336+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 拡張子不一致サンプル (`mismatch_001.pdf` ← 中身 PNG) を Invalid 判定で検出成功

### 業務価値の見える化 (`product_demo_recovery_with_quality_breakdown`)
```
=== DDS Recovery Workbench - Quality Breakdown Demo ===

Source:    ntfs_mixed_formats.img.zst
Matched:   14
Recovered: 14

Validation breakdown:
  ✓ Valid:     10
  ✗ Invalid:    4
  ? Uncertain:  0

Format breakdown:
  PNG  : 3/4 valid (1 invalid)
  JPEG : 2/3 valid (1 invalid)
  PDF  : 2/4 valid (2 invalid)
  GIF  : 1/1 valid
  BMP  : 1/1 valid
  DOCX : 1/1 valid

Invalid files (要 CS 確認):
  ✗ \broken_001.png → IEND chunk not found at end of file
  ✗ \broken_002.jpg → EOI marker missing
  ✗ \broken_003.pdf → %%EOF trailer not found
  ✗ \mismatch_001.pdf → PDF header missing (got %89PNG)
```

### 🎉 マイルストーン達成
- **validator 業務価値の実証**: PNG/PDF/DOCX の Valid/Invalid 判定が正常動作
- **拡張子嘘の検出**: PDF 拡張子で PNG 中身を Invalid 判定で警告
- **M4「復旧+品質」90% 進捗**

- **関連 FR**: FR-QUAL-01/02/03 (実証完了)

→ tester エージェントへ引き継ぎお願いします
```
