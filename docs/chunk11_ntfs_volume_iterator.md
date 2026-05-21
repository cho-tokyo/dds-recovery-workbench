# Chunk 11 指示: NtfsVolume + MFTイテレータ（全エントリ列挙）

このチャンクで NTFS が **「ボリュームを開いて全ファイルを列挙する」** ことができる実用形になります。Chunk 4-10 までの純粋関数群を **ステートフルな高レベル API** で束ね、上位層（業務統合）からの呼び出しを容易にします。

> 🎯 完了時点で「`NtfsVolume::open(reader)` → 全削除ファイル含めて列挙」が 1 行で書ける状態に。

---

## 目的

Chunks 4-10 で実装した純粋関数群（`parse_boot_sector` / `parse_mft_entry` / `find_attribute` / `parse_runlist`）を **NtfsVolume** という高レベル API に統合する:

1. **ブートセクタ自動パース**
2. **$MFT 自身の bootstrap**: MFT record 0 を読んで自分自身の runlist を取得
3. **任意の MFT エントリへのランダムアクセス**: `read_record(index)`
4. **全 MFT エントリの順次列挙**: `iter_records()`（削除エントリも含む）
5. **NTFS が断片化していても透過対応**: 多 run MFT に対応

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 必読セクション（書籍）

- 書籍 **Chapter 11「FILES AND BASE INODE」**（p.~275 付近）: MFT entry とファイルの関係
- 書籍 **Chapter 13「$MFT and $MFTMirr」** / **Table 13.18 + $MFT FILE record の解説**: $MFT 自身が MFT record 0 として格納される構造
- 書籍 **Chapter 12「ALLOCATION ALGORITHM AND ANALYSIS」**: 削除エントリが MFT 内に保持される性質

### 補助参照

- `docs/specs/ntfs-references/notes.md` の $MFT セクション
- 既存実装: `boot_sector.rs`, `mft.rs`, `attribute.rs`, `attributes/runlist.rs`

### $MFT bootstrap の流れ（書籍 Chapter 13 より）

```
Step 1: ブートセクタ（first 512 bytes）から:
        - cluster_size
        - mft_record_size  
        - mft_lcn （MFT 開始 LCN）
        を取得

Step 2: mft_lcn のクラスタから mft_record_size バイト分を読み取り、
        これが MFT record 0 = $MFT 自身

Step 3: MFT record 0 の $DATA 属性を取得（必ず非常駐）

Step 4: $DATA の runlist を parse_runlist で解析
        → MFT 全体がどのクラスタに配置されているかが判明

Step 5: 以降、任意のレコード index は
        virtual_offset = index * mft_record_size
        runlist を辿って physical_lcn を算出
        → クラスタ読み取り
        → parse_mft_entry
```

## 実装内容

### モジュール配置

`crates/fs-ntfs/src/volume.rs` を新規作成（既存の `boot_sector.rs` 等と同階層、トップレベル）。

理由: `NtfsVolume` はクレートのエントリポイント。`attributes/` 配下に置くと「属性の一種」と誤解されるため top-level に配置。

### 1. `VolumeError` enum

確立されたエラー命名規約に準拠 + `#[from]` を活用して既存エラー型を集約:

```rust
use crate::attribute::AttributeError;
use crate::attributes::RunlistError;
use crate::boot_sector::BootSectorError;
use crate::mft::MftError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VolumeError {
    #[error("Boot sector error: {0}")]
    BootSector(#[from] BootSectorError),
    
    #[error("MFT entry error: {0}")]
    Mft(#[from] MftError),
    
    #[error("Attribute parse error: {0}")]
    Attribute(#[from] AttributeError),
    
    #[error("Runlist error: {0}")]
    Runlist(#[from] RunlistError),
    
    #[error("Disk I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("$MFT record 0 has no $DATA attribute (corrupted volume?)")]
    NoMftDataAttribute,
    
    #[error("$MFT $DATA attribute must be non-resident, got resident")]
    MftDataMustBeNonResident,
    
    #[error("Unexpected sparse run in $MFT runlist")]
    SparseMftRun,
    
    #[error("MFT record index out of range: {index} (total {total})")]
    RecordIndexOutOfRange { index: u64, total: u64 },
    
    #[error("Buffer too small for boot sector: got {got}")]
    BootSectorBufferTooSmall { got: usize },
}
```

`PartialEq` 派生は `std::io::Error` 含むため**外す**（Chunks 4-9 の `DataError` などと同様）。

### 2. `NtfsVolume<F>` 構造体

```rust
use crate::attribute::{AttributeHeader, AttributeType};
use crate::attributes::{find_attribute, parse_runlist, Run};
use crate::boot_sector::{parse_boot_sector, BootSector};
use crate::mft::{parse_mft_entry, MftEntry};

/// NTFS ボリュームの高レベル API。
///
/// ブートセクタを解析し $MFT を bootstrap した上で、
/// 任意の MFT エントリへのアクセスを提供する。
///
/// 内部に `read_clusters` クロージャを保持し、disk-io との結合を
/// 後段（Chunk 13+）で導入できるよう疎結合に設計。
pub struct NtfsVolume<F> {
    boot_sector: BootSector,
    /// $MFT 自身の $DATA 属性から取得した runlist
    mft_runs: Vec<Run>,
    /// MFT レコードサイズ（バイト）
    mft_record_size: u32,
    /// クラスタサイズ（バイト）
    cluster_size: u64,
    /// 推定総 MFT レコード数（$MFT 全 run の合計バイト / record_size）
    total_records: u64,
    /// LCN, count を受け取りクラスタ単位でバイト列を返すクロージャ
    read_clusters: F,
}

impl<F> NtfsVolume<F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// ボリュームを開く。ブートセクタ + $MFT bootstrap を実行。
    ///
    /// `read_clusters(lcn, count)` は `count` クラスタ分のバイト列を返すクロージャ。
    /// `lcn = 0, count = 1` でブートセクタを含む先頭クラスタが読まれる前提。
    pub fn open(mut read_clusters: F) -> Result<Self, VolumeError> {
        // Step 1: 先頭クラスタを読み、最初の 512 バイトでブートセクタ解析
        let first_chunk = read_clusters(0, 1)?;
        if first_chunk.len() < 512 {
            return Err(VolumeError::BootSectorBufferTooSmall { got: first_chunk.len() });
        }
        let boot_sector = parse_boot_sector(&first_chunk[..512])?;
        
        let cluster_size = boot_sector.cluster_size_bytes() as u64;
        let mft_record_size = boot_sector.mft_record_size_bytes();
        let mft_byte_offset = boot_sector.mft_byte_offset();
        let mft_lcn = mft_byte_offset / cluster_size;
        
        // Step 2: MFT record 0 を読み取り
        let clusters_for_record = (mft_record_size as u64).div_ceil(cluster_size).max(1);
        let record_0_bytes = read_clusters(mft_lcn, clusters_for_record)?;
        if record_0_bytes.len() < mft_record_size as usize {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "MFT record 0 read incomplete",
            )));
        }
        let record_0 = parse_mft_entry(&record_0_bytes[..mft_record_size as usize])?;
        
        // Step 3: $DATA 属性を探索
        let data_attr = find_attribute(
            &record_0.data,
            record_0.header.first_attribute_offset as usize,
            AttributeType::Data,
        )
        .ok_or(VolumeError::NoMftDataAttribute)?;
        
        // Step 4: 非常駐確認 + runlist 取得
        let runlist_offset = match &data_attr.header {
            AttributeHeader::NonResident { non_resident, .. } => {
                non_resident.runlist_offset as usize
            }
            _ => return Err(VolumeError::MftDataMustBeNonResident),
        };
        
        let mft_runs = parse_runlist(&data_attr.raw[runlist_offset..])?;
        
        // Step 5: 総レコード数を算出
        let total_mft_bytes: u64 = mft_runs.iter().map(|r| r.byte_length(cluster_size)).sum();
        let total_records = total_mft_bytes / mft_record_size as u64;
        
        Ok(Self {
            boot_sector,
            mft_runs,
            mft_record_size,
            cluster_size,
            total_records,
            read_clusters,
        })
    }
    
    /// 推定総 MFT レコード数（システム + ユーザ + 未使用全部）
    pub fn total_records(&self) -> u64 {
        self.total_records
    }
    
    pub fn mft_record_size(&self) -> u32 {
        self.mft_record_size
    }
    
    pub fn cluster_size(&self) -> u64 {
        self.cluster_size
    }
    
    pub fn boot_sector(&self) -> &BootSector {
        &self.boot_sector
    }
    
    /// 指定 index の MFT レコードを読み取る。
    pub fn read_record(&mut self, index: u64) -> Result<MftEntry, VolumeError> {
        if index >= self.total_records {
            return Err(VolumeError::RecordIndexOutOfRange {
                index,
                total: self.total_records,
            });
        }
        
        let virtual_offset = index * self.mft_record_size as u64;
        let (lcn, byte_in_cluster) = self.virtual_to_physical(virtual_offset)?;
        
        let total_bytes_needed = byte_in_cluster + self.mft_record_size as u64;
        let clusters_needed = total_bytes_needed.div_ceil(self.cluster_size);
        let raw = (self.read_clusters)(lcn, clusters_needed)?;
        
        let start = byte_in_cluster as usize;
        let end = start + self.mft_record_size as usize;
        if raw.len() < end {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "record bytes incomplete",
            )));
        }
        Ok(parse_mft_entry(&raw[start..end])?)
    }
    
    /// 仮想 MFT オフセット（連続バイト位置）→ 物理 (LCN, byte_in_cluster)
    fn virtual_to_physical(&self, virtual_offset: u64) -> Result<(u64, u64), VolumeError> {
        let mut cumulative: u64 = 0;
        for run in &self.mft_runs {
            let run_bytes = run.byte_length(self.cluster_size);
            if virtual_offset < cumulative + run_bytes {
                let offset_in_run = virtual_offset - cumulative;
                let cluster_offset = offset_in_run / self.cluster_size;
                let byte_in_cluster = offset_in_run % self.cluster_size;
                let base_lcn = run.lcn.ok_or(VolumeError::SparseMftRun)?;
                return Ok((base_lcn + cluster_offset, byte_in_cluster));
            }
            cumulative += run_bytes;
        }
        Err(VolumeError::RecordIndexOutOfRange {
            index: virtual_offset / self.mft_record_size as u64,
            total: self.total_records,
        })
    }
    
    /// 全 MFT レコードを順次列挙するイテレータ
    pub fn iter_records(&mut self) -> NtfsMftIterator<'_, F> {
        NtfsMftIterator {
            volume: self,
            current: 0,
        }
    }
}
```

### 3. `NtfsMftIterator` 構造体

```rust
/// 全 MFT レコードの順次イテレータ。
///
/// 削除エントリ・未使用エントリ・特殊エントリ全てを yield する。
/// 呼び出し側で `entry.header.is_deleted()` 等で絞り込む。
pub struct NtfsMftIterator<'a, F> {
    volume: &'a mut NtfsVolume<F>,
    current: u64,
}

impl<'a, F> Iterator for NtfsMftIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// (record_index, MftEntry) のペアを yield。
    /// パースエラーはエントリ単位で `Err` として yield、イテレーション継続。
    type Item = (u64, Result<MftEntry, VolumeError>);
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.volume.total_records {
            return None;
        }
        let idx = self.current;
        self.current += 1;
        Some((idx, self.volume.read_record(idx)))
    }
}
```

**重要な設計判断**: イテレーション中のパースエラー（個別レコード破損）で停止しない。各エントリを `Result` 化して yield、呼び出し側でフィルタ可能にする。これは復旧ソフトとして**破損データへの耐性**を持つ設計。

### 4. `lib.rs` 更新

```rust
pub mod attribute;
pub mod attributes;
pub mod boot_sector;
pub mod mft;
pub mod volume;  // 新規

pub use volume::{NtfsVolume, NtfsMftIterator, VolumeError};
// ... (既存の re-export 維持)
```

## 単体テスト要件（最低 8 件）

`volume.rs` 内 `#[cfg(test)] mod tests`:

### モックリーダーを使った基本動作テスト

確立された build_* パターンに従い、テスト用の最小 NTFS イメージ構築ヘルパーを用意:

```rust
fn build_minimal_ntfs_volume() -> Vec<u8> {
    // 最小サイズの合成 NTFS イメージを構築（ブートセクタ + $MFT 4 レコード分）
    // - cluster_size = 512, mft_record_size = 1024
    // - MFT runs: 単一 run [LCN=4, length=8 clusters = 4096 bytes = 4 records]
    // - record 0: $MFT 自身（$DATA non-resident 含む）
    // - record 1-3: 偽データ（FILE シグネチャだけ持つ）
    // ...
}
```

### 必須テストケース

1. **`opens_minimal_valid_volume`**: 合成イメージで `NtfsVolume::open` が成功、`total_records()` が期待値と一致
2. **`virtual_to_physical_single_run_correct_mapping`**: 単一 run の MFT で仮想オフセット → 物理 (LCN, byte_in_cluster) が正しい
3. **`virtual_to_physical_multi_run_crosses_boundary`**: 複数 run の MFT で仮想オフセットが run 境界を跨いだ時の物理マッピング
4. **`read_record_out_of_range_returns_error`**: `total_records` 以上の index で `RecordIndexOutOfRange`
5. **`read_record_zero_returns_mft_itself`**: index=0 で $MFT 自身が読める（FILE シグネチャ + DATA 属性持ち）
6. **`open_fails_without_boot_sector`**: 先頭バッファが 512 バイト未満で `BootSectorBufferTooSmall`
7. **`open_fails_when_mft_data_is_resident`**: 合成イメージで $MFT $DATA を常駐にして `MftDataMustBeNonResident`
8. **`open_fails_when_no_mft_data_attribute`**: $DATA 属性を取り除いた合成データで `NoMftDataAttribute`
9. **`iter_records_yields_all_indices_in_order`**: モックボリュームで 0..total_records までを順次 yield
10. **`iter_records_continues_on_individual_parse_error`**: 1 つのレコードを意図的に破損させても、他のレコードは正常に yield 継続

### テストヘルパーの DRY

合成 NTFS イメージのビルダーは複雑になる可能性が高い。`build_minimal_ntfs_volume()` を中核ヘルパーとして、各テストはこれをベースに必要箇所だけ書き換える。

## 結合テスト要件

`crates/fs-ntfs/tests/volume_integration.rs` を作成。既存の `tests/common/mod.rs` ヘルパーを活用:

### 1. **健全イメージで全エントリ列挙 + ユーザファイル数検証**

```rust
#[test]
fn ntfs_healthy_small_enumerates_all_records_and_finds_30_user_files() {
    let img = decompress_fixture("ntfs_healthy_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    
    let read_clusters = |lcn: u64, count: u64| -> Result<Vec<u8>, std::io::Error> {
        let start = (lcn * cluster_size) as usize;
        let end = start + (count * cluster_size) as usize;
        if end > img.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "out of bounds"));
        }
        Ok(img[start..end].to_vec())
    };
    
    let mut volume = NtfsVolume::open(read_clusters).unwrap();
    assert!(volume.total_records() > 23, "MFT should have system records 0-23 plus users");
    
    let mut user_file_count = 0;
    for (_idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_in_use() { continue; }
        if let Some(name) = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize) {
            if name.filename.starts_with("file_") {
                user_file_count += 1;
            }
        }
    }
    assert_eq!(user_file_count, 30, "Expected 30 user files in healthy fixture");
}
```

### 2. **削除入りイメージで削除エントリ列挙**

```rust
#[test]
fn ntfs_with_deletions_finds_5_deleted_user_files() {
    // ntfs_with_5_deletions_small を開いて削除エントリだけを抽出
    // 削除されたファイル名（file_003.txt 等）が全て発見される
}
```

### 3. **大ファイルイメージで全タイプのファイル列挙**

```rust
#[test]
fn ntfs_large_files_enumerates_resident_nonresident_and_sparse() {
    // ntfs_large_files を開いて、ファイル種別を集計:
    // - 常駐 $DATA: small_001..010 (10 件)
    // - 非常駐 $DATA: large_001..006 + random_001..003 (9 件)
    // - スパース: sparse_001 (1 件)
    // 全 20 件が列挙可能
}
```

### 4. **プロダクトデモテスト（拡張版）**

Chunk 9 の `product_demo_complete_recovery` を `NtfsVolume::iter_records` ベースに書き換えて、より簡潔に:

```rust
#[test]
fn product_demo_with_volume_api() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Phase 1 (post-Chunk 11) ===");
    println!("Total MFT records: {}", volume.total_records());
    
    let mut recovered = 0;
    let mut deleted_recovered = 0;
    
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        let Some(name) = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize) 
            else { continue };
        
        if !name.filename.starts_with("file_") { continue; }
        
        let status = if entry.header.is_deleted() { "[DELETED]" } else { "[Live]   " };
        println!("  {} #{:<4} {}", status, idx, name.filename);
        recovered += 1;
        if entry.header.is_deleted() { deleted_recovered += 1; }
    }
    
    println!("\nTotal: {}, Deleted: {}", recovered, deleted_recovered);
    assert!(recovered >= 30);
    assert!(deleted_recovered >= 5);
}
```

このテストは `cargo test -p dds-fs-ntfs --release -- --nocapture product_demo_with_volume_api` で実行すると、CS デモにそのまま使える出力が得られます。

## Cargo.toml 設定

変更不要。

## 制約（Chunks 4-10 で確立した規約に準拠）

- **行数上限**: **220 行（実装 + 単体テスト合計）**、結合テストは別カウント
  - ※合成 NTFS ビルダーヘルパが大きくなる可能性。実装本体 + 主要テストで 220 以内目標、ビルダーが超える場合は別ファイル（`tests/helpers/mod.rs` 等）に逃がす
- **単体テスト最低 8 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、`from_be_bytes` 0 件、書き込み API 0 件**
- **エラー型は構造化バリアント** + `#[from]` で既存エラー型を集約（既存 `MftError` などとの整合）
- **テスト命名規約**: `<situation>_<expected_result>` / `<input>_returns_<error>` の確立規約

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（既存 + 新規 ≥10 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥3 件）
- [ ] `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: warning 0
- [ ] 既存 fixture 3 つ全てで `NtfsVolume::open` が成功
- [ ] `recovered >= 30 && deleted_recovered >= 5` がプロダクトデモテストでパス
- [ ] `cargo doc -p dds-fs-ntfs --no-deps` がエラーなく生成、全公開API に rustdoc
- [ ] `grep -r 'unsafe\|from_be_bytes\|fn write' crates/fs-ntfs/src/` で 0 件

## 関連 FR 要件

- **FR-LIVE-01** (NTFS読み取り) ← **これで NTFS リーダの実用形完成**
- **FR-LIVE-04** (ファイルツリー構築) ← 部分達成（フラットなエントリ列挙まで）
- **FR-LIVE-05** (削除エントリ可視化) ← API 経由で容易に
- **FR-LIVE-06** (メタデータ表示) ← API 経由で容易に

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次のステップ候補:
   - **Chunk 12**: `$INDEX_ROOT` / `$INDEX_ALLOCATION` パーサ（ディレクトリエントリ取得）
   - **Chunk 13**: フルパス再構築（親 MFT 参照を辿る + ディレクトリツリー）
   - **Chunk 14**: `NtfsFile` 高レベル構造体（name + meta + content + path を統合）

## 注意事項

### $ATTRIBUTE_LIST は Phase 1 範囲外

書籍 Chapter 11 が言及する `$ATTRIBUTE_LIST` 属性（0x20）は、属性が MFT エントリに収まりきらない場合に複数エントリに分割するための仕組み。**Phase 1 では非対応**。

`$MFT` 自身の $DATA が単一 MFT レコード内に収まることを前提とする（典型的な小〜中規模ボリュームでは成立）。例外時は `NoMftDataAttribute` / `AttributeListNotSupported`（必要なら variant 追加）で明示的にエラー化する。

### MFT 自身が断片化している場合

非常駐 $DATA で複数 run の MFT は普通にあり得る（特に大規模ボリューム）。`virtual_to_physical` の単体テストで多 run 対応を必ず検証すること。

### 削除エントリのパースエラーに寛容

実 NTFS の削除エントリは「使用中の整合性」を保証されない（OS が再利用するまで自由に破損し得る）。`iter_records` でパースエラーが出ても**イテレーション継続**する設計を維持。これは復旧ソフトとしての**破損耐性**の核心。

### `read_clusters` クロージャの責務

`read_clusters(lcn, count)` は `count * cluster_size` バイトを返すことが期待される。**部分読み込みは未定義動作**。短いバッファが返ってきた場合は `Io(UnexpectedEof)` で明示的にエラー化（実装本体で防御済み）。

### 既存テストの破壊リスク

新規 `volume.rs` 追加は破壊的変更ではないが、`lib.rs` の re-export 追加で名前衝突がないか確認:
```bash
cargo test --workspace
```

### 性能の注意点

`iter_records()` 経由で全レコード読み取りすると、合計で `total_records × clusters_per_record` 回のクラスタ読み取りが発生。
- 健全 fixture: 約 64 records × 1〜2 clusters = ~100 reads
- 大規模ボリューム（100万ファイル）: 100万 × 1〜2 clusters = ~200万 reads

Phase 1 では問題にならないが、Chunk 13+ で **連続レコードを 1 回のクラスタ読みで取る最適化** の余地を残しておく（将来のチャンク）。

---

## 質問が必要なケース

- `$MFT` の $DATA が常駐になっている異常データの扱い（理論上ありえないが破損で発生）
- $ATTRIBUTE_LIST を持つ MFT エントリの扱い（現状は単純にスキップでも可）
- 部分読み込み（disk I/O が短いバッファを返す）への対応方針

---

## 完了報告例

```markdown
## Chunk 11 完了報告

- **クレート**: dds-fs-ntfs
- **実装ファイル**: 
  - crates/fs-ntfs/src/volume.rs (新規, 150行 + テスト 70行 = 220行)
  - crates/fs-ntfs/src/lib.rs (更新)
- **公開API**:
  - `NtfsVolume<F>` (open / read_record / iter_records / total_records / ...)
  - `NtfsMftIterator<'a, F>`
  - `VolumeError`
- **テスト統計**:
  - 単体: 既存 92 + 新規 10 = **102 件 pass**
  - 結合: 既存 18 + 新規 4 = **22 件 pass**
- **品質**: clippy 0 warning, unsafe 0, 書き込み API 0
- **プロダクトデモテスト結果**:
  ```
  Total MFT records: 64
  Recovered file_*: 30 (Live: 25, Deleted: 5)
  ```
- **🎉 NTFS リーダの実用形完成** - `NtfsVolume::open()` 1行で全エントリ列挙可能
- **関連 FR**: FR-LIVE-01, FR-LIVE-04 (部分), FR-LIVE-05, FR-LIVE-06

→ tester エージェントへ引き継ぎお願いします
```
