//! Chunk 21: 案件管理クレートのエラー型。
//!
//! `CaseError` は案件番号バリデーション失敗、ストレージ操作（重複作成 / 不存在）、
//! I/O / JSON 失敗を統合する。`#[from]` で `std::io::Error` と
//! `serde_json::Error` を自動変換し、`?` 演算子で簡潔に扱える。
//!
//! 関連 FR: FR-CASE-01 (案件単位管理), FR-CASE-03 (案件情報の永続化)。

use thiserror::Error;

/// 案件管理処理で発生し得るエラー種別。
#[derive(Error, Debug)]
pub enum CaseError {
    /// 案件番号 (yymmdd-NN) の形式が不正。
    ///
    /// 例: 長さ違い、数字以外の文字、ハイフン位置ずれ等。
    /// `input` には受け取った文字列、`reason` には人間可読な理由を格納する。
    #[error("Invalid case ID '{input}': {reason}")]
    InvalidCaseId {
        /// 受け取った入力文字列。
        input: String,
        /// バリデーション失敗の理由。
        reason: String,
    },

    /// `create_new` で既に同名案件が存在する場合。
    ///
    /// Q28 の業務確定動作: 既存案件を上書きせずエラーを返す。
    #[error("Case already exists: {case_id}")]
    CaseAlreadyExists {
        /// 既存の案件番号（文字列化）。
        case_id: String,
    },

    /// `load` / `delete` で対象案件が存在しない場合。
    #[error("Case not found: {case_id}")]
    CaseNotFound {
        /// 対象の案件番号（文字列化）。
        case_id: String,
    },

    /// ファイルシステム I/O 失敗（ディレクトリ作成、書込、読込、削除）。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON シリアライズ / デシリアライズ失敗。
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
