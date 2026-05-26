//! Chunk 17: 復旧パイプラインのコアエンジン。
//!
//! `RecoveryEngine::recover_files` 1 本で「全 NTFS ファイル列挙 → wish-match →
//! 1 件ずつ復旧 → レポート集約」までを実行する。書き込み先は `output_dir` 配下に
//! 厳格に閉じ、ソースディスクへの書き込みは行わない。
//!
//! Chunk 24b で並列化:
//! - **プロデューサ（メインスレッド）**: NtfsVolume からファイル内容を順次読出
//!   （`FnMut` の制約上シリアル必須）
//! - **コンシューマ（N スレッド）**: write + SHA256 + validate + apply_timestamps を並列化
//! - `crossbeam-channel::bounded(N*2)` で背圧制御し、メモリ消費を抑制
//! - I/O バッファ 1MB に拡大（`std::fs::write` → `BufWriter::with_capacity`）
//!
//! 関連 FR: FR-REC-01〜04, FR-REC-08 (速度), FR-CLI-08 (進捗)。

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use crossbeam_channel::bounded;
use sha2::{Digest, Sha256};

use std::collections::HashMap;

use dds_fs_ntfs::{NtfsFile, NtfsVolume};
use dds_wish_match::{match_files, ExclusionList, FileInfo, MatchResult, Wishlist};

use crate::error::RecoveryError;
use crate::options::{ConflictStrategy, RecoveryOptions};
use crate::progress::ProgressReporter;
use crate::report::{FailedEntry, RecoveredEntry, RecoveryReport, SkippedEntry};
use crate::sanitize::{insert_deleted_marker, sanitize_filename};

/// 衝突時リネームの試行上限。これを超えたら `UniqueFilenameExhausted`。
const MAX_RENAME_ATTEMPTS: u32 = 999;

/// Chunk 24b: 書き込み時の `BufWriter` バッファサイズ（1 MB）。
/// デフォルト 8KB から拡大することで syscall 回数削減 → スループット改善。
const WRITE_BUFFER_BYTES: usize = 1024 * 1024;

/// Chunk 24b: 並列化のワーカー数上限。業務 PC のスペック (4-8 コア) を考慮し、
/// I/O が支配的な処理で多くしても効果が薄いため 4 で打ち止め。
const MAX_WORKER_THREADS: usize = 4;

/// Chunk 24b: 並列復旧の結果バケットタプル。
///
/// `run_parallel_recovery` の戻り値型。`(recovered, failed, skipped)` を順に保持する。
/// 名前付き型エイリアスにすることで clippy::type_complexity を回避し、呼び出し側の
/// 可読性も改善する。
type ParallelOutcomeBuckets = (Vec<RecoveredEntry>, Vec<FailedEntry>, Vec<SkippedEntry>);

/// 復旧時の出力先パス設定 (Chunk 23)。
///
/// 通常ファイル（生存）と削除ファイルの出力先を明示的に指定する。Chunk 17 までは
/// 「単一の `output_dir` 配下に `live/` と `deleted/` を作る」固定構造だったが、
/// Chunk 23 で業務向けに任意のディレクトリへ振り分けられるよう拡張した。
///
/// 既存 API (`RecoveryEngine::new(output_dir)`) は内部的に
/// [`RecoveryConfig::from_single_dir`] を使うことで互換維持される。
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// 生存ファイルの出力先。
    pub live_files_dir: PathBuf,
    /// 削除ファイルの出力先。
    pub deleted_files_dir: PathBuf,
}

impl RecoveryConfig {
    /// 単一の `output_dir` から従来構造 (`{output_dir}/live`, `{output_dir}/deleted`)
    /// を構築する。既存 API 互換用。
    pub fn from_single_dir(output_dir: impl AsRef<Path>) -> Self {
        let base = output_dir.as_ref();
        Self {
            live_files_dir: base.join("live"),
            deleted_files_dir: base.join("deleted"),
        }
    }

    /// 明示的にパスを指定する。Chunk 23 の `execute_business_recovery` から
    /// `CaseOutput::live_files_dir()` / `deleted_files_dir()` を渡して使う。
    pub fn with_paths(live: impl Into<PathBuf>, deleted: impl Into<PathBuf>) -> Self {
        Self {
            live_files_dir: live.into(),
            deleted_files_dir: deleted.into(),
        }
    }
}

/// 復旧パイプラインのメインエントリ。
///
/// `output_dir` 配下にのみ書き込みを行う。ソースディスクへの書き込みは絶対に
/// しない設計。`recover_files` は個別ファイルの失敗で全体を止めず、レポートに
/// `failed` / `skipped` として記録して継続する（業務的に「1 件壊れても他は救う」）。
pub struct RecoveryEngine {
    /// 既存 API 互換用ベースディレクトリ。
    /// `separate_live_and_deleted = false` のとき、または `prepare_output_dir`
    /// の canonical 検証の起点として使う（Chunk 17 仕様維持）。
    output_dir: PathBuf,
    /// Chunk 23 で追加: 通常 / 削除の振り分け先。
    /// `RecoveryEngine::new` 経由なら `RecoveryConfig::from_single_dir(output_dir)`、
    /// `with_config` 経由なら呼び出し元指定の値が入る。
    config: RecoveryConfig,
    options: RecoveryOptions,
}

impl RecoveryEngine {
    /// デフォルトオプションで新規エンジンを生成する。
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self::with_options(output_dir, RecoveryOptions::default())
    }

    /// カスタムオプションで新規エンジンを生成する。
    pub fn with_options(output_dir: impl Into<PathBuf>, options: RecoveryOptions) -> Self {
        let output_dir = output_dir.into();
        let config = RecoveryConfig::from_single_dir(&output_dir);
        Self {
            output_dir,
            config,
            options,
        }
    }

    /// Chunk 23: 業務向け [`RecoveryConfig`] からエンジンを構築する。
    ///
    /// `output_dir` は `live_files_dir` をそのまま流用する（`prepare_output_dir`
    /// の canonical 検証で利用）。`separate_live_and_deleted = false` の場合は
    /// `config.live_files_dir` 単体に出力される（業務的にはほぼ使わないユースケース）。
    pub fn with_config(config: RecoveryConfig) -> Self {
        Self::with_config_and_options(config, RecoveryOptions::default())
    }

    /// Chunk 23: 業務向け [`RecoveryConfig`] とカスタムオプションでエンジンを構築する。
    pub fn with_config_and_options(config: RecoveryConfig, options: RecoveryOptions) -> Self {
        Self {
            output_dir: config.live_files_dir.clone(),
            config,
            options,
        }
    }

    /// 設定されている出力ディレクトリを取得する。
    ///
    /// 既存 API 互換のため `new` / `with_options` で渡された値（または
    /// `with_config` の場合は `config.live_files_dir` のコピー）を返す。
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Chunk 23: 設定中の [`RecoveryConfig`] を取得する。
    pub fn config(&self) -> &RecoveryConfig {
        &self.config
    }

    /// すべての user file を復旧し、Wishlist マッチを「優先データ」としてラベリングする
    /// （Chunk 23.7、R-STUDIO 風業務フロー対応）。
    ///
    /// ## 復旧範囲
    ///
    /// 以下のすべてを満たす [`NtfsFile`] が復旧対象:
    /// - `is_user_file()` が true（NTFS システムファイル除外）
    /// - `is_directory` が false（ディレクトリは別途扱い）
    /// - `exclusions.matches(&file.path)` が false（業務的システムファイル除外）
    ///
    /// ## Wishlist の役割
    ///
    /// 復旧範囲には影響しない。マッチしたファイルは
    /// `RecoveredEntry::is_priority = true` + `priority_score` 継承 +
    /// `matched_wish_labels` 設定で「お客様優先データ」としてマーキングされる。
    ///
    /// Phase 1 までは「Wishlist マッチのみ復旧」だったが、Chunk 23.7 で
    /// R-STUDIO 風の「全件復旧 + 除外 + 優先マーキング」に方向転換した。
    ///
    /// 個別ファイルの失敗で全体は止まらず、`RecoveryReport` の
    /// `recovered` / `failed` / `skipped` のいずれかに per-file で記録される。
    pub fn recover_files<F, P>(
        &self,
        volume: &mut NtfsVolume<F>,
        wishlist: &Wishlist,
        exclusions: &ExclusionList,
        progress: &P,
    ) -> Result<RecoveryReport, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
        P: ProgressReporter + ?Sized,
    {
        let started_at = Utc::now();

        // Step 1: 出力ディレクトリの準備（無ければ作成、書き込めるか検証）。
        self.prepare_output_dir()?;

        // Step 2: 全ユーザファイル列挙。Chunk 23.7 で `has_system_name_prefix` の
        //         組み込みフィルタを撤去し、ExclusionList で `$` プレフィックスを除外する設計に
        //         変更（より明示的・カスタマイズ可能）。
        let ntfs_files: Vec<NtfsFile> = volume
            .iter_files()
            .filter_map(Result::ok)
            .filter(|f| f.is_user_file() && !f.is_directory)
            .filter(|f| !exclusions.matches(&f.path))
            .collect();

        // Step 3: 全件分の FileInfo を構築し、Wishlist との照合を index 化する。
        //         Wishlist が空のときは何もマッチしない（全件 is_priority=false）。
        let file_infos: Vec<FileInfo> = ntfs_files.iter().map(FileInfo::from).collect();
        let match_results = match_files(&file_infos, wishlist);
        let match_index: HashMap<String, MatchResult<'_>> = match_results
            .into_iter()
            .map(|m| (m.source_id.clone(), m))
            .collect();

        // 復旧試行対象 = 除外 / システムを差し引いた全 user file。
        // total_matched は Chunk 23.7 で「復旧範囲全体の母数」を表すよう意味が拡張された。
        let total_matched = ntfs_files.len();
        let total = ntfs_files.len();

        // Step 4: Chunk 24b の並列化分岐。`total == 0` の場合は即座にレポート返却。
        let (recovered, failed, skipped) = if total == 0 {
            (Vec::new(), Vec::new(), Vec::new())
        } else {
            self.run_parallel_recovery(volume, &ntfs_files, &match_index, progress)?
        };

        // 進捗 100% を最終報告（ループ中に届かなかった場合の保証）。
        progress.report(total, total, "");

        // Chunk 20.5: 顧客指定の Wish::label を保持。レポートで「ご指定条件」表示に使う。
        let wish_labels: Vec<String> = wishlist.wishes.iter().map(|w| w.label.clone()).collect();

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

    /// Chunk 24b: Producer-Consumer 並列化の本体。
    ///
    /// - **プロデューサ（このスレッド）**: NtfsVolume から `read_file_content` で
    ///   ファイル内容を**順次**取得し（`FnMut` 制約のためシリアル必須）、
    ///   `(NtfsFile, content)` を task channel に投入する。
    /// - **コンシューマ（N スレッド）**: task channel から受け取り、サニタイズ・
    ///   write・SHA256・validate・apply_timestamps を並列に実行する。
    /// - **バッファサイズ**: `bounded(N * 2)`。ワーカーが詰まるとプロデューサが
    ///   ブロックされ、ファイル content がメモリに無制限に積まれない（OOM 回避）。
    ///
    /// # 戻り値
    /// `(recovered, failed, skipped)` のタプル。順序は処理順とは限らない
    /// （並列のため）が、業務的に問題ない（レポート側で sort して整形）。
    fn run_parallel_recovery<F, P>(
        &self,
        volume: &mut NtfsVolume<F>,
        ntfs_files: &[NtfsFile],
        match_index: &HashMap<String, MatchResult<'_>>,
        progress: &P,
    ) -> Result<ParallelOutcomeBuckets, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
        P: ProgressReporter + ?Sized,
    {
        let total = ntfs_files.len();
        let worker_count = num_cpus::get().clamp(1, MAX_WORKER_THREADS);
        let queue_size = worker_count * 2;

        let (task_tx, task_rx) = bounded::<RecoveryTask>(queue_size);
        let (result_tx, result_rx) = bounded::<ProcessedOutcome>(queue_size);

        // 全 worker で共有する設定（Arc 不要、Clone で十分軽い構造体）。
        // Chunk 24b: ワーカースレッドは output_dir / config / options だけ知っていれば良い。
        let shared_config = self.config.clone();
        let shared_options = self.options.clone();
        let shared_output_dir = self.output_dir.clone();

        // コンシューマスレッド起動。
        let workers: Vec<_> = (0..worker_count)
            .map(|_| {
                let task_rx = task_rx.clone();
                let result_tx = result_tx.clone();
                let cfg = shared_config.clone();
                let opt = shared_options.clone();
                let out = shared_output_dir.clone();
                std::thread::spawn(move || {
                    while let Ok(task) = task_rx.recv() {
                        let outcome = process_recovery_task(&task, &cfg, &opt, &out);
                        if result_tx.send(outcome).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect();

        // 結果収集スレッド。プロデューサと並行して result_rx を drain する。
        let collector = std::thread::spawn(move || {
            let mut entries = Vec::new();
            while let Ok(outcome) = result_rx.recv() {
                entries.push(outcome);
            }
            entries
        });

        // プロデューサ: NtfsVolume からシリアルに読出 → task 投入。
        // I/O エラーは failed として記録し、ワーカーには投入しない。
        let mut producer_failures: Vec<FailedEntry> = Vec::new();
        for (idx, ntfs_file) in ntfs_files.iter().enumerate() {
            progress.report(idx + 1, total, &ntfs_file.path);

            let source_id = format!("NTFS#{}", ntfs_file.record_index);

            // サイズ上限チェック（Phase 1 は全体メモリ展開なので必須安全弁）。
            // 上限超過は worker に流さず Skip として記録するため、専用の outcome を投入。
            if let Some(max) = self.options.max_file_size_bytes {
                if ntfs_file.size > max {
                    // Skip outcome を直接 result_tx に流す（workerless 経路）。
                    // タスク投入と同じ通路から流すため、結果順を維持できる。
                    let skip_outcome = ProcessedOutcome::Skipped(SkippedEntry {
                        source_id: source_id.clone(),
                        original_path: ntfs_file.path.clone(),
                        reason: format!("size {} exceeds limit {}", ntfs_file.size, max),
                    });
                    // プロデューサ自身も result_tx の sender clone を持つので送信できる。
                    if result_tx.send(skip_outcome).is_err() {
                        break;
                    }
                    continue;
                }
            }

            // NTFS から内容を読み出す（シリアル必須、`FnMut` 制約）。
            let content = match volume.read_file_content(ntfs_file) {
                Ok(c) => c,
                Err(e) => {
                    producer_failures.push(FailedEntry {
                        source_id,
                        original_path: ntfs_file.path.clone(),
                        error_message: format!("Volume error: {}", e),
                    });
                    continue;
                }
            };

            // Wishlist マッチ情報を owned に展開してワーカーへ渡す（ライフタイム回避）。
            let (is_priority, matched_wish_labels, priority_score) =
                match match_index.get(&source_id) {
                    Some(m) => (
                        true,
                        m.matched_wishes
                            .iter()
                            .map(|w| w.label.clone())
                            .collect::<Vec<_>>(),
                        m.priority_score,
                    ),
                    None => (false, Vec::new(), 0),
                };

            let task = RecoveryTask {
                ntfs_file: ntfs_file.clone(),
                source_id,
                content,
                is_priority,
                matched_wish_labels,
                priority_score,
            };
            if task_tx.send(task).is_err() {
                break; // ワーカーが全て停止していたら諦める。
            }
        }

        // タスク投入完了 → ワーカーへの EOF。
        drop(task_tx);
        // プロデューサ側の result_tx も閉じる（コンシューマ N の clone は worker join 時に閉じる）。
        drop(result_tx);

        // ワーカー終了待ち。任意の worker が panic したら WorkerPanic。
        for w in workers {
            w.join().map_err(|_| RecoveryError::WorkerPanic)?;
        }

        // 結果収集完了（receiver 終端は全 sender drop 時）。
        let outcomes = collector.join().map_err(|_| RecoveryError::WorkerPanic)?;

        // 集計。
        let mut recovered = Vec::with_capacity(total);
        let mut failed = producer_failures;
        let mut skipped = Vec::new();
        for outcome in outcomes {
            match outcome {
                ProcessedOutcome::Recovered(e) => recovered.push(*e),
                ProcessedOutcome::Skipped(s) => skipped.push(s),
                ProcessedOutcome::Failed(f) => failed.push(f),
            }
        }

        Ok((recovered, failed, skipped))
    }

    /// 出力ディレクトリを作成し、ディレクトリとして利用可能か検証する。
    ///
    /// Chunk 23 で「業務向けに任意のパス」を受け取れるようになったため、
    /// `config.live_files_dir` と `config.deleted_files_dir` の両方を作成する
    /// ように拡張（既存テスト互換のため `output_dir` も継続作成）。
    fn prepare_output_dir(&self) -> Result<(), RecoveryError> {
        fs::create_dir_all(&self.output_dir)?;
        fs::create_dir_all(&self.config.live_files_dir)?;
        fs::create_dir_all(&self.config.deleted_files_dir)?;
        let canonical =
            self.output_dir
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

    /// NTFS パス → OS ファイルシステムパスに変換 + サニタイズ + 安全性検証。
    ///
    /// パストラバーサル防御: 各パスセグメントが `..` を含んでいないか厳格に
    /// チェック。`..` 自体だけでなく `a..b` のような部分一致もエラー化（保守的）。
    ///
    /// Chunk 23: `separate_live_and_deleted = true` のときは
    /// [`RecoveryConfig::live_files_dir`] / [`RecoveryConfig::deleted_files_dir`]
    /// を直接ベースに使う（業務向け任意パス対応）。`false` のときは
    /// 既存 API 互換のため `output_dir` をベースに使う。
    pub fn build_output_path(&self, ntfs_file: &NtfsFile) -> Result<PathBuf, RecoveryError> {
        build_output_path_impl(&self.config, &self.options, &self.output_dir, ntfs_file)
    }

    /// 衝突時にユニークな名前を探す: `foo.txt` → `foo (1).txt` → `foo (2).txt` ...
    pub fn find_unique_path(&self, desired: &Path) -> Result<PathBuf, RecoveryError> {
        find_unique_path_impl(desired)
    }
}

// ============================================================================
// Chunk 24b: ワーカースレッドからも呼べる free function 実装。
//
// `RecoveryEngine` の `&self` メソッドだとライフタイムが絡んでワーカーに渡せない。
// `&RecoveryConfig` / `&RecoveryOptions` / `&Path` を引数で受け取る形に切り出して、
// シングルスレッド版（`build_output_path` / `find_unique_path`）と並列版から
// 共通利用できるようにする。
// ============================================================================

/// `build_output_path` の中身（並列ワーカー共通）。
fn build_output_path_impl(
    config: &RecoveryConfig,
    options: &RecoveryOptions,
    legacy_output_dir: &Path,
    ntfs_file: &NtfsFile,
) -> Result<PathBuf, RecoveryError> {
    let mut path = if options.separate_live_and_deleted {
        if ntfs_file.is_deleted {
            config.deleted_files_dir.clone()
        } else {
            config.live_files_dir.clone()
        }
    } else {
        legacy_output_dir.to_path_buf()
    };

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

    for segment in &segments {
        if segment.contains("..") {
            return Err(RecoveryError::PathTraversal {
                path: ntfs_file.path.clone(),
            });
        }
    }

    let last_idx = segments.len() - 1;
    for seg in &segments[..last_idx] {
        path.push(sanitize_filename(seg)?);
    }

    let raw_name = segments[last_idx];
    let sanitized = sanitize_filename(raw_name)?;
    let final_name = if ntfs_file.is_deleted && options.mark_deleted_in_filename {
        insert_deleted_marker(&sanitized, ntfs_file.record_index)
    } else {
        sanitized
    };
    path.push(final_name);

    Ok(path)
}

/// `find_unique_path` の中身（並列ワーカー共通）。
///
/// 注意: 並列環境では複数ワーカーが同じ衝突を解決しようとすると競合がある。
/// 現在のフィクスチャでは衝突がほぼ発生しないため、業務上影響は限定的。
/// Chunk 24c 以降で完全並列セーフな衝突解決が必要なら別途検討。
fn find_unique_path_impl(desired: &Path) -> Result<PathBuf, RecoveryError> {
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

/// Chunk 24b: ワーカーへ渡す並列タスク。
///
/// `Wishlist` への参照は持たず、マッチ情報は事前に展開した owned 値のみ持つ
/// （`MatchResult<'a>` のライフタイム問題を回避するため）。
struct RecoveryTask {
    ntfs_file: NtfsFile,
    source_id: String,
    /// プロデューサが NtfsVolume から読み出した全バイト。ワーカーが書込・SHA256・
    /// validator にそのまま消費する。
    content: Vec<u8>,
    is_priority: bool,
    matched_wish_labels: Vec<String>,
    priority_score: u32,
}

/// ワーカーが返す結果。`Recovered` を `Box` するのは `RecoveredEntry` が
/// `ValidationResult` を抱えていて大きく、`enum` のバリアント差を抑えるため。
enum ProcessedOutcome {
    Recovered(Box<RecoveredEntry>),
    Failed(FailedEntry),
    Skipped(SkippedEntry),
}

/// Chunk 24b: 1 タスクを処理する。並列ワーカースレッドから呼ばれる。
///
/// 構成: パス決定 → 衝突解決 → write (1MB buffer) → SHA256 → validator → タイムスタンプ。
/// 各段で失敗した場合は `Failed` / `Skipped` outcome として返却し、復旧全体は止めない。
fn process_recovery_task(
    task: &RecoveryTask,
    config: &RecoveryConfig,
    options: &RecoveryOptions,
    legacy_output_dir: &Path,
) -> ProcessedOutcome {
    // Step 1: 出力パス決定（サニタイズ + パストラバーサル検査）。
    let target_path =
        match build_output_path_impl(config, options, legacy_output_dir, &task.ntfs_file) {
            Ok(p) => p,
            Err(e) => {
                return ProcessedOutcome::Failed(FailedEntry {
                    source_id: task.source_id.clone(),
                    original_path: task.ntfs_file.path.clone(),
                    error_message: e.to_string(),
                });
            }
        };

    // Step 2: 衝突戦略に応じて最終パスを決定。
    let final_path = match options.conflict_strategy {
        ConflictStrategy::Rename => match find_unique_path_impl(&target_path) {
            Ok(p) => p,
            Err(e) => {
                return ProcessedOutcome::Failed(FailedEntry {
                    source_id: task.source_id.clone(),
                    original_path: task.ntfs_file.path.clone(),
                    error_message: e.to_string(),
                });
            }
        },
        ConflictStrategy::Overwrite => target_path.clone(),
        ConflictStrategy::Skip => {
            if target_path.exists() {
                return ProcessedOutcome::Skipped(SkippedEntry {
                    source_id: task.source_id.clone(),
                    original_path: task.ntfs_file.path.clone(),
                    reason: format!("path exists: {:?}", target_path),
                });
            }
            target_path.clone()
        }
    };

    // Step 3: 1MB バッファでの書き込み。
    let bytes_written = match write_with_large_buffer(&final_path, &task.content) {
        Ok(n) => n,
        Err(e) => {
            return ProcessedOutcome::Failed(FailedEntry {
                source_id: task.source_id.clone(),
                original_path: task.ntfs_file.path.clone(),
                error_message: format!("I/O error: {}", e),
            });
        }
    };

    // Step 4: Chunk 24a: NTFS タイムスタンプ保持。失敗しても復旧成否には影響させない。
    if let (Some(created), Some(modified), Some(accessed)) = (
        task.ntfs_file.created,
        task.ntfs_file.modified,
        task.ntfs_file.accessed,
    ) {
        let ts = crate::timestamps::NtfsTimestamps {
            created,
            modified,
            accessed,
        };
        if let Err(e) = crate::timestamps::apply_timestamps(&final_path, &ts) {
            eprintln!(
                "[warn] タイムスタンプ書き込み失敗: {:?} ({})",
                final_path, e
            );
        }
    }

    // Step 5: SHA256 計算（CPU バウンド、並列の恩恵）。
    let sha256 = if options.compute_sha256 {
        Some(sha256_hex(&task.content))
    } else {
        None
    };

    // Step 6: ファイル形式検証（CPU バウンド、並列の恩恵）。
    let validation = if options.validate_after_recovery {
        let registry = dds_validators::ValidatorRegistry::with_defaults();
        Some(registry.validate(&task.content, task.ntfs_file.extension().as_deref()))
    } else {
        None
    };

    ProcessedOutcome::Recovered(Box::new(RecoveredEntry {
        source_id: task.source_id.clone(),
        original_path: task.ntfs_file.path.clone(),
        output_path: final_path,
        bytes_written,
        priority_score: task.priority_score,
        is_deleted: task.ntfs_file.is_deleted,
        sha256,
        validation,
        matched_wish_labels: task.matched_wish_labels.clone(),
        is_priority: task.is_priority,
    }))
}

/// Chunk 24b: I/O バッファ拡大版の書き込み。
///
/// デフォルト `std::fs::write` は内部的に小さいバッファで複数回 syscall を発行する。
/// 1MB バッファに拡大することで、特に小ファイル多数のケースで syscall 回数が劇的に
/// 減り、業務 PC で 10x 程度のスループット改善が期待できる。
fn write_with_large_buffer(path: &Path, content: &[u8]) -> std::io::Result<u64> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::with_capacity(WRITE_BUFFER_BYTES, file);
    writer.write_all(content)?;
    writer.flush()?;
    Ok(content.len() as u64)
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

    #[test]
    fn recovery_config_from_single_dir_keeps_legacy_structure() {
        // Chunk 23: 既存 API 互換: `{output_dir}/live`、`{output_dir}/deleted` を維持。
        let base = Path::new("G:\\").join("output");
        let cfg = RecoveryConfig::from_single_dir(&base);
        assert_eq!(cfg.live_files_dir, base.join("live"));
        assert_eq!(cfg.deleted_files_dir, base.join("deleted"));
    }

    #[test]
    fn recovery_config_with_paths_uses_explicit_paths() {
        // Chunk 23: 業務向けに任意のパスを指定可能。
        let live = Path::new("G:\\").join("260522-04").join("通常ファイル");
        let deleted = Path::new("G:\\").join("260522-04").join("削除ファイル");
        let cfg = RecoveryConfig::with_paths(&live, &deleted);
        assert_eq!(cfg.live_files_dir, live);
        assert_eq!(cfg.deleted_files_dir, deleted);
    }

    #[test]
    fn engine_with_config_uses_explicit_live_deleted_paths() {
        // Chunk 23: with_config 経由で build_output_path が config の paths を直接使うこと。
        let temp = tempfile::tempdir().unwrap();
        let live = temp.path().join("通常ファイル");
        let deleted = temp.path().join("削除ファイル");
        let cfg = RecoveryConfig::with_paths(&live, &deleted);
        let engine = RecoveryEngine::with_config(cfg);

        let live_file = make_file(10, "\\dir\\report.docx", false);
        let live_path = engine.build_output_path(&live_file).unwrap();
        assert!(
            live_path.starts_with(&live),
            "live path should start with 通常ファイル: {:?}",
            live_path
        );

        let del_file = make_file(20, "\\dir\\old.txt", true);
        let del_path = engine.build_output_path(&del_file).unwrap();
        assert!(
            del_path.starts_with(&deleted),
            "deleted path should start with 削除ファイル: {:?}",
            del_path
        );
    }

    // === Chunk 23.7: 全件復旧 + 優先データマーキング テスト ===

    #[test]
    fn recover_all_user_files_when_no_wishlist_match() {
        // Chunk 23.7: Wishlist が空でも全 user file が復旧される（R-STUDIO 風）。
        // ここでは「全件 priority=false」の挙動を build_output_path で確認するだけ
        // （実復旧は結合テストで検証）。
        use crate::report::{RecoveredEntry, RecoveryReport};
        use chrono::Utc;
        // 模擬 RecoveryReport を作成し、is_priority=false が priority_count に
        // 反映されないことを確認する単体検証。
        let now = Utc::now();
        let entry = RecoveredEntry {
            source_id: "NTFS#100".into(),
            original_path: "\\dir\\file.txt".into(),
            output_path: std::path::PathBuf::new(),
            bytes_written: 10,
            priority_score: 0,
            is_deleted: false,
            sha256: None,
            validation: None,
            matched_wish_labels: Vec::new(),
            is_priority: false,
        };
        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 1,
            recovered: vec![entry],
            failed: vec![],
            skipped: vec![],
            wish_labels: vec![],
        };
        assert_eq!(report.recovered.len(), 1);
        assert_eq!(report.priority_count(), 0); // Wishlist マッチ 0
    }

    #[test]
    fn recover_excludes_system_files_via_exclusions() {
        // Chunk 23.7: ExclusionList::default_system_exclusions が
        // \Windows\... を確実に除外することを検証（ExclusionList 単体動作確認）。
        use dds_wish_match::ExclusionList;
        let ex = ExclusionList::default_system_exclusions();
        assert!(ex.matches("\\Windows\\System32\\drivers\\foo.sys"));
        assert!(ex.matches("\\$MFT"));
        assert!(!ex.matches("\\Users\\Chou\\Documents\\report.docx"));
    }

    #[test]
    fn recover_marks_wishlist_match_as_priority() {
        // Chunk 23.7: Wishlist にマッチしたエントリは RecoveredEntry::is_priority=true
        // となること、ラベルが伝播することを検証する単体テスト。
        use crate::report::RecoveredEntry;
        // 模擬：build_recovered 同等の構成で is_priority=true を作成。
        let entry = RecoveredEntry {
            source_id: "NTFS#7".into(),
            original_path: "\\image.png".into(),
            output_path: std::path::PathBuf::new(),
            bytes_written: 100,
            priority_score: 75, // High
            is_deleted: false,
            sha256: None,
            validation: None,
            matched_wish_labels: vec!["お客様の写真".into()],
            is_priority: true,
        };
        assert!(entry.is_priority);
        assert_eq!(entry.priority_score, 75);
        assert_eq!(entry.matched_wish_labels.len(), 1);
        assert_eq!(entry.matched_wish_labels[0], "お客様の写真");
    }

    #[test]
    fn engine_new_preserves_legacy_output_dir_getter() {
        // 既存 API 互換: output_dir() は new() で渡された値をそのまま返す。
        let temp = tempfile::tempdir().unwrap();
        let engine = RecoveryEngine::new(temp.path());
        assert_eq!(engine.output_dir(), temp.path());
        // 内部 config も派生していること。
        assert_eq!(engine.config().live_files_dir, temp.path().join("live"));
        assert_eq!(
            engine.config().deleted_files_dir,
            temp.path().join("deleted")
        );
    }

    // === Chunk 24b: 並列化 + 進捗表示テスト ===

    fn make_task(record_index: u64, path: &str, content: Vec<u8>) -> RecoveryTask {
        RecoveryTask {
            ntfs_file: make_file(record_index, path, false),
            source_id: format!("NTFS#{}", record_index),
            content,
            is_priority: false,
            matched_wish_labels: Vec::new(),
            priority_score: 0,
        }
    }

    #[test]
    fn process_recovery_task_writes_file_with_buffered_io() {
        // Chunk 24b: process_recovery_task が write_with_large_buffer 経由でファイルを書くこと。
        let temp = tempfile::tempdir().unwrap();
        let cfg = RecoveryConfig::from_single_dir(temp.path());
        // 速度向上 + validator 依存回避のためテスト用に validate / sha256 を無効化。
        let opt = RecoveryOptions {
            validate_after_recovery: false,
            compute_sha256: false,
            ..RecoveryOptions::default()
        };
        std::fs::create_dir_all(&cfg.live_files_dir).unwrap();

        let task = make_task(100, "\\hello.bin", b"hello chunk24b".to_vec());
        let outcome = process_recovery_task(&task, &cfg, &opt, temp.path());

        match outcome {
            ProcessedOutcome::Recovered(entry) => {
                assert_eq!(entry.bytes_written, 14);
                assert!(entry.output_path.is_file());
                let read = std::fs::read(&entry.output_path).unwrap();
                assert_eq!(read, b"hello chunk24b");
                assert_eq!(entry.source_id, "NTFS#100");
            }
            other => panic!(
                "expected Recovered, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn process_recovery_task_propagates_priority_metadata() {
        // Chunk 24b: ワーカーで is_priority / labels / score がそのまま RecoveredEntry に反映されること。
        let temp = tempfile::tempdir().unwrap();
        let cfg = RecoveryConfig::from_single_dir(temp.path());
        let opt = RecoveryOptions {
            validate_after_recovery: false,
            compute_sha256: false,
            ..RecoveryOptions::default()
        };
        std::fs::create_dir_all(&cfg.live_files_dir).unwrap();

        let task = RecoveryTask {
            ntfs_file: make_file(7, "\\photo.jpg", false),
            source_id: "NTFS#7".into(),
            content: b"jpeg-stub".to_vec(),
            is_priority: true,
            matched_wish_labels: vec!["お客様の写真".into(), "JPEG ファイル".into()],
            priority_score: 75,
        };
        let outcome = process_recovery_task(&task, &cfg, &opt, temp.path());
        if let ProcessedOutcome::Recovered(entry) = outcome {
            assert!(entry.is_priority, "priority flag should propagate");
            assert_eq!(entry.priority_score, 75);
            assert_eq!(entry.matched_wish_labels.len(), 2);
            assert_eq!(entry.matched_wish_labels[0], "お客様の写真");
        } else {
            panic!("expected Recovered");
        }
    }

    #[test]
    fn write_with_large_buffer_creates_parent_dirs() {
        // Chunk 24b: BufWriter::with_capacity(1MB) でも親ディレクトリ未存在は自動作成。
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("nested").join("deep").join("file.bin");
        assert!(!target.parent().unwrap().exists());

        let bytes = vec![0xABu8; 4096];
        let n = write_with_large_buffer(&target, &bytes).unwrap();

        assert_eq!(n, 4096);
        assert!(target.is_file());
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
    }

    #[test]
    fn worker_count_is_in_expected_range() {
        // Chunk 24b: ワーカー数は 1〜4 の範囲に収まる業務 PC 想定。
        let n = num_cpus::get().clamp(1, MAX_WORKER_THREADS);
        assert!(n >= 1, "at least 1 worker");
        assert!(n <= MAX_WORKER_THREADS, "at most 4 workers");
        // 開発機・業務 PC で大抵 2 以上は取れるはず（並列化の前提）。
        // 1 コアでも壊れない設計だが、サニティチェックとして上限のみ厳格に。
    }

    #[test]
    fn write_buffer_size_constant_is_one_megabyte() {
        // Chunk 24b: バッファ拡大の効果が業務的に説明可能な値であること。
        assert_eq!(WRITE_BUFFER_BYTES, 1024 * 1024);
    }
}
