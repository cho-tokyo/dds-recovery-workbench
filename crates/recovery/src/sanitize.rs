//! Chunk 17: ファイル名サニタイズ。
//!
//! NTFS は Windows の予約名や禁止文字を含むファイル名を許可するが、復旧後の
//! Windows 出力先ファイルシステムでは OS が開けない / 削除できない問題が生じる。
//! 業務上の納品成果物として安定的に扱うため、出力直前に予約名と禁止文字を
//! 安全に置換する。
//!
//! 関連 FR: FR-REC-02 (出力先指定), FR-REC-03 (衝突解決の前段)。

use crate::error::RecoveryError;

/// Windows でファイル名に使えない文字。
///
/// NTFS 仕様上は許可されるが、Windows API (Win32) は禁止しているもの。
const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows の予約名（大文字小文字非区別）。COM1-9 / LPT1-9 は別途数値判定で扱う。
const RESERVED_BASE_NAMES: &[&str] = &["CON", "PRN", "AUX", "NUL"];

/// ファイル名 1 セグメントをサニタイズする。
///
/// 処理内容（順序重要）:
/// 1. 禁止文字 (`<>:"/\|?*`) と制御文字 (`0x00`〜`0x1F`) を `_` に置換
/// 2. 末尾の `.` と空白を全て削除（Windows で問題になる）
/// 3. ベース部（`.` で分割した最初）を大文字化して予約名判定
/// 4. `CON` / `PRN` / `AUX` / `NUL` / `COM1`〜`COM9` / `LPT1`〜`LPT9` なら
///    `_` プレフィックスで衝突回避
/// 5. 空文字列なら [`RecoveryError::UnsanitizableFilename`] エラー
///
/// 業務観点: 日本語等の非 ASCII 文字はそのまま保持（Windows NTFS は Unicode 対応）。
pub fn sanitize_filename(name: &str) -> Result<String, RecoveryError> {
    let mut sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_control() || FORBIDDEN_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();

    // 末尾の `.` および空白を削除（Windows API が末尾ピリオド・空白を許容しない）。
    while sanitized.ends_with('.') || sanitized.ends_with(' ') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        return Err(RecoveryError::UnsanitizableFilename {
            original: name.to_string(),
        });
    }

    // 予約名判定: ベース部分のみ（`.` 前部分）。「con.txt」も予約名衝突になる。
    let base_upper = sanitized
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();

    let is_reserved = RESERVED_BASE_NAMES.contains(&base_upper.as_str())
        || is_reserved_numbered(&base_upper);

    if is_reserved {
        sanitized = format!("_{}", sanitized);
    }

    Ok(sanitized)
}

/// `COM1`〜`COM9`, `LPT1`〜`LPT9` の予約名判定（大文字済み入力前提）。
fn is_reserved_numbered(base_upper: &str) -> bool {
    if base_upper.len() != 4 {
        return false;
    }
    let bytes = base_upper.as_bytes();
    let prefix = &bytes[..3];
    let digit = bytes[3];
    if !(b'1'..=b'9').contains(&digit) {
        return false;
    }
    prefix == b"COM" || prefix == b"LPT"
}

/// 削除ファイル識別子をファイル名に挿入する。
///
/// 例:
/// - `("foo.txt", 67)` → `"foo (deleted-#67).txt"`
/// - `("Makefile", 42)` → `"Makefile (deleted-#42)"`
///
/// CS がファイルマネージャで見たときに「これは削除復旧されたもの」と一目で
/// 識別できるよう、ファイル名内に MFT エントリ番号を埋め込む。
pub fn insert_deleted_marker(filename: &str, record_index: u64) -> String {
    if let Some((stem, ext)) = filename.rsplit_once('.') {
        format!("{} (deleted-#{}).{}", stem, record_index, ext)
    } else {
        format!("{} (deleted-#{})", filename, record_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_forbidden_chars() {
        assert_eq!(sanitize_filename("foo<>bar.txt").unwrap(), "foo__bar.txt");
        assert_eq!(sanitize_filename("a|b?c.txt").unwrap(), "a_b_c.txt");
        assert_eq!(sanitize_filename("path/with\\sep.txt").unwrap(), "path_with_sep.txt");
    }

    #[test]
    fn sanitize_strips_trailing_dots_and_spaces() {
        assert_eq!(sanitize_filename("foo.txt . ").unwrap(), "foo.txt");
        assert_eq!(sanitize_filename("name   ").unwrap(), "name");
        assert_eq!(sanitize_filename("name...").unwrap(), "name");
    }

    #[test]
    fn sanitize_prefixes_reserved_names() {
        // 予約名 4 種類: CON / PRN / AUX / NUL（拡張子なし・あり両方）。
        assert_eq!(sanitize_filename("CON").unwrap(), "_CON");
        assert_eq!(sanitize_filename("con.txt").unwrap(), "_con.txt");
        assert_eq!(sanitize_filename("PRN.log").unwrap(), "_PRN.log");
        // COM1-9 / LPT1-9。
        assert_eq!(sanitize_filename("COM1").unwrap(), "_COM1");
        assert_eq!(sanitize_filename("com1.txt").unwrap(), "_com1.txt");
        assert_eq!(sanitize_filename("LPT9.dat").unwrap(), "_LPT9.dat");
        // 似た名前は除外（COM0 / LPT0 は予約ではない、COMA も同じく）。
        assert_eq!(sanitize_filename("COM0").unwrap(), "COM0");
        assert_eq!(sanitize_filename("LPTA").unwrap(), "LPTA");
        assert_eq!(sanitize_filename("COM10").unwrap(), "COM10");
    }

    #[test]
    fn sanitize_preserves_normal_names() {
        assert_eq!(sanitize_filename("report.docx").unwrap(), "report.docx");
        // 日本語ファイル名はそのまま保持。
        assert_eq!(
            sanitize_filename("日本語ファイル.pdf").unwrap(),
            "日本語ファイル.pdf"
        );
        assert_eq!(sanitize_filename("Makefile").unwrap(), "Makefile");
    }

    #[test]
    fn sanitize_empty_returns_error() {
        assert!(matches!(
            sanitize_filename(""),
            Err(RecoveryError::UnsanitizableFilename { .. })
        ));
        // 末尾の `.` と空白の除去で空文字になるケースもエラー。
        assert!(matches!(
            sanitize_filename(" . . "),
            Err(RecoveryError::UnsanitizableFilename { .. })
        ));
    }

    #[test]
    fn insert_deleted_marker_with_and_without_extension() {
        assert_eq!(
            insert_deleted_marker("foo.txt", 67),
            "foo (deleted-#67).txt"
        );
        assert_eq!(
            insert_deleted_marker("Makefile", 42),
            "Makefile (deleted-#42)"
        );
        // 複数 `.` の場合は最後を拡張子と見なす。
        assert_eq!(
            insert_deleted_marker("archive.tar.gz", 7),
            "archive.tar (deleted-#7).gz"
        );
    }
}
