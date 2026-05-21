//! Chunk 20: 顧客向け HTML レポート生成。
//!
//! `RecoveryReport` から、お客様に納品可能な HTML を生成する。
//!
//! **設計原則（最重要）**: `internal_note_ja` を**絶対に**含めない。
//! 業務的にお客様に共有してはならない内部メモが漏れることを機械テストで防ぐ。
//!
//! 含む情報:
//! - サマリ（件数）
//! - 各ファイルのパス・サイズ・状態バッジ・顧客向けメッセージ
//!
//! 含まない情報:
//! - `internal_note_ja` (CS 内部メモ)
//! - 技術詳細 (`diagnostics`)
//! - SHA256
//! - 出力先パス
//! - source_id（MFT エントリ番号等）
//!
//! 関連 FR: FR-REP-01 (顧客向け復旧レポート出力)。

use chrono::Local;
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::escape::escape_html;

/// 顧客向け HTML レポートを生成する。
///
/// 戻り値は単一の HTML 文字列（`<!DOCTYPE html>` 始まり）。
/// `lang="ja"` 属性付与、全 CSS インライン、外部 CSS/JS なし。
///
/// XSS 防止のためファイル名・メッセージは全て [`escape_html`] でエスケープ。
pub fn render_customer_html(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut html = String::with_capacity(8192);
    let now = Local::now();

    html.push_str(&format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <title>データ復旧レポート - {date}</title>
  <style>
    body {{ font-family: "Yu Gothic", "Hiragino Sans", sans-serif; max-width: 900px; margin: 2em auto; padding: 1em; color: #333; }}
    h1 {{ border-bottom: 3px solid #2c5aa0; padding-bottom: 0.3em; }}
    h2 {{ color: #2c5aa0; margin-top: 2em; border-left: 4px solid #2c5aa0; padding-left: 0.5em; }}
    .summary {{ background: #f5f7fa; padding: 1em 1.5em; border-radius: 4px; }}
    .summary ul {{ list-style: none; padding: 0; }}
    .summary li {{ padding: 0.3em 0; }}
    table {{ width: 100%; border-collapse: collapse; margin-top: 1em; }}
    th, td {{ padding: 0.6em; text-align: left; border-bottom: 1px solid #ddd; }}
    th {{ background: #f0f3f7; }}
    tr.valid {{ background: #f0fdf4; }}
    tr.invalid {{ background: #fef2f2; }}
    tr.uncertain {{ background: #fffbeb; }}
    .badge {{ display: inline-block; padding: 0.2em 0.5em; border-radius: 3px; font-size: 0.85em; font-weight: bold; }}
    .badge-valid {{ background: #16a34a; color: white; }}
    .badge-invalid {{ background: #dc2626; color: white; }}
    .badge-uncertain {{ background: #d97706; color: white; }}
    footer {{ margin-top: 3em; color: #666; font-size: 0.85em; text-align: center; }}
  </style>
</head>
<body>
  <h1>データ復旧レポート</h1>
  <p>作成日時: {datetime}</p>
"#,
        date = now.format("%Y-%m-%d"),
        datetime = now.format("%Y年%m月%d日 %H:%M"),
    ));

    // サマリ
    let valid_count = report.validated_count();
    let invalid_count = report.invalid_count();
    let uncertain_count = report
        .recovered
        .len()
        .saturating_sub(valid_count + invalid_count);

    html.push_str(&format!(
        r#"  <section class="summary">
    <h2>概要</h2>
    <ul>
      <li><strong>復旧対象ファイル:</strong> {} 件</li>
      <li><strong>復旧成功:</strong> {} 件</li>
      <li><strong>品質確認済み:</strong> {} 件</li>
      <li><strong>要確認:</strong> {} 件</li>
      <li><strong>自動検証対象外:</strong> {} 件</li>
    </ul>
  </section>
"#,
        report.total_matched,
        report.recovered.len(),
        valid_count,
        invalid_count,
        uncertain_count,
    ));

    // ファイル一覧
    html.push_str(
        r#"  <section class="files">
    <h2>復旧ファイル一覧</h2>
    <table>
      <thead>
        <tr><th>パス</th><th>サイズ</th><th>状態</th><th>備考</th></tr>
      </thead>
      <tbody>
"#,
    );

    for entry in &report.recovered {
        let (row_class, badge_class, badge_label) = match entry.validation.as_ref() {
            Some(v) => match v.status {
                ValidationStatus::Valid => ("valid", "badge-valid", "正常"),
                ValidationStatus::Invalid => ("invalid", "badge-invalid", "要確認"),
                ValidationStatus::Uncertain => ("uncertain", "badge-uncertain", "検証外"),
            },
            None => ("uncertain", "badge-uncertain", "未検証"),
        };

        // 顧客向けメッセージのみ（internal_note は絶対使わない）。
        let message = entry
            .validation
            .as_ref()
            .map(|v| v.customer_message())
            .unwrap_or_else(|| "検証情報なし".to_string());

        html.push_str(&format!(
            r#"        <tr class="{cls}">
          <td>{path}</td>
          <td>{bytes} B</td>
          <td><span class="badge {badge_cls}">{badge}</span></td>
          <td>{msg}</td>
        </tr>
"#,
            cls = row_class,
            path = escape_html(&entry.original_path),
            bytes = entry.bytes_written,
            badge_cls = badge_class,
            badge = badge_label,
            msg = escape_html(&message),
        ));
    }

    html.push_str(
        r#"      </tbody>
    </table>
  </section>
  <footer>
    <p>本レポートは DDS Recovery Workbench により自動生成されました。</p>
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

    fn empty_report() -> RecoveryReport {
        let now = Utc::now();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched: 0,
            recovered: vec![],
            failed: vec![],
            skipped: vec![],
        }
    }

    fn entry_with_validation(path: &str, validation: Option<ValidationResult>) -> RecoveredEntry {
        RecoveredEntry {
            source_id: "NTFS#1".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/tmp/out"),
            bytes_written: 100,
            priority_score: 10,
            is_deleted: false,
            sha256: Some("deadbeef".repeat(8)),
            validation,
        }
    }

    #[test]
    fn customer_html_includes_user_message_ja() {
        let mut report = empty_report();
        let validation = ValidationResult::valid(
            "PNG",
            "png_v1",
            vec!["magic OK".into()],
            "PNG 画像として正常です",
            None,
        );
        report.recovered.push(entry_with_validation(
            "\\photos\\img_001.png",
            Some(validation),
        ));
        report.total_matched = 1;

        let html = render_customer_html(&report).unwrap();
        assert!(html.contains("PNG 画像として正常です"));
        assert!(html.contains("\\photos\\img_001.png"));
    }

    #[test]
    fn customer_html_excludes_internal_note_ja() {
        // 最重要: internal_note_ja が顧客 HTML に含まれないこと。
        let mut report = empty_report();
        let validation = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG ファイルではないようです",
            "拡張子嘘の典型例。再復旧推奨",
        );
        report
            .recovered
            .push(entry_with_validation("\\bad.png", Some(validation)));
        report.total_matched = 1;

        let html = render_customer_html(&report).unwrap();
        assert!(html.contains("PNG ファイルではないようです"), "顧客向けは含む");
        assert!(
            !html.contains("再復旧推奨"),
            "CS 内部メモは顧客 HTML に含まれてはならない"
        );
        assert!(!html.contains("拡張子嘘の典型例"));
    }

    #[test]
    fn customer_html_excludes_sha256_and_output_path() {
        // SHA256 と出力先パスは顧客 HTML には含めない。
        let mut report = empty_report();
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let mut entry = entry_with_validation("\\x.png", Some(validation));
        entry.sha256 = Some("abcdef1234567890".to_string());
        entry.output_path = PathBuf::from("/secret/output/path/x.png");
        report.recovered.push(entry);
        report.total_matched = 1;

        let html = render_customer_html(&report).unwrap();
        assert!(!html.contains("abcdef1234567890"), "SHA256 を含むべきでない");
        assert!(!html.contains("/secret/output"), "出力先パスを含むべきでない");
    }

    #[test]
    fn customer_html_escapes_special_chars() {
        // XSS 防止: ファイル名に `<script>` を含むケース。
        let mut report = empty_report();
        let validation = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        report.recovered.push(entry_with_validation(
            "\\<script>alert(1)</script>.png",
            Some(validation),
        ));
        report.total_matched = 1;

        let html = render_customer_html(&report).unwrap();
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn customer_html_has_ja_lang_attribute() {
        let html = render_customer_html(&empty_report()).unwrap();
        assert!(html.contains(r#"<html lang="ja">"#));
        assert!(html.contains(r#"<meta charset="UTF-8">"#));
    }
}
