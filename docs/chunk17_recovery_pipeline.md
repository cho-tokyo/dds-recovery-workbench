# Chunk 17 指示: 復旧パイプライン基盤（recovery エンジン）

このチャンクで **「希望リスト → 実ファイル抽出」** の end-to-end が動きます。Chunks 4-16 で構築した NTFS リーダ + wish-match の上に、復旧結果を実ファイルとして書き出すパイプラインを構築します。

> 🎯 完了時点で「`engine.recover_files(volume, wishlist)` の 1 行で復旧フォルダにファイルが出てくる」状態に。**Phase 1 NTFS-α リリース** への大きな一歩。

---

## 目的

復旧パイプラインの基盤を構築する:

1. **マッチ → 抽出 → 書き込み** の自動化（end-to-end）
2. **フォルダ階層の再現**（NTFS の `\dir1\sub2\file.txt` → 出力先の `dir1/sub2/file.txt`）
3. **削除/生存の区別**（出力先で `deleted/` `live/` サブフォルダに分離）
4. **衝突処理**（同名ファイルを安全に共存）
5. **パストラバーサル防止**（`..` 等で出力ディレクトリ外へエスケープを阻止）
6. **Windows ファイル名サニタイズ**（NTFS で許可される名前が Windows OS で開けない問題対応）
7. **詳細なレポート**（成功/失敗/スキップを per-file で記録）

## 対象クレート

`crates/recovery/`（Chunk 1 で空スケルトン作成済み、本実装）

## 重要な設計原則

### read/write の境界を明確化

| 対象 | アクセス |
|---|---|
| ソースディスク（NTFS イメージ） | **READ ONLY** ← 復旧対象、絶対書き込み禁止 |
| 出力ディレクトリ | READ + WRITE OK ← 復旧結果の書き先 |

**ソースディスクへの書き込みは Chunks 4-16 で確立した「unsafe / write API 0 件」原則を維持**。recovery クレートは出力先のみに `std::fs::write` 等を使う。

## 仕様参照

### ビジネス要件

- **FR-REC-01**: 目標優先抽出 — Wishlist マッチ結果を優先度順に復旧
- **FR-REC-02**: 復旧結果の出力先指定 — お客様/CSが指定したディレクトリへ
- **FR-REC-03**: 衝突解決 — 同名ファイル衝突時の安全な処理
- **FR-REC-04**: データ整合性 — SHA256 等で復旧結果の検証可能性

### 既存の参照

- 既存実装: `dds-fs-ntfs` (NtfsVolume, NtfsFile), `dds-wish-match` (Wishlist, FileInfo, match_files)

## 実装内容

### モジュール構成

```
crates/recovery/
├── Cargo.toml
└── src/
    ├── lib.rs        ← re-export
    ├── error.rs      ← RecoveryError
    ├── options.rs    ← RecoveryOptions, ConflictStrategy
    ├── report.rs     ← RecoveryReport, RecoveredEntry, FailedEntry, SkippedEntry
    ├── sanitize.rs   ← ファイル名 + パスのサニタイズ
    └── engine.rs     ← RecoveryEngine（メインロジック）
```

### Cargo.toml

```toml
[package]
name = "dds-recovery"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono.workspace = true
sha2.workspace = true
thiserror.workspace = true
dds-core.workspace = true
dds-wish-match.workspace = true
dds-fs-ntfs.workspace = true

[dev-dependencies]
tempfile = "3.10"  # 統合テスト用の一時ディレクトリ
```

`tempfile` をワークスペース dev-dependencies に追加。

### 1. `error.rs`

確立されたエラー命名規約に準拠:

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid output directory: {path:?} ({reason})")]
    InvalidOutputDir { path: PathBuf, reason: String },
    
    #[error("Path traversal attempt: {path} contains '..' or escapes output dir")]
    PathTraversal { path: String },
    
    #[error("Filename cannot be sanitized: {original:?}")]
    UnsanitizableFilename { original: String },
    
    #[error("Volume error: {0}")]
    Volume(#[from] dds_fs_ntfs::VolumeError),
    
    #[error("Could not find unique filename after {attempts} attempts")]
    UniqueFilenameExhausted { attempts: u32 },
}
```

### 2. `options.rs`

```rust
/// 復旧時の動作オプション。
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    /// 同名ファイル衝突時の戦略
    pub conflict_strategy: ConflictStrategy,
    
    /// 削除ファイルのファイル名に識別子（例: "(deleted-#67)"）を埋め込むか
    pub mark_deleted_in_filename: bool,
    
    /// 生存/削除をサブディレクトリで分離するか（`live/` `deleted/`）
    pub separate_live_and_deleted: bool,
    
    /// 復旧した各ファイルの SHA256 をレポートに含めるか
    pub compute_sha256: bool,
    
    /// このサイズを超えるファイルはスキップ（None = 上限なし）
    pub max_file_size_bytes: Option<u64>,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Rename,
            mark_deleted_in_filename: true,
            separate_live_and_deleted: true,
            compute_sha256: true,
            max_file_size_bytes: None,
        }
    }
}

/// 同名ファイル衝突時の処理方針
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// 連番付与でリネーム (foo.txt → foo (1).txt)、デフォルト
    Rename,
    /// 既存ファイルを上書き（要注意）
    Overwrite,
    /// スキップしてレポートに記録
    Skip,
}
```

### 3. `report.rs`

```rust
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// 復旧結果の全体レポート。
#[derive(Debug)]
pub struct RecoveryReport {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    /// マッチ総数（復旧試行対象）
    pub total_matched: usize,
    /// 復旧成功したファイル
    pub recovered: Vec<RecoveredEntry>,
    /// 復旧失敗したファイル
    pub failed: Vec<FailedEntry>,
    /// スキップされたファイル
    pub skipped: Vec<SkippedEntry>,
}

impl RecoveryReport {
    /// 復旧成功率（パーセント）
    pub fn success_rate(&self) -> f64 {
        if self.total_matched == 0 { return 0.0 }
        (self.recovered.len() as f64) / (self.total_matched as f64) * 100.0
    }
    
    pub fn duration_ms(&self) -> i64 {
        (self.finished_at - self.started_at).num_milliseconds()
    }
    
    pub fn total_bytes_written(&self) -> u64 {
        self.recovered.iter().map(|e| e.bytes_written).sum()
    }
}

/// 復旧成功したファイル 1 件
#[derive(Debug, Clone)]
pub struct RecoveredEntry {
    pub source_id: String,
    /// NTFS フルパス (例: `\dir1\file.txt`)
    pub original_path: String,
    /// 実出力先パス (例: `output/live/dir1/file.txt`)
    pub output_path: PathBuf,
    pub bytes_written: u64,
    pub priority_score: u32,
    pub is_deleted: bool,
    /// SHA256 (RecoveryOptions::compute_sha256 が true の時)
    pub sha256: Option<String>,
}

/// 復旧失敗したファイル 1 件
#[derive(Debug, Clone)]
pub struct FailedEntry {
    pub source_id: String,
    pub original_path: String,
    pub error_message: String,
}

/// スキップされたファイル 1 件
#[derive(Debug, Clone)]
pub struct SkippedEntry {
    pub source_id: String,
    pub original_path: String,
    pub reason: String,
}
```

### 4. `sanitize.rs`

Windows OS で許可されない文字 / 予約名のサニタイズ:

```rust
use crate::error::RecoveryError;

/// Windows で禁止されている文字
const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows の予約名（大文字小文字非区別）
const RESERVED_BASE_NAMES: &[&str] = &["CON", "PRN", "AUX", "NUL"];

/// ファイル名 1 セグメントをサニタイズする。
///
/// 処理:
/// 1. 禁止文字を `_` に置換
/// 2. 制御文字 (0x00-0x1F) を `_` に置換
/// 3. 末尾の `.` / 空白を削除（Windows で問題になる）
/// 4. Windows 予約名（CON, PRN, AUX, NUL, COM1-9, LPT1-9）を `_` プレフィックスで回避
/// 5. 空文字列ならエラー返却
pub fn sanitize_filename(name: &str) -> Result<String, RecoveryError> {
    let mut sanitized: String = name.chars()
        .map(|c| {
            if c.is_control() || FORBIDDEN_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    
    // 末尾の `.` および空白を削除
    while sanitized.ends_with('.') || sanitized.ends_with(' ') {
        sanitized.pop();
    }
    
    // 予約名チェック（ベース部分のみ、拡張子は無視）
    let base = sanitized.split('.').next().unwrap_or("").to_uppercase();
    
    let is_reserved = RESERVED_BASE_NAMES.contains(&base.as_str())
        || (1..=9).any(|n| base == format!("COM{}", n) || base == format!("LPT{}", n));
    
    if is_reserved {
        sanitized = format!("_{}", sanitized);
    }
    
    if sanitized.is_empty() {
        return Err(RecoveryError::UnsanitizableFilename {
            original: name.to_string(),
        });
    }
    
    Ok(sanitized)
}

/// 削除ファイルの識別子をファイル名に挿入する。
/// 例: `foo.txt` + record 67 → `foo (deleted-#67).txt`
pub fn insert_deleted_marker(filename: &str, record_index: u64) -> String {
    if let Some((stem, ext)) = filename.rsplit_once('.') {
        format!("{} (deleted-#{}).{}", stem, record_index, ext)
    } else {
        format!("{} (deleted-#{})", filename, record_index)
    }
}
```

### 5. `engine.rs`

メインロジック。長くなるのでセクション分けてコメント:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use dds_fs_ntfs::{NtfsFile, NtfsVolume};
use dds_wish_match::{match_files, FileInfo, MatchResult, Wishlist};

use crate::error::RecoveryError;
use crate::options::{ConflictStrategy, RecoveryOptions};
use crate::report::{FailedEntry, RecoveredEntry, RecoveryReport, SkippedEntry};
use crate::sanitize::{insert_deleted_marker, sanitize_filename};

const MAX_RENAME_ATTEMPTS: u32 = 999;

/// 復旧エンジン本体。
///
/// 使い方:
/// ```ignore
/// let engine = RecoveryEngine::new("./output");
/// let report = engine.recover_files(&mut volume, &wishlist)?;
/// println!("Recovered {} files", report.recovered.len());
/// ```
pub struct RecoveryEngine {
    output_dir: PathBuf,
    options: RecoveryOptions,
}

impl RecoveryEngine {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self::with_options(output_dir, RecoveryOptions::default())
    }
    
    pub fn with_options(output_dir: impl Into<PathBuf>, options: RecoveryOptions) -> Self {
        Self {
            output_dir: output_dir.into(),
            options,
        }
    }
    
    /// マッチしたファイルを実際にディスクに復旧する。
    ///
    /// 戻り値の RecoveryReport に成功/失敗/スキップが per-file で記録される。
    /// 個別ファイルの失敗で全体は止まらない。
    pub fn recover_files<F>(
        &self,
        volume: &mut NtfsVolume<F>,
        wishlist: &Wishlist,
    ) -> Result<RecoveryReport, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();
        
        // Step 1: 出力ディレクトリの準備と検証
        self.prepare_output_dir()?;
        
        // Step 2: 全ファイル列挙 + FileInfo 変換
        let ntfs_files: Vec<NtfsFile> = volume
            .iter_files()
            .filter_map(Result::ok)
            .filter(|f| f.is_user_file())
            .collect();
        
        let file_infos: Vec<FileInfo> = ntfs_files.iter().map(FileInfo::from).collect();
        
        // Step 3: マッチング（wish-match の責務）
        let matches = match_files(&file_infos, wishlist);
        
        let total_matched = matches.len();
        let mut recovered = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();
        
        // Step 4: 各マッチを 1 件ずつ復旧
        for match_result in &matches {
            // source_id "NTFS#XX" から該当 NtfsFile を逆引き
            let Some(ntfs_file) = find_ntfs_file_by_source_id(&ntfs_files, &match_result.source_id)
            else {
                failed.push(FailedEntry {
                    source_id: match_result.source_id.clone(),
                    original_path: String::new(),
                    error_message: "NtfsFile not found for source_id".into(),
                });
                continue;
            };
            
            match self.recover_one(volume, ntfs_file, match_result) {
                Ok(SingleOutcome::Recovered(entry)) => recovered.push(entry),
                Ok(SingleOutcome::Skipped(reason)) => skipped.push(SkippedEntry {
                    source_id: match_result.source_id.clone(),
                    original_path: ntfs_file.path.clone(),
                    reason,
                }),
                Err(e) => failed.push(FailedEntry {
                    source_id: match_result.source_id.clone(),
                    original_path: ntfs_file.path.clone(),
                    error_message: e.to_string(),
                }),
            }
        }
        
        Ok(RecoveryReport {
            started_at,
            finished_at: Utc::now(),
            total_matched,
            recovered,
            failed,
            skipped,
        })
    }
    
    fn prepare_output_dir(&self) -> Result<(), RecoveryError> {
        fs::create_dir_all(&self.output_dir)?;
        
        // 書き込み可能か確認（簡易チェック）
        let canonical = self.output_dir.canonicalize().map_err(|e| {
            RecoveryError::InvalidOutputDir {
                path: self.output_dir.clone(),
                reason: format!("canonicalize failed: {}", e),
            }
        })?;
        
        if !canonical.is_dir() {
            return Err(RecoveryError::InvalidOutputDir {
                path: canonical,
                reason: "not a directory".into(),
            });
        }
        
        Ok(())
    }
    
    fn recover_one<F>(
        &self,
        volume: &mut NtfsVolume<F>,
        ntfs_file: &NtfsFile,
        match_result: &MatchResult,
    ) -> Result<SingleOutcome, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        // サイズ制限チェック
        if let Some(max) = self.options.max_file_size_bytes {
            if ntfs_file.size > max {
                return Ok(SingleOutcome::Skipped(format!(
                    "size {} exceeds limit {}",
                    ntfs_file.size, max
                )));
            }
        }
        
        // 出力パスを構築
        let target_path = self.build_output_path(ntfs_file)?;
        
        // 衝突処理
        let final_path = match self.options.conflict_strategy {
            ConflictStrategy::Rename => self.find_unique_path(&target_path)?,
            ConflictStrategy::Overwrite => target_path.clone(),
            ConflictStrategy::Skip => {
                if target_path.exists() {
                    return Ok(SingleOutcome::Skipped(format!(
                        "path exists: {:?}",
                        target_path
                    )));
                }
                target_path.clone()
            }
        };
        
        // 内容を読み出し
        let content = volume.read_file_content(ntfs_file)?;
        
        // 親ディレクトリ作成
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // 書き込み
        fs::write(&final_path, &content)?;
        
        let sha256 = if self.options.compute_sha256 {
            Some(sha256_hex(&content))
        } else {
            None
        };
        
        Ok(SingleOutcome::Recovered(RecoveredEntry {
            source_id: match_result.source_id.clone(),
            original_path: ntfs_file.path.clone(),
            output_path: final_path,
            bytes_written: content.len() as u64,
            priority_score: match_result.priority_score,
            is_deleted: ntfs_file.is_deleted,
            sha256,
        }))
    }
    
    /// NTFS パス → OS ファイルシステムパスに変換 + サニタイズ + 安全性検証
    fn build_output_path(&self, ntfs_file: &NtfsFile) -> Result<PathBuf, RecoveryError> {
        let mut path = self.output_dir.clone();
        
        // 生存/削除でサブディレクトリ分離
        if self.options.separate_live_and_deleted {
            path.push(if ntfs_file.is_deleted { "deleted" } else { "live" });
        }
        
        // NTFS パスを `\` で分解
        let segments: Vec<&str> = ntfs_file
            .path
            .split('\\')
            .filter(|s| !s.is_empty())
            .collect();
        
        // 親ディレクトリ部分を 1 つずつサニタイズ
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            if *segment == ".." || segment.contains("..") {
                return Err(RecoveryError::PathTraversal {
                    path: ntfs_file.path.clone(),
                });
            }
            path.push(sanitize_filename(segment)?);
        }
        
        // ファイル名（最終セグメント）の処理
        let filename_raw = segments.last().copied().unwrap_or("unnamed");
        let sanitized = sanitize_filename(filename_raw)?;
        
        let final_name = if ntfs_file.is_deleted && self.options.mark_deleted_in_filename {
            insert_deleted_marker(&sanitized, ntfs_file.record_index)
        } else {
            sanitized
        };
        
        path.push(final_name);
        Ok(path)
    }
    
    /// 衝突時にユニークな名前を探す: foo.txt → foo (1).txt → foo (2).txt ...
    fn find_unique_path(&self, desired: &Path) -> Result<PathBuf, RecoveryError> {
        if !desired.exists() {
            return Ok(desired.to_path_buf());
        }
        
        let parent = desired.parent().unwrap_or(Path::new("."));
        let stem = desired
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = desired.extension().and_then(|e| e.to_str());
        
        for n in 1..=MAX_RENAME_ATTEMPTS {
            let new_name = match ext {
                Some(e) => format!("{} ({}).{}", stem, n, e),
                None => format!("{} ({})", stem, n),
            };
            let candidate = parent.join(new_name);
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        
        Err(RecoveryError::UniqueFilenameExhausted {
            attempts: MAX_RENAME_ATTEMPTS,
        })
    }
}

enum SingleOutcome {
    Recovered(RecoveredEntry),
    Skipped(String),
}

fn find_ntfs_file_by_source_id<'a>(
    files: &'a [NtfsFile],
    source_id: &str,
) -> Option<&'a NtfsFile> {
    files
        .iter()
        .find(|f| format!("NTFS#{}", f.record_index) == source_id)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
```

### 6. `lib.rs`

```rust
//! DDS 復旧パイプライン。
//!
//! Wishlist マッチ結果を実ファイルとして出力ディレクトリに書き出す。
//!
//! # 使い方
//!
//! ```no_run
//! use dds_recovery::{RecoveryEngine, RecoveryOptions};
//! use dds_wish_match::{Wishlist, Wish, WishItem, Priority};
//! # use dds_fs_ntfs::NtfsVolume;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let mut volume: NtfsVolume<Box<dyn FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>> = unimplemented!();
//!
//! let wishlist = Wishlist::new()
//!     .add(Wish::new(WishItem::Extension("docx".into()), "全ての Word 文書")
//!         .with_priority(Priority::High));
//!
//! let engine = RecoveryEngine::new("./recovered_files");
//! let report = engine.recover_files(&mut volume, &wishlist)?;
//!
//! println!("Recovered: {} files", report.recovered.len());
//! println!("Failed:    {} files", report.failed.len());
//! # Ok(())
//! # }
//! ```

pub mod engine;
pub mod error;
pub mod options;
pub mod report;
pub mod sanitize;

pub use engine::RecoveryEngine;
pub use error::RecoveryError;
pub use options::{ConflictStrategy, RecoveryOptions};
pub use report::{FailedEntry, RecoveredEntry, RecoveryReport, SkippedEntry};
```

## 単体テスト要件（最低 12 件）

### `sanitize.rs` 単体テスト（最低 6 件）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn sanitize_replaces_forbidden_chars() {
        assert_eq!(sanitize_filename("foo<>bar.txt").unwrap(), "foo__bar.txt");
        assert_eq!(sanitize_filename("a|b?c.txt").unwrap(), "a_b_c.txt");
    }
    
    #[test]
    fn sanitize_strips_trailing_dots_and_spaces() {
        assert_eq!(sanitize_filename("foo.txt . ").unwrap(), "foo.txt");
    }
    
    #[test]
    fn sanitize_prefixes_reserved_names() {
        assert_eq!(sanitize_filename("CON").unwrap(), "_CON");
        assert_eq!(sanitize_filename("con.txt").unwrap(), "_con.txt");
        assert_eq!(sanitize_filename("COM1").unwrap(), "_COM1");
        assert_eq!(sanitize_filename("LPT9.dat").unwrap(), "_LPT9.dat");
    }
    
    #[test]
    fn sanitize_preserves_normal_names() {
        assert_eq!(sanitize_filename("report.docx").unwrap(), "report.docx");
        assert_eq!(sanitize_filename("日本語ファイル.pdf").unwrap(), "日本語ファイル.pdf");
    }
    
    #[test]
    fn sanitize_empty_returns_error() {
        assert!(matches!(
            sanitize_filename(""),
            Err(RecoveryError::UnsanitizableFilename { .. })
        ));
    }
    
    #[test]
    fn insert_deleted_marker_with_and_without_extension() {
        assert_eq!(insert_deleted_marker("foo.txt", 67), "foo (deleted-#67).txt");
        assert_eq!(insert_deleted_marker("Makefile", 42), "Makefile (deleted-#42)");
    }
}
```

### `report.rs` 単体テスト（最低 2 件）

```rust
#[test]
fn success_rate_calculates_percentage() {
    let report = RecoveryReport {
        started_at: Utc::now(),
        finished_at: Utc::now(),
        total_matched: 10,
        recovered: vec![/* 7 件のダミー */; 7],
        failed: vec![/* 3 件 */; 3],
        skipped: vec![],
    };
    assert!((report.success_rate() - 70.0).abs() < 0.01);
}

#[test]
fn total_bytes_written_sums_all_recovered() {
    // 100 + 200 + 300 = 600
}
```

### `engine.rs` 単体テスト（最低 4 件、tempfile 使用）

```rust
#[test]
fn build_output_path_separates_live_and_deleted() {
    // is_deleted=true → output/deleted/...
    // is_deleted=false → output/live/...
}

#[test]
fn build_output_path_rejects_path_traversal() {
    // path に ".." を含む合成 NtfsFile で PathTraversal エラー
}

#[test]
fn find_unique_path_increments_until_available() {
    // tempdir 内で foo.txt 既存 → foo (1).txt
    // foo (1).txt も既存 → foo (2).txt
}

#[test]
fn prepare_output_dir_creates_missing_directory() {
    // 存在しないパスを渡しても create_dir_all で作成成功
}
```

## 結合テスト要件（最低 3 件）

`crates/recovery/tests/recovery_integration.rs` を作成:

### 1. **全削除 .txt の復旧（end-to-end）**

```rust
use dds_recovery::*;
use dds_wish_match::*;
use dds_fs_ntfs::*;
use tempfile::TempDir;

#[test]
fn recovers_all_5_deleted_txt_files() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".into()), "全 .txt ファイル")
            .with_priority(Priority::High));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    // マッチは 30 件（live 25 + deleted 5）
    assert_eq!(report.total_matched, 30);
    assert_eq!(report.recovered.len(), 30);
    assert_eq!(report.failed.len(), 0);
    
    // 削除ファイルが deleted/ サブディレクトリに書かれていることを確認
    let deleted_dir = temp_dir.path().join("deleted");
    assert!(deleted_dir.exists());
    let deleted_count = fs::read_dir(&deleted_dir).unwrap().count();
    assert_eq!(deleted_count, 5);
    
    // 生存ファイルが live/ サブディレクトリに
    let live_dir = temp_dir.path().join("live");
    assert!(live_dir.exists());
}
```

### 2. **ground truth との SHA256 整合性検証**

```rust
#[test]
fn recovered_files_match_ground_truth_sha256() {
    let img = decompress_fixture("ntfs_directories");
    let ground_truth = load_ground_truth("ntfs_directories");
    // ... volume open ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".into()), "全 .txt"));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    // 109 ファイル全部復旧（小規模なので全部 live、deleted は 0）
    assert!(report.recovered.len() >= 100);
    
    // RecoveredEntry の sha256 と ground truth を照合
    let expected: HashMap<String, String> = ground_truth["files"]
        .as_array().unwrap().iter()
        .map(|f| (
            f["path"].as_str().unwrap().to_string(),
            f["content_hash_sha256"].as_str().unwrap().to_string(),
        ))
        .collect();
    
    let mut matched = 0;
    for entry in &report.recovered {
        if let Some(expected_hash) = expected.get(&entry.original_path) {
            assert_eq!(entry.sha256.as_deref(), Some(expected_hash.as_str()));
            matched += 1;
        }
    }
    assert!(matched >= 100);
}
```

### 3. **プロダクトデモテスト（最終形）**

```rust
#[test]
fn product_demo_end_to_end_recovery() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // お客様の希望: 削除されたテキストファイルを最優先で復旧
    let wishlist = Wishlist::new()
        .add(Wish::new(
            WishItem::All(vec![
                WishItem::Extension("txt".into()),
                // 全 .txt（削除ファイルだけマッチさせる方法は WishItem に追加が必要だが
                //  Phase 1 では「削除を含む全マッチ」+ レポートで分離する形）
            ]),
            "テキスト全般"
        ).with_priority(Priority::High));
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path());
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Phase 1 End-to-End Demo ===\n");
    println!("Source:    ntfs_with_5_deletions_small.img.zst");
    println!("Output:    {:?}", temp_dir.path());
    println!("Wishlist:  {} 希望", wishlist.wishes.len());
    println!("");
    println!("Matched:   {}", report.total_matched);
    println!("Recovered: {} (success rate: {:.1}%)", 
        report.recovered.len(), report.success_rate());
    println!("Failed:    {}", report.failed.len());
    println!("Skipped:   {}", report.skipped.len());
    println!("Duration:  {} ms", report.duration_ms());
    println!("");
    
    let deleted_recovered: Vec<&RecoveredEntry> = report.recovered.iter()
        .filter(|e| e.is_deleted)
        .collect();
    
    println!("Deleted files recovered:");
    for entry in &deleted_recovered {
        println!("  ✓ {} -> {:?}", entry.original_path, entry.output_path);
        println!("      sha256: {}...", &entry.sha256.as_deref().unwrap_or("")[..16]);
    }
    
    println!("");
    println!("=== Summary ===");
    println!("Total recovered:    {} files ({} bytes)", 
        report.recovered.len(), report.total_bytes_written());
    println!("Deleted recovered:  {} files", deleted_recovered.len());
    
    assert_eq!(deleted_recovered.len(), 5, "Should recover all 5 deleted files");
    assert_eq!(report.failed.len(), 0, "No failures expected");
}
```

## Cargo.toml 設定（再掲）

`crates/recovery/Cargo.toml`:
```toml
[dependencies]
chrono.workspace = true
sha2.workspace = true
thiserror.workspace = true
dds-core.workspace = true
dds-wish-match.workspace = true
dds-fs-ntfs.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

ワークスペースルートの `Cargo.toml` の `[workspace.dependencies]` にも `tempfile` を追加することを推奨（他のクレートでも使うので）。

## 制約

- **行数目安**:
  - `error.rs`: 30 行
  - `options.rs`: 50 行
  - `report.rs`: 80 行
  - `sanitize.rs`: 70 行 + テスト 50 行
  - `engine.rs`: 200 行 + テスト 70 行
  - 合計: 約 550 行（複数ファイル分散で各 200 行以内）
- **単体テスト最低 12 件**
- **結合テスト最低 3 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **書き込み先は self.output_dir 配下のみ**（パストラバーサル絶対防止）
- **ソースディスクへの書き込み 0 件**（recovery クレートは出力先のみ書く）

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-recovery` が全パス（≥12 件）
- [ ] `cargo test --workspace` 全体で全パス（既存 + 新規結合 ≥3 件）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_end_to_end_recovery` が pass + 出力が見える
- [ ] `recovered_files_match_ground_truth_sha256` で 100+ ファイル一致
- [ ] `grep -r 'unsafe' crates/recovery/src/` で 0 件
- [ ] パストラバーサル防御テストが通る
- [ ] Windows 予約名サニタイズが動く

## 関連 FR 要件

- **FR-REC-01** (目標優先抽出) ← end-to-end 完成
- **FR-REC-02** (出力先指定) ← RecoveryEngine::new で対応
- **FR-REC-03** (衝突解決) ← ConflictStrategy 3 種で対応
- **FR-REC-04** (データ整合性) ← SHA256 オプションで実証可能

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1 復旧パイプライン基盤完成、M4「復旧+品質」40% 進捗**
4. 次のステップ候補:
   - **Chunk 18**: 品質判定基盤 (`validators` クレート、PDF/DOCX 等のマジックナンバー検証)
   - **Chunk 19**: 復旧結果レポート生成 (PDF/Excel/HTML)
   - または Tauri UI 着手

---

## 注意事項

### read/write 境界の厳格な維持

このチャンクで初めて `std::fs::write` 等の書き込み API を使うが、**書き込み先は `self.output_dir` 配下のみ**。ソースディスク（NtfsVolume）への書き込みは絶対禁止。

検証方法:
```bash
# recovery クレート内で書き込み API を使うのは想定内
grep -r 'fs::write\|fs::create_dir' crates/recovery/src/
# fs-ntfs / wish-match / core 等で書き込みがゼロのまま維持
grep -r 'fs::write\|fs::create_dir' crates/fs-ntfs/src/  # → 0 件のまま
grep -r 'fs::write\|fs::create_dir' crates/wish-match/src/  # → 0 件のまま
```

### パストラバーサル防御

`build_output_path` で NTFS パス内に `..` が含まれていないか厳格にチェック。`..` 自体だけでなく、`a..b` のような部分一致もエラー化（保守的に）。

実 NTFS では `..` を含むファイル名は作れないはずだが、破損データ・悪意あるイメージへの耐性を持つ防衛線。

### Windows 予約名

`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9` は Windows でファイル名として使えない。NTFS は許可しているので、フィクスチャ生成時に作ろうと思えば作れる（実際にお客様 HDD で発生し得る）。サニタイズで `_CON` 等にプレフィックスして回避。

### 削除ファイルマーカーの意義

`foo.txt (deleted-#67).txt` 形式は、CS がファイルマネージャで見たときに「これは削除復旧されたもの」と一目でわかるため。お客様への成果物として整理しやすい。Phase 2 で UI 上でラベル表示する場合は不要になる可能性もあるが、Phase 1 はファイル名に埋め込む方式。

### 日本語ファイル名の扱い

Windows (NTFS), Linux (ext4), macOS (APFS) いずれも Unicode ファイル名対応。Rust の `std::fs::write` も Unicode パス対応。日本語ファイル名はそのまま出力可能（特別な処理不要）。

ただし、出力先がリモートマウントのファイルシステム（SMB, NFS 等）の場合、相手の制限に依存。Phase 1 はローカル出力前提でOK。

### サイズ制限

`max_file_size_bytes` は実 HDD 復旧時のメモリ枯渇防止。デフォルト None（無制限）だが、業務側で 100MB / 1GB 等の制限を設定する想定。

Phase 1 では「全ファイルを Vec<u8> に読む」設計なので、メモリに乗り切らないサイズはスキップする方が安全。

### Phase 1 で意図的に除外した機能

- **並列復旧 (Rayon, async)**: Phase 2 で
- **再開機能 (チェックポイント)**: Phase 2 で
- **進捗コールバック (UI 連携)**: Phase 2 (Tauri UI 着手時)
- **大規模ファイルのストリーミング書き込み**: Phase 2 で
- **削除のみ復旧 (削除エントリだけのフィルタ)**: WishItem に variant 追加で対応可

これらは指示書では実装しない。`RecoveryEngine` の API 設計が将来拡張に開かれていることを意識。

---

## 質問が必要なケース

- 出力ディレクトリが既存ファイルを多数含む場合の安全策（事前検証要否）
- Windows 短いパス（8.3）と長いパス（>260 文字）の境界
- ハードリンク（同じ NTFS エントリへの複数パス）の Phase 1 扱い

---

## 完了報告例

```markdown
## Chunk 17 完了報告

### 新規ファイル
- `crates/recovery/src/lib.rs` (35 行)
- `crates/recovery/src/error.rs` (35 行)
- `crates/recovery/src/options.rs` (55 行 + テスト 20 行)
- `crates/recovery/src/report.rs` (85 行 + テスト 30 行)
- `crates/recovery/src/sanitize.rs` (75 行 + テスト 50 行)
- `crates/recovery/src/engine.rs` (210 行 + テスト 70 行)
- `crates/recovery/Cargo.toml`
- `crates/recovery/tests/recovery_integration.rs` (180 行)

### 公開API
- `RecoveryEngine` (new / with_options / recover_files)
- `RecoveryOptions`, `ConflictStrategy` (Rename / Overwrite / Skip)
- `RecoveryReport`, `RecoveredEntry`, `FailedEntry`, `SkippedEntry`
- `RecoveryError`
- `sanitize_filename`, `insert_deleted_marker`

### テスト統計
- 単体: 既存 240 + 新規 15 = **255 件 pass**
- 結合: 既存 39 + 新規 3 = **42 件 pass**
- 全 workspace: **297+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- recovery クレート以外の書き込み API: 0 件（grep で確認）
- パストラバーサル防御テスト pass

### 業務価値の見える化 (`product_demo_end_to_end_recovery`)
```
=== DDS Recovery Workbench - Phase 1 End-to-End Demo ===

Source:    ntfs_with_5_deletions_small.img.zst
Output:    /tmp/dds-recovery-XXX/
Wishlist:  1 希望

Matched:   30
Recovered: 30 (success rate: 100.0%)
Failed:    0
Skipped:   0
Duration:  XX ms

Deleted files recovered:
  ✓ \file_003.txt -> /tmp/.../deleted/file_003 (deleted-#67).txt
      sha256: abc123def4567890...
  ✓ \file_007.txt -> /tmp/.../deleted/file_007 (deleted-#71).txt
      sha256: 9876543210abcdef...
  ✓ \file_015.txt -> ...
  ✓ \file_022.txt -> ...
  ✓ \file_028.txt -> ...

=== Summary ===
Total recovered:    30 files (1500 bytes)
Deleted recovered:  5 files
```

### 🎉 マイルストーン
- **Phase 1 復旧パイプライン基盤完成**
- end-to-end で「希望リスト → 実ファイル復旧」が動作
- M4「復旧 + 品質」40% 進捗

- **関連 FR**: FR-REC-01〜04 (基盤完成)

→ tester エージェントへ引き継ぎお願いします
```
