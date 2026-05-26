//! Chunk 24a: NTFS タイムスタンプ保持 (Creation / Modified / Accessed)。
//! Chunk 24c: 高速版 ([`apply_timestamps_to_handle`]) を追加。
//!
//! 業界標準 (R-STUDIO 等) と同等の挙動。復旧したファイルに元の NTFS の
//! `$STANDARD_INFORMATION` 由来 3 タイムスタンプを書き戻す。
//!
//! ## API バリエーション
//!
//! - [`apply_timestamps_to_handle`] (Chunk 24c): 既に開いている `File` ハンドルに
//!   `SetFileTime` を実行する高速版。書き込み直後の `close` 前に呼ぶことで
//!   ファイル毎の open 回数を半減し、復旧速度を改善する (**推奨**)。
//! - [`apply_timestamps`]: パスからファイルを再 open して設定する互換版。
//!   エラーリカバリや外部呼び出し用。内部で `apply_timestamps_to_handle` を呼ぶ。
//!
//! ## 安全性
//!
//! このモジュールは Windows API `SetFileTime` を直接呼ぶため `unsafe` を含むが、
//! `unsafe` ブロックは [`apply_timestamps_to_handle`] 内 2 箇所に**集約**されている
//! (Chunk 24c で `apply_timestamps` から移設済み、合計行数は変わらず)。
//! 引数検証 + RAII (`File` で OS ハンドルを所有) + Windows 限定 (`#[cfg(windows)]`) で
//! 安全性を確保。他のクレート / モジュールは全て safe のまま (workspace 全体での唯一の `unsafe`)。
//!
//! ## 失敗時の業務的扱い
//!
//! タイムスタンプ書き込みに失敗してもファイル内容自体は復旧成功している。
//! 呼び出し側 ([`crate::engine::RecoveryEngine`]) は警告ログのみ出力して
//! 復旧フローを継続する設計。
//!
//! 関連 FR: FR-REC-07 (タイムスタンプ保持、業界標準準拠)、FR-REC-09 (ファイル open 最小化、Chunk 24c)。

use std::fs::File;
use std::path::Path;

use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows_sys::Win32::Foundation::FILETIME;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{SetFileTime, FILE_WRITE_ATTRIBUTES};

/// タイムスタンプ書き込みに関するエラー。
#[derive(Debug, Error)]
pub enum TimestampError {
    /// 対象ファイルを開けなかった (パーミッション・パス不在等)。
    #[error("ファイルを開けません: {0}")]
    Open(#[from] std::io::Error),

    /// Windows API `SetFileTime` が失敗した。`GetLastError()` の値を保持。
    #[error("Windows API SetFileTime が失敗しました (エラーコード: {0})")]
    Win32Error(u32),

    /// `chrono::DateTime` → `FILETIME` 変換でオーバーフロー等が発生した。
    #[error("時刻の変換に失敗しました: {0}")]
    TimeConversion(String),

    /// 非 Windows プラットフォームで呼び出された (このバイナリではサポート外)。
    #[error("タイムスタンプ書き込みは Windows のみサポートしています")]
    Unsupported,
}

/// NTFS の 3 種類のタイムスタンプ (Creation / Modified / Accessed)。
///
/// 業務的に「3 種すべて Some」のときのみ `apply_timestamps` を呼ぶ前提。
/// いずれかが欠落している場合は呼び出し側で skip する (`$STANDARD_INFORMATION`
/// が壊れている可能性があり、部分書き込みは行わない設計)。
#[derive(Debug, Clone, Copy)]
pub struct NtfsTimestamps {
    /// 作成日時。
    pub created: DateTime<Utc>,
    /// 内容更新日時。
    pub modified: DateTime<Utc>,
    /// 最終アクセス日時。
    pub accessed: DateTime<Utc>,
}

/// UNIX epoch (1970-01-01) から Windows FILETIME epoch (1601-01-01) までの秒差。
#[cfg(windows)]
const EPOCH_DIFFERENCE_SECONDS: i64 = 11_644_473_600;

/// `chrono::DateTime<Utc>` を Windows `FILETIME` に変換する純粋関数。
///
/// `FILETIME` は 1601-01-01 UTC からの 100 ナノ秒単位。`checked_add` /
/// `checked_mul` でオーバーフロー時は `TimeConversion` エラーを返す。
#[cfg(windows)]
fn datetime_to_filetime(dt: DateTime<Utc>) -> Result<FILETIME, TimestampError> {
    let unix_secs = dt.timestamp();
    let unix_nanos = dt.timestamp_subsec_nanos();

    let windows_seconds = unix_secs
        .checked_add(EPOCH_DIFFERENCE_SECONDS)
        .ok_or_else(|| TimestampError::TimeConversion("時刻オーバーフロー (epoch shift)".into()))?;

    let filetime_100ns = windows_seconds
        .checked_mul(10_000_000)
        .and_then(|v| v.checked_add((unix_nanos / 100) as i64))
        .ok_or_else(|| {
            TimestampError::TimeConversion("FILETIME 換算オーバーフロー (100ns)".into())
        })?;

    if filetime_100ns < 0 {
        return Err(TimestampError::TimeConversion(
            "FILETIME は 1601 年より前の時刻を表現できません".into(),
        ));
    }

    let filetime_u64 = filetime_100ns as u64;
    Ok(FILETIME {
        dwLowDateTime: (filetime_u64 & 0xFFFF_FFFF) as u32,
        dwHighDateTime: ((filetime_u64 >> 32) & 0xFFFF_FFFF) as u32,
    })
}

/// 既に開いている [`File`] ハンドルに 3 種のタイムスタンプを書き込む (Chunk 24c、推奨)。
///
/// ファイル書き込み直後 (`File::create` → `write_all` → これを呼ぶ → drop で close) という
/// フローで使うことで、ファイル毎の open 回数を半減し、復旧速度を改善する。
///
/// ## 引数の条件
///
/// `file` は `GENERIC_WRITE` または `FILE_WRITE_ATTRIBUTES` アクセスで開かれている必要がある。
/// `std::fs::File::create()` で開いた場合は条件を満たす (write アクセス)。
///
/// ## 失敗時の挙動
///
/// 失敗してもファイル内容自体には影響しない。呼び出し側は警告ログを出して復旧フローを
/// 継続する設計 (タイムスタンプは「あれば良い」業界標準だが、欠落しても復旧失敗ではない)。
#[cfg(windows)]
pub fn apply_timestamps_to_handle(
    file: &File,
    timestamps: &NtfsTimestamps,
) -> Result<(), TimestampError> {
    let creation_ft = datetime_to_filetime(timestamps.created)?;
    let modified_ft = datetime_to_filetime(timestamps.modified)?;
    let accessed_ft = datetime_to_filetime(timestamps.accessed)?;

    let handle = file.as_raw_handle();

    // SAFETY:
    // - `handle` は引数 `file: &File` から取得した有効な OS ハンドル。
    // - `file` の lifetime は本関数のスコープ内で生存しているため、handle は
    //   `SetFileTime` 実行中に閉じられない (Rust の borrow checker が保証)。
    // - `FILETIME` は POD 値型で、`&ft as *const FILETIME` は本関数のスタック上の有効な値を指す。
    // - SetFileTime は Microsoft 提供 API で副作用は対象ファイルのメタデータのみ。
    // - 戻り値 0 のときのみ GetLastError を呼ぶ Microsoft の標準慣習に従う。
    let result = unsafe {
        SetFileTime(
            handle as windows_sys::Win32::Foundation::HANDLE,
            &creation_ft as *const FILETIME,
            &accessed_ft as *const FILETIME,
            &modified_ft as *const FILETIME,
        )
    };

    if result == 0 {
        // SAFETY: GetLastError は副作用なし。直前の Win32 呼び出しのエラーコードを返すだけ。
        let error_code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(TimestampError::Win32Error(error_code));
    }

    Ok(())
}

/// 非 Windows プラットフォーム用 stub。常に `Unsupported` を返す。
#[cfg(not(windows))]
pub fn apply_timestamps_to_handle(
    _file: &File,
    _timestamps: &NtfsTimestamps,
) -> Result<(), TimestampError> {
    Err(TimestampError::Unsupported)
}

/// 指定パスのファイルに 3 種のタイムスタンプを書き込む (互換版)。
///
/// 内部で `OpenOptions::write(true).access_mode(FILE_WRITE_ATTRIBUTES)` で属性書き込み権限の
/// ハンドルを開き、[`apply_timestamps_to_handle`] を呼ぶ。`File` の drop で OS ハンドルが
/// 自動的に閉じられる (RAII)。
///
/// ## 性能
///
/// ファイルを再 open する分のオーバーヘッドがある。復旧パイプライン中で書き込み直後に
/// 呼ぶ場合は [`apply_timestamps_to_handle`] を使うこと (Chunk 24c で導入)。
///
/// ## いつ使うか
///
/// - エラーリカバリ: 書き込み完了後に別途タイムスタンプだけ修正する
/// - 外部からの呼び出し: ファイルハンドルを持たない呼び出し元
/// - テストコード: 既存テストとの互換性
#[cfg(windows)]
pub fn apply_timestamps(path: &Path, timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    let file = OpenOptions::new()
        .write(true)
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .open(path)?;
    apply_timestamps_to_handle(&file, timestamps)
}

/// 非 Windows プラットフォーム用 stub。常に `Unsupported` を返す。
#[cfg(not(windows))]
pub fn apply_timestamps(_path: &Path, _timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    Err(TimestampError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntfs_timestamps_struct_holds_three_dates() {
        // 3 フィールドが独立に保持されることの基本確認。
        let c = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let m = DateTime::parse_from_rfc3339("2024-06-15T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);
        let a = DateTime::parse_from_rfc3339("2024-12-31T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: c,
            modified: m,
            accessed: a,
        };
        assert_eq!(ts.created, c);
        assert_eq!(ts.modified, m);
        assert_eq!(ts.accessed, a);
        // Copy なのでムーブ後も使える。
        let _ts2 = ts;
        assert_eq!(ts.modified, m);
    }

    #[cfg(windows)]
    #[test]
    fn datetime_to_filetime_roundtrip() {
        // 既知の日時 (2024-03-15T10:30:00Z) を変換 → 戻して整合確認。
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ft = datetime_to_filetime(dt).unwrap();
        let filetime_u64 = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        let windows_seconds = (filetime_u64 / 10_000_000) as i64;
        let unix_secs = windows_seconds - EPOCH_DIFFERENCE_SECONDS;
        assert_eq!(unix_secs, dt.timestamp());
    }

    #[cfg(windows)]
    #[test]
    fn apply_timestamps_to_actual_file() {
        use std::fs::write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        write(temp.path(), b"test content").unwrap();

        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };

        apply_timestamps(temp.path(), &ts).expect("apply_timestamps should succeed");

        // 書き込まれたか確認 (NTFS は 100ns 精度、秒で比較すれば一致するはず)。
        let metadata = std::fs::metadata(temp.path()).unwrap();
        let modified_time = metadata.modified().unwrap();
        let modified_dt: DateTime<Utc> = modified_time.into();
        assert_eq!(modified_dt.timestamp(), dt.timestamp());
    }

    #[cfg(not(windows))]
    #[test]
    fn apply_timestamps_returns_error_on_non_windows() {
        let dt = Utc::now();
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };
        // non-Windows ターゲットでは stub が Unsupported を返す。
        let result = apply_timestamps(Path::new("/tmp/dummy"), &ts);
        assert!(matches!(result, Err(TimestampError::Unsupported)));
    }

    /// Chunk 24c: 高速版 (handle 版) が開いている File に対して動作することを確認。
    /// 書き込み直後の close 前に呼ぶフロー (engine.rs::write_with_timestamps と同じパターン)。
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_to_open_handle_works() {
        use std::io::Write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp); // NamedTempFile を一度 drop してパスのみ保持。

        // engine の write_with_timestamps と同様、File::create で開いて write → SetFileTime → close。
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"test content").unwrap();
        file.flush().unwrap();

        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };

        // open している file に対して直接 SetFileTime。
        apply_timestamps_to_handle(&file, &ts).expect("apply_timestamps_to_handle should succeed");
        drop(file); // close。

        let metadata = std::fs::metadata(&path).unwrap();
        let modified_time = metadata.modified().unwrap();
        let modified_dt: DateTime<Utc> = modified_time.into();
        assert_eq!(modified_dt.timestamp(), dt.timestamp());

        // クリーンアップ。
        let _ = std::fs::remove_file(&path);
    }

    /// Chunk 24c: path 版が handle 版と同じ結果を返すことの回帰確認。
    /// 既存呼び出し元 (Chunk 24a の API ユーザ) への後方互換性保証。
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_via_path_calls_handle_version() {
        use std::fs::write;
        let temp1 = tempfile::NamedTempFile::new().unwrap();
        let temp2 = tempfile::NamedTempFile::new().unwrap();
        write(temp1.path(), b"a").unwrap();
        write(temp2.path(), b"b").unwrap();

        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };

        // path 版。
        apply_timestamps(temp1.path(), &ts).unwrap();

        // handle 版 (同一 ts で同じ結果になるはず)。
        let file = OpenOptions::new()
            .write(true)
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .open(temp2.path())
            .unwrap();
        apply_timestamps_to_handle(&file, &ts).unwrap();
        drop(file);

        // 秒精度で両者一致。
        let m1: DateTime<Utc> = std::fs::metadata(temp1.path())
            .unwrap()
            .modified()
            .unwrap()
            .into();
        let m2: DateTime<Utc> = std::fs::metadata(temp2.path())
            .unwrap()
            .modified()
            .unwrap()
            .into();
        assert_eq!(m1.timestamp(), m2.timestamp());
        assert_eq!(m1.timestamp(), dt.timestamp());
    }
}
