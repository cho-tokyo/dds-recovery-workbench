# Chunk 4 指示: NTFS ブートセクタパーサ

このチャンクで NTFS 解析の第一歩であるブートセクタ（VBR: Volume Boot Record）パーサを実装します。

---

## 目的

NTFS パーティションの先頭512バイトを解析し、`$MFT` の位置やクラスタサイズなど、後続のすべての解析に必要な基礎情報を抽出する。

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクなら十分）

- Linux NTFS Documentation: https://flatcap.github.io/linux-ntfs/ntfs/files/boot.html
- libfsntfs ドキュメント: https://github.com/libyal/libfsntfs/blob/main/documentation/

### 書籍（あれば理想）

- Brian Carrier『File System Forensic Analysis』第11章 "NTFS Concepts" → 特に Boot Sector セクション

### NTFSブートセクタ構造（512バイト、リトルエンディアン）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 3 | Jump instruction | `EB 52 90` |
| 0x03 | 8 | OEM ID | `"NTFS    "`（最後に3つスペース） |
| 0x0B | 2 | Bytes per sector | 通常 512 |
| 0x0D | 1 | Sectors per cluster | 通常 8 (=4KB cluster) |
| 0x0E | 2 | Reserved sectors | 常に 0 |
| 0x10 | 3 | （ゼロ） | |
| 0x13 | 2 | （ゼロ） | |
| 0x15 | 1 | Media descriptor | 通常 `0xF8` |
| 0x16 | 2 | （ゼロ） | |
| 0x18 | 2 | Sectors per track | |
| 0x1A | 2 | Number of heads | |
| 0x1C | 4 | Hidden sectors | |
| 0x20 | 4 | （ゼロ） | |
| 0x24 | 4 | （通常 `0x80008000`） | |
| 0x28 | 8 | Total sectors | |
| 0x30 | 8 | **MFT LCN** | `$MFT` の論理クラスタ番号 |
| 0x38 | 8 | **MFTMirror LCN** | `$MFTMirr` の論理クラスタ番号 |
| 0x40 | 1 | Clusters per MFT record | 正なら クラスタ数、**負ならバイトサイズの -log2**（通常 -10 = 1024B） |
| 0x41 | 3 | Reserved | |
| 0x44 | 1 | Clusters per Index record | 同上のエンコーディング |
| 0x45 | 3 | Reserved | |
| 0x48 | 8 | Volume serial number | |
| 0x50 | 4 | Checksum | 未使用、通常 0 |
| 0x54 | 426 | Boot code | パース不要 |
| 0x1FE | 2 | Signature | `0x55 0xAA` |

**特に注意**: `Clusters per MFT record` (offset 0x40) は **符号付きi8** として読み取り、
- 正の値 → そのクラスタ数
- 負の値 → MFTレコードは `2^(-value)` バイト（例: -10 → 1024バイト）

## 実装内容

`crates/fs-ntfs/src/boot_sector.rs` に以下を実装:

### 1. `BootSector` 構造体

公開フィールドとして以下を持つ:
- `bytes_per_sector: u16`
- `sectors_per_cluster: u8`
- `media_descriptor: u8`
- `total_sectors: u64`
- `mft_lcn: u64` ($MFT のクラスタ番号)
- `mft_mirror_lcn: u64`
- `clusters_per_mft_record: i8` (生の値を保持)
- `clusters_per_index_record: i8`
- `volume_serial: u64`

### 2. パース関数

```rust
pub fn parse_boot_sector(bytes: &[u8]) -> Result<BootSector, BootSectorError>
```

### 3. 派生メソッド（`impl BootSector`）

- `cluster_size_bytes(&self) -> u32`: クラスタサイズ算出（`bytes_per_sector * sectors_per_cluster`）
- `mft_record_size_bytes(&self) -> u32`: MFTレコードサイズ算出（符号付きルール適用）
- `mft_byte_offset(&self) -> u64`: $MFTのバイトオフセット（`mft_lcn * cluster_size`）

### 4. エラー型

```rust
#[derive(thiserror::Error, Debug)]
pub enum BootSectorError {
    #[error("Buffer too small: got {got}, need at least 512")]
    BufferTooSmall { got: usize },
    
    #[error("Invalid OEM ID: expected 'NTFS    ', got {got:?}")]
    InvalidOemId { got: [u8; 8] },
    
    #[error("Invalid boot signature: expected 0xAA55, got 0x{got:04X}")]
    InvalidSignature { got: u16 },
    
    #[error("Invalid bytes per sector: {got}")]
    InvalidBytesPerSector { got: u16 },
    
    #[error("Invalid sectors per cluster: {got}")]
    InvalidSectorsPerCluster { got: u8 },
}
```

### 5. lib.rs に公開

```rust
pub mod boot_sector;
pub use boot_sector::{BootSector, BootSectorError, parse_boot_sector};
```

## 単体テスト要件（最低5件、推奨7件）

`boot_sector.rs` の同ファイル内 `#[cfg(test)] mod tests`:

1. **正常なバイト列のパース成功** - 全フィールドが期待値通り
2. **バッファサイズ不足** - 512未満で `BufferTooSmall` エラー
3. **無効なOEM ID** - "FAT32   " 等で `InvalidOemId` エラー
4. **無効なシグネチャ** - 末尾2バイトを `0x0000` にすると `InvalidSignature`
5. **MFTレコードサイズ - 負の値** - `clusters_per_mft_record = -10` → `mft_record_size_bytes() == 1024`
6. **MFTレコードサイズ - 正の値** - `clusters_per_mft_record = 1` + クラスタサイズ4096 → `mft_record_size_bytes() == 4096`
7. **クラスタサイズ算出** - 各種組合せ（512×1、512×8、4096×1 等）

テストデータは手書きの `[u8; 512]` 配列を作って使用。実フィクスチャは結合テストで使う。

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/boot_sector_integration.rs` を作成:

1. **実NTFSイメージのパース成功** - `fixtures/images/ntfs_healthy_small.img.zst` を解凍してパース、エラーなく完了
2. **値の妥当性確認** - 解析結果と ground truth JSON が一致
   - `bytes_per_sector` が `expected_total_entries` から逆算可能な値か
   - `cluster_size_bytes()` が一般的な値（512〜65536）に収まるか

ヘルパー関数として `fixtures/images/` の `.img.zst` を解凍するロジックを `crates/fs-ntfs/tests/common/mod.rs` に分離（次チャンク以降でも再利用）。

## Cargo.toml 設定

`crates/fs-ntfs/Cargo.toml` の `[dependencies]` に追加:

```toml
[dependencies]
dds-core.workspace = true
dds-fs-common.workspace = true
thiserror.workspace = true

[dev-dependencies]
zstd = "0.13"
serde_json.workspace = true
```

## 制約

- 行数上限: **200行（実装+テスト合計）** - 結合テストは別カウント可
- 単体テスト最低5件
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- パニックする可能性のあるパス禁止（`Result` 返却）
- バイト読み取りは `u16::from_le_bytes` 等の標準APIで（外部クレート未使用）

## 完了条件チェックリスト

builder 完了時点で:

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥5件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥2件）
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り) の最初の一歩

## 完了後

1. tester エージェントへ引き継ぎ（単体 + 結合テスト両方の実行を依頼）
2. tester がテスト合格を確認したら progress-tracker へ
3. 進捗反映後、**Chunk 5: NTFS MFTエントリヘッダパーサ**の指示を出す（別途）

---

## 注意事項

### リトルエンディアン処理

NTFSは全フィールドがリトルエンディアン。`u16::from_le_bytes(...)`, `u32::from_le_bytes(...)`, `u64::from_le_bytes(...)` を使う。間違って `from_be_bytes` を使うと数値が逆順になる典型バグ。

### 符号付きフィールド

`Clusters per MFT record` は `i8` として読む。`bytes[0x40] as i8` か `i8::from_le_bytes([bytes[0x40]])` で取得。`u8 as i32` のような変換は符号拡張に注意。

### バッファ長チェック

最初に `if bytes.len() < 512 { return Err(...) }` で防御。これがないと配列アクセスでパニックする。

### OEM ID の比較

`"NTFS    "` は8バイト固定（最後に3つスペース）。文字列比較ではなく `&bytes[3..11] == b"NTFS    "` の形で比較する。

---

## 質問が必要なケース

以下は推測せず人間に確認:
- 仕様に明記されていない動作（例: Volume serial の解釈）
- 異常値の扱い（例: `bytes_per_sector` が 0 や 1024 だった場合）
- パフォーマンス最適化が必要か（Phase 1 では「動くこと」優先）
