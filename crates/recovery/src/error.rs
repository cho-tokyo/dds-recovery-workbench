//! Chunk 17: 復旧パイプラインのエラー型。
//!
//! Chunks 4-16 で確立した `thiserror` ベースのエラー命名規約に準拠し、
//! `RecoveryError` 1 つに全失敗ケースを集約する。アプリケーション層では
//! `anyhow` で扱う前提なので `Send + Sync + 'static` を維持する。

use std::path::PathBuf;
use thiserror::Error;

/// 復旧パイプラインで発生し得るエラー全集合。
#[derive(Error, Debug)]
pub enum RecoveryError {
    /// 出力先 I/O エラー（書き込み・作成失敗等）。`std::io::Error` を保持。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// 出力ディレクトリが不正（存在しないパスを作れない、ディレクトリでない等）。
    #[error("Invalid output directory: {path:?} ({reason})")]
    InvalidOutputDir {
        /// 問題のあったパス。
        path: PathBuf,
        /// 失敗理由。
        reason: String,
    },

    /// NTFS パスに `..` 等のパストラバーサル要素が含まれていた。
    #[error("Path traversal attempt: {path} contains '..' or escapes output dir")]
    PathTraversal {
        /// 問題のあった NTFS パス。
        path: String,
    },

    /// サニタイズしても有効なファイル名にできなかった（空文字等）。
    #[error("Filename cannot be sanitized: {original:?}")]
    UnsanitizableFilename {
        /// サニタイズ前の原本ファイル名。
        original: String,
    },

    /// 下層の NTFS ボリュームエラーを集約。
    #[error("Volume error: {0}")]
    Volume(#[from] dds_fs_ntfs::VolumeError),

    /// `Rename` 衝突戦略で `MAX_RENAME_ATTEMPTS` 回試してもユニーク名が得られなかった。
    #[error("Could not find unique filename after {attempts} attempts")]
    UniqueFilenameExhausted {
        /// 試行回数。
        attempts: u32,
    },
}
