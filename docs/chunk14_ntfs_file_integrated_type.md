# Chunk 14 指示: NtfsFile 高レベル統合型

このチャンクで **「1 つのファイルを 1 つの型で表す」** 完成形 API を作ります。Chunks 4-13 で構築した個別の関数群（`parse_mft_entry` / `find_best_file_name` / `parse_standard_information` / `extract_main_data_stream` / `PathResolver`）を、`NtfsFile` という統合型で束ね、業務統合層（wish-match, recovery）から呼びやすい形にします。

> 🎯 完了時点で「`volume.iter_files()` → `NtfsFile` を列挙」が業務統合層の標準呼び出しに。Phase 1 NTFS リーダー実装の**最終形**。

---

## 目的

複数の純粋関数の呼び出しを 1 つの統合型に集約し、API を簡潔にする:

### Before（Chunk 13 まで）

```rust
let mut resolver = PathResolver::new();
for (idx, result) in volume.iter_records() {
    let Ok(entry) = result else { continue };
    let Some(fn_) = find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize) 
        else { continue };
    if fn_.filename.starts_with('$') { continue }
    
    let si_attr = find_attribute(&entry.data, entry.header.first_attribute_offset as usize, AttributeType::StandardInformation);
    // ... さらに $SI のコンテンツ抽出
    // ... さらに $DATA の取得
    // ... さらに full_path の解決
    
    let path = resolver.resolve(idx, &mut volume).unwrap_or_else(|_| fn_.filename.clone());
    let status = if entry.header.is_deleted() { "[DELETED]" } else { "[Live]   " };
    println!("{} {}", status, path);
}
```

### After（Chunk 14 以降）

```rust
for file in volume.iter_files() {
    let file = file?;
    if !file.is_user_file() { continue }  // システムメタファイル除外
    
    let status = if file.is_deleted { "[DELETED]" } else { "[Live]   " };
    println!("{} {} ({} bytes, modified {:?})", 
        status, file.path, file.size, file.modified);
}
```

**コード量がほぼ半分**、可読性が大幅向上。

## 対象クレート

`crates/fs-ntfs/`

## 仕様参照

### 設計参考

- 書籍 **Chapter 11「FILES AND BASE INODE」**: MFT エントリとファイル概念の対応
- 既存実装: Chunks 4-13 で構築した全 NTFS パーサ群

このチャンクは**新しい NTFS 知識は不要**。既存実装の統合のみ。

## 実装内容

### モジュール配置

`crates/fs-ntfs/src/file.rs` を新規作成（既存の `volume.rs` と同階層、トップレベル）。

理由: `NtfsFile` は `NtfsVolume` と並ぶ主要型として扱う。`attributes/` 配下は不適切（属性ではない）。

### 1. `FileContentRef` enum

NtfsFile を owned 型として保持できるよう、ライフタイム不要の所有データ形式:

```rust
use crate::attributes::Run;

/// NtfsFile が保持するファイル内容情報。
///
/// 常駐の場合: バイト列を直接所有
/// 非常駐の場合: 読み取りに必要な runs と real_size のみ保持（実データは要求時に読む）
#[derive(Debug, Clone)]
pub enum FileContentRef {
    /// MFT エントリ内に格納された小ファイル（content_size バイト）
    Resident(Vec<u8>),
    
    /// クラスタに分散保存された大ファイル
    NonResident {
        real_size: u64,
        runs: Vec<Run>,
    },
    
    /// $DATA 属性なし（ディレクトリ・空ファイルなど）
    None,
}

impl FileContentRef {
    pub fn is_resident(&self) -> bool {
        matches!(self, FileContentRef::Resident(_))
    }
    
    pub fn size(&self) -> u64 {
        match self {
            FileContentRef::Resident(bytes) => bytes.len() as u64,
            FileContentRef::NonResident { real_size, .. } => *real_size,
            FileContentRef::None => 0,
        }
    }
}
```

### 2. `NtfsFile` 構造体

```rust
use chrono::{DateTime, Utc};
use crate::attributes::{FileAttributes, MftReference};

/// 1 つの NTFS ファイル/ディレクトリの統合情報。
///
/// MFT エントリから抽出した全ての情報を 1 つの owned 型に統合する。
/// 業務統合層（wish-match, recovery）はこの型に対して操作する。
#[derive(Debug, Clone)]
pub struct NtfsFile {
    /// MFT エントリ番号（このファイルの一意 ID）
    pub record_index: u64,
    
    /// NTFS 形式のフルパス（例: `\dir1\sub2\file.txt`）
    pub path: String,
    
    /// ファイル名のみ（例: `file.txt`）
    pub name: String,
    
    /// 親ディレクトリの MFT 参照（パス再構築・ハードリンク用）
    pub parent: MftReference,
    
    /// ディレクトリか
    pub is_directory: bool,
    
    /// 削除済みエントリか（In Use フラグ = 0）
    pub is_deleted: bool,
    
    /// 作成日時（$STANDARD_INFORMATION から）
    pub created: Option<DateTime<Utc>>,
    
    /// 内容更新日時
    pub modified: Option<DateTime<Utc>>,
    
    /// アクセス日時
    pub accessed: Option<DateTime<Utc>>,
    
    /// MFT エントリ自体の更新日時
    pub mft_modified: Option<DateTime<Utc>>,
    
    /// ファイル属性フラグ（$SI から、なければ $FILE_NAME 由来）
    pub file_attributes: FileAttributes,
    
    /// Alternate Data Stream を持つか
    pub has_alternate_streams: bool,
    
    /// 圧縮属性が立っているか
    pub is_compressed: bool,
    
    /// 暗号化属性が立っているか
    pub is_encrypted: bool,
    
    /// スパース属性が立っているか
    pub is_sparse: bool,
    
    /// メイン $DATA ストリームの内容参照
    pub content: FileContentRef,
    
    /// ファイルサイズ（メイン $DATA の real_size or content_size、無ければ 0）
    pub size: u64,
}

impl NtfsFile {
    /// NTFS のルートディレクトリか（MFT entry 5）
    pub fn is_root(&self) -> bool {
        self.record_index == 5
    }
    
    /// システムメタファイル（MFT entry 0〜23）か
    pub fn is_system_metafile(&self) -> bool {
        self.record_index < 24
    }
    
    /// ユーザファイルか
    /// （削除されていてもユーザファイル扱い、復旧対象）
    pub fn is_user_file(&self) -> bool {
        !self.is_directory && !self.is_system_metafile()
    }
    
    /// ファイル拡張子（小文字、`.txt` の `txt` 部分のみ）。なければ None。
    pub fn extension(&self) -> Option<String> {
        self.name.rsplit_once('.').map(|(_, ext)| ext.to_lowercase())
    }
    
    /// 復旧優先度判定用: 圧縮・暗号化されていない、削除済み、ユーザファイル
    pub fn is_simple_deleted_user_file(&self) -> bool {
        self.is_deleted 
            && self.is_user_file() 
            && !self.is_compressed 
            && !self.is_encrypted
    }
}
```

### 3. ファイル構築関数（file.rs 内、`pub(crate)`）

```rust
use crate::attribute::{AttributeHeader, AttributeType};
use crate::attributes::{
    extract_all_data_streams, find_attribute, find_best_file_name,
    parse_runlist, parse_standard_information, DataContent,
};
use crate::path::PathResolver;
use crate::volume::{NtfsVolume, VolumeError};

/// 指定 MFT エントリから NtfsFile を構築する。
///
/// 戻り値:
/// - `Ok(Some(file))`: 構築成功
/// - `Ok(None)`: $FILE_NAME 属性がない（システム未使用エントリ等）→ スキップ推奨
/// - `Err(e)`: パースエラー
pub(crate) fn build_file_for_record<F>(
    volume: &mut NtfsVolume<F>,
    record_index: u64,
    resolver: &mut PathResolver,
) -> Result<Option<NtfsFile>, VolumeError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let entry = volume.read_record(record_index)?;
    let first_attr_offset = entry.header.first_attribute_offset as usize;
    
    // ファイル名取得（なければスキップ対象）
    let Some(fn_) = find_best_file_name(&entry.data, first_attr_offset) else {
        return Ok(None);
    };
    
    let is_deleted = entry.header.is_deleted();
    let is_directory = entry.header.is_directory();
    let parent = fn_.parent_directory;
    let name = fn_.filename.clone();
    
    // フルパス解決（ルート(5)はスペシャルケース、削除エントリは親が再利用済みで失敗の可能性）
    let path = if record_index == 5 {
        "\\".to_string()
    } else {
        resolver.resolve(record_index, volume).unwrap_or_else(|_| format!("\\{}", name))
    };
    
    // $SI から正確なタイムスタンプ + 属性フラグを取得（なければ $FILE_NAME 由来で代用）
    let (created, modified, accessed, mft_modified, file_attrs) =
        extract_si_or_fallback(&entry, first_attr_offset, &fn_);
    
    // $DATA ストリーム全部を取得
    let data_streams = extract_all_data_streams(&entry.data, first_attr_offset);
    let main_stream = data_streams.iter().find(|s| s.name.is_empty());
    let has_alternate_streams = data_streams.iter().any(|s| !s.name.is_empty());
    
    let (content, size, is_compressed, is_encrypted, is_sparse) = match main_stream {
        Some(stream) => {
            let (content, size) = match &stream.content {
                DataContent::Resident { bytes, size } => {
                    (FileContentRef::Resident(bytes.to_vec()), *size as u64)
                }
                DataContent::NonResident {
                    real_size,
                    runlist_offset_in_attr,
                    attribute_raw,
                    ..
                } => {
                    let runlist_bytes = &attribute_raw[*runlist_offset_in_attr..];
                    let runs = parse_runlist(runlist_bytes)?;
                    (
                        FileContentRef::NonResident { real_size: *real_size, runs },
                        *real_size,
                    )
                }
            };
            (content, size, stream.is_compressed, stream.is_encrypted, stream.is_sparse)
        }
        None => (FileContentRef::None, 0, false, false, false),
    };
    
    Ok(Some(NtfsFile {
        record_index,
        path,
        name,
        parent,
        is_directory,
        is_deleted,
        created,
        modified,
        accessed,
        mft_modified,
        file_attributes: file_attrs,
        has_alternate_streams,
        is_compressed,
        is_encrypted,
        is_sparse,
        content,
        size,
    }))
}

/// $SI 抽出（フォールバックで $FILE_NAME の値を使う）
fn extract_si_or_fallback(
    entry: &crate::mft::MftEntry,
    first_attr_offset: usize,
    fn_: &crate::attributes::FileName,
) -> (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    FileAttributes,
) {
    let Some(si_attr) = find_attribute(
        &entry.data,
        first_attr_offset,
        AttributeType::StandardInformation,
    ) else {
        return (
            fn_.created.to_datetime(),
            fn_.modified.to_datetime(),
            fn_.accessed.to_datetime(),
            fn_.mft_modified.to_datetime(),
            fn_.file_attributes,
        );
    };
    
    let AttributeHeader::Resident { resident, .. } = &si_attr.header else {
        return (None, None, None, None, fn_.file_attributes);
    };
    
    let content_start = resident.content_offset as usize;
    let content_end = content_start + resident.content_size as usize;
    if content_end > si_attr.raw.len() {
        return (None, None, None, None, fn_.file_attributes);
    }
    
    match parse_standard_information(&si_attr.raw[content_start..content_end]) {
        Ok(si) => (
            si.created.to_datetime(),
            si.modified.to_datetime(),
            si.accessed.to_datetime(),
            si.mft_modified.to_datetime(),
            si.file_attributes,
        ),
        Err(_) => (None, None, None, None, fn_.file_attributes),
    }
}
```

### 4. `NtfsFileIterator` 構造体（file.rs 内）

```rust
/// 全 NtfsFile を順次列挙するイテレータ。
///
/// $FILE_NAME 属性のないエントリは自動的にスキップ。
/// 個別エントリのパースエラーは Result として yield、イテレーション継続。
pub struct NtfsFileIterator<'a, F> {
    volume: &'a mut NtfsVolume<F>,
    current: u64,
    resolver: PathResolver,
}

impl<'a, F> NtfsFileIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    pub fn new(volume: &'a mut NtfsVolume<F>) -> Self {
        Self {
            volume,
            current: 0,
            resolver: PathResolver::new(),
        }
    }
}

impl<'a, F> Iterator for NtfsFileIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    type Item = Result<NtfsFile, VolumeError>;
    
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current >= self.volume.total_records() {
                return None;
            }
            let idx = self.current;
            self.current += 1;
            
            match build_file_for_record(self.volume, idx, &mut self.resolver) {
                Ok(Some(file)) => return Some(Ok(file)),
                Ok(None) => continue,  // FILE_NAME なしはスキップ
                Err(e) => return Some(Err(e)),
            }
        }
    }
}
```

### 5. `NtfsVolume` への追加メソッド（volume.rs に追加）

```rust
use crate::file::{build_file_for_record, NtfsFile, NtfsFileIterator, FileContentRef};
use crate::attributes::read_runs_with;

impl<F> NtfsVolume<F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// 全 NtfsFile を列挙するイテレータを返す。
    pub fn iter_files(&mut self) -> NtfsFileIterator<'_, F> {
        NtfsFileIterator::new(self)
    }
    
    /// 単一の MFT エントリから NtfsFile を構築する。
    /// （単発呼び出し向け、複数呼ぶなら iter_files の方が効率良い）
    pub fn build_file(&mut self, record_index: u64) -> Result<Option<NtfsFile>, VolumeError> {
        let mut resolver = PathResolver::new();
        build_file_for_record(self, record_index, &mut resolver)
    }
    
    /// NtfsFile の実データを取得する。
    ///
    /// 常駐: 既に bytes を持っているのでそのまま返却
    /// 非常駐: runs を辿ってクラスタを読み、実バイト列を構築
    pub fn read_file_content(&mut self, file: &NtfsFile) -> Result<Vec<u8>, VolumeError> {
        match &file.content {
            FileContentRef::Resident(bytes) => Ok(bytes.clone()),
            FileContentRef::NonResident { real_size, runs } => {
                let cluster_size = self.cluster_size();
                // 分割借用: self.read_clusters だけ &mut で借りる
                let read_fn = &mut self.read_clusters;
                read_runs_with(runs, cluster_size, *real_size, |lcn, count| {
                    (read_fn)(lcn, count)
                })
                .map_err(VolumeError::Runlist)
            }
            FileContentRef::None => Ok(Vec::new()),
        }
    }
}
```

### 6. lib.rs 更新

```rust
pub mod file;
pub use file::{NtfsFile, NtfsFileIterator, FileContentRef};
// ... (既存)
```

## 単体テスト要件（最低 8 件）

### `file.rs` のテスト

NtfsFile のメソッドは入力 → 出力が決定論的なので、構造体リテラルで NtfsFile を組み立ててテスト:

1. **`is_root_returns_true_for_record_5`**: record_index=5 で `is_root() == true`
2. **`is_system_metafile_for_records_0_to_23`**: 0, 23 で true、24 で false
3. **`is_user_file_excludes_directory_and_system`**: ディレクトリ・システムは false、通常ファイルは true（削除済み含む）
4. **`extension_basic_cases`**: `"foo.txt"` → `"txt"`, `"foo.TXT"` → `"txt"`, `"foo"` → `None`, `"foo.tar.gz"` → `"gz"`
5. **`is_simple_deleted_user_file_combinations`**: 削除 + ユーザファイル + 非圧縮 + 非暗号化 のみ true
6. **`file_content_ref_size_correct`**: Resident(50 bytes) → 50、NonResident(real_size=1024) → 1024、None → 0
7. **`file_content_ref_is_resident`**: Resident は true、NonResident と None は false

### iter_files ヘルパー / build_file テスト

合成 NTFS イメージ（Chunk 11 で確立した `build_minimal_ntfs_volume`）に基づく:

8. **`build_file_returns_none_for_entry_without_filename`**: $FILE_NAME 属性のないエントリで `Ok(None)`
9. **`build_file_extracts_all_timestamps`**: $SI 含むエントリで created/modified/accessed/mft_modified が Some
10. **`build_file_falls_back_to_filename_when_si_missing`**: $SI 欠落時、$FILE_NAME のタイムスタンプを使う

## 結合テスト要件

`crates/fs-ntfs/tests/ntfs_file_integration.rs` を作成。既存ヘルパーフル活用:

### 1. **全フィクスチャでの iter_files 列挙**

```rust
#[test]
fn iter_files_enumerates_all_three_fixtures() {
    for (fixture_name, expected_user_files) in [
        ("ntfs_healthy_small", 30),
        ("ntfs_with_5_deletions_small", 30),
        ("ntfs_large_files", 20),
        ("ntfs_directories", 109),
    ] {
        let img = decompress_fixture(fixture_name);
        let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
        let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
        
        let user_files: Vec<NtfsFile> = volume.iter_files()
            .filter_map(Result::ok)
            .filter(|f| f.is_user_file())
            .collect();
        
        // Win32+DOS 重複や $FILE_NAME 由来の重複を除外して unique なファイルを数える
        // record_index がユニークキー
        let unique_user_files: HashSet<u64> = user_files.iter().map(|f| f.record_index).collect();
        
        assert!(
            unique_user_files.len() >= expected_user_files,
            "Fixture {} expected ≥{} user files, got {}",
            fixture_name, expected_user_files, unique_user_files.len()
        );
    }
}
```

### 2. **read_file_content で全フィクスチャの SHA256 整合性確認**

```rust
#[test]
fn read_file_content_matches_ground_truth_sha256() {
    let img = decompress_fixture("ntfs_directories");
    let ground_truth = load_ground_truth("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // ground truth の {path → sha256} マップ構築
    let expected: HashMap<String, String> = ground_truth["files"].as_array().unwrap().iter()
        .map(|f| (
            f["path"].as_str().unwrap().to_string(),
            f["content_hash_sha256"].as_str().unwrap().to_string(),
        ))
        .collect();
    
    // 全ユーザファイルを iter_files 経由で取得、内容ハッシュ比較
    let files: Vec<NtfsFile> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file())
        .collect();
    
    let mut matched = 0;
    for file in &files {
        if let Some(expected_hash) = expected.get(&file.path) {
            let content = volume.read_file_content(file).unwrap();
            let actual_hash = sha256_hex(&content);
            assert_eq!(&actual_hash, expected_hash, 
                "Hash mismatch for {}: expected {}, got {}", 
                file.path, expected_hash, actual_hash);
            matched += 1;
        }
    }
    
    assert!(matched >= 100, "Expected at least 100 files matched (ground truth has 109), got {}", matched);
}
```

### 3. **削除ファイル復旧の最終デモテスト**

```rust
#[test]
fn product_demo_with_ntfs_file_api() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Phase 1 NTFS Final Demo ===\n");
    
    // iter_files でファイルを集める
    let files: Vec<NtfsFile> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && f.name.starts_with("file_"))
        .collect();
    
    let live: Vec<&NtfsFile> = files.iter().filter(|f| !f.is_deleted).collect();
    let deleted: Vec<&NtfsFile> = files.iter().filter(|f| f.is_deleted).collect();
    
    println!("Recoverable files:");
    for f in &deleted {
        let content = volume.read_file_content(f).unwrap();
        let hash = sha256_hex(&content);
        println!("  [DELETED] #{:<4} {} ({} bytes, sha256: {}...)", 
            f.record_index, f.path, f.size, &hash[..16]);
    }
    
    println!("\nLive files:");
    for f in live.iter().take(5) {
        println!("  [Live]    #{:<4} {} ({} bytes)", 
            f.record_index, f.path, f.size);
    }
    
    println!("\n=== Summary ===");
    println!("Live files:    {}", live.len());
    println!("Deleted files: {}", deleted.len());
    
    assert_eq!(live.len(), 25);
    assert_eq!(deleted.len(), 5);
    
    // 全削除ファイル復旧成功を SHA256 で実証
    for f in &deleted {
        let content = volume.read_file_content(f).unwrap();
        assert!(!content.is_empty(), "Deleted file content should not be empty");
    }
}
```

### 4. **多階層パス + 拡張子フィルタの組合せ**

```rust
#[test]
fn iter_files_supports_path_and_extension_filtering() {
    let img = decompress_fixture("ntfs_directories");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let txt_files: Vec<NtfsFile> = volume.iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && f.extension().as_deref() == Some("txt"))
        .collect();
    
    // 109 ファイル全部 .txt
    let unique: HashSet<u64> = txt_files.iter().map(|f| f.record_index).collect();
    assert_eq!(unique.len(), 109);
    
    // 多階層パス確認
    let deeply = txt_files.iter().find(|f| f.path == "\\dir1\\sub1\\sub2\\file_deeply.txt");
    assert!(deeply.is_some(), "Expected to find deeply nested file");
    
    // many/ 配下が 100 件
    let many_files: Vec<_> = txt_files.iter().filter(|f| f.path.starts_with("\\many\\")).collect();
    assert_eq!(many_files.len(), 100);
}
```

## Cargo.toml 設定

変更不要。

## 制約

- **行数上限**: 
  - `file.rs` 新規: 200 行以内
  - `volume.rs` 追加分: 50 行以内（既存 ~400 行に乗せる形）
- **単体テスト最低 8 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件、`from_be_bytes` 0 件、書き込み API 0 件**
- **エラー型は既存 `VolumeError` を再利用**（新エラー型不要）
- **NtfsFile は owned 型**（ライフタイム不要、業務統合層に渡しやすい）

## 完了条件チェックリスト

- [ ] `cargo check -p dds-fs-ntfs` がエラーなし
- [ ] `cargo test --lib -p dds-fs-ntfs` が全パス（既存 + 新規 ≥10 件）
- [ ] `cargo test -p dds-fs-ntfs` が全パス（既存 + 新規結合 ≥4 件）
- [ ] `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc -p dds-fs-ntfs --no-deps`: 全公開 API に rustdoc
- [ ] 4 つの既存フィクスチャ全てで iter_files が動作
- [ ] product_demo_with_ntfs_file_api テストが pass（25 live + 5 deleted）
- [ ] read_file_content で全 109 ファイル（ntfs_directories）の SHA256 一致
- [ ] `grep -r 'unsafe\|from_be_bytes\|fn write' crates/fs-ntfs/src/` で 0 件

## 関連 FR 要件

- **FR-LIVE-01** (NTFS読み取り) ← **API 完成形**
- **FR-LIVE-04** (ファイルツリー構築) ← NtfsFile.path で完全達成
- **FR-LIVE-05** (削除エントリ可視化) ← `is_deleted` フラグで明示
- **FR-LIVE-06** (メタデータ表示) ← 全タイムスタンプ + 属性フラグ
- **FR-REC-01** (目標優先抽出) ← `is_user_file()`, `extension()` 等のフィルタが業務層で使える
- **FR-REC-04** (データ整合性) ← read_file_content + SHA256 で実証

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1 NTFS リーダー実装完成**
4. 次のステップ:
   - **Chunk 15**: wish-match 基盤（希望リスト管理 + パターン定義）
   - **Chunk 16+**: 突合エンジン + 復旧パイプライン統合

---

## 注意事項

### NtfsFile は owned 型

`String`, `Vec<u8>`, `Vec<Run>` で完全に所有された型なので、`Vec<NtfsFile>` で集めて後で処理できる。ライフタイムなしの設計が業務統合層から扱いやすい根本理由。

### 削除エントリの path 解決

削除されたファイルの**親ディレクトリも削除されている**場合、PathResolver は親エントリの解決に失敗する。フォールバックとして `\filename` 形式（ルート直下と仮定）を使う設計。これは部分復旧の一形態として許容。

業務統合層で「親不明」を明示したい場合、将来的に `parent_status: Option<ParentStatus>` フィールドの追加余地を残す。

### Win32 + DOS の重複は record_index で排除

同じ MFT エントリは 1 つの NtfsFile に対応する。`find_best_file_name` が Win32 名を優先するため、DOS 短縮名は NtfsFile に現れない。

### システムメタファイル判定の境界

`record_index < 24` はヒューリスティック。NTFS 仕様上、エントリ 0〜15 が予約済み、16〜23 が拡張用予約だが、Windows のバージョンや形式オプションで変動する余地はある。Phase 1 では 24 を境界として割り切る。

業務統合層で厳密に判定したい場合は `name.starts_with('$')` の併用も推奨。

### `$SI` 欠落時のフォールバック

$STANDARD_INFORMATION 属性が欠落・破損している場合、`$FILE_NAME` 内のタイムスタンプを代替使用。$FILE_NAME のタイムスタンプはファイル作成時のスナップショットなので、$SI ほど正確ではないが、表示には十分。

### `read_file_content` のメモリ使用量

非常駐ファイルは全体を `Vec<u8>` に読む。1GB ファイルでは 1GB のメモリ確保。Phase 1 では小〜中サイズ前提でOK。大容量対応は Phase 2 でストリーミング API（ジェネレータ的）を導入する余地を残す。

### `parse_runlist` のタイミング

`build_file_for_record` で runlist を即時パースする設計。`read_file_content` 時に再パースしない。これにより、ファイル列挙の段階で runlist の妥当性チェックも完了する。runlist パースエラーは `build_file_for_record` の段階で `Err` として伝播。

### ハードリンク

NTFS のハードリンクは同じ MFT エントリを複数の親ディレクトリから参照する。`iter_files` は MFT エントリベースなので、ハードリンクされたファイルは **1 回のみ yield**。表示パスは `find_best_file_name` の選択結果（最初の Win32）。

全ハードリンク列挙 API は Phase 1 範囲外。`MftReference` の `parent` フィールドは保持されるので、業務統合層で必要に応じて拡張可能。

---

## 質問が必要なケース

- システムメタファイル境界（24 で固定 or `$` プレフィックス併用）の Phase 1 方針
- `read_file_content` のメモリ管理（全部 Vec or イテレータ風）の優先度
- ハードリンク列挙 API の Phase 1 必要性

---

## 完了報告例

```markdown
## Chunk 14 完了報告

- **クレート**: dds-fs-ntfs
- **新規ファイル**: 
  - crates/fs-ntfs/src/file.rs (新規, 180行 + テスト 70行)
- **既存ファイル更新**:
  - crates/fs-ntfs/src/volume.rs (+45行: iter_files, build_file, read_file_content)
  - crates/fs-ntfs/src/lib.rs (+3行: file モジュール公開)
- **公開API追加**:
  - `NtfsFile` 構造体（17 フィールド + 5 メソッド）
  - `NtfsFileIterator<'a, F>` 
  - `FileContentRef` enum
  - `NtfsVolume::iter_files`, `NtfsVolume::build_file`, `NtfsVolume::read_file_content`
- **テスト統計**:
  - 単体: 既存 123 + 新規 10 = **133 件 pass**
  - 結合: 既存 31 + 新規 4 = **35 件 pass**
- **品質**: clippy 0 warning, unsafe 0, 書き込み API 0
- **🎉 Phase 1 NTFS リーダー実装完成**: NtfsFile 型で全情報を 1 つの owned 型に統合
- **API 簡潔化実証**:
  ```
  Before: 15行（resolver + iter_records + 4つのパース呼び出し + フィルタ）
  After:  5行（volume.iter_files() + filter + display）
  ```
- **プロダクトデモ出力**:
  ```
  Recoverable files:
    [DELETED] #67   \file_003.txt (50 bytes, sha256: abc12345...)
    [DELETED] #71   \file_007.txt (50 bytes, sha256: def67890...)
    ...
  Live files:    25
  Deleted files: 5
  ```
- **関連 FR**: FR-LIVE-01/04/05/06, FR-REC-01/04

→ tester エージェントへ引き継ぎお願いします
```
