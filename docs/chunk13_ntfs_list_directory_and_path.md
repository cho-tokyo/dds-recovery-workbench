# Chunk 13 指示: NtfsVolume::list_directory + フルパス再構築

このチャンクで NTFS が「**全ファイルにフルパスがついた一覧**」を出せる状態に到達します。Chunk 12 のインデックスパースを B+ ツリー全体に拡張し、各ファイルの絶対パス（`\dir1\sub2\file.txt` 形式）を再構築します。

> 🎯 完了時点で NTFS リーダの「**実用形完成形**」に到達。Chunk 14 で `NtfsFile` 統合型を作れば、業務統合層（wish-match）の素材が揃います。

---

## 目的

2 つの機能を統合する:

### A. ディレクトリリスティング（B+ ツリー走査）

- `$INDEX_ROOT` をルートとして再帰的に B+ ツリーを辿る
- `$INDEX_ALLOCATION` 内の INDX ブロックも読み込む
- 結果: ディレクトリ内の**全ファイル**（Win32/DOS 重複含む）を一覧化

### B. フルパス再構築

- ファイル MFT エントリの `$FILE_NAME` 属性から親 MFT 参照を取得
- 親ディレクトリを再帰的に辿ってルート (`\`) まで到達
- 結果: `\path\to\file.txt` 形式のフルパス文字列
- キャッシュで効率化（同じディレクトリパスを 1 度だけ計算）

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 必読セクション（書籍）

- 書籍 **Chapter 12「INDEX ANALYSIS」「FINDING FILES」**: B+ ツリー走査の手順
- 書籍 **Chapter 12「LINKS TO FILES AND DIRECTORIES」**: 親ディレクトリ参照、ハードリンク
- 書籍 **Chapter 13「$INDEX_ALLOCATION」**: 非常駐インデックスの読み出し方

### 補助参照

- 既存実装: `volume.rs` (Chunk 11), `attributes/index.rs` (Chunk 12), `attributes/file_name.rs` (Chunk 8, ハードリンク対応)

### B+ ツリー走査のアルゴリズム

書籍 Chapter 12 を要約:

```
function traverse_node(node):
    for entry in node.entries:
        if entry.has_child_node:
            child_block = read_indx_block(entry.child_vcn)
            traverse_node(child_block.node)  # 再帰
        
        if entry.is_last:
            break  # 最終エントリ自体は値を持たない、子ノードのみ
        
        yield entry  # 値を持つエントリ
```

**重要**: 値 (`file_name` 付き) を持つエントリと、子ノードへのポインタを持つエントリは**同じエントリ**（B+ ツリーの内部ノードは両方を持つ）。最終エントリだけは値を持たない（子ノードへのポインタのみ）。

### フルパス再構築のアルゴリズム

```
function full_path(record_index):
    if record_index == 5:  # NTFS root
        return "\"
    
    entry = read_record(record_index)
    file_name = find_best_file_name(entry)
    
    parent_index = file_name.parent_directory.entry_number
    parent_path = full_path(parent_index)  # 再帰
    
    if parent_path == "\":
        return "\" + file_name.filename
    else:
        return parent_path + "\" + file_name.filename
```

**深さ制限**: NTFS のディレクトリ深さは現実的に 32 階層程度が上限。Phase 1 では **64 階層** で打ち切ってエラー化（破損データ・循環参照防護）。

## 実装内容

### モジュール構成

2 ファイル分割で各 220 行以内を維持:

1. `crates/fs-ntfs/src/volume.rs` 拡張: `list_directory` メソッド + B+ ツリー走査
2. `crates/fs-ntfs/src/path.rs` 新規: `PathResolver` + `NtfsVolume::full_path` メソッド

### 1. `DirectoryListing` 構造体（volume.rs に追加）

```rust
use crate::attributes::file_name::{FileName, MftReference};

/// ディレクトリ内の 1 エントリの情報。
///
/// インデックス経由なので「ライブモードで見えるファイル」のみ。
/// 削除済みエントリは含まれない（書籍 Chapter 12 の動作）。
#[derive(Debug, Clone)]
pub struct DirectoryListing {
    /// 子ファイル/ディレクトリの MFT 参照
    pub child_ref: MftReference,
    /// ファイル名情報（$FILE_NAME）
    pub file_name: FileName,
}

impl DirectoryListing {
    pub fn is_directory(&self) -> bool {
        self.file_name.file_attributes.is_directory()
    }
    
    pub fn name(&self) -> &str {
        &self.file_name.filename
    }
}
```

### 2. `NtfsVolume::list_directory` メソッド（volume.rs 追加）

```rust
use crate::attribute::{AttributeHeader, AttributeType};
use crate::attributes::{
    find_attribute, parse_index_root, parse_indx_block, parse_entries_in_node,
    parse_runlist, IndexEntry, Run,
};

const MAX_BTREE_DEPTH: u32 = 32;  // 破損データ防護

impl<F> NtfsVolume<F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// 指定ディレクトリ MFT エントリ内の全子ファイル/ディレクトリを列挙する。
    ///
    /// B+ ツリー全体を走査するため、$INDEX_ROOT と $INDEX_ALLOCATION 両方をパース。
    /// 重複（Win32 + DOS 別エントリ）はそのまま yield。呼び出し側で重複排除。
    pub fn list_directory(&mut self, dir_record_index: u64) 
        -> Result<Vec<DirectoryListing>, VolumeError> 
    {
        let dir_entry = self.read_record(dir_record_index)?;
        
        // ディレクトリでない場合はエラー
        // ($STANDARD_INFORMATION の attribute flags を確認、または
        //  $INDEX_ROOT 属性の存在で判断する方法もある。後者がシンプル)
        
        // $INDEX_ROOT 属性取得（必ず常駐）
        let index_root_attr = find_attribute(
            &dir_entry.data,
            dir_entry.header.first_attribute_offset as usize,
            AttributeType::IndexRoot,
        ).ok_or(VolumeError::NotADirectory { record_index: dir_record_index })?;
        
        let (content_offset, content_size) = match &index_root_attr.header {
            AttributeHeader::Resident { resident, .. } => 
                (resident.content_offset as usize, resident.content_size as usize),
            _ => return Err(VolumeError::IndexRootNotResident),
        };
        let index_root_content = &index_root_attr.raw[content_offset..content_offset + content_size];
        let index_root = parse_index_root(index_root_content)?;
        
        let mut results = Vec::new();
        
        // ルートノードのエントリを走査
        let root_entries = parse_entries_in_node(index_root.node_body)?;
        self.walk_entries(&root_entries, dir_entry.clone(), &mut results, 0)?;
        
        Ok(results)
    }
    
    /// B+ ツリーノードのエントリ列を走査し、必要に応じて子ノードへ再帰。
    fn walk_entries(
        &mut self,
        entries: &[IndexEntry],
        dir_entry_for_alloc: crate::mft::MftEntry,  // $INDEX_ALLOCATION を取るため
        results: &mut Vec<DirectoryListing>,
        depth: u32,
    ) -> Result<(), VolumeError> {
        if depth > MAX_BTREE_DEPTH {
            return Err(VolumeError::BtreeTooDeep { depth });
        }
        
        for entry in entries {
            // 子ノードへの再帰（値を持つエントリにも子ノードがあり得る）
            if entry.has_child_node() {
                if let Some(vcn) = entry.child_vcn {
                    self.walk_indx_block(vcn, &dir_entry_for_alloc, results, depth + 1)?;
                }
            }
            
            // 最終エントリは値を持たない（ナビゲーション専用）
            if entry.is_last() {
                continue;
            }
            
            // 値を持つエントリを yield
            if let Some(fn_) = &entry.file_name {
                results.push(DirectoryListing {
                    child_ref: entry.child_ref,
                    file_name: fn_.clone(),
                });
            }
        }
        
        Ok(())
    }
    
    /// $INDEX_ALLOCATION 内の指定 VCN の INDX ブロックを読み、エントリを走査。
    fn walk_indx_block(
        &mut self,
        vcn: u64,
        dir_entry: &crate::mft::MftEntry,
        results: &mut Vec<DirectoryListing>,
        depth: u32,
    ) -> Result<(), VolumeError> {
        // $INDEX_ALLOCATION 属性を取得
        let alloc_attr = find_attribute(
            &dir_entry.data,
            dir_entry.header.first_attribute_offset as usize,
            AttributeType::IndexAllocation,
        ).ok_or(VolumeError::IndexAllocationMissing)?;
        
        let (runlist_offset, _real_size) = match &alloc_attr.header {
            AttributeHeader::NonResident { non_resident, .. } => 
                (non_resident.runlist_offset as usize, non_resident.real_size),
            _ => return Err(VolumeError::IndexAllocationNotNonResident),
        };
        
        // runlist パース（Chunk 10）
        let runs = parse_runlist(&alloc_attr.raw[runlist_offset..])?;
        
        // INDX ブロックサイズ取得（$INDEX_ROOT から取りたいが、ここでは bytes_per_index_record を再取得）
        // 簡略: 4096 バイト（典型値）と仮定。厳密には $INDEX_ROOT.bytes_per_index_record を使うべき
        let block_size = 4096u64;  // 注: 後段で動的に取る形に改善可
        
        // VCN → 物理 LCN へ変換
        let (lcn, byte_offset) = self.virtual_to_physical_in_runs(&runs, vcn * block_size)?;
        let clusters_to_read = (block_size + self.cluster_size() - 1) / self.cluster_size();
        let raw = (self.read_clusters)(lcn, clusters_to_read)?;
        
        let block_bytes = &raw[byte_offset as usize..byte_offset as usize + block_size as usize];
        let sector_size = self.boot_sector().bytes_per_sector;
        let indx = parse_indx_block(block_bytes, sector_size)?;
        
        // INDX ブロック内のエントリを走査
        let entries = parse_entries_in_node(indx.node_body())?;
        self.walk_entries(&entries, dir_entry.clone(), results, depth)?;
        
        Ok(())
    }
    
    /// runlist の virtual offset から物理 (LCN, byte_in_cluster) へ変換するヘルパー
    fn virtual_to_physical_in_runs(&self, runs: &[Run], virtual_offset: u64) 
        -> Result<(u64, u64), VolumeError> 
    {
        let mut cumulative: u64 = 0;
        for run in runs {
            let run_bytes = run.byte_length(self.cluster_size());
            if virtual_offset < cumulative + run_bytes {
                let offset_in_run = virtual_offset - cumulative;
                let cluster_offset = offset_in_run / self.cluster_size();
                let byte_in_cluster = offset_in_run % self.cluster_size();
                let base_lcn = run.lcn.ok_or(VolumeError::SparseMftRun)?;
                return Ok((base_lcn + cluster_offset, byte_in_cluster));
            }
            cumulative += run_bytes;
        }
        Err(VolumeError::IndexVcnOutOfRange { virtual_offset })
    }
}
```

### 3. `VolumeError` バリアント追加（volume.rs 既存 enum に追加）

```rust
// 既存 VolumeError enum に追加
#[error("Record {record_index} is not a directory ($INDEX_ROOT missing)")]
NotADirectory { record_index: u64 },

#[error("$INDEX_ROOT must be resident, got non-resident")]
IndexRootNotResident,

#[error("$INDEX_ALLOCATION attribute missing (referenced by has_children flag)")]
IndexAllocationMissing,

#[error("$INDEX_ALLOCATION must be non-resident, got resident")]
IndexAllocationNotNonResident,

#[error("Index VCN out of range: virtual_offset={virtual_offset}")]
IndexVcnOutOfRange { virtual_offset: u64 },

#[error("B+ tree too deep (max {})", MAX_BTREE_DEPTH)]
BtreeTooDeep { depth: u32 },

#[error("Path resolution depth exceeded ({depth}) for record {record_index} (possible cycle)")]
PathDepthExceeded { record_index: u64, depth: u32 },

#[error("Index error: {0}")]
Index(#[from] crate::attributes::IndexError),
```

### 4. `PathResolver` 構造体（path.rs 新規）

```rust
//! フルパス再構築モジュール
//!
//! 各 MFT エントリの $FILE_NAME 属性に含まれる親ディレクトリ参照を辿り、
//! NTFS root (\) からの絶対パスを構築する。

use crate::attributes::file_name::find_best_file_name;
use crate::volume::{NtfsVolume, VolumeError};
use std::collections::HashMap;

const NTFS_ROOT_RECORD: u64 = 5;
const PATH_SEPARATOR: char = '\\';
const MAX_PATH_DEPTH: u32 = 64;

/// MFT エントリ番号 → フルパス文字列のキャッシュ付き解決器。
///
/// ディレクトリパス（中間ノード）を再利用するため、
/// 大量ファイルの全パス解決時に大幅な高速化を実現する。
pub struct PathResolver {
    /// MFT entry number → resolved path
    cache: HashMap<u64, String>,
}

impl PathResolver {
    pub fn new() -> Self {
        let mut cache = HashMap::new();
        // ルートディレクトリは特殊
        cache.insert(NTFS_ROOT_RECORD, String::from("\\"));
        Self { cache }
    }
    
    /// 指定 MFT エントリのフルパスを解決する。
    pub fn resolve<F>(
        &mut self,
        record_index: u64,
        volume: &mut NtfsVolume<F>,
    ) -> Result<String, VolumeError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        self.resolve_inner(record_index, volume, 0)
    }
    
    fn resolve_inner<F>(
        &mut self,
        record_index: u64,
        volume: &mut NtfsVolume<F>,
        depth: u32,
    ) -> Result<String, VolumeError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        // キャッシュヒット
        if let Some(cached) = self.cache.get(&record_index) {
            return Ok(cached.clone());
        }
        
        if depth > MAX_PATH_DEPTH {
            return Err(VolumeError::PathDepthExceeded { record_index, depth });
        }
        
        // 自分のエントリを読む
        let entry = volume.read_record(record_index)?;
        let file_name = find_best_file_name(
            &entry.data,
            entry.header.first_attribute_offset as usize,
        ).ok_or(VolumeError::NoFileName { record_index })?;
        
        let parent_index = file_name.parent_directory.entry_number;
        
        // 親パスを再帰的に取得
        let parent_path = self.resolve_inner(parent_index, volume, depth + 1)?;
        
        // 結合
        let my_path = if parent_path == "\\" {
            format!("\\{}", file_name.filename)
        } else {
            format!("{}\\{}", parent_path, file_name.filename)
        };
        
        // キャッシュに保存
        self.cache.insert(record_index, my_path.clone());
        Ok(my_path)
    }
    
    /// キャッシュをクリア（ボリューム再オープン時等）
    pub fn clear(&mut self) {
        self.cache.clear();
        self.cache.insert(NTFS_ROOT_RECORD, String::from("\\"));
    }
}

impl Default for PathResolver {
    fn default() -> Self {
        Self::new()
    }
}
```

### 5. `NtfsVolume::full_path` メソッド（volume.rs 追加、薄いラッパー）

```rust
impl<F> NtfsVolume<F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// 指定 MFT エントリのフルパスを解決する（簡易版、キャッシュなし単発呼出向け）
    ///
    /// 複数エントリを連続解決する場合は `PathResolver` を直接使う方が高速。
    pub fn full_path(&mut self, record_index: u64) -> Result<String, VolumeError> {
        let mut resolver = crate::path::PathResolver::new();
        resolver.resolve(record_index, self)
    }
}
```

### 6. lib.rs 更新

```rust
pub mod path;  // 新規
pub use path::PathResolver;
// ... (既存)
```

### 7. `VolumeError::NoFileName` 追加（path.rs から参照されるため volume.rs に）

```rust
#[error("Record {record_index} has no $FILE_NAME attribute")]
NoFileName { record_index: u64 },
```

## 単体テスト要件（最低 8 件）

### `path.rs` のテスト（PathResolver、モック volume 使用）

PathResolver は `NtfsVolume` に依存するため、テストは結合テスト寄りになる。単体テストは `PathResolver` 単独でできる範囲（キャッシュ動作等）に限定:

1. **`path_resolver_root_returns_backslash`**: 新規作成直後、`record_index=5` で `"\\"` を返す
2. **`path_resolver_caches_resolved_paths`**: 同じ index を 2 回 resolve しても 2 度目はキャッシュ使用（モック volume のアクセス回数で検証）
3. **`path_resolver_clear_removes_cache`**: `clear()` でキャッシュが空 + ルートだけ復元される

### `volume.rs` のテスト（合成 NTFS イメージ）

Chunk 11 で確立した `build_minimal_ntfs_volume` ヘルパーを拡張:

4. **`list_directory_small_uses_index_root_only`**: 5 ファイル程度の小ディレクトリで $INDEX_ROOT のみ走査、全エントリ取得
5. **`list_directory_large_uses_index_allocation`**: 50+ ファイルのディレクトリで $INDEX_ALLOCATION の INDX ブロック走査、全エントリ取得
6. **`list_directory_returns_error_for_non_directory`**: ファイル（非ディレクトリ）の record_index で `NotADirectory`
7. **`list_directory_btree_depth_limit_protection`**: 意図的に深い B+ ツリー構造で `BtreeTooDeep` エラー
8. **`full_path_root_record_returns_backslash`**: record_index=5 で `"\\"`
9. **`full_path_user_file_returns_full_path`**: ルート直下のファイルで `"\\file.txt"`

## 結合テスト要件

`crates/fs-ntfs/tests/path_integration.rs` を作成:

### 1. **ルートディレクトリの全ファイル列挙 + フルパス取得**

```rust
#[test]
fn lists_all_files_in_root_with_full_paths() {
    let img = decompress_fixture("ntfs_healthy_small");
    let mut volume = NtfsVolume::open(make_image_reader(img, ...)).unwrap();
    
    let entries = volume.list_directory(5).unwrap();  // root = 5
    let user_entries: Vec<_> = entries.iter()
        .filter(|e| e.name().starts_with("file_"))
        .collect();
    
    // Win32 + DOS 重複排除前なので 30〜60 件
    // 重複排除後（Win32 系のみ）は 30 件のはず
    let mut win32_names: Vec<&str> = user_entries.iter()
        .filter(|e| e.file_name.namespace.is_preferred_for_display())
        .map(|e| e.name())
        .collect();
    win32_names.sort();
    win32_names.dedup();
    
    assert_eq!(win32_names.len(), 30, "Expected 30 unique user files");
    
    // フルパス取得
    let mut resolver = PathResolver::new();
    for entry in user_entries.iter().take(5) {
        let full = resolver.resolve(entry.child_ref.entry_number, &mut volume).unwrap();
        assert!(full.starts_with("\\file_"), "got {}", full);
    }
}
```

### 2. **新フィクスチャ: 多階層ディレクトリ**

`fixtures/scripts/gen_ntfs_directories.py` を新規作成（WSL Ubuntu で実行）:

```python
#!/usr/bin/env python3
"""
階層構造を持つ NTFS イメージを生成。

生成内容:
  - /file_root_001.txt 〜 /file_root_005.txt
  - /dir1/file_001.txt
  - /dir1/sub1/file_002.txt
  - /dir1/sub1/sub2/file_deeply.txt
  - /dir2/file_003.txt
  - /many/file_*.txt × 100 ($INDEX_ALLOCATION 強制用)

イメージサイズ: ~30MB
"""
# gen_ntfs_basic.py をベースに拡張
```

ground truth JSON には各ファイルのフルパスを記録:

```json
{
  "fixture_name": "ntfs_directories",
  "files": [
    { "path": "\\file_root_001.txt", ... },
    { "path": "\\dir1\\file_001.txt", ... },
    { "path": "\\dir1\\sub1\\sub2\\file_deeply.txt", ... },
    { "path": "\\many\\file_042.txt", ... }
  ]
}
```

### 3. **多階層パス再構築**

```rust
#[test]
fn reconstructs_deep_nested_paths() {
    let img = decompress_fixture("ntfs_directories");
    let mut volume = NtfsVolume::open(make_image_reader(img, ...)).unwrap();
    let ground_truth = load_ground_truth("ntfs_directories");
    
    let mut resolver = PathResolver::new();
    let mut found_paths: HashSet<String> = HashSet::new();
    
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_in_use() { continue }
        let Some(fn_) = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize) 
            else { continue };
        if fn_.filename.starts_with('$') { continue }  // システムファイル除外
        
        if let Ok(path) = resolver.resolve(idx, &mut volume) {
            found_paths.insert(path);
        }
    }
    
    // ground truth の各 path が found_paths に含まれることを確認
    for expected in ground_truth["files"].as_array().unwrap() {
        let expected_path = expected["path"].as_str().unwrap();
        assert!(found_paths.contains(expected_path), 
                "expected path {} not found", expected_path);
    }
}
```

### 4. **大規模ディレクトリの B+ ツリー走査**

```rust
#[test]
fn enumerates_100_files_directory_via_index_allocation() {
    let img = decompress_fixture("ntfs_directories");
    let mut volume = NtfsVolume::open(make_image_reader(img, ...)).unwrap();
    
    // /many ディレクトリの MFT エントリ番号を特定
    let many_dir_idx = find_directory_by_name(&mut volume, "many").unwrap();
    
    let entries = volume.list_directory(many_dir_idx).unwrap();
    let unique_files: HashSet<String> = entries.iter()
        .filter(|e| e.file_name.namespace.is_preferred_for_display())
        .map(|e| e.name().to_string())
        .collect();
    
    assert_eq!(unique_files.len(), 100, "Expected 100 files in /many/");
}
```

### 5. **総合デモテスト（プロダクト価値の可視化）**

```rust
#[test]
fn product_demo_with_full_paths() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let mut volume = NtfsVolume::open(make_image_reader(img, ...)).unwrap();
    let mut resolver = PathResolver::new();
    
    println!("\n=== DDS Recovery Workbench - Phase 1 (post-Chunk 13) ===\n");
    
    let mut live_count = 0;
    let mut deleted_count = 0;
    
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        let Some(fn_) = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize) 
            else { continue };
        if !fn_.filename.starts_with("file_") { continue }
        
        let path = resolver.resolve(idx, &mut volume).unwrap_or_else(|_| fn_.filename.clone());
        let status = if entry.header.is_deleted() { "[DELETED]" } else { "[Live]   " };
        println!("  {} {}", status, path);
        
        if entry.header.is_deleted() { deleted_count += 1 } else { live_count += 1 }
    }
    
    println!("\nLive: {}, Deleted: {}", live_count, deleted_count);
    assert_eq!(live_count, 25);
    assert_eq!(deleted_count, 5);
}
```

## Cargo.toml 設定

変更不要。

## 制約

- **行数上限**: 
  - `volume.rs` への追加: 既存 ~220行 + 150行 = ~370行（複雑性考慮で許容）
  - `path.rs` 新規: 80〜100 行
- **単体テスト最低 8 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、`from_be_bytes` 0 件、書き込み API 0 件**
- **MAX_BTREE_DEPTH = 32、MAX_PATH_DEPTH = 64** で破損データ防護
- **新フィクスチャ `ntfs_directories.img.zst` を生成** + ground truth JSON

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（既存 + 新規 ≥9 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥4 件）
- [ ] `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: warning 0
- [ ] `fixtures/images/ntfs_directories.img.zst` 生成済み + JSON 同梱
- [ ] 多階層パス（`\dir1\sub1\sub2\file_deeply.txt`）が ground truth と一致
- [ ] 100 ファイルディレクトリの全エントリが取得可能（$INDEX_ALLOCATION 経由）
- [ ] `cargo doc -p dds-fs-ntfs --no-deps`: 全公開 API に rustdoc
- [ ] `grep -r 'unsafe\|from_be_bytes\|fn write' crates/fs-ntfs/src/` で 0 件

## 関連 FR 要件

- **FR-LIVE-04** (ファイルツリー構築) ← **完全達成**
- **FR-LIVE-05** (削除エントリ可視化) ← パス付きで強化
- **FR-LIVE-06** (メタデータ表示) ← パスメタデータ追加

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. 次のステップ:
   - **Chunk 14**: `NtfsFile` 高レベル統合型（path + name + meta + content を 1 構造体に）
   - **Chunk 15+**: 業務統合層着手（wish-match で具体的なお客様希望リスト突合）

---

## 注意事項

### ディレクトリでない MFT エントリの判定

「$INDEX_ROOT 属性の存在」で判定するのが最も確実（書籍 Chapter 12 推奨）。$STANDARD_INFORMATION の `is_directory()` フラグでも判定可能だが、稀に不整合あり。Phase 1 は前者を採用。

### Win32 / DOS 重複は `list_directory` の戻り値に含む

仕様準拠で全エントリを yield。重複排除（Win32 のみ採用）は呼び出し側の責務。これは Chunk 8 で `find_all_file_names` を `find_best_file_name` でフィルタする設計と整合。

### 削除ファイルはディレクトリリスティングに現れない

書籍 Chapter 12 の動作: 削除時にインデックスエントリが除去される。`list_directory` は**生存ファイルのみ**を返す。削除ファイル復旧には `iter_records` （Chunk 11）+ `full_path` （このチャンク）の組み合わせが必要。

ただし**削除ファイルでも `full_path` は動く**（$FILE_NAME 属性は MFT 内に残っているため）。親ディレクトリが既に再利用されていなければ、削除ファイルでも正しいパスが復元できる。

### `walk_indx_block` の改善余地

実装スケッチで `block_size = 4096` を固定値としているが、本来は `$INDEX_ROOT::bytes_per_index_record` から取得すべき。実装時は dir_entry から $INDEX_ROOT を再取得して動的に決定するか、`NtfsVolume` 内にキャッシュしておく設計に改善する。

### キャッシュ汚染リスク

`PathResolver` のキャッシュは MFT エントリ番号ベース。**同じボリュームを再度開いた時**は新しい `PathResolver` を作るか `clear()` を呼ぶこと。

### ハードリンクの扱い

書籍 Chapter 12 が言及するハードリンクは、1 ファイルが複数のパスを持ち得る。`PathResolver::resolve` は **最初の Win32 系 `$FILE_NAME`** に基づくパスを返す（`find_best_file_name` 経由）。ハードリンクの全パス取得は将来の API で対応。

### システムファイルの扱い

NTFS は `$MFT`, `$LogFile`, `$Volume`, `$AttrDef` 等のシステムメタファイル（MFT エントリ 0〜23）を持つ。これらは `$` で始まる名前。`product_demo_with_full_paths` ではフィルタしている。業務統合層でも同様のフィルタが必要。

### 性能

`PathResolver` のキャッシュにより、N ファイル全パス解決の計算量は O(深さ平均 × N) → O(N + 深さ合計)。実用上問題ない。

---

## 質問が必要なケース

- ハードリンクされたファイルの全パス取得 API を Phase 1 でサポートすべきか
- `$DELETED` フォルダ内のファイル（ゴミ箱）の特別扱いを Phase 1 でサポートすべきか
- 削除ディレクトリの子ファイル群のパス再構築（途中の削除ディレクトリを経由する場合）

---

## 完了報告例

```markdown
## Chunk 13 完了報告

- **クレート**: dds-fs-ntfs
- **新規ファイル**: 
  - crates/fs-ntfs/src/path.rs (新規, 90行 + テスト 30行)
- **既存ファイル更新**:
  - crates/fs-ntfs/src/volume.rs (+150行: list_directory, walk_*, full_path)
  - crates/fs-ntfs/src/lib.rs (+2行: path モジュール公開)
- **新フィクスチャ**: ntfs_directories.img.zst（多階層 + 100ファイルディレクトリ）
- **公開API追加**:
  - `DirectoryListing`, `PathResolver`
  - `NtfsVolume::list_directory`, `NtfsVolume::full_path`
- **テスト統計**:
  - 単体: 既存 114 + 新規 9 = **123 件 pass**
  - 結合: 既存 26 + 新規 5 = **31 件 pass**
- **品質**: clippy 0 warning, unsafe 0, 書き込み API 0
- **🎉 NTFS リーダ実用形完成**: 削除ファイル含む全エントリにフルパス付与可能
- **プロダクトデモテスト出力**:
  ```
  [Live]    \file_000.txt
  [DELETED] \file_003.txt
  [Live]    \dir1\file_001.txt
  [Live]    \dir1\sub1\file_002.txt
  [Live]    \dir1\sub1\sub2\file_deeply.txt
  [Live]    \many\file_042.txt
  ...
  Live: 25, Deleted: 5
  ```
- **関連 FR**: FR-LIVE-04 (完全達成), FR-LIVE-05, FR-LIVE-06

→ tester エージェントへ引き継ぎお願いします
```
