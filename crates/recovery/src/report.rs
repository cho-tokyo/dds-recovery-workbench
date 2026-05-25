//! Chunk 17 / Chunk 20.5: 復旧結果レポート。
//!
//! 復旧パイプラインが per-file で「成功 / 失敗 / スキップ」のいずれかを記録し、
//! 全体集計（成功率・所要時間・総書込バイト数）を返す。Chunk 19/20 のレポート生成
//! が `RecoveryReport` を受け取って HTML / CSV / DOCX 等を出力する設計。
//!
//! Chunk 20.5 で業務適用版に進化:
//! - `wish_labels` フィールドで顧客指定の Wish ラベルを保持
//! - 業務指標メソッド（`recovery_success_rate` / `quality_assurance_rate`）追加
//! - 形式別ブレイクダウン（`format_breakdown` → `FormatStats`）
//! - Invalid 理由別グルーピング（`invalid_grouped_by_reason`）
//! - `RecoveredEntry::matched_wish_labels` で CSV / レポートに顧客希望紐付けを出力

use chrono::{DateTime, Utc};
use dds_validators::{ValidationResult, ValidationStatus};
use std::collections::BTreeMap;
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
    /// Wishlist の各 `Wish::label` を順に格納（顧客向け表示用、Chunk 20.5）。
    pub wish_labels: Vec<String>,
}

impl RecoveryReport {
    /// 復旧成功率（パーセント、0.0〜100.0）。`total_matched == 0` のときは `0.0`。
    pub fn success_rate(&self) -> f64 {
        if self.total_matched == 0 {
            return 0.0;
        }
        (self.recovered.len() as f64) / (self.total_matched as f64) * 100.0
    }

    /// `success_rate` の業務適用版エイリアス（Chunk 20.5）。
    ///
    /// 「復旧成功率」= 復旧成功件数 / 該当ファイル数（パーセント）。
    pub fn recovery_success_rate(&self) -> f64 {
        self.success_rate()
    }

    /// 品質保証率（パーセント、Chunk 20.5）。
    ///
    /// = `validated_count` / 復旧成功件数 × 100。
    /// 復旧成功 0 件のときは `0.0`。
    pub fn quality_assurance_rate(&self) -> f64 {
        if self.recovered.is_empty() {
            return 0.0;
        }
        (self.validated_count() as f64) / (self.recovered.len() as f64) * 100.0
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

    /// 検証で `Uncertain` 判定されたファイル数（Chunk 18 / 20.5）。
    ///
    /// `validation` フィールドが `None` のもの（`validate_after_recovery=false`）も
    /// 「判定なし＝判定外」として `Uncertain` 同等にカウントする（Chunk 20.5 業務観点）。
    pub fn uncertain_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_uncertain())
                    .unwrap_or(true)
            })
            .count()
    }

    /// 形式別ブレイクダウン（Chunk 20.5）。
    ///
    /// 復旧ファイルを `format_detected` 別に集計し、各形式の Valid/Invalid/Uncertain
    /// 件数と合計を返す。`validation` が `None` のものは `"(検証なし)"` キーへ。
    /// `BTreeMap` のためキーアルファベット順で安定して並ぶ。
    pub fn format_breakdown(&self) -> BTreeMap<String, FormatStats> {
        let mut map: BTreeMap<String, FormatStats> = BTreeMap::new();

        for entry in &self.recovered {
            let Some(validation) = &entry.validation else {
                let stats = map.entry("(検証なし)".to_string()).or_default();
                stats.total += 1;
                stats.uncertain += 1;
                continue;
            };

            let format = validation
                .format_detected
                .clone()
                .unwrap_or_else(|| "(未検出)".to_string());
            let stats = map.entry(format).or_default();
            stats.total += 1;
            match validation.status {
                ValidationStatus::Valid => stats.valid += 1,
                ValidationStatus::Invalid => stats.invalid += 1,
                ValidationStatus::Uncertain => stats.uncertain += 1,
            }
        }
        map
    }

    // === Chunk 23.7: お客様優先データ統計（Wishlist マッチ分のみ） ===

    /// 優先データ（`is_priority = true`）の件数（Chunk 23.7）。
    ///
    /// 全件復旧 + Wishlist ラベリング設計（Chunk 23.7）の下で、
    /// 「お客様が特に希望されたデータ」の件数をレポート上で強調するために使う。
    pub fn priority_count(&self) -> usize {
        self.recovered.iter().filter(|e| e.is_priority).count()
    }

    /// 優先データ中の Valid 件数（Chunk 23.7）。
    pub fn priority_validated_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| e.is_priority)
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_valid())
                    .unwrap_or(false)
            })
            .count()
    }

    /// 優先データ中の Invalid 件数（Chunk 23.7）。
    pub fn priority_invalid_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| e.is_priority)
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_invalid())
                    .unwrap_or(false)
            })
            .count()
    }

    /// 優先データ中の Uncertain 件数（Chunk 23.7）。
    ///
    /// `uncertain_count` と同様、`validation` が `None` のものも Uncertain 扱い。
    pub fn priority_uncertain_count(&self) -> usize {
        self.recovered
            .iter()
            .filter(|e| e.is_priority)
            .filter(|e| {
                e.validation
                    .as_ref()
                    .map(|v| v.status.is_uncertain())
                    .unwrap_or(true)
            })
            .count()
    }

    /// 優先データの品質保証率（パーセント、Chunk 23.7）。
    ///
    /// = `priority_validated_count` / `priority_count` × 100。
    /// `priority_count == 0` のときは `0.0`（業務的に「優先データが 0 件なら品質保証率の概念は無意味」）。
    pub fn priority_quality_assurance_rate(&self) -> f64 {
        let count = self.priority_count();
        if count == 0 {
            return 0.0;
        }
        (self.priority_validated_count() as f64) / (count as f64) * 100.0
    }

    /// 優先データの合計書込バイト数（Chunk 23.7）。
    pub fn priority_total_bytes(&self) -> u64 {
        self.recovered
            .iter()
            .filter(|e| e.is_priority)
            .map(|e| e.bytes_written)
            .sum()
    }

    /// Invalid なファイルを「形式 + 主要顧客メッセージ冒頭」でグルーピング（Chunk 20.5）。
    ///
    /// 業務的に「PNG ヘッダー破損 N 件」「JPEG マジック不一致 N 件」のように
    /// CS が概観できる粒度のグルーピングを提供する。
    pub fn invalid_grouped_by_reason(&self) -> BTreeMap<String, Vec<&RecoveredEntry>> {
        let mut map: BTreeMap<String, Vec<&RecoveredEntry>> = BTreeMap::new();

        for entry in &self.recovered {
            let Some(v) = &entry.validation else {
                continue;
            };
            if !v.status.is_invalid() {
                continue;
            }

            let reason_key = match (&v.format_detected, &v.user_message_ja) {
                (Some(fmt), Some(msg)) => {
                    let summary: String = msg.chars().take(20).collect();
                    format!("{} - {}", fmt, summary)
                }
                (Some(fmt), None) => format!("{} - (詳細なし)", fmt),
                _ => "その他".to_string(),
            };
            map.entry(reason_key).or_default().push(entry);
        }
        map
    }
}

/// 形式別の Valid/Invalid/Uncertain 件数統計（Chunk 20.5）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FormatStats {
    /// Valid 件数。
    pub valid: usize,
    /// Invalid 件数。
    pub invalid: usize,
    /// Uncertain 件数。
    pub uncertain: usize,
    /// 合計件数。
    pub total: usize,
}

impl FormatStats {
    /// Valid 比率（パーセント、0.0〜100.0）。`total == 0` のときは `0.0`。
    pub fn valid_ratio(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.valid as f64) / (self.total as f64) * 100.0
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

    /// このファイルにマッチした `Wish::label` のリスト（Chunk 20.5）。
    /// CSV / レポートで「何の希望と紐付いたか」を示すために使う。
    pub matched_wish_labels: Vec<String>,

    /// このファイルが「お客様優先データ」か（Chunk 23.7）。
    ///
    /// Phase 1.5 Chunk 23.7 で導入。Wishlist にマッチしたファイルは `true`、
    /// それ以外（全件復旧で復旧されたが Wishlist にマッチしなかった user file）
    /// は `false`。レポート上で「お客様優先データ」セクションの集計対象になる。
    pub is_priority: bool,
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
            matched_wish_labels: Vec::new(),
            is_priority: false,
        }
    }

    fn build_report(recovered: Vec<RecoveredEntry>, total_matched: usize) -> RecoveryReport {
        let now = Utc::now();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched,
            recovered,
            failed: Vec::new(),
            skipped: Vec::new(),
            wish_labels: Vec::new(),
        }
    }

    #[test]
    fn success_rate_calculates_percentage() {
        let report = build_report((0..7).map(|_| build_recovered(0)).collect(), 10);
        assert!((report.success_rate() - 70.0).abs() < 0.01);
        // recovery_success_rate is the business-grade alias and matches.
        assert_eq!(report.success_rate(), report.recovery_success_rate());
    }

    #[test]
    fn success_rate_zero_when_no_matches() {
        let report = build_report(vec![], 0);
        assert_eq!(report.success_rate(), 0.0);
        assert_eq!(report.recovery_success_rate(), 0.0);
    }

    #[test]
    fn total_bytes_written_sums_all_recovered() {
        let report = build_report(
            vec![
                build_recovered(100),
                build_recovered(200),
                build_recovered(300),
            ],
            3,
        );
        assert_eq!(report.total_bytes_written(), 600);
    }

    #[test]
    fn validation_counts_classify_by_status() {
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
        let none_entry = build_recovered(40);

        let report = build_report(
            vec![valid_entry, invalid_entry, uncertain_entry, none_entry],
            4,
        );
        assert_eq!(report.validated_count(), 1);
        assert_eq!(report.invalid_count(), 1);
        // uncertain_count: explicit Uncertain (1) + validation=None (1) = 2 in Chunk 20.5 semantics.
        assert_eq!(report.uncertain_count(), 2);
    }

    #[test]
    fn recovery_success_rate_calculates_correctly() {
        // 14/15 -> ~93.3%
        let recovered: Vec<_> = (0..14).map(|_| build_recovered(0)).collect();
        let report = build_report(recovered, 15);
        assert!((report.recovery_success_rate() - 93.333).abs() < 0.1);
    }

    #[test]
    fn quality_assurance_rate_calculates_correctly() {
        // 10 Valid out of 14 recovered -> ~71.4%
        let mut entries: Vec<_> = Vec::new();
        for _ in 0..10 {
            let mut e = build_recovered(0);
            e.validation = Some(ValidationResult::valid("PNG", "png_v1", vec![], "ok", None));
            entries.push(e);
        }
        for _ in 0..4 {
            let mut e = build_recovered(0);
            e.validation = Some(ValidationResult::invalid(
                "PNG",
                "png_v1",
                "bad",
                "壊れています",
                "再復旧推奨",
            ));
            entries.push(e);
        }
        let report = build_report(entries, 14);
        assert!((report.quality_assurance_rate() - 71.428).abs() < 0.1);
    }

    #[test]
    fn format_breakdown_groups_by_format() {
        let mut entries = Vec::new();
        for _ in 0..3 {
            let mut e = build_recovered(0);
            e.validation = Some(ValidationResult::valid("PNG", "png_v1", vec![], "ok", None));
            entries.push(e);
        }
        let mut bad_png = build_recovered(0);
        bad_png.validation = Some(ValidationResult::invalid(
            "PNG", "png_v1", "x", "壊れ", "メモ",
        ));
        entries.push(bad_png);
        for _ in 0..2 {
            let mut e = build_recovered(0);
            e.validation = Some(ValidationResult::valid(
                "JPEG",
                "jpg_v1",
                vec![],
                "ok",
                None,
            ));
            entries.push(e);
        }
        let report = build_report(entries, 6);
        let bd = report.format_breakdown();
        let png = bd.get("PNG").expect("PNG present");
        assert_eq!(png.valid, 3);
        assert_eq!(png.invalid, 1);
        assert_eq!(png.total, 4);
        assert!((png.valid_ratio() - 75.0).abs() < 0.01);
        let jpeg = bd.get("JPEG").expect("JPEG present");
        assert_eq!(jpeg.valid, 2);
        assert_eq!(jpeg.total, 2);
    }

    // === Chunk 23.7: 優先データ統計テスト ===

    #[test]
    fn priority_count_only_counts_marked_entries() {
        // 全 5 件の recovered のうち、is_priority=true のもの 2 件だけ priority_count にカウント。
        let mut entries = Vec::new();
        for i in 0..5 {
            let mut e = build_recovered(i as u64);
            e.is_priority = i < 2; // 最初の 2 件のみ priority
            entries.push(e);
        }
        let report = build_report(entries, 5);
        assert_eq!(report.priority_count(), 2);
        assert_eq!(report.recovered.len(), 5);
    }

    #[test]
    fn priority_quality_assurance_rate_calculated_separately() {
        // 優先データのみ 4 件 (Valid 3, Invalid 1) → 品質保証率 75.0%
        // 非優先データは混在するが priority_quality_assurance_rate には影響しない。
        let mut entries = Vec::new();
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "ok", None);
        let v_ng = ValidationResult::invalid("PNG", "png_v1", "x", "壊れ", "メモ");
        for _ in 0..3 {
            let mut e = build_recovered(0);
            e.is_priority = true;
            e.validation = Some(v_ok.clone());
            entries.push(e);
        }
        let mut bad = build_recovered(0);
        bad.is_priority = true;
        bad.validation = Some(v_ng.clone());
        entries.push(bad);
        // 非優先データ：Invalid 5 件あっても priority_quality_assurance_rate には影響しない。
        for _ in 0..5 {
            let mut e = build_recovered(0);
            e.is_priority = false;
            e.validation = Some(v_ng.clone());
            entries.push(e);
        }
        let report = build_report(entries, 9);
        assert_eq!(report.priority_count(), 4);
        assert!((report.priority_quality_assurance_rate() - 75.0).abs() < 0.01);
        // 全体の品質保証率はもっと悪い (3/9 = 33.3%)
        assert!((report.quality_assurance_rate() - 33.333).abs() < 0.1);
    }

    #[test]
    fn report_can_compute_both_overall_and_priority_stats() {
        // 「全体 + 優先データ」の二重表示が成立する：両方の集計が同時に取れる。
        let mut entries = Vec::new();
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "ok", None);
        // 優先データ 2 件 (Valid)
        for _ in 0..2 {
            let mut e = build_recovered(100);
            e.is_priority = true;
            e.validation = Some(v_ok.clone());
            entries.push(e);
        }
        // 非優先データ 3 件 (Valid)
        for _ in 0..3 {
            let mut e = build_recovered(50);
            e.is_priority = false;
            e.validation = Some(v_ok.clone());
            entries.push(e);
        }
        let report = build_report(entries, 5);
        // 全体集計
        assert_eq!(report.recovered.len(), 5);
        assert_eq!(report.validated_count(), 5);
        assert_eq!(report.total_bytes_written(), 100 * 2 + 50 * 3);
        // 優先データ集計
        assert_eq!(report.priority_count(), 2);
        assert_eq!(report.priority_validated_count(), 2);
        assert_eq!(report.priority_invalid_count(), 0);
        assert_eq!(report.priority_uncertain_count(), 0);
        assert_eq!(report.priority_total_bytes(), 200);
        assert!((report.priority_quality_assurance_rate() - 100.0).abs() < 0.01);
    }

    #[test]
    fn invalid_grouped_by_reason_separates_distinct_reasons() {
        let mut tail_missing = build_recovered(0);
        tail_missing.validation = Some(ValidationResult::invalid(
            "PNG",
            "png_v1",
            "trailer missing",
            "PNG 画像の末尾が欠けています",
            "IEND チャンク欠損",
        ));
        let mut wrong_ext = build_recovered(0);
        wrong_ext.validation = Some(ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG 拡張子ですが内容が違います",
            "拡張子嘘の典型例",
        ));
        let report = build_report(vec![tail_missing, wrong_ext], 2);
        let grouped = report.invalid_grouped_by_reason();
        assert_eq!(grouped.len(), 2, "different reasons -> distinct groups");
    }
}
