//! Chunk 21: 案件本体 `Case` と復旧結果サマリ `RecoveryReportSummary`。
//!
//! 1 案件 = 1 つの `Case` インスタンス = 1 つの `case.json` ファイル。
//!
//! 業務フロー:
//! 1. CRM から案件番号を受領 → `Case::new(case_id)` で初期化
//! 2. 診断エンジンが `diagnostic_input` を埋める (Chunk 22)
//! 3. お客様ヒアリングで `wishlist` を埋める
//! 4. 復旧パイプラインが `recovery_report_summary` と `output_dir` を埋める
//! 5. `CaseStorage::save` で逐次永続化（各ステップで途中保存可能）
//!
//! 関連 FR: FR-CASE-01 (案件単位での業務情報管理),
//!         FR-CASE-04 (1 PC 1 案件専有の業務フロー対応)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use dds_wish_match::Wishlist;

use crate::case_id::CaseId;
use crate::diagnostic::DiagnosticInput;

/// 案件全体の業務情報。
///
/// 各フィールドは案件ライフサイクル中の異なるタイミングで埋められる。
/// `Option<T>` は「まだ埋まっていない」を意味し、JSON 上は `null` になる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    /// 案件番号 (yymmdd-NN)。CRM が採番。
    pub case_id: CaseId,

    /// 案件作成日時（Workbench 上での `Case::new` 呼び出し時刻）。
    pub created_at: DateTime<Utc>,
    /// 最終更新日時。`CaseStorage::save` で都度自動更新される。
    pub updated_at: DateTime<Utc>,

    /// 診断結果。未診断時は `DiagnosticInput::default()`（全フィールド空）。
    #[serde(default)]
    pub diagnostic_input: DiagnosticInput,

    /// お客様希望リスト。ヒアリング前は `None`。
    pub wishlist: Option<Wishlist>,

    /// 復旧結果サマリ。復旧未実施なら `None`。
    pub recovery_report_summary: Option<RecoveryReportSummary>,

    /// 復旧データの出力先ディレクトリ（例: `G:\260522-04`）。未設定なら `None`。
    pub output_dir: Option<PathBuf>,
}

impl Case {
    /// 新規案件を生成。`created_at` と `updated_at` は現在時刻で初期化される。
    pub fn new(case_id: CaseId) -> Self {
        let now = Utc::now();
        Self {
            case_id,
            created_at: now,
            updated_at: now,
            diagnostic_input: DiagnosticInput::default(),
            wishlist: None,
            recovery_report_summary: None,
            output_dir: None,
        }
    }
}

/// 復旧パイプライン実行後のサマリ統計（レポート生成・お客様報告用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryReportSummary {
    /// 復旧開始日時。
    pub started_at: DateTime<Utc>,
    /// 復旧完了日時。
    pub finished_at: DateTime<Utc>,
    /// 所要時間（ミリ秒）。
    pub duration_ms: i64,

    /// 希望リストにマッチした候補総数。
    pub total_matched: usize,
    /// 復旧成功（書き出し成功）件数。
    pub recovered_count: usize,
    /// 復旧失敗件数。
    pub failed_count: usize,
    /// スキップ件数（重複、サイズ 0 等）。
    pub skipped_count: usize,

    /// バリデーション合格件数。
    pub validated_count: usize,
    /// バリデーション失敗件数。
    pub invalid_count: usize,
    /// バリデーション結果不明件数。
    pub uncertain_count: usize,

    /// 書き出し合計バイト数。
    pub total_bytes_written: u64,

    /// 復旧成功率（0.0〜1.0）= recovered_count / total_matched。
    pub recovery_success_rate: f64,
    /// 品質保証率（0.0〜1.0）= validated_count / recovered_count。
    pub quality_assurance_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticInput;

    #[test]
    fn case_new_initializes_with_defaults() {
        let id = CaseId::parse("260522-04").unwrap();
        let case = Case::new(id.clone());
        assert_eq!(case.case_id, id);
        assert_eq!(case.created_at, case.updated_at);
        assert!(case.wishlist.is_none());
        assert!(case.recovery_report_summary.is_none());
        assert!(case.output_dir.is_none());
        assert_eq!(case.diagnostic_input.total_files, 0);
    }

    #[test]
    fn case_roundtrip_preserves_all_fields() {
        let id = CaseId::parse("260601-12").unwrap();
        let mut case = Case::new(id);
        case.diagnostic_input.filesystem_type = Some("NTFS".into());
        case.diagnostic_input.notes = "テスト備考".into();
        case.output_dir = Some(PathBuf::from("G:\\260601-12"));

        let json = serde_json::to_string(&case).unwrap();
        let restored: Case = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.case_id, case.case_id);
        assert_eq!(
            restored.diagnostic_input.filesystem_type,
            Some("NTFS".into())
        );
        assert_eq!(restored.diagnostic_input.notes, "テスト備考");
        assert_eq!(restored.output_dir, Some(PathBuf::from("G:\\260601-12")));
    }

    #[test]
    fn diagnostic_input_default_has_empty_stats() {
        let d = DiagnosticInput::default();
        assert!(d.diagnosed_at.is_none());
        assert!(d.duration_secs.is_none());
        assert!(d.filesystem_type.is_none());
        assert!(d.symptom.is_none());
        assert_eq!(d.total_files, 0);
        assert_eq!(d.deleted_files, 0);
        assert_eq!(d.total_size_bytes, 0);
        assert!(d.deleted_file_stats.is_none());
        assert_eq!(d.notes, "");
    }
}
