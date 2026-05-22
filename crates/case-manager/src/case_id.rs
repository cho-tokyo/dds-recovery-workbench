//! Chunk 21: 案件番号 `CaseId` （yymmdd-NN 形式）の newtype 実装。
//!
//! CRM が採番する案件番号を Workbench 内で型安全に扱う。形式チェックは
//! 文字種・位置のみ（緩いバリデーション）：日付の月日妥当性は CRM 責務のためチェックしない。
//!
//! JSON では `"260522-04"` のような plain string として表現される（タプル struct の
//! ラップ `["260522-04"]` ではない）ため、Serialize / Deserialize は手動実装。
//!
//! 関連 FR: FR-CASE-02 (yymmdd-NN 形式案件番号の識別)。

use crate::error::CaseError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 案件番号 (yymmdd-NN)。
///
/// 形式仕様:
/// - 全 9 文字（ASCII）
/// - 先頭 6 文字: 日付 yymmdd（すべて数字）
/// - 7 文字目: ハイフン '-'
/// - 末尾 2 文字: 連番 NN（すべて数字、00-99）
///
/// 例: `"260522-04"` （2026 年 5 月 22 日の 4 番目の案件）
///
/// 注: 日付の意味的妥当性 (mm が 01-12、dd が 01-31 等) はチェックしない。
/// CRM が採番責任を持つため、形式の構造のみ検証する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaseId(String);

impl CaseId {
    /// 案件番号文字列を検証してラップする。
    ///
    /// # エラー
    /// - 長さが 9 以外
    /// - 位置 0-5 / 7-8 が数字でない
    /// - 位置 6 がハイフンでない
    pub fn parse(s: &str) -> Result<Self, CaseError> {
        if s.len() != 9 {
            return Err(CaseError::InvalidCaseId {
                input: s.to_string(),
                reason: format!("length must be exactly 9, got {}", s.len()),
            });
        }

        let bytes = s.as_bytes();

        for (i, b) in bytes.iter().enumerate().take(6) {
            if !b.is_ascii_digit() {
                return Err(CaseError::InvalidCaseId {
                    input: s.to_string(),
                    reason: format!("position {} must be a digit, got '{}'", i, *b as char),
                });
            }
        }

        if bytes[6] != b'-' {
            return Err(CaseError::InvalidCaseId {
                input: s.to_string(),
                reason: format!("position 6 must be '-', got '{}'", bytes[6] as char),
            });
        }

        for (i, b) in bytes.iter().enumerate().take(9).skip(7) {
            if !b.is_ascii_digit() {
                return Err(CaseError::InvalidCaseId {
                    input: s.to_string(),
                    reason: format!("position {} must be a digit, got '{}'", i, *b as char),
                });
            }
        }

        Ok(Self(s.to_string()))
    }

    /// 内部の案件番号文字列を借用で返す。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for CaseId {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CaseId {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        CaseId::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_id_parses_valid_format() {
        let id = CaseId::parse("260522-04").expect("valid format");
        assert_eq!(id.as_str(), "260522-04");
        assert_eq!(id.to_string(), "260522-04");
    }

    #[test]
    fn case_id_rejects_short_input() {
        let err = CaseId::parse("26052-04").unwrap_err();
        assert!(matches!(err, CaseError::InvalidCaseId { .. }));
    }

    #[test]
    fn case_id_rejects_long_input() {
        let err = CaseId::parse("2605221-04").unwrap_err();
        assert!(matches!(err, CaseError::InvalidCaseId { .. }));
    }

    #[test]
    fn case_id_rejects_missing_hyphen() {
        let err = CaseId::parse("260522X04").unwrap_err();
        let CaseError::InvalidCaseId { reason, .. } = err else {
            panic!("wrong variant");
        };
        assert!(reason.contains("position 6"));
    }

    #[test]
    fn case_id_rejects_non_digit_in_date_part() {
        let err = CaseId::parse("26A522-04").unwrap_err();
        let CaseError::InvalidCaseId { reason, .. } = err else {
            panic!("wrong variant");
        };
        assert!(reason.contains("position 2"));
    }

    #[test]
    fn case_id_rejects_non_digit_in_sequence_part() {
        let err = CaseId::parse("260522-0X").unwrap_err();
        let CaseError::InvalidCaseId { reason, .. } = err else {
            panic!("wrong variant");
        };
        assert!(reason.contains("position 8"));
    }

    #[test]
    fn case_id_serializes_as_plain_string() {
        let id = CaseId::parse("260522-04").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"260522-04\"");
    }

    #[test]
    fn case_id_deserializes_from_string() {
        let id: CaseId = serde_json::from_str("\"260601-12\"").unwrap();
        assert_eq!(id.as_str(), "260601-12");
    }

    #[test]
    fn case_id_deserialize_rejects_invalid_string() {
        let result: Result<CaseId, _> = serde_json::from_str("\"INVALID\"");
        assert!(result.is_err());
    }
}
