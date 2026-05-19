# Chunk 7 指示: 属性イテレータ + $STANDARD_INFORMATION 属性

このチャンクで2つの機能を実装します:

1. **属性イテレータ**: MFTエントリ内の属性を順次取り出す機構（Chunk 8以降の全属性パースで再利用）
2. **`$STANDARD_INFORMATION` 属性パーサ**: ファイルのタイムスタンプ・属性フラグを取得

このチャンク完了時点で、**実NTFSイメージから「削除されたファイルが、いつ作成され・いつ削除されたか」が分かる**ようになります。

---

## 目的

- MFT エントリ内の属性を順次・安全に巡回する API を提供する
- `$STANDARD_INFORMATION`（タイプID 0x10）属性をパースし、4種のタイムスタンプ＋ファイル属性フラグを取得する
- FILETIME 形式（Windows独自の時刻表現）を `chrono::DateTime<Utc>` に変換する

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクで十分）

- Linux NTFS Documentation - $STANDARD_INFORMATION: https://flatcap.github.io/linux-ntfs/ntfs/attributes/standard_information.html
- Microsoft FILETIME structure: https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime
- Microsoft File Attributes: https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants

### $STANDARD_INFORMATION 属性のコンテンツ構造（リトルエンディアン）

**NT版（48バイト、最小形式）**:

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 8 | Creation time | FILETIME |
| 0x08 | 8 | Last modification time | FILETIME（ファイル内容変更） |
| 0x10 | 8 | MFT modification time | FILETIME（メタデータ変更） |
| 0x18 | 8 | Last access time | FILETIME |
| 0x20 | 4 | DOS file attributes | ビットフラグ |
| 0x24 | 4 | Maximum versions | 通常 0 |
| 0x28 | 4 | Version number | 通常 0 |
| 0x2C | 4 | Class ID | 通常 0 |

**Windows 2000+ 拡張版（72バイト）** - 上記に加えて:

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x30 | 4 | Owner ID | |
| 0x34 | 4 | Security ID | |
| 0x38 | 8 | Quota charged | |
| 0x40 | 8 | Update Sequence Number (USN) | |

**注意**: 属性のコンテンツサイズ（Chunk 6 で取得した `ResidentInfo::content_size`）で NT版 or W2K+ 版を判別する。

### FILETIME とは

Windows の時刻表現で、**1601年1月1日 UTC からの 100ナノ秒単位の経過時間**を表す u64 値。

Unix epoch (1970年1月1日) との差: `11_644_473_600 秒 = 11_644_473_600 * 10_000_000 (100ns units)`

### DOS ファイル属性ビット（offset 0x20、u32）

| ビット | 値 | 意味 |
|---|---|---|
| 0 | 0x0001 | READ_ONLY |
| 1 | 0x0002 | HIDDEN |
| 2 | 0x0004 | SYSTEM |
| 5 | 0x0020 | ARCHIVE |
| 7 | 0x0080 | NORMAL |
| 8 | 0x0100 | TEMPORARY |
| 9 | 0x0200 | SPARSE_FILE |
| 10 | 0x0400 | REPARSE_POINT |
| 11 | 0x0800 | COMPRESSED |
| 12 | 0x1000 | OFFLINE |
| 13 | 0x2000 | NOT_CONTENT_INDEXED |
| 14 | 0x4000 | ENCRYPTED |
| 28 | 0x10000000 | DIRECTORY (NTFSのみ) |

## 実装内容

### ファイル構成

```
crates/fs-ntfs/src/
├── lib.rs          ← 更新
├── boot_sector.rs  ← Chunk 4
├── mft.rs          ← Chunk 5
├── attribute.rs    ← Chunk 6
└── attributes/     ← 新規モジュール
    ├── mod.rs                    ← 属性イテレータ + 公開API
    └── standard_information.rs   ← $STANDARD_INFORMATION パーサ
```

### 1. 属性イテレータ (`attributes/mod.rs`)

```rust
//! 属性巡回モジュール

pub mod standard_information;

pub use standard_information::{StandardInformation, FileTime, FileAttributes, SiError};

use crate::attribute::{AttributeError, AttributeHeader, parse_attribute_header};

/// MFT エントリの属性を順次取り出すためのリファレンス。
pub struct AttributeRef<'a> {
    /// パース済みのヘッダ
    pub header: AttributeHeader,
    /// この属性の生バイト（ヘッダ含む全体）。
    /// 常駐属性のコンテンツは `header` の `ResidentInfo::content_offset` から取れる。
    pub raw: &'a [u8],
    /// MFTエントリ先頭からのこの属性のオフセット
    pub offset_in_entry: usize,
}

/// MFT エントリの属性イテレータ。
/// 
/// 終端マーカー (`AttributeHeader::End`) で停止する。
/// パースエラーが発生した場合、エラーを yield して停止する。
pub struct AttributeIterator<'a> {
    /// MFTエントリの全データ（フィクサップ適用済み）
    entry_data: &'a [u8],
    /// 次に読む位置
    cursor: usize,
    /// エラー or 終端で停止済みか
    done: bool,
}

impl<'a> AttributeIterator<'a> {
    /// MFTエントリのデータと最初の属性オフセットから開始する。
    pub fn new(entry_data: &'a [u8], first_attribute_offset: usize) -> Self {
        Self {
            entry_data,
            cursor: first_attribute_offset,
            done: false,
        }
    }
}

impl<'a> Iterator for AttributeIterator<'a> {
    type Item = Result<AttributeRef<'a>, AttributeError>;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.cursor >= self.entry_data.len() {
            return None;
        }
        
        let remaining = &self.entry_data[self.cursor..];
        
        match parse_attribute_header(remaining) {
            Ok(AttributeHeader::End) => {
                self.done = true;
                None
            }
            Ok(header) => {
                let length = header.length() as usize;
                if length == 0 || self.cursor + length > self.entry_data.len() {
                    self.done = true;
                    return Some(Err(AttributeError::InvalidLength { length: length as u32 }));
                }
                
                let raw = &self.entry_data[self.cursor..self.cursor + length];
                let attr_ref = AttributeRef {
                    header,
                    raw,
                    offset_in_entry: self.cursor,
                };
                self.cursor += length;
                Some(Ok(attr_ref))
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// 属性タイプで最初に一致する属性を取得するヘルパー
pub fn find_attribute<'a>(
    entry_data: &'a [u8],
    first_attribute_offset: usize,
    target_type: crate::attribute::AttributeType,
) -> Option<AttributeRef<'a>> {
    AttributeIterator::new(entry_data, first_attribute_offset)
        .filter_map(Result::ok)
        .find(|attr| attr.header.attribute_type() == target_type)
}
```

### 2. $STANDARD_INFORMATION パーサ (`attributes/standard_information.rs`)

```rust
//! $STANDARD_INFORMATION 属性（タイプID 0x10）

use thiserror::Error;
use chrono::{DateTime, Utc};

/// FILETIME (Windows 1601年起算 100ns単位) のラッパー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileTime(pub u64);

impl FileTime {
    /// FILETIME を UTC DateTime に変換する。
    /// 範囲外（unrepresentable）の場合は None。
    pub fn to_datetime(&self) -> Option<DateTime<Utc>> {
        // 1601-01-01 から 1970-01-01 までの秒数
        const EPOCH_DIFF_SECS: i64 = 11_644_473_600;
        let total_100ns = self.0 as i64;
        let secs = total_100ns / 10_000_000 - EPOCH_DIFF_SECS;
        let nanos = ((total_100ns % 10_000_000) * 100) as u32;
        DateTime::from_timestamp(secs, nanos)
    }
}

/// DOS ファイル属性フラグ
#[derive(Debug, Clone, Copy)]
pub struct FileAttributes(pub u32);

impl FileAttributes {
    pub const READ_ONLY: u32 = 0x0001;
    pub const HIDDEN: u32 = 0x0002;
    pub const SYSTEM: u32 = 0x0004;
    pub const ARCHIVE: u32 = 0x0020;
    pub const COMPRESSED: u32 = 0x0800;
    pub const ENCRYPTED: u32 = 0x4000;
    pub const DIRECTORY: u32 = 0x1000_0000;
    
    pub fn is_read_only(&self) -> bool { self.0 & Self::READ_ONLY != 0 }
    pub fn is_hidden(&self) -> bool { self.0 & Self::HIDDEN != 0 }
    pub fn is_system(&self) -> bool { self.0 & Self::SYSTEM != 0 }
    pub fn is_archive(&self) -> bool { self.0 & Self::ARCHIVE != 0 }
    pub fn is_compressed(&self) -> bool { self.0 & Self::COMPRESSED != 0 }
    pub fn is_encrypted(&self) -> bool { self.0 & Self::ENCRYPTED != 0 }
    pub fn is_directory(&self) -> bool { self.0 & Self::DIRECTORY != 0 }
}

/// $STANDARD_INFORMATION 属性のパース結果
#[derive(Debug, Clone)]
pub struct StandardInformation {
    pub created: FileTime,
    pub modified: FileTime,
    pub mft_modified: FileTime,
    pub accessed: FileTime,
    pub file_attributes: FileAttributes,
    pub max_versions: u32,
    pub version_number: u32,
    pub class_id: u32,
    // W2K+ 拡張部（content_size >= 72 の場合のみ）
    pub owner_id: Option<u32>,
    pub security_id: Option<u32>,
    pub quota_charged: Option<u64>,
    pub usn: Option<u64>,
}

#[derive(Error, Debug)]
pub enum SiError {
    #[error("Buffer too small for $STANDARD_INFORMATION: got {got}, need at least 48")]
    BufferTooSmall { got: usize },
}

/// $STANDARD_INFORMATION 属性の **コンテンツ部分** をパースする。
/// 入力は属性ヘッダ部分を除いた純粋なコンテンツバイト列。
pub fn parse_standard_information(bytes: &[u8]) -> Result<StandardInformation, SiError> {
    if bytes.len() < 48 {
        return Err(SiError::BufferTooSmall { got: bytes.len() });
    }
    
    let read_u64 = |offset| u64::from_le_bytes(bytes[offset..offset+8].try_into().unwrap());
    let read_u32 = |offset| u32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap());
    
    let result = StandardInformation {
        created: FileTime(read_u64(0x00)),
        modified: FileTime(read_u64(0x08)),
        mft_modified: FileTime(read_u64(0x10)),
        accessed: FileTime(read_u64(0x18)),
        file_attributes: FileAttributes(read_u32(0x20)),
        max_versions: read_u32(0x24),
        version_number: read_u32(0x28),
        class_id: read_u32(0x2C),
        owner_id: (bytes.len() >= 0x34).then(|| read_u32(0x30)),
        security_id: (bytes.len() >= 0x38).then(|| read_u32(0x34)),
        quota_charged: (bytes.len() >= 0x40).then(|| read_u64(0x38)),
        usn: (bytes.len() >= 0x48).then(|| read_u64(0x40)),
    };
    
    Ok(result)
}
```

### 3. lib.rs 更新

```rust
pub mod attribute;
pub mod attributes;
pub mod boot_sector;
pub mod mft;

pub use attribute::{
    AttributeType, AttributeHeader, AttributeCommonHeader,
    ResidentInfo, NonResidentInfo, AttributeError,
    parse_attribute_header,
};
pub use attributes::{
    AttributeIterator, AttributeRef, find_attribute,
    StandardInformation, FileTime, FileAttributes, SiError,
    standard_information::parse_standard_information,
};
pub use boot_sector::{BootSector, BootSectorError, parse_boot_sector};
pub use mft::{MftEntry, MftEntryHeader, MftError, parse_mft_entry};
```

### 4. Cargo.toml 更新

`crates/fs-ntfs/Cargo.toml` に追加:

```toml
[dependencies]
# 既存に加えて:
chrono.workspace = true
```

## 単体テスト要件（最低8件）

### `attributes/mod.rs` のテスト（最低4件）

1. **空のイテレータ** - 先頭がEnd marker（`0xFFFFFFFF`）で即終了
2. **単一属性 + End** - 1つの常駐属性 + End marker、1回 yield して終了
3. **複数属性 + End** - 2つの属性 + End、2回 yield して終了
4. **`find_attribute` - 存在するタイプ** - `AttributeType::StandardInformation` で発見
5. **`find_attribute` - 存在しないタイプ** - 見つからない時 `None`

### `attributes/standard_information.rs` のテスト（最低4件）

6. **48バイト NT版パース成功** - 全フィールド検証、`owner_id`等は `None`
7. **72バイト W2K+版パース成功** - 拡張フィールドが `Some(値)` で取得
8. **バッファサイズ不足** - 47バイトで `BufferTooSmall`
9. **FileTime → DateTime 変換** - 既知の FILETIME 値（例: 2026年1月1日UTC）が正しい DateTime に変換される
10. **FileAttributes ビット判定** - 各 `is_*()` メソッドが正しく動作

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/standard_information_integration.rs` を作成:

1. **健全イメージのファイル群から $SI を取得**
   - `ntfs_healthy_small.img.zst` を解凍
   - ブートセクタ → $MFT 開始位置を取得
   - $MFT のエントリを順次（最低でも 24〜50 番目まで）走査
   - 各エントリに対し `find_attribute(AttributeType::StandardInformation)` を実行
   - 取得した $SI から `created.to_datetime()` を取り、ground truth JSON の `creation_date` と整合（同じ日付）

2. **削除エントリの $SI も取得可能**
   - `ntfs_with_5_deletions_small.img.zst` を解凍
   - `is_deleted() == true` のエントリでも $SI が読めることを確認
   - 削除エントリのタイムスタンプが取得可能（生存エントリと同様）

ヘルパーの `decompress_fixture(name)` を再利用。

## Cargo.toml 設定

`chrono.workspace = true` を `[dependencies]` に追加（workspace側で既に定義済み）。

## 制約

- 行数上限: **200行（実装+単体テスト合計）**、結合テストは別カウント可
- 単体テスト最低8件（実装ファイル別の合計）
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- `unwrap()` は `try_into().unwrap()` のバイト長保証部分のみ許容（事前の長さチェック後）

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥8件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥2件）
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り) の重要構成要素
- **FR-LIVE-06** (メタデータ表示) のタイムスタンプ部分

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次は **Chunk 8: NTFS `$FILE_NAME` 属性**（ファイル名取得！プロダクト価値の見える化マイルストーン）

---

## 注意事項

### FILETIME の値域外

FILETIME が 0 (= "未設定") や非常に大きな値（破損データ）で DateTime 変換が失敗する場合がある。`to_datetime()` は `Option<DateTime>` を返すので、呼び出し側で `unwrap_or_default()` 等で防御。

### コンテンツサイズで NT/W2K+ を判別

`content_size < 72` なら NT版（owner_id等は None）、`>= 72` なら W2K+。**バイト長で判別**するのが正しい（フラグやバージョン番号での判別は不可）。

### Iterator のライフタイム

`AttributeIterator<'a>` と `AttributeRef<'a>` のライフタイムは MFT エントリデータと同じ。MFT エントリのデータは `MftEntry::data` の所有データなので、`mft_entry.data.as_slice()` を渡せばOK。

### find_attribute のオーバーヘッド

`find_attribute` は線形探索だが、NTFS の属性数は通常10未満なので問題にならない。複数属性を取りたい場合は `AttributeIterator` を直接使う。

### 名前付き属性

このチャンクでは属性名は無視（`AttributeCommonHeader::name_length` と `name_offset` は持っているが、文字列として抽出するのは将来チャンク）。$STANDARD_INFORMATION は通常無名なので問題なし。

### $MFT エントリ自身のタイムスタンプ

$MFT エントリ（レコード番号0）自身にも $SI 属性があり、ファイルシステムの「作成日時」が入っている。結合テストでこれを使うと、フィクスチャ生成日と一致するはず。

---

## 質問が必要なケース

- FILETIME = 0 の場合の扱い（「未設定」表現か、エラーか）
- DateTime 変換失敗時のフォールバック方針
- 名前付き属性の優先度（Phase 1 範囲か外か）

---

## 完了時の報告例

```markdown
## Chunk 7 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: 
  - crates/fs-ntfs/src/attributes/mod.rs (新規, 75行 + テスト 30行)
  - crates/fs-ntfs/src/attributes/standard_information.rs (新規, 80行 + テスト 40行)
  - crates/fs-ntfs/src/lib.rs (更新)
- **行数合計**: 実装 155行 / 単体テスト 70行 / 合計 225行 ※若干超過、レビュー希望
- **結合テスト**: tests/standard_information_integration.rs に2件追加（60行）
- **公開API**: 
  - `AttributeIterator`, `AttributeRef`, `find_attribute`
  - `StandardInformation`, `FileTime`, `FileAttributes`, `SiError`
  - `parse_standard_information(bytes) -> Result<StandardInformation, SiError>`
- **単体テスト**: 10件パス
- **結合テスト**: 2件パス（健全/削除エントリ両方からタイムスタンプ取得成功）
- **発見した内容**: $MFT エントリ0 の作成日時 = 2026-05-XX（フィクスチャ生成日と一致）
- **関連FR**: FR-LIVE-01, FR-LIVE-06

→ tester エージェントへ引き継ぎお願いします
```
