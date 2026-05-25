//! Chunk 20 / 20.5: CS 内部業務向け HTML レポート生成（業務適用版）。
//!
//! Chunk 20.5 で大幅再設計:
//! - 「該当ファイル数 / 復旧成功率 / 品質保証率」の 3 段階業務指標
//! - 形式別ブレイクダウン（件数 + Valid 比率）
//! - Invalid を「形式 + 主要メッセージ冒頭」でグルーピングして、各グループ最大 20 件表示
//! - バイト数を人間可読、処理時間も人間可読
//!
//! 顧客に共有しないよう、レポート冒頭に警告文を必ず付与する。
//!
//! 関連 FR: FR-REP-02 (内部業務管理レポート出力), FR-REP-04 (業務指標可視化),
//! FR-REP-05 (大規模ファイル対応)。

use chrono::Local;

use dds_recovery::RecoveryReport;

use crate::error::ReportError;
use crate::escape::escape_html;
use crate::format::{format_bytes, format_duration_ms};

/// 1 つの Invalid グループで HTML に詳細表示する最大件数。
/// これを超えた分は「... 他 N 件（詳細は CSV を参照）」と省略する。
const MAX_INVALID_PER_GROUP: usize = 20;

/// CS 向け HTML レポートを生成する（業務適用版）。
///
/// 顧客向けと違い、`internal_note_ja` / `sha256` 等の内部情報を含む。
/// 冒頭に「お客様に共有しないでください」の警告を必ず付与。
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
    th, td {{ padding: 0.4em 0.6em; text-align: left; border-bottom: 1px solid #ddd; vertical-align: top; }}
    th {{ background: #1e40af; color: white; }}
    .metric {{ font-size: 1.5em; font-weight: bold; color: #1e3a8a; }}
    .ratio {{ color: #6b7280; font-size: 0.85em; }}
    .invalid-group {{ margin: 1em 0; padding: 0.8em; background: #fef2f2; border-left: 4px solid #dc2626; border-radius: 4px; }}
    .invalid-group h3 {{ margin: 0 0 0.5em 0; color: #991b1b; font-size: 14px; }}
    .invalid-group ul {{ margin: 0; padding-left: 1.5em; font-family: monospace; font-size: 11px; }}
    .note {{ color: #6b7280; font-size: 11px; font-style: italic; }}
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

    // === サマリ（業務指標） ===
    let valid = report.validated_count();
    let invalid = report.invalid_count();
    let uncertain = report.uncertain_count();
    let recovered_n = report.recovered.len();

    html.push_str("  <section class=\"summary\">\n");

    // 復旧対象 (ご指定条件 + 該当ファイル数)
    html.push_str("    <h2>復旧対象</h2>\n    <table>\n");
    if !report.wish_labels.is_empty() {
        html.push_str("      <tr><th>ご指定条件</th><td>");
        for (i, label) in report.wish_labels.iter().enumerate() {
            if i > 0 {
                html.push_str("<br>");
            }
            html.push_str(&format!("「{}」", escape_html(label)));
        }
        html.push_str("</td></tr>\n");
    }
    html.push_str(&format!(
        "      <tr><th>該当ファイル数</th><td><span class=\"metric\">{}</span> 件</td></tr>\n    </table>\n",
        report.total_matched,
    ));

    // 復旧結果 (全体) - Chunk 23.7 で「全体」ラベル明示
    html.push_str(&format!(
        "    <h2>復旧結果 (全体)</h2>\n    <table>\n\
         <tr><th>復旧成功</th><td><span class=\"metric\">{rec}</span> 件 <span class=\"ratio\">(該当の {rate:.1}%)</span></td></tr>\n\
         <tr><th>復旧失敗</th><td>{fail} 件</td></tr>\n\
         <tr><th>スキップ</th><td>{skip} 件</td></tr>\n\
         </table>\n",
        rec = recovered_n,
        rate = report.recovery_success_rate(),
        fail = report.failed.len(),
        skip = report.skipped.len(),
    ));

    // 品質判定内訳
    html.push_str(&format!(
        "    <h2>品質判定内訳</h2>\n    <table>\n\
         <tr><th>判定</th><th>件数</th><th>比率</th></tr>\n\
         <tr><td>Valid (品質確認済み)</td><td>{v}</td><td>{vr:.1}%</td></tr>\n\
         <tr><td>Invalid (要確認)</td><td>{i}</td><td>{ir:.1}%</td></tr>\n\
         <tr><td>Uncertain (検証外)</td><td>{u}</td><td>{ur:.1}%</td></tr>\n\
         </table>\n\
         <p><strong>品質保証率: <span class=\"metric\">{qa:.1}%</span></strong> (復旧成功のうち Valid の比率)</p>\n",
        v = valid,
        vr = ratio_safe(valid, recovered_n),
        i = invalid,
        ir = ratio_safe(invalid, recovered_n),
        u = uncertain,
        ur = ratio_safe(uncertain, recovered_n),
        qa = report.quality_assurance_rate(),
    ));

    // データ量と時間
    html.push_str(&format!(
        "    <h2>データ量と時間</h2>\n    <table>\n\
         <tr><th>復旧総量</th><td>{bytes}</td></tr>\n\
         <tr><th>処理時間</th><td>{dur}</td></tr>\n\
         </table>\n  </section>\n",
        bytes = escape_html(&format_bytes(report.total_bytes_written())),
        dur = escape_html(&format_duration_ms(report.duration_ms())),
    ));

    // === Chunk 23.7: お客様優先データセクション ===
    // priority_count == 0 のとき（Wishlist が空 or マッチなし）は省略。
    let priority_count = report.priority_count();
    if priority_count > 0 {
        html.push_str(&format!(
            "  <h2>お客様優先データ (Wishlist マッチ)</h2>\n  <table>\n\
             <tr><th>該当ファイル数</th><td><span class=\"metric\">{pc}</span> 件</td></tr>\n\
             <tr><th>復旧データ量</th><td>{bytes}</td></tr>\n\
             <tr><th>品質保証率</th><td><span class=\"metric\">{qa:.1}%</span></td></tr>\n\
             </table>\n  <table>\n\
             <tr><th>判定</th><th>件数</th></tr>\n\
             <tr><td>Valid (正常)</td><td>{v}</td></tr>\n\
             <tr><td>Invalid (要確認)</td><td>{i}</td></tr>\n\
             <tr><td>Uncertain (検証外)</td><td>{u}</td></tr>\n\
             </table>\n",
            pc = priority_count,
            bytes = escape_html(&format_bytes(report.priority_total_bytes())),
            qa = report.priority_quality_assurance_rate(),
            v = report.priority_validated_count(),
            i = report.priority_invalid_count(),
            u = report.priority_uncertain_count(),
        ));
    }

    // === 形式別ブレイクダウン ===
    let breakdown = report.format_breakdown();
    if !breakdown.is_empty() {
        html.push_str(
            "  <h2>形式別ブレイクダウン</h2>\n  <table>\n\
             <tr><th>形式</th><th>正常</th><th>要確認</th><th>検証外</th><th>合計</th><th>正常率</th></tr>\n",
        );
        for (format, stats) in &breakdown {
            html.push_str(&format!(
                "    <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.1}%</td></tr>\n",
                escape_html(format),
                stats.valid,
                stats.invalid,
                stats.uncertain,
                stats.total,
                stats.valid_ratio(),
            ));
        }
        html.push_str("  </table>\n");
    }

    // === Invalid グルーピング表示 ===
    let grouped = report.invalid_grouped_by_reason();
    if !grouped.is_empty() {
        html.push_str("  <h2>要確認ファイル (理由別)</h2>\n");
        for (reason, entries) in &grouped {
            html.push_str(&format!(
                "  <div class=\"invalid-group\">\n    <h3>{} ({} 件)</h3>\n    <ul>\n",
                escape_html(reason),
                entries.len(),
            ));
            for entry in entries.iter().take(MAX_INVALID_PER_GROUP) {
                let internal = entry
                    .validation
                    .as_ref()
                    .and_then(|v| v.internal_note().map(|s| s.to_string()))
                    .unwrap_or_else(|| "-".to_string());
                html.push_str(&format!(
                    "      <li>{} <span class=\"note\">[CS メモ: {}]</span></li>\n",
                    escape_html(&entry.original_path),
                    escape_html(&internal),
                ));
            }
            if entries.len() > MAX_INVALID_PER_GROUP {
                html.push_str(&format!(
                    "      <li class=\"note\">... 他 {} 件 (詳細は CSV を参照)</li>\n",
                    entries.len() - MAX_INVALID_PER_GROUP
                ));
            }
            html.push_str("    </ul>\n  </div>\n");
        }
    }

    // === 失敗・スキップ ===
    if !report.failed.is_empty() {
        html.push_str(
            "  <section>\n    <h2>失敗ファイル</h2>\n    <table>\n\
             <thead><tr><th>source_id</th><th>パス</th><th>エラー</th></tr></thead>\n      <tbody>\n",
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

    if !report.skipped.is_empty() {
        html.push_str(
            "  <section>\n    <h2>スキップファイル</h2>\n    <table>\n\
             <thead><tr><th>source_id</th><th>パス</th><th>理由</th></tr></thead>\n      <tbody>\n",
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
        "  <footer>\n    <p>DDS Recovery Workbench - 内部業務レポート</p>\n  </footer>\n</body>\n</html>\n",
    );

    Ok(html)
}

/// `num / denom * 100`、ただし `denom == 0` のとき `0.0`。
fn ratio_safe(num: usize, denom: usize) -> f64 {
    if denom == 0 {
        0.0
    } else {
        (num as f64) / (denom as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dds_recovery::{RecoveredEntry, RecoveryReport};
    use dds_validators::ValidationResult;
    use std::path::PathBuf;

    fn entry(path: &str, validation: Option<ValidationResult>) -> RecoveredEntry {
        RecoveredEntry {
            source_id: "NTFS#1".into(),
            original_path: path.into(),
            output_path: PathBuf::from("/tmp/out"),
            bytes_written: 1234,
            priority_score: 100,
            is_deleted: false,
            sha256: Some("aa".repeat(32)),
            validation,
            matched_wish_labels: vec![],
            is_priority: false,
        }
    }

    fn make_report(recovered: Vec<RecoveredEntry>, total_matched: usize) -> RecoveryReport {
        let now = Utc::now();
        RecoveryReport {
            started_at: now,
            finished_at: now,
            total_matched,
            recovered,
            failed: vec![],
            skipped: vec![],
            wish_labels: vec!["写真".into()],
        }
    }

    #[test]
    fn internal_html_warns_not_to_share_with_customer() {
        let html = render_internal_html(&make_report(vec![], 0)).unwrap();
        assert!(html.contains("お客様に共有しないでください"));
        assert!(html.contains("CS 内部"));
    }

    #[test]
    fn internal_html_includes_internal_note_for_invalid() {
        let v = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "magic mismatch",
            "PNG ファイルではないようです",
            "拡張子嘘の典型例。再復旧推奨",
        );
        let html =
            render_internal_html(&make_report(vec![entry("\\bad.png", Some(v))], 1)).unwrap();
        assert!(html.contains("拡張子嘘の典型例"));
        assert!(html.contains("再復旧推奨"));
        // 顧客向けメッセージも併存
        assert!(html.contains("PNG ファイルではないようです"));
    }

    #[test]
    fn internal_html_shows_recovery_success_rate() {
        // 7 件成功 / 10 件マッチ -> 70.0%
        let entries: Vec<_> = (0..7).map(|_| entry("\\x.png", None)).collect();
        let html = render_internal_html(&make_report(entries, 10)).unwrap();
        assert!(html.contains("70.0%"));
        assert!(html.contains("該当の"));
    }

    #[test]
    fn internal_html_shows_quality_assurance_rate() {
        // Valid 2 / 復旧 4 -> 50.0%
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let v_ng = ValidationResult::invalid("PNG", "png_v1", "x", "壊れ", "メモ");
        let entries = vec![
            entry("\\a.png", Some(v_ok.clone())),
            entry("\\b.png", Some(v_ok)),
            entry("\\c.png", Some(v_ng.clone())),
            entry("\\d.png", Some(v_ng)),
        ];
        let html = render_internal_html(&make_report(entries, 4)).unwrap();
        assert!(html.contains("品質保証率"));
        assert!(html.contains("50.0%"));
    }

    #[test]
    fn internal_html_groups_invalid_by_reason() {
        let v1 = ValidationResult::invalid(
            "PNG",
            "png_v1",
            "trailer",
            "末尾が欠けています",
            "IEND 欠損",
        );
        let v2 =
            ValidationResult::invalid("PNG", "png_v1", "magic", "PNG ではないようです", "拡張子嘘");
        let entries = vec![entry("\\a.png", Some(v1)), entry("\\b.png", Some(v2))];
        let html = render_internal_html(&make_report(entries, 2)).unwrap();
        // 2 つの distinct reason グループが invalid-group div として現れる。
        let group_count = html.matches("class=\"invalid-group\"").count();
        assert_eq!(group_count, 2, "two distinct invalid-reason groups");
    }

    #[test]
    fn internal_html_shows_priority_section_when_priority_present() {
        // Chunk 23.7: is_priority=true のエントリがあれば「お客様優先データ」セクションを表示。
        let v_ok = ValidationResult::valid("PNG", "png_v1", vec![], "OK", None);
        let mut priority_entry = entry("\\photo.png", Some(v_ok));
        priority_entry.is_priority = true;
        let html = render_internal_html(&make_report(vec![priority_entry], 1)).unwrap();
        assert!(html.contains("お客様優先データ"));
        assert!(html.contains("Wishlist マッチ"));
        // 全体ラベルも併記されること。
        assert!(html.contains("復旧結果 (全体)"));
    }

    #[test]
    fn internal_html_hides_priority_section_when_no_priority() {
        // Chunk 23.7: priority_count == 0 のときセクションは省略される。
        let entries = vec![entry("\\a.txt", None)];
        let html = render_internal_html(&make_report(entries, 1)).unwrap();
        assert!(!html.contains("お客様優先データ"));
    }

    #[test]
    fn internal_html_caps_invalid_list_at_20_per_group() {
        // 同じ Invalid 理由を 25 件 → 表示 20 件 + 「... 他 5 件」
        let v =
            ValidationResult::invalid("PNG", "png_v1", "magic", "PNG ではないようです", "拡張子嘘");
        let entries: Vec<_> = (0..25)
            .map(|i| entry(&format!("\\x{}.png", i), Some(v.clone())))
            .collect();
        let html = render_internal_html(&make_report(entries, 25)).unwrap();
        assert!(html.contains("... 他 5 件"));
    }
}
