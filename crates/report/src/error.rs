//! Chunk 20: レポート生成エラー型。
//!
//! HTML / CSV レポート生成中に発生し得るエラーを統一的に表現する。
//! `thiserror` で from 変換を自動派生（`std::io::Error`、`csv::Error`）。

use thiserror::Error;

/// レポート生成中に発生し得るエラー。
#[derive(Error, Debug)]
pub enum ReportError {
    /// ファイル書き込み・ディレクトリ作成等の I/O エラー。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// CSV シリアライズ中のエラー（`csv` crate 由来）。
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    /// HTML テンプレート組み立て中のエラー（UTF-8 変換失敗等）。
    #[error("Template rendering error: {0}")]
    Template(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_is_convertible_from_std_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let rep: ReportError = io_err.into();
        assert!(matches!(rep, ReportError::Io(_)));
        assert!(rep.to_string().contains("I/O error"));
    }

    #[test]
    fn template_error_message_round_trip() {
        let err = ReportError::Template("bad utf-8".into());
        assert!(err.to_string().contains("Template"));
        assert!(err.to_string().contains("bad utf-8"));
    }

    #[test]
    fn display_distinguishes_variants() {
        let io = ReportError::Io(std::io::Error::other("disk full"));
        let tpl = ReportError::Template("x".into());
        // Display 文字列が variant ごとに異なるプレフィックスを持つこと
        assert_ne!(io.to_string(), tpl.to_string());
    }
}
