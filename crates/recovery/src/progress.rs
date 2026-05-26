//! Chunk 24b: 復旧処理の進捗報告。
//!
//! [`ProgressReporter`] trait を提供し、CLI 用の [`ConsoleProgressReporter`] と
//! テスト用の [`NoopProgressReporter`] を実装する。Phase 2.1 の Tauri UI では
//! 別の実装（`TauriProgressReporter` 等）で trait を満たす想定。
//!
//! ## なぜ trait か
//!
//! 進捗報告の責務（出力先 / フォーマット / 頻度制御）を呼び出し元（CLI / UI）に
//! 委ねるため。`recover_files` 側は「進捗を報告する」インタフェースだけ知り、
//! 表示先（stderr / GUI イベント）には依存しない。
//!
//! ## 並列化対応
//!
//! Chunk 24b で `recover_files` が Producer-Consumer 並列化されるため、
//! `ProgressReporter` は `Send + Sync` を要求する。`ConsoleProgressReporter` は
//! 内部状態（最終報告時刻）を `Mutex` で保護することで両方の制約を満たす。
//!
//! 関連 FR: FR-CLI-08（進捗表示）。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 進捗報告の trait。実装は呼び出し元（CLI / UI）が提供する。
///
/// `Send + Sync` が必須（並列処理から呼ばれるため）。実装側は出力頻度制御の
/// 責任を持つ（例: 5 秒おきにしか出さない、初回と最終は必ず出す、等）。
pub trait ProgressReporter: Send + Sync {
    /// 進捗を報告する。
    ///
    /// - `current`: 現在処理中のファイル番号（1-based）
    /// - `total`: 全ファイル数
    /// - `current_path`: 現在処理中のファイルパス（空文字可、完了時は `""`）
    fn report(&self, current: usize, total: usize, current_path: &str);
}

/// 何もしない実装。テストや進捗表示不要なバッチ処理で使う。
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn report(&self, _current: usize, _total: usize, _current_path: &str) {}
}

/// CLI 向けの進捗報告。
///
/// 指定間隔（デフォルト 5 秒）または最終ファイル時に stderr へ進捗を表示する。
/// フォーマット例:
///
/// ```text
/// [復旧中] 245/1858 ファイル (13.2%) - 経過 0:08 - 現在: \Users\Chou\report.docx
/// ```
pub struct ConsoleProgressReporter {
    start_time: Instant,
    last_report: Mutex<Instant>,
    interval: Duration,
}

impl ConsoleProgressReporter {
    /// 新規作成。デフォルト報告間隔は 5 秒。
    ///
    /// 初回呼び出しが即時に表示されるよう、`last_report` は十分過去に設定する。
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_report: Mutex::new(now - Duration::from_secs(3600)),
            interval: Duration::from_secs(5),
        }
    }

    /// 報告間隔を指定して作成。テスト用に短い間隔も指定可能。
    pub fn with_interval(interval: Duration) -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_report: Mutex::new(now - Duration::from_secs(3600)),
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
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        let elapsed_since_last = now.duration_since(*last);
        let is_final = total > 0 && current == total;
        if elapsed_since_last >= self.interval || is_final {
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

/// 経過時間を `0:43` (分:秒) または `1:02:05` (時:分:秒) 形式に整形する。
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

/// パスが長すぎる場合に「prefix...suffix」で中央を省略する。
///
/// `max_len` 以下の長さならそのまま返す。`max_len` が極端に小さい（< 3）場合は
/// 先頭から `max_len` 文字を切り出して返す（パニックさせない）。char 境界で
/// 切るため、マルチバイト文字でも安全。
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.chars().count() <= max_len {
        return path.to_string();
    }
    if max_len < 3 {
        return path.chars().take(max_len).collect();
    }
    let keep = (max_len - 3) / 2;
    let prefix: String = path.chars().take(keep).collect();
    let total = path.chars().count();
    let suffix: String = path.chars().skip(total - keep).collect();
    format!("{}...{}", prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_under_one_hour() {
        // 125 秒 = 2 分 5 秒 → "2:05"。1 時間未満は `mm:ss`。
        let d = Duration::from_secs(125);
        assert_eq!(format_duration(d), "2:05");
    }

    #[test]
    fn format_duration_over_one_hour() {
        // 3725 秒 = 1 時間 2 分 5 秒 → "1:02:05"。1 時間以上は `h:mm:ss`。
        let d = Duration::from_secs(3725);
        assert_eq!(format_duration(d), "1:02:05");
    }

    #[test]
    fn truncate_path_short_unchanged() {
        // max_len 以下のパスはそのまま返る。
        let path = "C:\\test.txt";
        assert_eq!(truncate_path(path, 50), "C:\\test.txt");
    }

    #[test]
    fn truncate_path_long_truncated() {
        // 長すぎる場合は中央が ... で省略される。
        let path = "C:\\Users\\Chou\\Documents\\very\\deep\\nested\\folder\\report.docx";
        let result = truncate_path(path, 30);
        assert!(
            result.chars().count() <= 30,
            "result too long: {:?}",
            result
        );
        assert!(result.contains("..."), "ellipsis missing: {:?}", result);
        assert!(result.starts_with("C:\\"), "prefix missing: {:?}", result);
        assert!(result.ends_with(".docx"), "suffix missing: {:?}", result);
    }

    #[test]
    fn truncate_path_handles_multibyte_safely() {
        // 業務上「お客様」「復旧データ」など日本語パスが入る。char 境界で切ること。
        let path = "\\お客様データ\\写真フォルダ\\旅行記念撮影_2025_夏休み.jpg";
        let result = truncate_path(path, 20);
        assert!(result.chars().count() <= 20);
        // バイト境界違反でクラッシュしないこと（assert_eq! まで到達すれば OK）。
        assert!(!result.is_empty());
    }

    #[test]
    fn noop_reporter_does_nothing() {
        // クラッシュしないことだけ確認。出力は無い。
        let reporter = NoopProgressReporter;
        reporter.report(1, 100, "test.txt");
        reporter.report(100, 100, "");
    }

    #[test]
    fn console_reporter_respects_interval() {
        // 大きな interval を設定 → 初回は表示、直後の 2 回目は閾値未満で抑制。
        // 実 stderr 出力の検査はしない（テストランナー外で観察）。
        // ここでは「クラッシュしない」「Mutex 競合がない」ことを担保する。
        let reporter = ConsoleProgressReporter::with_interval(Duration::from_secs(1000));
        reporter.report(1, 100, "first.txt");
        reporter.report(2, 100, "second.txt");
        // total == current のときは間隔に関わらず出力されること（パスでクラッシュしない）。
        reporter.report(100, 100, "");
    }
}
