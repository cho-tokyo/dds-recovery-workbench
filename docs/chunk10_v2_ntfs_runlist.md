# Chunk 10 v2 指示: $DATA 非常駐属性（runlist 解析）

**v2 改訂**: Chunks 4-9 のレビュー結果（`docs/review_summary_for_chunk10.md`）で確立された設計パターンを反映、書籍 (`docs/specs/ntfs-references/_private/9780321374752.pdf`) を一次ソースとする版。

> 🎯 このチャンクで **Phase 1 NTFS リーダ技術コアが完成**します。Chunks 4-9 で確立した設計品質をそのまま維持して仕上げます。

---

## 目的

NTFS の非常駐属性に格納される **runlist**（データラン）を解析し、ファイル本体をディスクから読み取れるようにする:

1. **runlist エンコーディングのデコード** → `Vec<Run>`
2. **スパースランの正しい処理** → クラスタ未割当領域はゼロ埋め
3. **`DataContent::NonResident` との接続** → Chunk 9 で保持済みの `attribute_raw` + `runlist_offset_in_attr` を起点に走査
4. **`extract_file_content` 統一 API** → 常駐/非常駐を意識せずファイル本体を取得

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照（書籍を一次ソースに）

### 必読セクション

- 書籍 **Chapter 11「OTHER ATTRIBUTE CONCEPTS」内 Figure 11.6**（p.~284 付近）: runlist の概念図（VCN → LCN マッピング）
- 書籍 **Chapter 13「ATTRIBUTE HEADER」末尾 + Figure 13.3**（p.357-358 付近）: 物理エンコーディング詳細
- 書籍 **Chapter 13 p.358-359 サンプル**: 具体的な runlist 例（バイト列 `32 c0 1e b5 3a 05 21 70 1b 1f ...`）

### 補助参照

- `docs/specs/ntfs-references/notes.md` の runlist 関連セクション（自前メモ）
- Linux NTFS Documentation: https://flatcap.github.io/linux-ntfs/ntfs/concepts/data_runs.html （補助確認用）

### Runlist エンコーディング要点（書籍 Chapter 13 より）

各 run:
- **最初のバイト**: 上位 4 bit = offset フィールドのバイト数 (O)、下位 4 bit = length フィールドのバイト数 (L)
- **続く L バイト**: length（符号なし little-endian、クラスタ数）
- **続く O バイト**: offset（符号付き little-endian、前 run の絶対 LCN との差分、**符号拡張が必要**）
- **終端マーカー**: 最初のバイト = `0x00`
- **スパースラン**: `O == 0`（offset フィールドなし、クラスタ未割当 → 論理ゼロ）

## 実装内容

### モジュール配置

`crates/fs-ntfs/src/attributes/runlist.rs` を新規作成（既存の `attributes/` 配下の命名規約と整合）。

### 1. `Run` 構造体

```rust
/// 単一のデータラン（連続したクラスタ群を表す）。
///
/// 書籍 Chapter 11 Figure 11.6 の VCN → LCN マッピングの 1 要素に対応。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// このランのクラスタ数
    pub length_clusters: u64,
    /// 開始 LCN（論理クラスタ番号）。
    /// `None` = スパースラン（クラスタ未割当、論理的にゼロ）
    pub lcn: Option<u64>,
}

impl Run {
    pub fn is_sparse(&self) -> bool {
        self.lcn.is_none()
    }
    
    pub fn byte_length(&self, cluster_size: u64) -> u64 {
        self.length_clusters * cluster_size
    }
}
```

### 2. `RunlistError` enum

確立されたエラー命名規約（`BufferTooSmall` / `Invalid*` / `*Mismatch` / `*Overflow`）に準拠:

```rust
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum RunlistError {
    #[error("Buffer too small for runlist header: got {got}, need at least 1")]
    BufferTooSmall { got: usize },
    
    #[error("Invalid runlist header nibble: length_bytes={length_bytes}, offset_bytes={offset_bytes} (must be 0..=8, and length_bytes > 0 for non-terminator)")]
    InvalidHeaderNibble { length_bytes: u8, offset_bytes: u8 },
    
    #[error("Length field truncated: need {need} bytes, got {got}")]
    LengthFieldTruncated { need: usize, got: usize },
    
    #[error("Offset field truncated: need {need} bytes, got {got}")]
    OffsetFieldTruncated { need: usize, got: usize },
    
    #[error("LCN overflow during accumulation: previous=0x{previous:X}, delta={delta}")]
    LcnOverflow { previous: i64, delta: i64 },
    
    #[error("Resolved LCN is negative: got {got}")]
    NegativeLcn { got: i64 },
    
    #[error("Invalid cluster size: {got} (must be > 0)")]
    InvalidClusterSize { got: u64 },
    
    #[error("Real size mismatch: computed={computed}, declared={declared}")]
    RealSizeMismatch { computed: u64, declared: u64 },
}
```

### 3. 純粋関数: `parse_runlist`

書籍 Chapter 13 のエンコーディングを直接実装。**書籍 p.358-359 サンプルがそのまま動く実装**にすること。

```rust
/// バイト列から runlist をデコードする（純粋関数、I/O なし）。
///
/// 入力は属性 raw データの `runlist_offset_in_attr` 以降のバイト列。
/// 終端マーカー (`0x00`) まで走査し、全ランをリスト化する。
///
/// 書籍 Chapter 13 Figure 13.3 のエンコーディングに準拠。
pub fn parse_runlist(bytes: &[u8]) -> Result<Vec<Run>, RunlistError> {
    let mut runs = Vec::new();
    let mut cursor = 0;
    let mut current_lcn: i64 = 0;  // 累積 LCN（符号付き、書籍 p.358 の方式）
    
    loop {
        if cursor >= bytes.len() {
            return Err(RunlistError::BufferTooSmall { got: cursor });
        }
        
        let header = bytes[cursor];
        cursor += 1;
        
        // 終端マーカー
        if header == 0x00 {
            break;
        }
        
        let length_bytes = (header & 0x0F) as u8;
        let offset_bytes = ((header >> 4) & 0x0F) as u8;
        
        // 書籍 Chapter 13 が示す制約: L は 1..=8、O は 0..=8
        if length_bytes == 0 || length_bytes > 8 || offset_bytes > 8 {
            return Err(RunlistError::InvalidHeaderNibble { length_bytes, offset_bytes });
        }
        
        // length（符号なし）
        let length = read_unsigned_le(bytes, cursor, length_bytes as usize)
            .map_err(|got| RunlistError::LengthFieldTruncated { 
                need: length_bytes as usize, got 
            })?;
        cursor += length_bytes as usize;
        
        // offset: 0 バイト = スパース、それ以外 = 符号付き
        let lcn = if offset_bytes == 0 {
            None  // スパースラン
        } else {
            let delta = read_signed_le(bytes, cursor, offset_bytes as usize)
                .map_err(|got| RunlistError::OffsetFieldTruncated { 
                    need: offset_bytes as usize, got 
                })?;
            cursor += offset_bytes as usize;
            
            current_lcn = current_lcn.checked_add(delta)
                .ok_or(RunlistError::LcnOverflow { previous: current_lcn, delta })?;
            
            if current_lcn < 0 {
                return Err(RunlistError::NegativeLcn { got: current_lcn });
            }
            
            Some(current_lcn as u64)
        };
        
        runs.push(Run { length_clusters: length, lcn });
    }
    
    Ok(runs)
}

// 内部ヘルパー（rustdoc 必須）

/// 指定バイト数を符号なし little-endian で読む（最大 8 バイト）。
fn read_unsigned_le(bytes: &[u8], offset: usize, count: usize) -> Result<u64, usize> {
    if offset + count > bytes.len() {
        return Err(bytes.len().saturating_sub(offset));
    }
    let mut buf = [0u8; 8];
    buf[..count].copy_from_slice(&bytes[offset..offset + count]);
    Ok(u64::from_le_bytes(buf))
}

/// 指定バイト数を符号付き little-endian で読む（**符号拡張あり**、最大 8 バイト）。
///
/// 書籍 Chapter 13 が明示する「offset は前 run との差分、符号拡張が必要」を実装。
fn read_signed_le(bytes: &[u8], offset: usize, count: usize) -> Result<i64, usize> {
    if offset + count > bytes.len() {
        return Err(bytes.len().saturating_sub(offset));
    }
    let mut buf = [0u8; 8];
    buf[..count].copy_from_slice(&bytes[offset..offset + count]);
    // 符号拡張: 最上位バイトの最上位ビットが 1 なら、残りを 0xFF で埋める
    if bytes[offset + count - 1] & 0x80 != 0 {
        for i in count..8 {
            buf[i] = 0xFF;
        }
    }
    Ok(i64::from_le_bytes(buf))
}
```

### 4. 高レベル API: `read_runs_with`（クロージャベース）

`dds-disk-io::ReadOnlyDisk` を直接依存しない設計で、テスタビリティを最優先:

```rust
/// runlist に従ってデータを読み取る。
///
/// `read_clusters(lcn, count)` はディスク（またはイメージバッファ）から
/// `count` クラスタ分のバイトを読む関数。
///
/// - 結合テストでは `move |lcn, count| { ... }` で image バイトから読む
/// - 本番（Chunk 11+）では `disk-io::ReadOnlyDisk` をラップして呼ぶ
///
/// スパースランは `0x00` バイトで埋める（書籍 Figure 11.6 のスパース概念に準拠）。
pub fn read_runs_with<F>(
    runs: &[Run],
    cluster_size: u64,
    real_size: u64,
    mut read_clusters: F,
) -> Result<Vec<u8>, RunlistError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    if cluster_size == 0 {
        return Err(RunlistError::InvalidClusterSize { got: 0 });
    }
    
    let total_bytes: u64 = runs.iter().map(|r| r.byte_length(cluster_size)).sum();
    if total_bytes < real_size {
        return Err(RunlistError::RealSizeMismatch {
            computed: total_bytes, declared: real_size,
        });
    }
    
    let mut buffer = Vec::with_capacity(real_size as usize);
    
    for run in runs {
        match run.lcn {
            Some(lcn) => {
                let data = read_clusters(lcn, run.length_clusters)
                    .map_err(|e| RunlistError::from(e))?;
                buffer.extend_from_slice(&data);
            }
            None => {
                // スパース: 全ゼロで埋める
                buffer.extend(std::iter::repeat(0u8).take(run.byte_length(cluster_size) as usize));
            }
        }
    }
    
    // クラスタ境界に切り上げられているので real_size でトリミング
    buffer.truncate(real_size as usize);
    Ok(buffer)
}

// io::Error 変換のため、エラー enum に From 実装を追加
impl From<std::io::Error> for RunlistError {
    fn from(_e: std::io::Error) -> Self {
        // 詳細を保持する場合は variant を増やす。Phase 1 は簡略版でOK
        // 必要なら #[from] アトリビュートと variant 追加に切替
        RunlistError::InvalidClusterSize { got: 0 }  // placeholder
    }
}
```

**注**: 上記 `From` 実装は placeholder。実装時は専用 variant `DiskRead(std::io::Error)` を追加して `#[from]` を使う形が望ましい。Chunks 4-9 の `MftError` / `DataError` の流儀に合わせて設計。

### 5. `DataContent::NonResident` との接続

`crates/fs-ntfs/src/attributes/data.rs` を**最小限拡張**:

```rust
impl<'a> DataContent<'a> {
    /// 非常駐の場合、属性 raw データから runlist バイト列を取得する。
    /// 常駐の場合は `None`。
    pub fn runlist_bytes(&self) -> Option<&[u8]> {
        match self {
            DataContent::NonResident { runlist_offset_in_attr, attribute_raw, .. } => {
                attribute_raw.get(*runlist_offset_in_attr..)
            }
            DataContent::Resident { .. } => None,
        }
    }
}
```

**注**: `extract_file_content` のような統一 API は **Chunk 11 でリーダー側に置く** 方が責務分離として綺麗。runlist.rs は「パース + クロージャベース読み出し」だけに専念。

### 6. `attributes/mod.rs` と `lib.rs` の更新

`attributes/mod.rs`:
```rust
pub mod data;
pub mod file_name;
pub mod runlist;
pub mod standard_information;

// 公開 re-export
pub use runlist::{Run, RunlistError, parse_runlist, read_runs_with};
// ... (既存)
```

`lib.rs`:
```rust
pub use attributes::{
    Run, RunlistError, parse_runlist, read_runs_with,
    // ... (既存)
};
```

## 単体テスト要件（最低 10 件）

### 書籍例題テスト（必須・最重要）

```rust
#[test]
fn book_chapter13_runlist_example_two_runs() {
    // 書籍 Chapter 13 p.358-359 のサンプルバイト列
    // 第1run: 0x32 c0 1e b5 3a 05 → length=0x1ec0=7872, offset=0x053ab5=342709 (絶対LCN)
    // 第2run: 0x21 70 1b 1f       → length=0x70=112, offset=0x1f1b=+7963 (delta)
    //                              → 累積LCN = 342709 + 7963 = 350672
    // 終端: 0x00
    let bytes = [
        0x32, 0xc0, 0x1e, 0xb5, 0x3a, 0x05,
        0x21, 0x70, 0x1b, 0x1f,
        0x00,
    ];
    let runs = parse_runlist(&bytes).unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0], Run { length_clusters: 7872, lcn: Some(342709) });
    assert_eq!(runs[1], Run { length_clusters: 112,  lcn: Some(350672) });
}
```

### その他の必須テスト

2. `book_example_three_runs_with_negative_delta`: 書籍 Figure 11.6 を模した 3 ラン例（第2runで戻る方向への遷移）
3. `single_run_then_end_marker`: シンプルな 1 ラン + `0x00`
4. `empty_runlist_immediate_end`: `[0x00]` で空 `Vec`
5. `unterminated_runlist_returns_buffer_too_small`: 終端なしで `BufferTooSmall`
6. `sparse_run_offset_bytes_zero`: `[0x01, 0x05, 0x00]` でスパース 1 ラン (length=5, lcn=None)
7. `sparse_mixed_with_normal_runs`: スパース → 通常 → スパースの混在で累積LCNが正しく維持される
8. `sign_extension_negative_one_byte_offset`: `[0x11, 0x05, 0xFF, ...]` 第1run で offset=`0xFF` → -1 として処理
9. `sign_extension_three_byte_offset_boundary`: 3 バイト offset の最上位バイト `0x80` で正しく符号拡張
10. `invalid_header_nibble_length_zero_with_non_terminator_byte`: header=`0xF0`（L=0, O=15）で `InvalidHeaderNibble`
11. `invalid_header_nibble_offset_over_eight`: header=`0x91`（L=1, O=9）で `InvalidHeaderNibble`
12. `length_field_truncated_returns_specific_error`: L=4 だが残り 2 バイト
13. `offset_field_truncated_returns_specific_error`: O=3 だが残り 1 バイト
14. `lcn_overflow_returns_lcn_overflow_error`: 巨大な正 delta で i64 オーバーフロー
15. `negative_lcn_after_subtraction_returns_negative_lcn_error`: 第1run で LCN=100、第2run で delta=-200

### `read_runs_with` テスト

16. `read_runs_with_mock_reader_assembles_continuous_data`: 2 ランで連続バイト列を復元
17. `read_runs_with_sparse_run_fills_zeros`: スパースランがゼロバイトで埋められる
18. `read_runs_with_truncates_to_real_size`: クラスタ境界より小さい `real_size` で正しくトリミング
19. `read_runs_with_cluster_size_zero_returns_invalid_cluster_size`: cluster_size=0 で適切なエラー
20. `read_runs_with_disk_error_propagates`: クロージャがエラー返却で `RunlistError` に伝播

### 命名規約（既存パターン準拠）

- 書籍由来テスト: `book_chapter<N>_<topic>_<scenario>` または `book_example_<scenario>`
- 通常テスト: `<situation>_<expected_result>`
- エラー検証: `<input_condition>_returns_<error_variant_name>`

## 結合テスト要件（新フィクスチャ）

### Step 1: 大ファイル含むフィクスチャ作成

`fixtures/scripts/gen_ntfs_large_files.py` を新規作成（WSL Ubuntu で sudo 実行）:

```python
#!/usr/bin/env python3
"""
大ファイル（非常駐 $DATA）を含む NTFS イメージを生成。

生成内容（合計約 70MB のイメージ、zstd 圧縮で約 5MB に圧縮）:
  - small_001.txt 〜 small_010.txt: 各 50 バイト（常駐確定）
  - large_001.bin 〜 large_005.bin: 各 100KB（非常駐確定、複数 cluster）
  - large_006.bin: 1MB（複数 run の可能性高、断片化テスト用）
  - sparse_001.bin: 2MB のスパースファイル (dd seek+write)
  - random_001.bin 〜 random_003.bin: 各 500KB のランダムバイト（圧縮無効化）
"""
# 既存 gen_ntfs_basic.py をベースに拡張
# ground truth JSON に各ファイルの SHA256 を必ず記録
```

ground truth JSON フォーマット（既存 `ntfs_with_5_deletions_small.json` と互換）:

```json
{
  "fixture_name": "ntfs_large_files",
  "fs_type": "NTFS",
  "image_size_bytes": 73400320,
  "files": [
    { "path": "small_001.txt", "size_bytes": 50, "content_hash_sha256": "...", "is_deleted": false },
    { "path": "large_006.bin",  "size_bytes": 1048576, "content_hash_sha256": "...", "is_deleted": false, "expected_resident": false }
  ]
}
```

### Step 2: 結合テストファイル

`crates/fs-ntfs/tests/runlist_integration.rs` を作成。既存の `tests/common/mod.rs` ヘルパー（`decompress_fixture`, `load_ground_truth`, `sha256_hex`）を活用:

```rust
mod common;
use common::*;

#[test]
fn recovers_all_large_files_with_matching_sha256() {
    let img = decompress_fixture("ntfs_large_files");
    let ground_truth = load_ground_truth("ntfs_large_files");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    
    // image を直接読む read_clusters クロージャ
    let read_clusters = |lcn: u64, count: u64| -> Result<Vec<u8>, std::io::Error> {
        let start = (lcn * cluster_size) as usize;
        let end = start + (count * cluster_size) as usize;
        if end > img.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "out of bounds"));
        }
        Ok(img[start..end].to_vec())
    };
    
    // 各ファイルを MFT 走査 → $DATA 取得 → 非常駐なら runlist 解析 → SHA256 比較
    // ground truth と全件一致を確認
    // ...
}

#[test]
fn large_006_bin_has_multiple_runs() {
    // 断片化したファイルが 2+ ランに分かれていることを assert
}

#[test]
fn sparse_001_bin_recovers_correctly_with_zero_filled_regions() {
    // スパースファイルの内容（ゼロ + 実データ）が完全一致
}

#[test]
fn deleted_large_file_still_recoverable() {
    // 大ファイルを 1 つ削除したフィクスチャでも完全復元可能（任意で追加フィクスチャ）
}
```

## Cargo.toml 設定

変更不要（既存依存で足りる）。

## 制約（Chunks 4-9 で確立した規約に準拠）

- **行数上限**: **220 行（実装 + 単体テスト合計、複雑性考慮で若干緩和）**、結合テストは別カウント
- **単体テスト最低 10 件**、書籍例題テスト 1 件以上必須
- **全公開 type/method に rustdoc 必須**、`#![warn(missing_docs)]` 有効
- **`unsafe` 0 件**、`from_be_bytes` 0 件、書き込み API 0 件
- **エラー型は構造化バリアント**、`#[error("...")]` メッセージは規約準拠（観測値埋込、英文）
- **テスト命名規約**: 書籍由来は `book_chapter*` / `book_example_*`、エラー検証は `*_returns_*_error`
- **`unwrap()` は事前長さチェック後の `try_into().unwrap()` のみ許容**

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（**単体テスト 既存 72 + 新規 ≥10 = 82+ 件**）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（**結合テスト 既存 14 + 新規 ≥3 = 17+ 件**）
- [ ] **書籍例題テスト `book_chapter13_runlist_example_two_runs` が pass**
- [ ] 新フィクスチャ `fixtures/images/ntfs_large_files.img.zst` が生成済み + JSON 同梱
- [ ] `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: warning 0 件
- [ ] `cargo doc -p dds-fs-ntfs --no-deps`: 公開 API 全てに rustdoc
- [ ] **`grep -r 'unsafe\|from_be_bytes\|fn write' crates/fs-ntfs/src/` で 0 件**

## 関連 FR 要件

- **FR-LIVE-01** (NTFS読み取り) ← **このチャンクで NTFS リーダ完成**
- **FR-REC-01** (目標優先抽出) ← 大ファイル復旧の基盤完成
- **FR-REC-04** (データ整合性) ← SHA256 検証で実証

## 完了後

1. tester エージェントへ引き継ぎ（単体 + 結合 + フィクスチャ存在確認）
2. テスト合格後、progress-tracker へ進捗反映
3. **🎉 Phase 1 NTFS リーダ技術コア完成**
4. 次のステップ候補:
   - **Chunk 11**: MFT イテレータ + ディレクトリツリー再構築（フルパス再構築）
   - **Chunk 12**: ファイルシステム高レベル API（`NtfsVolume::iter_files()` 等）
   - **Chunk 13**: dds-disk-io と統合した実 HDD 対応

---

## 注意事項（Chunks 4-9 レビューで判明したパターン適用）

### 符号拡張の徹底（最重要）

書籍 p.358-359 が明示するように、offset フィールドは **符号付き** で読み、**符号拡張** が必要。これを怠ると負方向の delta（前 run より小さな LCN）で誤った絶対 LCN を計算する致命バグになる。

- `read_signed_le` の符号拡張ロジックは**単体テスト #8, #9 で必ず検証**
- 書籍例題テスト #1 は正方向 delta なので、別途負方向テストが必須

### `From<std::io::Error>` 実装の正しい形

placeholder で書いたが、Chunks 4-9 の `MftError::DiskRead(#[from] std::io::Error)` パターンに合わせて専用 variant を追加するのが正しい:

```rust
#[derive(thiserror::Error, Debug)]
pub enum RunlistError {
    // ...
    #[error("Disk read error: {0}")]
    DiskRead(#[from] std::io::Error),
}
```

ただし `std::io::Error` は `PartialEq` を実装しないので、`RunlistError` の `PartialEq` 派生を外す（既存 `DataError` などと同様）。

### スパースランの判定基準

書籍 Figure 11.6 + Chapter 13 のエンコーディング規則より、**`offset_bytes == 0` がスパース判定の唯一の基準**。`length_bytes == 0` は終端マーカー（`header == 0x00`）として既に処理済み。`lcn == 0` をスパース扱いするのは誤り（LCN 0 はパーティション先頭の有効値）。

### 圧縮 $DATA の扱い

書籍 Chapter 11 が言及する LZNT1 圧縮の検出は **Phase 1 範囲外**だが:
- `AttributeCommonHeader::flags & 0x0001` が立っていることを `DataStream::is_compressed` で既に検出済み（Chunk 9）
- runlist 自体のパースは圧縮ファイルでも同じく動作する（圧縮単位の境界は気にしなくてよい）
- 取得したバイト列が LZNT1 圧縮されているので、そのまま開いても意味のあるデータにならないことを呼び出し側に通知する責務がある
- Phase 1 では `is_compressed == true` のファイルに対しては raw 圧縮データを返す。レポート層（後の Chunk）で「圧縮ファイル含む」と注記する

### 既存テスト破壊チェック

`DataContent::runlist_bytes()` メソッド追加は破壊的変更ではないが、`use` パスや re-export を変更する場合は既存テストへの影響を確認:

```bash
cargo test --workspace 2>&1 | tee /tmp/test_after_chunk10.log
```

---

## 質問が必要なケース

以下は推測せず人間に確認:
- 圧縮 $DATA の `is_compressed == true` で `runlist_bytes` を取得した時の警告レベル（エラーか、警告ログか、サイレントか）
- `last_vcn - starting_vcn + 1 != sum(length_clusters)` の不整合検出を厳密に行うか
- 1 ファイルが数十ラン以上に断片化している極端ケースの動作確認方針

---

## 完了時の報告例

```markdown
## Chunk 10 完了報告（v2 仕様準拠）

- **クレート**: dds-fs-ntfs
- **実装ファイル**: 
  - crates/fs-ntfs/src/attributes/runlist.rs (新規, 140行 + テスト 80行 = 220行)
  - crates/fs-ntfs/src/attributes/data.rs (拡張: `runlist_bytes()` メソッド追加 8行)
  - crates/fs-ntfs/src/attributes/mod.rs (更新)
  - crates/fs-ntfs/src/lib.rs (更新)
- **新フィクスチャ**: ntfs_large_files.img.zst（圧縮後 5.2MB）
  - 10 small_*.txt (常駐)
  - 6 large_*.bin (非常駐 100KB〜1MB)
  - 1 sparse_001.bin (2MB スパース)
- **公開API**:
  - `Run`, `RunlistError`
  - `parse_runlist(bytes) -> Result<Vec<Run>, RunlistError>`
  - `read_runs_with<F>(runs, cluster_size, real_size, read_clusters)`
  - `DataContent::runlist_bytes()` (拡張)
- **テスト統計**:
  - 単体テスト: 既存 72 + 新規 20 = **92 件 pass**
  - 結合テスト: 既存 14 + 新規 4 = **18 件 pass**
  - 書籍例題テスト 2 件含む
- **品質**: clippy warning 0, unsafe 0, from_be_bytes 0, 書き込み API 0
- **🎉 Phase 1 NTFS リーダ技術コア完成**
- **関連FR**: FR-LIVE-01, FR-REC-01, FR-REC-04

### 書籍例題の再現結果
```
book_chapter13_runlist_example_two_runs:
  入力: [0x32, 0xc0, 0x1e, 0xb5, 0x3a, 0x05, 0x21, 0x70, 0x1b, 0x1f, 0x00]
  期待: Run { length_clusters: 7872, lcn: Some(342709) },
       Run { length_clusters: 112,  lcn: Some(350672) }
  結果: ✓ PASS
```

→ tester エージェントへ引き継ぎお願いします
```
