# Chunk 6 指示: NTFS 属性ヘッダパーサ

このチャンクで MFT エントリ内に格納されている**属性**の共通ヘッダを解析します。NTFS では、ファイル名・作成日時・実データなど、ファイルに関するあらゆる情報が「属性」として MFT エントリに保存されています。

このチャンクは属性ヘッダだけ。具体的な属性タイプ（`$STANDARD_INFORMATION`、`$FILE_NAME`、`$DATA`）の中身は Chunk 7 以降で扱います。

---

## 目的

MFT エントリの「first_attribute_offset」（Chunk 5 で取得済み）から始まる属性の連続を解析するための基盤を構築する:

- **属性タイプの識別**（`$STANDARD_INFORMATION` か `$DATA` か等）
- **常駐 / 非常駐の判定**（resident / non-resident）
- **次の属性へのオフセット計算**（属性巡回の基盤）
- **属性終端マーカーの検出**（`0xFFFFFFFF`）

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクで十分）

- Linux NTFS Documentation - Attribute Header: https://flatcap.github.io/linux-ntfs/ntfs/concepts/attribute_header.html
- Linux NTFS Documentation - File Attributes overview: https://flatcap.github.io/linux-ntfs/ntfs/attributes/
- libfsntfs ドキュメント: https://github.com/libyal/libfsntfs/blob/main/documentation/

### 属性共通ヘッダ構造（最初の16バイト、リトルエンディアン）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 4 | **Type identifier** | 0x10〜0x100 の属性タイプ、または `0xFFFFFFFF`（終端） |
| 0x04 | 4 | **Length** | この属性全体の長さ（ヘッダ含む、次属性へのオフセット） |
| 0x08 | 1 | **Non-resident flag** | 0=常駐、1=非常駐 |
| 0x09 | 1 | Name length | 属性名の長さ（**文字数**、バイト数ではない） |
| 0x0A | 2 | Name offset | 属性先頭からの属性名オフセット |
| 0x0C | 2 | Flags | 0x0001=圧縮、0x4000=暗号化、0x8000=スパース |
| 0x0E | 2 | Attribute ID | MFT エントリ内で一意 |

### 常駐属性の追加ヘッダ（offset 0x10〜、非常駐フラグ=0）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x10 | 4 | Content size | コンテンツバイト数 |
| 0x14 | 2 | Content offset | 属性先頭からのコンテンツオフセット |
| 0x16 | 1 | Indexed flag | インデックス対象か |
| 0x17 | 1 | Padding | |

### 非常駐属性の追加ヘッダ（offset 0x10〜、非常駐フラグ=1）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x10 | 8 | Starting VCN | 開始仮想クラスタ番号 |
| 0x18 | 8 | Last VCN | 最終仮想クラスタ番号 |
| 0x20 | 2 | **Runlist offset** | 属性先頭からの runlist オフセット |
| 0x22 | 2 | Compression unit size | |
| 0x24 | 4 | Padding | |
| 0x28 | 8 | Allocated size | アロケート済みバイト数 |
| 0x30 | 8 | **Real size** | 実データバイト数 |
| 0x38 | 8 | Initialized size | 初期化済みバイト数 |

### 主要な属性タイプ

| Type ID | 名前 | 内容 | Chunk |
|---|---|---|---|
| 0x10 | `$STANDARD_INFORMATION` | タイムスタンプ・アクセス権 | Chunk 7 |
| 0x20 | `$ATTRIBUTE_LIST` | 属性リスト（大きいファイル用） | 後続 |
| 0x30 | `$FILE_NAME` | ファイル名・親ディレクトリ参照 | Chunk 8 |
| 0x40 | `$OBJECT_ID` | オブジェクトID | 後続 |
| 0x50 | `$SECURITY_DESCRIPTOR` | セキュリティ記述子 | 後続 |
| 0x60 | `$VOLUME_NAME` | ボリューム名 | 後続 |
| 0x70 | `$VOLUME_INFORMATION` | ボリューム情報 | 後続 |
| 0x80 | `$DATA` | ファイル内容 | Chunk 9-10 |
| 0x90 | `$INDEX_ROOT` | インデックスルート（ディレクトリ） | 後続 |
| 0xA0 | `$INDEX_ALLOCATION` | インデックスアロケーション | 後続 |
| 0xB0 | `$BITMAP` | ビットマップ | 後続 |
| 0xC0 | `$REPARSE_POINT` | リパースポイント | 後続 |
| 0xFFFFFFFF | （終端マーカー） | これ以上属性なし | - |

## 実装内容

### 1. `AttributeType` enum

`crates/fs-ntfs/src/attribute.rs` を新規作成:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeType {
    StandardInformation,   // 0x10
    AttributeList,         // 0x20
    FileName,              // 0x30
    ObjectId,              // 0x40
    SecurityDescriptor,    // 0x50
    VolumeName,            // 0x60
    VolumeInformation,     // 0x70
    Data,                  // 0x80
    IndexRoot,             // 0x90
    IndexAllocation,       // 0xA0
    Bitmap,                // 0xB0
    ReparsePoint,          // 0xC0
    EaInformation,         // 0xD0
    Ea,                    // 0xE0
    LoggedUtilityStream,   // 0x100
    Unknown(u32),          // それ以外（将来用、警告ログ）
    End,                   // 0xFFFFFFFF（終端）
}

impl AttributeType {
    pub fn from_raw(value: u32) -> Self { /* ... */ }
    pub fn to_raw(&self) -> u32 { /* ... */ }
}
```

### 2. 属性ヘッダ構造体群

```rust
/// 全属性に共通の最初の16バイト分のヘッダ
#[derive(Debug, Clone)]
pub struct AttributeCommonHeader {
    pub attribute_type: AttributeType,
    pub length: u32,
    pub non_resident: bool,
    pub name_length: u8,
    pub name_offset: u16,
    pub flags: u16,
    pub attribute_id: u16,
}

/// 常駐属性固有のヘッダ
#[derive(Debug, Clone)]
pub struct ResidentInfo {
    pub content_size: u32,
    pub content_offset: u16,
    pub indexed: bool,
}

/// 非常駐属性固有のヘッダ
#[derive(Debug, Clone)]
pub struct NonResidentInfo {
    pub starting_vcn: u64,
    pub last_vcn: u64,
    pub runlist_offset: u16,
    pub compression_unit_size: u16,
    pub allocated_size: u64,
    pub real_size: u64,
    pub initialized_size: u64,
}

/// 属性ヘッダ全体（常駐/非常駐の区別含む）
#[derive(Debug, Clone)]
pub enum AttributeHeader {
    Resident {
        common: AttributeCommonHeader,
        resident: ResidentInfo,
    },
    NonResident {
        common: AttributeCommonHeader,
        non_resident: NonResidentInfo,
    },
    End, // 終端マーカー
}

impl AttributeHeader {
    /// 共通ヘッダへのアクセサ（End の場合は None）
    pub fn common(&self) -> Option<&AttributeCommonHeader>;
    
    /// この属性の総バイト長（次属性までのオフセット用）
    pub fn length(&self) -> u32;
    
    /// 属性タイプ
    pub fn attribute_type(&self) -> AttributeType;
    
    /// 終端マーカーか
    pub fn is_end(&self) -> bool;
}
```

### 3. パース関数

```rust
/// 単一の属性ヘッダを解析する。
/// 
/// 入力バイト列は属性開始位置からの連続バイト（少なくとも16バイト以上）。
/// 戻り値は AttributeHeader::End なら終端、それ以外なら有効な属性。
pub fn parse_attribute_header(bytes: &[u8]) -> Result<AttributeHeader, AttributeError>;
```

### 4. エラー型

```rust
#[derive(thiserror::Error, Debug)]
pub enum AttributeError {
    #[error("Buffer too small for attribute header: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    
    #[error("Invalid attribute length: {length} (must be > 0 and ≤ buffer)")]
    InvalidLength { length: u32 },
    
    #[error("Invalid non-resident flag: {got}")]
    InvalidNonResidentFlag { got: u8 },
}
```

**注意**: `AttributeType::Unknown(value)` は**エラーにしない**。将来追加された属性タイプや未対応タイプは警告ログに留め、データは読み飛ばす設計とする（forward compatibility）。

### 5. lib.rs に公開

```rust
pub mod attribute;
pub mod boot_sector;
pub mod mft;

pub use attribute::{
    AttributeType, AttributeHeader, AttributeCommonHeader,
    ResidentInfo, NonResidentInfo, AttributeError,
    parse_attribute_header,
};
pub use boot_sector::{BootSector, BootSectorError, parse_boot_sector};
pub use mft::{MftEntry, MftEntryHeader, MftError, parse_mft_entry};
```

## 単体テスト要件（最低8件）

`attribute.rs` の同ファイル内 `#[cfg(test)] mod tests`:

1. **AttributeType from_raw** - 全主要タイプ（0x10, 0x30, 0x80 等）の変換テスト
2. **AttributeType Unknown** - 未知の値（例: 0x42）が `Unknown(0x42)` になる
3. **AttributeType End** - 0xFFFFFFFF が `End` になる
4. **常駐属性ヘッダのパース成功** - 手書きバイト列で全フィールド検証
5. **非常駐属性ヘッダのパース成功** - 手書きバイト列で全フィールド検証
6. **終端マーカーのパース** - 先頭4バイトが `0xFFFFFFFF` で `AttributeHeader::End` が返る
7. **バッファサイズ不足** - 16バイト未満で `BufferTooSmall` エラー
8. **無効な non_resident フラグ** - 0/1 以外で `InvalidNonResidentFlag` エラー
9. **length() メソッド** - Resident / NonResident / End で正しい値が返る

テストデータ作成のヘルパー `fn build_resident_header(...)` と `fn build_nonresident_header(...)` を内部に作ると便利。

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/attribute_integration.rs` を作成:

1. **$MFT エントリ0 の属性巡回**
   - `ntfs_healthy_small.img.zst` を解凍
   - ブートセクタパース → MFT エントリ0 を読み取り（Chunk 4-5 利用）
   - `header.first_attribute_offset` から開始
   - `parse_attribute_header` を呼び出し → 次属性へ `length()` 分進む → 繰り返し
   - `AttributeHeader::End` で停止
   - $MFT エントリには最低 `$STANDARD_INFORMATION` (0x10)、`$FILE_NAME` (0x30)、`$DATA` (0x80) が含まれているはず
   - これらが見つかることをassert

2. **属性タイプの順序確認**
   - 同じ操作で、属性が**タイプID昇順**で並んでいること（NTFS仕様の制約）を確認
   - 例: `[0x10, 0x30, 0x80, 0xB0]` のような順序

ヘルパー（Chunk 4 で作った想定）の `decompress_fixture(name)` と、ブートセクタ・MFTエントリ読み取りの流れを再利用。

## Cargo.toml 設定

変更不要。

## 制約

- 行数上限: **200行（実装+単体テスト合計）**、結合テストは別カウント可
- 単体テスト最低8件
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- パニックする可能性のあるパス禁止（`Result` 返却）

## 完了条件チェックリスト

builder 完了時点で:

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥8件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥2件）
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り) の基盤
- **FR-LIVE-06** (メタデータ表示) の前提

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格を確認後、progress-tracker へ
3. 進捗反映後、**Chunk 7: NTFS `$STANDARD_INFORMATION` 属性 + 属性イテレータ**へ進む（指示は別途）

---

## 注意事項

### Name Length は文字数

`name_length` (offset 0x09) は**文字数**（UTF-16 ワード数）であって**バイト数ではない**。実際の属性名バイト長は `name_length * 2`。これ次の Chunk で属性名を抽出する時に重要になる。

### length フィールドは8バイトアラインメント

NTFS の属性 `length` フィールドは通常 8バイト境界にアラインメントされている。次属性のオフセット計算は単に `current_offset + length` でOK（アラインメントは内部で済んでいる）。

### 終端マーカーの特殊性

`AttributeType` が `0xFFFFFFFF` の場合、その属性には **length フィールドも含めて意味のあるデータがない**。Type ID だけ読んで即終了するのが正しい挙動。

### Unknown 属性タイプの扱い

NTFS は将来拡張用に属性タイプが追加されることがある（実際に 0xD0、0xE0、0x100 などが後年追加された）。**未知のタイプもエラーにせず、`Unknown(raw_value)` として保持**して読み飛ばす設計。これにより新しい Windows バージョンの NTFS にも一定の互換性を持てる。

### Flags の解釈は今は不要

`flags` フィールド（圧縮・暗号化・スパース）は **このチャンクでは値を保持するだけ**で、機能対応は将来のチャンク。圧縮属性のデコードは Phase 1 範囲外でも記録しておくと有用。

### length が 0 の場合

これは破損データ。無限ループを防ぐため、必ず `length == 0` を `InvalidLength` エラーで弾く。

---

## 質問が必要なケース

- `name_offset` がヘッダサイズより小さい場合（不正配置）の扱い
- `compression_unit_size` が異常値の扱い（圧縮属性は Phase 1 範囲外でも要記録）
- 重複した属性ID（仕様上ありえないが、破損データで遭遇する可能性）

---

## 完了時の報告例

```markdown
## Chunk 6 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: crates/fs-ntfs/src/attribute.rs (新規)
- **行数**: 実装 130行 / 単体テスト 65行 / 合計 195行
- **結合テスト**: tests/attribute_integration.rs に2件追加（50行）
- **公開API**: 
  - `AttributeType` enum (16バリアント)
  - `AttributeCommonHeader`, `ResidentInfo`, `NonResidentInfo` 構造体
  - `AttributeHeader` enum (Resident/NonResident/End)
  - `parse_attribute_header(bytes) -> Result<AttributeHeader, AttributeError>`
  - `AttributeError` enum
- **単体テスト**: 9件パス
- **結合テスト**: 2件パス（$MFT エントリ0 の属性巡回、属性順序確認）
- **発見した属性タイプ**: 0x10, 0x30, 0x80, 0xB0（$STANDARD_INFORMATION, $FILE_NAME, $DATA, $BITMAP）
- **関連FR**: FR-LIVE-01 の基盤

→ tester エージェントへ引き継ぎお願いします
```
