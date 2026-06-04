//! Chunk 21 / 22.6 / 24d-4-1: 診断入力 `DiagnosticInput` と
//! ファイルシステム破損事実 `FilesystemFindings`、および業務的診断指標。
//!
//! Chunk 22.6 で症状判定型を完全排除した。Workbench は「事実報告者」
//! であり「判定者」ではないため、複合判定等の業務的に意味の薄い
//! 情報は出力しない。代わりに `FilesystemFindings` で「件数」「フラグ」のみ
//! 記録する。
//!
//! Chunk 24d-4-1 で「業務的診断指標」型 ([`DirtyBitStatus`] / [`LogFileStatus`] /
//! [`BitLockerStatus`] / [`FileEstimation`] / [`RecoveryDifficulty`] /
//! [`SuccessRatePrediction`]) を追加。これらは `dds-diagnostic` 側で
//! 計算され、`DiagnosticInput` 経由で case.json に永続化される。
//! 型をここ (case-manager) に置く理由は `DiagnosticInput` が保有するフィールド
//! 型は case-manager の責務であり、`dds-diagnostic → dds-case-manager`
//! の既存依存方向と整合するため。
//!
//! 業務的には CRM 貼り付け用テキスト生成や、お客様への進捗説明資料の元データになる。
//!
//! 全フィールドは `Option` または `Default` 可能で、空状態 (`DiagnosticInput::default()`)
//! が「未診断」を表現する。
//!
//! 関連 FR: FR-CASE-01 (案件単位管理), FR-DIAG-04 ~ FR-DIAG-07 (業務的診断指標)。

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

    // --- Chunk 24d-4-1: 業務的診断指標 ---
    // すべて `Option<T>` で `#[serde(default)]` 相当 (Default::default で None)。
    // 旧 case.json 互換性のため、未指定なら None に復元される。
    /// NTFS の Dirty Bit 状態 (Windows がマウント拒否する主因)。
    #[serde(default)]
    pub dirty_bit: Option<DirtyBitStatus>,
    /// $LogFile の整合性状態 (未完了トランザクションの有無)。
    #[serde(default)]
    pub log_file_status: Option<LogFileStatus>,
    /// BitLocker 暗号化の状態。
    #[serde(default)]
    pub bitlocker: Option<BitLockerStatus>,
    /// ファイル数の推定 (MFT ベース概算)。
    #[serde(default)]
    pub file_estimation: Option<FileEstimation>,
    /// 復旧難易度の評価 (易/中/難/注意)。
    #[serde(default)]
    pub recovery_difficulty: Option<RecoveryDifficulty>,
    /// 復旧成功率の予測 (全体 + 優先データ)。
    #[serde(default)]
    pub success_rate: Option<SuccessRatePrediction>,
}

/// Chunk 24d-4-1: NTFS の Dirty Bit 状態。
///
/// `$Volume` MFT エントリ (インデックス 3) の `$VOLUME_INFORMATION` 属性 (タイプ 0x70) に
/// フラグが立っていると Windows はマウントを拒否し chkdsk を要求する。業務的に
/// Windows がマウントを拒否する原因の最多であり、本指標が「Dirty」だと
/// 営業はお客様に「Windows でアクセスできない原因が判明しました」と説明できる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirtyBitStatus {
    /// 正常 (Dirty Bit なし)。
    Clean,
    /// Dirty Bit が立っている (Windows が chkdsk を要求)。
    Dirty,
    /// 判定不能 ($Volume が読めない、属性が見つからない等)。
    Unknown,
}

impl DirtyBitStatus {
    /// 業務的な日本語メッセージ。
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::Clean => "正常",
            Self::Dirty => "立っている (Windows がマウント拒否する原因)",
            Self::Unknown => "判定不能",
        }
    }
}

/// Chunk 24d-4-1: NTFS `$LogFile` の整合性状態 (簡易判定)。
///
/// `$LogFile` は NTFS のトランザクションログ。未完了トランザクションが残ると
/// Windows がマウント前に再生を試みる。Phase 1.5 では先頭マジック値のみ判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFileStatus {
    /// 正常 (整合性 OK)。
    Consistent,
    /// 未完了トランザクションあり。
    Inconsistent,
    /// 判定不能。
    Unknown,
}

impl LogFileStatus {
    /// 業務的な日本語メッセージ。
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::Consistent => "正常",
            Self::Inconsistent => "不整合あり (未完了トランザクション)",
            Self::Unknown => "判定不能",
        }
    }
}

/// Chunk 24d-4-1: BitLocker 暗号化の状態。
///
/// 業務的に BitLocker 暗号化の検出は復旧難易度に大きな影響を与える。
/// 「受注不可」と決めつけず「回復キーが必要」という事実を伝え、判断は人間に委ねる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitLockerStatus {
    /// 暗号化なし (通常)。
    NotEncrypted,
    /// BitLocker で暗号化されている。
    Encrypted,
    /// 判定不能。
    Unknown,
}

impl BitLockerStatus {
    /// 業務的な日本語メッセージ。
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::NotEncrypted => "なし",
            Self::Encrypted => "BitLocker 暗号化を検出 (回復キーが必要)",
            Self::Unknown => "判定不能",
        }
    }
}

/// Chunk 24d-4-1: MFT 走査ベースのファイル数推定。
///
/// `dds-diagnostic` の aggregator が既に持つ `FileStatistics` から派生して
/// 生成される。CRM テキスト / 営業見積で使う「概算ファイル数」の根拠。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEstimation {
    /// 推定総ファイル数 (システムメタファイル除く)。
    pub estimated_total_files: u64,
    /// 推定削除ファイル数。
    pub estimated_deleted_files: u64,
    /// 推定生存ファイル数。
    pub estimated_live_files: u64,
}

impl FileEstimation {
    /// 営業向けの業務的サマリ文字列。
    pub fn business_summary(&self) -> String {
        format!(
            "推定ファイル数: 約 {} 件 (生存 {} / 削除 {})",
            format_estimation_number(self.estimated_total_files),
            format_estimation_number(self.estimated_live_files),
            format_estimation_number(self.estimated_deleted_files),
        )
    }
}

/// 数値を業務向け短縮表記にする (1,500 / 2.5万 等)。
///
/// 公開しているのは CLI 表示など他レイヤから同一書式で再利用するため。
pub fn format_estimation_number(n: u64) -> String {
    if n >= 10_000 {
        format!("{:.1}万", n as f64 / 10_000.0)
    } else if n >= 1_000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

/// Chunk 24d-4-1: 復旧難易度 (4 段階)。
///
/// 業務原則:
/// - 「受注不可」「対応困難」のような決めつけ表現は使わない
/// - 「注意」は物理障害の兆候を示し、受注可否は人間が判断する
/// - 完全な FS 構造破壊もファイル単位の復旧で可能なため「難」扱い (決して「不可」ではない)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryDifficulty {
    /// 易: 標準的な業務ケース、復旧成功の見込み高い。
    Easy,
    /// 中: 部分的な障害あり、業務的に標準範囲。
    Medium,
    /// 難: 大規模な障害、ファイル単位の復旧が必要、業務的に難度高。
    Hard,
    /// 注意: 物理障害の兆候あり、業務的に慎重判断が必要 (受注可否は人間が判断)。
    Caution,
}

impl RecoveryDifficulty {
    /// 業務的な短縮表示名 (CLI / CRM)。
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Easy => "易",
            Self::Medium => "中",
            Self::Hard => "難",
            Self::Caution => "注意",
        }
    }

    /// 業務的な日本語の説明。`Caution` には必ず「人間が判断」を含める (テスト要件)。
    pub fn business_explanation(&self) -> &'static str {
        match self {
            Self::Easy => "標準的な業務ケース、復旧成功の見込み高い",
            Self::Medium => "部分的な障害あり、業務的に標準範囲",
            Self::Hard => "大規模な障害、ファイル単位の復旧が必要、業務的に難度高",
            Self::Caution => "物理障害の兆候あり、業務的に慎重判断が必要 (受注可否は人間が判断)",
        }
    }
}

/// Chunk 24d-4-1: 復旧成功率の予測。
///
/// 全体成功率 + (Wishlist 指定時の) 優先データ成功率 + 計算根拠リスト。
/// 営業がお客様に「なぜこの数字なのか」を説明できるよう `reasoning` を提供する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuccessRatePrediction {
    /// 全体的な復旧成功率 (0-100)。
    pub overall_rate: u8,
    /// 優先データの復旧成功率 (Wishlist 指定時のみ、0-100)。
    pub priority_rate: Option<u8>,
    /// 計算根拠 (営業の説明用、減点要因の日本語リスト)。
    pub reasoning: Vec<String>,
}

impl SuccessRatePrediction {
    /// 営業向けの業務的サマリ文字列。
    pub fn business_summary(&self) -> String {
        let mut s = format!("推定復旧成功率: {}% (全体)", self.overall_rate);
        if let Some(priority) = self.priority_rate {
            s.push_str(&format!("、{}% (優先データ)", priority));
        }
        s
    }
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

    // --- Chunk 24d-4-1: 業務的診断指標型のテスト ---

    #[test]
    fn business_diagnostic_types_default_to_none_in_diagnostic_input() {
        let d = DiagnosticInput::default();
        assert!(d.dirty_bit.is_none());
        assert!(d.log_file_status.is_none());
        assert!(d.bitlocker.is_none());
        assert!(d.file_estimation.is_none());
        assert!(d.recovery_difficulty.is_none());
        assert!(d.success_rate.is_none());
    }

    #[test]
    fn diagnostic_input_legacy_json_without_business_fields_deserializes() {
        // 旧 case.json (24d-3 以前) は新規フィールドを持たないが、
        // #[serde(default)] により None で復元できることを業務的に保証する。
        let legacy_json = r#"{
            "diagnosed_at": null,
            "duration_secs": null,
            "filesystem_type": "NTFS",
            "filesystem_findings": null,
            "total_files": 10,
            "deleted_files": 2,
            "total_size_bytes": 1024,
            "deleted_file_stats": null,
            "notes": ""
        }"#;
        let d: DiagnosticInput = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(d.total_files, 10);
        assert!(d.dirty_bit.is_none());
        assert!(d.recovery_difficulty.is_none());
    }

    #[test]
    fn format_estimation_number_short_thousand_and_man() {
        assert_eq!(format_estimation_number(500), "500");
        assert_eq!(format_estimation_number(1500), "1,500");
        assert_eq!(format_estimation_number(25_000), "2.5万");
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
