# Chunk 20.5 指示: 業務適用版レポート（.docx + TXT + サマリ強化）

Chunk 20 で実装したレポート機能を、**業務現場のフィードバックを反映した最終形** に進化させます。万単位ファイル復旧での実用性、CS の業務フロー、お客様への納品体験を考慮した設計です。

> 🎯 完了時点で「**お客様へ実際に納品できる業務品質のレポート**」が自動生成される。Phase 1 NTFS-α リリースの実運用版到達。

---

## 背景: 業務観点フィードバック

実機での復旧業務で発生する課題への対応:

1. **ファイル数が万単位の場合、1 行ずつ HTML に出すのは非現実的**
   → 要対応ファイル中心の構造に変更、Valid は集計のみ
2. **「希望充足率」が範囲指定型では意味が不明確**
   → 「該当ファイル数 / 復旧成功率 / 品質保証率」の 3 段階指標
3. **バイト数表示が読みにくい**
   → GB/MB/KB の自動切替
4. **顧客への納品は PDF + 要確認ファイル一覧が業務的に最適**
   → .docx (CS が Word で編集 → PDF 化) + .txt (フォルダ単位リスト)
5. **顧客 HTML は廃止し、.docx に一本化**

## 目的

8 つの統合された変更:

| Part | 内容 |
|---|---|
| **A** | サマリ計算ロジック追加 (該当/復旧成功/品質保証率) |
| **B** | バイト数の人間可読化 (B/KB/MB/GB/TB 自動切替) |
| **C** | 形式別ブレイクダウン (件数 + 比率) |
| **D** | Invalid 形式別グルーピング表示 |
| **E** | 顧客向け .docx レポート生成 (新規) |
| **F** | 顧客向け recovered_files.txt 生成 (新規、フォルダ単位) |
| **G** | 内部 HTML 更新 (サマリ強化、Invalid グルーピング) |
| **H** | report_customer.html 廃止 + テスト調整 |

## 対象クレート

- **主**: `crates/report/` (大幅更新)
- **副**: `crates/recovery/src/report.rs` (新規ヘルパーメソッド + wish_labels フィールド)

## 重要な設計原則

### 顧客向けと内部向けの完全分離（継続）

| 情報 | 顧客 (.docx + .txt) | CS (HTML + CSV) |
|---|:---:|:---:|
| user_message_ja | ○ | ○ |
| internal_note_ja | **✗** | ○ |
| diagnostics (英語) | ✗ | ○ (CSV のみ) |
| SHA256 | ✗ | ○ |
| 出力先パス | ✗ | ○ |
| 全ファイル一覧 | ✗ (Invalid のみ) | ○ (CSV のみ) |
| Wish::label | ○ | ○ |
| 業務指標サマリ | ○ | ○ |

**.docx と .txt に internal_note_ja が含まれないことを機械テストで検証**。Chunk 20 の `customer_html_must_not_contain_internal_notes` テストを新形式に移植。

## 仕様参照

### ビジネス要件

- **FR-REP-01**: 顧客向け復旧レポート出力 (DOCX 形式に変更)
- **FR-REP-02**: 内部業務管理レポート出力 (サマリ強化)
- **FR-REP-03**: 外部システム連携用 CSV (Wish::label 追加)
- **FR-REP-04**: 業務指標可視化 (該当/復旧成功/品質保証率) ← **新規**
- **FR-REP-05**: 大規模ファイル数対応 (グルーピング表示) ← **新規**

## 実装内容

### Part A: サマリ計算ヘルパー

#### A-1. `crates/recovery/src/report.rs` 拡張

新規フィールド + 計算メソッド:

```rust
pub struct RecoveryReport {
    // ===== 既存 =====
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub total_matched: usize,
    pub recovered: Vec<RecoveredEntry>,
    pub failed: Vec<FailedEntry>,
    pub skipped: Vec<SkippedEntry>,
    
    // ===== 新規: 希望リスト情報 =====
    /// Wishlist の各 Wish::label を順に格納 (顧客向け表示用)
    pub wish_labels: Vec<String>,
}

impl RecoveryReport {
    // ===== 既存メソッド =====
    // success_rate, duration_ms, validated_count, invalid_count, total_bytes_written
    
    // ===== 新規メソッド =====
    
    /// 復旧成功率 = 復旧成功 / 該当ファイル数 (パーセント)
    pub fn recovery_success_rate(&self) -> f64 {
        if self.total_matched == 0 { return 0.0 }
        (self.recovered.len() as f64) / (self.total_matched as f64) * 100.0
    }
    
    /// 品質保証率 = Valid / 復旧成功 (パーセント)
    pub fn quality_assurance_rate(&self) -> f64 {
        if self.recovered.is_empty() { return 0.0 }
        (self.validated_count() as f64) / (self.recovered.len() as f64) * 100.0
    }
    
    /// 検証外件数 (Uncertain) - 復旧成功のうち validator なしのもの
    pub fn uncertain_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_uncertain())
                .unwrap_or(true))
            .count()
    }
    
    /// 形式別の集計を返す。
    /// HashMap<形式名, FormatStats>
    pub fn format_breakdown(&self) -> std::collections::BTreeMap<String, FormatStats> {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<String, FormatStats> = BTreeMap::new();
        
        for entry in &self.recovered {
            let Some(validation) = &entry.validation else {
                let stats = map.entry("(検証なし)".to_string()).or_default();
                stats.total += 1;
                stats.uncertain += 1;
                continue;
            };
            
            let format = validation.format_detected.clone()
                .unwrap_or_else(|| "(未検出)".to_string());
            let stats = map.entry(format).or_default();
            stats.total += 1;
            match validation.status {
                dds_validators::ValidationStatus::Valid => stats.valid += 1,
                dds_validators::ValidationStatus::Invalid => stats.invalid += 1,
                dds_validators::ValidationStatus::Uncertain => stats.uncertain += 1,
            }
        }
        map
    }
    
    /// Invalid なファイルを「Invalid 理由」別にグルーピング。
    /// キーは user_message_ja の冒頭部分 (理由の概要)
    pub fn invalid_grouped_by_reason(&self) -> std::collections::BTreeMap<String, Vec<&RecoveredEntry>> {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<String, Vec<&RecoveredEntry>> = BTreeMap::new();
        
        for entry in &self.recovered {
            let Some(v) = &entry.validation else { continue };
            if !v.status.is_invalid() { continue }
            
            // 「PNG 画像の末尾が欠けています...」のように形式 + 主原因でグルーピング
            let reason_key = match (&v.format_detected, &v.user_message_ja) {
                (Some(fmt), Some(msg)) => {
                    let summary = msg.chars().take(20).collect::<String>();
                    format!("{} - {}", fmt, summary)
                }
                _ => "その他".to_string(),
            };
            map.entry(reason_key).or_insert_with(Vec::new).push(entry);
        }
        map
    }
}

/// 形式別の統計
#[derive(Debug, Default, Clone)]
pub struct FormatStats {
    pub valid: usize,
    pub invalid: usize,
    pub uncertain: usize,
    pub total: usize,
}

impl FormatStats {
    pub fn valid_ratio(&self) -> f64 {
        if self.total == 0 { return 0.0 }
        (self.valid as f64) / (self.total as f64) * 100.0
    }
}
```

#### A-2. `crates/recovery/src/engine.rs` 更新

`recover_files` の戻り値に wish_labels をセット:

```rust
pub fn recover_files<F>(
    &self,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
) -> Result<RecoveryReport, RecoveryError>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // ... 既存処理 ...
    
    // 新規: wish_labels を抽出
    let wish_labels: Vec<String> = wishlist.wishes.iter()
        .map(|w| w.label.clone())
        .collect();
    
    Ok(RecoveryReport {
        // ... 既存フィールド ...
        wish_labels,
    })
}
```

### Part B: バイト数の人間可読化

#### B-1. `crates/report/src/format.rs` (新規ファイル)

```rust
/// バイト数を人間可読な形式に変換する (1024 ベース)。
///
/// 例:
/// - 127 → "127 B"
/// - 5_572 → "5.44 KB"
/// - 7_529_840 → "7.18 MB"
/// - 2_147_483_648 → "2.00 GB"
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

/// ミリ秒を人間可読な形式に変換する。
///
/// 例:
/// - 229 → "0.23 秒"
/// - 12300 → "12.3 秒"
/// - 65000 → "1 分 5 秒"
/// - 3725000 → "1 時間 2 分 5 秒"
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
```

### Part C-D: 内部 HTML 更新

#### `crates/report/src/html_internal.rs` の再設計

```rust
use chrono::Local;
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::escape::escape_html;
use crate::format::{format_bytes, format_duration_ms};

pub fn render_internal_html(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut html = String::with_capacity(16384);
    
    html.push_str(&format!(r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <title>復旧業務レポート (CS 用)</title>
  <style>
    body {{ font-family: "Yu Gothic", "Hiragino Sans", sans-serif; max-width: 1400px; margin: 1em auto; padding: 1em; font-size: 13px; }}
    h1 {{ border-bottom: 3px solid #1e40af; padding-bottom: 0.3em; color: #1e3a8a; }}
    h2 {{ color: #1e40af; margin-top: 1.5em; }}
    .summary {{ background: #eff6ff; padding: 1em 1.5em; border-radius: 4px; border-left: 4px solid #1e40af; }}
    .warning {{ background: #fef2f2; padding: 0.5em 1em; border-left: 4px solid #dc2626; margin: 1em 0; }}
    table {{ width: 100%; border-collapse: collapse; margin-top: 0.5em; font-size: 12px; }}
    th, td {{ padding: 0.4em 0.6em; text-align: left; border-bottom: 1px solid #ddd; }}
    th {{ background: #1e40af; color: white; }}
    .metric {{ font-size: 1.5em; font-weight: bold; color: #1e3a8a; }}
    .ratio {{ color: #6b7280; font-size: 0.85em; }}
    tr.valid {{ background: #f0fdf4; }}
    tr.invalid {{ background: #fef2f2; }}
    .invalid-group {{ margin: 1em 0; padding: 0.8em; background: #fef2f2; border-left: 4px solid #dc2626; border-radius: 4px; }}
    .invalid-group h3 {{ margin: 0 0 0.5em 0; color: #991b1b; }}
    .invalid-group ul {{ margin: 0; padding-left: 1.5em; font-family: monospace; font-size: 11px; }}
    .note {{ color: #6b7280; font-size: 11px; font-style: italic; }}
    footer {{ margin-top: 2em; color: #666; font-size: 0.8em; }}
  </style>
</head>
<body>
  <h1>復旧業務レポート（CS 内部用）</h1>
  <p>作成日時: {}</p>
  
  <div class="warning">
    <strong>⚠ 注意:</strong> このレポートは社内業務用です。internal_note を含むため、お客様には共有しないでください。
  </div>
"#,
        Local::now().format("%Y-%m-%d %H:%M:%S"),
    ));
    
    // ===== サマリセクション (再設計) =====
    let valid = report.validated_count();
    let invalid = report.invalid_count();
    let uncertain = report.uncertain_count();
    
    html.push_str(&format!(r#"  <section class="summary">
    <h2>復旧対象</h2>
    <table>
"#));
    
    // 希望条件の表示
    if !report.wish_labels.is_empty() {
        html.push_str("      <tr><th>ご指定条件</th><td>");
        for (i, label) in report.wish_labels.iter().enumerate() {
            if i > 0 { html.push_str("<br>"); }
            html.push_str(&format!("「{}」", escape_html(label)));
        }
        html.push_str("</td></tr>\n");
    }
    
    html.push_str(&format!(r#"      <tr><th>該当ファイル数</th><td><span class="metric">{}</span> 件</td></tr>
    </table>
    
    <h2>復旧結果</h2>
    <table>
      <tr><th>復旧成功</th><td><span class="metric">{}</span> 件 <span class="ratio">(該当の {:.1}%)</span></td></tr>
      <tr><th>復旧失敗</th><td>{} 件</td></tr>
      <tr><th>スキップ</th><td>{} 件</td></tr>
    </table>
    
    <h2>品質判定内訳</h2>
    <table>
      <tr><th>判定</th><th>件数</th><th>比率</th></tr>
      <tr><td>✓ Valid (品質確認済み)</td><td>{}</td><td>{:.1}%</td></tr>
      <tr><td>✗ Invalid (要確認)</td><td>{}</td><td>{:.1}%</td></tr>
      <tr><td>? Uncertain (検証外)</td><td>{}</td><td>{:.1}%</td></tr>
    </table>
    <p><strong>品質保証率: <span class="metric">{:.1}%</span></strong> (復旧成功のうち Valid の比率)</p>
    
    <h2>データ量と時間</h2>
    <table>
      <tr><th>復旧総量</th><td>{}</td></tr>
      <tr><th>処理時間</th><td>{}</td></tr>
    </table>
  </section>
"#,
        report.total_matched,
        report.recovered.len(),
        report.recovery_success_rate(),
        report.failed.len(),
        report.skipped.len(),
        valid, ratio_safe(valid, report.recovered.len()),
        invalid, ratio_safe(invalid, report.recovered.len()),
        uncertain, ratio_safe(uncertain, report.recovered.len()),
        report.quality_assurance_rate(),
        format_bytes(report.total_bytes_written()),
        format_duration_ms(report.duration_ms()),
    ));
    
    // ===== 形式別ブレイクダウン =====
    let breakdown = report.format_breakdown();
    if !breakdown.is_empty() {
        html.push_str("  <h2>形式別ブレイクダウン</h2>\n  <table>\n");
        html.push_str("    <tr><th>形式</th><th>正常</th><th>要確認</th><th>検証外</th><th>合計</th><th>正常率</th></tr>\n");
        for (format, stats) in &breakdown {
            html.push_str(&format!("    <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.1}%</td></tr>\n",
                escape_html(format),
                stats.valid, stats.invalid, stats.uncertain, stats.total,
                stats.valid_ratio(),
            ));
        }
        html.push_str("  </table>\n");
    }
    
    // ===== Invalid グルーピング表示 (Part D) =====
    let grouped = report.invalid_grouped_by_reason();
    if !grouped.is_empty() {
        html.push_str("  <h2>要確認ファイル (理由別)</h2>\n");
        for (reason, entries) in &grouped {
            html.push_str(&format!(r#"  <div class="invalid-group">
    <h3>{} ({} 件)</h3>
"#,
                escape_html(reason), entries.len()));
            
            // 各グループ内で最大 20 件まで表示
            let display_count = entries.len().min(20);
            html.push_str("    <ul>\n");
            for entry in entries.iter().take(display_count) {
                let internal = entry.validation.as_ref()
                    .and_then(|v| v.internal_note().map(|s| s.to_string()))
                    .unwrap_or_else(|| "-".to_string());
                html.push_str(&format!("      <li>{} <span class=\"note\">[CS メモ: {}]</span></li>\n",
                    escape_html(&entry.original_path),
                    escape_html(&internal),
                ));
            }
            if entries.len() > 20 {
                html.push_str(&format!("      <li class=\"note\">... 他 {} 件 (詳細は CSV を参照)</li>\n",
                    entries.len() - 20));
            }
            html.push_str("    </ul>\n  </div>\n");
        }
    }
    
    // フッター
    html.push_str(r#"  <footer>
    <p>DDS Recovery Workbench - 内部業務レポート</p>
  </footer>
</body>
</html>
"#);
    
    Ok(html)
}

fn ratio_safe(num: usize, denom: usize) -> f64 {
    if denom == 0 { 0.0 } else { (num as f64) / (denom as f64) * 100.0 }
}
```

### Part E: 顧客向け .docx レポート

#### E-1. Cargo.toml 更新

ワークスペースルート `Cargo.toml`:
```toml
[workspace.dependencies]
# 既存に追加:
docx-rs = "0.4"
```

`crates/report/Cargo.toml`:
```toml
[dependencies]
# 既存に追加:
docx-rs.workspace = true
```

#### E-2. `crates/report/src/docx_customer.rs` (新規)

```rust
use chrono::Local;
use docx_rs::*;

use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::format::{format_bytes, format_duration_ms};

const COMPANY_NAME: &str = "デジタルデータソリューション株式会社";

/// 顧客向け .docx レポートをバイト列で生成する。
///
/// 構造:
/// - タイトル + 会社名
/// - 作成日
/// - ご指定条件 (Wish::label)
/// - 復旧結果サマリ
/// - 品質確認サマリ
/// - 要確認ファイル概要 (件数 + 主な理由)
/// - データ量
///
/// 含まれない情報:
/// - 個別ファイル名 (TXT に分離)
/// - internal_note_ja
/// - SHA256 / 出力先パス
/// - 技術的 diagnostics
pub fn render_customer_docx(report: &RecoveryReport) -> Result<Vec<u8>, ReportError> {
    let date = Local::now().format("%Y年%m月%d日").to_string();
    
    let mut docx = Docx::new();
    
    // 会社名 (右寄せ、小さめ)
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Right)
            .add_run(Run::new().add_text(COMPANY_NAME).size(20))
    );
    
    // タイトル
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text("データ復旧レポート").size(40).bold())
    );
    
    // 作成日
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(format!("作成日: {}", date)))
    );
    
    // 空行
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== ご指定条件 =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ ご指定条件").size(28).bold())
    );
    
    if report.wish_labels.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("  (条件指定なし)"))
        );
    } else {
        for label in &report.wish_labels {
            docx = docx.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(format!("  「{}」", label)))
            );
        }
    }
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== 復旧結果サマリ =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 復旧結果サマリ").size(28).bold())
    );
    
    let summary_table = Table::new(vec![
        make_kv_row("該当ファイル数", &format!("{} 件", report.total_matched)),
        make_kv_row("復旧成功", &format!("{} 件 ({:.1}%)",
            report.recovered.len(), report.recovery_success_rate())),
    ]);
    docx = docx.add_table(summary_table);
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== 品質確認 =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 品質確認").size(28).bold())
    );
    
    let valid = report.validated_count();
    let invalid = report.invalid_count();
    let uncertain = report.uncertain_count();
    
    let quality_table = Table::new(vec![
        make_kv_row("正常確認済み", &format!("{} 件 ({:.1}%)", valid, report.quality_assurance_rate())),
        make_kv_row("要ご確認", &format!("{} 件", invalid)),
        make_kv_row("自動確認対象外", &format!("{} 件", uncertain)),
    ]);
    docx = docx.add_table(quality_table);
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== 要確認ファイル概要 (Invalid) =====
    if invalid > 0 {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("■ 要ご確認のファイルについて").size(28).bold())
        );
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text(format!("合計 {} 件のファイルに品質上の懸念があります。", invalid)))
        );
        docx = docx.add_paragraph(Paragraph::new());
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("主な内訳:").bold())
        );
        
        let grouped = report.invalid_grouped_by_reason();
        for (reason, entries) in grouped.iter().take(5) {  // 上位 5 件
            docx = docx.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(format!("  ・{}: {} 件", reason, entries.len())))
            );
        }
        docx = docx.add_paragraph(Paragraph::new());
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("詳細なファイル一覧は、別添「recovered_files.txt」をご参照ください。").italic())
        );
        docx = docx.add_paragraph(Paragraph::new());
    }
    
    // ===== データ量 =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 復旧データ量").size(28).bold())
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(format!("  合計: {}", format_bytes(report.total_bytes_written()))))
    );
    
    // ===== フッター =====
    docx = docx.add_paragraph(Paragraph::new());
    docx = docx.add_paragraph(Paragraph::new());
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text("ご不明な点がございましたら、担当者までお問い合わせください。").size(18))
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text(COMPANY_NAME).size(20).bold())
    );
    
    // パック
    let mut buf = Vec::new();
    let cursor = std::io::Cursor::new(&mut buf);
    docx.build().pack(cursor)
        .map_err(|e| ReportError::Template(format!("docx pack error: {}", e)))?;
    Ok(buf)
}

/// "ラベル: 値" のテーブル行を作る helper
fn make_kv_row(label: &str, value: &str) -> TableRow {
    TableRow::new(vec![
        TableCell::new().add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(label).bold())
        ),
        TableCell::new().add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(value))
        ),
    ])
}
```

> 💡 **docx-rs の API**: バージョン 0.4.x の API を想定。実装時に最新版で微調整必要かも。`Table::new` のシグネチャ、`AlignmentType` の名前など、コンパイル時にエラーが出たらドキュメント参照して修正。

### Part F: 顧客向け recovered_files.txt

#### `crates/report/src/txt_customer.rs` (新規)

```rust
use std::collections::BTreeMap;
use chrono::Local;

use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

/// 顧客向け 要確認ファイル一覧 TXT を生成する。
///
/// フォルダ単位でグルーピング。Invalid なファイルのみ列挙。
pub fn render_invalid_files_txt(report: &RecoveryReport) -> String {
    let mut content = String::new();
    
    content.push_str("要確認ファイル一覧\n");
    content.push_str("====================\n\n");
    content.push_str(&format!("作成日: {}\n\n", Local::now().format("%Y年%m月%d日")));
    
    content.push_str("このリストは、復旧したものの破損の可能性があるファイルです。\n");
    content.push_str("お手元のディスクで実際に開いて、内容をご確認ください。\n\n");
    
    // Invalid ファイルを収集
    let invalid_entries: Vec<_> = report.recovered.iter()
        .filter(|e| e.validation.as_ref()
            .map(|v| v.status.is_invalid())
            .unwrap_or(false))
        .collect();
    
    if invalid_entries.is_empty() {
        content.push_str("(要確認ファイルはありません)\n");
        return content;
    }
    
    // フォルダ単位でグルーピング (BTreeMap でソート順保証)
    let mut by_folder: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for entry in &invalid_entries {
        let path = &entry.original_path;
        // 最後の `\` 以前をフォルダ、以降をファイル名とする
        let (folder, filename) = match path.rfind('\\') {
            Some(0) => ("\\".to_string(), &path[1..]),  // ルート直下
            Some(pos) => (path[..pos].to_string(), &path[pos+1..]),
            None => (String::new(), path.as_str()),
        };
        by_folder.entry(folder).or_insert_with(Vec::new).push(filename);
    }
    
    // 各フォルダごとに出力
    for (folder, files) in &by_folder {
        let folder_display = if folder.is_empty() { "(ルート)" } else { folder.as_str() };
        content.push_str(&format!("==== {} ====\n", folder_display));
        for filename in files {
            content.push_str(&format!("  {}\n", filename));
        }
        content.push_str("\n");
    }
    
    // 合計
    content.push_str(&format!("合計: {} ファイル\n", invalid_entries.len()));
    content.push_str("\n");
    content.push_str("ご不明な点は、担当者までお問い合わせください。\n");
    content.push_str("デジタルデータソリューション株式会社\n");
    
    content
}
```

### Part G: report_customer.html 廃止

`crates/report/src/html_customer.rs` を**削除**。`mod.rs` (lib.rs) からも export 削除。

### Part H: lib.rs / write_all_reports 更新

```rust
//! DDS 復旧レポート生成。
//!
//! 4 種類のレポートを生成:
//! - 顧客向け .docx (CS が編集 → PDF 化して納品)
//! - 顧客向け要確認 .txt (フォルダ単位リスト)
//! - CS 業務用 HTML (詳細業務管理)
//! - 外部連携用 CSV (全フィールド)

pub mod csv;
pub mod docx_customer;
pub mod error;
pub mod escape;
pub mod format;
pub mod html_internal;
pub mod txt_customer;

pub use crate::csv::render_csv;
pub use crate::docx_customer::render_customer_docx;
pub use crate::format::{format_bytes, format_duration_ms};
pub use crate::html_internal::render_internal_html;
pub use crate::txt_customer::render_invalid_files_txt;
pub use error::ReportError;

use std::path::{Path, PathBuf};
use dds_recovery::RecoveryReport;

/// 4 種類のレポートを `output_dir` に書き出す。
///
/// 出力:
/// - `{output_dir}/report_customer.docx`
/// - `{output_dir}/recovered_files.txt`
/// - `{output_dir}/report_internal.html`
/// - `{output_dir}/report.csv`
pub fn write_all_reports(
    report: &RecoveryReport,
    output_dir: &Path,
) -> Result<ReportPaths, ReportError> {
    std::fs::create_dir_all(output_dir)?;
    
    let customer_docx = output_dir.join("report_customer.docx");
    let invalid_txt = output_dir.join("recovered_files.txt");
    let internal_html = output_dir.join("report_internal.html");
    let csv_path = output_dir.join("report.csv");
    
    std::fs::write(&customer_docx, render_customer_docx(report)?)?;
    std::fs::write(&invalid_txt, render_invalid_files_txt(report))?;
    std::fs::write(&internal_html, render_internal_html(report)?)?;
    std::fs::write(&csv_path, render_csv(report)?)?;
    
    Ok(ReportPaths {
        customer_docx,
        invalid_txt,
        internal_html,
        csv: csv_path,
    })
}

/// 生成されたレポートのファイルパス
#[derive(Debug, Clone)]
pub struct ReportPaths {
    pub customer_docx: PathBuf,
    pub invalid_txt: PathBuf,
    pub internal_html: PathBuf,
    pub csv: PathBuf,
}
```

### CSV の更新

`crates/report/src/csv.rs` に `wish_labels` 列を追加 (1 列増、計 14 列):

```rust
wtr.write_record(&[
    "source_id",
    "original_path",
    "output_path",
    "bytes_written",
    "is_deleted",
    "priority_score",
    "matched_wishes",       // 新規: 該当した Wish::label をカンマ区切り
    "sha256",
    "validation_status",
    "format_detected",
    "validator_name",
    "customer_message",
    "internal_note",
    "diagnostics",
])?;
```

`matched_wishes` はマッチしたファイルに対する Wish::label を `; ` 区切りで連結。MatchResult から取得。

## 単体テスト要件（最低 18 件）

### format.rs (新規)
1. `format_bytes_under_1024_shows_bytes`: 127 → "127 B"
2. `format_bytes_kilobytes`: 5572 → "5.44 KB"
3. `format_bytes_megabytes`: 7_529_840 → "7.18 MB"
4. `format_bytes_gigabytes`: 2_147_483_648 → "2.00 GB"
5. `format_bytes_zero`: 0 → "0 B"
6. `format_duration_seconds`: 12300 → "12.30 秒"
7. `format_duration_minutes`: 65000 → "1 分 5 秒"
8. `format_duration_hours`: 3725000 → "1 時間 2 分 5 秒"

### recovery report 拡張
9. `recovery_success_rate_calculates_correctly`: 14/15 → ~93.3%
10. `quality_assurance_rate_calculates_correctly`: 10/14 → ~71.4%
11. `format_breakdown_groups_by_format`: PNG 3 件、JPEG 2 件 etc.
12. `invalid_grouped_by_reason_separates_distinct_reasons`: 末尾欠損 vs 拡張子嘘

### docx_customer.rs
13. `customer_docx_contains_company_name`: "デジタルデータソリューション株式会社" が含まれる
14. `customer_docx_contains_wish_labels`: wish_labels の各 label が含まれる
15. **`customer_docx_excludes_internal_note`**: internal_note_ja の文言が含まれない (バイト列内検索、重要)
16. `customer_docx_contains_summary_metrics`: 該当数、復旧成功率、品質保証率が含まれる

### txt_customer.rs
17. `txt_groups_by_folder`: 同じフォルダのファイルが連続して出力される
18. `txt_only_includes_invalid_entries`: Valid なファイルは出力されない
19. `txt_includes_summary_line`: "合計: N ファイル" が含まれる
20. `txt_handles_root_files_correctly`: ルート直下のファイルが "(ルート)" でグルーピング

### html_internal.rs 更新
21. `internal_html_shows_recovery_success_rate`: 復旧成功率の表示
22. `internal_html_shows_quality_assurance_rate`: 品質保証率の表示
23. `internal_html_groups_invalid_by_reason`: 理由別グルーピングの DOM 構造
24. `internal_html_caps_invalid_list_at_20_per_group`: 1 グループ 20 件超で省略

## 結合テスト要件（最低 3 件）

### 1. 4 ファイル生成の end-to-end

```rust
#[test]
fn generates_four_report_files_in_business_format() {
    let img = decompress_fixture("ntfs_mixed_formats");
    // ... setup ...
    let temp_dir = TempDir::new().unwrap();
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    let paths = dds_report::write_all_reports(&report, temp_dir.path()).unwrap();
    
    assert!(paths.customer_docx.exists());
    assert!(paths.invalid_txt.exists());
    assert!(paths.internal_html.exists());
    assert!(paths.csv.exists());
    
    // .docx は ZIP として読める (OOXML の検証)
    let docx_bytes = std::fs::read(&paths.customer_docx).unwrap();
    assert!(docx_bytes.starts_with(b"PK\x03\x04"));  // ZIP magic
}
```

### 2. .docx に internal_note が含まれないこと

```rust
#[test]
fn customer_docx_must_not_contain_internal_notes() {
    // ... setup, recovery, report 生成 ...
    
    let docx_bytes = std::fs::read(&paths.customer_docx).unwrap();
    
    // .docx は ZIP なので、内部の XML を抽出して検証
    let cursor = std::io::Cursor::new(&docx_bytes);
    let mut archive = zip::ZipArchive::new(cursor).unwrap();
    
    let mut all_text = String::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        if file.name().ends_with(".xml") {
            let mut content = String::new();
            std::io::Read::read_to_string(&mut file, &mut content).unwrap();
            all_text.push_str(&content);
        }
    }
    
    let forbidden = [
        "再復旧推奨",
        "CS 確認",
        "業務判断",
        "技術調査",
        "disk-io 層",
    ];
    for phrase in &forbidden {
        assert!(!all_text.contains(phrase),
            "Customer DOCX should not contain CS-internal phrase: {}", phrase);
    }
}
```

### 3. プロダクトデモテスト (Chunk 20.5 完成版)

```rust
#[test]
fn product_demo_business_grade_reports() {
    let img = decompress_fixture("ntfs_mixed_formats");
    // ... setup ...
    let temp_dir = TempDir::new().unwrap();
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    let paths = dds_report::write_all_reports(&report, temp_dir.path()).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Business-Grade Reports (Chunk 20.5) ===\n");
    println!("入力:");
    println!("  ソース: ntfs_mixed_formats.img.zst");
    println!();
    println!("業務指標:");
    println!("  該当ファイル数:  {} 件", report.total_matched);
    println!("  復旧成功率:      {:.1}%", report.recovery_success_rate());
    println!("  品質保証率:      {:.1}%", report.quality_assurance_rate());
    println!("  復旧データ量:    {}", dds_report::format_bytes(report.total_bytes_written()));
    println!("  処理時間:        {}", dds_report::format_duration_ms(report.duration_ms()));
    println!();
    println!("形式別ブレイクダウン:");
    for (format, stats) in report.format_breakdown() {
        println!("  {:6} : {}/{} 正常 ({:.1}%)", format, stats.valid, stats.total, stats.valid_ratio());
    }
    println!();
    println!("出力ファイル:");
    println!("  [顧客向け] report_customer.docx ({:?})", paths.customer_docx.metadata().unwrap().len());
    println!("  [顧客向け] recovered_files.txt  ({:?})", paths.invalid_txt.metadata().unwrap().len());
    println!("  [CS 内部] report_internal.html  ({:?})", paths.internal_html.metadata().unwrap().len());
    println!("  [外部連携] report.csv           ({:?})", paths.csv.metadata().unwrap().len());
    println!();
    println!("CS のフロー:");
    println!("  1. report_customer.docx を Word で開いて確認");
    println!("  2. 案件固有の注記を追加 (必要なら)");
    println!("  3. 「PDF として保存」(Word の機能)");
    println!("  4. PDF + recovered_files.txt をお客様に納品");
    println!();
    println!("=== Phase 1 NTFS-α 業務適用版完成 ===");
    
    // 基本的な assertions
    assert!(paths.customer_docx.metadata().unwrap().len() > 1000);
    assert!(paths.invalid_txt.metadata().unwrap().len() > 100);
}
```

## 既存テストのマイグレーション

Chunk 20 の以下テストは新形式に合わせて更新:

- `customer_html_excludes_internal_note_ja` → `customer_docx_must_not_contain_internal_notes` に置換
- `customer_html_*` 系すべて削除 (html_customer.rs 廃止のため)
- `write_all_reports_creates_three_files` → `write_all_reports_creates_four_files` に変更

## 制約

- **行数目安**:
  - `crates/report/src/format.rs` 新規: ~60 行
  - `crates/report/src/docx_customer.rs` 新規: ~200 行
  - `crates/report/src/txt_customer.rs` 新規: ~80 行
  - `crates/report/src/html_internal.rs` 大幅更新: ~250 行
  - `crates/recovery/src/report.rs` 拡張: ~100 行
  - 削除: `crates/report/src/html_customer.rs` (約 150 行削除)
- **単体テスト最低 18 件**
- **結合テスト最低 3 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **.docx に internal_note_ja が含まれないこと**を ZIP 解凍 + 文字列検索でテスト

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-report` が全パス（≥18 件）
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `customer_docx_must_not_contain_internal_notes` テストが pass
- [ ] `product_demo_business_grade_reports` が pass + 出力が見える
- [ ] 生成された `report_customer.docx` を実際に Word で開いて視覚確認
- [ ] `recovered_files.txt` を Notepad で開いて視覚確認

## 関連 FR 要件

- **FR-REP-01** (顧客向け復旧レポート) ← **業務適用版到達 (.docx)**
- **FR-REP-02** (内部業務管理レポート) ← サマリ強化済み
- **FR-REP-03** (外部システム連携 CSV) ← Wish::label 列追加
- **FR-REP-04** (業務指標可視化) ← **新規達成**
- **FR-REP-05** (大規模ファイル対応) ← **新規達成**

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1 NTFS-α リリース業務適用版完成**
4. 次のステップ候補:
   - **Chunk 21**: case-manager (案件管理基盤)
   - **Chunk 22**: Tauri UI 着手
   - **実機検証**: 中古 NTFS HDD での動作確認

---

## 注意事項

### docx-rs の API バージョン

docx-rs は 0.4.x 系を想定。実装時に最新版 (0.4.x 系で互換性ある範囲) を使用。API 変更でコンパイルエラーが出たら:
- `Run::new().add_text(...)` のチェーン方式維持
- `Paragraph::new().add_run(...)` 維持
- `Table::new(vec![TableRow::new(vec![TableCell::new()...])])` の階層維持
- `AlignmentType::Center` / `Right` の enum 名確認

### `.docx` 内の日本語

UTF-8 で自然に動作。Word が標準フォントで表示。フォント指定は最小限 (Word のデフォルトを利用)。

### CS の Word ワークフロー

レポートを Word で開いた CS が:
1. 案件番号や担当者名を冒頭に追加
2. 顧客名を「○○様」形式で挿入
3. 必要に応じて挨拶文を追記
4. 「PDF として保存」 (File → Export → Create PDF/XPS Document)

実際の運用では、ここで CS の人間性が入る余地を残す設計。完全自動化を目指すと逆に冷たい印刷物になる。

### 内部 HTML の役割は維持

CS が業務管理で見るための情報源として `report_internal.html` は維持。internal_note + SHA256 + 出力先パス + 形式別詳細を含む。

### TXT ファイルの BOM

UTF-8 (BOM なし) で出力。Notepad on Windows 10/11 は UTF-8 BOM なしを正しく扱える (10 1903 以降)。

ただし古い Windows での文字化けが懸念されるなら、BOM 付与オプションを検討:
```rust
content.insert_str(0, "\u{FEFF}");
```

Phase 1 では BOM なし。問題報告があれば対応。

### TXT のフォルダ並び順

`BTreeMap` で自動アルファベット順ソート。お客様が見たときに「ルート → サブ → 深い階層」の自然な並びになる。

### 万件規模での実用性

10,000 ファイル復旧で Invalid が 1,000 件の場合:
- 内部 HTML: 形式別グルーピングで「PNG 末尾欠損 234 件」のように集約、各 20 件まで詳細表示 → 数 KB の HTML
- 顧客 .docx: 件数 + 主な理由のみ、全リストは TXT へ → 数 KB の Word
- recovered_files.txt: 1,000 件をフォルダ別に → 数十 KB の TXT (Notepad で問題なく開ける)
- CSV: 全 10,000 行 → 数 MB、Excel で開ける

業務的なスケーラビリティ確保。

### Phase 1 で意図的に除外した機能

- **テンプレートカスタマイズ**: docx-rs ベースでハードコード
- **会社ロゴ画像**: Phase 2 で対応
- **顧客ごとのテーマ設定**: Phase 2 で対応
- **ファイル数の閾値による表示切替**: 現在「常に Invalid 集中表示」、量に関わらず同じ構造

---

## 質問が必要なケース

- docx-rs のバージョン互換性問題への対処
- Word 古いバージョン (2010 以前) との互換性確認の優先度
- CSV の Excel 文字化け対策 (BOM 付与) の優先度

---

## 完了報告例

```markdown
## Chunk 20.5 完了報告

### 新規ファイル
- `crates/report/src/format.rs`            (60 行 + テスト 50 行)
- `crates/report/src/docx_customer.rs`     (210 行 + テスト 45 行)
- `crates/report/src/txt_customer.rs`      (80 行 + テスト 40 行)

### 更新ファイル
- `crates/report/src/html_internal.rs`     (大幅再設計、~250 行)
- `crates/report/src/csv.rs`                (matched_wishes 列追加)
- `crates/report/src/lib.rs`                (4 ファイル出力に変更)
- `crates/recovery/src/report.rs`           (wish_labels + 計算メソッド追加)
- `crates/recovery/src/engine.rs`           (wish_labels セット)

### 削除ファイル
- `crates/report/src/html_customer.rs`     (廃止)

### 公開 API
- `dds_report::render_customer_docx(&RecoveryReport) -> Result<Vec<u8>>`
- `dds_report::render_invalid_files_txt(&RecoveryReport) -> String`
- `dds_report::format_bytes(u64) -> String`
- `dds_report::format_duration_ms(i64) -> String`
- `RecoveryReport::recovery_success_rate() -> f64`
- `RecoveryReport::quality_assurance_rate() -> f64`
- `RecoveryReport::format_breakdown() -> BTreeMap<String, FormatStats>`
- `RecoveryReport::invalid_grouped_by_reason() -> BTreeMap<String, Vec<&RecoveredEntry>>`

### テスト統計
- 単体: 既存 304 + 新規 24 = **328 件 pass**
- 結合: 既存 52 + 新規 3 = **55 件 pass**
- 全 workspace: **383+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 顧客 .docx に internal_note が含まれない (ZIP 解凍検証で機械的に確認)

### 業務価値の見える化 (`product_demo_business_grade_reports`)
```
=== DDS Recovery Workbench - Business-Grade Reports (Chunk 20.5) ===

入力:
  ソース: ntfs_mixed_formats.img.zst

業務指標:
  該当ファイル数:  14 件
  復旧成功率:      100.0%
  品質保証率:      71.4%
  復旧データ量:    548 B
  処理時間:        0.23 秒

形式別ブレイクダウン:
  BMP    : 1/1 正常 (100.0%)
  DOCX   : 1/1 正常 (100.0%)
  GIF    : 1/1 正常 (100.0%)
  JPEG   : 2/3 正常 (66.7%)
  PDF    : 2/4 正常 (50.0%)
  PNG    : 3/4 正常 (75.0%)

出力ファイル:
  [顧客向け] report_customer.docx (5234 bytes)
  [顧客向け] recovered_files.txt  (412 bytes)
  [CS 内部] report_internal.html  (8932 bytes)
  [外部連携] report.csv           (5512 bytes)

CS のフロー:
  1. report_customer.docx を Word で開いて確認
  2. 案件固有の注記を追加 (必要なら)
  3. 「PDF として保存」(Word の機能)
  4. PDF + recovered_files.txt をお客様に納品

=== Phase 1 NTFS-α 業務適用版完成 ===
```

### 🎉 マイルストーン
- **Phase 1 NTFS-α リリース業務適用版完成**
- 顧客への実納品可能な成果物自動生成
- 業務指標 (該当/成功率/品質保証率) の可視化
- 万件規模での実用性確保 (グルーピング戦略)

- **関連 FR**: FR-REP-01〜05 (達成)

→ tester エージェントへ引き継ぎお願いします
```
