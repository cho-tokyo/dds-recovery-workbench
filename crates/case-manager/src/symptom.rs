//! Chunk 21: 案件の症状分類 `Symptom` と FS 異常詳細 `FsAnomaly`。
//!
//! 業務シナリオ:
//! - `None`: 異常なし（誤発注、ヒアリングミス等）
//! - `Deleted`: 削除案件（ゴミ箱からの削除、Shift+Delete、社員退職時の一括削除）
//! - `Formatted`: クイックフォーマット / 再インストール後の復旧
//! - `FilesystemError`: MFT 破損 / ブートセクタ異常 / ボリュームシリアル不整合等
//! - `Mixed`: 複合症状（フォーマット後にさらに削除、等）
//!
//! `primary_label` で業務向け日本語ラベル（CRM 表示・レポート見出し用）を返す。
//!
//! 関連 FR: FR-CASE-01 (案件単位管理) の一部。

use serde::{Deserialize, Serialize};

/// 案件の主症状分類。
///
/// `#[serde(tag = "type")]` により JSON では `{"type": "Deleted"}` のように
/// タグ付きで表現される（人間が読んだ時に意味が分かる形式）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Symptom {
    /// 異常なし（誤発注、ヒアリング段階で誤認だった場合等）。
    None,
    /// 削除のみ。MFT は健全、エントリは存在する状態。
    Deleted,
    /// フォーマット済み。MFT が再初期化されており、旧 MFT 残骸から復旧を試みる。
    Formatted {
        /// 現在のフォーマット後 MFT に存在するエントリ数。
        current_mft_entries: usize,
        /// 旧 MFT 残骸からの復旧可能性ヒント（0.0〜1.0、不明なら None）。
        old_mft_recoverability_hint: Option<f64>,
    },
    /// ファイルシステム構造の異常（MFT エントリ破損、ランリスト不正等）。
    FilesystemError {
        /// 検出された個別異常のリスト。
        anomalies: Vec<FsAnomaly>,
    },
    /// 上記の複合症状（例: フォーマット後に追加削除、FS 異常 + 削除）。
    Mixed {
        /// 含まれる個別症状（`Mixed` 自体を再ネストすることは想定しない）。
        symptoms: Vec<Symptom>,
    },
}

/// ファイルシステム上で検出した個別異常の種別。
///
/// `Symptom::FilesystemError { anomalies }` に格納される要素。
/// `#[serde(tag = "kind")]` で JSON タグ付き表現。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FsAnomaly {
    /// MFT エントリが破損している（マジック番号不一致、フィックスアップ失敗等）。
    MftEntryCorrupted {
        /// 検出された破損エントリ数。
        count: usize,
    },
    /// ランリストの解析に失敗（オフセット負値、データラン範囲外等）。
    InvalidRunList {
        /// 検出された不正ランリスト数。
        count: usize,
    },
    /// ブートセクタが異常（マジック番号不一致、ジオメトリ矛盾等）。
    BootSectorAnomaly {
        /// 異常内容の人間可読な説明。
        description: String,
    },
    /// ボリュームシリアルが想定外（クローン後のミスマッチ、ゼロ値等）。
    InvalidVolumeSerial,
    /// 上記に分類されないその他の異常。
    Other {
        /// 異常内容の人間可読な説明。
        description: String,
    },
}

impl Symptom {
    /// 業務的な「主症状」を日本語で返す（CRM 表示・レポート見出し用）。
    ///
    /// `Mixed` の場合の優先順位:
    /// 1. `FilesystemError` を含む → `"ファイルシステム異常 (複合)"`
    /// 2. `Formatted` を含む（上記なし） → `"フォーマット (複合)"`
    /// 3. それ以外（削除のみの複合等） → `"削除 (複合)"`
    pub fn primary_label(&self) -> &str {
        match self {
            Symptom::None => "異常なし",
            Symptom::Deleted => "削除",
            Symptom::Formatted { .. } => "フォーマット",
            Symptom::FilesystemError { .. } => "ファイルシステム異常",
            Symptom::Mixed { symptoms } => {
                if symptoms
                    .iter()
                    .any(|s| matches!(s, Symptom::FilesystemError { .. }))
                {
                    "ファイルシステム異常 (複合)"
                } else if symptoms
                    .iter()
                    .any(|s| matches!(s, Symptom::Formatted { .. }))
                {
                    "フォーマット (複合)"
                } else {
                    "削除 (複合)"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symptom_none_serializes_with_type_tag() {
        let s = Symptom::None;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "{\"type\":\"None\"}");
    }

    #[test]
    fn symptom_deleted_serializes_correctly() {
        let s = Symptom::Deleted;
        let json = serde_json::to_string(&s).unwrap();
        let restored: Symptom = serde_json::from_str(&json).unwrap();
        assert_eq!(s, restored);
        assert!(json.contains("\"type\":\"Deleted\""));
    }

    #[test]
    fn symptom_formatted_includes_fields() {
        let s = Symptom::Formatted {
            current_mft_entries: 24,
            old_mft_recoverability_hint: Some(0.85),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("current_mft_entries"));
        assert!(json.contains("24"));
        let restored: Symptom = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, s);
    }

    #[test]
    fn symptom_primary_label_returns_japanese() {
        assert_eq!(Symptom::None.primary_label(), "異常なし");
        assert_eq!(Symptom::Deleted.primary_label(), "削除");
        assert_eq!(
            Symptom::Formatted {
                current_mft_entries: 0,
                old_mft_recoverability_hint: None
            }
            .primary_label(),
            "フォーマット"
        );
        assert_eq!(
            Symptom::FilesystemError { anomalies: vec![] }.primary_label(),
            "ファイルシステム異常"
        );
    }

    #[test]
    fn symptom_mixed_primary_label_prioritizes_fs_error() {
        let mixed_with_fs = Symptom::Mixed {
            symptoms: vec![
                Symptom::Deleted,
                Symptom::Formatted {
                    current_mft_entries: 0,
                    old_mft_recoverability_hint: None,
                },
                Symptom::FilesystemError { anomalies: vec![] },
            ],
        };
        assert_eq!(mixed_with_fs.primary_label(), "ファイルシステム異常 (複合)");

        let mixed_with_format = Symptom::Mixed {
            symptoms: vec![
                Symptom::Deleted,
                Symptom::Formatted {
                    current_mft_entries: 0,
                    old_mft_recoverability_hint: None,
                },
            ],
        };
        assert_eq!(mixed_with_format.primary_label(), "フォーマット (複合)");

        let mixed_deleted_only = Symptom::Mixed {
            symptoms: vec![Symptom::Deleted, Symptom::Deleted],
        };
        assert_eq!(mixed_deleted_only.primary_label(), "削除 (複合)");
    }
}
