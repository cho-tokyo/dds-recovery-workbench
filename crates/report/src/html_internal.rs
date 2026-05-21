//! Chunk 20: CS 内部業務向け HTML レポート生成。
//!
//! 顧客向けに加え、CS の業務判断補助となる詳細情報を含む:
//! - `internal_note_ja` (CS 内部メモ)
//! - SHA256 ハッシュ
//! - 出力先パス
//! - 優先度スコア
//! - source_id（MFT エントリ番号等）
//! - 失敗・スキップエントリの詳細
//!
//! 顧客に共有しないよう、レポート冒頭に警告文を必ず付与する。
//!
//! 関連 FR: FR-REP-02 (内部業務管理レポート出力)。

use chrono::Local;
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::escape::escape_html;

/// CS 向け HTML レポートを生成する。
///
/// 顧客向けと違い、`internal_note_ja` / `sha256` / `output_path` / `source_id`
/// 等の詳細情報を全て含む。冒頭に「お客様に共有しないでください」の警告を表示。
pub fn render_internal_html(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut html = String::with_capacity(16384);
    let now = Local::now();

    html.push_str(&format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <title>復旧業務レポート (CS 内部用) - {date}</title>
  <style>
    body {{ font-family: "Yu Gothic", "Hiragino Sans", sans-serif; max-width: 1400px; margin: 1em auto; padding: 1em; font-size: 13px; }}
    h1 {{ border-bottom: 3px solid #1e40af; padding-bottom: 0.3em; color: #1e3a8a; }}
    h2 {{ color: #1e40af; margin-top: 1.5em; }}
    .summary {{ background: #eff6ff; padding: 1em 1.5em; border-radius: 4px; border-left: 4px solid #1e40af; }}
    .warning {{ background: #fef2f2; padding: 0.5em 1em; border-left: 4px solid #dc2626; margin: 1em 0; font-weight: bold; color: #991b1b; }}
    table {{ width: 100%; border-collapse: collapse; margin-top: 0.5em; font-size: 12px; }}
    th, td {{ padding: 0.4em 0.5em; text-align: left; border-bottom: 1px solid #ddd; vertical-align: top; }}
    th {{ background: #1e40af; color: white; }}
    tr.valid {{ background: #f0fdf4; }}
    tr.invalid {{ background: #fef2f2; }}
    tr.uncertain {{ background: #fffbeb; }}
    .note {{ color: #6b7280; font-size: 11px; font-style: italic; }}
    .sha {{ font-family: monospace; font-size: 10px; color: #6b7280; word-break: break-all; }}
    .badge {{ display: inline-block; padding: 0.15em 0.5em; border-radius: 3px; font-size: 11px; font-weight: bold; }}
    .badge-valid {{ background: #16a34a; color: white; }}
    .badge-invalid {{ background: #dc2626; color: white; }}
    .badge-uncertain {{ background: #d97706; color: white; }}
    footer {{ margin-top: 2em; color: #666; font-size: 0.8em; }}
  </style>
</head>
<body>
  <h1>復旧業務レポート（CS 内部用）</h1>
  <p>作成日時: {datetime}</p>

  <div class="warning">
    [注意] このレポートは社内業務用です。CS 内部メモを含むため、お客様に共有しないでください。
  </div>
"#,
        date = now.format("%Y-%m-%d %H:%M"),
        datetime = now.format("%Y-%m-%d %H:%M:%S"),
    ));

    // サマリ
    let uncertain_count = report
        .recovered
        .len()
        .saturating_sub(report.validated_count() + report.invalid_count());

    html.push_str(&format!(
        r#"  <section class="summary">
    <h2>サマリ</h2>
    <table>
      <tr><th>項目</th><th>値</th></tr>
      <tr><td>マッチ総数</td><td>{matched}</td></tr>
      <tr><td>復旧成功</td><td>{rec} ({rate:.1}%)</td></tr>
      <tr><td>復旧失敗</td><td>{fail}</td></tr>
      <tr><td>スキップ</td><td>{skip}</td></tr>
      <tr><td>処理時間</td><td>{dur} ms</td></tr>
      <tr><td>復旧バイト総量</td><td>{bytes} bytes</td></tr>
    </table>

    <h2>品質判定内訳</h2>
    <table>
      <tr><th>判定</th><th>件数</th></tr>
      <tr><td>Valid</td><td>{v}</td></tr>
      <tr><td>Invalid</td><td>{i}</td></tr>
      <tr><td>Uncertain</td><td>{u}</td></tr>
    </table>
  </section>
"#,
        matched = report.total_matched,
        rec = report.recovered.len(),
        rate = report.success_rate(),
        fail = report.failed.len(),
        skip = report.skipped.len(),
        dur = report.duration_ms(),
        bytes = report.total_bytes_written(),
        v = report.validated_count(),
        i = report.invalid_count(),
        u = uncertain_count,
    ));

    // 復旧ファイル一覧（CS 詳細）
    html.push_str(
        r#"  <section>
    <h2>復旧ファイル一覧（CS 詳細）</h2>
    <table>
      <thead>
        <tr>
          <th>source_id</th>
          <th>パス</th>
          <th>サイズ</th>
          <th>優先度</th>
          <th>判定</th>
          <th>顧客向けメッセージ</th>
          <th>CS 内部メモ</th>
          <th>出力先</th>
          <th>SHA256</th>
        </tr>
      </thead>
      <tbody>
"#,
    );

    for entry in &report.recovered {
        let (row_class, badge_class, badge_label) = match entry.validation.as_ref() {
            Some(v) => match v.status {
                ValidationStatus::Valid => ("valid", "badge-valid", "Valid"),
                ValidationStatus::Invalid => ("invalid", "badge-invalid", "Invalid"),
                ValidationStatus::Uncertain => ("uncertain", "badge-uncertain", "Uncertain"),
            },
            None => ("uncertain", "badge-uncertain", "-"),
        };

        let customer_msg = entry
            .validation
            .as_ref()
            .map(|v| v.customer_message())
            .unwrap_or_else(|| "-".to_string());
        let internal_note = entry
            .validation
            .as_ref()
            .and_then(|v| v.internal_note().map(|s| s.to_string()))
            .unwrap_or_else(|| "-".to_string());
        let sha = entry.sha256.as_deref().unwrap_or("-");

        html.push_str(&format!(
            r#"        <tr class="{cls}">
          <td>{src}</td>
          <td>{path}</td>
          <td>{bytes}</td>
          <td>{prio}</td>
          <td><span class="badge {bc}">{bl}</span></td>
          <td>{cust}</td>
          <td class="note">{note}</td>
          <td>{out}</td>
          <td class="sha">{sha}</td>
        </tr>
"#,
            cls = row_class,
            src = escape_html(&entry.source_id),
            path = escape_html(&entry.original_path),
            bytes = entry.bytes_written,
            prio = entry.priority_score,
            bc = badge_class,
            bl = badge_label,
            cust = escape_html(&customer_msg),
            note = escape_html(&internal_note),
            out = escape_html(&entry.output_path.display().to_string()),
            sha = escape_html(sha),
        ));
    }

    html.push_str(
        r#"      </tbody>
    </table>
  </section>
"#,
    );

    // 失敗エントリ
    if !report.failed.is_empty() {
        html.push_str(
            r#"  <section>
    <h2>失敗ファイル</h2>
    <table>
      <thead><tr><th>source_id</th><th>パス</th><th>エラー</th></tr></thead>
      <tbody>
"#,
        );
        for entry in &report.failed {
            html.push_str(&format!(
                "        <tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape_html(&entry.source_id),
                escape_html(&entry.original_path),
                escape_html(&entry.error_message),
            ));
        }
        html.push_str("      </tbody>\n    </table>\n  </section>\n");
    }

    // スキップエントリ
    if !report.skipped.is_empty() {
        html.push_str(
            r#"  <section>
    <h2>スキップファイル</h2>
    <table>
      <thead><tr><th>source_id</th><th>パス</th><th>理由</th></tr></thead>
      <tbody>
"#,
        );
        for entry in &report.skipped {
            html.push_str(&format!(
                "        <tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                escape_html(&entry.source_id),
                escape_html(&entry.original_path),
                escape_html(&entry.reason),
            ));
        }
        html.push_str("      </tbody>\n    </table>\n  </section>\n");
    }

    html.push_str(
        r#"  <footer>
    <p>DDS Recovery Workbench - 内部業務レポート</p>
  </footer>
</body>
</html>
"#,
    );

    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::ValidationResult;
    use std::path::PathBuf;

    fn report_with_one_invalid_entry() -> RecoveryReport {
        let now = Utc::now();
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG ファイルではないようです",
            "拡張子嘘の典型例。再復旧推奨",
        );
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 1,
            recovered: vec![RecoveredEntry {
                source_id: "NTFS#42".into(),
                original_path: "\\bad.png".into(),
                output_path: PathBuf::from("/output/bad.png"),
                bytes_written: 250,
                priority_score: 80,
                is_deleted: true,
                sha256: Some("abcd1234".repeat(8)),
                validation: Some(validation),
            }],
            failed: vec![],
            skipped: vec![],
        }
    }

    #[test]
    fn internal_html_includes_internal_note_ja() {
        let html = render_internal_html(&report_with_one_invalid_entry()).unwrap();
        assert!(
            html.contains("拡張子嘘の典型例"),
            "CS 内部メモが含まれること"
        );
        assert!(html.contains("再復旧推奨"));
        // 顧客向けメッセージも併存
        assert!(html.contains("PNG ファイルではないようです"));
    }

    #[test]
    fn internal_html_includes_sha256_output_path_and_source_id() {
        let html = render_internal_html(&report_with_one_invalid_entry()).unwrap();
        let expected_sha = "abcd1234".repeat(8);
        assert!(html.contains(&expected_sha), "SHA256 が含まれること");
        assert!(html.contains("NTFS#42"), "source_id が含まれること");
        // パスは OS により表現が変わる可能性があるためサブストリングで確認
        assert!(
            html.contains("output") && html.contains("bad.png"),
            "出力先パスが含まれること: {}",
            html.chars().take(2000).collect::<String>()
        );
    }

    #[test]
    fn internal_html_warns_not_to_share_with_customer() {
        let html = render_internal_html(&report_with_one_invalid_entry()).unwrap();
        assert!(
            html.contains("お客様に共有しないでください")
                || html.contains("お客様に共有"),
            "警告文が含まれること"
        );
        assert!(html.contains("CS 内部用") || html.contains("内部業務"));
    }
}
