//! Chunk 17: 復旧パイプラインのコアエンジン。
//!
//! `RecoveryEngine::recover_files` 1 本で「全 NTFS ファイル列挙 → wish-match →
//! 1 件ずつ復旧 → レポート集約」までを実行する。書き込み先は `output_dir` 配下に
//! 厳格に閉じ、ソースディスクへの書き込みは行わない。
//!
//! 関連 FR: FR-REC-01〜04。

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

/// 衝突時リネームの試行上限。これを超えたら `UniqueFilenameExhausted`。
const MAX_RENAME_ATTEMPTS: u32 = 999;

/// 復旧パイプラインのメインエントリ。
///
/// `output_dir` 配下にのみ書き込みを行う。ソースディスクへの書き込みは絶対に
/// しない設計。`recover_files` は個別ファイルの失敗で全体を止めず、レポートに
/// `failed` / `skipped` として記録して継続する（業務的に「1 件壊れても他は救う」）。
pub struct RecoveryEngine {
    output_dir: PathBuf,
    options: RecoveryOptions,
}

impl RecoveryEngine {
    /// デフォルトオプションで新規エンジンを生成する。
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self::with_options(output_dir, RecoveryOptions::default())
    }

    /// カスタムオプションで新規エンジンを生成する。
    pub fn with_options(output_dir: impl Into<PathBuf>, options: RecoveryOptions) -> Self {
        Self {
            output_dir: output_dir.into(),
            options,
        }
    }

    /// 設定されている出力ディレクトリを取得する。
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// マッチしたファイルを実際にディスクに復旧する。
    ///
    /// 個別ファイルの失敗で全体は止まらず、`RecoveryReport` の
    /// `recovered` / `failed` / `skipped` のいずれかに per-file で記録される。
    pub fn recover_files<F>(
        &self,
        volume: &mut NtfsVolume<F>,
        wishlist: &Wishlist,
    ) -> Result<RecoveryReport, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();

        // Step 1: 出力ディレクトリの準備（無ければ作成、書き込めるか検証）。
        self.prepare_output_dir()?;

        // Step 2: 全ユーザファイル列挙 + FileInfo 変換。$ プレフィックスは除外し
        //         「お客様の希望」と純粋にマッチするものに絞る（業務的方針）。
        let ntfs_files: Vec<NtfsFile> = volume
            .iter_files()
            .filter_map(Result::ok)
            .filter(|f| f.is_user_file() && !f.has_system_name_prefix())
            .collect();
        let file_infos: Vec<FileInfo> = ntfs_files.iter().map(FileInfo::from).collect();

        // Step 3: マッチング（wish-match の責務、優先度降順ソート済み）。
        let matches = match_files(&file_infos, wishlist);

        let total_matched = matches.len();
        let mut recovered = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();

        // Step 4: 1 件ずつ復旧。失敗しても全体は止めない。
        for m in &matches {
            let Some(ntfs_file) = find_ntfs_file_by_source_id(&ntfs_files, &m.source_id) else {
                failed.push(FailedEntry {
                    source_id: m.source_id.clone(),
                    original_path: String::new(),
                    error_message: "NtfsFile not found for source_id".into(),
                });
                continue;
            };
            match self.recover_one(volume, ntfs_file, m) {
                Ok(SingleOutcome::Recovered(entry)) => recovered.push(*entry),
                Ok(SingleOutcome::Skipped(reason)) => skipped.push(SkippedEntry {
                    source_id: m.source_id.clone(),
                    original_path: ntfs_file.path.clone(),
                    reason,
                }),
                Err(e) => failed.push(FailedEntry {
                    source_id: m.source_id.clone(),
                    original_path: ntfs_file.path.clone(),
                    error_message: e.to_string(),
                }),
            }
        }

        // Chunk 20.5: 顧客指定の Wish::label を保持。レポートで「ご指定条件」表示に使う。
        let wish_labels: Vec<String> =
            wishlist.wishes.iter().map(|w| w.label.clone()).collect();

        Ok(RecoveryReport {
            started_at,
            finished_at: Utc::now(),
            total_matched,
            recovered,
            failed,
            skipped,
            wish_labels,
        })
    }

    /// 出力ディレクトリを作成し、ディレクトリとして利用可能か検証する。
    fn prepare_output_dir(&self) -> Result<(), RecoveryError> {
        fs::create_dir_all(&self.output_dir)?;
        let canonical = self
            .output_dir
            .canonicalize()
            .map_err(|e| RecoveryError::InvalidOutputDir {
                path: self.output_dir.clone(),
                reason: format!("canonicalize failed: {}", e),
            })?;
        if !canonical.is_dir() {
            return Err(RecoveryError::InvalidOutputDir {
                path: canonical,
                reason: "not a directory".into(),
            });
        }
        Ok(())
    }

    /// 1 つのマッチ結果を実ファイルとして書き出す。サイズ超過なら `Skipped`。
    fn recover_one<F>(
        &self,
        volume: &mut NtfsVolume<F>,
        ntfs_file: &NtfsFile,
        m: &MatchResult<'_>,
    ) -> Result<SingleOutcome, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        // サイズ上限チェック（Phase 1 は全体メモリ展開なので必須安全弁）。
        if let Some(max) = self.options.max_file_size_bytes {
            if ntfs_file.size > max {
                return Ok(SingleOutcome::Skipped(format!(
                    "size {} exceeds limit {}",
                    ntfs_file.size, max
                )));
            }
        }

        // 出力パスをサニタイズ込みで構築 + パストラバーサル検査。
        let target_path = self.build_output_path(ntfs_file)?;

        // 衝突戦略に応じて最終パスを決定。
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

        // 原本 NTFS から内容を読み出し（read-only）、出力先へ書き込み。
        let content = volume.read_file_content(ntfs_file)?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&final_path, &content)?;

        let sha256 = if self.options.compute_sha256 {
            Some(sha256_hex(&content))
        } else {
            None
        };

        // Chunk 18: 復旧後の品質判定。validate_after_recovery が true なら、
        // dds-validators の registry で拡張子に応じた検証を実行する。
        let validation = if self.options.validate_after_recovery {
            let registry = dds_validators::ValidatorRegistry::with_defaults();
            Some(registry.validate(&content, ntfs_file.extension().as_deref()))
        } else {
            None
        };

        // Chunk 20.5: マッチした各 Wish のラベルを集約。CSV / レポート用。
        let matched_wish_labels: Vec<String> =
            m.matched_wishes.iter().map(|w| w.label.clone()).collect();

        Ok(SingleOutcome::Recovered(Box::new(RecoveredEntry {
            source_id: m.source_id.clone(),
            original_path: ntfs_file.path.clone(),
            output_path: final_path,
            bytes_written: content.len() as u64,
            priority_score: m.priority_score,
            is_deleted: ntfs_file.is_deleted,
            sha256,
            validation,
            matched_wish_labels,
        })))
    }

    /// NTFS パス → OS ファイルシステムパスに変換 + サニタイズ + 安全性検証。
    ///
    /// パストラバーサル防御: 各パスセグメントが `..` を含んでいないか厳格に
    /// チェック。`..` 自体だけでなく `a..b` のような部分一致もエラー化（保守的）。
    pub fn build_output_path(&self, ntfs_file: &NtfsFile) -> Result<PathBuf, RecoveryError> {
        let mut path = self.output_dir.clone();

        if self.options.separate_live_and_deleted {
            path.push(if ntfs_file.is_deleted { "deleted" } else { "live" });
        }

        // NTFS パスは `\` 区切り。空セグメントは除外（先頭 `\` 由来等）。
        let segments: Vec<&str> = ntfs_file
            .path
            .split('\\')
            .filter(|s| !s.is_empty())
            .collect();

        if segments.is_empty() {
            return Err(RecoveryError::UnsanitizableFilename {
                original: ntfs_file.path.clone(),
            });
        }

        // 全セグメントでパストラバーサル検査（親も最終ファイル名も対象）。
        for segment in &segments {
            if segment.contains("..") {
                return Err(RecoveryError::PathTraversal {
                    path: ntfs_file.path.clone(),
                });
            }
        }

        // 親ディレクトリ部分（最後を除く）をサニタイズして push。
        let last_idx = segments.len() - 1;
        for seg in &segments[..last_idx] {
            path.push(sanitize_filename(seg)?);
        }

        // 最終セグメント（ファイル名）をサニタイズ。削除なら deleted-marker を挿入。
        let raw_name = segments[last_idx];
        let sanitized = sanitize_filename(raw_name)?;
        let final_name = if ntfs_file.is_deleted && self.options.mark_deleted_in_filename {
            insert_deleted_marker(&sanitized, ntfs_file.record_index)
        } else {
            sanitized
        };
        path.push(final_name);

        Ok(path)
    }

    /// 衝突時にユニークな名前を探す: `foo.txt` → `foo (1).txt` → `foo (2).txt` ...
    pub fn find_unique_path(&self, desired: &Path) -> Result<PathBuf, RecoveryError> {
        if !desired.exists() {
            return Ok(desired.to_path_buf());
        }
        let parent = desired.parent().unwrap_or_else(|| Path::new("."));
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

/// `recover_one` の戻り値内部型（成功と Skip を区別、失敗は `Err` で表現）。
///
/// `RecoveredEntry` は `ValidationResult` を抱えるためサイズが大きい。
/// バリアント間のサイズ差を抑えるため `Box` でヒープに退避する。
enum SingleOutcome {
    Recovered(Box<RecoveredEntry>),
    Skipped(String),
}

/// `MatchResult::source_id` から対応する `NtfsFile` を逆引きする。
fn find_ntfs_file_by_source_id<'a>(
    files: &'a [NtfsFile],
    source_id: &str,
) -> Option<&'a NtfsFile> {
    files
        .iter()
        .find(|f| format!("NTFS#{}", f.record_index) == source_id)
}

/// SHA256 を 16 進文字列で計算（小文字）。`RecoveredEntry::sha256` 用ヘルパ。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds_fs_ntfs::FileContentRef;
    use dds_fs_ntfs::{FileAttributes, MftReference};

    fn make_file(record_index: u64, path: &str, is_deleted: bool) -> NtfsFile {
        let name = path
            .rsplit_once('\\')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| path.to_string());
        NtfsFile {
            record_index,
            path: path.to_string(),
            name,
            parent: MftReference {
                entry_number: 5,
                sequence_number: 1,
            },
            is_directory: false,
            is_deleted,
            created: None,
            modified: None,
            accessed: None,
            mft_modified: None,
            file_attributes: FileAttributes(0),
            has_alternate_streams: false,
            is_compressed: false,
            is_encrypted: false,
            is_sparse: false,
            content: FileContentRef::None,
            size: 0,
        }
    }

    #[test]
    fn build_output_path_separates_live_and_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let engine = RecoveryEngine::new(temp.path());

        let live = make_file(100, "\\dir1\\report.docx", false);
        let live_path = engine.build_output_path(&live).unwrap();
        assert!(live_path.starts_with(temp.path().join("live")));
        assert!(live_path.ends_with("report.docx"));

        let del = make_file(67, "\\dir1\\file_003.txt", true);
        let del_path = engine.build_output_path(&del).unwrap();
        assert!(del_path.starts_with(temp.path().join("deleted")));
        // deleted-marker 込みのファイル名であること。
        let final_name = del_path.file_name().unwrap().to_str().unwrap();
        assert_eq!(final_name, "file_003 (deleted-#67).txt");
    }

    #[test]
    fn build_output_path_rejects_path_traversal() {
        // 破損 / 悪意あるイメージ対策: `..` を含むパスは PathTraversal エラー。
        let temp = tempfile::tempdir().unwrap();
        let engine = RecoveryEngine::new(temp.path());
        let bad = make_file(100, "\\..\\..\\evil.txt", false);
        let result = engine.build_output_path(&bad);
        assert!(matches!(result, Err(RecoveryError::PathTraversal { .. })));

        // 部分一致 `a..b` も保守的にブロック。
        let partial = make_file(101, "\\dir1\\a..b\\file.txt", false);
        assert!(matches!(
            engine.build_output_path(&partial),
            Err(RecoveryError::PathTraversal { .. })
        ));
    }

    #[test]
    fn find_unique_path_increments_until_available() {
        let temp = tempfile::tempdir().unwrap();
        let engine = RecoveryEngine::new(temp.path());

        let p = temp.path().join("foo.txt");
        // 存在しないので desired そのまま。
        assert_eq!(engine.find_unique_path(&p).unwrap(), p);

        // 作って → foo (1).txt が返る。
        fs::write(&p, b"x").unwrap();
        let p1 = engine.find_unique_path(&p).unwrap();
        assert_eq!(p1.file_name().unwrap().to_str().unwrap(), "foo (1).txt");

        // foo (1).txt も作って → foo (2).txt。
        fs::write(&p1, b"x").unwrap();
        let p2 = engine.find_unique_path(&p).unwrap();
        assert_eq!(p2.file_name().unwrap().to_str().unwrap(), "foo (2).txt");
    }

    #[test]
    fn prepare_output_dir_creates_missing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        let engine = RecoveryEngine::new(&nested);
        engine.prepare_output_dir().unwrap();
        assert!(nested.is_dir());
    }

    #[test]
    fn build_output_path_sanitizes_reserved_names_in_segments() {
        // 業務シナリオ: NTFS 上に "CON" という名前のディレクトリ + "report.docx" があった場合、
        // Windows 出力先で開けるよう、ディレクトリ部もサニタイズされること。
        let temp = tempfile::tempdir().unwrap();
        let engine = RecoveryEngine::new(temp.path());
        let f = make_file(100, "\\CON\\report.docx", false);
        let p = engine.build_output_path(&f).unwrap();
        // パス中に `_CON` ディレクトリが現れる。
        assert!(p.to_string_lossy().contains("_CON"), "got: {:?}", p);
    }
}
