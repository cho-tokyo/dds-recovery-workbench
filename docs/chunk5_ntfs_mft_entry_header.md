# Chunk 5 指示: NTFS MFTエントリヘッダパーサ

このチャンクで MFT (Master File Table) エントリの**ヘッダ部分**を解析します。Chunk 4 のブートセクタパーサで `$MFT` の位置とレコードサイズが取得できているので、それを使って実際の MFT エントリを読みます。

---

## 目的

NTFS の `$MFT` から取得した MFT エントリ（FILE レコード）のヘッダ部分（最初の約48バイト）を解析し、以下を判定可能にする:

- **エントリが使用中か削除済みか**（削除復旧の中核判定）
- **ファイルかディレクトリか**
- **属性データの開始位置**（次チャンクで属性パースに使う）
- **Update Sequence（フィクサップ）の適用**（NTFS整合性メカニズム）

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクで十分）

- Linux NTFS Documentation - File Record: https://flatcap.github.io/linux-ntfs/ntfs/concepts/file_record.html
- Linux NTFS Documentation - Fixup: https://flatcap.github.io/linux-ntfs/ntfs/concepts/fixup.html
- libfsntfs ドキュメント: https://github.com/libyal/libfsntfs/blob/main/documentation/

### MFT エントリヘッダ構造（リトルエンディアン）

通常 MFT エントリは1024バイト（Chunk 4 の `mft_record_size_bytes()` で取得）。
その先頭が以下のヘッダ:

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 4 | Magic number | `"FILE"` (0x46 0x49 0x4C 0x45)、破損時は `"BAAD"` |
| 0x04 | 2 | **Update Sequence Offset** | USA配列の開始オフセット |
| 0x06 | 2 | **Update Sequence Size (in words)** | USN + フィクサップ値の合計ワード数 |
| 0x08 | 8 | LogFile Sequence Number (LSN) | `$LogFile` 参照（このチャンクでは未使用） |
| 0x10 | 2 | Sequence Number | エントリ再利用時にインクリメント |
| 0x12 | 2 | Hard Link Count | このファイルへの参照数 |
| 0x14 | 2 | **Offset to first Attribute** | 通常 0x0030〜0x0038、属性パースの起点 |
| 0x16 | 2 | **Flags** | 後述 |
| 0x18 | 4 | Used size of MFT entry | 実際に使用されているバイト数 |
| 0x1C | 4 | Allocated size of MFT entry | 通常 1024 |
| 0x20 | 8 | File reference to base record | 0 = ベースレコード |
| 0x28 | 2 | Next Attribute ID | |
| 0x2A | 2 | Padding | Windows XP+ |
| 0x2C | 4 | MFT Record Number | 自身のレコード番号（XP+） |
| 0x30〜 | varies | Update Sequence Array (USA) | フィクサップ用 |

### Flags フィールド（offset 0x16）

| ビット | 意味 | 影響 |
|---|---|---|
| 0x0001 | **In Use** | 0=削除済み（**復旧対象**）、1=使用中 |
| 0x0002 | **Directory** | 1=ディレクトリ、0=ファイル |
| 0x0004 | Extension | `$Extend` 用、通常0 |
| 0x0008 | Special Index View | |

**削除復旧の根幹**: `flags & 0x0001 == 0` なら削除済みエントリ。データは残っている可能性が高い。

### Update Sequence（フィクサップ）の仕組み

NTFS は多セクタ構造の整合性チェックに「フィクサップ」を使う:

1. **書き込み時**:
   - USN（Update Sequence Number）をインクリメント
   - 各512バイトセクタの**最後の2バイト**を一時退避し、その位置に USN を書き込む
   - 退避した元の値は Update Sequence Array (USA) に格納

2. **読み込み時**:
   - 各セクタ末尾の2バイトが USN と一致するか確認（不一致→破損）
   - 一致したら、USA から元の値を取り出してセクタ末尾に復元

例: 1024バイト MFT エントリ（512×2セクタ）の場合
- USA size = 3 words (6 bytes): `[USN, fixup0, fixup1]`
- offset 0x1FE-0x1FF（セクタ0末尾）と 0x3FE-0x3FF（セクタ1末尾）に USN が書かれている
- 読み込み後、これらを fixup0, fixup1 で置き換える

## 実装内容

### 1. `MftEntryHeader` 構造体

`crates/fs-ntfs/src/mft.rs` を新規作成:

公開フィールド:
- `usa_offset: u16`
- `usa_size: u16` (ワード数)
- `lsn: u64`
- `sequence_number: u16`
- `hard_link_count: u16`
- `first_attribute_offset: u16`
- `flags: u16`
- `used_size: u32`
- `allocated_size: u32`
- `base_record_reference: u64`
- `next_attribute_id: u16`
- `mft_record_number: Option<u32>` (XP+のみ、それより前は `None`)

### 2. パース関数

```rust
/// 単一の MFT エントリ（FILE レコード）を解析する。
/// 
/// 入力バイト列は1つのエントリ全体（通常1024バイト）を想定。
/// この関数内でフィクサップを自動適用する。
pub fn parse_mft_entry(bytes: &[u8]) -> Result<MftEntry, MftError>;
```

ここで `MftEntry` は:

```rust
pub struct MftEntry {
    pub header: MftEntryHeader,
    /// フィクサップ適用済みの全データ。属性パース時にここから読む。
    pub data: Vec<u8>,
}
```

### 3. フィクサップ適用関数（内部、private）

```rust
fn apply_fixup(bytes: &mut [u8], usa_offset: u16, usa_size: u16, sector_size: u16) 
    -> Result<(), MftError>;
```

処理:
1. USA配列の最初のワード = USN
2. USA配列の2番目以降 = フィクサップ値
3. 各セクタ末尾（`sector_size * (i+1) - 2` の位置）が USN と一致するか確認
4. 一致したら、対応するフィクサップ値で上書き
5. 不一致なら `MftError::FixupMismatch` を返す

### 4. 派生メソッド（`impl MftEntryHeader`）

```rust
impl MftEntryHeader {
    /// このエントリが使用中か（生存ファイル/ディレクトリ）
    pub fn is_in_use(&self) -> bool {
        self.flags & 0x0001 != 0
    }
    
    /// このエントリが削除済みか
    pub fn is_deleted(&self) -> bool {
        !self.is_in_use()
    }
    
    /// ディレクトリか（false ならファイル）
    pub fn is_directory(&self) -> bool {
        self.flags & 0x0002 != 0
    }
    
    /// ベースレコードか（拡張レコードでない）
    pub fn is_base_record(&self) -> bool {
        self.base_record_reference == 0
    }
}
```

### 5. エラー型

```rust
#[derive(thiserror::Error, Debug)]
pub enum MftError {
    #[error("Buffer too small for MFT entry: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    
    #[error("Invalid MFT entry magic: expected 'FILE', got {got:?}")]
    InvalidMagic { got: [u8; 4] },
    
    #[error("BAAD MFT entry: data corruption detected at entry")]
    BadEntry,
    
    #[error("Invalid USA offset: {offset}")]
    InvalidUsaOffset { offset: u16 },
    
    #[error("Invalid USA size: {size}")]
    InvalidUsaSize { size: u16 },
    
    #[error("Fixup mismatch at sector {sector}: expected USN 0x{expected:04X}, got 0x{got:04X}")]
    FixupMismatch { sector: usize, expected: u16, got: u16 },
}
```

**注意**: `"BAAD"` シグネチャは NTFS が破損を明示マーキングした状態。Phase 1 ではエラー扱いとし、将来的に「警告付きで部分復旧」する拡張余地を残す。

### 6. lib.rs に公開

```rust
pub mod boot_sector;
pub mod mft;
pub use boot_sector::{BootSector, BootSectorError, parse_boot_sector};
pub use mft::{MftEntry, MftEntryHeader, MftError, parse_mft_entry};
```

## 単体テスト要件（最低6件）

`mft.rs` の同ファイル内 `#[cfg(test)] mod tests`:

1. **正常なFILEヘッダのパース** - 手書きの1024バイトバッファで全フィールド検証
2. **BAADシグネチャの検出** - 先頭4バイトを `"BAAD"` にして `BadEntry` エラー
3. **無効なマジック** - `"XXXX"` で `InvalidMagic` エラー
4. **In Use判定** - `flags = 0x0001` で `is_in_use() == true`
5. **削除判定** - `flags = 0x0000` で `is_deleted() == true`
6. **Directory判定** - `flags = 0x0003`（In Use + Directory）で両方 true
7. **フィクサップ適用成功** - USN と対応するフィクサップ値を仕込んだバッファで、適用後にセクタ末尾が復元されることを確認
8. **フィクサップ不一致エラー** - セクタ末尾の USN を意図的に違う値にして `FixupMismatch` エラー

テストデータ作成のヘルパー関数 `fn build_test_mft_entry(...)` を内部に作ると便利。

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/mft_integration.rs` を作成:

1. **健全イメージから$MFTのレコード0を読む**
   - `ntfs_healthy_small.img.zst` を解凍
   - ブートセクタをパース（Chunk 4 利用）
   - `mft_byte_offset()` 位置から `mft_record_size_bytes()` バイト読み込み
   - パース成功
   - `header.is_in_use() == true`（$MFT 自身は使用中）
   - `header.is_directory() == false`
   - `mft_record_number == Some(0)`（XP+の場合）

2. **削除入りイメージで複数エントリをスキャン**
   - `ntfs_with_5_deletions_small.img.zst` を解凍
   - $MFT 開始から先頭100エントリを順次パース
   - `is_deleted() == true` のエントリ数をカウント
   - ground truth JSON の `expected_deleted_count` と整合（≥ 5）

**重要**: 結合テストでは複数エントリを順次パースするので、`for record_idx in 0..100` のループを書くことになる。$MFT 自体のデータ領域取得は次チャンク（runlist解析）で本格化するが、ここでは**先頭から連続するエントリ**を前提に簡易的に読む（連続レコードは大半のNTFSで成立）。

ヘルパー（Chunk 4で作った想定）の `decompress_fixture(name) -> Vec<u8>` をそのまま再利用。

## Cargo.toml 設定

変更不要（Chunk 4 で追加済みの `zstd`, `serde_json` を継続利用）。

## 制約

- 行数上限: **200行（実装+単体テスト合計）**、結合テストは別カウント可
- 単体テスト最低6件
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- パニックする可能性のあるパス禁止（`Result` 返却）
- バイト読み取りは `u16::from_le_bytes` 等の標準APIで

## 完了条件チェックリスト

builder 完了時点で:

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥6件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥2件）
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り) の中核ロジック
- **FR-LIVE-05** (削除エントリ可視化) の基盤判定（`is_deleted()` 提供）

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格を確認後、progress-tracker へ
3. 進捗反映後、**Chunk 6: NTFS 属性ヘッダパーサ**へ進む（指示は別途）

---

## 注意事項

### フィクサップは絶対に飛ばさない

「USNチェックを省略して動かす」誘惑があるが、これをやると本物の破損データを正常として誤読する原因になる。**必ず実装**してテストすること。

### MFT Record Number の Windows バージョン依存

`mft_record_number` (offset 0x2C) は Windows XP 以降。それ以前は0で埋められている。判定基準として **Allocated Size と OS 推定** を組み合わせるのが厳密だが、Phase 1 では「offset 0x2C の4バイトが 0 以外なら採用、0 なら `None`」で簡略化可。

### セクタサイズ

フィクサップ適用時の `sector_size` は、ブートセクタの `bytes_per_sector` を使う（通常 512）。**4Kn ドライブ**（sector_size=4096）の場合はフィクサップが1セクタ分しかない可能性もある。今のフィクスチャは 512n 想定なのでまずはそれで OK。

### ループ走査時のサニティチェック

結合テストで連続レコードを読むとき、`header.allocated_size != 1024` の場合は警告（または読み飛ばし）すべき。スパースな MFT もあるため。

### 検出すべき異常

- マジックが `"FILE"` でも `"BAAD"` でもない: パーティション境界を超えた可能性 → 即停止
- USA size が異常に大きい: 破損疑い → エラー
- `used_size > allocated_size`: 不整合 → エラー

---

## 質問が必要なケース

以下は推測せず人間に確認:
- フィクサップが部分一致する場合の挙動（厳密にはエラー、寛容モードは別フラグで）
- 4Kn セクタの扱い（Phase 1 範囲外でも記録しておくと有用）
- 「BAAD」エントリの扱い（Phase 1 では Error、将来は警告付き許容）

---

## 完了時の報告例

```markdown
## Chunk 5 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: crates/fs-ntfs/src/mft.rs (新規)
- **行数**: 実装 110行 / 単体テスト 85行 / 合計 195行
- **結合テスト**: tests/mft_integration.rs に2件追加（40行）
- **公開API**: 
  - `MftEntryHeader` 構造体
  - `MftEntry` 構造体
  - `parse_mft_entry(bytes) -> Result<MftEntry, MftError>`
  - `MftError` enum
- **単体テスト**: 8件パス
- **結合テスト**: 2件パス（$MFT エントリ0、削除エントリ検出 ≥5件）
- **関連FR**: FR-LIVE-01, FR-LIVE-05

→ tester エージェントへ引き継ぎお願いします
```
