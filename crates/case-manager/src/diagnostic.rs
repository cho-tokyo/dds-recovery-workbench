//! Chunk 21 / 22.6: 診断入力 `DiagnosticInput` と
//! ファイルシステム破損事実 `FilesystemFindings`。
//!
//! Chunk 22.6 で症状判定型を完全排除した。Workbench は「事実報告者」
//! であり「判定者」ではないため、複合判定等の業務的に意味の薄い
//! 情報は出力しない。代わりに `FilesystemFindings` で「件数」「フラグ」のみ
//! 記録する。
//!
//! 業務的には CRM 貼り付け用テキスト生成や、お客様への進捗説明資料の元データになる。
//!
//! 全フィールドは `Option` または `Default` 可能で、空状態 (`DiagnosticInput::default()`)
//! が「未診断」を表現する。
//!
//! 関連 FR: FR-CASE-01 (案件単位管理), FR-DIAG-06 (事実ベースの報告)。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 案件の診断結果スナップショット。Chunk 22 で実データを書き込む。
///
/// 各フィールドが個別に Optional / 0 / 空文字なため、`Default` で「未診断」を表現する。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticInput {
    /// 診断実行日時。None なら未診断。
    pub diagnosed_at: Option<DateTime<Utc>>,
    /// 診断にかかった所要時間（秒）。
    pub duration_secs: Option<u64>,

    /// 検出されたファイルシステム種別（"NTFS" / "exFAT" / "FAT32" 等）。
    pub filesystem_type: Option<String>,

    /// ファイルシステムの破損事実（件数・フラグのみ、判定なし）。
    ///
    /// Chunk 22.6 で旧症状判定フィールドから置換。Workbench は事実だけ
    /// 提供し、業務判断 (CS の「これはフォーマット案件」等) は外部に委ねる。
    pub filesystem_findings: Option<FilesystemFindings>,

    /// 検出された総ファイル数（削除含む全体）。
    pub total_files: usize,
    /// 削除フラグが立っているファイル数。
    pub deleted_files: usize,
    /// 検出された総バイト数（参考値）。
    pub total_size_bytes: u64,

    /// 削除ファイルに関する集計統計。
    pub deleted_file_stats: Option<DeletedFileStats>,

    /// 診断担当者によるフリーテキスト備考（CRM 貼り付け用）。
    pub notes: String,
}

/// ファイルシステムの破損状態 (事実のみ、判定なし)。
///
/// Chunk 22.6 で導入。case.json 永続化用の slim 表現で、CRM 貼り付け用テキスト
/// 「【ファイルシステムの破損】」セクションに対応する。
///
/// `Default` は全フィールド 0 / false / 空のため `has_any_issue` は `signature_valid`
/// と `boot_sector_ok` が false 扱いとなり真。業務的な「正常」を表したい場合は
/// 明示的に `signature_valid: true, boot_sector_ok: true` を指定する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemFindings {
    /// NTFS シグネチャが有効か (true = 認識成功)。
    pub signature_valid: bool,
    /// 読み取りに失敗した MFT エントリ数。
    pub mft_corrupted_count: usize,
    /// 不正な run-list の検出件数。
    pub invalid_runlist_count: usize,
    /// Boot sector に異常なし。
    pub boot_sector_ok: bool,
    /// その他の異常 (説明文の Vec)。
    pub other_issues: Vec<String>,
}

impl FilesystemFindings {
    /// 何らかの異常があるか。
    ///
    /// 業務的な判定基準:
    /// - 署名が無効 → 異常
    /// - MFT エントリ破損 1 件以上 → 異常
    /// - 不正 run-list 1 件以上 → 異常
    /// - Boot sector NG → 異常
    /// - その他異常 1 件以上 → 異常
    pub fn has_any_issue(&self) -> bool {
        !self.signature_valid
            || self.mft_corrupted_count > 0
            || self.invalid_runlist_count > 0
            || !self.boot_sector_ok
            || !self.other_issues.is_empty()
    }
}

/// 削除ファイル群に関する集計統計（拡張子別 / フォルダ別の内訳）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeletedFileStats {
    /// 集計対象の削除ファイル総数。
    pub total_count: usize,
    /// 拡張子別ファイル数（小文字キー、ソート済み）。
    pub by_extension: BTreeMap<String, usize>,
    /// フォルダ別ファイル数（パス, 件数）の上位リスト。
    pub by_folder: Vec<(String, usize)>,
    /// 削除ファイルの推定合計バイト数。
    pub estimated_total_size: u64,

    /// 復旧可能性推定（Chunk 22.5 で埋まる）。
    pub recoverability_estimate: Option<RecoverabilityEstimate>,
}

/// 削除ファイルの復旧可能性推定（信頼度別件数）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoverabilityEstimate {
    /// 高信頼で復旧可能と推定された件数。
    pub high_confidence: usize,
    /// 中信頼で復旧可能と推定された件数。
    pub medium_confidence: usize,
    /// 低信頼（上書きリスク高）の件数。
    pub low_confidence: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 業務的に「正常な NTFS ボリューム」を表す典型値。
    fn healthy_findings() -> FilesystemFindings {
        FilesystemFindings {
            signature_valid: true,
            boot_sector_ok: true,
            ..Default::default()
        }
    }

    #[test]
    fn filesystem_findings_default_has_no_issues() {
        // 業務的に妥当な「正常」値で has_any_issue は false。
        let f = healthy_findings();
        assert!(!f.has_any_issue());
    }

    #[test]
    fn filesystem_findings_has_any_issue_detects_mft_corruption() {
        let f = FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 3,
            invalid_runlist_count: 0,
            boot_sector_ok: true,
            other_issues: vec![],
        };
        assert!(f.has_any_issue());
    }

    #[test]
    fn filesystem_findings_has_any_issue_detects_runlist_and_others() {
        let f = FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 0,
            invalid_runlist_count: 2,
            boot_sector_ok: true,
            other_issues: vec![],
        };
        assert!(f.has_any_issue());

        let g = FilesystemFindings {
            signature_valid: true,
            boot_sector_ok: true,
            other_issues: vec!["unknown attribute".into()],
            ..Default::default()
        };
        assert!(g.has_any_issue());

        let h = FilesystemFindings {
            signature_valid: true,
            boot_sector_ok: false,
            ..Default::default()
        };
        assert!(h.has_any_issue());
    }

    #[test]
    fn filesystem_findings_serializes_correctly() {
        // case.json 用 JSON ラウンドトリップ。
        let f = FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: 2,
            invalid_runlist_count: 1,
            boot_sector_ok: false,
            other_issues: vec!["foo".into(), "bar".into()],
        };
        let json = serde_json::to_string(&f).unwrap();
        let restored: FilesystemFindings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, f);
        assert!(json.contains("signature_valid"));
        assert!(json.contains("mft_corrupted_count"));
    }
}
