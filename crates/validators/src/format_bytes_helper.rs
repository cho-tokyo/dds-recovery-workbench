//! Chunk 23.8: バイト数を人間可読文字列に整形するヘルパー（validators 内蔵版）。
//!
//! `report` クレートの `format_bytes` と同等の表記。validators は他クレートに
//! 依存しないため (lib.rs の依存方向設計) 内蔵で持つ。

/// バイト数を `1.23 MB` のような人間可読表記に整形する。
///
/// 単位は 1024 進法 (KB = 1024 bytes)。
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_small_uses_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn format_bytes_kb_for_kilobyte_range() {
        assert!(format_bytes(2048).contains("KB"));
    }

    #[test]
    fn format_bytes_mb_for_megabyte_range() {
        assert!(format_bytes(2 * 1024 * 1024).contains("MB"));
    }
}
