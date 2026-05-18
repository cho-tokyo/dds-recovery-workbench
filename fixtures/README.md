# Test Fixtures

テスト用のディスクイメージを配置するディレクトリです。

## 構成

```
fixtures/
├── scripts/        ← イメージ生成スクリプト（Python）
└── images/         ← 生成済みイメージ（.img.zst）と ground truth（.json）
```

## イメージ命名規則

```
<fs>_<シナリオ>_<サイズ>.img.zst
<fs>_<シナリオ>_<サイズ>.json     ← ground truth
```

例:
- `ntfs_healthy_small.img.zst` - 健全NTFS、20MB
- `ntfs_healthy_small.json` - 上記の正解データ
- `ntfs_with_deletions_small.img.zst` - 削除入りNTFS
- `exfat_quick_format.img.zst` - クイックフォーマット後exFAT

## ground truth JSON フォーマット

```json
{
  "fixture_name": "ntfs_with_deletions_small",
  "fs_type": "NTFS",
  "image_size_bytes": 20971520,
  "creation_date": "2026-05-16",
  "scenario": "30 files created, 5 deleted",
  "expected_total_entries": 30,
  "expected_deleted_count": 5,
  "expected_live_count": 25,
  "files": [
    {
      "path": "file_000.txt",
      "size_bytes": 19,
      "content_hash_sha256": "abc...",
      "is_deleted": false,
      "created_at": "2026-05-16T10:00:00Z"
    },
    {
      "path": "file_003.txt",
      "size_bytes": 19,
      "content_hash_sha256": "def...",
      "is_deleted": true,
      "deletion_method": "rm"
    }
  ]
}
```

## イメージ生成（Linux環境）

```bash
# 必要パッケージ
sudo apt install ntfs-3g exfat-fuse exfat-utils zstd python3

# 全フィクスチャ生成
cd fixtures/scripts
python3 gen_all.py

# 個別生成
python3 gen_ntfs_basic.py
```

⚠️ 生成にはsudo権限が必要（ループバックマウント）。

## サイズ目安

| シナリオ | 元サイズ | zstd圧縮後 |
|---|---|---|
| ntfs_healthy_small | 20MB | ~2MB |
| ntfs_with_deletions_small | 20MB | ~2MB |
| ntfs_quick_format_small | 20MB | ~3MB |
| ntfs_healthy_medium | 200MB | ~20MB |
| ntfs_realistic_large | 1GB | ~100MB（CIのみ） |

小サイズ（〜10MB圧縮後）はgitに直接コミット可能。大サイズはLFSまたはCI生成。

## 必要なフィクスチャ一覧（Phase 1）

### NTFS
- [ ] ntfs_healthy_small（健全、20MB）
- [ ] ntfs_with_5_deletions_small（5ファイル削除）
- [ ] ntfs_with_50_deletions_medium（50ファイル削除）
- [ ] ntfs_pt_corrupted（PT破損のみ、FS健全）
- [ ] ntfs_mft_mirror_used（メインMFT破損、MFTMirrorから復元）
- [ ] ntfs_with_japanese_filenames（日本語ファイル名）

### exFAT
- [ ] exfat_healthy_small
- [ ] exfat_with_deletions_small
- [ ] exfat_quick_format_small

### FAT32
- [ ] fat32_healthy_small
- [ ] fat32_with_deletions_small
- [ ] fat32_with_long_filenames

### 共通
- [ ] mixed_partitions（MBR、複数パーティション）
- [ ] gpt_partitions（GPT、複数パーティション）
