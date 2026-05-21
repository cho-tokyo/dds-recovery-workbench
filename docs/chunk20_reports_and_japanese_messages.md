# Chunk 20 指示: 3 層メッセージ + レポート生成（顧客 / CS / 開発者）

このチャンクで Phase 1 NTFS-α リリースの**最後のピース**が揃います。validator メッセージの 3 層構造化（顧客向け日本語 / CS 内部メモ / 技術詳細）+ レポート生成（HTML 顧客向け / HTML CS 向け / CSV）の本実装。

> 🎯 完了時点で「**お客様に納品できる成果物 + CS が業務管理できる成果物**」が自動生成される。Phase 1 NTFS-α リリース達成。

---

## 目的

3 つの統合された改善を実施:

### Part A: ValidationResult 3 層メッセージ化

- 技術詳細（既存 `diagnostics`、英語、開発者向け）
- **顧客向け日本語**（新規 `user_message_ja`、報告書に載せる）
- **CS 内部メモ**（新規 `internal_note_ja`、報告書には載せない）

各 validator の全結果に 3 層メッセージを実装。

### Part B: report クレート本実装

`crates/report/` を実装し、`RecoveryReport` から以下を生成:

- **HTML 顧客向け**: 顧客に納品する正式レポート（user_message_ja のみ使用）
- **HTML CS 向け**: CS の業務管理用（user_message_ja + internal_note_ja + SHA256 等）
- **CSV**: 他システム連携用（全フィールド）

### Part C: end-to-end 統合

`recovery → validators → report` の連鎖が動作。実フィクスチャで実証。

## 重要な設計原則

### 3 層情報の使い分け（責務分離）

| 層 | 用途 | 表示先 | 載せる場所 |
|---|---|---|---|
| `diagnostics` (英語) | デバッグ・ログ・障害解析 | 開発者 | CSV のみ |
| `user_message_ja` (日本語) | 顧客への結果説明 | 顧客 + CS | **HTML 顧客向け / HTML CS 向け / CSV すべて** |
| `internal_note_ja` (日本語) | CS の業務判断補助 | CS のみ | **HTML CS 向け / CSV のみ**（顧客向けには絶対載せない） |

**「顧客に internal_note_ja を見せないこと」が業務的に最重要**。テストで明示的に検証する。

## 対象クレート

- **主**: `crates/validators/`（3 層メッセージ化）
- **主**: `crates/report/`（Chunk 1 で空スケルトン作成済み、本実装）
- **副**: `crates/recovery/`（既存テストの調整、不要かも）

## 仕様参照

### ビジネス要件

- **FR-REP-01**: 顧客向け復旧レポート出力
- **FR-REP-02**: 内部業務管理レポート出力
- **FR-REP-03**: 外部システム連携用 CSV
- **FR-QUAL-04**: 検証結果の多言語サポート（日本語）

### 既存実装

- `crates/validators/` (Chunks 18-19)
- `crates/recovery/` (Chunk 17)

## 実装内容

### Part A: ValidationResult 拡張

#### A-1. `crates/validators/src/result.rs` 更新

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Uncertain,
}

impl ValidationStatus {
    pub fn is_valid(self) -> bool { matches!(self, ValidationStatus::Valid) }
    pub fn is_invalid(self) -> bool { matches!(self, ValidationStatus::Invalid) }
    pub fn is_uncertain(self) -> bool { matches!(self, ValidationStatus::Uncertain) }
}

/// 検証結果。3 層のメッセージを持つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub status: ValidationStatus,
    pub format_detected: Option<String>,
    pub validator_name: String,
    
    /// 技術詳細（英語、開発者向け、CSV のみ表示）
    pub diagnostics: Vec<String>,
    
    /// 顧客向け日本語サマリ（顧客 HTML / CS HTML / CSV に表示）
    /// None の場合はデフォルト文言を使う
    pub user_message_ja: Option<String>,
    
    /// CS 内部メモ（CS HTML / CSV のみ表示、**顧客 HTML には絶対載せない**）
    /// 「次にこうしてください」等の業務判断補助
    pub internal_note_ja: Option<String>,
}

impl ValidationResult {
    /// 構造体リテラルで直接構築するか、以下の helper を使う

    /// Valid 結果（3 層メッセージ込み）
    pub fn valid(
        format: impl Into<String>,
        validator: impl Into<String>,
        diagnostics: Vec<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: Option<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Valid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics,
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja,
        }
    }

    /// Invalid 結果（3 層メッセージ込み）
    pub fn invalid(
        format: impl Into<String>,
        validator: impl Into<String>,
        diagnostic: impl Into<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: impl Into<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Invalid,
            format_detected: Some(format.into()),
            validator_name: validator.into(),
            diagnostics: vec![diagnostic.into()],
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja: Some(internal_note_ja.into()),
        }
    }

    /// Uncertain 結果
    pub fn uncertain(
        reason: impl Into<String>,
        user_message_ja: impl Into<String>,
        internal_note_ja: impl Into<String>,
    ) -> Self {
        Self {
            status: ValidationStatus::Uncertain,
            format_detected: None,
            validator_name: "none".into(),
            diagnostics: vec![reason.into()],
            user_message_ja: Some(user_message_ja.into()),
            internal_note_ja: Some(internal_note_ja.into()),
        }
    }

    /// 顧客向けに公開可能なメッセージ（user_message_ja がなければデフォルト）
    pub fn customer_message(&self) -> String {
        self.user_message_ja.clone().unwrap_or_else(|| match self.status {
            ValidationStatus::Valid => format!("{}として正常です",
                self.format_detected.as_deref().unwrap_or("ファイル")),
            ValidationStatus::Invalid => "ファイルに問題があります".to_string(),
            ValidationStatus::Uncertain => "自動検証の対象外です".to_string(),
        })
    }

    /// CS 向け内部メモ（顧客には絶対公開しない）
    pub fn internal_note(&self) -> Option<&str> {
        self.internal_note_ja.as_deref()
    }
}
```

#### A-2. 全 validator のメッセージ更新

各 validator（PNG/JPEG/PDF/GIF/BMP/ZIP/DOCX/XLSX/PPTX）の各分岐に 3 層メッセージを実装。

##### PNG validator メッセージ例

```rust
// formats/png.rs の validate メソッド内

// Valid 時
return ValidationResult::valid(
    "PNG",
    self.name(),
    vec!["Magic signature OK".into(), "IHDR chunk found".into(), "IEND chunk found".into()],
    "PNG 画像として正常です",
    None,  // 正常時は内部メモ不要
);

// Magic 不一致
return ValidationResult::invalid(
    "PNG",
    self.name(),
    format!("Magic signature mismatch (got {:02X?})", &content[0..8]),
    "PNG として保存されていますが、PNG ファイルではないようです（別の形式の可能性）",
    "拡張子と中身の不一致。バイト列冒頭から実形式を判定し、正しい拡張子で再復旧推奨",
);

// 末尾欠損
return ValidationResult::invalid(
    "PNG",
    self.name(),
    "IEND chunk not found at end of file".into(),
    "PNG 画像の末尾が欠けています。画像の一部または全体が表示できない可能性があります",
    "末尾チャンク欠損のため部分復旧。可能なら元データから再復旧を試行",
);

// IHDR 欠落
return ValidationResult::invalid(
    "PNG",
    self.name(),
    format!("First chunk should be IHDR, got {:02X?}", &content[12..16]),
    "PNG 画像のヘッダー情報が破損しています。表示できない可能性があります",
    "IHDR チャンク欠落。深い構造破損のため再復旧は困難の可能性。サンプル復旧を推奨",
);

// 小さすぎ
return ValidationResult::invalid(
    "PNG",
    self.name(),
    format!("File too small ({} bytes, need at least 45)", content.len()),
    "ファイルが小さすぎて PNG として認識できません",
    format!("{} バイトしかない。MFT クラスタ単位の不整合の可能性、disk-io 層を確認", content.len()),
);
```

##### JPEG validator メッセージ例

```rust
// Valid
ValidationResult::valid(
    "JPEG", self.name(), 
    vec!["SOI OK".into(), "EOI OK".into()],
    "JPEG 画像として正常です",
    None,
);

// EOI なし
ValidationResult::invalid(
    "JPEG", self.name(),
    format!("EOI marker missing (got {:02X?} at end)", &content[end-2..end]),
    "JPEG 画像の末尾が欠けています。画像の一部が表示できない可能性があります",
    "EOI marker 欠落。画像末尾切り詰めの可能性、元データから再復旧推奨",
);

// SOI なし
ValidationResult::invalid(
    "JPEG", self.name(),
    format!("SOI marker missing (got {:02X?})", &content[0..2]),
    "JPEG として保存されていますが、JPEG ファイルではないようです（別の形式の可能性）",
    "ヘッダー破損または拡張子嘘。実形式を判定して正しい拡張子で再復旧",
);
```

##### PDF validator メッセージ例

```rust
// Valid
ValidationResult::valid(
    "PDF", self.name(),
    vec![format!("Header OK (1.{})", version_byte as char), "%%EOF found".into()],
    format!("PDF ファイルとして正常です（バージョン 1.{}）", version_byte as char),
    None,
);

// %%EOF なし
ValidationResult::invalid(
    "PDF", self.name(),
    format!("%%EOF trailer not found in last {} bytes", TRAILER_SEARCH_TAIL),
    "PDF の末尾マーカーが見つかりません。保存途中で中断された可能性があります",
    "%%EOF 欠落。書き込み中断の可能性、最新の自動保存版があれば確認推奨",
);

// ヘッダーなし
ValidationResult::invalid(
    "PDF", self.name(),
    format!("PDF header missing (got {:?})", std::str::from_utf8(&content[..8]).unwrap_or("<binary>")),
    "PDF として保存されていますが、PDF ファイルではないようです（別の形式の可能性）",
    "拡張子嘘の典型例。バイト列先頭から実形式を判定（PNG/JPEG/Office 等の可能性）し、正しい拡張子で再復旧",
);

// 古バージョン
ValidationResult::invalid(
    "PDF", self.name(),
    format!("Unsupported PDF version: 1.{}", version_byte as char),
    format!("PDF バージョン 1.{} は現在サポート対象外です", version_byte as char),
    format!("PDF 1.{} は範囲外（1.0-1.7 のみ対応）。技術調査必要", version_byte as char),
);
```

##### GIF / BMP / ZIP / OOXML も同様

各 validator の各分岐に 3 層メッセージを追加。実装パターンは PNG/JPEG/PDF と同じ。

##### Registry: 未知拡張子の場合

```rust
// registry.rs
pub fn validate(&self, content: &[u8], extension: Option<&str>) -> ValidationResult {
    let Some(ext) = extension else {
        return ValidationResult::uncertain(
            "No extension provided",
            "拡張子が指定されていないため、自動検証できません",
            "拡張子なしファイル。マジック自動検出は Phase 2 対応予定。CS で内容確認",
        );
    };
    
    let lower = ext.to_lowercase();
    let Some(validator) = self.by_extension.get(&lower) else {
        return ValidationResult::uncertain(
            format!("No validator for extension: .{}", lower),
            format!(".{} 形式の自動検証は現在対応していません", lower),
            format!(".{} は未実装。CS で実際にファイルを開いて確認推奨。複数件発生する場合は validator 追加検討", lower),
        );
    };
    
    validator.validate(content)
}
```

#### A-3. 既存テストの更新

Chunk 19 までの単体テストは `ValidationResult::valid(...)` 等を新シグネチャに合わせて修正必要。

**マイグレーション戦略**:
- `grep -rn "ValidationResult::valid\|ValidationResult::invalid\|ValidationResult::uncertain" crates/`
- 既存呼び出しを新シグネチャに機械的に置換
- メッセージはサンプル文言（上記例）を使う

---

### Part B: report クレート本実装

#### B-1. モジュール構成

```
crates/report/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-export
    ├── error.rs            ← ReportError
    ├── html_customer.rs    ← 顧客向け HTML レポート生成
    ├── html_internal.rs    ← CS 向け HTML レポート生成
    ├── csv.rs              ← CSV エクスポート
    └── escape.rs           ← HTML エスケープ helper
```

#### B-2. `Cargo.toml`

```toml
[package]
name = "dds-report"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono.workspace = true
thiserror.workspace = true
csv = "1.3"
dds-recovery.workspace = true
dds-validators.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

#### B-3. `error.rs`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    
    #[error("Template rendering error: {0}")]
    Template(String),
}
```

#### B-4. `escape.rs`

HTML エスケープ helper（外部依存を最小化、自前実装）:

```rust
/// HTML 特殊文字をエスケープする。
/// `< > & " '` を対応するエンティティに変換。
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
```

#### B-5. `html_customer.rs`

顧客向け HTML。**internal_note_ja を絶対に含めない**:

```rust
use chrono::Local;
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::escape::escape_html;

/// 顧客向け HTML レポートを生成する。
///
/// 含まれる情報:
/// - 概要サマリ
/// - フォーマット別ブレイクダウン
/// - ファイル一覧（パス、サイズ、状態、顧客向けメッセージ）
///
/// 含まれない情報（重要）:
/// - internal_note_ja（CS 内部メモ）
/// - 技術詳細 (diagnostics)
/// - SHA256
/// - 出力先パス
pub fn render_customer_html(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut html = String::with_capacity(8192);
    
    // ヘッダー
    html.push_str(&format!(r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <title>データ復旧レポート - {}</title>
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
  <p>作成日時: {}</p>
"#,
        Local::now().format("%Y-%m-%d"),
        Local::now().format("%Y年%m月%d日 %H:%M"),
    ));
    
    // サマリ
    let valid_count = report.validated_count();
    let invalid_count = report.invalid_count();
    let uncertain_count = report.recovered.len() - valid_count - invalid_count;
    
    html.push_str(&format!(r#"  <section class="summary">
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
    html.push_str(r#"  <section class="files">
    <h2>復旧ファイル一覧</h2>
    <table>
      <thead>
        <tr>
          <th>パス</th>
          <th>サイズ</th>
          <th>状態</th>
          <th>備考</th>
        </tr>
      </thead>
      <tbody>
"#);
    
    for entry in &report.recovered {
        let (status_class, badge_class, badge_label) = match entry.validation.as_ref() {
            Some(v) => match v.status {
                ValidationStatus::Valid => ("valid", "badge-valid", "正常"),
                ValidationStatus::Invalid => ("invalid", "badge-invalid", "要確認"),
                ValidationStatus::Uncertain => ("uncertain", "badge-uncertain", "検証外"),
            },
            None => ("uncertain", "badge-uncertain", "未検証"),
        };
        
        let message = entry.validation.as_ref()
            .map(|v| v.customer_message())
            .unwrap_or_else(|| "検証情報なし".to_string());
        
        html.push_str(&format!(r#"        <tr class="{}">
          <td>{}</td>
          <td>{} B</td>
          <td><span class="badge {}">{}</span></td>
          <td>{}</td>
        </tr>
"#,
            status_class,
            escape_html(&entry.original_path),
            entry.bytes_written,
            badge_class,
            badge_label,
            escape_html(&message),
        ));
    }
    
    html.push_str(r#"      </tbody>
    </table>
  </section>
"#);
    
    // フッター
    html.push_str(r#"  <footer>
    <p>本レポートは DDS Recovery Workbench により自動生成されました。</p>
  </footer>
</body>
</html>
"#);
    
    Ok(html)
}
```

#### B-6. `html_internal.rs`

CS 向け HTML。**internal_note_ja、SHA256、出力先パスを含む**:

```rust
use chrono::Local;
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;
use crate::escape::escape_html;

/// CS 向け HTML レポートを生成する。
///
/// 顧客向けに加えて以下を含む:
/// - internal_note_ja (CS 内部メモ)
/// - SHA256 hash
/// - 出力先パス
/// - 優先度スコア
/// - 元の MFT エントリ番号 (source_id)
pub fn render_internal_html(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut html = String::with_capacity(16384);
    
    html.push_str(&format!(r#"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8">
  <title>復旧業務レポート (CS 用) - {}</title>
  <style>
    body {{ font-family: "Yu Gothic", "Hiragino Sans", sans-serif; max-width: 1400px; margin: 1em auto; padding: 1em; font-size: 13px; }}
    h1 {{ border-bottom: 3px solid #1e40af; padding-bottom: 0.3em; color: #1e3a8a; }}
    h2 {{ color: #1e40af; margin-top: 1.5em; }}
    .summary {{ background: #eff6ff; padding: 1em 1.5em; border-radius: 4px; border-left: 4px solid #1e40af; }}
    .warning {{ background: #fef2f2; padding: 0.5em 1em; border-left: 4px solid #dc2626; margin: 1em 0; }}
    table {{ width: 100%; border-collapse: collapse; margin-top: 0.5em; font-size: 12px; }}
    th, td {{ padding: 0.4em 0.5em; text-align: left; border-bottom: 1px solid #ddd; vertical-align: top; }}
    th {{ background: #1e40af; color: white; }}
    tr.valid {{ background: #f0fdf4; }}
    tr.invalid {{ background: #fef2f2; }}
    tr.uncertain {{ background: #fffbeb; }}
    .note {{ color: #6b7280; font-size: 11px; font-style: italic; }}
    .sha {{ font-family: monospace; font-size: 10px; color: #6b7280; }}
    .badge {{ display: inline-block; padding: 0.15em 0.5em; border-radius: 3px; font-size: 11px; font-weight: bold; }}
    .badge-valid {{ background: #16a34a; color: white; }}
    .badge-invalid {{ background: #dc2626; color: white; }}
    .badge-uncertain {{ background: #d97706; color: white; }}
    footer {{ margin-top: 2em; color: #666; font-size: 0.8em; }}
  </style>
</head>
<body>
  <h1>復旧業務レポート（CS 内部用）</h1>
  <p>作成日時: {}</p>
  
  <div class="warning">
    <strong>⚠ 注意:</strong> このレポートは社内業務用です。CS 内部メモを含むため、お客様には共有しないでください。
  </div>
"#,
        Local::now().format("%Y-%m-%d %H:%M"),
        Local::now().format("%Y-%m-%d %H:%M:%S"),
    ));
    
    // サマリ
    html.push_str(&format!(r#"  <section class="summary">
    <h2>サマリ</h2>
    <table>
      <tr><th>項目</th><th>値</th></tr>
      <tr><td>マッチ総数</td><td>{}</td></tr>
      <tr><td>復旧成功</td><td>{} ({:.1}%)</td></tr>
      <tr><td>復旧失敗</td><td>{}</td></tr>
      <tr><td>スキップ</td><td>{}</td></tr>
      <tr><td>処理時間</td><td>{} ms</td></tr>
      <tr><td>復旧バイト総量</td><td>{} bytes</td></tr>
    </table>
    
    <h2>品質判定内訳</h2>
    <table>
      <tr><th>判定</th><th>件数</th></tr>
      <tr><td>✓ Valid</td><td>{}</td></tr>
      <tr><td>✗ Invalid</td><td>{}</td></tr>
      <tr><td>? Uncertain</td><td>{}</td></tr>
    </table>
  </section>
"#,
        report.total_matched,
        report.recovered.len(),
        report.success_rate(),
        report.failed.len(),
        report.skipped.len(),
        report.duration_ms(),
        report.total_bytes_written(),
        report.validated_count(),
        report.invalid_count(),
        report.recovered.len() - report.validated_count() - report.invalid_count(),
    ));
    
    // 復旧ファイル一覧（詳細）
    html.push_str(r#"  <section>
    <h2>復旧ファイル一覧（CS 詳細）</h2>
    <table>
      <thead>
        <tr>
          <th>パス</th>
          <th>サイズ</th>
          <th>判定</th>
          <th>顧客向けメッセージ</th>
          <th>CS 内部メモ</th>
          <th>出力先</th>
          <th>SHA256</th>
        </tr>
      </thead>
      <tbody>
"#);
    
    for entry in &report.recovered {
        let (status_class, badge_class, badge_label) = match entry.validation.as_ref() {
            Some(v) => match v.status {
                ValidationStatus::Valid => ("valid", "badge-valid", "Valid"),
                ValidationStatus::Invalid => ("invalid", "badge-invalid", "Invalid"),
                ValidationStatus::Uncertain => ("uncertain", "badge-uncertain", "Uncertain"),
            },
            None => ("uncertain", "badge-uncertain", "-"),
        };
        
        let customer_msg = entry.validation.as_ref()
            .map(|v| v.customer_message())
            .unwrap_or_else(|| "-".to_string());
        
        let internal_note = entry.validation.as_ref()
            .and_then(|v| v.internal_note().map(|s| s.to_string()))
            .unwrap_or_else(|| "-".to_string());
        
        let sha = entry.sha256.as_deref().unwrap_or("-");
        
        html.push_str(&format!(r#"        <tr class="{}">
          <td>{}</td>
          <td>{}</td>
          <td><span class="badge {}">{}</span></td>
          <td>{}</td>
          <td class="note">{}</td>
          <td>{}</td>
          <td class="sha">{}</td>
        </tr>
"#,
            status_class,
            escape_html(&entry.original_path),
            entry.bytes_written,
            badge_class,
            badge_label,
            escape_html(&customer_msg),
            escape_html(&internal_note),
            escape_html(&entry.output_path.display().to_string()),
            escape_html(sha),
        ));
    }
    
    html.push_str(r#"      </tbody>
    </table>
  </section>
"#);
    
    // 失敗・スキップがあれば追加表示
    if !report.failed.is_empty() {
        html.push_str(r#"  <section>
    <h2>失敗ファイル</h2>
    <table>
      <thead><tr><th>パス</th><th>エラー</th></tr></thead>
      <tbody>
"#);
        for entry in &report.failed {
            html.push_str(&format!(r#"        <tr><td>{}</td><td>{}</td></tr>
"#,
                escape_html(&entry.original_path),
                escape_html(&entry.error_message),
            ));
        }
        html.push_str("      </tbody>\n    </table>\n  </section>\n");
    }
    
    html.push_str(r#"  <footer>
    <p>DDS Recovery Workbench - 内部業務レポート</p>
  </footer>
</body>
</html>
"#);
    
    Ok(html)
}
```

#### B-7. `csv.rs`

CSV エクスポート（全フィールド、外部システム連携用）:

```rust
use dds_recovery::RecoveryReport;
use dds_validators::ValidationStatus;

use crate::error::ReportError;

/// CSV レポートを生成する（全フィールド含む）。
///
/// 用途: 外部システムへのエクスポート、Excel での詳細分析
pub fn render_csv(report: &RecoveryReport) -> Result<String, ReportError> {
    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());
    
    // ヘッダー行
    wtr.write_record(&[
        "source_id",
        "original_path",
        "output_path",
        "bytes_written",
        "is_deleted",
        "priority_score",
        "sha256",
        "validation_status",
        "format_detected",
        "validator_name",
        "customer_message",
        "internal_note",
        "diagnostics",
    ])?;
    
    for entry in &report.recovered {
        let (status, format, validator_name, customer_msg, internal_note, diag) = 
            match entry.validation.as_ref() {
                Some(v) => (
                    match v.status {
                        ValidationStatus::Valid => "valid",
                        ValidationStatus::Invalid => "invalid",
                        ValidationStatus::Uncertain => "uncertain",
                    },
                    v.format_detected.clone().unwrap_or_default(),
                    v.validator_name.clone(),
                    v.customer_message(),
                    v.internal_note().unwrap_or("").to_string(),
                    v.diagnostics.join("; "),
                ),
                None => ("", String::new(), String::new(), String::new(), String::new(), String::new()),
            };
        
        wtr.write_record(&[
            &entry.source_id,
            &entry.original_path,
            &entry.output_path.display().to_string(),
            &entry.bytes_written.to_string(),
            &entry.is_deleted.to_string(),
            &entry.priority_score.to_string(),
            &entry.sha256.clone().unwrap_or_default(),
            status,
            &format,
            &validator_name,
            &customer_msg,
            &internal_note,
            &diag,
        ])?;
    }
    
    let data = wtr.into_inner()
        .map_err(|e| ReportError::Template(e.to_string()))?;
    String::from_utf8(data).map_err(|e| ReportError::Template(e.to_string()))
}
```

#### B-8. `lib.rs`

```rust
//! DDS 復旧レポート生成。
//!
//! RecoveryReport から 3 種類のレポートを生成:
//! - 顧客向け HTML（user_message_ja のみ）
//! - CS 向け HTML（internal_note_ja 含む）
//! - CSV（全フィールド、外部連携用）

pub mod csv;
pub mod error;
pub mod escape;
pub mod html_customer;
pub mod html_internal;

pub use crate::csv::render_csv;
pub use error::ReportError;
pub use html_customer::render_customer_html;
pub use html_internal::render_internal_html;

use std::path::{Path, PathBuf};
use dds_recovery::RecoveryReport;

/// 3 種類のレポートを `output_dir` に書き出す。
///
/// 出力:
/// - `{output_dir}/report_customer.html`
/// - `{output_dir}/report_internal.html`
/// - `{output_dir}/report.csv`
pub fn write_all_reports(
    report: &RecoveryReport,
    output_dir: &Path,
) -> Result<ReportPaths, ReportError> {
    std::fs::create_dir_all(output_dir)?;
    
    let customer_path = output_dir.join("report_customer.html");
    let internal_path = output_dir.join("report_internal.html");
    let csv_path = output_dir.join("report.csv");
    
    std::fs::write(&customer_path, render_customer_html(report)?)?;
    std::fs::write(&internal_path, render_internal_html(report)?)?;
    std::fs::write(&csv_path, render_csv(report)?)?;
    
    Ok(ReportPaths {
        customer_html: customer_path,
        internal_html: internal_path,
        csv: csv_path,
    })
}

/// 生成されたレポートのファイルパス
#[derive(Debug, Clone)]
pub struct ReportPaths {
    pub customer_html: PathBuf,
    pub internal_html: PathBuf,
    pub csv: PathBuf,
}
```

---

## 単体テスト要件（最低 16 件）

### Part A: validators 単体テスト更新（各 validator 既存テスト + 新規）

各 validator のテストで `user_message_ja` と `internal_note_ja` の有無を検証:

1. `validates_minimal_valid_png_with_japanese_message`: Valid 時に `user_message_ja` が日本語、`internal_note_ja` が None
2. `invalid_png_includes_actionable_internal_note`: Invalid 時に `internal_note_ja` が存在し、業務指示を含む
3. `invalid_pdf_extension_mismatch_user_message_is_polite`: 拡張子嘘の場合、顧客向けメッセージが攻撃的でない
4. `uncertain_unknown_extension_includes_internal_action`: 未知拡張子で `internal_note_ja` に「CS 確認」等の指示
5. `customer_message_fallback_works`: `user_message_ja = None` でも `customer_message()` がデフォルト文言を返す

### Part B: report クレート単体テスト

`html_customer.rs`:

6. `customer_html_includes_user_message_ja`: 顧客 HTML に `user_message_ja` が含まれる
7. **`customer_html_excludes_internal_note_ja`**: 顧客 HTML に `internal_note_ja` が**含まれない**（重要）
8. `customer_html_excludes_sha256`: SHA256 が顧客 HTML に含まれない
9. `customer_html_escapes_special_chars`: ファイル名に `<` 等を含む場合の XSS 防止
10. `customer_html_lang_attribute_is_ja`: `<html lang="ja">` 属性付与

`html_internal.rs`:

11. `internal_html_includes_internal_note_ja`: CS 内部メモが CS HTML に含まれる
12. `internal_html_includes_sha256_and_output_path`: 詳細情報が CS HTML に含まれる
13. `internal_html_warns_not_to_share_with_customer`: 警告文「お客様に共有しないでください」が含まれる

`csv.rs`:

14. `csv_has_all_fields_in_header`: 13 列が全部 CSV ヘッダーに含まれる
15. `csv_handles_commas_and_quotes_in_paths`: パス内に `,` や `"` がある場合の適切なエスケープ（csv crate の責務）
16. `csv_writes_internal_note_in_dedicated_column`: internal_note 列が独立して存在

`escape.rs`:

17. `escape_html_handles_all_special_chars`: `< > & " '` すべてエスケープ
18. `escape_html_passes_through_japanese`: 日本語文字（マルチバイト）はそのまま通過

`lib.rs`:

19. `write_all_reports_creates_three_files`: 3 ファイルすべて出力ディレクトリに生成される

## 結合テスト要件（最低 3 件）

`crates/recovery/tests/recovery_with_reports_integration.rs`:

### 1. 混在フィクスチャから 3 形式レポート生成

```rust
#[test]
fn generates_all_three_report_formats_from_mixed_fixture() {
    let img = decompress_fixture("ntfs_mixed_formats");
    // ... volume open, wishlist 構築 ...
    
    let temp_dir = TempDir::new().unwrap();
    let recovery_dir = temp_dir.path().join("recovered");
    let report_dir = temp_dir.path().join("reports");
    
    let engine = RecoveryEngine::new(&recovery_dir);
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    let paths = dds_report::write_all_reports(&report, &report_dir).unwrap();
    
    assert!(paths.customer_html.exists());
    assert!(paths.internal_html.exists());
    assert!(paths.csv.exists());
    
    // 各ファイルが空でないこと
    assert!(paths.customer_html.metadata().unwrap().len() > 1000);
    assert!(paths.internal_html.metadata().unwrap().len() > 1000);
    assert!(paths.csv.metadata().unwrap().len() > 500);
}
```

### 2. 顧客 HTML に internal_note が含まれないこと（重要）

```rust
#[test]
fn customer_html_must_not_contain_internal_notes() {
    // 混在フィクスチャでレポート生成
    let html = dds_report::render_customer_html(&report).unwrap();
    
    // 既知の internal_note 文言が含まれないことを確認
    let forbidden_strings = [
        "再復旧推奨",
        "CS 確認",
        "業務判断",
        "技術調査",
        "validator 追加検討",
        "disk-io 層を確認",
    ];
    
    for forbidden in &forbidden_strings {
        assert!(
            !html.contains(forbidden),
            "Customer HTML should not contain CS-internal phrase: {}",
            forbidden
        );
    }
}
```

### 3. プロダクトデモテスト（最終形）

```rust
#[test]
fn product_demo_full_pipeline_with_reports() {
    let img = decompress_fixture("ntfs_mixed_formats");
    // ... setup ...
    
    let temp_dir = TempDir::new().unwrap();
    let engine = RecoveryEngine::new(temp_dir.path().join("recovered"));
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    
    let report_dir = temp_dir.path().join("reports");
    let paths = dds_report::write_all_reports(&report, &report_dir).unwrap();
    
    println!("\n=== DDS Recovery Workbench - Full Pipeline Demo (Chunk 20) ===\n");
    println!("入力:");
    println!("  ソース: ntfs_mixed_formats.img.zst");
    println!("  希望: 全形式（PNG/JPEG/PDF/GIF/BMP/DOCX）");
    println!();
    println!("復旧結果:");
    println!("  対象: {} ファイル", report.total_matched);
    println!("  成功: {} ファイル", report.recovered.len());
    println!("  品質 ✓: {}", report.validated_count());
    println!("  品質 ✗: {}", report.invalid_count());
    println!();
    println!("出力レポート:");
    println!("  顧客向け HTML: {:?}", paths.customer_html);
    println!("    (お客様に納品可能、internal_note を含まない)");
    println!("  CS 向け HTML:  {:?}", paths.internal_html);
    println!("    (業務管理用、internal_note + SHA256 含む)");
    println!("  CSV:           {:?}", paths.csv);
    println!("    (外部システム連携用、全 13 フィールド)");
    println!();
    println!("=== Phase 1 NTFS-α 完成 ===");
    
    // 顧客 HTML には internal note が一切含まれない
    let customer = std::fs::read_to_string(&paths.customer_html).unwrap();
    assert!(!customer.contains("CS 内部"));
    assert!(!customer.contains("再復旧推奨"));
    
    // CS HTML には逆に含まれる
    let internal = std::fs::read_to_string(&paths.internal_html).unwrap();
    assert!(internal.contains("CS 内部用") || internal.contains("CS"));
}
```

## Cargo.toml 設定

`crates/report/Cargo.toml`:
```toml
[dependencies]
csv = "1.3"
chrono.workspace = true
thiserror.workspace = true
dds-recovery.workspace = true
dds-validators.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

ワークスペースルートに `csv = "1.3"` の追加は不要（report クレートのみで使用）。

## 制約

- **行数目安**:
  - validators メッセージ更新: 各 validator ~20 行追加 × 9 = ~180 行
  - report/src/ 全体: ~500 行 + テスト 200 行
- **単体テスト最低 16 件**
- **結合テスト最低 3 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **顧客 HTML に internal_note_ja が含まれないこと**を機械的に検証

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-validators` が全パス（メッセージ更新後）
- [ ] `cargo test -p dds-report` が全パス（≥16 件）
- [ ] `cargo test --workspace` が全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `customer_html_must_not_contain_internal_notes` テストが pass
- [ ] `product_demo_full_pipeline_with_reports` が pass + 出力が見える
- [ ] 生成された 3 つのレポートを実際にブラウザ/Excel で開いて視覚確認

## 関連 FR 要件

- **FR-REP-01** (顧客向け復旧レポート出力) ← **達成**
- **FR-REP-02** (内部業務管理レポート出力) ← **達成**
- **FR-REP-03** (外部システム連携用 CSV) ← **達成**
- **FR-QUAL-04** (検証結果の多言語サポート) ← 日本語実装

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1 NTFS-α リリース達成 (M5 100%)**
4. 次のステップ候補:
   - **Chunk 21**: case-manager (案件管理基盤)
   - **Chunk 22**: Tauri UI 着手
   - **実機検証**: 中古 NTFS HDD での動作確認

---

## 注意事項

### 顧客 HTML の安全性検証は最重要

`internal_note_ja` を絶対に顧客 HTML に出さないこと。テストで機械的に検証必須:

```rust
let forbidden_phrases = ["再復旧推奨", "CS 確認", "業務判断", ...];
for phrase in forbidden_phrases {
    assert!(!customer_html.contains(phrase));
}
```

### メッセージ文言は実装時に CS と協議推奨

サンプル文言は提案。実装時に DDS の CS チームと文体・トーンを協議して調整可能。
ベースは「礼儀正しく、技術用語を避け、お客様に不安を煽らない」。

### HTML テンプレートは自己完結型

外部 CSS/JS/フォントへのリンクなし。`<style>` 内に全 CSS をインライン。
これにより:
- メールに添付可能
- ネット環境なしでも閲覧可能
- セキュリティリスク低減

### 日本語フォント指定

```css
font-family: "Yu Gothic", "Hiragino Sans", sans-serif;
```

Windows (Yu Gothic) と Mac (Hiragino Sans) どちらでも読みやすい順序。
Linux でも sans-serif でフォールバック。

### PDF 出力は Phase 2

PDF 出力には `genpdf` / `printpdf` 等の重い依存が必要。Phase 1 では HTML 出力を **ブラウザの印刷機能で PDF 化** することで代替可能。実用上は十分。

### CSV のエンコーディング

`csv` crate はデフォルト UTF-8。Excel で UTF-8 CSV を開く際、BOM がないと文字化けする可能性。

**対策案** (Phase 1 では Phase 2 に持ち越し可):
- BOM 付き UTF-8 で書き出す `\u{FEFF}` を先頭に
- または Shift_JIS で書き出す（古い Excel 互換性向上）

Phase 1 はシンプルに UTF-8 (BOM なし)。Excel での文字化け報告があれば対応。

### 認知負荷の考慮

CS 向け HTML は情報量が多いので、視覚的に「警告」「内部メモ」を強調するスタイリング。
顧客向け HTML は逆にシンプルに（情報を抑制）。

### Phase 1 で意図的に除外した機能

- **PDF 直接出力**: ブラウザ印刷で代替
- **Excel 直接出力**: CSV で代替
- **多言語対応**: 日本語のみ（英語等は Phase 2）
- **テンプレートカスタマイズ**: Phase 1 はハードコード
- **会社ロゴ・ヘッダー画像**: Phase 1 はテキストのみ

---

## 質問が必要なケース

- 顧客向けレポートの会社名・ロゴ等のブランディング情報
- CS 向けレポートの情報量（もっと詳細 / もっと簡素）
- CSV の Excel 文字化け対策の優先度（BOM 付与の要否）

---

## 完了報告例

```markdown
## Chunk 20 完了報告

### Part A: validators 3 層メッセージ化
- `crates/validators/src/result.rs` 更新: user_message_ja, internal_note_ja フィールド追加
- 全 9 validator のメッセージを日本語化（PNG/JPEG/PDF/GIF/BMP/ZIP/DOCX/XLSX/PPTX）
- Registry の Uncertain 結果も日本語化

### Part B: report クレート本実装
- `crates/report/src/lib.rs`           (45 行)
- `crates/report/src/error.rs`         (20 行)
- `crates/report/src/escape.rs`        (25 行 + テスト 20 行)
- `crates/report/src/html_customer.rs` (150 行 + テスト 40 行)
- `crates/report/src/html_internal.rs` (170 行 + テスト 40 行)
- `crates/report/src/csv.rs`           (75 行 + テスト 25 行)
- `crates/report/Cargo.toml`

### Part C: 結合テスト
- `crates/recovery/tests/recovery_with_reports_integration.rs` (200 行、3 件)

### 公開 API
- `dds_report::render_customer_html(&RecoveryReport)`
- `dds_report::render_internal_html(&RecoveryReport)`
- `dds_report::render_csv(&RecoveryReport)`
- `dds_report::write_all_reports(&RecoveryReport, &Path)`
- `dds_report::ReportPaths`

### テスト統計
- 単体: 既存 287 + 新規 17 = **304 件 pass**
- 結合: 既存 49 + 新規 3 = **52 件 pass**
- 全 workspace: **356+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 顧客 HTML に internal_note_ja が含まれない（機械テストで検証済）
- ファイル名 XSS 防止（escape_html）動作確認済

### 業務価値の見える化
3 つのレポートが ./reports/ に出力:
- report_customer.html: 顧客納品用（user_message_ja のみ）
- report_internal.html: CS 業務用（internal_note 含む + 警告文付き）
- report.csv:           外部連携用（全 13 フィールド）

### 🎉 マイルストーン
- **Phase 1 NTFS-α リリース達成**
- 顧客への納品可能な成果物が自動生成
- CS 業務効率化の基盤完成

- **関連 FR**: FR-REP-01/02/03, FR-QUAL-04

→ tester エージェントへ引き継ぎお願いします
```
