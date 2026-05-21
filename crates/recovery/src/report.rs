//! Chunk 17: 復旧結果レポート。
//!
//! 復旧パイプラインが per-file で「成功 / 失敗 / スキップ」のいずれかを記録し、
//! 全体集計（成功率・所要時間・総書込バイト数）を返す。Chunk 19（レポート生成）
//! が `RecoveryReport` を受け取って PDF/Excel/HTML 出力する設計。

use chrono::{DateTime, Utc};
use dds_validators::ValidationResult;
use std::path::PathBuf;

/// 復旧結果の全体レポート。1 回の `recover_files` 呼び出し結果を表現。
#[derive(Debug)]
pub struct RecoveryReport {
    /// 復旧処理を開始した時刻（UTC）。
    pub started_at: DateTime<Utc>,
    /// 復旧処理が完了した時刻（UTC）。
    pub finished_at: DateTime<Utc>,
    /// wish-match がマッチさせた総ファイル数（復旧試行対象の総数）。
    pub total_matched: usize,
    /// 復旧に成功したファイルのリスト。
    pub recovered: Vec<RecoveredEntry>,
    /// 復旧に失敗したファイルのリスト（個別 I/O エラー等）。
    pub failed: Vec<FailedEntry>,
    /// スキップされたファイルのリスト（サイズ超過・衝突 Skip 戦略等）。
    pub skipped: Vec<SkippedEntry>,
}

impl RecoveryReport {
    /// 復旧成功率（パーセント、0.0〜100.0）。`total_matched == 0` のときは `0.0`。
    pub fn success_rate(&self) -> f64 {
        if self.total_matched == 0 {
            return 0.0;
        }
        (self.recovered.len() as f64) / (self.total_matched as f64) * 100.0
    }

    /// 復旧処理に要した時間（ミリ秒）。
    pub fn duration_ms(&self) -> i64 {
        (self.finished_at - self.started_at).num_milliseconds()
    }

    /// 復旧成功ファイルの合計書込バイト数。
    pub fn total_bytes_written(&self) -> u64 {
        self.recovered.iter().map(|e| e.bytes_written).sum()
    }

    /// 検証で `Valid` 判定されたファイル数（Chunk 18）。
    pub fn validated_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_valid())
                    .unwrap_or(false)
            })
            .count()
    }

    /// 検証で `Invalid` 判定されたファイル数（Chunk 18）。
    pub fn invalid_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_invalid())
                    .unwrap_or(false)
            })
            .count()
    }

    /// 検証で `Uncertain` 判定されたファイル数（Chunk 18）。
    ///
    /// `validation` フィールドが `None` のもの（`validate_after_recovery=false`）はカウントしない。
    pub fn uncertain_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_uncertain())
                    .unwrap_or(false)
            })
            .count()
    }
}

/// 復旧成功したファイル 1 件の詳細情報。
#[derive(Debug, Clone)]
pub struct RecoveredEntry {
    /// `FileInfo::source_id` 互換の識別子（例: `"NTFS#67"`）。
    pub source_id: String,
    /// 原本 FS 上のフルパス（例: `\dir1\file.txt`）。
    pub original_path: String,
    /// 実出力先パス（例: `output/live/dir1/file.txt`）。
    pub output_path: PathBuf,
    /// 出力ファイルに書き込んだバイト数。
    pub bytes_written: u64,
    /// マッチした希望の優先度スコア合計（`MatchResult::priority_score` 由来）。
    pub priority_score: u32,
    /// 原本が削除済みエントリだったか。
    pub is_deleted: bool,
    /// 出力ファイル内容の SHA256（`RecoveryOptions::compute_sha256` が `true` のとき）。
    pub sha256: Option<String>,

    /// 復旧後の検証結果（Chunk 18）。
    /// `RecoveryOptions::validate_after_recovery` が `true` のときのみ `Some`。
    pub validation: Option<ValidationResult>,
}

/// 復旧失敗したファイル 1 件。レポートで原因確認できるようにする。
#[derive(Debug, Clone)]
pub struct FailedEntry {
    /// `FileInfo::source_id`。
    pub source_id: String,
    /// 原本 FS 上のフルパス。
    pub original_path: String,
    /// 失敗理由のメッセージ。
    pub error_message: String,
}

/// スキップされたファイル 1 件。サイズ上限超過や衝突 Skip 戦略時に記録。
#[derive(Debug, Clone)]
pub struct SkippedEntry {
    /// `FileInfo::source_id`。
    pub source_id: String,
    /// 原本 FS 上のフルパス。
    pub original_path: String,
    /// スキップ理由。
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn build_recovered(bytes: u64) -> RecoveredEntry {
        RecoveredEntry {
            source_id: format!("NTFS#{}", bytes),
            original_path: String::new(),
            output_path: PathBuf::new(),
            bytes_written: bytes,
            priority_score: 50,
            is_deleted: false,
            sha256: None,
            validation: None,
        }
    }

    #[test]
    fn success_rate_calculates_percentage() {
        // 7 件成功 / 10 件マッチ = 70.0%。Chunk 19 のレポート集計の基底ロジック。
        let now = Utc::now();
        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 10,
            recovered: (0..7).map(|_| build_recovered(0)).collect(),
            failed: Vec::new(),
            skipped: Vec::new(),
        };
        assert!((report.success_rate() - 70.0).abs() < 0.01);
    }

    #[test]
    fn success_rate_zero_when_no_matches() {
        let now = Utc::now();
        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 0,
            recovered: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
        };
        assert_eq!(report.success_rate(), 0.0);
    }

    #[test]
    fn total_bytes_written_sums_all_recovered() {
        let now = Utc::now();
        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 3,
            recovered: vec![build_recovered(100), build_recovered(200), build_recovered(300)],
            failed: Vec::new(),
            skipped: Vec::new(),
        };
        assert_eq!(report.total_bytes_written(), 600);
    }

    #[test]
    fn validation_counts_classify_by_status() {
        // Chunk 18 業務観測: validation フィールドの 3 値で集計が分離されること。
        let now = Utc::now();
        let mut valid_entry = build_recovered(10);
        valid_entry.validation = Some(ValidationResult::valid(
            "PNG",
            "png_v1",
            vec!["magic OK".into()],
            "PNG 画像として正常です",
            None,
        ));
        let mut invalid_entry = build_recovered(20);
        invalid_entry.validation = Some(ValidationResult::invalid(
            "PNG",
            "png_v1",
            "bad header",
            "PNG ヘッダーが壊れています",
            "IHDR 破損のため再復旧推奨",
        ));
        let mut uncertain_entry = build_recovered(30);
        uncertain_entry.validation = Some(ValidationResult::uncertain(
            "no validator",
            "自動検証の対象外です",
            "CS で確認",
        ));
        let none_entry = build_recovered(40); // validation = None

        let report = RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 4,
            recovered: vec![valid_entry, invalid_entry, uncertain_entry, none_entry],
            failed: Vec::new(),
            skipped: Vec::new(),
        };
        assert_eq!(report.validated_count(), 1);
        assert_eq!(report.invalid_count(), 1);
        assert_eq!(report.uncertain_count(), 1);
    }
}
