//! # dds-core
//!
//! DDS Recovery Workbench の共通基盤クレート。全クレートが依存する基本型・
//! エラー型を提供します。関連 FR: 設計基盤（全 FR の前提）。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod format;

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// DDS Recovery Workbench 共通のエラー型。
///
/// 各クレートのエラーはこの型に集約するか、変換可能な独自エラー型を用意します。
/// `#[error]` のメッセージは CS 担当者が閲覧する可能性があるため日本語かつ平易な表現とします。
#[derive(Debug, Error)]
pub enum CoreError {
    /// I/O エラー（ディスク読込・ファイル操作など）。
    #[error("I/O エラーが発生しました: {0}")]
    Io(#[from] std::io::Error),

    /// バイナリ・構造体のパース失敗。
    #[error("パースに失敗しました ({context}): {reason}")]
    Parse {
        /// パースが行われていた文脈（例: "NTFS boot sector"）。
        context: String,
        /// 失敗の理由。
        reason: String,
    },

    /// 関数に渡された引数が不正。
    #[error("不正な引数です: {0}")]
    InvalidArgument(String),

    /// 値が許容範囲外。`value` と `max` は必ずメッセージに含めます。
    #[error("範囲外の値です: {what} = {value} (最大 {max})")]
    OutOfRange {
        /// 対象の項目名。
        what: String,
        /// 与えられた値。
        value: u64,
        /// 許容上限。
        max: u64,
    },

    /// 現状未対応の機能・形式。
    #[error("未対応の機能です: {0}")]
    Unsupported(String),

    /// 想定外の内部エラー（バグの可能性が高い）。
    #[error("内部エラー: {0}")]
    Internal(String),
}

/// `CoreError` を用いた結果型エイリアス。
pub type CoreResult<T> = Result<T, CoreError>;

/// PRD で定義される損傷レベル（L1〜L6 + 物理障害）。
///
/// 仕様書の表記に合わせ、バリアント名は `L1_DeletionOnly` 形式を許容します。
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageLevel {
    /// L1: 削除のみ（FS メタは健全）。
    L1_DeletionOnly,
    /// L2: パーティションテーブル損傷。
    L2_PartitionTableDamaged,
    /// L3: FS メタデータ部分損傷。
    L3_FsMetadataPartiallyDamaged,
    /// L4: パーティション + FS 両損傷（Phase 2 以降）。
    L4_BothDamaged,
    /// L5: FS メタデータ消失（フルフォーマット相当、Phase 2 以降）。
    L5_FsMetadataLost,
    /// L6: 重度損傷（Phase 2 以降）。
    L6_SevereDamage,
    /// 物理障害（ヘッド不良・基板故障等、ソフト復旧範囲外）。
    PhysicalIssue,
}

impl DamageLevel {
    /// 日本語ラベルを返します。レポート出力・UI 表示用。
    pub fn display_ja(&self) -> &'static str {
        match self {
            Self::L1_DeletionOnly => "L1: 削除のみ",
            Self::L2_PartitionTableDamaged => "L2: パーティションテーブル損傷",
            Self::L3_FsMetadataPartiallyDamaged => "L3: FSメタデータ部分損傷",
            Self::L4_BothDamaged => "L4: パーティション・FS両損傷",
            Self::L5_FsMetadataLost => "L5: FSメタデータ消失",
            Self::L6_SevereDamage => "L6: 重度損傷",
            Self::PhysicalIssue => "物理障害",
        }
    }
}

impl fmt::Display for DamageLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_ja())
    }
}

/// 復旧手法。どの段階の情報を使って抽出したかを示します。
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoveryMethod {
    /// FS メタデータが健全な状態からの抽出。
    L1_MetadataIntact,
    /// パーティションを再構築して抽出。
    L2_PartitionReconstructed,
    /// FS メタデータを再構築して抽出。
    L3_FsMetadataReconstructed,
}

impl fmt::Display for RecoveryMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::L1_MetadataIntact => "L1: FSメタ健全",
            Self::L2_PartitionReconstructed => "L2: パーティション再構築",
            Self::L3_FsMetadataReconstructed => "L3: FSメタデータ再構築",
        };
        f.write_str(s)
    }
}

/// 抽出ファイルの品質評価（信号機モデル）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QualityRating {
    /// 健全（バリデータ完全通過）。
    Green,
    /// 軽微破損（実用可能）。
    Yellow,
    /// 重大破損（要確認）。
    Orange,
    /// 破損（実用不可）。
    Red,
}

impl QualityRating {
    /// 納品として許容可能か（Green/Yellow のみ true）。
    pub fn is_acceptable(&self) -> bool {
        matches!(self, Self::Green | Self::Yellow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_error_io_display_contains_inner_message() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "missing sector");
        let err: CoreError = inner.into();
        let msg = format!("{}", err);
        assert!(msg.contains("I/O"), "actual: {}", msg);
        assert!(msg.contains("missing sector"), "actual: {}", msg);
    }

    #[test]
    fn core_error_out_of_range_includes_value_and_max() {
        let err = CoreError::OutOfRange {
            what: "cluster index".to_string(),
            value: 4096,
            max: 1024,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("4096"), "value 抜け: {}", msg);
        assert!(msg.contains("1024"), "max 抜け: {}", msg);
        assert!(msg.contains("cluster index"), "what 抜け: {}", msg);
    }

    #[test]
    fn damage_level_display_ja_all_variants() {
        assert_eq!(DamageLevel::L1_DeletionOnly.display_ja(), "L1: 削除のみ");
        assert_eq!(
            DamageLevel::L2_PartitionTableDamaged.display_ja(),
            "L2: パーティションテーブル損傷"
        );
        assert_eq!(
            DamageLevel::L3_FsMetadataPartiallyDamaged.display_ja(),
            "L3: FSメタデータ部分損傷"
        );
        assert_eq!(
            DamageLevel::L4_BothDamaged.display_ja(),
            "L4: パーティション・FS両損傷"
        );
        assert_eq!(
            DamageLevel::L5_FsMetadataLost.display_ja(),
            "L5: FSメタデータ消失"
        );
        assert_eq!(DamageLevel::L6_SevereDamage.display_ja(), "L6: 重度損傷");
        assert_eq!(DamageLevel::PhysicalIssue.display_ja(), "物理障害");
        assert_eq!(format!("{}", DamageLevel::L1_DeletionOnly), "L1: 削除のみ");
    }

    #[test]
    fn quality_rating_is_acceptable_truth_table() {
        assert!(QualityRating::Green.is_acceptable());
        assert!(QualityRating::Yellow.is_acceptable());
        assert!(!QualityRating::Orange.is_acceptable());
        assert!(!QualityRating::Red.is_acceptable());
    }

    #[test]
    fn recovery_method_display_outputs_japanese_label() {
        assert_eq!(
            format!("{}", RecoveryMethod::L1_MetadataIntact),
            "L1: FSメタ健全"
        );
        assert_eq!(
            format!("{}", RecoveryMethod::L2_PartitionReconstructed),
            "L2: パーティション再構築"
        );
        assert_eq!(
            format!("{}", RecoveryMethod::L3_FsMetadataReconstructed),
            "L3: FSメタデータ再構築"
        );
    }
}
