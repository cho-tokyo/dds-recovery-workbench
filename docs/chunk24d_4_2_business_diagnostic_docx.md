# Chunk 24d-4-2 指示: 営業向け診断書 (DOCX) 生成

Phase 1.5 拡張の **第 4 段階 (後編)**。診断結果を業務管理用 + お客様用の 2 セクション構造の DOCX として生成し、営業の業務基盤を完成させる。

> 🎯 完了時点で「営業が診断結果を DOCX として保存・印刷・共有できる」状態に到達。Workbench が「業務的な文書化ツール」として機能する。

---

## 全体像 (Chunk 24d シリーズ)

```
✅ Chunk 24d-1: 物理ディスクアクセス層
✅ Chunk 24d-2: パーティションテーブル解析
✅ Chunk 24d-3: NtfsVolume との統合
✅ Chunk 24d-4-1: 業務的診断項目の拡充
✅ Chunk 24d-4-1.5: 業務的説明文の追加
🚧 Chunk 24d-4-2: 営業向け診断書 (DOCX) ← 本指示書
⏳ Chunk 24d-4-3: 実機ドライランとフィードバック反映
```

## 背景: 営業の業務的なニーズ

Chouさんの業務観点:

```
[現状]
診断結果は CLI 表示 + CRM 貼り付けテキスト
   ↓
営業が手作業で CRM にコピペ
お客様への説明資料を別途作成

[業務的な期待]
診断完了時に「営業向け診断書.docx」を自動生成
   ↓
業務管理用セクション: 営業の内部確認、見積根拠
お客様用セクション: お客様への説明資料 (口頭説明の台本)
   ↓
業務効率の向上 + 業務的な一貫性
```

### 業務的なゴール

```
[DOCX があれば...]
- 営業が PC で確認、必要なら印刷
- お客様セクションを口頭説明の台本に使用
- 業務管理用セクションを CRM に添付
- 案件ごとに業務記録として保存
- 上司への報告資料としても使用可能
```

## 本チャンクのスコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | 営業向け診断書 DOCX の生成 (`業務診断書.docx`) |
| **B** | 業務管理用セクション (技術詳細 + 業務サマリ + 業務説明) |
| **C** | お客様用セクション (平易な表現 + お客様への説明 + 免責) |
| **D** | レポート生成の workbench-dryrun への統合 |
| **E** | CLI 出力で DOCX 保存パスを表示 |

### 含まないもの

```
✗ 実機ドライラン → Chunk 24d-4-3
✗ DOCX を直接お客様に渡す機能 (社内保存のみ)
✗ 営業向け診断書の自動メール送信 (Phase 2)
✗ お客様用 DOCX の単独切り出し (Phase 2)
✗ PDF 変換 (Phase 2)
```

## 対象クレート

- **新規ファイル**: `crates/report/src/business_diagnostic_docx.rs`
- **修正**: `crates/report/src/lib.rs`
- **修正**: `crates/workbench-dryrun/src/commands/diagnose.rs` (DOCX 生成統合)
- **修正**: `crates/case-manager/src/storage.rs` (保存パス管理)

## 重要な設計原則

### 既存の DOCX 生成パターンに従う

```
[Phase 1 で確立された DOCX 生成]
crates/report/src/business_report_docx.rs (Chunk 14-16 あたりで実装)
- お客様向け復旧レポート.docx
- Chunk 24a で簡素化済み

[本チャンクでの方針]
既存の DOCX 生成ライブラリ (docx-rs 等) を継続使用
既存のスタイル (フォント、見出し、レイアウト) を踏襲
業務的な一貫性を保つ
```

### 2 セクション構造 (Q4=a 確定)

```
[業務的な役割分担]
業務管理用セクション (前半)
  → 営業の内部確認、見積根拠
  → 技術詳細、業務的指標、業務説明文

お客様用セクション (後半)
  → お客様への説明資料、口頭説明の台本
  → 平易な表現、専門用語ゼロ、免責注釈付き

[同一ファイル]
1 つの DOCX で 2 セクション (Q4=a)
業務的にシンプル、1 ファイル管理
```

### 保存先

```
C:\cases\{案件番号}\
├ 案件情報.json
├ 診断結果_CRM貼り付け用.txt
├ 業務管理レポート.html
├ 復旧詳細.csv
└ ★ 業務診断書.docx ← 新規 (Chunk 24d-4-2)
```

社内保存のみ (お客様への直接納品ではない)。

### お客様向け文言の責任

Chunk 24d-4-1.5 で確立した原則:

```
[業務的な原則 (Q3=a+c)]
- 法的責任は持たない、参考情報として提供
- 免責注釈を必ず付与
- 個別案件の状況により異なる場合があります
```

DOCX のお客様用セクション末尾にも必ず明記。

## 仕様参照

### ビジネス要件

- **FR-DIAG-11** (営業向け診断書 DOCX 生成) ← 新規達成
- **FR-DIAG-12** (業務管理用 + お客様用の 2 セクション) ← 新規達成

## 実装内容

### Part A: 営業向け診断書 DOCX 生成 (`crates/report/src/business_diagnostic_docx.rs` 新規)

```rust
//! 営業向け診断書 (DOCX) の生成.
//!
//! 業務管理用セクション + お客様用セクションの 2 セクション構造で
//! 診断結果を業務的に文書化する。
//!
//! ## 出力先
//!
//! `C:\cases\{案件番号}\業務診断書.docx`
//!
//! ## セクション構造
//!
//! 1. ヘッダ (案件番号、診断日、診断者)
//! 2. 業務管理用セクション
//!    - 技術詳細 (NTFS 構造、Dirty Bit 等)
//!    - 業務サマリ (難易度、推定ファイル数、成功率)
//!    - 業務的説明 (各項目の詳細)
//! 3. お客様用セクション
//!    - 案件概要 (平易な表現)
//!    - HDD の状態説明 (お客様向け文言)
//!    - 復旧の見通し
//!    - 注意事項 + 免責注釈

use std::path::Path;
use anyhow::{Context, Result};

use dds_case_manager::case::Case;
use dds_diagnostic::{
    DirtyBitStatus, LogFileStatus, BitLockerStatus,
    RecoveryDifficulty,
    BusinessExplanation, CUSTOMER_DISCLAIMER,
    mft_corruption_explanation, boot_sector_explanation,
};

// 注: 既存の DOCX 生成パターンに従う
// 既存実装 (Phase 1 の business_report_docx.rs 等) を参考に同じライブラリを使用
// 想定: docx-rs クレートを使用
use docx_rs::{
    Docx, Paragraph, Run, RunFonts, ParagraphChild,
    AlignmentType, LineSpacing,
};

/// 営業向け診断書 DOCX の生成
pub fn generate_business_diagnostic_docx(
    case: &Case,
    output_path: &Path,
) -> Result<()> {
    let diag = case.diagnostic_input.as_ref()
        .context("診断結果が存在しません")?;
    
    let mut docx = Docx::new();
    
    // ============================================================
    // ヘッダ
    // ============================================================
    docx = add_header(docx, case)?;
    
    // ============================================================
    // 業務管理用セクション
    // ============================================================
    docx = add_section_divider(docx, "業務管理用 (内部確認・見積根拠)");
    docx = add_business_internal_section(docx, case, diag)?;
    
    // ============================================================
    // お客様用セクション
    // ============================================================
    docx = add_section_divider(docx, "お客様用 (口頭説明の参考)");
    docx = add_customer_section(docx, case, diag)?;
    
    // ファイルに書き出し
    let file = std::fs::File::create(output_path)
        .with_context(|| format!("DOCX ファイルを作成できません: {}", output_path.display()))?;
    docx.build()
        .pack(file)
        .with_context(|| "DOCX のシリアライズに失敗しました")?;
    
    Ok(())
}

// ============================================================
// ヘッダ
// ============================================================
fn add_header(docx: Docx, case: &Case) -> Result<Docx> {
    let title = Paragraph::new()
        .add_run(
            Run::new()
                .add_text("業務診断書")
                .size(36)
                .bold()
                .fonts(RunFonts::new().east_asia("Yu Gothic"))
        )
        .align(AlignmentType::Center);
    
    let case_info = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(&format!("案件番号: {}", case.case_id))
                .size(24)
                .fonts(RunFonts::new().east_asia("Yu Gothic"))
        )
        .align(AlignmentType::Center);
    
    let date_info = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(&format!("診断日時: {}", 
                    case.diagnosed_at.format("%Y年%m月%d日 %H:%M:%S")))
                .size(20)
                .fonts(RunFonts::new().east_asia("Yu Gothic"))
        )
        .align(AlignmentType::Center);
    
    Ok(docx
        .add_paragraph(title)
        .add_paragraph(case_info)
        .add_paragraph(date_info)
        .add_paragraph(Paragraph::new()))  // 空行
}

// ============================================================
// セクション区切り
// ============================================================
fn add_section_divider(docx: Docx, title: &str) -> Docx {
    let divider = Paragraph::new()
        .add_run(
            Run::new()
                .add_text(format!("━━━━━━━━━━ {} ━━━━━━━━━━", title))
                .size(24)
                .bold()
                .fonts(RunFonts::new().east_asia("Yu Gothic"))
        )
        .align(AlignmentType::Center);
    
    docx.add_paragraph(divider)
        .add_paragraph(Paragraph::new())  // 空行
}

// ============================================================
// 業務管理用セクション
// ============================================================
fn add_business_internal_section(
    docx: Docx, 
    case: &Case, 
    diag: &DiagnosticInput,
) -> Result<Docx> {
    let mut docx = docx;
    
    // === 1. ファイルシステムの基本情報 ===
    docx = add_h2(docx, "1. ファイルシステムの基本情報");
    
    docx = add_table_row(docx, "ファイルシステム署名", 
        if diag.is_nfs_structure_ok() {"正常 (NTFS 認識成功)"} else {"異常"});
    docx = add_table_row(docx, "MFT エントリ破損", 
        &format!("{} 件", diag.mft_corruption_count()));
    docx = add_table_row(docx, "不正な run-list", 
        &format!("{} 件", diag.invalid_runlist_count()));
    docx = add_table_row(docx, "Boot sector", 
        if diag.is_boot_sector_ok() {"正常"} else {"異常"});
    
    docx = add_blank_paragraph(docx);
    
    // === 2. Windows のマウント状態 ===
    docx = add_h2(docx, "2. Windows のマウント状態");
    
    if let Some(status) = &diag.dirty_bit {
        docx = add_table_row(docx, "Dirty Bit", status.business_message());
    }
    if let Some(status) = &diag.log_file {
        docx = add_table_row(docx, "$LogFile 整合性", status.business_message());
    }
    if let Some(status) = &diag.bitlocker {
        docx = add_table_row(docx, "BitLocker 暗号化", status.business_message());
    }
    
    docx = add_blank_paragraph(docx);
    
    // === 3. 業務的な評価 ===
    docx = add_h2(docx, "3. 業務的な評価");
    
    if let Some(est) = &diag.file_estimation {
        docx = add_table_row(docx, "推定ファイル数", &est.business_summary());
    }
    if let Some(diff) = &diag.recovery_difficulty {
        docx = add_table_row(docx, "復旧難易度", 
            &format!("{} ({})", diff.display_name(), diff.business_explanation()));
    }
    if let Some(rate) = &diag.success_rate {
        docx = add_table_row(docx, "推定成功率", &rate.business_summary());
        
        if !rate.reasoning.is_empty() {
            docx = add_paragraph(docx, "計算根拠:", true);
            for reason in &rate.reasoning {
                docx = add_paragraph(docx, &format!("  • {}", reason), false);
            }
        }
    }
    
    docx = add_blank_paragraph(docx);
    
    // === 4. 業務的な詳細説明 ===
    docx = add_h2(docx, "4. 業務的な詳細説明 (営業の判断材料)");
    
    let mut explanation_count = 0;
    
    // Dirty Bit の説明
    if let Some(status) = &diag.dirty_bit {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● Dirty Bit について");
            docx = add_explanation_section(docx, exp, false);  // false = 業務管理用
            explanation_count += 1;
        }
    }
    
    // $LogFile の説明
    if let Some(status) = &diag.log_file {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● $LogFile 整合性について");
            docx = add_explanation_section(docx, exp, false);
            explanation_count += 1;
        }
    }
    
    // BitLocker の説明
    if let Some(status) = &diag.bitlocker {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● BitLocker 暗号化について");
            docx = add_explanation_section(docx, exp, false);
            explanation_count += 1;
        }
    }
    
    // MFT 破損の説明
    if let Some(exp) = mft_corruption_explanation(diag.mft_corruption_count()) {
        docx = add_h3(docx, "● MFT エントリ破損について");
        docx = add_explanation_section(docx, exp, false);
        explanation_count += 1;
    }
    
    // Boot sector の説明
    if let Some(exp) = boot_sector_explanation(!diag.is_boot_sector_ok()) {
        docx = add_h3(docx, "● Boot sector について");
        docx = add_explanation_section(docx, exp, false);
        explanation_count += 1;
    }
    
    // 復旧難易度の説明
    if let Some(diff) = &diag.recovery_difficulty {
        if let Some(exp) = diff.explanation() {
            docx = add_h3(docx, "● 復旧難易度について");
            docx = add_explanation_section(docx, exp, false);
            explanation_count += 1;
        }
    }
    
    if explanation_count == 0 {
        docx = add_paragraph(docx, "特に異常な項目はありません。標準的な業務ケースとして処理可能です。", false);
    }
    
    docx = add_blank_paragraph(docx);
    
    Ok(docx)
}

// ============================================================
// お客様用セクション
// ============================================================
fn add_customer_section(
    docx: Docx, 
    case: &Case, 
    diag: &DiagnosticInput,
) -> Result<Docx> {
    let mut docx = docx;
    
    docx = add_h2(docx, "1. 案件概要");
    
    docx = add_paragraph(docx, 
        "このたびは当社のデータ復旧サービスをご検討いただき、誠にありがとうございます。", false);
    docx = add_paragraph(docx, 
        &format!("案件番号: {} のお客様の HDD の状態について、診断結果をご説明いたします。", case.case_id), false);
    
    docx = add_blank_paragraph(docx);
    
    docx = add_h2(docx, "2. HDD の状態について");
    
    let mut has_explanation = false;
    
    // Dirty Bit
    if let Some(status) = &diag.dirty_bit {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● Windows がマウントを拒否している原因 (Dirty Bit)");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }
    
    // $LogFile
    if let Some(status) = &diag.log_file {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● ファイル管理ログの状態 ($LogFile)");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }
    
    // BitLocker
    if let Some(status) = &diag.bitlocker {
        if let Some(exp) = status.explanation() {
            docx = add_h3(docx, "● 暗号化について (BitLocker)");
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
            has_explanation = true;
        }
    }
    
    // MFT 破損
    if let Some(exp) = mft_corruption_explanation(diag.mft_corruption_count()) {
        docx = add_h3(docx, "● ファイル管理情報の状態");
        docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
        has_explanation = true;
    }
    
    // Boot sector
    if let Some(exp) = boot_sector_explanation(!diag.is_boot_sector_ok()) {
        docx = add_h3(docx, "● HDD の起動情報の状態");
        docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
        has_explanation = true;
    }
    
    if !has_explanation {
        docx = add_customer_friendly_paragraph(docx, 
            "お客様の HDD は技術的に健全な状態です。標準的な復旧プロセスでデータを取り出すことが可能です。");
    }
    
    docx = add_blank_paragraph(docx);
    
    // 復旧の見通し
    docx = add_h2(docx, "3. 復旧の見通し");
    
    if let Some(diff) = &diag.recovery_difficulty {
        if let Some(exp) = diff.explanation() {
            docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
        }
    }
    
    if let Some(rate) = &diag.success_rate {
        docx = add_blank_paragraph(docx);
        docx = add_paragraph(docx, "推定復旧成功率:", true);
        docx = add_paragraph(docx, 
            &format!("  全体: 約 {}%", rate.overall_rate), false);
        if let Some(priority) = rate.priority_rate {
            docx = add_paragraph(docx, 
                &format!("  優先データ: 約 {}%", priority), false);
        }
    }
    
    docx = add_blank_paragraph(docx);
    
    // 注意事項 + 免責
    docx = add_h2(docx, "4. 注意事項");
    
    for line in CUSTOMER_DISCLAIMER.lines() {
        docx = add_paragraph(docx, line, false);
    }
    
    docx = add_blank_paragraph(docx);
    docx = add_paragraph(docx, "ご不明な点がございましたら、お気軽にお問い合わせください。", false);
    
    Ok(docx)
}

// ============================================================
// ヘルパー関数
// ============================================================

fn add_h2(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text(text)
                    .size(28)
                    .bold()
                    .fonts(RunFonts::new().east_asia("Yu Gothic"))
            )
    )
}

fn add_h3(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text(text)
                    .size(24)
                    .bold()
                    .fonts(RunFonts::new().east_asia("Yu Gothic"))
            )
    )
}

fn add_paragraph(docx: Docx, text: &str, bold: bool) -> Docx {
    let run = if bold {
        Run::new()
            .add_text(text)
            .size(20)
            .bold()
            .fonts(RunFonts::new().east_asia("Yu Gothic"))
    } else {
        Run::new()
            .add_text(text)
            .size(20)
            .fonts(RunFonts::new().east_asia("Yu Gothic"))
    };
    docx.add_paragraph(Paragraph::new().add_run(run))
}

fn add_blank_paragraph(docx: Docx) -> Docx {
    docx.add_paragraph(Paragraph::new())
}

fn add_table_row(docx: Docx, label: &str, value: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text(&format!("  {}: ", label))
                    .size(20)
                    .bold()
                    .fonts(RunFonts::new().east_asia("Yu Gothic"))
            )
            .add_run(
                Run::new()
                    .add_text(value)
                    .size(20)
                    .fonts(RunFonts::new().east_asia("Yu Gothic"))
            )
    )
}

fn add_customer_friendly_paragraph(docx: Docx, text: &str) -> Docx {
    docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text(text)
                    .size(22)
                    .fonts(RunFonts::new().east_asia("Yu Gothic"))
            )
            .line_spacing(LineSpacing::new().line(360))  // 1.5 倍行間
    )
}

fn add_explanation_section(
    docx: Docx, 
    exp: &BusinessExplanation,
    customer_facing: bool,
) -> Docx {
    let mut docx = docx;
    
    if !customer_facing {
        // 業務管理用: 5 セクション全て表示
        docx = add_paragraph(docx, "  【何が起きているか】", true);
        docx = add_paragraph(docx, &format!("    {}", exp.what_happened), false);
        docx = add_blank_paragraph(docx);
        
        docx = add_paragraph(docx, "  【考えられる原因】", true);
        for cause in exp.causes {
            docx = add_paragraph(docx, &format!("    ・{}", cause), false);
        }
        docx = add_blank_paragraph(docx);
        
        docx = add_paragraph(docx, "  【Windows の挙動】", true);
        docx = add_paragraph(docx, &format!("    {}", exp.windows_behavior), false);
        docx = add_blank_paragraph(docx);
        
        docx = add_paragraph(docx, "  【業務的な意味】", true);
        docx = add_paragraph(docx, &format!("    {}", exp.business_meaning), false);
        docx = add_blank_paragraph(docx);
        
        docx = add_paragraph(docx, "  【お客様への説明例】", true);
        docx = add_paragraph(docx, &format!("    「{}」", exp.customer_explanation), false);
        docx = add_blank_paragraph(docx);
    } else {
        // お客様向け: customer_explanation のみ
        docx = add_customer_friendly_paragraph(docx, exp.customer_explanation);
    }
    
    docx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    
    fn make_test_case() -> Case {
        // テスト用の Case を作成 (詳細は省略、既存パターン参照)
        // ...
        unimplemented!("既存の test fixture を使用")
    }
    
    #[test]
    fn generate_docx_creates_file() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("業務診断書.docx");
        
        let case = make_test_case();
        generate_business_diagnostic_docx(&case, &output_path).unwrap();
        
        assert!(output_path.exists());
        let metadata = std::fs::metadata(&output_path).unwrap();
        assert!(metadata.len() > 1000);  // DOCX は最低でも 1KB 以上
    }
    
    // 他のテスト (構造、内容の確認等) は結合テストで
}
```

### Part B: `crates/report/src/lib.rs` への追加

```rust
// 既存:
pub mod customer_recovery_docx;
pub mod management_html;
pub mod report_csv;
pub mod crm_text;

// 新規追加:
pub mod business_diagnostic_docx;

// 公開 API:
pub use business_diagnostic_docx::generate_business_diagnostic_docx;
```

### Part C: workbench-dryrun の diagnose に統合

`crates/workbench-dryrun/src/commands/diagnose.rs`:

```rust
// 既存の診断処理の後、案件保存後に追加:

// case を保存
storage.save(&case)?;

// ★ 営業向け診断書 DOCX を生成
let docx_path = storage.case_dir(&case.case_id).join("業務診断書.docx");
match generate_business_diagnostic_docx(&case, &docx_path) {
    Ok(_) => {
        println!();
        println!("📄 営業向け診断書を生成しました:");
        println!("   {}", docx_path.display());
    }
    Err(e) => {
        eprintln!("⚠ 営業向け診断書の生成に失敗しました: {}", e);
        eprintln!("  (診断結果は他のレポートで確認できます)");
    }
}

// 既存の表示
println!();
println!("[保存先]");
println!("  {}\\", storage.case_dir(&case.case_id).display());
println!("    ├ 案件情報.json");
println!("    ├ 診断結果_CRM貼り付け用.txt");
println!("    ├ 業務管理レポート.html");
println!("    ├ 復旧詳細.csv");
println!("    └ 業務診断書.docx ← 新規");
```

### Part D: case-manager の保存パス管理

`crates/case-manager/src/storage.rs`:

```rust
impl CaseStorage {
    /// 業務診断書 DOCX のパス
    pub fn business_diagnostic_docx_path(&self, case_id: &CaseId) -> PathBuf {
        self.case_dir(case_id).join("業務診断書.docx")
    }
}
```

## 単体テスト要件 (最低 4 件)

### `business_diagnostic_docx.rs` (最低 4 件)

1. `generate_docx_creates_file` - DOCX ファイルが作成される
2. `generate_docx_includes_case_id` - 案件番号が含まれる
3. `generate_docx_with_anomalies` - 異常がある場合、業務説明が含まれる
4. `generate_docx_healthy_case` - 健全な場合、簡潔な内容

## 結合テスト要件 (最低 2 件)

```rust
#[test]
fn diagnose_command_generates_business_docx() {
    // workbench-dryrun diagnose 実行後、業務診断書.docx が作成される
}

#[test]
fn business_docx_contains_both_sections() {
    // 生成された DOCX に「業務管理用」と「お客様用」両方のセクションが含まれる
}
```

## 制約

- **行数目安**:
  - `report/src/business_diagnostic_docx.rs` (新規): 約 500 行 + テスト 100 行
  - `report/src/lib.rs` 修正: +3 行
  - `workbench-dryrun/src/commands/diagnose.rs` 修正: +25 行 (DOCX 生成統合)
  - `case-manager/src/storage.rs` 修正: +10 行
  - 合計: 約 638 行追加・修正
- **単体テスト新規**: 最低 4 件
- **結合テスト新規**: 最低 2 件
- **`unsafe` 追加行数**: 0
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)
- [ ] `diagnose` 実行時に `業務診断書.docx` が自動生成される
- [ ] DOCX に「業務管理用」セクションが含まれる
- [ ] DOCX に「お客様用」セクションが含まれる
- [ ] DOCX が Word で開いて正常表示される
- [ ] 異常がない場合は簡潔な内容
- [ ] 異常がある場合は業務説明文が含まれる
- [ ] 免責注釈が含まれる
- [ ] DOCX の保存先が CLI に表示される

## 関連 FR 要件

- **FR-DIAG-11** (営業向け診断書 DOCX 生成) ← 達成
- **FR-DIAG-12** (業務管理用 + お客様用の 2 セクション) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **次のチャンク: Chunk 24d-4-3 (実機ドライランとフィードバック反映)**

---

## 注意事項

### 既存の DOCX 生成パターンの確認

```
[必ず確認すること]
crates/report/src/customer_recovery_docx.rs (or 同等のファイル)
   ↓
- 使用している DOCX ライブラリ (docx-rs 等)
- フォント (Yu Gothic 等)
- スタイル (見出しサイズ、行間)
- ヘルパー関数

[本チャンクでの方針]
既存のパターンに合わせて実装
業務的な一貫性を保つ
```

もし既存実装が `docx-rs` ではなく他のライブラリを使っている場合、Cargo.toml を要確認。

### 日本語フォントの注意

```
[業務的な確認ポイント]
- Yu Gothic (推奨、Windows 標準)
- メイリオ (代替案)
- MS ゴシック (古いが確実)

[既存実装での選択]
Phase 1 (Chunk 14-16) のお客様向け復旧レポート.docx で使用しているフォントに合わせる
```

### お客様用セクションの表現原則

```
[Chunk 24d-4-1.5 で確立した原則]
- 専門用語ゼロ (MFT, $Volume 等を含めない)
- 平易な日本語
- 「お客様の HDD は...」で始める
- 免責注釈で締める

[本チャンクでの追加]
- 読みやすい行間 (1.5 倍)
- フォントサイズはやや大きめ (本文 22pt)
- 段落間の余白
```

### 「業務診断書.docx」のファイル名

```
[業務的な選択]
A: 業務診断書.docx ← 推奨、業務的に明確
B: 営業向け診断書.docx ← 用途が明確だが「営業向け」が業務的に冗長
C: 診断書.docx ← シンプルだが他の診断書類との区別困難

推奨: A 「業務診断書.docx」
理由: 業務用と明確、他のファイル名と区別容易
```

### お客様への DOCX 直接納品の禁止

```
[業務的な原則]
業務診断書.docx は社内保存のみ
お客様への直接納品は禁止

[理由]
- 業務管理用セクション (内部情報) が含まれる
- お客様用セクションも業務的な内部資料
- 営業がお客様への口頭説明の台本として使用

[お客様への納品物]
納品 HDD の 復旧レポート.docx (Chunk 24a で確立)
これは引き続き別ファイル
```

### Phase 2.1 UI への引き継ぎ

```
[Tauri UI で表示する診断結果画面]
- 業務管理用ビュー (営業のメイン画面)
- お客様用ビュー (お客様への画面共有用)
- 「DOCX として出力」ボタン

[Chunk 24d-4-2 で公開する API]
generate_business_diagnostic_docx(case, output_path)
→ UI から呼び出して DOCX 生成可能
```

### 既存の Cargo.toml 依存確認

```toml
# crates/report/Cargo.toml に既存で含まれているか確認:
[dependencies]
docx-rs = "0.4"  # または同等のバージョン
chrono = { version = "0.4", features = ["serde"] }
# ...
```

もし `docx-rs` が含まれていなければ追加が必要。

---

## 質問が必要なケース

- 既存の DOCX 生成ライブラリが想定と違う場合
- 既存のフォント・スタイルが想定と違う場合
- DiagnosticInput の構造が想定外の場合
- case.diagnosed_at フィールドが存在しない場合 (代替フィールドを使う)

---

## 完了報告例

```markdown
## Chunk 24d-4-2 完了報告

### 新規ファイル
- crates/report/src/business_diagnostic_docx.rs (約 500 行 + テスト 100 行)

### 修正ファイル
- crates/report/src/lib.rs (+3 行 export)
- crates/workbench-dryrun/src/commands/diagnose.rs (+25 行 DOCX 生成統合)
- crates/case-manager/src/storage.rs (+10 行 パス管理)

### 新規 API
- generate_business_diagnostic_docx(case, output_path) -> Result<()>
- CaseStorage::business_diagnostic_docx_path(case_id) -> PathBuf

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)

### テスト統計
- 単体: 既存 + 新規 4 件
- 結合: 既存 + 新規 2 件
- 全 workspace: 全パス

### 動作確認サンプル
[diagnose 実行時の CLI 出力]
```
> workbench-dryrun diagnose --physical 1 --partition 1

📡 物理ドライブモードで診断します
...
[診断完了]

📄 営業向け診断書を生成しました:
   C:\cases\260603-01\業務診断書.docx

[保存先]
  C:\cases\260603-01\
    ├ 案件情報.json
    ├ 診断結果_CRM貼り付け用.txt
    ├ 業務管理レポート.html
    ├ 復旧詳細.csv
    └ 業務診断書.docx ← 新規
```

[業務診断書.docx の構造]
- ヘッダ (案件番号、診断日時)
- ━━━━ 業務管理用 ━━━━
  1. ファイルシステムの基本情報
  2. Windows のマウント状態
  3. 業務的な評価
  4. 業務的な詳細説明 (営業の判断材料)
- ━━━━ お客様用 ━━━━
  1. 案件概要
  2. HDD の状態について
  3. 復旧の見通し
  4. 注意事項

### 🎯 達成事項
- 営業向け診断書 DOCX を自動生成
- 業務管理用 + お客様用の 2 セクション構造
- 業務適用品質の「文書化」を実現
- 営業のお客様説明の業務的な台本として使用可能

### Phase 1.5 拡張の業務的な完成度
✓ 物理ドライブ対応
✓ 業務的診断項目
✓ 業務的説明文
✓ 営業向け診断書 DOCX  ← 今ここ

→ Workbench が「業務基盤」として完成
→ R-STUDIO の代替候補として実用レベル

### 次のステップ
Chunk 24d-4-3 で:
- Chouさんが実機ドライランを実施
  - 通常診断 + 物理診断
  - 業務診断書 DOCX の業務的な品質確認
- フィードバックを反映 (文言調整、レイアウト修正等)
- Phase 1.5 拡張の最終完成

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんが実機ドライランを実施
→ 業務的なフィードバックを共有
→ Chunk 24d-4-3 (フィードバック反映) の指示書を私に依頼
```
