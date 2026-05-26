# Chunk 24b 指示: 並列化によるパフォーマンス改善 + 進捗表示

実機ドライランで判明したパフォーマンス問題と UX 問題を解決:

1. **復旧速度**: 4 MB/s → 50-100 MB/s (12-25 倍速、目標 100 MB/s)
2. **進捗表示**: 5 秒おきに「N/M ファイル + 現在ファイル名」

> 🎯 完了時点で「Phase 1.5 が業務的に十分使える性能と UX に到達」する。Chunk 24a と合わせて 2 回目のドライランで業務適用品質を確定。

---

## 背景: 実機ドライランで判明した問題

```
[実機テスト計測値]
ソース HDD: 120 GB (実利用 5 GB 未満)
ファイル数: 1858 件
復旧データ量: 4.52 GB
復旧時間: 20 分
速度: 4.52 GB / 20 分 ≈ 3.86 MB/s

[平均ファイルサイズ]
4.52 GB / 1858 件 = 約 2.4 MB/ファイル
→ 小ファイル多数のケース、並列化が効く
```

### 業務的な目標

```
[Chouさんの希望 (Q1)]
最低ライン: 1TB / 3 時間 (約 100 MB/s) ← 25 倍速
理想:       1TB / 1 時間 (約 300 MB/s) ← 75 倍速

[Chunk 24b の目標]
100 MB/s 到達を最低ライン
並列化のみで届かない場合、追加最適化 (Chunk 24c) を検討
```

## 目的

2 つの統合された改善:

| Part | 内容 | 所要 |
|---|---|---|
| **A** | 進捗表示 (CLI、5 秒おき、現在ファイル名表示) | 1-2 時間 |
| **B** | ファイル並列化 (rayon ベース、I/O バッファ拡大、SHA256 並行) | 3-5 時間 |
| **C** | パフォーマンス計測 | 30 分 |

合計: 5-7 時間

## 対象クレート

- **修正**: `crates/recovery/`, `crates/workbench-dryrun/`
- **新規依存**: `rayon`, `crossbeam-channel` (どちらか)
- **影響テスト**: 既存テスト約 10 件の調整

## 重要な設計原則

### NtfsVolume のシリアル制約

NtfsVolume の reader closure (`F: FnMut(u64, u64) -> Result<Vec<u8>, _>`) は **`FnMut` でシングルスレッド前提**。並列化は以下のパターンで実現:

```
[アプローチ: Producer-Consumer]
プロデューサ (1 スレッド):
  NtfsVolume からファイル内容を順次読み出し
  → channel に (metadata, content) を送る

コンシューマ (N スレッド、N = CPU コア数、最大 4):
  channel から受け取って:
    - ファイル書き込み (output_path への write)
    - SHA256 計算
    - Validator 実行
  → RecoveredEntry を生成、結果集約
```

これで「NTFS read のシリアル」と「post-processing の並列」を両立。

### 並列化の効果見積もり

```
[現状 4 MB/s のボトルネック分析]
1. ファイル毎のオーバーヘッド (1858 件)
   - open → write → SHA256 → validate → close
   - 各処理が累積的にシリアル実行
   
2. SHA256 計算 (CPU バウンド)
   - 全ファイル分の計算がシリアル
   
3. Validator (CPU バウンド)
   - DOCX/XLSX 等 ZIP 解凍が重い

[並列化の効果]
4 コア並列 (現実的な PC スペック):
  CPU バウンド処理が 4x 速くなる
  4 MB/s × 4 = 16 MB/s (理論値)

[追加最適化]
+ I/O バッファ拡大 (BufWriter 64KB → 1MB)
+ SHA256 並行計算 (write と同時)
→ 50-100 MB/s 期待
```

100 MB/s 到達は実装次第。Chunk 24b で計測し、足りなければ Chunk 24c で追加最適化。

### unsafe 0 維持

Chunk 24a で `timestamps.rs` に限定的に unsafe を導入したが、本チャンクでは:

```
✓ 並列化に unsafe は不要 (crossbeam/rayon は safe API)
✓ I/O バッファ拡大に unsafe は不要 (BufWriter)
✓ SHA256 並行に unsafe は不要 (sha2 クレート)
```

→ Chunk 24b の追加 unsafe は **0 行**。

## 仕様参照

### ビジネス要件

- **FR-REC-08** (復旧速度の業務適用性、目標 100 MB/s) ← 新規達成
- **FR-CLI-08** (進捗表示) ← 新規達成

## 実装内容

### Part A: 進捗表示

#### `crates/recovery/src/progress.rs` (新規ファイル)

```rust
//! 復旧処理の進捗報告
//!
//! `ProgressReporter` trait と、CLI 用の `ConsoleProgressReporter` を提供。
//! Phase 2.1 UI では別の実装 (TauriProgressReporter 等) で置き換える想定。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 進捗報告の trait。実装は呼び出し元 (CLI / UI) が提供。
///
/// 注意: `Send + Sync` が必要 (並列処理から呼ばれるため)
pub trait ProgressReporter: Send + Sync {
    /// 進捗を報告する。
    ///
    /// - `current`: 現在のファイル番号 (1-based)
    /// - `total`: 全ファイル数
    /// - `current_path`: 現在処理中のファイルパス
    ///
    /// 実装側は出力頻度を制御する責任を持つ (例: 5 秒おき)。
    fn report(&self, current: usize, total: usize, current_path: &str);
}

/// 何もしない実装 (テスト用)
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _current: usize, _total: usize, _current_path: &str) {}
}

/// CLI 向けの進捗報告。
/// 指定間隔 (デフォルト 5 秒) で stderr に進捗を表示する。
pub struct ConsoleProgressReporter {
    start_time: Instant,
    last_report: Mutex<Instant>,
    interval: Duration,
}

impl ConsoleProgressReporter {
    /// 新規作成。デフォルトの間隔は 5 秒。
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_report: Mutex::new(now - Duration::from_secs(10)),  // 初回即表示
            interval: Duration::from_secs(5),
        }
    }
    
    /// 報告間隔を指定して作成
    pub fn with_interval(interval: Duration) -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_report: Mutex::new(now - Duration::from_secs(10)),
            interval,
        }
    }
}

impl Default for ConsoleProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for ConsoleProgressReporter {
    fn report(&self, current: usize, total: usize, current_path: &str) {
        let now = Instant::now();
        let mut last = match self.last_report.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        
        if now.duration_since(*last) >= self.interval || current == total {
            let elapsed = now.duration_since(self.start_time);
            let percent = if total > 0 {
                (current as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            
            eprintln!(
                "[復旧中] {}/{} ファイル ({:.1}%) - 経過 {} - 現在: {}",
                current,
                total,
                percent,
                format_duration(elapsed),
                truncate_path(current_path, 50),
            );
            *last = now;
        }
    }
}

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}

/// パスが長い場合に末尾を残して中央を ... で省略
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let keep = max_len.saturating_sub(3) / 2;
    let prefix = &path[..keep];
    let suffix = &path[path.len() - keep..];
    format!("{}...{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn format_duration_under_one_hour() {
        let d = Duration::from_secs(125);  // 2 分 5 秒
        assert_eq!(format_duration(d), "2:05");
    }
    
    #[test]
    fn format_duration_over_one_hour() {
        let d = Duration::from_secs(3725);  // 1 時間 2 分 5 秒
        assert_eq!(format_duration(d), "1:02:05");
    }
    
    #[test]
    fn truncate_path_short_unchanged() {
        let path = "C:\\test.txt";
        assert_eq!(truncate_path(path, 50), "C:\\test.txt");
    }
    
    #[test]
    fn truncate_path_long_truncated() {
        let path = "C:\\Users\\Chou\\Documents\\very\\deep\\nested\\folder\\report.docx";
        let result = truncate_path(path, 30);
        assert!(result.len() <= 30);
        assert!(result.contains("..."));
        assert!(result.starts_with("C:\\"));
        assert!(result.ends_with(".docx"));
    }
    
    #[test]
    fn noop_reporter_does_nothing() {
        let reporter = NoopProgressReporter;
        // ただ呼び出してもクラッシュしない
        reporter.report(1, 100, "test.txt");
    }
    
    #[test]
    fn console_reporter_respects_interval() {
        let reporter = ConsoleProgressReporter::with_interval(Duration::from_secs(1000));
        // 初回は即表示、2 回目は表示しない (時間経過なし)
        // 実際の stderr 出力は確認できないが、クラッシュしないこと
        reporter.report(1, 100, "first.txt");
        reporter.report(2, 100, "second.txt");
    }
}
```

#### `crates/recovery/src/lib.rs` への追加

```rust
pub mod progress;
pub use progress::{ProgressReporter, ConsoleProgressReporter, NoopProgressReporter};
```

### Part B: 並列化 + 最適化

#### Cargo.toml への依存追加

```toml
# crates/recovery/Cargo.toml
[dependencies]
# 既存依存に追加:
crossbeam-channel = "0.5"
rayon = "1.10"
num_cpus = "1.16"
```

#### `crates/recovery/src/engine.rs` の修正

```rust
use std::sync::Arc;
use crossbeam_channel::{bounded, Receiver, Sender};

impl RecoveryEngine {
    /// 並列化された復旧処理。
    ///
    /// 構成:
    /// - プロデューサスレッド: NtfsVolume からファイル内容を順次読み出し
    /// - コンシューマスレッド × N: write + SHA256 + validate を並列実行
    /// - N = CPU コア数 (最大 4、業務 PC のスペックを考慮)
    pub fn recover_files<F, P>(
        &self,
        volume: &mut NtfsVolume<F>,
        wishlist: &Wishlist,
        exclusions: &ExclusionList,
        progress: &P,
    ) -> Result<RecoveryReport, RecoveryError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
        P: ProgressReporter,
    {
        let started_at = Utc::now();
        
        // Step 1: ファイルメタデータを先に収集 (NTFS 走査、シリアル)
        let user_files = collect_user_files_metadata(volume, exclusions)?;
        let total = user_files.len();
        let mut total_matched = total;
        
        // Step 2: チャネル設定
        // bounded で背圧制御 (メモリ消費を制限)
        let worker_count = num_cpus::get().min(4).max(1);
        let queue_size = worker_count * 2;
        let (task_tx, task_rx): (Sender<RecoveryTask>, Receiver<RecoveryTask>) = bounded(queue_size);
        let (result_tx, result_rx): (Sender<ProcessedEntry>, Receiver<ProcessedEntry>) = bounded(queue_size);
        
        // Step 3: ワーカースレッド起動
        let workers: Vec<_> = (0..worker_count).map(|_| {
            let task_rx = task_rx.clone();
            let result_tx = result_tx.clone();
            let config = self.config.clone();
            let wishlist = wishlist.clone();
            
            std::thread::spawn(move || {
                while let Ok(task) = task_rx.recv() {
                    let result = process_recovery_task(&task, &config, &wishlist);
                    if result_tx.send(result).is_err() {
                        break;  // 受信側が閉じられた
                    }
                }
            })
        }).collect();
        
        // Step 4: プロデューサ (メインスレッド)
        // NtfsVolume からファイル内容を読み出し、ワーカーキューに投入
        // result も受信
        let mut recovered = Vec::with_capacity(total);
        let mut failed = Vec::new();
        let mut skipped = Vec::new();
        let mut current_count = 0usize;
        
        // 受信処理用のスレッド (タスク投入と並行)
        let result_collector_handle = std::thread::spawn(move || {
            let mut entries = Vec::new();
            while let Ok(processed) = result_rx.recv() {
                entries.push(processed);
            }
            entries
        });
        
        // プロデューサ: ファイル読み出し → タスク投入
        for (idx, file_meta) in user_files.iter().enumerate() {
            current_count = idx + 1;
            progress.report(current_count, total, &file_meta.path);
            
            // NtfsVolume から内容を読む (シリアル必須)
            let content = match read_file_content(volume, file_meta) {
                Ok(c) => c,
                Err(e) => {
                    failed.push(FailedEntry {
                        source_id: file_meta.entry_index,
                        original_path: file_meta.path.clone(),
                        reason: format!("読み出しエラー: {}", e),
                    });
                    continue;
                }
            };
            
            let task = RecoveryTask {
                file_meta: file_meta.clone(),
                content,
            };
            
            if task_tx.send(task).is_err() {
                break;  // ワーカーが全て停止
            }
        }
        
        // タスク投入完了、ワーカーに終了通知
        drop(task_tx);
        
        // ワーカー終了待ち
        for w in workers {
            w.join().map_err(|_| RecoveryError::WorkerPanic)?;
        }
        
        // 結果収集完了
        drop(result_tx);
        let entries = result_collector_handle.join()
            .map_err(|_| RecoveryError::WorkerPanic)?;
        
        // Step 5: 結果集約
        for entry in entries {
            match entry {
                ProcessedEntry::Success(e) => recovered.push(e),
                ProcessedEntry::Failed(f) => failed.push(f),
            }
        }
        
        // 最終進捗 (100%)
        progress.report(total, total, "");
        
        let finished_at = Utc::now();
        
        Ok(RecoveryReport {
            started_at,
            finished_at,
            total_matched,
            recovered,
            failed,
            skipped,
            wish_labels: wishlist.wishes.iter().map(|w| w.label.clone()).collect(),
            case_id: ...,  // 既存通り
        })
    }
}

// 内部型
struct RecoveryTask {
    file_meta: FileMetadata,
    content: Vec<u8>,
}

enum ProcessedEntry {
    Success(RecoveredEntry),
    Failed(FailedEntry),
}

fn process_recovery_task(
    task: &RecoveryTask,
    config: &RecoveryConfig,
    wishlist: &Wishlist,
) -> ProcessedEntry {
    // ファイルパス計算
    let output_path = compute_output_path(config, &task.file_meta);
    
    // バッファサイズ拡大した書き込み
    let bytes_written = match write_with_large_buffer(&output_path, &task.content) {
        Ok(n) => n,
        Err(e) => return ProcessedEntry::Failed(FailedEntry {
            source_id: task.file_meta.entry_index,
            original_path: task.file_meta.path.clone(),
            reason: format!("書き込みエラー: {}", e),
        }),
    };
    
    // SHA256 計算 (CPU バウンド、並列の恩恵)
    let sha256 = compute_sha256(&task.content);
    
    // Validator 実行 (CPU バウンド、並列の恩恵)
    let validation = run_validators(&task.content, task.file_meta.extension.as_deref());
    
    // Wishlist マッチ
    let wish_matches = wishlist.match_metadata(&task.file_meta);
    let is_priority = !wish_matches.is_empty();
    let matched_wishes: Vec<String> = wish_matches.iter()
        .map(|w| w.label.clone()).collect();
    let priority_score = wish_matches.iter()
        .map(|w| w.priority.score()).max().unwrap_or(0);
    
    // タイムスタンプ保持 (Chunk 24a で追加済み)
    let timestamps = NtfsTimestamps {
        created: task.file_meta.creation_time,
        modified: task.file_meta.modified_time,
        accessed: task.file_meta.accessed_time,
    };
    if let Err(e) = apply_timestamps(&output_path, &timestamps) {
        log::warn!("タイムスタンプ書き込み失敗: {:?} ({})", output_path, e);
    }
    
    ProcessedEntry::Success(RecoveredEntry {
        source_id: task.file_meta.entry_index,
        original_path: task.file_meta.path.clone(),
        output_path,
        bytes_written,
        is_deleted: task.file_meta.is_deleted,
        priority_score,
        is_priority,
        matched_wishes,
        sha256,
        validation: Some(validation),
    })
}

/// I/O バッファ拡大版の書き込み
/// デフォルト BufWriter (8KB) → 1MB に拡大
fn write_with_large_buffer(path: &Path, content: &[u8]) -> std::io::Result<u64> {
    use std::io::{BufWriter, Write};
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let file = std::fs::File::create(path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);  // 1MB バッファ
    writer.write_all(content)?;
    writer.flush()?;
    
    Ok(content.len() as u64)
}
```

#### `crates/recovery/src/error.rs` への追加

```rust
#[derive(Debug, Error)]
pub enum RecoveryError {
    // 既存バリアント...
    
    #[error("ワーカースレッドが panic しました")]
    WorkerPanic,
}
```

### Part C: workbench-dryrun で進捗表示利用

`crates/workbench-dryrun/src/commands/recover.rs` の修正:

```rust
use dds_recovery::{ConsoleProgressReporter, ProgressReporter};

pub fn run() -> Result<()> {
    // ... 既存処理 ...
    
    // Step 7: 復旧実行
    println!();
    println!("[復旧開始]");
    let start = std::time::Instant::now();
    
    let progress = ConsoleProgressReporter::new();
    let mut volume = open_ntfs_volume(&source_drive.access_path)?;
    
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &progress,  // ★ 新規パラメータ
    ).context("復旧の実行に失敗しました")?;
    
    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    
    // 速度計算 (パフォーマンス確認用)
    let mb_per_sec = (result.report.total_bytes_written() as f64 / 1_048_576.0)
        / elapsed.as_secs_f64().max(0.001);
    println!("  速度: {:.1} MB/s", mb_per_sec);
    
    // ... 既存処理 (結果表示) ...
}
```

### Part D: case-manager の execute_business_recovery 更新

```rust
pub fn execute_business_recovery<F, P>(
    case: &mut Case,
    drive_root: impl AsRef<Path>,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
    exclusions: &ExclusionList,
    storage: &CaseStorage,
    progress: &P,  // ★ 新規パラメータ
) -> Result<BusinessRecoveryResult, BusinessRecoveryError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    P: ProgressReporter,
{
    let case_output = CaseOutput::new(case.case_id.clone(), drive_root.as_ref().to_path_buf());
    case_output.create_all_dirs()?;
    
    let config = RecoveryConfig::from_case_output(&case_output);
    let engine = RecoveryEngine::with_config(config);
    let report = engine.recover_files(volume, wishlist, exclusions, progress)?;
    
    let report_paths = dds_report::write_business_reports(
        &report, &case_output, storage.base_dir(),
    )?;
    
    case.output_dir = Some(case_output.root());
    case.recovery_report_summary = Some(summarize_report(&report));
    case.wishlist = Some(wishlist.clone());
    
    Ok(BusinessRecoveryResult { case_output, report, report_paths })
}
```

### Part E: パフォーマンス計測 product_demo

`crates/case-manager/tests/business_flow_integration.rs` に追加:

```rust
#[test]
#[ignore]  // 通常テストでは実行しない、計測用
fn perf_demo_chunk24b_recovery_speed() {
    use std::time::Instant;
    
    // ntfs_mixed_formats では小さすぎる、より大きいフィクスチャ or 実機相当のデータ
    // ※ 実機は別途、Chouさんが手動で実行
    
    // ... setup ...
    
    let progress = NoopProgressReporter;  // テストでは進捗無視
    let start = Instant::now();
    
    let result = execute_business_recovery(
        &mut case, delivery_dir.path(), &mut volume, &wishlist, &exclusions, &storage, &progress,
    ).unwrap();
    
    let elapsed = start.elapsed();
    
    let total_bytes = result.report.total_bytes_written();
    let mb_per_sec = (total_bytes as f64 / 1_048_576.0) / elapsed.as_secs_f64().max(0.001);
    
    println!("\n=== Chunk 24b Performance Demo ===\n");
    println!("ファイル数:   {}", result.report.recovered.len());
    println!("データ量:    {}", format_bytes(total_bytes));
    println!("経過時間:    {:.2} 秒", elapsed.as_secs_f64());
    println!("速度:        {:.1} MB/s", mb_per_sec);
    println!();
    println!("注: ベースライン (Chunk 24a 時点): 約 4 MB/s");
    println!("    目標 (Chunk 24b):              50-100 MB/s");
    println!("=== Performance Demo End ===\n");
    
    // パフォーマンステストではアサーションは緩く
    // フィクスチャは小さいので絶対値より「動く」を確認
    assert!(result.report.recovered.len() > 0);
}
```

## 単体テスト要件 (最低 8 件、新規)

### `progress.rs` (最低 5 件)

1. `format_duration_under_one_hour`
2. `format_duration_over_one_hour`
3. `truncate_path_short_unchanged`
4. `truncate_path_long_truncated`
5. `console_reporter_respects_interval`

### `engine.rs` 並列化 (最低 3 件)

6. `parallel_recovery_processes_all_files`: 全ファイル処理される
7. `parallel_recovery_with_worker_panic_handled`: ワーカー panic 時のエラー
8. `parallel_recovery_progress_called_for_each_file`: progress.report が呼ばれる

## 結合テスト要件 (最低 1 件)

```rust
#[test]
fn business_recovery_with_progress_reporter() {
    // ... setup ...
    
    let progress = Arc::new(MockProgressReporter::new());
    let result = execute_business_recovery(
        &mut case, delivery_dir.path(), &mut volume, &wishlist, &exclusions, &storage,
        progress.as_ref(),
    ).unwrap();
    
    // 進捗が複数回呼ばれた
    assert!(progress.call_count() > 0);
    // 最後の進捗は 100%
    assert_eq!(progress.last_current(), result.report.recovered.len());
}

struct MockProgressReporter {
    calls: Mutex<Vec<(usize, usize, String)>>,
}
// ... impl ...
```

## 制約

- **行数目安**:
  - `recovery/src/progress.rs` (新規): 130 行 + テスト 60 行
  - `recovery/src/engine.rs` 修正: +150 行 (並列化ロジック)
  - `recovery/src/error.rs` 修正: +5 行
  - `recovery/Cargo.toml` 修正: +3 行 (crossbeam, rayon, num_cpus)
  - `case-manager/src/orchestration.rs` 修正: +15 行 (progress パラメータ)
  - `workbench-dryrun/src/commands/recover.rs` 修正: +10 行
  - 既存テスト修正: 約 10 件、約 30 行
  - 合計: 約 350 行追加・修正
- **単体テスト新規**: 最低 8 件
- **結合テスト新規**: 最低 1 件
- **`unsafe` 追加行数**: 0 (Chunk 24a の 5-10 行から増えない)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `recover_files` が並列化されている (workers >= 2)
- [ ] ConsoleProgressReporter が 5 秒おきに stderr に出力
- [ ] 進捗表示に「N/M ファイル」「経過時間」「現在ファイル名」が含まれる
- [ ] workbench-dryrun の recover で進捗が見える
- [ ] workbench-dryrun の recover 完了時に速度 (MB/s) が表示される
- [ ] `unsafe` 追加なし (Chunk 24a の 5-10 行のまま)

## 関連 FR 要件

- **FR-REC-08** (復旧速度、目標 100 MB/s) ← 達成見込み (実機で検証)
- **FR-CLI-08** (進捗表示) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1.5 業務適用品質の最終形完成**
4. **Chouさんが 2 回目の実機ドライラン実施**
   - Chunk 24a (お客様向け簡素化、タイムスタンプ) の確認
   - Chunk 24b (パフォーマンス、進捗) の確認
   - 4 MB/s → 50-100 MB/s の改善を実測
5. パフォーマンス目標未達なら Chunk 24c (追加最適化) を検討

---

## 注意事項

### crossbeam-channel vs rayon の選択

並列化の実装方法は 2 つ:

```
[crossbeam-channel + std::thread (指示書の例)]
- Producer-Consumer パターン
- NTFS read のシリアル制約を明示的に守る
- 実装が明確、デバッグしやすい
- 推奨

[rayon::par_iter()]
- バッチで並列処理
- 実装がシンプル
- ただし NTFS read のシリアル制約が暗黙的
- メモリ消費に注意
```

実装者 (Claude Code) の判断に委ねますが、**crossbeam-channel の方が安全**です。

### 並列度の決定

```rust
let worker_count = num_cpus::get().min(4).max(1);
```

- min(4): 業務 PC で 8 コア以上ある場合、I/O が支配的になるので 4 で十分
- max(1): 単一コア PC でも動く

ディスク I/O が支配的なら、CPU コア数を増やしても効果は限定的。並列化の主目的は「CPU バウンドな SHA256/Validation の並列化」。

### メモリ消費の制御

```rust
let queue_size = worker_count * 2;
let (task_tx, task_rx) = bounded(queue_size);
```

bounded(N) で背圧制御:
- ワーカーが処理しきれないと、プロデューサがブロック
- メモリに無制限にファイル content を溜め込まない
- 大きいファイル (100MB+) が多いケースでも OOM 回避

### 100 MB/s 未達時のフォールバック

```
[Chunk 24b で並列化のみ → 計測]
場合 1: 100 MB/s 到達 → Phase 1.5 完成
場合 2: 50-100 MB/s → 業務的に許容、ただし次の改善余地として記録
場合 3: < 50 MB/s → Chunk 24c で追加最適化必要
```

Chunk 24c の候補:
- SHA256 並行計算 (write と同時進行)
- Validator の軽量化 (重い ZIP 解凍を省略可能オプション化)
- ファイル読み出しのプリフェッチ
- バッチ書き込みの最適化

Chunk 24b で計測してから判断。

### Phase 2.1 UI への引き継ぎ

```rust
// Phase 2.1 で実装する Tauri 用 ProgressReporter
pub struct TauriProgressReporter {
    app_handle: tauri::AppHandle,
}

impl ProgressReporter for TauriProgressReporter {
    fn report(&self, current: usize, total: usize, current_path: &str) {
        self.app_handle.emit_all("recovery-progress", json!({
            "current": current,
            "total": total,
            "path": current_path,
        })).ok();
    }
}
```

Chunk 24b で trait が定義されるので、Phase 2.1 UI で簡単に実装できる。

---

## 質問が必要なケース

- num_cpus クレートが既に使われている場合
- 既存の RecoveryEngine が想定外の構造になっている場合
- crossbeam-channel と std::sync::mpsc の選択

---

## 完了報告例

```markdown
## Chunk 24b 完了報告

### 新規ファイル
- crates/recovery/src/progress.rs (130 行 + テスト 60 行)

### 修正ファイル
- crates/recovery/src/engine.rs (並列化ロジック +150 行)
- crates/recovery/src/error.rs (+5 行 WorkerPanic バリアント)
- crates/recovery/Cargo.toml (+3 行 crossbeam-channel, rayon, num_cpus)
- crates/case-manager/src/orchestration.rs (+15 行 progress パラメータ)
- crates/workbench-dryrun/src/commands/recover.rs (+10 行 進捗 + 速度表示)
- 既存テスト 10 件、約 30 行修正

### 並列化アーキテクチャ
- crossbeam-channel ベースの Producer-Consumer
- ワーカースレッド: CPU コア数 (最大 4)
- バッファサイズ: worker_count × 2 (背圧制御)
- I/O バッファ: 1MB (デフォルト 8KB から拡大)

### unsafe 統計
- 全 workspace の unsafe 行数: 5-10 行 (Chunk 24a から増加なし)
- timestamps.rs に限定されたまま

### テスト統計
- 単体: 既存 + 新規 8 件
- 結合: 既存 + 新規 1 件
- 全 workspace: **530+ 件 pass**

### パフォーマンス計測 (フィクスチャ ntfs_mixed_formats、15 ファイル)
- 並列化前: X.X 秒
- 並列化後: X.X 秒
- 速度: X.X MB/s
- 注: フィクスチャが小さいため、実測は実機ドライランで

### 進捗表示サンプル
```
[復旧開始]
[復旧中] 245/1858 ファイル (13.2%) - 経過 0:08 - 現在: \Users\Chou\Documents\report.docx
[復旧中] 612/1858 ファイル (32.9%) - 経過 0:16 - 現在: \Users\Chou\Pictures\photo_001.jpg
[復旧中] 978/1858 ファイル (52.6%) - 経過 0:24 - 現在: \Users\Chou\Downloads\manual.pdf
[復旧中] 1340/1858 ファイル (72.1%) - 経過 0:32 - 現在: \Users\Chou\Documents\plan.pptx
[復旧中] 1705/1858 ファイル (91.8%) - 経過 0:40 - 現在: \Users\Chou\misc\notes.txt
[復旧中] 1858/1858 ファイル (100.0%) - 経過 0:43 - 現在: 
[復旧完了 - 43.21 秒]
  速度: 107.5 MB/s
```

### 🎉 マイルストーン
- **Phase 1.5 業務適用品質完成**
- 並列化により業務的に許容可能な速度に到達 (予想 50-100 MB/s)
- 進捗表示で「動いている」感が出る、お客様待ち時間の不安解消
- Chouさんの 2 回目実機ドライランで実速度を計測

- **関連 FR**: FR-REC-08、FR-CLI-08 (達成)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんによる 2 回目実機ドライラン
   - Chunk 24a (お客様向け簡素化、タイムスタンプ) の確認
   - Chunk 24b (パフォーマンス、進捗) の確認
```
