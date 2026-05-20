# Chunk 4-9 書籍突合レビュー結果サマリ（Chunk 10 着手前 外部 AI 共有用）

**対象クレート**: `dds-fs-ntfs`
**レビュー期間**: 2026-05-20
**参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752)、Chapter 11/12/13
**目的**: Chunk 10（NTFS `$DATA` 非常駐属性 + runlist 解析）着手前に、Chunk 4-9 のレビューを通じて確立された設計原則・テスト基盤・検証論理を外部 AI と共有する。

書籍の内容は引用せず、書籍のセクション名・Table 番号・ページ番号を事実情報として参照する形式で記述する。

---

## 1. 各 Chunk で書籍に従って追加・変更した検証ロジック

### Chunk 4: ブートセクタ（`crates/fs-ntfs/src/boot_sector.rs`）

書籍 Chapter 13「$BOOT FILE」セクション内 Table 13.18 を基準に突合。

- **`bytes_per_sector` 検証強化**: 既存の「0 でない、4096 以下」のみのチェックから、書籍が暗黙的に前提とする「2 の累乗かつ 256〜4096」へ厳格化。`is_pow2(u32) -> bool` 内部ヘルパで判定。
- **`sectors_per_cluster` 検証強化**: 既存の「0 でない」のみから、「2 の累乗かつ 1〜128」へ厳格化（一般的な NTFS の上限）。
- **`index_record_size_bytes(&self) -> u32` メソッド追加**: 書籍 Table 13.18 が明示する「MFT entry size と同じ符号付きエンコーディング」を Index Record にも適用。`mft_record_size_bytes` と同じ規則（負値 → `2^|N|`、正値 → `N * cluster_size`）を共有内部関数 `compute_record_size_bytes(raw: i8, cluster_size: u32) -> u32` で DRY 化。
- **定数化**: `MIN_BYTES_PER_SECTOR = 256`、`MAX_BYTES_PER_SECTOR = 4096`、`MAX_SECTORS_PER_CLUSTER = 128`。

### Chunk 5: MFT エントリ + フィクサップ（`crates/fs-ntfs/src/mft.rs`）

書籍 Chapter 13「FIXUP VALUES」「MFT ENTRIES (FILE RECORDS)」を基準に突合。

- **USA size 整合性検証追加**: 書籍が暗黙的に前提とする式 `usa_size == ceil(allocated_size / sector_size) + 1`（USN 1 ワード + 各セクタの fixup ワード）を `parse_mft_entry` 内で検証。式に合致しない場合は既存の `MftError::InvalidUsaSize` で拒絶。`allocated_size == 0` の場合は他の検証に委ねる安全分岐を入れる。
- **rustdoc 強化**: `usa_size` / `sequence_number` / `hard_link_count` の意味を書籍の説明（再利用検出・ハードリンク数）に沿って自前で言い換え。
- **既存実装の整合性確認**: BAAD シグネチャ → `BadEntry` 返却、各セクタ末尾 2 バイトを USN と比較してから USA[1..] で復元、という基本ロジックは書籍仕様と一致しており変更不要。

### Chunk 6: 属性ヘッダ（`crates/fs-ntfs/src/attribute.rs`）

書籍 Chapter 13 Table 13.2/13.3/13.4 を基準に突合。

- **実装本体への変更は不要**と判定。既存実装は Table 13.2（共通 16 バイト 7 フィールド）/ 13.3（常駐追加 `content_size`/`content_offset`）/ 13.4（非常駐追加 8 フィールド: `starting_vcn`/`last_vcn`/`runlist_offset`/`compression_unit_size`/`allocated_size`/`real_size`/`initialized_size`）と完全一致。属性タイプ enum 15 種（0x10〜0x100）+ `Unknown(u32)` + `End` も完全網羅。
- **テスト追加のみ**でリグレッション防止を強化（後述のセクション 4 参照）。

### Chunk 7: $STANDARD_INFORMATION + 属性イテレータ（`crates/fs-ntfs/src/attributes/standard_information.rs`, `crates/fs-ntfs/src/attributes/mod.rs`）

書籍 Chapter 13 Table 13.5/13.6 を基準に突合。

- **Flag ビット 7 種追加**: 書籍 Table 13.6 は 13 種を列挙するが、既存実装は 6 種のみだった。以下 7 種を `fa_bits!` マクロで定数 + `is_*` メソッドとして追加:
  - `DEVICE = 0x0040`
  - `NORMAL = 0x0080`
  - `TEMPORARY = 0x0100`
  - `SPARSE_FILE = 0x0200`
  - `REPARSE_POINT = 0x0400`
  - `OFFLINE = 0x1000`
  - `NOT_CONTENT_INDEXED = 0x2000`
  - 既存: `READ_ONLY/HIDDEN/SYSTEM/ARCHIVE/COMPRESSED/ENCRYPTED` + NTFS 独自 `DIRECTORY = 0x1000_0000`
- **Table 13.5 全 12 フィールドは完全一致**（過不足なし）。NT 版 48 バイト / W2K+ 拡張版 72 バイトの判別はバイト長で正しく実装済み。
- **FILETIME 変換**: `i64::try_from(u64)` → `checked_div` → `checked_sub` → `checked_mul` の 4 段 checked チェーンでオーバーフロー安全な実装を維持（書籍が示す変換式と整合）。
- **AttributeIterator 側**: 変更なし。書籍が言及する「属性は type ID 昇順、End マーカーで終端」という性質を結合テストで実フィクスチャに対し確認。

### Chunk 8: $FILE_NAME（`crates/fs-ntfs/src/attributes/file_name.rs`）

書籍 Chapter 13 Table 13.7/13.8 と Chapter 12「LINKS TO FILES AND DIRECTORIES」を基準に突合。

- **`reparse_value: u32` フィールド追加**: 書籍 Table 13.7 offset 60-63 で明示される 32bit フィールドが既存実装で未読だった。`FileName` 構造体に pub フィールドとして追加し、`parse_file_name` で `u32::from_le_bytes` で読み取り。通常ファイルは 0、Reparse Point（Mount Point 等）ではタグ値が入る。
- **ハードリンク対応 API 追加**: 書籍 Chapter 12 が言及する「ハードリンクごとに $FILE_NAME 属性が 1 つずつ存在」に対し、既存 `find_best_file_name` は最初の 1 つしか返さなかった。`find_all_file_names(entry_data, first_attribute_offset) -> Vec<FileName>` を新設し、ハードリンク・Win32+DOS 二重登録の全名前を列挙可能にした。
- **`find_best_file_name` のリファクタ**: 内部で `find_all_file_names` を呼ぶ形に変更（重複ロジック削減）。優先順位は維持: Win32 / Win32AndDos > Posix > Dos。
- **Namespace 4 種（Table 13.8）と UTF-16LE デコード**: 変更なし。`String::from_utf16` 非 lossy 変換を維持。

### Chunk 9: $DATA 常駐 + ADS（`crates/fs-ntfs/src/attributes/data.rs`）

書籍 Chapter 11/12/13 の $DATA / ADS セクションを基準に突合。

- **実装本体への変更は不要**と判定。書籍が示す本質（「$DATA はネイティブ構造なし、raw content」「無名 = メインストリーム / 名前付き = ADS」「~700 バイト超で probably 非常駐」「ADS 命名規則 `file.txt:streamname`」「暗号化と `$LOGGED_UTILITY_STREAM` の関連」）はすべて既存実装で満たされている。
- **テスト追加のみ**で典型 ADS（`Zone.Identifier` 等）と書籍 Figure 12.4（無名 + 暗号化 ADS 二重登録）のリグレッション防止を強化。

---

## 2. エラー型の設計パターン

### 全 Error enum の現在の variant 一覧

すべて `thiserror::Error` を派生、`Debug` 派生。比較が必要なものは `PartialEq, Eq` も派生。

#### `BootSectorError`（`crates/fs-ntfs/src/boot_sector.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `BufferTooSmall` | `{ got: usize }` | 入力が 512 バイト未満 |
| `InvalidOemId` | `{ got: [u8; 8] }` | offset 3..11 が `b"NTFS    "` でない |
| `InvalidSignature` | `{ got: u16 }` | 末尾 2 バイトが `0xAA55` でない |
| `InvalidBytesPerSector` | `{ got: u16 }` | 2 の累乗でない、または 256..=4096 範囲外 |
| `InvalidSectorsPerCluster` | `{ got: u8 }` | 2 の累乗でない、または 1..=128 範囲外 |

#### `MftError`（`crates/fs-ntfs/src/mft.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `BufferTooSmall` | `{ got, need: usize }` | 48 バイト未満 |
| `InvalidMagic` | `{ got: [u8; 4] }` | 先頭 4 バイトが `FILE` / `BAAD` でない |
| `BadEntry` | — | `BAAD` シグネチャ検出 |
| `InvalidUsaOffset` | `{ offset: u16 }` | USA offset がヘッダ内 (< 48) または範囲外 |
| `InvalidUsaSize` | `{ size: u16 }` | usa_size が `ceil(allocated/sector)+1` と不一致、または 0 |
| `FixupMismatch` | `{ sector: usize, expected: u16, got: u16 }` | セクタ末尾と USN 不一致 |
| `UsedExceedsAllocated` | `{ used, allocated: u32 }` | used_size > allocated_size の不整合 |

#### `AttributeError`（`crates/fs-ntfs/src/attribute.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `BufferTooSmall` | `{ got, need: usize }` | 4 バイト未満（type ID）または 16/24/64 バイト未満（共通/常駐/非常駐） |
| `InvalidLength` | `{ length: u32 }` | length == 0（無限ループ防止） |
| `InvalidNonResidentFlag` | `{ got: u8 }` | offset 8 が 0/1 以外 |

#### `SiError`（`crates/fs-ntfs/src/attributes/standard_information.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `BufferTooSmall` | `{ got: usize }` | 48 バイト未満 |

#### `FileNameError`（`crates/fs-ntfs/src/attributes/file_name.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `BufferTooSmall` | `{ got: usize }` | 66 バイト未満（固定ヘッダ） |
| `FilenameBufferTooSmall` | `{ declared: u8, got: usize }` | filename_length × 2 がバッファに収まらない |
| `InvalidNamespace` | `{ got: u8 }` | namespace バイトが 0..=3 以外 |
| `InvalidUtf16` | — | UTF-16 サロゲートペア不正 |

#### `DataError`（`crates/fs-ntfs/src/attributes/data.rs`）
| Variant | フィールド | 主な発生条件 |
|---|---|---|
| `ResidentBufferTooSmall` | — | 常駐コンテンツがバッファ範囲外、または非 $DATA 属性を渡された |
| `InvalidContentOffset` | `{ offset: u16 }` | content_offset がバッファサイズを超える |
| `InvalidStreamName` | — | ストリーム名（ADS 名）の UTF-16 デコード失敗または範囲外 |

### エラーメッセージの命名規約

全 `#[error("...")]` 属性に共通する規約:

1. **構造化された英文メッセージ**: 「`<原因>: <観測値>` 形式」「`expected X, got Y` 形式」「`<制約>: <観測値>` 形式」のいずれかを使う。
2. **観測値は本文に埋め込む**: `{got}` `{length}` `{offset}` 等のフィールド参照を `format!` で展開。バイト列は `{got:?}` で 16 進ダンプ風表示、u16/u32 は `{got:04X}` / `{got:08X}` で大文字 16 進。
3. **範囲指定の明示**: 「`got X, need at least Y`」「`must be > 0`」「`must be 0 or 1`」のように許容範囲を本文に書く。
4. **ユーザフレンドリー**: CS が読むことを想定し、変数名は CamelCase ではなく自然な英語（「Buffer too small」「Invalid OEM ID」「BAAD MFT entry: data corruption detected」）。
5. **エラー型名のサフィックスは `Error`**: `BootSectorError` / `MftError` / `AttributeError` / `SiError` / `FileNameError` / `DataError`。

### Variant 命名規約

- 「不足系」: `BufferTooSmall` / `FilenameBufferTooSmall` / `ResidentBufferTooSmall`
- 「不正系」: `Invalid*`（`InvalidMagic` / `InvalidOemId` / `InvalidSignature` / `InvalidUsaOffset` / `InvalidUsaSize` / `InvalidContentOffset` / `InvalidNonResidentFlag` / `InvalidNamespace` / `InvalidStreamName` / `InvalidUtf16` / `InvalidLength` / `InvalidBytesPerSector` / `InvalidSectorsPerCluster`）
- 「不整合系」: `*Mismatch` / `*ExceedsAllocated`（`FixupMismatch` / `UsedExceedsAllocated`）
- 「特殊状態」: 名詞句単独（`BadEntry`）

---

## 3. テストコードのヘルパー関数

### `crates/fs-ntfs/src/*.rs` 内の単体テスト用ビルダー関数

すべて `#[cfg(test)] mod tests` 内に private 配置。

#### `mft.rs`
```rust
fn build_valid_mft_entry(flags: u16, usn: u16, fx0: u16, fx1: u16) -> Vec<u8>
```
1024 バイトの有効な MFT エントリを構築（`FILE` シグネチャ、USA offset=0x30、USA size=3、used=512、allocated=1024、各セクタ末尾に USN を配置）。テスト側はバイナリ詳細を意識せず、フラグと USN/fixup 値だけで MFT エントリを生成できる。

#### `boot_sector.rs`
```rust
fn make_valid_boot_sector() -> [u8; 512]
```
512 バイトの有効なブートセクタを構築（OEM `"NTFS    "`、bytes_per_sector=512、spc=8、total_sectors=2_048_000、mft_lcn=4、mft_mirror_lcn=1024、cpmr=-10、cpir=1、signature=0xAA55）。

#### `attribute.rs`
```rust
fn br(type_id: u32, length: u32) -> Vec<u8>   // 常駐属性 1 件分のバイト列
fn bnr(type_id: u32, real_size: u64) -> Vec<u8>   // 非常駐属性 1 件分のバイト列
```
属性ヘッダパーサ単体テスト用。`br` は ResidentInfo を埋めた resident 属性、`bnr` は NonResidentInfo（real_size を指定）を埋めた non-resident 属性を返す。

#### `attributes/mod.rs`
```rust
fn resident(type_id: u32, length: u32) -> Vec<u8>
fn entry(attrs: &[Vec<u8>]) -> Vec<u8>
```
イテレータ単体テスト用。`resident` は属性 1 件、`entry` は複数属性 + End マーカー（`0xFFFF_FFFF`）+ パディングを連結。

#### `attributes/standard_information.rs`
```rust
fn build_si(ext: bool) -> Vec<u8>
```
$SI コンテンツバイト列を構築。`ext=true` で W2K+ 拡張部（72 バイト）、`false` で NT 版（48 バイト）。4 タイムスタンプは `FT_2026` 定数（2026-01-01 UTC）を基準に微小オフセット。

#### `attributes/file_name.rs`
```rust
fn build_file_name_bytes(name: &str, namespace: u8, parent_ref_raw: u64) -> Vec<u8>
fn build_resident_fn_attr(name: &str, namespace: u8, parent: u64) -> Vec<u8>
fn build_entry(attrs: &[Vec<u8>]) -> Vec<u8>
```
- `build_file_name_bytes`: $FILE_NAME コンテンツのみ（ヘッダなし）
- `build_resident_fn_attr`: ヘッダ込みの常駐 $FILE_NAME 属性 1 件
- `build_entry`: 複数属性 + End マーカー + パディング連結

#### `attributes/data.rs`
```rust
fn ch(name_len: u8, name_offset: u16, flags: u16, length: u32, non_resident: bool) -> AttributeCommonHeader
fn put_name(buf: &mut [u8], offset: usize, utf16: &[u16])
fn br(name: &str, content: &[u8], flags: u16) -> (Vec<u8>, AttributeHeader)
fn bnr(name: &str, real_size: u64) -> (Vec<u8>, AttributeHeader)
fn cat(parts: &[&[u8]]) -> Vec<u8>
```
$DATA 単体テスト用。`br`/`bnr` は raw バイト列とパース済み `AttributeHeader` のタプルを返す（ヘッダ生成と raw 構築を一括）。`cat` は複数属性 + End マーカー + パディングを連結（属性巡回テスト用）。

### `crates/fs-ntfs/tests/*.rs` 内の結合テスト用ヘルパー

#### `tests/common/mod.rs`（全結合テストから利用）
```rust
pub fn decompress_fixture(name: &str) -> Vec<u8>
pub fn load_ground_truth(name: &str) -> serde_json::Value
```
- `decompress_fixture`: `fixtures/images/<name>.img.zst` を zstd 解凍して `Vec<u8>` 返却
- `load_ground_truth`: 同名 `.json` を読み込み `serde_json::Value` 返却

#### `tests/mft_integration.rs` / `tests/file_name_integration.rs` / `tests/standard_information_integration.rs`
```rust
fn read_record(img: &[u8], idx: usize) -> Option<MftEntry>
```
3 ファイルで重複定義（コピペ）。MFT エントリ idx を読み出し、`FILE` シグネチャを確認してから `parse_mft_entry` で返す。範囲外/シグネチャ不一致は `None`。

#### `tests/attribute_integration.rs`
```rust
fn collect_attribute_types_for_record(img: &[u8], record_index: usize) -> Vec<AttributeType>
```
指定 MFT レコードの属性タイプを順次列挙して `Vec` 返却（End マーカー含む）。

#### `tests/data_integration.rs`
```rust
fn read_record(img: &[u8], idx: usize) -> Option<MftEntry>
fn sha256_hex(bytes: &[u8]) -> String
fn collect_recovered_hashes(img: &[u8]) -> HashMap<String, String>
```
- `sha256_hex`: バイト列の SHA256 を 64 文字 16 進文字列に
- `collect_recovered_hashes`: `file_*` 形式のユーザファイルを全 MFT 走査で抽出し、`{filename → sha256}` の HashMap を返す。SHA256 ground truth 比較の中核。

#### `tests/file_name_integration.rs`
```rust
fn collect_user_files(img: &[u8]) -> Vec<(usize, bool, String)>
```
MFT 走査で `file_*` 形式の名前を持つエントリを集め、`(entry_idx, is_deleted, filename)` の 3 タプルを返す。`product_demo_complete_recovery` テストの出力源。

---

## 4. 書籍由来のテストケース

書籍の例題（バイト列、フィールド値、Figure）をテスト化した箇所。テスト関数名と参照書籍セクションを一覧化。

### Chunk 4 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `boot_sector::tests::book_example_512_byte_sector_2_spc_1kb_cluster` | Chapter 13「$BOOT FILE」内 Table 13.18 サンプル | bps=512, spc=2 → cluster=1024、cpmr=1 → MFT record=1024、cpir=4 → index record=4096、serial=0x0450_2284_5022_7C94 |
| `boot_sector::tests::parses_4kn_drive_with_4096_byte_sectors` | 同上（Advanced Format 4Kn のエッジケース） | bps=4096, spc=1 → cluster=4096 |

### Chunk 5 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `mft::tests::book_example_signature_0x0058_applies_fixup` | Chapter 13「FIXUP VALUES」+ Figure 13.1 + 例示 USN=0x0058 | USN=0x0058、USA size=3、record=1024、sector=512、fixup=0x0000 ×2 を適用後にセクタ末尾が復元 |
| `mft::tests::parses_2kb_entry_with_four_fixups` | 同章 マルチセクタ整合性 | allocated_size=2048、usa_size=5（USN + 4 fixup）、4 セクタ全てに USN 配置 |
| `mft::tests::partial_corruption_detected_at_second_sector` | 同章「one sector damaged」記述 | sector 0 末尾は USN 一致、sector 1 で不一致 → `FixupMismatch { sector: 1, .. }` |

### Chunk 6 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `attribute::tests::book_example_si_resident_96_byte_attribute` | Chapter 13「ATTRIBUTE HEADER」内 Table 13.3 サンプル ($SI) | type=0x10、length=0x60、content_size=0x48、content_offset=0x18、サニティ式 0x18+0x48=0x60 |
| `attribute::tests::book_example_data_nonresident_with_runlist` | Chapter 13 Table 13.4 サンプル ($DATA non-resident) | type=0x80、starting_vcn=0、last_vcn=0x20EF (=8431)、runlist_offset=0x40、allocated/real/initialized=0x83C000 (=8634368) |
| `attribute::tests::all_attribute_types_roundtrip_including_unknown_and_end` | Chapter 13 全タイプ ID 列挙 | 15 種 (0x10〜0x100) + Unknown 3 種 + End ラウンドトリップ |
| `attribute::tests::flag_bit_combinations_preserved_as_raw_value` | Chapter 13 Table 13.6 (0x0001/0x4000/0x8000) | 5 パターンの組合せで生値保持 |

### Chunk 7 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `attributes::standard_information::tests::book_example_mft_standard_information` | Chapter 13「$STANDARD_INFORMATION ATTRIBUTE」内 $MFT 自身の $SI サンプル | flags=0x06 (HIDDEN+SYSTEM)、4 タイムスタンプ同一値、max_versions=0、class_id=0、owner_id=0、security_id=1 |
| `attributes::standard_information::tests::extended_file_attribute_bits_book_table_13_6` | Chapter 13 Table 13.6 完全列挙 | 7 個の新規ビット (DEVICE/NORMAL/TEMPORARY/SPARSE_FILE/REPARSE_POINT/OFFLINE/NOT_CONTENT_INDEXED) を個別検証 |
| `attributes::standard_information::tests::filetime_overflow_safely_returns_none` | Chapter 13「one hundred nanoseconds since 1601」記述 | `u64::MAX` で `to_datetime()` が `None` 返却（パニックなし） |

### Chunk 8 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `attributes::file_name::tests::book_example_mft_self_file_name` | Chapter 13「$FILE_NAME ATTRIBUTE」内 $MFT 自身の $FILE_NAME サンプル | parent ref raw=0x0005_0000_0000_0005 (entry=5/seq=5、root)、name="$MFT"、namespace=Win32&DOS、allocated_size=real_size=0x4000 |
| `attributes::file_name::tests::book_example_dual_filename_win32_and_dos` | Chapter 13 entry 5009 の Win32+DOS 二重登録例 | "57398408d01" (Win32) + "573984~1" (DOS)、`find_all_file_names` 2 件、`find_best_file_name` で Win32 選択 |
| `attributes::file_name::tests::find_all_file_names_returns_multiple_hardlinks` | Chapter 12「LINKS TO FILES AND DIRECTORIES」 | 3 ハードリンク名すべて取得 |
| `attributes::file_name::tests::reparse_value_field_is_parsed` | Chapter 13 Table 13.7 offset 60-63 | Mount Point タグ `0xA0000003` と 0 の両方を確認 |

### Chunk 9 由来
| テスト関数 | 参照書籍セクション | 検証内容 |
|---|---|---|
| `attributes::data::tests::zone_identifier_ads_name_decoded` | Chapter 12「$DATA ATTRIBUTE」内 ADS 命名例（Zone.Identifier 言及） | 無名 $DATA + ADS "Zone.Identifier" の連結、`extract_main_data_stream` で無名取得、`extract_all_data_streams` で 2 件取得 |
| `attributes::data::tests::book_figure_12_4_dual_encrypted_data_streams` | Chapter 12 Figure 12.4「2 つの $DATA（無名 + ADS "ADS"、両方暗号化）」 | 無名 + ADS "ADS"、両方 `is_encrypted == true`、`extract_main_data_stream` で無名選択 |

---

## 5. モジュール構造の現状

### `crates/fs-ntfs/src/` の全 `.rs` ファイル（責務一行サマリ）

| ファイル | 行数 | 責務 |
|---|---|---|
| `lib.rs` | 24 | クレートエントリ。各モジュール `pub mod` 宣言 + 全公開 API の re-export |
| `boot_sector.rs` | 247 | NTFS Volume Boot Record（VBR）パーサ。`BootSector` 構造体 + `parse_boot_sector` + 派生メソッド 4 種（cluster/MFT/Index/byte offset） |
| `mft.rs` | 243 | MFT エントリ（FILE レコード）ヘッダパーサ + フィクサップ（Update Sequence）適用。`MftEntryHeader` + `MftEntry { header, data: Vec<u8> }` + `parse_mft_entry` + 4 判定メソッド |
| `attribute.rs` | 247 | NTFS 属性共通ヘッダパーサ。`AttributeType` enum（17 バリアント）+ `AttributeCommonHeader` + `ResidentInfo` + `NonResidentInfo` + `AttributeHeader` enum + `parse_attribute_header` |
| `attributes/mod.rs` | 93 | 属性巡回イテレータ + 検索ヘルパ。`AttributeIterator` + `AttributeRef<'a>` + `find_attribute` |
| `attributes/standard_information.rs` | 152 | $STANDARD_INFORMATION (0x10) コンテンツパーサ。`FileTime` + `FileAttributes`（14 ビット定数 + 14 `is_*` メソッド）+ `StandardInformation` + `parse_standard_information` |
| `attributes/file_name.rs` | 258 | $FILE_NAME (0x30) コンテンツパーサ + ベスト名選択。`FileNameNamespace` + `MftReference` + `FileName` + `parse_file_name` + `find_best_file_name` + `find_all_file_names` |
| `attributes/data.rs` | 206 | $DATA (0x80) コンテンツパーサ + ADS 列挙。`DataContent<'a>` enum (Resident/NonResident) + `DataStream<'a>` + `parse_data_stream` + `extract_all_data_streams` + `extract_main_data_stream` |

合計 src 行数: **1470**

### `lib.rs` の公開 API 一覧（re-export ベース）

**`boot_sector::`** から:
- `BootSector` (構造体)
- `BootSectorError` (enum)
- `parse_boot_sector(bytes: &[u8]) -> Result<BootSector, BootSectorError>`

**`mft::`** から:
- `MftEntry` (構造体: `header`, `data: Vec<u8>`)
- `MftEntryHeader` (構造体: 12 pub フィールド)
- `MftError` (enum)
- `parse_mft_entry(bytes: &[u8]) -> Result<MftEntry, MftError>`

**`attribute::`** から:
- `AttributeType` (enum: 17 バリアント、`from_raw`/`to_raw`)
- `AttributeHeader` (enum: Resident/NonResident/End、`common`/`length`/`attribute_type`/`is_end`)
- `AttributeCommonHeader` (構造体)
- `ResidentInfo` (構造体)
- `NonResidentInfo` (構造体)
- `AttributeError` (enum)
- `parse_attribute_header(bytes: &[u8]) -> Result<AttributeHeader, AttributeError>`

**`attributes::` モジュール直下**から:
- `AttributeIterator<'a>` (構造体: `new`)
- `AttributeRef<'a>` (構造体: `header`, `raw`, `offset_in_entry`)
- `find_attribute(entry_data, first_attribute_offset, target_type) -> Option<AttributeRef>`

**`attributes::standard_information::`** から:
- `StandardInformation` (構造体: 12 pub フィールド、W2K+ 拡張は `Option`)
- `FileTime` (構造体: `pub u64`、`to_datetime() -> Option<DateTime<Utc>>`)
- `FileAttributes` (構造体: `pub u32`、14 定数 + 14 `is_*` メソッド)
- `SiError` (enum)
- `parse_standard_information(bytes: &[u8]) -> Result<StandardInformation, SiError>`

**`attributes::file_name::`** から:
- `FileName` (構造体: 11 pub フィールド、`reparse_value` 含む)
- `FileNameNamespace` (enum: Posix/Win32/Dos/Win32AndDos、`from_raw`/`is_preferred_for_display`)
- `MftReference` (構造体: `entry_number: u64` 48bit + `sequence_number: u16`、`from_raw`/`is_root_directory`)
- `FileNameError` (enum)
- `parse_file_name(bytes: &[u8]) -> Result<FileName, FileNameError>`
- `find_best_file_name(entry_data, first_attribute_offset) -> Option<FileName>`
- `find_all_file_names(entry_data, first_attribute_offset) -> Vec<FileName>`

**`attributes::data::`** から:
- `DataStream<'a>` (構造体: `name`, `content`, `is_compressed`, `is_encrypted`, `is_sparse`)
- `DataContent<'a>` (enum: Resident { bytes, size } / NonResident { real_size, allocated_size, starting_vcn, last_vcn, runlist_offset_in_attr, attribute_raw })
- `DataError` (enum)
- `parse_data_stream<'a>(attr_raw, header) -> Result<DataStream<'a>, DataError>`
- `extract_all_data_streams<'a>(entry_data, first_attribute_offset) -> Vec<DataStream<'a>>`
- `extract_main_data_stream<'a>(entry_data, first_attribute_offset) -> Option<DataStream<'a>>`

### 依存方向（クレート内）

```
lib.rs
 ├─ boot_sector.rs    (依存: dds-core, thiserror)
 ├─ mft.rs            (依存: dds-core, thiserror)
 ├─ attribute.rs      (依存: dds-core, thiserror)
 └─ attributes/
     ├─ mod.rs        (依存: attribute)
     ├─ standard_information.rs  (依存: chrono, thiserror)
     ├─ file_name.rs  (依存: attribute, attributes/mod, standard_information, thiserror)
     └─ data.rs       (依存: attribute, attributes/mod, thiserror)
```

`attributes/file_name.rs` と `attributes/data.rs` は `AttributeIterator` を活用するため `attributes::` 直下に配置。`file_name.rs` は `FileTime` / `FileAttributes` を `standard_information` から借りる。

---

## 6. 進捗と統計

### 各 Chunk の最終行数とテスト数（書籍突合レビュー反映後）

| Chunk | 主要ファイル | 行数 | 単体テスト | 結合テスト |
|---|---|---|---|---|
| 4 | `boot_sector.rs` | 247 | 11 | 2 |
| 5 | `mft.rs` | 243 | 13 | 2 |
| 6 | `attribute.rs` | 247 | 12 | 2 |
| 7a | `attributes/mod.rs` | 93 | 5 | — |
| 7b | `attributes/standard_information.rs` | 152 | 8 | 2 |
| 8 | `attributes/file_name.rs` | 258 | 13 | 3 |
| 9 | `attributes/data.rs` | 206 | 10 | 3 |
| **計** | — | **1470** | **72** | **14** |

`cargo test -p dds-fs-ntfs` の最終結果: **単体 72 + 結合 14 = 86 件 すべて pass**（0 failed）

### カバレッジ計測

`cargo tarpaulin` バージョン 0.35.4 が `~/.cargo/bin/cargo-tarpaulin` に installed されており利用可能。ただし本サマリ作成時点では計測未実施（実行に時間がかかるため）。Chunk 10 着手時、または M2 完了時にカバレッジ計測を推奨。

参考: PRD の NFR-MAINT-02 が「コアモジュールテストカバレッジ 80%以上」を目標として規定。

### Clippy / 安全性ステータス

- `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: **warning 0 件**（全 Chunk 完了時点で維持）
- `cargo doc -p dds-fs-ntfs --no-deps`: 全公開 API に rustdoc 付与済み（`#![warn(missing_docs)]` 有効）
- `crates/fs-ntfs/src/**` 全体で:
  - `unsafe` キーワード: **0 件**
  - `from_be_bytes`（big-endian 誤読込）: **0 件**（NTFS は完全 little-endian）
  - 書き込み API（`fn write` / `fn save` / `fn flush` / `fn truncate` 等）: **0 件**（read-only パーサ）
  - `String::from_utf16_lossy`（lossy 変換）: **0 件**（不正 UTF-16 はエラー化）

### 書籍突合レビュー全体統計

| Chunk | 変更行数 | 追加テスト |
|---|---|---|
| 4 | +83 | +5 |
| 5 | +69 | +5 |
| 6 | +73 | +4 |
| 7 | +58 | +3 |
| 8 | +70 | +4 |
| 9 | +26 | +2 |
| **計** | **+379** | **+23** |

`docs/specs/ntfs-references/notes.md`: 547 行の自前日本語要約（書籍逐語コピー 0 件、複数の tester による Grep 検証済み）。

---

## 7. Chunk 10 (runlist) 着手前に意識すべき点

### レビューを経て確立した設計原則

1. **読み込み専用パーサの徹底**:
   - 全モジュールで `unsafe` / 書き込み API を 0 件に維持
   - `String::from_utf16_lossy` ではなく `String::from_utf16` を使い、不正データはエラー化
   - ファイル I/O は read-only オープン専用

2. **エンディアン規約の徹底**:
   - NTFS は完全 little-endian、`from_be_bytes` を一切使わない
   - クロージャ `u16le` / `u32le` / `u64le` をパース関数内で定義して再利用する慣行

3. **エラー設計**:
   - 構造化バリアント（フィールド付き enum）を使い、エラーメッセージにフィールド値を埋め込む
   - 範囲外チェックを最初に集約し、`unwrap()` / `expect()` が安全な位置以降のみ使う
   - `#[allow(missing_docs)]` を enum/struct 単位で適用しつつ、特殊バリアントには個別 rustdoc

4. **テストフィクスチャの分離**:
   - 単体テストは手書きバイト列（`build_*` ヘルパ）
   - 結合テストは実 NTFS イメージ + ground truth JSON で SHA256 一致まで検証
   - `tests/common/mod.rs` で zstd 解凍と ground truth ロードを共有

5. **書籍例題の数学的再現**:
   - Carrier 著書のサンプルバイト列・フィールド値（USN=0x0058、MFT LCN=342_709、serial=0x0450_2284_5022_7C94 等）をテストにエンコードし、リグレッション防止と仕様根拠の自明化を両立

6. **Forward Compatibility**:
   - 未知の属性タイプは `AttributeType::Unknown(value)` で受け入れ、エラーにしない
   - 未知の DOS 属性フラグビットは保持して捨てない（既知ビットの判定のみメソッド提供）

7. **無限ループ防止**:
   - 属性巡回・runlist 走査では「length == 0」「buffer 範囲外」を必ずエラーで終端
   - `AttributeIterator` は `done` フラグで再 `next()` 呼び出しでも `None` を返す

8. **2 ファイル分散による行数制約遵守**:
   - 200 行制約を 1 ファイルで満たせない場合は責務分割（Chunk 7 の `attributes/mod.rs` + `attributes/standard_information.rs`）

9. **DRY 原則の機械的適用**:
   - 同じ符号付きエンコーディング（MFT/Index record size）は内部関数 `compute_record_size_bytes` で共有
   - 14 ビット判定メソッドは `fa_bits!` 宣言マクロで一元定義

### 過去 Chunk で「後で見直すべき」とコメントされた箇所

`crates/fs-ntfs/` 配下を Grep で全文検索した結果、TODO / FIXME / XXX / 「後で」/ TBD コメントは **0 件**（`mft.rs:176` の `b"XXXX"` リテラルはテストで InvalidMagic を発生させるためのダミーバイト列で、コメントではない）。

過去 Chunk から Chunk 10 で扱うべき残作業として明示されているもの:
- **`NonResidentInfo` の `runlist_offset`**: Chunk 6 で値は取得済みだが、実際の cluster run 解析は Chunk 10 範囲
- **`DataContent::NonResident::attribute_raw` + `runlist_offset_in_attr`**: Chunk 9 で属性 raw データへの参照を保持済み。Chunk 10 で `attribute_raw[runlist_offset_in_attr..]` から runlist バイト列を取得して走査する設計を前提

### 書籍の runlist セクションに関する記載

書籍 Chapter 11 と Chapter 13 に分散:

- **Chapter 11「OTHER ATTRIBUTE CONCEPTS」内 Figure 11.6**（p.~284 付近）: 3 つの run を持つ runlist の概念図（VCN → LCN マッピング）
- **Chapter 13「ATTRIBUTE HEADER」末尾 + Figure 13.3**（p.357-358 付近）: runlist の物理エンコーディング詳細
  - 各 run の最初のバイトは 4-bit ずつ 2 つに分割
    - 下位 4 bit: run length フィールドのバイト数
    - 上位 4 bit: run offset フィールドのバイト数
  - 続いて length バイト（little-endian 符号なし）、offset バイト（little-endian 符号付き、前 run の offset との差分）
  - 終端マーカー: 最初のバイトが 0
  - offset の符号拡張: 上位バイトが 0x80 以上なら負値、計算時は 32/64 bit へ符号拡張が必要
- **Chapter 13 p.358-359 のサンプル**: 書籍が分解する具体例
  - サンプルバイト列: `32 c0 1e b5 3a 05 21 70 1b 1f ...`
  - 第 1 run: header=`0x32` → length=2 bytes, offset=3 bytes → length=`c0 1e`=7872 clusters, offset=`b5 3a 05`=342709（絶対 LCN）
  - 第 2 run: header=`0x21` → length=1 byte, offset=2 bytes → length=`0x70`=112 clusters, offset=`1b 1f`=+7963（前 offset との差分、結果 350672）

このサンプルは Chunk 10 の **書籍例題テスト**として最有力候補（テスト名案: `book_chapter13_runlist_example_two_runs`、検証値: `[(LCN=342709, len=7872), (LCN=350672, len=112)]`）。

### Chunk 10 で踏襲すべきパターン

1. **`AttributeIterator` パターンの再利用**: runlist 走査も `RunlistIterator` のような Iterator 実装が自然
2. **エラー設計の継承**: `RunlistError` enum で `BufferTooSmall` / `InvalidHeaderNibble` / `OffsetOverflow` / `LengthOverflow` 等を定義
3. **2 段階の API**:
   - 純粋関数 `parse_runlist(bytes: &[u8]) -> Result<Vec<Run>, RunlistError>`（書籍 Chapter 13 のエンコーディングを直接実装、単体テスト容易）
   - 高レベル API `read_non_resident_data(disk: &mut dyn ReadOnlyDisk, runlist: &[Run], cluster_size, real_size) -> CoreResult<Vec<u8>>`（実バイト取得、結合テストで SHA256 一致を検証）
4. **`DataContent::NonResident` との接続**: Chunk 9 で保持した `runlist_offset_in_attr` と `attribute_raw` を入力源にする。`extract_main_data_stream` を非常駐ケースに拡張する形が自然
5. **書籍例題の数学的再現**: Carrier の `32 c0 1e b5 3a 05 21 70 1b 1f` を `book_chapter13_runlist_example_two_runs` テストに直接エンコード
6. **結合テスト**: 既存の `recovers_all_30_files_with_matching_sha256_in_healthy_image` を非常駐ファイルにも拡張、または 700 バイト超の大ファイルを含む新フィクスチャ追加（必要なら）
7. **行数制約**: `attributes/runlist.rs`（または `crates/fs-ntfs/src/runlist.rs`）を新規ファイルとして 200 行以内目標。複雑なら 2 ファイル分散も可

### 安全性に関する Chunk 10 特有の注意

- **符号付きオフセットの符号拡張**: 1 byte offset が `0x80` 以上の場合の i8 → i64 拡張、計算時のオーバーフローに `checked_add` / `checked_sub` を使う
- **絶対 LCN への変換**: 累積 offset を `i64` で保持し、最終的に `u64` への変換は範囲チェック必須
- **disk 範囲外読み込み防止**: `disk.total_size()` と `cluster_size * (lcn + length)` の境界チェック
- **疎ファイル（sparse）対応**: 書籍 Figure 11.6 が暗黙的に示すように、`run header == 0` 以外にも「offset bytes = 0」のスパースランがあり得る。Phase 1 ではゼロ埋めで返す方針が妥当

---

## 8. 参考リソース

- **書籍メモ**: `docs/specs/ntfs-references/notes.md`（547 行、自前日本語要約。書籍逐語コピー 0 件）
- **進捗詳細**: `docs/progress.md`（チャンク完了履歴、書籍突合レビュー結果サマリ表、累積メトリクス）
- **PRD**: `docs/PRD.md`（FR-LIVE-01〜07、FR-REC-01/04、NFR-REL-01 等の要件定義）
- **チャンク 10 指示**: `docs/chunk10_*.md`（着手時に参照、本サマリ作成時点では未配置）

---

**作成日**: 2026-05-20
**作成者**: Claude Code（builder / tester / progress-tracker エージェント連携）
**用途**: Chunk 10 着手前の外部 AI 共有用要約。書籍逐語コピーなし、事実情報のみ参照。
