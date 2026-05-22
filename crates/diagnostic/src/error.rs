//! Chunk 22: 診断エンジンが返すエラー型 `DiagnosticError`。
//!
//! `NtfsVolume` の読み取りエラー（[`dds_fs_ntfs::VolumeError`]）、I/O エラー、
//! 案件 JSON 用 serde_json エラー、診断タイムアウトを集約する。
//! 個別 MFT エントリのパースエラーは aggregator 内で「FS 異常」としてカウントし、
//! 診断自体を中断させない設計（破損耐性）。
//!
//! 関連 FR: FR-DIAG-01〜05 共通の防御層。

use thiserror::Error;

/// 診断エンジンが返すエラー。
///
/// `Volume` / `Io` / `Json` は `?` で透過変換できるよう `#[from]` を付与。
/// `Timeout` は将来 Phase 2 で長時間診断の打ち切り用に予約（Phase 1.5 では未使用）。
#[derive(Error, Debug)]
pub enum DiagnosticError {
    /// `NtfsVolume::open` / `read_record` 等で発生したボリュームエラー。
    #[error("Volume error: {0}")]
    Volume(#[from] dds_fs_ntfs::VolumeError),

    /// ファイル I/O エラー（フィクスチャ解凍・case.json 書き出し等）。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// case.json への serialize/deserialize 失敗。
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// 診断時間が `limit_secs` 秒を超過した（Phase 2 予約バリアント）。
    #[error("Diagnostic timeout: exceeded {limit_secs} seconds")]
    Timeout {
        /// 設定されていた上限秒数。
        limit_secs: u64,
    },
}
