//! Chunk 22 / 22.6: 診断レポート構造体群。
//!
//! - [`DiagnosticReport`]: in-memory のフル診断結果（CRM テキスト生成・JSON 出力に利用）。
//! - [`HardwareInfo`] / [`FilesystemInfo`] / [`FileStatistics`] / [`FormatCount`] /
//!   [`FolderCount`] / [`FsAnomalyReport`]: それぞれ階層的なサマリ構造体。
//!
//! Chunk 22.6 で旧症状判定フィールドを `filesystem_findings: FilesystemFindings`
//! に置換。`FsAnomalyReport` (詳細 in-memory 用) と `FilesystemFindings` (slim 永続化用)
//! の両方を保持し、`FsAnomalyReport::to_findings()` で変換する。
//!
//! `DiagnosticReport::to_diagnostic_input()` で case.json 用 slim 版（[`DiagnosticInput`]）
//! に変換し、`to_crm_text()` で CRM 貼り付け用テキストを生成する。
//!
//! 関連 FR: FR-DIAG-01 (NTFS 論理診断), FR-DIAG-03 (削除ファイル統計),
//!         FR-DIAG-04 (CRM 貼り付けテキスト), FR-DIAG-06 (事実ベースの報告)。

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use dds_case_manager::{
    BitLockerStatus, CaseId, DeletedFileStats, DiagnosticInput, DirtyBitStatus, FileEstimation,
    FilesystemFindings, LogFileStatus, RecoveryDifficulty, SuccessRatePrediction,
};

/// 診断結果のフル構造体。in-memory で全情報を保持。
///
/// case.json への永続化は [`Self::to_diagnostic_input`] で slim 版に縮小、
/// CRM 貼り付け用テキストは [`Self::to_crm_text`] で生成する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// 案件番号（yymmdd-NN）。
    pub case_id: CaseId,
    /// 診断開始時刻。
    pub diagnosed_at: DateTime<Utc>,
    /// 診断所要時間（秒）。
    pub duration_secs: u64,

    /// ハードウェア情報（モデル・シリアル・容量）。Phase 1.5 では一部 `None`。
    pub hardware: HardwareInfo,
    /// ファイルシステム情報（種別・シリアル・クラスタサイズ等）。
    pub filesystem: FilesystemInfo,
    /// ファイルシステムの破損事実 (件数・フラグのみ、判定なし)。
    ///
    /// Chunk 22.6 で `symptom` フィールドから置換。CRM テキスト
    /// 「【ファイルシステムの破損】」セクションのソース。
    pub filesystem_findings: FilesystemFindings,

    /// 全ファイル統計（合計件数・生存/削除内訳・ディレクトリ数・総バイト）。
    pub file_stats: FileStatistics,
    /// 拡張子別の件数とサイズ（小文字キーで集計済み）。
    pub format_breakdown: BTreeMap<String, FormatCount>,
    /// 件数順 上位 10 フォルダ。
    pub folder_breakdown: Vec<FolderCount>,

    /// 削除ファイルの集計統計（削除 0 件時は `None`）。
    pub deleted_file_stats: Option<DeletedFileStats>,
    /// 検出された FS 異常レポート（in-memory 用の詳細情報）。
    pub anomalies: FsAnomalyReport,

    // --- Chunk 24d-4-1: 業務的診断指標 ---
    /// NTFS Dirty Bit 状態 (Windows がマウント拒否する主因)。
    #[serde(default)]
    pub dirty_bit: Option<DirtyBitStatus>,
    /// $LogFile 整合性状態。
    #[serde(default)]
    pub log_file_status: Option<LogFileStatus>,
    /// BitLocker 暗号化の状態。
    #[serde(default)]
    pub bitlocker: Option<BitLockerStatus>,
    /// ファイル数の推定 (MFT ベース概算)。
    #[serde(default)]
    pub file_estimation: Option<FileEstimation>,
    /// 復旧難易度 (易/中/難/注意)。
    #[serde(default)]
    pub recovery_difficulty: Option<RecoveryDifficulty>,
    /// 復旧成功率予測 (全体 + 優先データ + 計算根拠)。
    #[serde(default)]
    pub success_rate: Option<SuccessRatePrediction>,
}

/// ハードウェア（HDD/SSD）情報。
///
/// Phase 1.5 ではモデル名・シリアルの自動取得を行わないため、初期値は `None`。
/// Phase 2 で Windows API 経由の SMART 情報取得で埋める想定。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// HDD のモデル名（例: `"WDC WD20EZRX-00DC0B0"`）。Phase 1.5 では `None`。
    pub model: Option<String>,
    /// HDD のハードウェアシリアル（例: `"WD-WCC4N1234567"`）。Phase 1.5 では `None`。
    pub serial: Option<String>,
    /// パーティションサイズ（バイト）。`FilesystemInfo` から推定。
    pub size_bytes: u64,
}

/// ファイルシステム基本情報（NTFS 前提、Phase 1.5）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemInfo {
    /// ファイルシステム種別（"NTFS" / "exFAT" / "FAT32"）。
    pub fs_type: String,
    /// ボリュームシリアル番号（16 進大文字、例: `"A1B2C3D4"`）。
    pub volume_serial: Option<String>,
    /// クラスタサイズ（バイト）。
    pub cluster_size_bytes: u32,
    /// 総クラスタ数（パーティション全体）。
    pub total_clusters: u64,
    /// 使用クラスタ数。Phase 1.5 では `$Bitmap` 未対応のため 0 でフォールバック可。
    pub used_clusters: u64,
}

/// 全ファイル統計（生存 / 削除 / ディレクトリの内訳）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileStatistics {
    /// 全ユーザファイル数（生存 + 削除、ディレクトリ含む）。
    pub total_files: usize,
    /// 通常（生存）ファイル数（ディレクトリ含む）。
    pub live_files: usize,
    /// 削除フラグが立ったファイル数（ディレクトリ含む）。
    pub deleted_files: usize,
    /// ディレクトリ数（生存 + 削除合算）。
    pub directories: usize,
    /// 全ファイルの合計バイト数（参考値）。
    pub total_size_bytes: u64,
}

/// 拡張子別ブレイクダウンの 1 エントリ。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormatCount {
    /// この拡張子のファイル件数。
    pub count: usize,
    /// この拡張子の合計バイト数。
    pub total_size_bytes: u64,
}

/// フォルダ別ブレイクダウンの 1 エントリ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCount {
    /// フォルダパス（NTFS 形式、例: `"\Users\Chou"`）。
    pub path: String,
    /// このフォルダ直下のファイル件数（生存 + 削除合算）。
    pub file_count: usize,
    /// このフォルダ直下のファイル合計バイト数。
    pub total_size_bytes: u64,
}

/// FS 異常レポート（カテゴリ別件数 + 自由記述）。
///
/// 個別 MFT エントリのパースエラーは aggregator がカテゴリ振り分けして
/// ここに加算する。in-memory 詳細情報用で、永続化 slim 版は
/// [`FilesystemFindings`] に [`Self::to_findings`] で変換する。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FsAnomalyReport {
    /// MFT エントリ読み取りで失敗した件数（構造的破損の指標）。
    pub mft_corrupted_count: usize,
    /// run-list 解析失敗の件数（データクラスタ参照異常）。
    pub invalid_runlist_count: usize,
    /// Boot sector の異常（backup と不一致など、Phase 2 で詳細化）。
    pub boot_sector_issues: Vec<String>,
    /// 上記に分類されないその他の異常メッセージ。
    pub other_issues: Vec<String>,
}

impl FsAnomalyReport {
    /// 1 つでも異常があれば `true`。
    ///
    /// CRM テキストの「【ファイルシステムの破損】」セクションで補足表示の有無を
    /// 切り替えるために使う。Chunk 22.6 で症状判定から切り離された。
    pub fn has_any_anomaly(&self) -> bool {
        self.mft_corrupted_count > 0
            || self.invalid_runlist_count > 0
            || !self.boot_sector_issues.is_empty()
            || !self.other_issues.is_empty()
    }

    /// case.json 永続化用の slim 版 [`FilesystemFindings`] に変換する。
    ///
    /// 業務的仮定:
    /// - `signature_valid: true` (現状 MFT 読み取り成功 = ここまで到達できたので必ず true)
    /// - `boot_sector_ok: self.boot_sector_issues.is_empty()`
    /// - 件数系・other_issues は素直にコピー
    pub fn to_findings(&self) -> FilesystemFindings {
        FilesystemFindings {
            signature_valid: true,
            mft_corrupted_count: self.mft_corrupted_count,
            invalid_runlist_count: self.invalid_runlist_count,
            boot_sector_ok: self.boot_sector_issues.is_empty(),
            other_issues: self.other_issues.clone(),
        }
    }
}

impl DiagnosticReport {
    /// case.json 永続化用の slim 版 [`DiagnosticInput`] に変換する。
    ///
    /// 業務的に必要なフィールドのみ抽出（メモリ常駐の format_breakdown 等は除外）。
    /// Chunk 22.6 で `symptom` フィールドが `filesystem_findings` に置換され、
    /// Chunk 24d-4-1 で業務的診断指標 (Dirty Bit / $LogFile / BitLocker /
    /// ファイル数推定 / 難易度 / 成功率) が追加された。
    pub fn to_diagnostic_input(&self) -> DiagnosticInput {
        DiagnosticInput {
            diagnosed_at: Some(self.diagnosed_at),
            duration_secs: Some(self.duration_secs),
            filesystem_type: Some(self.filesystem.fs_type.clone()),
            filesystem_findings: Some(self.filesystem_findings.clone()),
            total_files: self.file_stats.total_files,
            deleted_files: self.file_stats.deleted_files,
            total_size_bytes: self.file_stats.total_size_bytes,
            deleted_file_stats: self.deleted_file_stats.clone(),
            notes: String::new(),
            dirty_bit: self.dirty_bit,
            log_file_status: self.log_file_status,
            bitlocker: self.bitlocker,
            file_estimation: self.file_estimation.clone(),
            recovery_difficulty: self.recovery_difficulty,
            success_rate: self.success_rate.clone(),
        }
    }

    /// CRM 貼り付け用業務テキストを生成する（[`crate::crm_text::render`] へ委譲）。
    pub fn to_crm_text(&self) -> String {
        crate::crm_text::render(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_anomaly_report_default_has_no_anomaly() {
        let r = FsAnomalyReport::default();
        assert!(!r.has_any_anomaly());
        let findings = r.to_findings();
        assert!(findings.signature_valid);
        assert!(findings.boot_sector_ok);
        assert_eq!(findings.mft_corrupted_count, 0);
        assert!(!findings.has_any_issue());
    }

    #[test]
    fn fs_anomaly_report_reports_mft_and_runlist() {
        let r = FsAnomalyReport {
            mft_corrupted_count: 3,
            invalid_runlist_count: 1,
            ..Default::default()
        };
        assert!(r.has_any_anomaly());
        let findings = r.to_findings();
        assert_eq!(findings.mft_corrupted_count, 3);
        assert_eq!(findings.invalid_runlist_count, 1);
        assert!(findings.has_any_issue());
    }

    #[test]
    fn fs_anomaly_report_to_findings_marks_boot_sector_ng() {
        let r = FsAnomalyReport {
            boot_sector_issues: vec!["magic mismatch".into()],
            ..Default::default()
        };
        let findings = r.to_findings();
        assert!(!findings.boot_sector_ok);
        assert!(findings.has_any_issue());
    }

    #[test]
    fn to_diagnostic_input_includes_filesystem_findings() {
        let report = DiagnosticReport {
            case_id: CaseId::parse("260522-04").unwrap(),
            diagnosed_at: Utc::now(),
            duration_secs: 12,
            hardware: HardwareInfo::default(),
            filesystem: FilesystemInfo {
                fs_type: "NTFS".into(),
                volume_serial: Some("A1B2C3D4".into()),
                cluster_size_bytes: 4096,
                total_clusters: 1000,
                used_clusters: 250,
            },
            filesystem_findings: FilesystemFindings {
                signature_valid: true,
                boot_sector_ok: true,
                ..Default::default()
            },
            file_stats: FileStatistics {
                total_files: 30,
                live_files: 25,
                deleted_files: 5,
                directories: 0,
                total_size_bytes: 1500,
            },
            format_breakdown: BTreeMap::new(),
            folder_breakdown: vec![],
            deleted_file_stats: None,
            anomalies: FsAnomalyReport::default(),
            dirty_bit: None,
            log_file_status: None,
            bitlocker: None,
            file_estimation: None,
            recovery_difficulty: None,
            success_rate: None,
        };
        let input = report.to_diagnostic_input();
        assert_eq!(input.filesystem_type, Some("NTFS".into()));
        let findings = input.filesystem_findings.expect("findings present");
        assert!(findings.signature_valid);
        assert!(findings.boot_sector_ok);
        assert_eq!(input.total_files, 30);
        assert_eq!(input.deleted_files, 5);
        assert_eq!(input.total_size_bytes, 1500);
        assert_eq!(input.duration_secs, Some(12));
    }
}
