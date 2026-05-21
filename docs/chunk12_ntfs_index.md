# Chunk 12 指示: $INDEX_ROOT / $INDEX_ALLOCATION（ディレクトリエントリ）

このチャンクで **ディレクトリが持つ子ファイル一覧** が取れるようになります。NTFS のディレクトリは MFT エントリ + 専用のインデックス属性で構成され、B+ ツリー構造でファイル名を保持しています。

> 🎯 完了時点で「ディレクトリの MFT エントリから、そこに含まれる全子ファイル名 + MFT 参照のリスト」が取得可能に。Chunk 13 のフルパス再構築の素材完成。

---

## 目的

NTFS ディレクトリのインデックス構造を解析する:

1. **$INDEX_ROOT (0x90)** の解析: 常駐属性、B+ ツリーのルートノード
2. **$INDEX_ALLOCATION (0xA0)** の解析: 非常駐属性、INDX ブロックの集まり
3. **INDX ブロック** の解析: 1 つの B+ ツリーノードに相当、フィクサップあり
4. **インデックスエントリ** の抽出: 各エントリは子ファイルの MFT 参照 + $FILE_NAME 情報を持つ

注: B+ ツリー走査（再帰的にノードを辿る）と NtfsVolume への統合は **Chunk 13** で実装。本チャンクは **パース primitives** までに専念。

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 必読セクション（書籍）

- 書籍 **Chapter 12「INDEXES」**（p.~270 付近）: B+ ツリーの概念図、エントリ構造、フィクサップ
- 書籍 **Chapter 13「$INDEX_ROOT ATTRIBUTE」Table 13.13/13.14**: ヘッダ詳細レイアウト
- 書籍 **Chapter 13「$INDEX_ALLOCATION ATTRIBUTE」Table 13.15**: INDX ブロック構造
- 書籍 **Chapter 13「INDEX_ENTRY structure」Table 13.16/13.17**: エントリの詳細

### 補助参照

- `docs/specs/ntfs-references/notes.md` の Index 関連セクション（あれば）
- 既存実装: `attribute.rs` (属性ヘッダ), `attributes/file_name.rs` (FileName), `mft.rs` (フィクサップ参考)

### NTFS インデックス構造の階層

```
ディレクトリ MFT エントリ
  ├ $INDEX_ROOT (0x90, 常駐)
  │   ├ Standard Index Header (16 bytes)
  │   ├ Index Node Header (16 bytes)
  │   └ Index Entries 列（B+ ツリーのルートノードのエントリ群）
  │
  └ $INDEX_ALLOCATION (0xA0, 非常駐、大規模ディレクトリのみ)
      ├ runlist → 複数の INDX ブロック（各 4096 バイトが標準）
      └ 各 INDX ブロック:
          ├ "INDX" シグネチャ (4 bytes)
          ├ Update Sequence Offset (2 bytes)
          ├ Update Sequence Size (2 bytes)
          ├ LSN, VCN 等 (24 bytes)
          ├ Index Node Header (16 bytes)
          ├ Index Entries 列
          └ Update Sequence Array (末尾)
```

### Standard Index Header（$INDEX_ROOT 先頭 16 バイト）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 4 | Type identifier | 0x30 = `$FILE_NAME` インデックス（ディレクトリ用） |
| 0x04 | 4 | Collation rule | 並び替え規則（Phase 1 では参照のみ） |
| 0x08 | 4 | Bytes per index record | 通常 4096 |
| 0x0C | 1 | Clusters per index record | MFT と同じ符号付きエンコーディング |
| 0x0D | 3 | Padding | |

### Index Node Header（Standard Index Header の直後 16 バイト）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 4 | First entry offset | このヘッダ先頭からの相対オフセット |
| 0x04 | 4 | End of entries offset | 有効エントリ末尾 |
| 0x08 | 4 | End of buffer offset | アロケート済みバッファ末尾 |
| 0x0C | 1 | Flags | bit 0: has children（INDEX_ALLOCATION が存在） |
| 0x0D | 3 | Padding | |

### Index Entry 構造（可変長）

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 8 | Child file MFT reference | エントリ番号 (48bit) + シーケンス番号 (16bit) |
| 0x08 | 2 | Length of this entry | 次エントリへのオフセット用 |
| 0x0A | 2 | Length of file name stream | $FILE_NAME コンテンツのバイト長 |
| 0x0C | 4 | Flags | bit 0: 子ノード有 / bit 1: 最終エントリ |
| 0x10 | varies | File name stream | $FILE_NAME 属性コンテンツと同形式（最終エントリは省略） |
| 末尾 | 8 | Child VCN | flags bit 0 == 1 の時のみ、子 INDX ブロックの VCN |

**重要**: 最終エントリ (flags & 0x02) は「子ノードへのポインタのみ」で `file_name` を持たない。B+ ツリー走査のナビゲーション用。

### INDX ブロック構造（$INDEX_ALLOCATION 内の各ブロック）

書籍 Chapter 13 Table 13.15 より:

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 4 | Magic | `"INDX"` (0x49 0x4E 0x44 0x58) |
| 0x04 | 2 | USA offset | Update Sequence Array の開始オフセット |
| 0x06 | 2 | USA size | USN + フィクサップ値の合計ワード数 |
| 0x08 | 8 | LSN | $LogFile sequence number |
| 0x10 | 8 | VCN | このノードの VCN |
| 0x18 | 16 | Index Node Header | MFT エントリと同じ構造 |
| 0x28〜 | varies | Index Entries 列 | |
| 末尾 | varies | Update Sequence Array | フィクサップ用 |

**フィクサップは MFT エントリと同じ仕組み**: 各セクタ末尾 2 バイトを USN と比較・復元。**Chunk 5 で実装した `apply_fixup` ロジックを共有**する。

## 実装内容

### 事前リファクタ: フィクサップ共有化

Chunk 5 で `mft.rs` 内 private として実装した `apply_fixup` を、INDX ブロックでも再利用するために共有化する:

**選択肢**:
1. **`src/fixup.rs` を新規作成**してそこに `apply_fixup(bytes, usa_offset, usa_size, sector_size) -> Result<(), FixupError>` を移す（推奨）
2. `mft.rs` の `apply_fixup` を `pub(crate)` に公開して再利用

選択肢 1 が DRY 原則と将来の保守性で優れる。`mft.rs` の単体テストも維持しやすい。

`FixupError` は既存の `MftError::FixupMismatch` と同等のバリアントを持つ専用 enum を新設し、`MftError` と `IndexError` 両方が `#[from]` で取り込めるようにする:

```rust
// src/fixup.rs
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum FixupError {
    #[error("Buffer too small for fixup: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    
    #[error("Invalid USA offset: {offset}")]
    InvalidUsaOffset { offset: u16 },
    
    #[error("Invalid USA size: {size}")]
    InvalidUsaSize { size: u16 },
    
    #[error("Fixup mismatch at sector {sector}: expected USN 0x{expected:04X}, got 0x{got:04X}")]
    FixupMismatch { sector: usize, expected: u16, got: u16 },
}

pub fn apply_fixup(
    bytes: &mut [u8],
    usa_offset: u16,
    usa_size: u16,
    sector_size: u16,
) -> Result<(), FixupError>;
```

`MftError` 側で `Fixup(#[from] FixupError)` バリアントを追加し、既存テストとの互換性を保つ。

### モジュール配置

`crates/fs-ntfs/src/attributes/index.rs` を新規作成（既存 `attributes/` 配下の命名規約に整合）。

### 1. `IndexError` enum

```rust
use crate::attributes::file_name::FileNameError;
use crate::fixup::FixupError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("Buffer too small for index structure: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    
    #[error("Invalid INDX magic: expected 'INDX', got {got:?}")]
    InvalidIndxMagic { got: [u8; 4] },
    
    #[error("Invalid index entry type: expected 0x30 ($FILE_NAME), got 0x{got:08X}")]
    UnsupportedIndexType { got: u32 },
    
    #[error("Invalid entry offset: {offset} (must be within node body)")]
    InvalidEntryOffset { offset: u32 },
    
    #[error("Entry length zero (infinite loop protection)")]
    EntryLengthZero,
    
    #[error("Entry length exceeds node body: length={length}, remaining={remaining}")]
    EntryLengthExceedsBuffer { length: u16, remaining: usize },
    
    #[error("File name parse error: {0}")]
    FileName(#[from] FileNameError),
    
    #[error("Fixup error in INDX block: {0}")]
    Fixup(#[from] FixupError),
}
```

### 2. 構造体定義

```rust
use crate::attributes::file_name::{FileName, MftReference};

/// Index Node Header（$INDEX_ROOT / INDX ブロック共通の 16 バイトヘッダ）
#[derive(Debug, Clone, Copy)]
pub struct IndexNodeHeader {
    pub first_entry_offset: u32,
    pub end_of_entries_offset: u32,
    pub end_of_buffer_offset: u32,
    /// bit 0 set => このノードは子ノード（INDEX_ALLOCATION のブロック）を持つ
    pub flags: u8,
}

impl IndexNodeHeader {
    pub fn has_children(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

/// $INDEX_ROOT の解析結果（コンテンツ全体への参照を保持）
#[derive(Debug)]
pub struct IndexRoot<'a> {
    /// インデックスのタイプ（ディレクトリは 0x30）
    pub index_type: u32,
    pub collation_rule: u32,
    pub bytes_per_index_record: u32,
    /// MFT と同じ符号付きエンコーディング
    pub clusters_per_index_record: i8,
    pub node_header: IndexNodeHeader,
    /// Index Node Header 直後のエントリ列のバイト範囲
    pub node_body: &'a [u8],
}

/// $INDEX_ALLOCATION の各 INDX ブロックの解析結果
#[derive(Debug)]
pub struct IndxBlock {
    pub vcn: u64,
    pub node_header: IndexNodeHeader,
    /// フィクサップ適用済みのブロック全データ
    pub data: Vec<u8>,
    /// data 内での Index Node Header の開始オフセット
    pub node_header_offset: usize,
}

impl IndxBlock {
    /// Index Node Header 直後のエントリ列を返す
    pub fn node_body(&self) -> &[u8] {
        &self.data[self.node_header_offset + 16..]
    }
}

/// 単一のインデックスエントリ
#[derive(Debug, Clone)]
pub struct IndexEntry {
    /// 子ファイルの MFT 参照
    pub child_ref: MftReference,
    /// このエントリの全長（次エントリへのオフセット用）
    pub entry_length: u16,
    pub flags: u32,
    /// 通常エントリの $FILE_NAME 情報。最終エントリ (is_last) では None。
    pub file_name: Option<FileName>,
    /// 子 INDX ブロックの VCN（has_child_node の時のみ Some）
    pub child_vcn: Option<u64>,
}

impl IndexEntry {
    pub fn is_last(&self) -> bool {
        self.flags & 0x02 != 0
    }
    
    pub fn has_child_node(&self) -> bool {
        self.flags & 0x01 != 0
    }
}
```

### 3. パース関数

```rust
/// $INDEX_ROOT 属性のコンテンツ部分を解析する。
///
/// 書籍 Chapter 13 Table 13.13/13.14 に準拠。
/// 入力: 属性ヘッダを除いた純粋なコンテンツバイト列。
pub fn parse_index_root(bytes: &[u8]) -> Result<IndexRoot<'_>, IndexError> {
    // Standard Index Header (16 bytes) + Index Node Header (16 bytes) = 最低 32 バイト
    if bytes.len() < 32 {
        return Err(IndexError::BufferTooSmall { got: bytes.len(), need: 32 });
    }
    
    let index_type = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let collation_rule = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let bytes_per_index_record = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let clusters_per_index_record = bytes[12] as i8;
    
    if index_type != 0x30 {
        return Err(IndexError::UnsupportedIndexType { got: index_type });
    }
    
    let node_header = parse_index_node_header(&bytes[16..32])?;
    let node_body = &bytes[32..];
    
    Ok(IndexRoot {
        index_type,
        collation_rule,
        bytes_per_index_record,
        clusters_per_index_record,
        node_header,
        node_body,
    })
}

/// INDX ブロック全体を解析する（フィクサップ適用込み）。
///
/// 入力: $INDEX_ALLOCATION の runlist から読んだ 1 ブロック分のバイト列（通常 4096）。
/// 書籍 Chapter 13 Table 13.15 に準拠。
pub fn parse_indx_block(bytes: &[u8], sector_size: u16) -> Result<IndxBlock, IndexError> {
    if bytes.len() < 40 {  // magic(4) + usa_offset(2) + usa_size(2) + lsn(8) + vcn(8) + node_header(16)
        return Err(IndexError::BufferTooSmall { got: bytes.len(), need: 40 });
    }
    
    // Magic check
    let magic: [u8; 4] = bytes[0..4].try_into().unwrap();
    if &magic != b"INDX" {
        return Err(IndexError::InvalidIndxMagic { got: magic });
    }
    
    let usa_offset = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    let usa_size = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let vcn = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    
    // フィクサップ適用（Chunk 5 と共有ロジック）
    let mut data = bytes.to_vec();
    crate::fixup::apply_fixup(&mut data, usa_offset, usa_size, sector_size)?;
    
    // Node Header は offset 0x18 から
    let node_header = parse_index_node_header(&data[0x18..0x18 + 16])?;
    
    Ok(IndxBlock {
        vcn,
        node_header,
        data,
        node_header_offset: 0x18,
    })
}

/// 単一ノード内のエントリ列を順次解析する。
///
/// 入力: Index Node Header 直後の `node_body`。
/// 走査は「最終エントリ (flags & 0x02)」までで、それを含めて返す。
/// 最終エントリ以降のバイトは無視（B+ ツリーのナビゲーション用）。
pub fn parse_entries_in_node(node_body: &[u8]) -> Result<Vec<IndexEntry>, IndexError> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    
    loop {
        if cursor + 16 > node_body.len() {
            return Err(IndexError::BufferTooSmall {
                got: node_body.len() - cursor,
                need: 16,
            });
        }
        
        let child_ref_raw = u64::from_le_bytes(node_body[cursor..cursor + 8].try_into().unwrap());
        let entry_length = u16::from_le_bytes(node_body[cursor + 8..cursor + 10].try_into().unwrap());
        let fn_length = u16::from_le_bytes(node_body[cursor + 10..cursor + 12].try_into().unwrap());
        let flags = u32::from_le_bytes(node_body[cursor + 12..cursor + 16].try_into().unwrap());
        
        if entry_length == 0 {
            return Err(IndexError::EntryLengthZero);
        }
        if cursor + entry_length as usize > node_body.len() {
            return Err(IndexError::EntryLengthExceedsBuffer {
                length: entry_length,
                remaining: node_body.len() - cursor,
            });
        }
        
        let is_last = flags & 0x02 != 0;
        let has_child = flags & 0x01 != 0;
        
        let file_name = if !is_last && fn_length > 0 {
            let fn_start = cursor + 16;
            let fn_end = fn_start + fn_length as usize;
            Some(crate::attributes::file_name::parse_file_name(&node_body[fn_start..fn_end])?)
        } else {
            None
        };
        
        let child_vcn = if has_child {
            let vcn_offset = cursor + entry_length as usize - 8;
            Some(u64::from_le_bytes(node_body[vcn_offset..vcn_offset + 8].try_into().unwrap()))
        } else {
            None
        };
        
        entries.push(IndexEntry {
            child_ref: MftReference::from_raw(child_ref_raw),
            entry_length,
            flags,
            file_name,
            child_vcn,
        });
        
        cursor += entry_length as usize;
        
        if is_last {
            break;
        }
    }
    
    Ok(entries)
}

/// 内部ヘルパー: Index Node Header (16 bytes) のパース
fn parse_index_node_header(bytes: &[u8]) -> Result<IndexNodeHeader, IndexError> {
    if bytes.len() < 16 {
        return Err(IndexError::BufferTooSmall { got: bytes.len(), need: 16 });
    }
    Ok(IndexNodeHeader {
        first_entry_offset: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        end_of_entries_offset: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        end_of_buffer_offset: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        flags: bytes[12],
    })
}
```

### 4. attributes/mod.rs と lib.rs 更新

```rust
// attributes/mod.rs
pub mod data;
pub mod file_name;
pub mod index;  // 新規
pub mod runlist;
pub mod standard_information;

pub use index::{
    IndexRoot, IndxBlock, IndexNodeHeader, IndexEntry, IndexError,
    parse_index_root, parse_indx_block, parse_entries_in_node,
};
// ... (既存)
```

```rust
// lib.rs（fixup モジュールも公開）
pub mod fixup;  // 新規
pub use fixup::{FixupError, apply_fixup};
pub use attributes::{
    IndexRoot, IndxBlock, IndexNodeHeader, IndexEntry, IndexError,
    parse_index_root, parse_indx_block, parse_entries_in_node,
    // ... (既存)
};
```

## 単体テスト要件（最低 10 件）

### 必須テストケース

1. **`parse_index_root_minimal_valid_directory`**: 手書きの $INDEX_ROOT バイト列で全フィールド検証
2. **`parse_index_root_rejects_non_filename_type`**: index_type が 0x30 以外（例: 0x90）で `UnsupportedIndexType`
3. **`parse_index_root_buffer_too_small`**: 31 バイトで `BufferTooSmall`
4. **`parse_indx_block_with_valid_magic_and_fixup`**: 4096 バイトの合成 INDX ブロックでフィクサップ適用成功 + ヘッダ解析
5. **`parse_indx_block_rejects_invalid_magic`**: 先頭 4 バイトが `"INDX"` でない場合 `InvalidIndxMagic`
6. **`parse_indx_block_fixup_mismatch_propagates`**: セクタ末尾の USN を意図的に違う値にして `Fixup(FixupMismatch)` 伝播
7. **`parse_entries_single_terminal_entry`**: `is_last` フラグのみのエントリ（ファイル名なし）を正しく解析、`file_name = None`
8. **`parse_entries_multiple_with_filenames`**: 3 つのエントリ + 終端で全て正しく yield、最終以外は `file_name` 有り
9. **`parse_entries_zero_length_returns_error`**: entry_length=0 で `EntryLengthZero`（無限ループ防止）
10. **`parse_entries_length_exceeds_buffer_returns_error`**: entry_length が残りバイト超過で `EntryLengthExceedsBuffer`
11. **`parse_entries_with_child_node_vcn_extracted`**: flags bit 0 set のエントリで `child_vcn` が末尾 8 バイトから取得
12. **`book_chapter13_index_entry_example`**: 書籍 Chapter 13 Table 13.16 のサンプル値（具体的なバイト列がある場合）を再現

### フィクサップモジュールのテスト（共有化に伴う必須テスト）

13. **`fixup::tests::apply_fixup_basic_two_sector_record`**: Chunk 5 の既存テストを移植
14. **`fixup::tests::apply_fixup_propagates_mismatch_error`**: USN 不一致の検出

既存 `mft.rs` のフィクサップ関連テストは、`MftError::Fixup(FixupError::...)` 経由でアサートを書き換える（後方互換）。

### テストヘルパー

確立された build_* パターンに従い:

```rust
fn build_index_root_content(entries: &[(MftReference, &str, u8)]) -> Vec<u8> {
    // (child_ref, filename, namespace) のリストから $INDEX_ROOT コンテンツを構築
}

fn build_indx_block(vcn: u64, entries: &[...]) -> Vec<u8> {
    // 4096 バイトの INDX ブロックを構築（フィクサップも仕込む）
}
```

## 結合テスト要件（既存フィクスチャ活用）

`crates/fs-ntfs/tests/index_integration.rs` を作成:

### 1. **ルートディレクトリの $INDEX_ROOT 解析**

```rust
#[test]
fn root_directory_index_root_lists_user_files() {
    let img = decompress_fixture("ntfs_healthy_small");
    let mut volume = NtfsVolume::open(make_image_reader(img.clone(), ...)).unwrap();
    
    // ルートディレクトリは MFT エントリ 5
    let root_entry = volume.read_record(5).unwrap();
    
    // $INDEX_ROOT 属性を探す
    let index_root_attr = find_attribute(
        &root_entry.data,
        root_entry.header.first_attribute_offset as usize,
        AttributeType::IndexRoot,
    ).unwrap();
    
    // 常駐コンテンツを取得して parse_index_root
    let AttributeHeader::Resident { resident, .. } = &index_root_attr.header else { panic!() };
    let content = &index_root_attr.raw[resident.content_offset as usize..][..resident.content_size as usize];
    let index_root = parse_index_root(content).unwrap();
    
    assert_eq!(index_root.index_type, 0x30);
    
    let entries = parse_entries_in_node(index_root.node_body).unwrap();
    let file_names: Vec<String> = entries.iter()
        .filter_map(|e| e.file_name.as_ref().map(|fn_| fn_.filename.clone()))
        .collect();
    
    // file_000.txt 〜 file_029.txt が見えるはず（B+ ツリー走査前提でルートに収まる場合）
    // 30 ファイルなら $INDEX_ROOT 単独 or $INDEX_ALLOCATION 併用の可能性両方ある
    let user_files: Vec<&String> = file_names.iter().filter(|n| n.starts_with("file_")).collect();
    
    // ルート単独で全 30 ファイル取れるか、has_children == true なら一部
    if !index_root.node_header.has_children() {
        assert_eq!(user_files.len(), 30, "Small dir should hold all entries in root");
    } else {
        assert!(user_files.len() <= 30, "Some entries may be in $INDEX_ALLOCATION");
    }
}
```

### 2. **$INDEX_ALLOCATION の INDX ブロック解析**

```rust
#[test]
fn root_index_allocation_indx_blocks_parseable() {
    // ntfs_healthy_small のルートで $INDEX_ALLOCATION 属性があれば、
    // runlist を解析して各 INDX ブロックを取得・parse_indx_block でパース成功
    // フィクスチャに $INDEX_ALLOCATION がない場合は #[ignore] で skip マーク
}
```

### 3. **大ファイル fixture でのインデックス検証**

```rust
#[test]
fn large_files_fixture_has_index_allocation() {
    // ntfs_large_files の root で has_children を確認
    // 16 ファイル程度では足りない可能性があるので、
    // 必要なら新フィクスチャ ntfs_many_files (100+ files) を生成する設計を検討
}
```

### 4. **削除イメージのディレクトリエントリ**

```rust
#[test]
fn deleted_files_appear_or_disappear_in_index() {
    // ntfs_with_5_deletions のルートで削除ファイルがインデックスに残っているか確認
    // NTFS の動作: 削除すると通常はインデックスから除去される（MFT 自体は残る）
    // → このテストは「インデックスに 25 ファイル、MFT に 30 ファイル」を確認することになる
    // → これにより、削除ファイル復旧には MFT 直接走査が必須であることが実証される
}
```

このテスト 4 は **業務上重要な事実の検証**: インデックス（≒「ライブモードでファイラに見えるファイル」）と MFT 直接走査（≒「削除済み含む全エントリ」）の差を定量化する。

## Cargo.toml 設定

変更不要。

## 制約

- **行数上限**: **250 行（実装 + 単体テスト合計、複雑性考慮で緩和）**、結合テストは別カウント
  - 内訳目安: `attributes/index.rs` 200 行 + `src/fixup.rs` 50 行 + 既存 `mft.rs` の修正 -30 行
- **単体テスト最低 10 件**（共有化テスト含めて）
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、`from_be_bytes` 0 件、書き込み API 0 件**
- **`mft.rs` の既存テスト互換性維持**（`#[from]` 経由のリファクタなので破壊しない）

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（既存 + 新規 ≥10 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥3 件）
- [ ] `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc -p dds-fs-ntfs --no-deps`: 全公開 API に rustdoc
- [ ] `src/fixup.rs` 新設、`mft.rs` のフィクサップは `apply_fixup` を呼ぶ形に整理
- [ ] `grep -r 'unsafe\|from_be_bytes\|fn write' crates/fs-ntfs/src/` で 0 件

## 関連 FR 要件

- **FR-LIVE-04** (ファイルツリー構築) ← ディレクトリ構造の基盤完成
- **FR-LIVE-01** (NTFS読み取り) の補強

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次のステップ:
   - **Chunk 13**: `NtfsVolume::list_directory` + フルパス再構築（B+ ツリー走査 + 親 MFT 参照を辿る）
   - **Chunk 14**: `NtfsFile` 高レベル統合型（name + meta + content + path を統合）

---

## 注意事項

### B+ ツリー走査は Chunk 13 で

本チャンクでは **単一ノード内のエントリ列挙** までに専念。子ノード（`has_child_node` の VCN）への再帰的走査は Chunk 13 で `NtfsVolume::list_directory` として統合実装。これにより責務分離と単体テスト容易性を両立。

### フィクサップ共有化の波及範囲

`mft.rs` の `apply_fixup` 移動は **既存 13 単体テスト + 2 結合テストの命名変更** を伴う可能性。エラー型が `MftError::FixupMismatch` → `MftError::Fixup(FixupError::FixupMismatch)` に変わるため、テストアサーション修正が必要。リファクタ前に既存テスト一覧を grep で抽出し、機械的に置換する。

### Collation Rule は Phase 1 範囲外

書籍 Chapter 13 で言及される `collation_rule` の値（0x00, 0x01, 0x10, 0x11, 0x12 等）は B+ ツリーの並び替え規則を示すが、**Phase 1 では値を保持するだけ**。ソート済みであることを利用した最適化は将来。

### 削除ファイルとインデックスの関係

書籍 Chapter 12 の重要記述: **削除されたファイルは通常、ディレクトリインデックスから即時除去される**（MFT エントリ自体は In Use フラグを 0 にして残る）。

これは復旧ソフトの設計上**極めて重要**:
- **ライブモード**（インデックス経由）では削除ファイルが見えない
- **削除ファイル復旧**には MFT 直接走査（Chunk 11 の `NtfsVolume::iter_records`）が必要

結合テスト 4 でこの事実を検証することで、プロダクトのアーキテクチャ判断が裏付けられる。

### Inactive INDX ブロック

$INDEX_ALLOCATION の中には**未使用 (inactive)** な INDX ブロックも含まれることがある（$BITMAP 属性でその有無が記録される）。Phase 1 では「INDX マジックがあれば有効、なければスキップ」のシンプル戦略でOK。$BITMAP 連動は将来。

### Long Filename と Short Filename の重複

NTFS ディレクトリは同じファイルに対して Win32 と DOS 両方のインデックスエントリを持ち得る（Chunk 8 のハードリンク対応と同様）。`parse_entries_in_node` は全エントリを yield し、呼び出し側（Chunk 13）で重複排除する設計とする。

---

## 質問が必要なケース

- $INDEX_ROOT に attribute header を含めて parse_attribute_header を経由するか、コンテンツのみ受け取るか（既存パターンと整合性チェック）
- $INDEX_ALLOCATION が複数ランに分散している場合の読み込み（runlist → 各クラスタ → INDX ブロック単位で切り出し）の責務範囲
- inactive INDX ブロックの厳密検出を Phase 1 で必要か（$BITMAP との連動）

---

## 完了報告例

```markdown
## Chunk 12 完了報告

- **クレート**: dds-fs-ntfs
- **新規ファイル**: 
  - crates/fs-ntfs/src/fixup.rs (新規, 60行 + テスト 30行)
  - crates/fs-ntfs/src/attributes/index.rs (新規, 180行 + テスト 80行)
- **既存ファイル更新**:
  - crates/fs-ntfs/src/mft.rs (フィクサップを fixup.rs 呼び出しに変更、+5 / -25 行)
  - crates/fs-ntfs/src/attributes/mod.rs (re-export 追加)
  - crates/fs-ntfs/src/lib.rs (re-export 追加)
- **公開API追加**:
  - `IndexRoot`, `IndxBlock`, `IndexNodeHeader`, `IndexEntry`, `IndexError`
  - `parse_index_root`, `parse_indx_block`, `parse_entries_in_node`
  - `FixupError`, `apply_fixup`
- **テスト統計**:
  - 単体: 既存 102 + 新規 12 = **114 件 pass**
  - 結合: 既存 22 + 新規 4 = **26 件 pass**
- **品質**: clippy 0 warning, unsafe 0, 書き込み API 0
- **重要な実証結果**:
  ```
  ntfs_with_5_deletions_small の root ディレクトリ:
    インデックス経由: 25 ファイル（生存のみ）
    MFT 直接走査:    30 ファイル（削除5含む）
  → 削除復旧には MFT 走査が必須であることを定量実証
  ```
- **関連 FR**: FR-LIVE-04, FR-LIVE-01

→ tester エージェントへ引き継ぎお願いします
```
