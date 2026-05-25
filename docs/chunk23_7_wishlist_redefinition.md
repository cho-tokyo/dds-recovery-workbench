# Chunk 23.7 指示: Wishlist 再定義 + ExclusionList + 全件復旧

Phase 1.5 の業務的本質を実装するチャンク。**R-STUDIO 風の「全ファイル復旧 + 除外パターン」**に設計を変更し、**Wishlist を「お客様優先データ」のラベリング**として再定義します。

> 🎯 完了時点で「Workbench は R-STUDIO と同じ復旧範囲で、かつ Workbench 独自の自動品質保証を提供する」状態になる。業務的に R-STUDIO の置き換え候補としての評価が可能に。

---

## 背景: 業務的に正しい設計への変更

### 現状 (Chunks 17-23.6) の問題

```
[現状の Wishlist の意味]
Wishlist = 復旧対象の指定 (Inclusion フィルタ)
  ↓
Wishlist にマッチした「ファイルのみ」復旧
  ↓
Wishlist にないファイルは復旧されない

[業務的な問題]
- お客様: 「全部復旧して」と言われた時の対応が困難
- R-STUDIO の標準運用 (全部選択して不要を除外) と思考が逆
- 「あのファイルが入ってない!」クレーム発生のリスク
```

### Chunk 23.7 後の設計

```
[新しい設計]
Wishlist = お客様優先データのラベリング (品質チェックの強調用)
ExclusionList = 復旧から除外するパターン (システムファイル等)

[復旧フロー]
  ↓
全 user file を復旧 (R-STUDIO 風、ExclusionList で除外)
  ↓
全件精密チェック (validator 9 形式)
  ↓
Wishlist マッチは「お客様優先データ」として is_priority=true
  ↓
レポートで「全体 + 優先データ」の二重表示
```

## 目的

7 つの統合された変更:

| Part | 内容 |
|---|---|
| **A** | `ExclusionList` 構造体追加 (システムファイル除外) |
| **B** | `RecoveryEngine::recover_files` の意味を「全件復旧」に変更 |
| **C** | `RecoveredEntry` に `is_priority` + `matched_wishes` 追加 |
| **D** | `RecoveryReport` の統計メソッド拡張 (優先データ向け) |
| **E** | レポート構造拡張 (全体 + 優先データの二重表示) |
| **F** | `case-manager::execute_business_recovery` のパラメータ追加 |
| **G** | `workbench-dryrun` の対応 |
| **H** | 既存テストの全面マイグレーション (影響範囲広い) |

## 対象クレート

- **新規**: `crates/wish-match/` (ExclusionList 追加)
- **大幅修正**: `crates/recovery/`, `crates/report/`
- **修正**: `crates/case-manager/`, `crates/workbench-dryrun/`
- **影響テスト**: 全 Phase 1 / Phase 1.5 のテスト

## 重要な設計原則

### Wishlist の意味の明確な再定義

```rust
// Phase 1 (旧):
/// 復旧対象のファイル指定
pub struct Wishlist { ... }

// Phase 1.5 Chunk 23.7 (新):
/// お客様優先データのラベリング。復旧範囲には影響しない。
/// 復旧後の品質レポートで「お客様が特に重要視するデータ」として強調表示される。
pub struct Wishlist { ... }
```

実装は同じ。**意味とドキュメントだけ変更**。`matched_wishes` フィールドで「どの Wishlist 項目にマッチしたか」を記録し、レポートで活用。

### ExclusionList のデフォルト

業務的に「絶対に復旧しない」システム系を組み込み:

```rust
pub fn default_system_exclusions() -> Self {
    Self {
        patterns: vec![
            // Windows システム
            ExclusionPattern::PathPrefix("\\Windows\\".into()),
            ExclusionPattern::PathPrefix("\\Program Files\\".into()),
            ExclusionPattern::PathPrefix("\\Program Files (x86)\\".into()),
            
            // NTFS メタデータ / ゴミ箱
            ExclusionPattern::PathPrefix("\\$Recycle.Bin\\".into()),
            ExclusionPattern::PathPrefix("\\System Volume Information\\".into()),
            ExclusionPattern::PathPrefix("\\$Extend\\".into()),
            
            // NTFS システムファイル ($MFT, $Bitmap 等)
            ExclusionPattern::NameStartsWith("$".into()),
        ],
    }
}
```

### 既存 `is_user_file()` との関係

`NtfsFile::is_user_file()` (既存) は NTFS の system file flag をチェック。ExclusionList はパスベースの追加除外:

```
復旧フロー:
  for file in volume.iter_files():
    if !file.is_user_file(): continue        ← NTFS レベル除外
    if exclusions.matches(&file): continue   ← 業務レベル除外
    // 復旧実行
```

## 仕様参照

### ビジネス要件

- **FR-REC-05** (全件復旧、業務適用) ← 新規達成
- **FR-REC-06** (システムファイル除外) ← 新規達成
- **FR-REP-04** (優先データの強調表示) ← 拡張

## 実装内容

### Part A: ExclusionList の追加

#### `crates/wish-match/src/exclusion.rs` (新規)

```rust
use std::path::Path;
use serde::{Deserialize, Serialize};

/// 復旧から除外するパターンのリスト。
///
/// 業務的に「絶対に復旧しない」システムファイルを排除するために使う。
/// デフォルトは `default_system_exclusions()` で Windows / NTFS のシステム系を網羅。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExclusionList {
    pub patterns: Vec<ExclusionPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ExclusionPattern {
    /// パスがこの接頭辞で始まる場合に除外 (case-insensitive)
    /// 例: `\Windows\`, `\Program Files\`
    PathPrefix(String),
    
    /// ファイル名 (パスの末尾要素) がこの文字で始まる場合に除外
    /// 例: `$` で始まるシステムファイル
    NameStartsWith(String),
    
    /// 拡張子による除外 (大文字小文字無視)
    /// 例: `tmp`, `bak` (ただし業務的にはあまり使わない)
    Extension(String),
}

impl ExclusionList {
    /// DDS 業務標準の除外パターン。
    /// Windows システム、NTFS メタデータ、ゴミ箱を除外。
    pub fn default_system_exclusions() -> Self {
        Self {
            patterns: vec![
                // Windows システム
                ExclusionPattern::PathPrefix("\\Windows\\".into()),
                ExclusionPattern::PathPrefix("\\Program Files\\".into()),
                ExclusionPattern::PathPrefix("\\Program Files (x86)\\".into()),
                
                // NTFS メタデータ / ゴミ箱
                ExclusionPattern::PathPrefix("\\$Recycle.Bin\\".into()),
                ExclusionPattern::PathPrefix("\\System Volume Information\\".into()),
                ExclusionPattern::PathPrefix("\\$Extend\\".into()),
                
                // NTFS システムファイル
                ExclusionPattern::NameStartsWith("$".into()),
            ],
        }
    }
    
    /// 何も除外しない空リスト (テスト用)
    pub fn empty() -> Self {
        Self { patterns: Vec::new() }
    }
    
    /// パターン追加
    pub fn add(mut self, pattern: ExclusionPattern) -> Self {
        self.patterns.push(pattern);
        self
    }
    
    /// 指定されたパスが除外対象か判定 (case-insensitive)
    pub fn matches(&self, path: &str) -> bool {
        let lower = path.to_lowercase();
        for pattern in &self.patterns {
            match pattern {
                ExclusionPattern::PathPrefix(prefix) => {
                    if lower.starts_with(&prefix.to_lowercase()) {
                        return true;
                    }
                }
                ExclusionPattern::NameStartsWith(prefix) => {
                    let filename = filename_from_path(&lower);
                    if filename.starts_with(&prefix.to_lowercase()) {
                        return true;
                    }
                }
                ExclusionPattern::Extension(ext) => {
                    if lower.ends_with(&format!(".{}", ext.to_lowercase())) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn filename_from_path(path: &str) -> &str {
    match path.rfind('\\') {
        Some(pos) => &path[pos + 1..],
        None => path,
    }
}
```

#### `crates/wish-match/src/lib.rs` 更新

```rust
pub mod exclusion;
pub use exclusion::{ExclusionList, ExclusionPattern};
```

#### `crates/wish-match/src/lib.rs` の Wishlist ドキュメント更新

```rust
/// お客様優先データのラベリング (品質チェックの強調用)。
///
/// **Phase 1 までの意味**: 復旧対象のファイル指定 (Inclusion フィルタ)
/// **Phase 1.5 Chunk 23.7 以降**: お客様優先データのラベリング、復旧範囲には影響しない
///
/// 復旧は ExclusionList で除外されないすべての user file が対象。
/// Wishlist にマッチしたファイルは RecoveredEntry::is_priority = true となり、
/// レポート上で「お客様優先データ」として強調表示される。
pub struct Wishlist {
    pub wishes: Vec<Wish>,
}
```

### Part B: recovery の動作変更

#### `crates/recovery/src/engine.rs` の修正

```rust
impl RecoveryEngine {
    /// すべての user file を復旧し、Wishlist マッチを「優先」としてラベリングする。
    ///
    /// 復旧対象:
    /// - `NtfsFile::is_user_file()` が true (NTFS システムファイルを除外)
    /// - かつ `exclusions` にマッチしない (業務的システムファイルを除外)
    ///
    /// Wishlist の役割:
    /// - 復旧範囲には影響しない (全 user file が復旧される)
    /// - マッチしたファイルは `RecoveredEntry::is_priority = true`
    /// - レポート上で「お客様優先データ」として強調
    pub fn recover_files<F>(
        &self,
        volume: &mut NtfsVolume<F>,
        wishlist: &Wishlist,
        exclusions: &ExclusionList,
    ) -> Result<RecoveryReport, RecoveryError>
    where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();
        let mut recovered = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();
        let mut total_matched = 0;
        
        for result in volume.iter_files() {
            let file = match result {
                Ok(f) => f,
                Err(e) => {
                    skipped.push(SkippedEntry {
                        reason: format!("ファイル列挙エラー: {}", e),
                    });
                    continue;
                }
            };
            
            // NTFS システムファイル除外
            if !file.is_user_file() {
                continue;
            }
            
            // ディレクトリ除外 (現状の挙動を維持)
            if file.is_directory {
                continue;
            }
            
            // 業務的システムファイル除外
            if exclusions.matches(&file.path) {
                continue;
            }
            
            total_matched += 1;
            
            // Wishlist マッチ確認 (優先度判定)
            let wish_matches = wishlist.match_file(&file);
            let is_priority = !wish_matches.is_empty();
            let matched_wishes: Vec<String> = wish_matches.iter()
                .map(|w| w.label.clone())
                .collect();
            let priority_score = wish_matches.iter()
                .map(|w| w.priority.score())
                .max()
                .unwrap_or(0);
            
            // 復旧実行
            let output_path = compute_output_path(&self.config, &file)?;
            let bytes_written = perform_recovery(&file, &output_path)?;
            
            // 全件精密チェック
            let validation = run_validators(&output_path);
            
            recovered.push(RecoveredEntry {
                source_id: file.entry_index,
                original_path: file.path.clone(),
                output_path,
                bytes_written,
                is_deleted: file.is_deleted,
                priority_score,
                is_priority,
                matched_wishes,
                sha256: compute_sha256(&output_path)?,
                validation: Some(validation),
            });
        }
        
        let finished_at = Utc::now();
        
        Ok(RecoveryReport {
            started_at,
            finished_at,
            total_matched,
            recovered,
            failed,
            skipped,
            wish_labels: wishlist.wishes.iter().map(|w| w.label.clone()).collect(),
        })
    }
}
```

### Part C: RecoveredEntry の拡張

#### `crates/recovery/src/report.rs` 更新

```rust
pub struct RecoveredEntry {
    // 既存フィールド (順序は維持)
    pub source_id: u64,
    pub original_path: String,
    pub output_path: PathBuf,
    pub bytes_written: u64,
    pub is_deleted: bool,
    pub priority_score: u32,
    pub sha256: String,
    pub validation: Option<ValidationResult>,
    
    // 新規フィールド
    pub is_priority: bool,
    pub matched_wishes: Vec<String>,
}
```

### Part D: RecoveryReport の統計拡張

```rust
impl RecoveryReport {
    // 既存メソッド (維持): success_rate, recovery_success_rate, quality_assurance_rate, etc.
    
    // 新規: 優先データのみの統計
    
    /// 優先データの件数
    pub fn priority_count(&self) -> usize {
        self.recovered.iter().filter(|e| e.is_priority).count()
    }
    
    /// 優先データ中の Valid 件数
    pub fn priority_validated_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.is_priority)
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_valid())
                .unwrap_or(false))
            .count()
    }
    
    /// 優先データ中の Invalid 件数
    pub fn priority_invalid_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.is_priority)
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_invalid())
                .unwrap_or(false))
            .count()
    }
    
    /// 優先データ中の Uncertain 件数
    pub fn priority_uncertain_count(&self) -> usize {
        self.recovered.iter()
            .filter(|e| e.is_priority)
            .filter(|e| e.validation.as_ref()
                .map(|v| v.status.is_uncertain())
                .unwrap_or(true))
            .count()
    }
    
    /// 優先データの品質保証率
    pub fn priority_quality_assurance_rate(&self) -> f64 {
        let count = self.priority_count();
        if count == 0 { return 0.0 }
        (self.priority_validated_count() as f64) / (count as f64) * 100.0
    }
    
    /// 優先データの合計バイト数
    pub fn priority_total_bytes(&self) -> u64 {
        self.recovered.iter()
            .filter(|e| e.is_priority)
            .map(|e| e.bytes_written)
            .sum()
    }
}
```

### Part E: レポート構造の更新

#### Internal HTML 

```rust
// crates/report/src/html_internal.rs の render_internal_html() を更新

// 既存の「復旧結果」「品質判定内訳」セクションに加えて:

// ===== お客様優先データのサマリ (新規) =====
let priority_count = report.priority_count();
if priority_count > 0 {
    html.push_str(&format!(r#"  <h2>お客様優先データ (Wishlist マッチ)</h2>
  <table>
    <tr><th>該当ファイル数</th><td><span class="metric">{}</span> 件</td></tr>
    <tr><th>復旧データ量</th><td>{}</td></tr>
    <tr><th>品質保証率</th><td><span class="metric">{:.1}%</span></td></tr>
  </table>
  <table>
    <tr><th>判定</th><th>件数</th></tr>
    <tr><td>✓ Valid (正常)</td><td>{}</td></tr>
    <tr><td>✗ Invalid (要確認)</td><td>{}</td></tr>
    <tr><td>? Uncertain (検証外)</td><td>{}</td></tr>
  </table>
"#,
        priority_count,
        format_bytes(report.priority_total_bytes()),
        report.priority_quality_assurance_rate(),
        report.priority_validated_count(),
        report.priority_invalid_count(),
        report.priority_uncertain_count(),
    ));
}
```

#### Customer DOCX

```rust
// crates/report/src/docx_customer.rs の render_customer_docx() を更新

// ご指定条件の後、復旧結果サマリの前に「優先データ」セクション追加:

if report.priority_count() > 0 {
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ ご指定優先データの結果").size(28).bold())
    );
    
    let priority_table = Table::new(vec![
        make_kv_row("該当ファイル数", &format!("{} 件", report.priority_count())),
        make_kv_row("復旧データ量", &format_bytes(report.priority_total_bytes())),
        make_kv_row("正常確認済み", &format!("{} 件 ({:.1}%)", 
            report.priority_validated_count(), report.priority_quality_assurance_rate())),
        make_kv_row("要ご確認", &format!("{} 件", report.priority_invalid_count())),
        make_kv_row("自動確認対象外", &format!("{} 件", report.priority_uncertain_count())),
    ]);
    docx = docx.add_table(priority_table);
    docx = docx.add_paragraph(Paragraph::new());
}

// その後、既存の「全体」のセクション (「ご指定条件」「復旧結果サマリ」など) は維持
// ラベルを「全体」と明示:
docx = docx.add_paragraph(
    Paragraph::new()
        .add_run(Run::new().add_text("■ 復旧結果サマリ (全体)").size(28).bold())
);
```

#### CSV

```rust
// crates/report/src/csv.rs に新規列追加

wtr.write_record(&[
    "source_id",
    "original_path",
    "output_path",
    "bytes_written",
    "is_deleted",
    "is_priority",          // 新規
    "matched_wishes",       // 新規
    "priority_score",
    "sha256",
    "validation_status",
    "format_detected",
    "validator_name",
    "customer_message",
    "internal_note",
    "diagnostics",
])?;

// 各行で:
// is_priority: "true" or "false"
// matched_wishes: ";" 区切り (例: "Word ファイル全部;Office 系")
```

### Part F: case-manager の execute_business_recovery 更新

```rust
pub fn execute_business_recovery<F>(
    case: &mut Case,
    drive_root: impl AsRef<Path>,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
    exclusions: &ExclusionList,  // 新規パラメータ
) -> Result<BusinessRecoveryResult, BusinessRecoveryError>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let case_output = CaseOutput::new(case.case_id.clone(), drive_root.as_ref().to_path_buf());
    case_output.create_all_dirs()?;
    
    let config = RecoveryConfig::from_case_output(&case_output);
    let engine = RecoveryEngine::with_config(config);
    let report = engine.recover_files(volume, wishlist, exclusions)?;  // exclusions 渡す
    
    let report_paths = dds_report::write_business_reports(&report, &case_output)?;
    
    case.output_dir = Some(case_output.root());
    case.recovery_report_summary = Some(summarize_report(&report));
    case.wishlist = Some(wishlist.clone());
    
    Ok(BusinessRecoveryResult { case_output, report, report_paths })
}
```

### Part G: workbench-dryrun の更新

```rust
// crates/workbench-dryrun/src/commands/recover.rs

use dds_wish_match::{ExclusionList, Wishlist};

pub fn run() -> Result<()> {
    // ... 既存処理 ...
    
    // 除外パターンは業務標準のデフォルトを使用
    let exclusions = ExclusionList::default_system_exclusions();
    
    // 確認画面で除外パターンも表示
    println!("除外パターン:");
    println!("  - Windows / Program Files フォルダ");
    println!("  - $Recycle.Bin, System Volume Information");
    println!("  - $ で始まるシステムファイル");
    println!();
    
    // ... 確認 ...
    
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
        &exclusions,  // 新規
    ).context("復旧の実行に失敗しました")?;
    
    // ... 既存処理 ...
}
```

### Part H: 既存テストの全面マイグレーション

#### Phase 1 のテスト修正方針

「Wishlist にマッチしたものだけ復旧される」前提のテストを修正:

```rust
// 旧 (Chunk 17 のテスト):
#[test]
fn recover_only_matched_files() {
    let wishlist = Wishlist::new().add(Wish::new(WishItem::Extension("png".into()), "PNG"));
    let report = engine.recover_files(&mut volume, &wishlist).unwrap();
    assert_eq!(report.recovered.len(), 3);  // PNG 3 件のみ
}

// 新 (Chunk 23.7):
#[test]
fn recover_all_files_with_priority_marking() {
    let wishlist = Wishlist::new().add(Wish::new(WishItem::Extension("png".into()), "PNG"));
    let exclusions = ExclusionList::default_system_exclusions();
    let report = engine.recover_files(&mut volume, &wishlist, &exclusions).unwrap();
    
    // 全 user file が復旧される (除外パターン適用後)
    assert!(report.recovered.len() > 3);  // 3 件以上
    
    // Wishlist マッチは is_priority = true
    let priority_count = report.recovered.iter().filter(|e| e.is_priority).count();
    assert_eq!(priority_count, 3);  // PNG 3 件
    
    // matched_wishes に Wishlist のラベルが入る
    let png_entry = report.recovered.iter()
        .find(|e| e.is_priority)
        .unwrap();
    assert!(png_entry.matched_wishes.contains(&"PNG".to_string()));
}
```

**影響箇所**:
- `crates/recovery/src/engine.rs` の全テスト (10+ 件)
- `crates/recovery/tests/*.rs` (5+ 件)
- `crates/report/tests/*.rs` (Chunk 20.5 で書いたもの、5+ 件)
- `crates/case-manager/tests/business_flow_integration.rs` (Chunk 23 で書いたもの)
- 各種 product_demo テスト (Chunks 9-23 で書いたもの、10+ 件)

#### 既存 product_demo の出力変化

```
[Chunk 23 までの product_demo_phase_1_5_complete (例)]
該当ファイル:    14 件
復旧成功率:      100.0%
品質保証率:      71.4%

[Chunk 23.7 後の product_demo_phase_1_5_complete (例)]
該当ファイル (全体): 30 件 (フィクスチャの全 user file 数)
復旧成功率:          100.0%
品質保証率 (全体):   85.0%

優先データ (Wishlist マッチ):
  該当: 14 件
  品質保証率: 71.4%
```

数字が変わるので、全 product_demo のアサーションを調整。

#### 新規テスト

##### ExclusionList (最低 6 件)

1. `exclusion_path_prefix_matches_windows_folder`
2. `exclusion_path_prefix_case_insensitive`
3. `exclusion_name_starts_with_dollar_sign`
4. `exclusion_default_includes_windows_system`
5. `exclusion_empty_matches_nothing`
6. `exclusion_add_chain_pattern`

##### 全件復旧 (最低 3 件)

7. `recover_all_user_files_when_no_wishlist_match`: Wishlist が空でも全 user file 復旧
8. `recover_excludes_system_files_via_exclusions`: ExclusionList が効く
9. `recover_marks_wishlist_match_as_priority`: マッチは is_priority=true

##### 優先データ統計 (最低 3 件)

10. `priority_count_only_counts_marked_entries`
11. `priority_quality_assurance_rate_calculated_separately`
12. `report_can_compute_both_overall_and_priority_stats`

##### レポート (最低 2 件)

13. `internal_html_shows_priority_section_when_priority_present`
14. `customer_docx_shows_priority_section_when_priority_present`

## 単体テスト要件 (最低 14 件、新規)
+ 既存テスト全部の修正

## 結合テスト要件 (最低 2 件)

### 1. 業務フロー end-to-end (更新版)

```rust
#[test]
fn full_business_flow_recovers_all_files_with_priority() {
    // ... setup ...
    
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("png".into()), "PNG 全部"));
    let exclusions = ExclusionList::default_system_exclusions();
    
    let result = execute_business_recovery(
        &mut case, drive_path.path(), &mut volume, &wishlist, &exclusions
    ).unwrap();
    
    // 全 user file 復旧
    assert!(result.report.recovered.len() > 3);
    
    // PNG だけが is_priority
    assert!(result.report.priority_count() > 0);
    let priority_paths: Vec<_> = result.report.recovered.iter()
        .filter(|e| e.is_priority)
        .map(|e| e.original_path.clone())
        .collect();
    assert!(priority_paths.iter().all(|p| p.to_lowercase().ends_with(".png")));
}
```

### 2. プロダクトデモ (Chunk 23.7 完成版)

```rust
#[test]
fn product_demo_phase_1_5_business_aligned() {
    // ... setup ...
    
    println!("\n=== Phase 1.5 Business-Aligned Demo (Chunk 23.7) ===\n");
    println!("[業務フロー]");
    println!("  Workbench は R-STUDIO 風の全件復旧を実施");
    println!("  Wishlist は『お客様優先データ』としてレポートで強調");
    println!();
    println!("案件番号: {}", case.case_id);
    println!();
    println!("[復旧結果 - 全体]");
    println!("  該当ファイル: {} 件", result.report.total_matched);
    println!("  復旧成功率:   {:.1}%", result.report.recovery_success_rate());
    println!("  品質保証率:   {:.1}%", result.report.quality_assurance_rate());
    println!();
    println!("[復旧結果 - お客様優先データ]");
    println!("  該当ファイル: {} 件", result.report.priority_count());
    println!("  品質保証率:   {:.1}%", result.report.priority_quality_assurance_rate());
    println!();
    println!("[除外パターン]");
    println!("  Windows / Program Files");
    println!("  $Recycle.Bin / System Volume Information");
    println!("  $ で始まるシステムファイル");
    println!();
    println!("=== R-STUDIO 風業務フロー対応完成 ===");
}
```

## 制約

- **行数目安**:
  - `wish-match/src/exclusion.rs`: 120 行 + テスト 80 行
  - `recovery/src/engine.rs` 修正: 100 行追加 + 既存修正
  - `recovery/src/report.rs` 修正: 100 行追加 (統計メソッド)
  - `report/src/html_internal.rs` 修正: 50 行追加
  - `report/src/docx_customer.rs` 修正: 40 行追加
  - `report/src/csv.rs` 修正: 20 行追加
  - `case-manager/src/orchestration.rs` 修正: 10 行
  - `workbench-dryrun/src/commands/recover.rs` 修正: 30 行
  - 既存テスト修正: 500 行以上の作業
  - 合計: 約 1000 行追加・修正
- **単体テスト新規**: 最低 14 件
- **結合テスト**: 最低 2 件 (既存は影響対応で修正)
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持**

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス (約 480+ 件)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_phase_1_5_business_aligned` が pass + 出力が見える
- [ ] 全 product_demo テストが新挙動に対応した出力を生成
- [ ] CRM 貼り付けテキストには影響なし (diagnostic は変更されない)
- [ ] Internal HTML / Customer DOCX に「お客様優先データ」セクション
- [ ] CSV に is_priority / matched_wishes 列
- [ ] ExclusionList::default_system_exclusions が業務的に正しい (Windows\ 等)

## 関連 FR 要件

- **FR-REC-05** (全件復旧、業務適用) ← 達成
- **FR-REC-06** (システムファイル除外) ← 達成
- **FR-REP-04** (優先データ強調表示) ← 拡張

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 R-STUDIO 風業務フロー対応完成**
4. 次のステップ:
   - **Chunk 23.8**: Uncertain の理由分類 + TXT 分割
   - その後、検証 PC で実機ドライラン
   - Phase 2.1 着手

---

## 注意事項

### Wishlist が空のときの挙動

```rust
let wishlist = Wishlist::new();  // 空
// → 全 user file 復旧、is_priority は全部 false
// → レポートで「優先データ」セクションは省略
```

これは業務的に自然 (お客様が「全部復旧して」と言った場合)。

### 除外パターンの大文字小文字

Windows のファイルシステムは大文字小文字を区別しない。`ExclusionList::matches` は内部で `to_lowercase()` してから比較。

### 既存テストへの影響範囲の見積もり

- recover_only_matched_* 系テスト: 約 10 件
- report の件数アサーション: 約 8 件
- product_demo の数字: 約 10 件
- 統合テスト: 約 5 件

合計 30+ 件のテスト修正。テスト 1 件あたり平均 5 行修正で 150 行。慎重に進める必要あり。

### Phase 2 への引き継ぎ

Phase 2.1 UI で:
- ExclusionList の編集 UI (デフォルト + カスタム)
- Wishlist の編集 UI (個別ファイル選択も含む R-STUDIO 風)
- 優先データの強調表示 (UI 上で色分け)

Chunk 23.7 で構造ができているので、UI 側は表示するだけ。

### Phase 1.5 で意図的に保持する設計

- **個別ファイル選択は未対応**: Wishlist のパターンマッチのみ
- **除外パターンの動的編集は未対応**: デフォルトのみ
- **品質チェックの並列化未対応**: シリアル実行 (Phase 2 で検討)

---

## 完了報告例

```markdown
## Chunk 23.7 完了報告

### 新規ファイル
- crates/wish-match/src/exclusion.rs (120 行 + テスト 80 行)

### 大幅修正
- crates/recovery/src/engine.rs (100 行追加、recover_files 動作変更)
- crates/recovery/src/report.rs (100 行追加、priority_* メソッド)
- crates/report/src/html_internal.rs (50 行追加、優先データセクション)
- crates/report/src/docx_customer.rs (40 行追加、優先データセクション)
- crates/report/src/csv.rs (is_priority, matched_wishes 列追加)
- crates/case-manager/src/orchestration.rs (exclusions パラメータ)
- crates/workbench-dryrun/src/commands/recover.rs (ExclusionList 使用)

### テスト修正
- 既存テスト 30+ 件のアサーション調整
- 全 product_demo テストの新挙動対応
- 新規テスト 14+ 件

### 統計
- 全 workspace: **490+ 件 pass**

### 業務的成果
- R-STUDIO 風の全件復旧に対応
- Wishlist を「お客様優先データ」として再定義
- レポートで「全体」と「優先データ」の二重表示
- システムファイルの自動除外 (Windows\, $* 等)

### 🎉 マイルストーン
- **業務的に正しい設計に到達**
- 月 800 件の案件に対応可能な復旧範囲
- お客様優先データのメリハリある報告

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chunk 23.8 (Uncertain 分類 + TXT 分割) または検証 PC ドライラン
```
