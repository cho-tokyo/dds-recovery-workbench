//! Chunk 20.5: 業務レポート用の人間可読フォーマッタ。
//!
//! - [`format_bytes`]: バイト数を `B / KB / MB / GB / TB / PB` に自動切替（1024 ベース）。
//! - [`format_duration_ms`]: ミリ秒を「秒 / 分秒 / 時分秒」表記に変換。
//!
//! いずれも `&str` で返さず `String` を返すため、レポート文字列組み立てに直接使える。
//!
//! 関連 FR: FR-REP-04 (業務指標可視化), FR-REP-05 (大規模ファイル対応)。

/// バイト数を人間可読な形式に変換する（1024 ベース、小数点 2 桁固定）。
///
/// 1024 未満は `"127 B"` のように単位 B のみ。それ以上は KB→MB→GB→TB→PB の順に切り替わる。
///
/// # Examples
///
/// ```
/// use dds_report::format_bytes;
///
/// assert_eq!(format_bytes(127), "127 B");
/// assert_eq!(format_bytes(5_572), "5.44 KB");
/// assert_eq!(format_bytes(7_529_840), "7.18 MB");
/// assert_eq!(format_bytes(2_147_483_648), "2.00 GB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];

    if bytes < 1024 {
        return format!("{} B", bytes);
    }

    let mut value = bytes as f64;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", value, UNITS[unit_idx])
}

/// ミリ秒を人間可読な時間表現に変換する。
///
/// - 60 秒未満: `"12.30 秒"`
/// - 1 時間未満: `"1 分 5 秒"`
/// - 1 時間以上: `"1 時間 2 分 5 秒"`
/// - 負値: `"0 秒"` （業務的に未来→過去の差分は無効、防御的に 0 扱い）
///
/// # Examples
///
/// ```
/// use dds_report::format_duration_ms;
///
/// assert_eq!(format_duration_ms(12_300), "12.30 秒");
/// assert_eq!(format_duration_ms(65_000), "1 分 5 秒");
/// assert_eq!(format_duration_ms(3_725_000), "1 時間 2 分 5 秒");
/// ```
pub fn format_duration_ms(ms: i64) -> String {
    if ms < 0 {
        return "0 秒".to_string();
    }
    let total_seconds = ms / 1000;

    if total_seconds < 60 {
        return format!("{:.2} 秒", (ms as f64) / 1000.0);
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{} 時間 {} 分 {} 秒", hours, minutes, seconds)
    } else {
        format!("{} 分 {} 秒", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_under_1024_shows_bytes() {
        assert_eq!(format_bytes(127), "127 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        // 5572 / 1024 = 5.4414... → 5.44 KB
        assert_eq!(format_bytes(5_572), "5.44 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        // 7_529_840 / 1024 / 1024 = 7.1810... → 7.18 MB
        assert_eq!(format_bytes(7_529_840), "7.18 MB");
    }

    #[test]
    fn format_bytes_gigabytes() {
        // 2 * 1024^3 = 2_147_483_648 → 2.00 GB
        assert_eq!(format_bytes(2_147_483_648), "2.00 GB");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration_ms(229), "0.23 秒");
        assert_eq!(format_duration_ms(12_300), "12.30 秒");
        // 59500 ms → 59 秒分岐 → 59.50 秒
        assert_eq!(format_duration_ms(59_500), "59.50 秒");
    }

    #[test]
    fn format_duration_minutes() {
        // 65 秒 = 1 分 5 秒
        assert_eq!(format_duration_ms(65_000), "1 分 5 秒");
    }

    #[test]
    fn format_duration_hours() {
        // 3725 秒 = 1 時間 2 分 5 秒
        assert_eq!(format_duration_ms(3_725_000), "1 時間 2 分 5 秒");
    }

    #[test]
    fn format_duration_negative_returns_zero_seconds() {
        // 業務防御: 開始時刻と終了時刻が逆転した不正データでもパニックせず "0 秒" を返す。
        assert_eq!(format_duration_ms(-100), "0 秒");
    }
}
