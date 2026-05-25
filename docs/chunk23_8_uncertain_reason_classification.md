# Chunk 23.8 指示: Uncertain 理由分類 + TXT 分割 (Phase 1.5 最終)

Phase 1.5 の最終チャンク。「自動品質確認の対象外」だったファイルの**理由を細分化**し、業務レポートで明示します。

> 🎯 完了時点で「お客様への品質報告がメリハリある形に到達」する。Phase 1.5 完全完成、検証 PC での実機ドライラン準備完了。

---

## 背景: 業務的な課題

### 現状の限界

```
[Chunk 23.7 完了時点]
品質判定:
  Valid (正常):     9,500 件
  Invalid (要確認):   400 件
  Uncertain (検証外): 100 件 ← なぜ Uncertain か不明

→ CS や お客様: 「Uncertain って何?」「なぜ確認できなかった?」
```

Uncertain には 5 つの異なる理由があります:

1. **対応 Validator なし** (拡張子未対応、独自形式)
2. **暗号化** (パスワード保護)
3. **サイズ超過** (検証上限を超える大きいファイル)
4. **Validator 内部エラー** (パースエラー等)
5. **拡張子不一致** (.jpg だが中身は PDF など)

業務的に区別する価値がある:
- 「対応 Validator なし」→ Workbench の機能限界として CS が説明
- 「暗号化」→ お客様にパスワード提供を依頼
- 「サイズ超過」→ 手動確認を推奨

### Chunk 23.8 完了後

```
[新しい品質判定]
Valid (正常):       9,500 件
Invalid (要確認):     400 件
Uncertain (検証外):   100 件
  内訳:
    対応 Validator なし: 80 件 (xyz, 独自形式等)
    暗号化:               10 件
    サイズ超過:            5 件
    Validator エラー:      3 件
    拡張子不一致:          2 件
```

加えて、お客様向け TXT が業務的に分割される:

```
[現状] レポート/
  └ 要確認ファイル一覧.txt (Invalid のみ)

[Chunk 23.8 後] レポート/
  ├ 破損疑いファイル一覧.txt        (Invalid のみ、お客様への注意喚起)
  └ 自動確認対象外ファイル一覧.txt  (Uncertain、お客様での手動確認依頼)
```

## 目的

7 つの統合された変更:

| Part | 内容 |
|---|---|
| **A** | `UncertainReason` enum の追加 |
| **B** | Validator の Uncertain 分類 (各 validator 更新) |
| **C** | `RecoveryReport::uncertain_breakdown()` 追加 |
| **D** | お客様向け TXT の分割 (破損疑い / 自動確認対象外) |
| **E** | Customer DOCX に Uncertain 内訳表示 |
| **F** | Internal HTML に Uncertain 内訳表示 |
| **G** | CaseOutput / BusinessReportPaths 更新 |

## 対象クレート

- **修正**: `crates/validators/`, `crates/recovery/`, `crates/report/`, `crates/case-manager/`
- **影響**: 既存テスト 20-30 件の修正

## 重要な設計原則

### 業務文言の確定

お客様向け文言は Chouさんが Q3 で確定済み:

```
「現在未対応もしくはファイル形式が特殊、
 ファイルサイズが大きすぎるなどで確認できませんでした」
```

これを「自動確認対象外ファイル一覧.txt」の冒頭で使用。

### Validator 内部の責務

各 validator は「なぜ Uncertain か」を返す責任を持つ:

```rust
// 例: PNG validator
fn validate(&self, content: &[u8]) -> ValidationResult {
    if content.len() < 8 {
        return Uncertain(UncertainReason::ValidatorError {
            message: "ファイルが PNG ヘッダ最小サイズ未満".into()
        });
    }
    if !starts_with_png_signature(content) {
        return Uncertain(UncertainReason::ExtensionMismatch {
            detected_format: detect_format(content)
        });
    }
    // ... 検証ロジック
}
```

### 既存 API への影響

破壊的変更を最小限に:

```rust
// 既存:
pub enum ValidationStatus {
    Valid,
    Invalid,
    Uncertain,
}

// Chunk 23.8 後:
pub enum ValidationStatus {
    Valid,
    Invalid,
    Uncertain(UncertainReason),  // ★ 拡張
}
```

`is_uncertain()` などの判定メソッドは互換性維持。

## 仕様参照

### ビジネス要件

- **FR-QUAL-04** (Uncertain 理由分類) ← 新規達成
- **FR-REP-05** (お客様向け TXT 分割) ← 新規達成

## 実装内容

### Part A: UncertainReason enum

`crates/validators/src/lib.rs` (または専用ファイル) に追加:

```rust
use serde::{Deserialize, Serialize};

/// Uncertain (検証外) の理由
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UncertainReason {
    /// 対応する Validator がない
    /// (拡張子未対応、独自形式、拡張子なし等)
    NoValidatorAvailable,
    
    /// ファイルが暗号化されている (パスワード保護等)
    Encrypted,
    
    /// ファイルが大きすぎて検証スキップ
    TooLargeForValidation { size: u64, threshold: u64 },
    
    /// Validator 内部でエラーが発生 (パースエラー等)
    ValidatorError { message: String },
    
    /// 拡張子と検出形式が一致しない
    /// (例: .jpg だが中身は PDF)
    ExtensionMismatch { detected_format: String },
}

impl UncertainReason {
    /// お客様向けの日本語メッセージ
    pub fn customer_message(&self) -> String {
        match self {
            Self::NoValidatorAvailable => 
                "現在未対応のファイル形式".to_string(),
            Self::Encrypted => 
                "暗号化されているため確認できません".to_string(),
            Self::TooLargeForValidation { size, threshold } => 
                format!("ファイルサイズが大きすぎます ({} 超、上限 {})",
                    format_bytes(*size), format_bytes(*threshold)),
            Self::ValidatorError { .. } => 
                "ファイル形式の確認中にエラーが発生しました".to_string(),
            Self::ExtensionMismatch { detected_format } => 
                format!("拡張子と中身が一致しません (検出: {})", detected_format),
        }
    }
    
    /// 内部向けの簡潔なラベル
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::NoValidatorAvailable => "対応 Validator なし",
            Self::Encrypted => "暗号化",
            Self::TooLargeForValidation { .. } => "サイズ超過",
            Self::ValidatorError { .. } => "Validator エラー",
            Self::ExtensionMismatch { .. } => "拡張子不一致",
        }
    }
}
```

### Part B: ValidationStatus の拡張

`crates/validators/src/lib.rs` の修正:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ValidationStatus {
    Valid,
    Invalid,
    Uncertain(UncertainReason),  // ★ 拡張
}

impl ValidationStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }
    
    pub fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid)
    }
    
    pub fn is_uncertain(&self) -> bool {
        matches!(self, Self::Uncertain(_))
    }
    
    /// Uncertain の場合に理由を取得
    pub fn uncertain_reason(&self) -> Option<&UncertainReason> {
        if let Self::Uncertain(reason) = self {
            Some(reason)
        } else {
            None
        }
    }
}
```

### Part C: ValidatorRegistry の更新

`crates/validators/src/registry.rs`:

```rust
impl ValidatorRegistry {
    pub fn validate(&self, content: &[u8], extension: Option<&str>) -> ValidationResult {
        // 拡張子なし or 未対応形式
        let ext = match extension {
            Some(e) if !e.is_empty() => e.to_lowercase(),
            _ => return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::NoValidatorAvailable),
                detected_format: None,
                // ...
            },
        };
        
        let validator = match self.validators.get(&ext) {
            Some(v) => v,
            None => return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::NoValidatorAvailable),
                detected_format: None,
                // ...
            },
        };
        
        // サイズ超過チェック (定数で上限定義、例: 100 MB)
        const VALIDATION_SIZE_THRESHOLD: u64 = 100 * 1024 * 1024;
        if (content.len() as u64) > VALIDATION_SIZE_THRESHOLD {
            return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::TooLargeForValidation {
                    size: content.len() as u64,
                    threshold: VALIDATION_SIZE_THRESHOLD,
                }),
                detected_format: Some(ext.clone()),
                // ...
            };
        }
        
        // Validator 実行
        validator.validate(content)
    }
}
```

### Part D: 各 Validator の Uncertain 分類

各 validator (PNG, JPEG, PDF, GIF, BMP, ZIP, DOCX, XLSX, PPTX) で:

```rust
// 例: PNG validator
impl PngValidator {
    fn validate(&self, content: &[u8]) -> ValidationResult {
        if content.len() < 8 {
            return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::ValidatorError {
                    message: "PNG ヘッダ最小サイズ未満".into()
                }),
                // ...
            };
        }
        
        let png_signature = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if !content.starts_with(&png_signature) {
            // 別の形式の可能性 → 検出を試みる
            let detected = detect_format_from_magic_bytes(content);
            return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::ExtensionMismatch {
                    detected_format: detected.unwrap_or("unknown".into()),
                }),
                // ...
            };
        }
        
        // 既存の検証ロジック (チャンク整合性等)
        // ...
    }
}

// 例: DOCX validator (ZIP ベース)
impl DocxValidator {
    fn validate(&self, content: &[u8]) -> ValidationResult {
        // ZIP ヘッダチェック
        if !content.starts_with(b"PK") {
            return ValidationResult {
                status: ValidationStatus::Uncertain(UncertainReason::ExtensionMismatch {
                    detected_format: detect_format_from_magic_bytes(content).unwrap_or("unknown".into()),
                }),
                // ...
            };
        }
        
        // ZIP 解凍時に暗号化エラー
        match zip::ZipArchive::new(std::io::Cursor::new(content)) {
            Ok(mut archive) => {
                // ... 既存の検証ロジック
            }
            Err(zip::result::ZipError::UnsupportedArchive(_)) => {
                return ValidationResult {
                    status: ValidationStatus::Uncertain(UncertainReason::Encrypted),
                    // ...
                };
            }
            Err(e) => {
                return ValidationResult {
                    status: ValidationStatus::Uncertain(UncertainReason::ValidatorError {
                        message: format!("ZIP 解凍エラー: {}", e),
                    }),
                    // ...
                };
            }
        }
    }
}
```

### Part E: RecoveryReport の uncertain_breakdown

`crates/recovery/src/report.rs`:

```rust
/// Uncertain の内訳
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UncertainBreakdown {
    pub no_validator: usize,
    pub encrypted: usize,
    pub too_large: usize,
    pub validator_error: usize,
    pub extension_mismatch: usize,
}

impl UncertainBreakdown {
    pub fn total(&self) -> usize {
        self.no_validator + self.encrypted + self.too_large 
        + self.validator_error + self.extension_mismatch
    }
}

impl RecoveryReport {
    /// Uncertain の理由内訳を計算
    pub fn uncertain_breakdown(&self) -> UncertainBreakdown {
        let mut breakdown = UncertainBreakdown::default();
        for entry in &self.recovered {
            if let Some(validation) = &entry.validation {
                if let Some(reason) = validation.status.uncertain_reason() {
                    match reason {
                        UncertainReason::NoValidatorAvailable => breakdown.no_validator += 1,
                        UncertainReason::Encrypted => breakdown.encrypted += 1,
                        UncertainReason::TooLargeForValidation { .. } => breakdown.too_large += 1,
                        UncertainReason::ValidatorError { .. } => breakdown.validator_error += 1,
                        UncertainReason::ExtensionMismatch { .. } => breakdown.extension_mismatch += 1,
                    }
                }
            }
        }
        breakdown
    }
    
    /// 優先データの Uncertain 内訳 (Chunk 23.7 と整合)
    pub fn priority_uncertain_breakdown(&self) -> UncertainBreakdown {
        let mut breakdown = UncertainBreakdown::default();
        for entry in self.recovered.iter().filter(|e| e.is_priority) {
            if let Some(validation) = &entry.validation {
                if let Some(reason) = validation.status.uncertain_reason() {
                    match reason {
                        UncertainReason::NoValidatorAvailable => breakdown.no_validator += 1,
                        UncertainReason::Encrypted => breakdown.encrypted += 1,
                        UncertainReason::TooLargeForValidation { .. } => breakdown.too_large += 1,
                        UncertainReason::ValidatorError { .. } => breakdown.validator_error += 1,
                        UncertainReason::ExtensionMismatch { .. } => breakdown.extension_mismatch += 1,
                    }
                }
            }
        }
        breakdown
    }
}
```

### Part F: お客様向け TXT の分割

`crates/report/src/txt_customer.rs` を 2 関数に分割:

```rust
/// 破損疑いファイル一覧 (Invalid のみ)
pub fn render_invalid_files_txt(report: &RecoveryReport) -> String {
    let mut s = String::new();
    
    s.push_str("=== 破損疑いファイル一覧 ===\n\n");
    s.push_str("以下のファイルは復旧されましたが、自動品質確認で破損の疑いがありました。\n");
    s.push_str("お開きになる前にお気をつけください。\n\n");
    
    let invalid_entries: Vec<_> = report.recovered.iter()
        .filter(|e| e.validation.as_ref()
            .map(|v| v.status.is_invalid())
            .unwrap_or(false))
        .collect();
    
    if invalid_entries.is_empty() {
        s.push_str("該当するファイルはありませんでした。\n");
        return s;
    }
    
    s.push_str(&format!("[ファイル一覧: {} 件]\n\n", invalid_entries.len()));
    
    for (i, entry) in invalid_entries.iter().enumerate() {
        let _ = writeln!(s, "{}. {}", i + 1, entry.original_path);
        let _ = writeln!(s, "   サイズ: {}", format_bytes(entry.bytes_written));
        if let Some(validation) = &entry.validation {
            if let Some(msg) = &validation.customer_message {
                let _ = writeln!(s, "   理由: {}", msg);
            }
        }
        let _ = writeln!(s);
    }
    
    s
}

/// 自動確認対象外ファイル一覧 (Uncertain のみ、Chunk 23.8 新規)
pub fn render_uncertain_files_txt(report: &RecoveryReport) -> String {
    let mut s = String::new();
    
    s.push_str("=== 自動確認対象外ファイル一覧 ===\n\n");
    s.push_str("以下のファイルは復旧されていますが、自動品質確認の対象外でした。\n");
    s.push_str("原因: 現在未対応もしくはファイル形式が特殊、ファイルサイズが大きすぎる\n");
    s.push_str("      などで確認できませんでした\n\n");
    s.push_str("お手元でお開きになってご確認ください。\n\n");
    
    let uncertain_entries: Vec<_> = report.recovered.iter()
        .filter(|e| e.validation.as_ref()
            .map(|v| v.status.is_uncertain())
            .unwrap_or(false))
        .collect();
    
    if uncertain_entries.is_empty() {
        s.push_str("該当するファイルはありませんでした。\n");
        return s;
    }
    
    s.push_str(&format!("[ファイル一覧: {} 件]\n\n", uncertain_entries.len()));
    
    for (i, entry) in uncertain_entries.iter().enumerate() {
        let _ = writeln!(s, "{}. {}", i + 1, entry.original_path);
        let _ = writeln!(s, "   サイズ: {}", format_bytes(entry.bytes_written));
        if let Some(validation) = &entry.validation {
            if let Some(reason) = validation.status.uncertain_reason() {
                let _ = writeln!(s, "   理由: {}", reason.short_label());
            }
        }
        let _ = writeln!(s);
    }
    
    s
}
```

### Part G: CaseOutput / BusinessReportPaths 更新

`crates/case-manager/src/output.rs`:

```rust
impl CaseOutput {
    // 旧: customer_txt_path → 破棄 (Chunk 23.8 で分割)
    
    /// 破損疑いファイル一覧のパス (Invalid のみ、お客様向け)
    pub fn customer_invalid_txt_path(&self) -> PathBuf {
        self.reports_dir().join("破損疑いファイル一覧.txt")
    }
    
    /// 自動確認対象外ファイル一覧のパス (Uncertain のみ、お客様向け)
    pub fn customer_uncertain_txt_path(&self) -> PathBuf {
        self.reports_dir().join("自動確認対象外ファイル一覧.txt")
    }
    
    // 他の path メソッドは維持
}
```

`crates/report/src/business.rs`:

```rust
#[derive(Debug, Clone)]
pub struct BusinessReportPaths {
    pub customer_docx: PathBuf,
    pub customer_invalid_txt: PathBuf,      // 旧 customer_txt から rename + 意味変更
    pub customer_uncertain_txt: PathBuf,    // 新規
    pub internal_html: PathBuf,
    pub csv: PathBuf,
}

pub fn write_business_reports(
    report: &RecoveryReport,
    case_output: &CaseOutput,
) -> Result<BusinessReportPaths, ReportError> {
    std::fs::create_dir_all(case_output.reports_dir())?;
    
    let customer_docx = case_output.customer_docx_path();
    let customer_invalid_txt = case_output.customer_invalid_txt_path();
    let customer_uncertain_txt = case_output.customer_uncertain_txt_path();
    let internal_html = case_output.internal_html_path();
    let csv = case_output.csv_path();
    
    std::fs::write(&customer_docx, render_customer_docx(report)?)?;
    std::fs::write(&customer_invalid_txt, render_invalid_files_txt(report))?;
    std::fs::write(&customer_uncertain_txt, render_uncertain_files_txt(report))?;
    std::fs::write(&internal_html, render_internal_html(report)?)?;
    std::fs::write(&csv, render_csv(report)?)?;
    
    Ok(BusinessReportPaths {
        customer_docx,
        customer_invalid_txt,
        customer_uncertain_txt,
        internal_html,
        csv,
    })
}
```

### Part H: Customer DOCX の Uncertain 内訳

`crates/report/src/docx_customer.rs` の品質確認セクションに追加:

```rust
// 「品質確認」セクション内に追加:

let breakdown = report.uncertain_breakdown();
let uncertain_total = breakdown.total();

if uncertain_total > 0 {
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("【自動確認対象外について】").bold())
    );
    
    let breakdown_text = format!(
        "  対応 Validator なし: {} 件\n  暗号化ファイル: {} 件\n  サイズ超過: {} 件\n  Validator エラー: {} 件\n  拡張子不一致: {} 件",
        breakdown.no_validator,
        breakdown.encrypted,
        breakdown.too_large,
        breakdown.validator_error,
        breakdown.extension_mismatch,
    );
    
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(breakdown_text))
    );
    
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(
            "DDS Workbench の自動品質確認は JPEG / PNG / PDF / Office (Word, Excel, PowerPoint) など主要形式に対応しています。それ以外の形式、暗号化されたファイル、極端に大きいファイル等は自動確認の対象外となります。お手元でお開きになってご確認ください。"
        ))
    );
}
```

### Part I: Internal HTML の Uncertain 内訳

`crates/report/src/html_internal.rs`:

```rust
// 「品質判定内訳」セクション内に追加:

let breakdown = report.uncertain_breakdown();
if breakdown.total() > 0 {
    html.push_str(r#"  <h3>Uncertain (検証外) の内訳</h3>
  <table>
    <tr><th>理由</th><th>件数</th></tr>
"#);
    html.push_str(&format!("    <tr><td>対応 Validator なし</td><td>{}</td></tr>\n", breakdown.no_validator));
    html.push_str(&format!("    <tr><td>暗号化</td><td>{}</td></tr>\n", breakdown.encrypted));
    html.push_str(&format!("    <tr><td>サイズ超過</td><td>{}</td></tr>\n", breakdown.too_large));
    html.push_str(&format!("    <tr><td>Validator エラー</td><td>{}</td></tr>\n", breakdown.validator_error));
    html.push_str(&format!("    <tr><td>拡張子不一致</td><td>{}</td></tr>\n", breakdown.extension_mismatch));
    html.push_str("  </table>\n");
}
```

## 単体テスト要件 (最低 10 件、新規)

### `UncertainReason` (最低 3 件)

1. `uncertain_reason_customer_message_in_japanese`
2. `uncertain_reason_short_label_format`
3. `uncertain_reason_size_too_large_includes_threshold`

### `ValidationStatus` (最低 2 件)

4. `validation_status_uncertain_reason_accessor`
5. `validation_status_serde_round_trip`

### `ValidatorRegistry` (最低 2 件)

6. `registry_returns_no_validator_for_unknown_extension`
7. `registry_returns_too_large_for_oversized_content`

### `UncertainBreakdown` (最低 2 件)

8. `breakdown_counts_each_reason_separately`
9. `breakdown_total_sums_all_categories`

### `txt_customer` (最低 2 件)

10. `invalid_files_txt_contains_only_invalid`
11. `uncertain_files_txt_uses_business_message`

## 結合テスト要件 (最低 2 件)

### 1. TXT 分割の end-to-end

```rust
#[test]
fn business_reports_generates_split_txt_files() {
    // ... setup ...
    let result = execute_business_recovery(/* ... */).unwrap();
    
    assert!(result.report_paths.customer_invalid_txt.exists());
    assert!(result.report_paths.customer_uncertain_txt.exists());
    
    let invalid_content = std::fs::read_to_string(&result.report_paths.customer_invalid_txt).unwrap();
    assert!(invalid_content.contains("破損疑いファイル一覧"));
    
    let uncertain_content = std::fs::read_to_string(&result.report_paths.customer_uncertain_txt).unwrap();
    assert!(uncertain_content.contains("自動確認対象外ファイル一覧"));
    assert!(uncertain_content.contains("現在未対応もしくはファイル形式が特殊"));
}
```

### 2. Phase 1.5 完成 demo

```rust
#[test]
fn product_demo_phase_1_5_final() {
    // ... setup with ntfs_mixed_formats ...
    
    let result = execute_business_recovery(/* ... */).unwrap();
    
    println!("\n=== Phase 1.5 完成 Demo (Chunk 23.8) ===\n");
    println!("案件番号: {}", case.case_id);
    println!();
    
    println!("[復旧結果 - 全体]");
    println!("  該当ファイル: {} 件", result.report.total_matched);
    println!("  品質保証率:   {:.1}%", result.report.quality_assurance_rate());
    println!();
    
    let breakdown = result.report.uncertain_breakdown();
    if breakdown.total() > 0 {
        println!("[Uncertain (検証外) の内訳]");
        println!("  対応 Validator なし: {} 件", breakdown.no_validator);
        println!("  暗号化:               {} 件", breakdown.encrypted);
        println!("  サイズ超過:           {} 件", breakdown.too_large);
        println!("  Validator エラー:     {} 件", breakdown.validator_error);
        println!("  拡張子不一致:         {} 件", breakdown.extension_mismatch);
        println!();
    }
    
    println!("[納品物]");
    println!("  📄 復旧レポート.docx");
    println!("  📄 破損疑いファイル一覧.txt");
    println!("  📄 自動確認対象外ファイル一覧.txt");
    println!("  📄 業務管理レポート.html");
    println!("  📄 report.csv");
    println!();
    println!("=== Phase 1.5 完成 ===");
}
```

## 制約

- **行数目安**:
  - `validators/src/lib.rs`: UncertainReason 等 +60 行
  - `validators/src/registry.rs`: +30 行 (サイズ超過、未対応分岐)
  - `validators/src/*_validator.rs`: 各 +20 行 × 5 = 100 行
  - `recovery/src/report.rs`: UncertainBreakdown +60 行
  - `report/src/txt_customer.rs`: render_uncertain_files_txt +60 行、render_invalid 既存修正
  - `report/src/docx_customer.rs`: Uncertain 内訳 +40 行
  - `report/src/html_internal.rs`: Uncertain 内訳 +30 行
  - `case-manager/src/output.rs`: パス追加 +20 行
  - `report/src/business.rs`: write_business_reports 更新 +20 行
  - 既存テストの調整: 約 50 行
  - 合計: 約 470 行追加・修正
- **単体テスト新規**: 最低 11 件
- **結合テスト**: 最低 2 件
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持**

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス (約 510+ 件)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_phase_1_5_final` が pass + 出力が見える
- [ ] 「破損疑いファイル一覧.txt」が Invalid のみリスト
- [ ] 「自動確認対象外ファイル一覧.txt」が冒頭文言を含む
- [ ] Customer DOCX に Uncertain 内訳セクション
- [ ] Internal HTML に Uncertain 内訳テーブル
- [ ] 5 つの UncertainReason すべてが適切に分類される

## 関連 FR 要件

- **FR-QUAL-04** (Uncertain 理由分類) ← 達成
- **FR-REP-05** (お客様向け TXT 分割) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉🎉🎉 Phase 1.5 完全完成 🎉🎉🎉**
4. 次のステップ:
   - Chouさんが検証 PC で実機ドライラン (約半日〜1 日)
   - Phase 2.1 (Tauri UI) 着手準備

---

## 注意事項

### サイズ超過の閾値

```rust
const VALIDATION_SIZE_THRESHOLD: u64 = 100 * 1024 * 1024;  // 100 MB
```

100 MB は仮の値。業務的に調整可能:
- メモリ消費とのトレードオフ
- 検証時間とのトレードオフ
- 大型動画 (4K mp4) は通常 GB 単位 → 100 MB 超

Phase 1.5 では 100 MB で進め、Phase 2 で調整。

### 暗号化検出の限界

完全な暗号化検出は困難:
- パスワード保護された DOCX/XLSX/ZIP → 検出可能
- 全体暗号化 (BitLocker 等) → 検出不可 (NTFS 自体が暗号化されている)

Phase 1.5 では:
- ZIP ベースの暗号化 (DOCX 等) は検出
- ファイル全体暗号化は Validator エラーとして扱う

### 拡張子不一致の判定

```
ファイル: photo.jpg
中身を確認: PDF ヘッダ "%PDF-"
→ ExtensionMismatch { detected_format: "PDF" }
```

業務的に:
- 「拡張子間違い」のサインとして CS が気づく
- お客様の HDD で何かの理由で拡張子が書き換わった可能性

### Phase 2 への引き継ぎ

Phase 2.1 UI で:
- Uncertain 内訳を視覚的に表示 (円グラフ、表)
- 「自動確認対象外ファイル」を一覧表示 (フィルタ可能)
- 個別ファイルの理由詳細を展開表示

Chunk 23.8 で構造ができているので、UI 側は表示するだけ。

---

## 完了報告例

```markdown
## Chunk 23.8 完了報告 (Phase 1.5 最終チャンク)

### 大幅修正
- crates/validators/src/lib.rs (UncertainReason +60 行)
- crates/validators/src/registry.rs (サイズ超過、未対応分岐 +30 行)
- crates/validators/src/*_validator.rs (各 Uncertain 分類 +100 行)
- crates/recovery/src/report.rs (UncertainBreakdown +60 行)
- crates/report/src/txt_customer.rs (TXT 分割 +60 行)
- crates/report/src/docx_customer.rs (Uncertain 内訳 +40 行)
- crates/report/src/html_internal.rs (Uncertain 内訳 +30 行)
- crates/case-manager/src/output.rs (パス追加 +20 行)
- crates/report/src/business.rs (write_business_reports 更新 +20 行)

### 既存テスト修正
- 約 30 件のテスト調整 (ValidationStatus::Uncertain のシグネチャ変更)

### 新規テスト
- 単体: 11 件
- 結合: 2 件

### テスト統計
- 全 workspace: **510+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 全公開 API に rustdoc

### Phase 1.5 完成
- 27 chunks all complete
- Phase 1 NTFS-α (Chunks 1-20.5): 完成
- Phase 1.5 業務統合 (Chunks 21-23.8): 完成

### 🎉🎉🎉 マイルストーン 🎉🎉🎉
- **Phase 1.5 完全完成**
- お客様への品質報告がメリハリある形に
- 「破損疑い」「自動確認対象外」の業務的区別
- Workbench は R-STUDIO の代替候補として真剣に評価可能
- 検証 PC での実機ドライラン準備完了
- Phase 2.1 (Tauri UI) 着手準備完了

- **関連 FR**: FR-QUAL-04、FR-REP-05 (達成)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんによる実機ドライランへ移行
```
