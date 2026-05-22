//! Chunk 21: 診断入力 `DiagnosticInput` の placeholder。
//!
//! Chunk 22 で診断エンジンと連携して中身を埋める「器」を先に定義する。
//! 業務的には CRM 貼り付け用テキスト生成や、お客様への進捗説明資料の元データになる。
//!
//! 全フィールドは `Option` または `Default` 可能で、空状態 (`DiagnosticInput::default()`)
//! が「未診断」を表現する。`Option<DiagnosticInput>` ではなく `DiagnosticInput` を直接
//! 持つことで、JSON 構造をシンプルに保つ。
//!
//! 関連 FR: FR-CASE-01 (案件単位管理), Chunk 22 への布石。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::symptom::Symptom;

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
    /// 検出された主症状。
    pub symptom: Option<Symptom>,

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
