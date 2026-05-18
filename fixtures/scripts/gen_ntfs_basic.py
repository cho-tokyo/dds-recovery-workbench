#!/usr/bin/env python3
"""
NTFS基本フィクスチャ生成スクリプト

生成内容:
  - ntfs_healthy_small.img: 健全NTFS、30ファイル
  - ntfs_with_5_deletions_small.img: 上記から5ファイル削除

要件:
  - Linux環境
  - sudo権限（ループバックマウント）
  - mkntfs, mount, zstd インストール済み

実行:
  sudo python3 gen_ntfs_basic.py
"""

import os
import sys
import json
import hashlib
import subprocess
import datetime
from pathlib import Path

# 設定
SCRIPT_DIR = Path(__file__).parent
IMAGES_DIR = SCRIPT_DIR.parent / "images"
TMP_DIR = SCRIPT_DIR.parent / ".tmp"
IMAGE_SIZE_MB = 20
NUM_FILES = 30
DELETED_INDICES = [3, 7, 15, 22, 28]  # 削除する5ファイルのインデックス


def run(cmd, check=True):
    """コマンド実行ヘルパー"""
    print(f"  $ {' '.join(cmd) if isinstance(cmd, list) else cmd}")
    result = subprocess.run(cmd, shell=isinstance(cmd, str), check=check, capture_output=True, text=True)
    if result.stderr and "ntfs" not in result.stderr.lower():
        print(f"    stderr: {result.stderr[:200]}")
    return result


def file_content(idx):
    """指定インデックスのファイル内容（再現可能）"""
    return f"This is the content of file number {idx:03d}.\nGenerated for DDS Recovery Workbench testing.\n"


def file_sha256(content):
    """SHA256ハッシュ計算"""
    return hashlib.sha256(content.encode("utf-8")).hexdigest()


def create_base_image(img_path: Path):
    """空のNTFSイメージを作成"""
    print(f"\n[1/4] Creating empty NTFS image: {img_path.name} ({IMAGE_SIZE_MB}MB)")
    run(["dd", "if=/dev/zero", f"of={img_path}", "bs=1M", f"count={IMAGE_SIZE_MB}", "status=none"])
    print("  Formatting as NTFS...")
    run(["mkntfs", "-F", "-Q", "-L", "DDSFIX01", str(img_path)])


def populate_files(img_path: Path) -> list:
    """ファイル群を作成"""
    print(f"\n[2/4] Populating {NUM_FILES} files")
    mount_point = TMP_DIR / "ntfs_mount"
    mount_point.mkdir(parents=True, exist_ok=True)

    files_metadata = []
    try:
        run(["mount", "-o", "loop", str(img_path), str(mount_point)])

        for idx in range(NUM_FILES):
            content = file_content(idx)
            filename = f"file_{idx:03d}.txt"
            filepath = mount_point / filename
            filepath.write_text(content)

            files_metadata.append({
                "path": filename,
                "size_bytes": len(content.encode("utf-8")),
                "content_hash_sha256": file_sha256(content),
                "is_deleted": False,
                "created_at": datetime.datetime.utcnow().isoformat() + "Z",
            })

    finally:
        run(["sync"])
        run(["umount", str(mount_point)], check=False)

    print(f"  Created {len(files_metadata)} files")
    return files_metadata


def create_healthy_image(files_metadata: list) -> Path:
    """健全イメージを生成して圧縮"""
    print("\n[3/4] Generating healthy image")
    img_path = TMP_DIR / "ntfs_healthy_small.img"
    create_base_image(img_path)
    populate_files(img_path)

    # 圧縮
    final_path = IMAGES_DIR / "ntfs_healthy_small.img.zst"
    run(["zstd", "-19", "-f", str(img_path), "-o", str(final_path)])

    # ground truth
    metadata = {
        "fixture_name": "ntfs_healthy_small",
        "fs_type": "NTFS",
        "image_size_bytes": IMAGE_SIZE_MB * 1024 * 1024,
        "creation_date": datetime.date.today().isoformat(),
        "scenario": f"{NUM_FILES} healthy files",
        "expected_total_entries": NUM_FILES,
        "expected_deleted_count": 0,
        "expected_live_count": NUM_FILES,
        "files": files_metadata,
    }
    (IMAGES_DIR / "ntfs_healthy_small.json").write_text(json.dumps(metadata, indent=2))
    print(f"  ✓ Generated: {final_path}")
    return img_path


def create_deletion_image():
    """削除入りイメージを生成"""
    print("\n[4/4] Generating image with deletions")
    img_path = TMP_DIR / "ntfs_with_5_deletions_small.img"
    create_base_image(img_path)
    files_metadata = populate_files(img_path)

    # 一度マウントして指定ファイルを削除
    mount_point = TMP_DIR / "ntfs_mount"
    mount_point.mkdir(parents=True, exist_ok=True)
    try:
        run(["mount", "-o", "loop", str(img_path), str(mount_point)])
        for idx in DELETED_INDICES:
            filename = f"file_{idx:03d}.txt"
            (mount_point / filename).unlink()
            # メタデータ更新
            for f in files_metadata:
                if f["path"] == filename:
                    f["is_deleted"] = True
                    f["deletion_method"] = "unlink"
                    f["deleted_at"] = datetime.datetime.utcnow().isoformat() + "Z"
                    break
    finally:
        run(["sync"])
        run(["umount", str(mount_point)], check=False)

    # 圧縮
    final_path = IMAGES_DIR / "ntfs_with_5_deletions_small.img.zst"
    run(["zstd", "-19", "-f", str(img_path), "-o", str(final_path)])

    # ground truth
    metadata = {
        "fixture_name": "ntfs_with_5_deletions_small",
        "fs_type": "NTFS",
        "image_size_bytes": IMAGE_SIZE_MB * 1024 * 1024,
        "creation_date": datetime.date.today().isoformat(),
        "scenario": f"{NUM_FILES} files created, {len(DELETED_INDICES)} deleted",
        "expected_total_entries": NUM_FILES,
        "expected_deleted_count": len(DELETED_INDICES),
        "expected_live_count": NUM_FILES - len(DELETED_INDICES),
        "files": files_metadata,
    }
    (IMAGES_DIR / "ntfs_with_5_deletions_small.json").write_text(json.dumps(metadata, indent=2))
    print(f"  ✓ Generated: {final_path}")


def main():
    if os.geteuid() != 0:
        print("ERROR: This script requires root privileges (for loop mounting).")
        print("       Run with: sudo python3 gen_ntfs_basic.py")
        sys.exit(1)

    # 必要なツールチェック
    for tool in ["mkntfs", "mount", "umount", "zstd", "dd"]:
        if subprocess.run(["which", tool], capture_output=True).returncode != 0:
            print(f"ERROR: '{tool}' not found. Install with: apt install ntfs-3g zstd")
            sys.exit(1)

    IMAGES_DIR.mkdir(parents=True, exist_ok=True)
    TMP_DIR.mkdir(parents=True, exist_ok=True)

    create_healthy_image([])  # 引数は使われない（内部で再生成）
    create_deletion_image()

    print("\n✅ All fixtures generated successfully")
    print(f"   Output: {IMAGES_DIR}")


if __name__ == "__main__":
    main()
