//! 対話形式の入力ヘルパー群。
//!
//! いずれも標準入力 (stdin) から 1 行読み、`trim` した文字列をベースに処理する。
//! 不正な入力は再プロンプトする（数値・案件番号）。
//!
//! テスト時は stdin のモックが複雑なため、`normalize_yes_no` や
//! `try_parse_number_in_range` のような純関数だけを抽出して検証する。

use anyhow::Result;
use std::io::{self, Write};

use dds_case_manager::CaseId;

/// 文字列入力を求める。プロンプトと末尾 `: ` を表示して 1 行受け取る。
pub fn prompt_string(message: &str) -> Result<String> {
    print!("{}: ", message);
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

/// 数値入力を求める。`min..=max` の範囲外なら再プロンプト。
pub fn prompt_number(message: &str, min: usize, max: usize) -> Result<usize> {
    loop {
        let input = prompt_string(&format!("{} ({}-{})", message, min, max))?;
        match try_parse_number_in_range(&input, min, max) {
            Ok(n) => return Ok(n),
            Err(msg) => println!("  {}", msg),
        }
    }
}

/// Yes/No 確認を求める (デフォルト Yes)。空 Enter / y / yes (大小区別なし) で true。
pub fn confirm(message: &str) -> Result<bool> {
    let input = prompt_string(&format!("{} [Y/n]", message))?;
    Ok(normalize_yes_no(&input))
}

/// 案件番号入力 (`yymmdd-NN` 形式) を求める。形式不正なら再プロンプト。
pub fn prompt_case_id() -> Result<CaseId> {
    loop {
        let input = prompt_string("案件番号 (yymmdd-NN 形式、例: 260522-04)")?;
        match CaseId::parse(&input) {
            Ok(id) => return Ok(id),
            Err(e) => println!("  入力エラー: {}", e),
        }
    }
}

/// `prompt_number` の純粋な範囲チェック部分。テスト容易性のため切り出し。
fn try_parse_number_in_range(input: &str, min: usize, max: usize) -> Result<usize, String> {
    match input.parse::<usize>() {
        Ok(n) if n >= min && n <= max => Ok(n),
        Ok(_) => Err(format!(
            "範囲外です。{}-{} の値を入力してください。",
            min, max
        )),
        Err(_) => Err("数値として読み取れませんでした。".to_string()),
    }
}

/// `confirm` の純粋な判定部分。空文字または `y` / `yes` (大小区別なし) で true。
fn normalize_yes_no(input: &str) -> bool {
    let lower = input.trim().to_lowercase();
    lower.is_empty() || lower == "y" || lower == "yes"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_parse_number_in_range_accepts_valid_value() {
        assert_eq!(try_parse_number_in_range("3", 1, 5).unwrap(), 3);
        assert_eq!(try_parse_number_in_range("1", 1, 5).unwrap(), 1);
        assert_eq!(try_parse_number_in_range("5", 1, 5).unwrap(), 5);
    }

    #[test]
    fn try_parse_number_in_range_rejects_out_of_range() {
        assert!(try_parse_number_in_range("0", 1, 5).is_err());
        assert!(try_parse_number_in_range("6", 1, 5).is_err());
    }

    #[test]
    fn try_parse_number_in_range_rejects_non_numeric() {
        let err = try_parse_number_in_range("abc", 1, 5).unwrap_err();
        assert!(err.contains("数値"));
    }

    #[test]
    fn normalize_yes_no_defaults_to_true_on_empty() {
        assert!(normalize_yes_no(""));
        assert!(normalize_yes_no("   "));
    }

    #[test]
    fn normalize_yes_no_accepts_y_and_yes_case_insensitive() {
        assert!(normalize_yes_no("y"));
        assert!(normalize_yes_no("Y"));
        assert!(normalize_yes_no("yes"));
        assert!(normalize_yes_no("YES"));
    }

    #[test]
    fn normalize_yes_no_rejects_other_inputs() {
        assert!(!normalize_yes_no("n"));
        assert!(!normalize_yes_no("no"));
        assert!(!normalize_yes_no("nope"));
        assert!(!normalize_yes_no("0"));
    }
}
