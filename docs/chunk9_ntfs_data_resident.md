# Chunk 9 指示: $DATA 常駐属性パーサ（ファイル内容取得）

このチャンクでファイルの**実際の内容**が取れるようになります。Chunk 8 でファイル名が見え、このチャンクで内容が見える。**Chunk 9 完了時点で「削除ファイルを名前付きで完全復旧できる」状態**になります（小サイズファイルに限る）。

---

## 目的

`$DATA` 属性（タイプID 0x80）の**常駐版**を解析し、ファイルの内容バイト列を取得する:

- **無名 `$DATA` ストリーム**: ファイルの主たる内容
- **名前付き `$DATA` ストリーム** (Alternate Data Streams): 副ストリームへのアクセス（フォレンジック価値）
- 非常駐 `$DATA`（大きいファイル）は **Chunk 10** で対応するため、ここでは識別だけして処理を分岐

完了時点で、フィクスチャの全30ファイル（生存25 + 削除5）の**内容が完全復元できる**ようになります。

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 無料リソース（このチャンクで十分）

- Linux NTFS Documentation - $DATA: https://flatcap.github.io/linux-ntfs/ntfs/attributes/data.html
- libfsntfs ドキュメント

### $DATA 属性の特性

`$DATA` には専用のコンテンツ構造はない:

- **常駐属性**: コンテンツバイト列がそのままファイル内容（生バイト）
- **非常駐属性**: クラスタ番号のリスト（runlist、Chunk 10で対応）

### 常駐 vs 非常駐の判別

属性共通ヘッダ（Chunk 6）の `non_resident` フラグで判別:

- `non_resident = 0` (false) → 常駐、内容は `ResidentInfo::content_offset` から `content_size` バイト
- `non_resident = 1` (true) → 非常駐、`NonResidentInfo::runlist_offset` から runlist 取得

### 常駐の閾値（参考）

NTFS は「MFT エントリ内に収まるなら常駐、収まらなければ非常駐」というルール。1024バイトの MFT エントリで、他の属性も含めると、実用的に常駐になるのは **約 700 バイト以下**のファイル。

### Alternate Data Streams (ADS)

NTFS の特徴的な機能で、1つのファイルが複数の `$DATA` ストリームを持てる:

- 無名ストリーム（name_length = 0）: 通常のファイル内容（エクスプローラで見える）
- 名前付きストリーム（name_length > 0）: 隠しデータ（マルウェアが悪用するケースも）

例えばファイル `foo.txt` には以下が同居可能:
- 無名 `$DATA`: テキスト本文
- 名前付き `$DATA` (名前 "Zone.Identifier"): Windowsの「インターネットから取得した」フラグ
- 名前付き `$DATA` (名前 "secret"): 任意の隠しデータ

通常の復旧では**無名ストリームが主**だが、フォレンジック観点で**全ストリームを取得可能**にしておくと価値が高い（DDSの強み）。

### 属性名の取得方法

Chunk 6 でパース済みの `AttributeCommonHeader` から:
- `name_length`: UTF-16 コードユニット数
- `name_offset`: 属性先頭からの名前オフセット

属性名は UTF-16LE で格納されている（Chunk 8 の Filename と同じデコード方式）。

## 実装内容

### ファイル作成

`crates/fs-ntfs/src/attributes/data.rs` を新規作成。

### 1. データストリーム情報

```rust
use crate::attribute::{AttributeHeader, AttributeType};

/// $DATA 属性の常駐/非常駐の状態を表す。
#[derive(Debug, Clone)]
pub enum DataContent<'a> {
    /// 常駐: コンテンツがそのまま入っている
    Resident {
        bytes: &'a [u8],
        size: u32,
    },
    /// 非常駐: 実データはクラスタに散在（Chunk 10 で対応）
    NonResident {
        real_size: u64,
        allocated_size: u64,
        starting_vcn: u64,
        last_vcn: u64,
        /// 属性 raw データの中での runlist 開始オフセット
        runlist_offset_in_attr: usize,
        /// 属性 raw データ全体（runlist を読み取るため）
        attribute_raw: &'a [u8],
    },
}

impl<'a> DataContent<'a> {
    pub fn is_resident(&self) -> bool {
        matches!(self, DataContent::Resident { .. })
    }
    
    pub fn size(&self) -> u64 {
        match self {
            DataContent::Resident { size, .. } => *size as u64,
            DataContent::NonResident { real_size, .. } => *real_size,
        }
    }
}

/// データストリーム（名前付きまたは無名）
#[derive(Debug, Clone)]
pub struct DataStream<'a> {
    /// ストリーム名（空文字列 = 無名/メインストリーム）
    pub name: String,
    /// データ内容
    pub content: DataContent<'a>,
    /// 圧縮フラグ（attribute common header の flags より）
    pub is_compressed: bool,
    /// 暗号化フラグ
    pub is_encrypted: bool,
    /// スパースフラグ
    pub is_sparse: bool,
}
```

### 2. エラー型

```rust
#[derive(thiserror::Error, Debug)]
pub enum DataError {
    #[error("Buffer too small for resident data")]
    ResidentBufferTooSmall,
    
    #[error("Invalid resident content offset: {offset}")]
    InvalidContentOffset { offset: u16 },
    
    #[error("Invalid stream name encoding (UTF-16 error)")]
    InvalidStreamName,
}
```

### 3. ストリーム名抽出ヘルパー

```rust
use crate::attribute::AttributeCommonHeader;

/// 属性の名前を UTF-8 文字列として取り出す。
/// 名前なしなら空文字列を返す。
fn extract_attribute_name(attr_raw: &[u8], header: &AttributeCommonHeader) 
    -> Result<String, DataError> 
{
    if header.name_length == 0 {
        return Ok(String::new());
    }
    let name_offset = header.name_offset as usize;
    let name_byte_length = header.name_length as usize * 2;
    let end = name_offset + name_byte_length;
    
    if end > attr_raw.len() {
        return Err(DataError::InvalidStreamName);
    }
    
    let name_bytes = &attr_raw[name_offset..end];
    let utf16_chars: Vec<u16> = name_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&utf16_chars).map_err(|_| DataError::InvalidStreamName)
}
```

### 4. 単一の $DATA 属性からストリーム情報を抽出

```rust
use crate::attribute::ResidentInfo;
use crate::attribute::NonResidentInfo;

/// 1つの $DATA 属性（AttributeRef）からデータストリームを取り出す。
pub fn parse_data_stream<'a>(attr_raw: &'a [u8], header: &AttributeHeader) 
    -> Result<DataStream<'a>, DataError> 
{
    let common = header.common().ok_or(DataError::ResidentBufferTooSmall)?;
    
    if common.attribute_type != AttributeType::Data {
        // 呼び出し側のバグ
        return Err(DataError::ResidentBufferTooSmall);
    }
    
    let name = extract_attribute_name(attr_raw, common)?;
    let is_compressed = common.flags & 0x0001 != 0;
    let is_encrypted = common.flags & 0x4000 != 0;
    let is_sparse = common.flags & 0x8000 != 0;
    
    let content = match header {
        AttributeHeader::Resident { resident, .. } => {
            let start = resident.content_offset as usize;
            let end = start + resident.content_size as usize;
            if end > attr_raw.len() {
                return Err(DataError::ResidentBufferTooSmall);
            }
            DataContent::Resident {
                bytes: &attr_raw[start..end],
                size: resident.content_size,
            }
        }
        AttributeHeader::NonResident { non_resident, .. } => {
            DataContent::NonResident {
                real_size: non_resident.real_size,
                allocated_size: non_resident.allocated_size,
                starting_vcn: non_resident.starting_vcn,
                last_vcn: non_resident.last_vcn,
                runlist_offset_in_attr: non_resident.runlist_offset as usize,
                attribute_raw: attr_raw,
            }
        }
        AttributeHeader::End => unreachable!("End marker should not reach here"),
    };
    
    Ok(DataStream { name, content, is_compressed, is_encrypted, is_sparse })
}
```

### 5. MFT エントリから全 $DATA ストリームを抽出

```rust
use crate::attributes::AttributeIterator;

/// MFT エントリ内の全 $DATA ストリーム（無名 + 名前付き）を取り出す。
pub fn extract_all_data_streams<'a>(
    entry_data: &'a [u8],
    first_attribute_offset: usize,
) -> Vec<DataStream<'a>> {
    AttributeIterator::new(entry_data, first_attribute_offset)
        .filter_map(Result::ok)
        .filter(|attr| attr.header.attribute_type() == AttributeType::Data)
        .filter_map(|attr| parse_data_stream(attr.raw, &attr.header).ok())
        .collect()
}

/// MFT エントリから無名（メイン）$DATA ストリームを取り出す。
/// ファイルでなくディレクトリの場合は None（$DATA 属性なし）。
pub fn extract_main_data_stream<'a>(
    entry_data: &'a [u8],
    first_attribute_offset: usize,
) -> Option<DataStream<'a>> {
    extract_all_data_streams(entry_data, first_attribute_offset)
        .into_iter()
        .find(|stream| stream.name.is_empty())
}
```

### 6. attributes/mod.rs 更新

```rust
pub mod data;
pub mod file_name;
pub mod standard_information;

pub use data::{
    DataContent, DataStream, DataError,
    parse_data_stream, extract_all_data_streams, extract_main_data_stream,
};
pub use file_name::{...};
pub use standard_information::{...};
```

### 7. lib.rs 更新

`pub use attributes::{... DataContent, DataStream, DataError, extract_main_data_stream, ...};`

## 単体テスト要件（最低8件）

`data.rs` の同ファイル内 `#[cfg(test)] mod tests`:

1. **常駐 $DATA の内容抽出** - 内容バイト列が正しく取り出される
2. **空ファイル（content_size = 0）** - 空 slice を返す
3. **無名ストリームの判別** - `name.is_empty() == true`
4. **名前付きストリームのデコード** - 名前 "secret" がパースされる
5. **日本語名のストリーム** - 名前 "秘匿データ" の UTF-16 → String 変換
6. **DataContent::is_resident()** - 常駐/非常駐の判別が正しい
7. **DataContent::size()** - 常駐サイズ取得
8. **非常駐 $DATA の情報抽出** - real_size, runlist_offset 等が正しく取得
9. **複数 $DATA ストリームの抽出** - 無名 + 名前付き2つを持つMFTエントリ模擬で `extract_all_data_streams()` が3つ返す
10. **無名のみ抽出** - `extract_main_data_stream()` が無名のみ返す

テストデータ作成のため、Chunk 6 で作った属性ヘッダ構築ヘルパーを再利用または拡張。

## 結合テスト要件（フィクスチャ使用）

`crates/fs-ntfs/tests/data_integration.rs` を作成:

### 1. **健全イメージから全ファイル内容を取得**

- `ntfs_healthy_small.img.zst` を解凍
- $MFT を走査して、各ユーザファイル（24〜100番台のエントリ）を処理
- 各エントリで:
  - `find_best_file_name()` でファイル名取得
  - `extract_main_data_stream()` で $DATA 取得
  - 常駐確認（フィクスチャは小サイズなので全部常駐のはず）
  - 内容バイト列を取得
- ground truth JSON の `files[].content_hash_sha256` と SHA256 を比較
- **全30ファイル分のハッシュが一致**することを assert

### 2. **削除エントリの内容も復元成功**

- `ntfs_with_5_deletions_small.img.zst` を解凍
- 削除済みエントリ（`is_deleted() == true`）の $DATA を取得
- 内容ハッシュが ground truth と一致
- **これがプロダクト価値の最終実証**: 削除されたファイルが完全に復元できる

### 3. **総合プロダクトデモテスト** （`--nocapture` で見やすく出力）

```rust
#[test]
fn product_demo_complete_recovery() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let boot = parse_boot_sector(&img[..512]).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Phase 1 Demo ===\n");
    println!("Source: ntfs_with_5_deletions_small.img");
    println!("Cluster size: {} bytes", boot.cluster_size_bytes());
    println!("MFT location: byte {}\n", boot.mft_byte_offset());
    
    let mft_record_size = boot.mft_record_size_bytes() as usize;
    let mft_start = boot.mft_byte_offset() as usize;
    
    let mut recovered = 0;
    let mut deleted_recovered = 0;
    
    for entry_idx in 16..150 {
        let entry_offset = mft_start + entry_idx * mft_record_size;
        if entry_offset + mft_record_size > img.len() { break; }
        
        if let Ok(entry) = parse_mft_entry(&img[entry_offset..entry_offset + mft_record_size]) {
            if entry.header.first_attribute_offset == 0 { continue; }
            
            let file_name = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize);
            let data = extract_main_data_stream(&entry.data, entry.header.first_attribute_offset as usize);
            
            if let (Some(name), Some(stream)) = (file_name, data) {
                if name.filename.starts_with("file_") {
                    let status = if entry.header.is_deleted() { "[DELETED]" } else { "[Live]   " };
                    let size = stream.content.size();
                    println!("  {} {:<20} ({} bytes)", status, name.filename, size);
                    recovered += 1;
                    if entry.header.is_deleted() { deleted_recovered += 1; }
                }
            }
        }
    }
    
    println!("\n=== Summary ===");
    println!("Total files recovered: {}", recovered);
    println!("Deleted files recovered: {}", deleted_recovered);
    
    assert!(recovered >= 30, "Expected at least 30 files, got {}", recovered);
    assert!(deleted_recovered >= 5, "Expected at least 5 deleted files, got {}", deleted_recovered);
}
```

このテストは `cargo test --release -- --nocapture test_product_demo` で実行すると、**実際に発見・復元したファイルが目で見える**形で出力される。社内デモにそのまま使える出力。

## Cargo.toml 設定

変更不要（`sha2` は workspace に既存）。

## 制約

- 行数上限: **200行（実装+単体テスト合計）**、結合テストは別カウント
- 単体テスト最低8件
- 全公開 type/method に rustdoc コメント必須
- `unsafe` 使用禁止
- 非常駐 $DATA は**情報抽出だけ**、実データ取得は Chunk 10

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（単体テスト ≥8件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（結合テスト ≥3件）
- [ ] フィクスチャの全30ファイルが SHA256 一致で復元成功
- [ ] 削除5ファイルも内容が SHA256 一致で復元成功
- [ ] `cargo clippy -p dds-fs-ntfs -- -D warnings` がエラーなし
- [ ] rustdoc コメントが全公開APIに記述

## 関連FR要件

- **FR-LIVE-01** (NTFS読み取り)
- **FR-LIVE-04** (ファイルツリー構築) の前提
- **FR-REC-01** (目標優先抽出) の基盤
- **FR-REC-04** (データ整合性) ← SHA256 比較で検証可能になる

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1 の技術的な核心が完成**
4. 次は **Chunk 10: `$DATA` 非常駐属性（runlist）** ← ここから Carrier 本があると有利

---

## 注意事項

### Phase 1 では圧縮・暗号化は非対応

属性ヘッダの flags で「圧縮 (0x0001)」「暗号化 (0x4000)」が立っていても、**Phase 1 では復号せず生バイトを返す**設計。`is_compressed`、`is_encrypted` フィールドで呼び出し側に通知して、復旧結果レポートで「圧縮データ含む」と注記する。将来チャンクで対応。

### スパースファイル

`is_sparse = true` でも、常駐ならコンテンツバイトはそのまま読める（小サイズのスパースは稀）。非常駐スパースは Chunk 10 で対応。

### ADS（Alternate Data Streams）の表示

メイン用途では無名ストリームを優先するが、ADS が存在することを CS が見える化することは**フォレンジック価値**として大きい。レポート生成（後のチャンク）で「このファイルは ADS を持っています」と明示できる設計を意識。

### 内容サイズが 0 のファイル

空ファイルは `$DATA` 属性が存在しても `content_size = 0` になる、または `$DATA` 自体がないこともある。両ケースを正常処理として扱う。

### MFT エントリ内のサイズ制約

常駐 $DATA は最大でも MFT エントリサイズ（通常1024バイト）から他の属性を引いた残りに収まる。実用上 **700バイト程度が常駐の上限**。これより大きいファイルは自動的に非常駐になり、Chunk 10 で扱う。

### フィクスチャは全て常駐

`gen_ntfs_basic.py` で作った30ファイルは各 ~50バイトなので、**全て常駐 $DATA**になっている。Chunk 9 だけで完全復元できる。

---

## 質問が必要なケース

- `non_resident` フラグと `content_size` が矛盾する場合（破損データ疑い）の扱い
- 圧縮 $DATA に遭遇した時の警告レベル（エラーにすべきか、警告で処理続行か）
- ADS の表示優先度（無名のみか、全部表示か）

---

## 完了時の報告例

```markdown
## Chunk 9 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: 
  - crates/fs-ntfs/src/attributes/data.rs (新規, 100行 + テスト 80行)
  - crates/fs-ntfs/src/attributes/mod.rs (更新)
  - crates/fs-ntfs/src/lib.rs (更新)
- **行数**: 実装 100行 / 単体テスト 80行 / 合計 180行
- **結合テスト**: tests/data_integration.rs に3件追加（120行）
- **公開API**:
  - `DataContent` enum (Resident/NonResident)
  - `DataStream`, `DataError`
  - `parse_data_stream`, `extract_all_data_streams`, `extract_main_data_stream`
- **単体テスト**: 10件パス
- **結合テスト**: 3件パス
  - 健全イメージ: 30/30 ファイル SHA256 一致
  - 削除イメージ: 5/5 削除ファイル SHA256 一致
- **🎉 マイルストーン**: 削除ファイル5個の完全復元成功（名前+内容+タイムスタンプ）
- **関連FR**: FR-LIVE-01, FR-LIVE-04, FR-REC-01, FR-REC-04

### プロダクトデモ出力例:
```
=== DDS Recovery Workbench - Phase 1 Demo ===

Source: ntfs_with_5_deletions_small.img
Cluster size: 4096 bytes
MFT location: byte 16384

  [Live]    file_000.txt          (50 bytes)
  [Live]    file_001.txt          (50 bytes)
  ...
  [DELETED] file_003.txt          (50 bytes)  ← 完全復元!
  ...
  [DELETED] file_028.txt          (50 bytes)  ← 完全復元!

=== Summary ===
Total files recovered: 30
Deleted files recovered: 5
```

→ tester エージェントへ引き継ぎお願いします
```
