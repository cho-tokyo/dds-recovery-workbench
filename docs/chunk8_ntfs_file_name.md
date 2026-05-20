# Chunk 8 指示: $FILE_NAME 属性パーサ

このチャンクで **削除されたファイルの名前** が取れるようになります。プロダクト価値が初めて可視化される、技術的に最も重要なマイルストーンです。

---

## 目的

`$FILE_NAME` 属性（タイプID 0x30）を解析し、以下を取得する:

- **ファイル名**（UTF-16LE エンコード、日本語含む）
- **親ディレクトリの MFT 参照**（後のパス再構築に使う）
- **ファイル名の名前空間**（Win32 / DOS / POSIX / Win32+DOS）
- **タイムスタンプとサイズ**（$STANDARD_INFORMATION と一部重複するが、こちらは作成時の値）

完了時点で、フィクスチャ画像から「`file_003.txt`（削除済み）」のような**実ファイル名**を抽出できるようになります。

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクで十分）

- Linux NTFS Documentation - $FILE_NAME: https://flatcap.github.io/linux-ntfs/ntfs/attributes/file_name.html
- libfsntfs ドキュメント

### $FILE_NAME 属性のコンテンツ構造（リトルエンディアン）

ヘッダ部分は固定66バイト、その後可変長のファイル名（UTF-16LE）:

| Offset | Size | フィールド | 備考 |
|---|---|---|---|
| 0x00 | 8 | **Parent directory MFT reference** | 下位48bit=エントリ番号、上位16bit=シーケンス番号 |
| 0x08 | 8 | Creation time | FILETIME |
| 0x10 | 8 | Last modification time | FILETIME |
| 0x18 | 8 | MFT modification time | FILETIME |
| 0x20 | 8 | Last access time | FILETIME |
| 0x28 | 8 | Allocated size | $FILE_NAME での値は通常0、$DATAで取得すべき |
| 0x30 | 8 | Real size | 同上 |
| 0x38 | 4 | File attribute flags | $STANDARD_INFORMATION と同じビット定義 |
| 0x3C | 4 | EA/Reparse value | 通常0 |
| 0x40 | 1 | **Filename length** | UTF-16 **コードユニット数**（バイト数の半分）|
| 0x41 | 1 | **Filename namespace** | 0/1/2/3、後述 |
| 0x42 | varies | **Filename (UTF-16LE)** | `filename_length * 2` バイト |

### Filename Namespace

| 値 | 名前 | 説明 | 表示優先度 |
|---|---|---|---|
| 0 | POSIX | Unixスタイル、大文字小文字区別、ほぼ任意の文字 | 低 |
| 1 | Win32 | Windowsロング名、大文字小文字非区別 | **高（推奨）** |
| 2 | DOS | 8.3短縮名（大文字、限定文字） | 最低（表示には使わない）|
| 3 | Win32+DOS | 1つの名前が Win32 と DOS 両方の規則を満たす | **高（推奨）** |

**重要**: ほとんどのファイルは `$FILE_NAME` 属性を **2つ持つ**:
- 1つ: Win32（ロング名、例: `IMG_2026年05月_重要書類.jpg`）
- 1つ: DOS（8.3短縮名、例: `IMG_~1.JPG`）

または、ロング名が 8.3 規則を満たす場合は namespace=3 で1つだけ。

**表示用には Win32 (1) または Win32+DOS (3) を選ぶ**、DOS (2) は補助情報として保持しておく程度。

### Parent Directory MFT Reference の構造

8バイトの値だが、以下のように分解する:

```
bits 63-48 (16bit): Sequence number  ← エントリ再利用検出用
bits 47-0  (48bit): MFT entry number ← 親ディレクトリのエントリ番号
```

ルートディレクトリ（NTFS の root）の MFT エントリ番号は通常 **5**（`.` ディレクトリ）。

### UTF-16LE のデコード

ファイル名は UTF-16 リトルエンディアン。各文字は2バイト:

```rust
let utf16_chars: Vec<u16> = bytes
    .chunks_exact(2)
    .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
    .collect();
let filename = String::from_utf16(&utf16_chars)
    .map_err(|_| FileNameError::InvalidUtf16)?;
```

サロゲートペアは Rust の `String::from_utf16` が自動処理。日本語・絵文字・サロゲート文字を含むファイル名にもこれで対応可能。

## 実装内容

### ファイル作成

`crates/fs-ntfs/src/attributes/file_name.rs` を新規作成。

### 1. 名前空間 enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileNameNamespace {
    Posix,        // 0
    Win32,        // 1
    Dos,          // 2
    Win32AndDos,  // 3
}

impl FileNameNamespace {
    pub fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Posix),
            1 => Some(Self::Win32),
            2 => Some(Self::Dos),
            3 => Some(Self::Win32AndDos),
            _ => None,
        }
    }
    
    /// 表示用に適した名前空間か（Win32 系を優先）
    pub fn is_preferred_for_display(&self) -> bool {
        matches!(self, Self::Win32 | Self::Win32AndDos | Self::Posix)
    }
}
```

### 2. MFT 参照構造体

```rust
/// MFT 参照: エントリ番号 + シーケンス番号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MftReference {
    pub entry_number: u64,    // 48bit実装、u64で保持
    pub sequence_number: u16,
}

impl MftReference {
    pub fn from_raw(raw: u64) -> Self {
        Self {
            entry_number: raw & 0x0000_FFFF_FFFF_FFFF,
            sequence_number: ((raw >> 48) & 0xFFFF) as u16,
        }
    }
    
    /// 親がルートディレクトリ（エントリ番号5）か
    pub fn is_root_directory(&self) -> bool {
        self.entry_number == 5
    }
}
```

### 3. FileName 構造体

```rust
use crate::attributes::standard_information::{FileTime, FileAttributes};

#[derive(Debug, Clone)]
pub struct FileName {
    pub parent_directory: MftReference,
    pub created: FileTime,
    pub modified: FileTime,
    pub mft_modified: FileTime,
    pub accessed: FileTime,
    pub allocated_size: u64,   // 通常 0、参考値
    pub real_size: u64,        // 通常 0、参考値
    pub file_attributes: FileAttributes,
    pub namespace: FileNameNamespace,
    pub filename: String,      // UTF-8 に変換済み（Rust String）
}
```

### 4. エラー型

```rust
#[derive(thiserror::Error, Debug)]
pub enum FileNameError {
    #[error("Buffer too small for $FILE_NAME: got {got}, need at least 66")]
    BufferTooSmall { got: usize },
    
    #[error("Buffer too small for filename: declared {declared} chars, got {got} bytes for chars")]
    FilenameBufferTooSmall { declared: u8, got: usize },
    
    #[error("Invalid filename namespace: {got}")]
    InvalidNamespace { got: u8 },
    
    #[error("Invalid UTF-16 in filename")]
    InvalidUtf16,
}
```

### 5. パース関数

```rust
/// $FILE_NAME 属性のコンテンツ部分をパースする。
pub fn parse_file_name(bytes: &[u8]) -> Result<FileName, FileNameError> {
    if bytes.len() < 66 {
        return Err(FileNameError::BufferTooSmall { got: bytes.len() });
    }
    
    let read_u64 = |off| u64::from_le_bytes(bytes[off..off+8].try_into().unwrap());
    let read_u32 = |off| u32::from_le_bytes(bytes[off..off+4].try_into().unwrap());
    
    let filename_length = bytes[0x40] as usize;
    let filename_byte_length = filename_length * 2;
    let namespace_raw = bytes[0x41];
    
    let namespace = FileNameNamespace::from_raw(namespace_raw)
        .ok_or(FileNameError::InvalidNamespace { got: namespace_raw })?;
    
    if bytes.len() < 0x42 + filename_byte_length {
        return Err(FileNameError::FilenameBufferTooSmall {
            declared: bytes[0x40],
            got: bytes.len() - 0x42,
        });
    }
    
    let filename_bytes = &bytes[0x42..0x42 + filename_byte_length];
    let utf16_chars: Vec<u16> = filename_bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let filename = String::from_utf16(&utf16_chars)
        .map_err(|_| FileNameError::InvalidUtf16)?;
    
    Ok(FileName {
        parent_directory: MftReference::from_raw(read_u64(0x00)),
        created: FileTime(read_u64(0x08)),
        modified: FileTime(read_u64(0x10)),
        mft_modified: FileTime(read_u64(0x18)),
        accessed: FileTime(read_u64(0x20)),
        allocated_size: read_u64(0x28),
        real_size: read_u64(0x30),
        file_attributes: FileAttributes(read_u32(0x38)),
        namespace,
        filename,
    })
}
```

### 6. ベストファイル名選択ヘルパー

MFT エントリ内に複数の `$FILE_NAME` がある場合、表示に最適なものを選ぶ:

```rust
use crate::attribute::AttributeType;
use crate::attributes::{AttributeIterator, find_attribute, AttributeRef};

/// MFT エントリ内の全 $FILE_NAME 属性から、表示に最適なものを選ぶ。
///
/// 優先順位:
/// 1. Win32 または Win32+DOS（ロング名）
/// 2. POSIX
/// 3. DOS（短縮名、最終手段）
pub fn find_best_file_name(
    entry_data: &[u8],
    first_attribute_offset: usize,
) -> Option<FileName> {
    let candidates: Vec<FileName> = AttributeIterator::new(entry_data, first_attribute_offset)
        .filter_map(Result::ok)
        .filter(|attr| attr.header.attribute_type() == AttributeType::FileName)
        .filter_map(|attr| {
            // 常駐属性のコンテンツを取り出してパース
            if let AttributeHeader::Resident { resident, .. } = &attr.header {
                let content_start = resident.content_offset as usize;
                let content_end = content_start + resident.content_size as usize;
                if content_end <= attr.raw.len() {
                    return parse_file_name(&attr.raw[content_start..content_end]).ok();
                }
            }
            None
        })
        .collect();
    
    // 優先度順に選ぶ
    candidates.iter()
        .find(|fn_attr| matches!(fn_attr.namespace, FileNameNamespace::Win32 | FileNameNamespace::Win32AndDos))
        .or_else(|| candidates.iter().find(|fn_attr| fn_attr.namespace == FileNameNamespace::Posix))
        .or_else(|| candidates.iter().find(|fn_attr| fn_attr.namespace == FileNameNamespace::Dos))
        .cloned()
}
```

### 7. attributes/mod.rs 更新

```rust
pub mod file_name;
pub mod standard_information;

pub use file_name::{FileName, FileNameNamespace, MftReference, FileNameError, parse_file_name, find_best_file_name};
pub use standard_information::{StandardInformation, FileTime, FileAttributes, SiError, parse_standard_information};
// ... (既存のAttributeIterator等)
```

### 8. lib.rs 更新

```rust
pub use attributes::{
    AttributeIterator, AttributeRef, find_attribute,
    FileName, FileNameNamespace, MftReference, FileNameError,
    parse_file_name, find_best_file_name,
    StandardInformation, FileTime, FileAttributes, SiError,
    parse_standard_information,
};
```

## 単体テスト要件（最低8件）

`file_name.rs` の同ファイル内 `#[cfg(test)] mod tests`:

1. **英字ファイル名のパース** - `"hello.txt"` を含むバッファで成功
2. **日本語ファイル名のパース** - `"報告書_山田.docx"` を含むバッファで成功（**重要**: 日本企業の実案件で必須）
3. **Win32 namespace** - namespace=1 のパース成功
4. **DOS namespace** - namespace=2 のパース成功
5. **Win32+DOS namespace** - namespace=3 のパース成功
6. **POSIX namespace** - namespace=0 のパース成功
7. **無効な namespace** - namespace=4 で `InvalidNamespace` エラー
8. **バッファサイズ不足** - 65バイトで `BufferTooSmall`
9. **ファイル名バッファ不足** - filename_length=10 だが実際は4文字分しかない場合の `FilenameBufferTooSmall`
10. **`MftReference::from_raw`** - 親ディレクトリ参照の bit 分解が正しい
11. **絵文字を含むファイル名** - サロゲートペアが正しく処理される（例: `"📁メモ.txt"`）

テストヘルパー `fn build_file_name_bytes(name: &str, namespace: u8) -> Vec<u8>` を用意すると楽。

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/file_name_integration.rs` を作成:

1. **健全イメージから全ファイル名を取得**
   - `ntfs_healthy_small.img.zst` を解凍
   - $MFT を走査（エントリ 24〜100 番台がユーザファイル）
   - 各エントリで `find_best_file_name()` を呼び出し
   - ground truth JSON の `files[].path` と一致するファイル名が**全て発見される**こと
   - 期待: `file_000.txt`, `file_001.txt`, ..., `file_029.txt` の30個

2. **削除エントリからもファイル名取得**
   - `ntfs_with_5_deletions_small.img.zst` を解凍
   - **削除済みエントリ**から `find_best_file_name()` でファイル名を取得
   - ground truth JSON の `is_deleted: true` のファイル（`file_003.txt`, `file_007.txt`, `file_015.txt`, `file_022.txt`, `file_028.txt`）が**削除済みフラグ付き**で発見される

3. **プロダクト価値の見える化テスト**
   - 出力例（stdout）:
     ```
     [Live] file_000.txt   (entry #34, parent: root)
     [Live] file_001.txt   (entry #35, parent: root)
     ...
     [DELETED] file_003.txt (entry #37, parent: root)  ← This is what we recover!
     ...
     ```
   - このテストは `cargo test --release -- --nocapture` で実行する想定で、assertion ではなく **stdout 出力の人手確認用**としてもOK

## Cargo.toml 設定

変更不要。

## 制約

- 行数上限: **220行（実装+単体テスト合計、若干緩和）**、結合テストは別カウント
- 単体テスト最低8件
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- 日本語・絵文字を含むファイル名のテスト**必須**（プロダクト要件）

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥8件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥2件）
- [ ] 日本語ファイル名テストが含まれている
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り)
- **FR-LIVE-05** (削除エントリ可視化) ← **このチャンクで初めて実用化**
- **FR-LIVE-06** (メタデータ表示)

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次は **Chunk 9: `$DATA` 常駐属性**（小さなファイルの内容取得）

---

## 注意事項

### Win32 と DOS の二重登録は標準動作

NTFS では1つのファイルが通常2つの $FILE_NAME 属性を持つ（Win32 + DOS）。`find_best_file_name()` で Win32 を優先選択することで、表示は1つのファイル名に統一できる。

### 日本語ファイル名は実案件で必須

DDS の顧客は日本企業中心なので、ファイル名に日本語が含まれるケースが大半。**UTF-16 → Rust String 変換が正しいか、必ず日本語テストで検証**すること。

### サロゲートペア

絵文字（U+10000 以降）や一部の珍しい漢字は UTF-16 でサロゲートペア（2つの u16 で1文字）になる。`String::from_utf16` は自動処理するので、`filename_length` は**コードユニット数**（u16の個数）であって**文字数**ではない点に注意。

### 親ディレクトリ参照の活用

`MftReference::entry_number` は将来 Chunk で「ディレクトリツリー再構築」をする時に使う。今はパースして保持するだけでOK。

### 削除エントリの $FILE_NAME も健在

NTFS の削除は In Use フラグを 0 にするだけで、属性データは消されない。**削除されたファイルからもファイル名・タイムスタンプが取れる**のがプロダクトの価値の根幹。結合テスト2番でこれが実証される。

### コンテンツ位置の特定

$FILE_NAME 属性は常駐属性なので、`ResidentInfo::content_offset` から `ResidentInfo::content_size` バイトがコンテンツ。`find_best_file_name()` の中で `attr.raw[content_start..content_end]` で取り出す。

### `allocated_size` / `real_size` は信用しない

$FILE_NAME 内のサイズフィールドは、ファイル作成時の値のスナップショットでファイルが更新されても更新されない。**実際のファイルサイズは $DATA 属性から取る**べき。

---

## 質問が必要なケース

- $FILE_NAME 属性が全く存在しないエントリ（メタファイル等）の扱い
- 巡回参照や不整合（例: 親ディレクトリが存在しない MFT 番号を指している）の検出
- 名前長が 255 文字を超える場合（NTFS 仕様では最大 255 char）

---

## 完了時の報告例

```markdown
## Chunk 8 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: 
  - crates/fs-ntfs/src/attributes/file_name.rs (新規, 130行 + テスト 80行)
  - crates/fs-ntfs/src/attributes/mod.rs (更新)
  - crates/fs-ntfs/src/lib.rs (更新)
- **行数**: 実装 130行 / 単体テスト 80行 / 合計 210行
- **結合テスト**: tests/file_name_integration.rs に3件追加（90行）
- **公開API**:
  - `FileName`, `FileNameNamespace`, `MftReference`, `FileNameError`
  - `parse_file_name(bytes) -> Result<FileName, FileNameError>`
  - `find_best_file_name(entry_data, offset) -> Option<FileName>`
- **単体テスト**: 11件パス（日本語・絵文字テスト含む）
- **結合テスト**: 3件パス
- **🎉 マイルストーン達成**: ntfs_with_5_deletions_small.img から
  削除された5ファイル（file_003.txt 等）の名前と削除タイムスタンプを取得成功

→ tester エージェントへ引き継ぎお願いします
```
