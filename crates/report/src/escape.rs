//! Chunk 20: HTML エスケープヘルパ。
//!
//! 顧客・CS 向けレポートでファイル名やメッセージを HTML に埋め込む際の
//! XSS 防止用エスケープ関数を提供する。外部依存を増やさないため自前実装。
//!
//! 関連 FR: FR-REP-01, FR-REP-02（HTML 出力時の安全性）。

/// HTML 特殊文字をエスケープする。
///
/// 対象文字: `<` `>` `&` `"` `'`。
/// 日本語等のマルチバイト文字はそのまま通過する。
///
/// # Example
///
/// ```
/// use dds_report::escape::escape_html;
///
/// assert_eq!(escape_html("<script>"), "&lt;script&gt;");
/// assert_eq!(escape_html("a&b"), "a&amp;b");
/// assert_eq!(escape_html("日本語"), "日本語");
/// ```
pub fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_handles_all_special_chars() {
        // 5 種特殊文字すべて変換されること。
        assert_eq!(escape_html("<"), "&lt;");
        assert_eq!(escape_html(">"), "&gt;");
        assert_eq!(escape_html("&"), "&amp;");
        assert_eq!(escape_html("\""), "&quot;");
        assert_eq!(escape_html("'"), "&#39;");
        assert_eq!(
            escape_html("<a href=\"x'y&z\">"),
            "&lt;a href=&quot;x&#39;y&amp;z&quot;&gt;"
        );
    }

    #[test]
    fn escape_html_passes_through_japanese_and_ascii() {
        // 日本語マルチバイトや ASCII の英数字はそのまま通過。
        assert_eq!(escape_html("日本語"), "日本語");
        assert_eq!(escape_html("abc 123"), "abc 123");
        assert_eq!(escape_html("写真_001.png"), "写真_001.png");
        assert_eq!(escape_html(""), "");
    }

    #[test]
    fn escape_html_prevents_basic_xss() {
        // XSS 防止の代表例: script タグや属性 injection。
        let payload = "<script>alert('x')</script>";
        let escaped = escape_html(payload);
        assert!(!escaped.contains("<script>"));
        assert!(!escaped.contains("</script>"));
        assert!(escaped.contains("&lt;script&gt;"));
    }
}
