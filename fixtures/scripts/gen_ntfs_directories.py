#!/usr/bin/env python3
"""
ntfs_directories.img.zst を生成するスクリプト。

階層ディレクトリ + 100 ファイルディレクトリ（$INDEX_ALLOCATION 強制）を含む
NTFS テストイメージを作成する。Chunk 13（list_directory + フルパス再構築）用。

実行環境:
  - WSL Ubuntu または Linux
  - root 権限 (sudo)
  - パッケージ: ntfs-3g, zstd, python3

実行コマンド:
  sudo python3 gen_ntfs_directories.py

出力:
  - fixtures/images/ntfs_directories.img.zst  (圧縮イメージ、約1MB)
  - fixtures/images/ntfs_directories.json     (ground truth)
"""

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

# === 設定 ===
SCRIPT_DIR = Path(__file__).resolve().parent
FIXTURES_DIR = SCRIPT_DIR.parent / "images"

IMAGE_NAME = "ntfs_directories"
IMAGE_SIZE_MB = 30  # raw 30MB、圧縮後 ~1MB
MOUNT_POINT = "/tmp/ntfs_directories_mount"

IMAGE_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img"
COMPRESSED_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.img.zst"
GROUND_TRUTH_PATH = FIXTURES_DIR / f"{IMAGE_NAME}.json"


# === ヘルパー ===

def sha256_of_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def deterministic_content(seed: str, size: int) -> bytes:
    """seed から決定論的なバイト列を生成（再実行でも同じハッシュ）"""
    h = hashlib.sha256(seed.encode()).digest()
    return (h * ((size // len(h)) + 1))[:size]


def run(cmd: str, check: bool = True):
    """シェルコマンド実行"""
    print(f"  [run] {cmd}")
    return subprocess.run(cmd, shell=True, check=check)


def cleanup():
    """マウント解除 + テンポラリディレクトリ削除"""
    subprocess.run(f"umount {MOUNT_POINT} 2>/dev/null", shell=True, check=False)
    subprocess.run(f"rmdir {MOUNT_POINT} 2>/dev/null", shell=True, check=False)


def check_environment():
    """root 権限と必要ツールの確認"""
    if os.geteuid() != 0:
        print("ERROR: sudo で実行してください: sudo python3 gen_ntfs_directories.py")
        sys.exit(1)

    for tool in ["mkntfs", "zstd"]:
        result = subprocess.run(f"which {tool}", shell=True, capture_output=True)
        if result.returncode != 0:
            print(f"ERROR: {tool} が見つかりません")
            print("  sudo apt install ntfs-3g zstd")
            sys.exit(1)


# === ファイル作成ロジック ===

def create_directory_structure(mount: Path, files_info: list):
    """テスト用ディレクトリ階層を作成し、ground truth に記録"""

    # ルート直下: 5 ファイル
    for i in range(1, 6):
        name = f"file_root_{i:03d}.txt"
        content = deterministic_content(name, 100)
        (mount / name).write_bytes(content)
        files_info.append({
            "path": f"\\{name}",
            "size_bytes": len(content),
            "content_hash_sha256": sha256_of_bytes(content),
            "is_deleted": False,
        })

    # dir1/
    (mount / "dir1").mkdir()
    content = deterministic_content("dir1/file_001.txt", 100)
    (mount / "dir1" / "file_001.txt").write_bytes(content)
    files_info.append({
        "path": "\\dir1\\file_001.txt",
        "size_bytes": len(content),
        "content_hash_sha256": sha256_of_bytes(content),
        "is_deleted": False,
    })

    # dir1/sub1/
    (mount / "dir1" / "sub1").mkdir()
    content = deterministic_content("dir1/sub1/file_002.txt", 100)
    (mount / "dir1" / "sub1" / "file_002.txt").write_bytes(content)
    files_info.append({
        "path": "\\dir1\\sub1\\file_002.txt",
        "size_bytes": len(content),
        "content_hash_sha256": sha256_of_bytes(content),
        "is_deleted": False,
    })

    # dir1/sub1/sub2/  (3 階層深さ - フルパス再構築のキーテスト)
    (mount / "dir1" / "sub1" / "sub2").mkdir()
    content = deterministic_content("dir1/sub1/sub2/file_deeply.txt", 100)
    (mount / "dir1" / "sub1" / "sub2" / "file_deeply.txt").write_bytes(content)
    files_info.append({
        "path": "\\dir1\\sub1\\sub2\\file_deeply.txt",
        "size_bytes": len(content),
        "content_hash_sha256": sha256_of_bytes(content),
        "is_deleted": False,
    })

    # dir2/
    (mount / "dir2").mkdir()
    content = deterministic_content("dir2/file_003.txt", 100)
    (mount / "dir2" / "file_003.txt").write_bytes(content)
    files_info.append({
        "path": "\\dir2\\file_003.txt",
        "size_bytes": len(content),
        "content_hash_sha256": sha256_of_bytes(content),
        "is_deleted": False,
    })

    # many/ - 100 ファイル ($INDEX_ALLOCATION 強制)
    (mount / "many").mkdir()
    for i in range(100):
        name = f"file_{i:03d}.txt"
        content = deterministic_content(f"many/{name}", 50)
        (mount / "many" / name).write_bytes(content)
        files_info.append({
            "path": f"\\many\\{name}",
            "size_bytes": len(content),
            "content_hash_sha256": sha256_of_bytes(content),
            "is_deleted": False,
        })


# === メイン ===

def main():
    check_environment()

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    os.makedirs(MOUNT_POINT, exist_ok=True)

    # 既存ファイルクリーンアップ
    cleanup()
    if IMAGE_PATH.exists():
        IMAGE_PATH.unlink()

    print(f"\n=== Generating {IMAGE_NAME}.img ({IMAGE_SIZE_MB} MB) ===\n")

    # 1. 空のイメージ作成
    run(f"dd if=/dev/zero of={IMAGE_PATH} bs=1M count={IMAGE_SIZE_MB} status=progress")

    # 2. NTFS フォーマット
    run(f"mkntfs -F -Q -L {IMAGE_NAME} {IMAGE_PATH}")

    # 3. マウント
    run(f"mount -o loop,rw {IMAGE_PATH} {MOUNT_POINT}")

    # 4. ディレクトリ構造作成
    files_info = []
    try:
        create_directory_structure(Path(MOUNT_POINT), files_info)
    finally:
        # 5. アンマウント
        run(f"umount {MOUNT_POINT}")

    # 6. ground truth JSON 出力
    ground_truth = {
        "fixture_name": IMAGE_NAME,
        "fs_type": "NTFS",
        "image_size_bytes": IMAGE_SIZE_MB * 1024 * 1024,
        "generated_with": "gen_ntfs_directories.py",
        "purpose": "Chunk 13: list_directory + PathResolver test fixture",
        "structure_summary": {
            "root_files": 5,
            "dir1_hierarchy": 3,
            "dir2": 1,
            "many_files": 100,
            "total": len(files_info),
        },
        "files": files_info,
    }
    GROUND_TRUTH_PATH.write_text(json.dumps(ground_truth, indent=2, ensure_ascii=False))
    print(f"\n  [ok] Ground truth: {GROUND_TRUTH_PATH}")

    # 7. zstd 圧縮
    if COMPRESSED_PATH.exists():
        COMPRESSED_PATH.unlink()
    run(f"zstd -19 {IMAGE_PATH} -o {COMPRESSED_PATH}")
    IMAGE_PATH.unlink()

    cleanup()

    # 8. 結果サマリ
    compressed_kb = COMPRESSED_PATH.stat().st_size / 1024
    print(f"\n=== Done ===")
    print(f"  Compressed image: {COMPRESSED_PATH} ({compressed_kb:.1f} KB)")
    print(f"  Ground truth:     {GROUND_TRUTH_PATH}")
    print(f"  Total files:      {len(files_info)}")
    print(f"    - Root:         5")
    print(f"    - dir1 hierarchy: 3 (nested)")
    print(f"    - dir2:         1")
    print(f"    - many/ :       100 (forces $INDEX_ALLOCATION)")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nInterrupted, cleaning up...")
        cleanup()
        sys.exit(1)
    except subprocess.CalledProcessError as e:
        print(f"\nERROR: command failed: {e}")
        cleanup()
        sys.exit(1)
