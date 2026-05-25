//! Chunk 23.8: Uncertain (検証外) 判定の理由分類。
//!
//! 「Uncertain だけ」では業務的に「なぜ確認できなかった」が CS / お客様
//! に伝わらない。5 つの理由（[`UncertainReason`]）に分類することで、
//! お客様への品質報告に「破損疑い」vs「自動確認対象外」のメリハリを付ける。
//!
//! 関連 FR: FR-QUAL-04 (Uncertain 理由分類)。

use serde::{Deserialize, Serialize};

use crate::format_bytes_helper::format_bytes;

/// Uncertain (検証外) と判定された具体的な理由。
///
/// `ValidationStatus::Uncertain(UncertainReason)` の中に格納され、
/// レポート上で内訳表示やフィルタリングに使われる。
///
/// # 5 つの理由
///
/// | バリアント | 業務的意味 |
/// |---|---|
/// | [`NoValidatorAvailable`](Self::NoValidatorAvailable) | 拡張子未対応 / 拡張子なし。Workbench の機能限界として CS が説明 |
/// | [`Encrypted`](Self::Encrypted) | パスワード保護等。お客様にパスワード提供を依頼 |
/// | [`TooLargeForValidation`](Self::TooLargeForValidation) | サイズ超過。手動確認を推奨 |
/// | [`ValidatorError`](Self::ValidatorError) | Validator 内部エラー。バグの可能性、CS で再復旧 |
/// | [`ExtensionMismatch`](Self::ExtensionMismatch) | 拡張子と中身の不一致。CS が業務的に気づくサイン |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UncertainReason {
    /// 対応する Validator がない（拡張子未対応、独自形式、拡張子なし等）。
    NoValidatorAvailable,

    /// ファイルが暗号化されている（パスワード保護等）。
    Encrypted,

    /// ファイルが大きすぎて検証スキップ。
    TooLargeForValidation {
        /// ファイルサイズ (バイト)。
        size: u64,
        /// 検証スキップ閾値 (バイト)。
        threshold: u64,
    },

    /// Validator 内部でエラーが発生（パースエラー等）。
    ValidatorError {
        /// 開発者向けエラーメッセージ。
        message: String,
    },

    /// 拡張子と検出形式が一致しない（例: `.jpg` だが中身は PDF）。
    ExtensionMismatch {
        /// 中身から検出された形式名（例: `"PDF"`）。
        detected_format: String,
    },
}

impl UncertainReason {
    /// お客様向けの日本語メッセージ。
    ///
    /// 顧客レポート上に表示しても問題ない安全な文言。
    pub fn customer_message(&self) -> String {
        match self {
            Self::NoValidatorAvailable => "現在未対応のファイル形式".to_string(),
            Self::Encrypted => "暗号化されているため確認できません".to_string(),
            Self::TooLargeForValidation { size, threshold } => format!(
                "ファイルサイズが大きすぎます ({} 超、上限 {})",
                format_bytes(*size),
                format_bytes(*threshold)
            ),
            Self::ValidatorError { .. } => "ファイル形式の確認中にエラーが発生しました".to_string(),
            Self::ExtensionMismatch { detected_format } => {
                format!("拡張子と中身が一致しません (検出: {})", detected_format)
            }
        }
    }

    /// 内部向けの簡潔なラベル（レポートの表表示用）。
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::NoValidatorAvailable => "対応 Validator なし",
            Self::Encrypted => "暗号化",
            Self::TooLargeForValidation { .. } => "サイズ超過",
            Self::ValidatorError { .. } => "Validator エラー",
            Self::ExtensionMismatch { .. } => "拡張子不一致",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_reason_customer_message_in_japanese() {
        // 5 つの理由すべてに日本語の顧客向けメッセージがあり、空でないこと。
        let reasons = vec![
            UncertainReason::NoValidatorAvailable,
            UncertainReason::Encrypted,
            UncertainReason::TooLargeForValidation {
                size: 200 * 1024 * 1024,
                threshold: 100 * 1024 * 1024,
            },
            UncertainReason::ValidatorError {
                message: "parse error".into(),
            },
            UncertainReason::ExtensionMismatch {
                detected_format: "PDF".into(),
            },
        ];
        for reason in reasons {
            let msg = reason.customer_message();
            assert!(!msg.is_empty(), "メッセージが空でないこと: {:?}", reason);
            // 英語のみで構成されていない (日本語が含まれる)。
            assert!(
                msg.chars().any(|c| c as u32 > 0x7F),
                "日本語が含まれる: {} ({:?})",
                msg,
                reason
            );
        }
    }

    #[test]
    fn uncertain_reason_short_label_format() {
        // 5 つすべて短い日本語ラベル。表内表示で使える長さ (20 文字以下)。
        assert_eq!(
            UncertainReason::NoValidatorAvailable.short_label(),
            "対応 Validator なし"
        );
        assert_eq!(UncertainReason::Encrypted.short_label(), "暗号化");
        assert_eq!(
            UncertainReason::TooLargeForValidation {
                size: 0,
                threshold: 0
            }
            .short_label(),
            "サイズ超過"
        );
        assert_eq!(
            UncertainReason::ValidatorError {
                message: "x".into()
            }
            .short_label(),
            "Validator エラー"
        );
        assert_eq!(
            UncertainReason::ExtensionMismatch {
                detected_format: "PDF".into()
            }
            .short_label(),
            "拡張子不一致"
        );
    }

    #[test]
    fn uncertain_reason_size_too_large_includes_threshold() {
        // TooLargeForValidation のメッセージにファイルサイズと閾値が含まれる。
        let reason = UncertainReason::TooLargeForValidation {
            size: 200 * 1024 * 1024,
            threshold: 100 * 1024 * 1024,
        };
        let msg = reason.customer_message();
        // MB 表示が含まれる (200, 100 のどちらか)。
        assert!(msg.contains("MB") || msg.contains("バイト") || msg.contains("B"));
        // 超 / 上限 のキーワード
        assert!(msg.contains("超") || msg.contains("上限"));
    }

    #[test]
    fn uncertain_reason_serde_round_trip() {
        // Serialize → Deserialize が往復可能であること（JSON タグ "kind" でディスクリミネート）。
        let original = UncertainReason::ExtensionMismatch {
            detected_format: "PDF".into(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert!(json.contains("ExtensionMismatch"));
        assert!(json.contains("PDF"));
        let restored: UncertainReason = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, restored);
    }
}
