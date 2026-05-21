#!/usr/bin/env python3
"""
ntfs_mixed_formats.img.zst を生成するスクリプト。

混在形式（PNG/JPEG/PDF/GIF/BMP/DOCX）+ 破損サンプル + 拡張子不一致サンプルを含む
NTFS テストイメージを作成する。Chunk 19（validators 拡充）用。

実行環境:
  - WSL Ubuntu または Linux
  - root 権限 (sudo)
  - パッケージ: ntfs-3g, zstd, python3

実行コマンド:
  sudo python3 gen_ntfs_mixed_formats.py

出力:
  - fixtures/images/ntfs_mixed_formats.img.zst  (圧縮イメージ)
  - fixtures/images/ntfs_mixed_formats.json     (ground truth + expected validation)

含まれるファイル (15 件):
  Valid (10件):  PNG×3, JPEG×2, PDF×2, GIF×1, BMP×1, DOCX×1
  Corrupted (3件): broken PNG, JPEG, PDF
  Mismatch (1件):  .pdf 拡張子だが中身は PNG (拡張子嘘の検出テスト)
  Unknown (1件):   .xyz 拡張子 (Uncertain 判定確認)
"""

import hashlib
import io
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path


# === 設定 ===

SCRIPT_DIR = Path(__file__).resolve().parent
FIXTURES_DIR = SCRIPT_DIR.parent / "images"

IMAGE_NAME = "ntfs_mixed_formats"
IMAGE_SIZE_MB = 30
MOUNT_POINT = "/tmp/ntfs_mixed_formats_mount"

IMAGE_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img"
COMPRESSED_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img.zst"
GROUND_TRUTH_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.json"


# === 形式別の最小有効バイト列 ===

# 1x1 透明 PNG (67 bytes) - Chunk 18 の Validator テスト用と同じ
VALID_PNG = bytes([
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,  # signature
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,  # IHDR length + type
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,  # width=1, height=1
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
    0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,  # IEND
    0x42, 0x60, 0x82,
])

# 最小 JPEG (JFIF コンテナ、22 bytes)
VALID_JPEG = bytes([
    0xFF, 0xD8,                                    # SOI
    0xFF, 0xE0, 0x00, 0x10,                        # APP0 marker + length 16
    0x4A, 0x46, 0x49, 0x46, 0x00,                  # "JFIF\0"
    0x01, 0x01,                                    # version 1.01
    0x00,                                          # density units
    0x00, 0x01, 0x00, 0x01,                        # x/y density
    0x00, 0x00,                                    # thumbnail w/h
    0xFF, 0xD9,                                    # EOI
])

# 最小 PDF
VALID_PDF = (
    b'%PDF-1.4\n'
    b'1 0 obj\n<<>>\nendobj\n'
    b'xref\n0 1\n0000000000 65535 f\n'
    b'trailer\n<</Size 1>>\n'
    b'%%EOF'
)

# 最小 GIF (1x1 GIF89a)
VALID_GIF = bytes([
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61,             # GIF89a
    0x01, 0x00, 0x01, 0x00,                         # 1x1
    0x80, 0x00, 0x00,                               # color table info
    0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,             # palette (black, white)
    0x21, 0xF9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, # GCE
    0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,  # image descriptor
    0x02, 0x02, 0x44, 0x01, 0x00,                   # LZW data
    0x3B,                                           # trailer
])


def make_valid_bmp() -> bytes:
    """2x2 24bit BMP を生成 (70 bytes)"""
    pixels = bytes([
        # 行 1 (BMP は下から上)
        0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00,  # 白白 + padding
        # 行 2
        0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00,  # 緑緑 + padding
    ])
    pixel_data_offset = 54  # 14 + 40
    file_size = pixel_data_offset + len(pixels)

    return (
        b'BM'
        + file_size.to_bytes(4, 'little')
        + b'\x00\x00\x00\x00'                      # reserved
        + pixel_data_offset.to_bytes(4, 'little')  # pixel data offset
        # DIB header (40 bytes)
        + (40).to_bytes(4, 'little')   # DIB header size
        + (2).to_bytes(4, 'little')    # width
        + (2).to_bytes(4, 'little')    # height
        + (1).to_bytes(2, 'little')    # planes
        + (24).to_bytes(2, 'little')   # bpp
        + b'\x00' * 24                 # compression, image size, etc.
        + pixels
    )


VALID_BMP = make_valid_bmp()


def make_minimal_docx() -> bytes:
    """最小 DOCX ([Content_Types].xml に wordprocessingml 含む synthetic ZIP)"""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w', zipfile.ZIP_STORED) as zf:
        zf.writestr(
            '[Content_Types].xml',
            '<?xml version="1.0" encoding="UTF-8"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/word/document.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
            '</Types>'
        )
        zf.writestr('word/document.xml', '<?xml version="1.0"?><document/>')
    return buf.getvalue()


VALID_DOCX = make_minimal_docx()


# === 破損サンプル ===

CORRUPT_PNG_NO_IEND = VALID_PNG[:-12]      # IEND チャンク削除
CORRUPT_JPEG_NO_EOI = VALID_JPEG[:-2]       # EOI marker 削除
CORRUPT_PDF_NO_EOF = VALID_PDF[:-5]         # %%EOF 削除


# === ファイル定義 ===

def define_files():
    """生成するファイルのリスト: (path, content, expected_status, expected_format)"""
    return [
        # ---- Valid samples (10件) ----
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

        # ---- Corrupted samples (3件) ----
        ("broken_001.png", CORRUPT_PNG_NO_IEND, "invalid", "PNG"),
        ("broken_002.jpg", CORRUPT_JPEG_NO_EOI, "invalid", "JPEG"),
        ("broken_003.pdf", CORRUPT_PDF_NO_EOF,  "invalid", "PDF"),

        # ---- Extension mismatch (1件) ----
        # .pdf 拡張子だが中身は PNG → PdfValidator が Invalid 判定すべき
        ("mismatch_001.pdf", VALID_PNG, "invalid", "PDF"),

        # ---- Unknown extension (1件) ----
        # .xyz は Validator なし → Uncertain
        ("unknown_001.xyz", b"some random bytes here", "uncertain", None),
    ]


# === ヘルパー ===

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
        print("ERROR: sudo で実行してください: sudo python3 gen_ntfs_mixed_formats.py")
        sys.exit(1)

    for tool in ["mkntfs", "zstd"]:
        result = subprocess.run(f"which {tool}", shell=True, capture_output=True)
        if result.returncode != 0:
            print(f"ERROR: {tool} が見つかりません")
            print("  sudo apt install ntfs-3g zstd")
            sys.exit(1)


# === メイン ===

def main():
    check_environment()

    # ★ 順序が重要 (v2 パターン)
    full_cleanup()
    if IMAGE_PATH.exists():
        IMAGE_PATH.unlink()
    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    os.makedirs(MOUNT_POINT, exist_ok=True)

    print(f"\n=== Generating {IMAGE_NAME}.img ({IMAGE_SIZE_MB} MB) ===\n")

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
            print(f"  [+] {path} ({len(content)} bytes, expect={expected_status})")
    finally:
        umount_only()

    ground_truth = {
        "fixture_name": IMAGE_NAME,
        "fs_type": "NTFS",
        "image_size_bytes": IMAGE_SIZE_MB * 1024 * 1024,
        "generated_with": "gen_ntfs_mixed_formats.py",
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
    print(f"\n  [ok] Ground truth: {GROUND_TRUTH_PATH}")

    if COMPRESSED_PATH.exists():
        COMPRESSED_PATH.unlink()
    run(f"zstd -19 {IMAGE_PATH} -o {COMPRESSED_PATH}")
    IMAGE_PATH.unlink()
    full_cleanup()

    compressed_kb = COMPRESSED_PATH.stat().st_size / 1024
    print(f"\n=== Done ===")
    print(f"  Compressed image: {COMPRESSED_PATH} ({compressed_kb:.1f} KB)")
    print(f"  Ground truth:     {GROUND_TRUTH_PATH}")
    print(f"  Total files:      {len(files_info)}")
    print(f"    - Valid:        10 (PNG×3, JPEG×2, PDF×2, GIF×1, BMP×1, DOCX×1)")
    print(f"    - Corrupted:     3 (broken_*)")
    print(f"    - Mismatch:      1 (mismatch_001.pdf, 中身 PNG)")
    print(f"    - Unknown:       1 (unknown_001.xyz)")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted, cleaning up...")
        full_cleanup()
        sys.exit(1)
    except subprocess.CalledProcessError as e:
        print(f"\nERROR: command failed: {e}")
        full_cleanup()
        sys.exit(1)
