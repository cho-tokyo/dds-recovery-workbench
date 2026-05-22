//! 業務レポート / CRM テキスト共通の人間可読フォーマッタ。
//!
//! Chunk 20.5 で `dds-report::format::format_bytes` として実装したものを
//! Chunk 22 で `dds-core` に格上げ移動。診断エンジン（`dds-diagnostic`）から
//! report への上向き依存を避けつつ、同一ロジックを 1 箇所で集中管理する。
//!
//! - [`format_bytes`]: バイト数を `B / KB / MB / GB / TB / PB` に自動切替（1024 ベース）。
//!
//! 関連 FR: FR-REP-04 (業務指標可視化), FR-REP-05 (大規模ファイル対応),
//!         FR-DIAG-04 (CRM 貼り付け用テキスト)。

/// バイト数を人間可読な形式に変換する（1024 ベース、小数点 2 桁固定）。
///
/// 1024 未満は `"127 B"` のように単位 B のみ。それ以上は KB→MB→GB→TB→PB の順に切り替わる。
///
/// # Examples
///
/// ```
/// use dds_core::format::format_bytes;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_under_1024_shows_bytes() {
        assert_eq!(format_bytes(127), "127 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_zero_returns_zero_bytes() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_kilobytes_rounds_to_two_decimals() {
        // 5572 / 1024 = 5.4414... → 5.44 KB
        assert_eq!(format_bytes(5_572), "5.44 KB");
    }

    #[test]
    fn format_bytes_megabytes_correct() {
        // 7_529_840 / 1024 / 1024 = 7.1810... → 7.18 MB
        assert_eq!(format_bytes(7_529_840), "7.18 MB");
    }

    #[test]
    fn format_bytes_gigabytes_correct() {
        // 2 * 1024^3 = 2_147_483_648 → 2.00 GB
        assert_eq!(format_bytes(2_147_483_648), "2.00 GB");
    }

    #[test]
    fn format_bytes_terabytes_correct() {
        // 1 TiB
        assert_eq!(format_bytes(1_099_511_627_776), "1.00 TB");
    }
}
