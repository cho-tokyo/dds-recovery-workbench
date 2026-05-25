# Chunk 23 指示: 業務向け出力ディレクトリ構造（Phase 1.5 最終チャンク）

Phase 1.5 の集大成。これまでの機能を**業務的に整理された納品物**に統合します。納品 HDD のフォルダ構造、ファイル名の日本語化、案件番号ベースの全体オーケストレーション。

> 🎯 完了時点で「`G:\260522-04\` をお客様にそのまま納品できる」状態になる。Phase 1.5 が完成し、Phase 2.1 (Tauri UI) への移行準備が整う。

---

## 目的

業務的な納品物を自動生成する 5 つの統合機能:

1. **`CaseOutput` 構造体**: 案件番号付きの納品ディレクトリ構造を表現
2. **`RecoveryEngine` の拡張**: 通常ファイルと削除ファイルを別々のパスに出力
3. **`write_business_reports`**: レポート 4 ファイルを日本語名で生成
4. **`execute_business_recovery`**: 案件単位の全体オーケストレーション
5. **既存テストのマイグレーション**: 新構造に統合

## 業務的な納品ディレクトリ構造

### 完了後の納品 HDD

```
G:\  (お客様への納品 HDD、ドライブレター)
└── 260522-04\                          ← 案件番号がルート
    ├── 復旧データ\
    │   ├── 通常ファイル\
    │   │   └── (NTFS 階層を再現: Users\Chou\Documents\...)
    │   └── 削除ファイル\
    │       └── (NTFS 階層を再現)
    └── レポート\
        ├── 復旧レポート.docx            ← お客様向け (Word で開く)
        ├── 要確認ファイル一覧.txt       ← お客様向け (Notepad で開く)
        ├── 業務管理レポート.html        ← 社内用 (任意で同梱)
        └── report.csv                   ← 外部システム連携用
```

### 社内に残る案件情報 (お客様には見せない)

```
C:\cases\
└── 260522-04\
    └── case.json                      ← Chunk 21 で実装済み、社内保存
```

### 業務的な意義

```
[復旧完了]
  ↓
[CS のフロー]
1. 納品 HDD (G:\) を取り出す
2. お客様に G:\ をそのまま送付
   → お客様は「G:\260522-04」をエクスプローラで開くだけで:
     - 復旧データ\通常ファイル\ → 自分のファイルを確認
     - 復旧データ\削除ファイル\ → 削除されたファイルを確認
     - レポート\復旧レポート.docx → 復旧サマリ
     - レポート\要確認ファイル一覧.txt → 品質要確認ファイル
3. 社内には C:\cases\260522-04\case.json が残る (再復旧依頼に備えて)
```

## 対象クレート

- **主**:
  - `crates/case-manager/` (CaseOutput 構造体追加)
  - `crates/recovery/` (RecoveryEngine の出力パス分離)
  - `crates/report/` (write_business_reports 追加、日本語名)
- **副**: 
  - 既存テストの調整

## 重要な設計原則

### 既存 API との共存

```rust
✗ 破壊的変更: 既存テストが全部壊れる
○ 拡張: 新 API を追加、既存 API は維持
```

- `RecoveryEngine::recover_files` (既存): そのまま維持
- `dds_report::write_all_reports` (既存): そのまま維持
- 新規追加: 業務向け API

### 案件番号ベースのパス構築

すべてのパスは `CaseOutput` 経由で統一的に構築。直接ハードコードしない:

```rust
✗ let dir = format!("G:\\{}\\復旧データ\\通常ファイル", case_id);
○ let dir = case_output.live_files_dir();
```

### Case との統合

復旧完了後、`Case.output_dir` と `Case.recovery_report_summary` が自動更新され、`case.json` に永続化される流れ。

## 仕様参照

### ビジネス要件

- **FR-OUT-01**: 案件番号付きルートディレクトリ
- **FR-OUT-02**: 「通常ファイル」「削除ファイル」のフォルダ分離
- **FR-OUT-03**: 日本語フォルダ名・ファイル名 (Windows 互換)
- **FR-OUT-04**: 社内保存と納品物の分離

## 実装内容

### Part A: `CaseOutput` 構造体 (case-manager)

`crates/case-manager/src/output.rs` (新規ファイル):

```rust
use std::io;
use std::path::{Path, PathBuf};

use crate::case_id::CaseId;

/// 案件の納品ディレクトリ構造を表現する。
///
/// 構造:
/// ```text
/// {drive_root}/{案件番号}/
///   ├ 復旧データ/
///   │   ├ 通常ファイル/
///   │   └ 削除ファイル/
///   └ レポート/
///       ├ 復旧レポート.docx
///       ├ 要確認ファイル一覧.txt
///       ├ 業務管理レポート.html
///       └ report.csv
/// ```
///
/// 例: `CaseOutput::new(case_id, "G:\\")` で `G:\260522-04\...` のパスを構築。
#[derive(Debug, Clone)]
pub struct CaseOutput {
    case_id: CaseId,
    drive_root: PathBuf,
}

impl CaseOutput {
    /// 新規の CaseOutput を作成する。
    ///
    /// `drive_root` は納品 HDD のドライブレター (例: `G:\\`) または任意のディレクトリ。
    pub fn new(case_id: CaseId, drive_root: impl Into<PathBuf>) -> Self {
        Self {
            case_id,
            drive_root: drive_root.into(),
        }
    }
    
    /// 案件のルートディレクトリ。
    /// 例: `G:\260522-04`
    pub fn root(&self) -> PathBuf {
        self.drive_root.join(self.case_id.as_str())
    }
    
    /// 通常ファイルの出力先。
    /// 例: `G:\260522-04\復旧データ\通常ファイル`
    pub fn live_files_dir(&self) -> PathBuf {
        self.root().join("復旧データ").join("通常ファイル")
    }
    
    /// 削除ファイルの出力先。
    /// 例: `G:\260522-04\復旧データ\削除ファイル`
    pub fn deleted_files_dir(&self) -> PathBuf {
        self.root().join("復旧データ").join("削除ファイル")
    }
    
    /// レポートディレクトリ。
    /// 例: `G:\260522-04\レポート`
    pub fn reports_dir(&self) -> PathBuf {
        self.root().join("レポート")
    }
    
    /// 顧客向け Word レポートのパス。
    pub fn customer_docx_path(&self) -> PathBuf {
        self.reports_dir().join("復旧レポート.docx")
    }
    
    /// 顧客向け要確認ファイル一覧 (テキスト) のパス。
    pub fn customer_txt_path(&self) -> PathBuf {
        self.reports_dir().join("要確認ファイル一覧.txt")
    }
    
    /// 社内向け業務管理レポート (HTML) のパス。
    pub fn internal_html_path(&self) -> PathBuf {
        self.reports_dir().join("業務管理レポート.html")
    }
    
    /// 外部システム連携用 CSV のパス。
    pub fn csv_path(&self) -> PathBuf {
        self.reports_dir().join("report.csv")
    }
    
    /// 案件番号を返す。
    pub fn case_id(&self) -> &CaseId {
        &self.case_id
    }
    
    /// 必要なすべてのサブディレクトリを作成する。
    /// 既存ならスキップ (create_dir_all 相当)。
    pub fn create_all_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.live_files_dir())?;
        std::fs::create_dir_all(self.deleted_files_dir())?;
        std::fs::create_dir_all(self.reports_dir())?;
        Ok(())
    }
}
```

`crates/case-manager/src/lib.rs` に追加:

```rust
pub mod output;
pub use output::CaseOutput;
```

### Part B: `RecoveryEngine` の拡張 (recovery)

既存の `RecoveryEngine` を拡張して、通常ファイルと削除ファイルを別々のディレクトリに出力できるようにします。

#### 現状の API (Chunk 17 ベース)

```rust
// 既存 (推定):
pub struct RecoveryEngine {
    output_dir: PathBuf,  // この下に live/ と deleted/ を作成
    // ...
}

impl RecoveryEngine {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self;
    pub fn recover_files<F>(&self, volume: &mut NtfsVolume<F>, wishlist: &Wishlist) 
        -> Result<RecoveryReport, RecoveryError>;
}
```

#### 拡張後の API (Chunk 23)

```rust
pub struct RecoveryEngine {
    config: RecoveryConfig,
    // ...
}

#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    pub live_files_dir: PathBuf,
    pub deleted_files_dir: PathBuf,
}

impl RecoveryConfig {
    /// 単一の output_dir から、従来の構造 (live/, deleted/) を構築。
    /// 既存 API 互換用。
    pub fn from_single_dir(output_dir: impl AsRef<Path>) -> Self {
        let base = output_dir.as_ref();
        Self {
            live_files_dir: base.join("live"),
            deleted_files_dir: base.join("deleted"),
        }
    }
    
    /// CaseOutput から業務向けの構造を構築。
    pub fn from_case_output(case_output: &CaseOutput) -> Self {
        Self {
            live_files_dir: case_output.live_files_dir(),
            deleted_files_dir: case_output.deleted_files_dir(),
        }
    }
    
    /// 明示的に指定する。
    pub fn with_paths(live: impl Into<PathBuf>, deleted: impl Into<PathBuf>) -> Self {
        Self {
            live_files_dir: live.into(),
            deleted_files_dir: deleted.into(),
        }
    }
}

impl RecoveryEngine {
    /// 既存 API (互換維持)
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self::with_config(RecoveryConfig::from_single_dir(output_dir.into()))
    }
    
    /// 新規 API: 明示的な config
    pub fn with_config(config: RecoveryConfig) -> Self {
        Self { config }
    }
    
    pub fn recover_files<F>(&self, volume: &mut NtfsVolume<F>, wishlist: &Wishlist)
        -> Result<RecoveryReport, RecoveryError>
    {
        // 既存ロジックを config.live_files_dir / config.deleted_files_dir 使用に変更
        // 内部で `output_dir/live/` を作っていた箇所を `config.live_files_dir` に
    }
}
```

**実装ポイント**:
- 既存の `RecoveryEngine::new(output_dir)` は互換維持 (`output_dir/live/` と `output_dir/deleted/` を構築)
- 新規 `RecoveryEngine::with_config(RecoveryConfig)` を追加
- 内部実装は `config.live_files_dir` と `config.deleted_files_dir` を直接使う
- 既存のテストは影響なし (互換性維持)

### Part C: `write_business_reports` (report)

`crates/report/src/business.rs` (新規ファイル):

```rust
use std::path::PathBuf;

use dds_case_manager::CaseOutput;
use dds_recovery::RecoveryReport;

use crate::csv::render_csv;
use crate::docx_customer::render_customer_docx;
use crate::error::ReportError;
use crate::html_internal::render_internal_html;
use crate::txt_customer::render_invalid_files_txt;

/// 業務的な納品物としてレポート 4 ファイルを生成する。
///
/// 出力ファイル:
/// - `{案件番号}/レポート/復旧レポート.docx` (お客様向け Word)
/// - `{案件番号}/レポート/要確認ファイル一覧.txt` (お客様向け Notepad)
/// - `{案件番号}/レポート/業務管理レポート.html` (社内用 HTML)
/// - `{案件番号}/レポート/report.csv` (外部システム連携用)
///
/// レポートディレクトリは事前に CaseOutput::create_all_dirs() で作成しておくこと。
pub fn write_business_reports(
    report: &RecoveryReport,
    case_output: &CaseOutput,
) -> Result<BusinessReportPaths, ReportError> {
    // 念のためレポートディレクトリを確保
    std::fs::create_dir_all(case_output.reports_dir())?;
    
    let customer_docx = case_output.customer_docx_path();
    let customer_txt = case_output.customer_txt_path();
    let internal_html = case_output.internal_html_path();
    let csv = case_output.csv_path();
    
    std::fs::write(&customer_docx, render_customer_docx(report)?)?;
    std::fs::write(&customer_txt, render_invalid_files_txt(report))?;
    std::fs::write(&internal_html, render_internal_html(report)?)?;
    std::fs::write(&csv, render_csv(report)?)?;
    
    Ok(BusinessReportPaths {
        customer_docx,
        customer_txt,
        internal_html,
        csv,
    })
}

/// 生成された業務向けレポートのファイルパス。
#[derive(Debug, Clone)]
pub struct BusinessReportPaths {
    pub customer_docx: PathBuf,
    pub customer_txt: PathBuf,
    pub internal_html: PathBuf,
    pub csv: PathBuf,
}
```

`crates/report/src/lib.rs` に追加:

```rust
pub mod business;
pub use business::{write_business_reports, BusinessReportPaths};
```

### Part D: 全体オーケストレーション

`crates/case-manager/src/orchestration.rs` (新規ファイル):

```rust
use std::path::Path;

use chrono::Utc;
use dds_fs_ntfs::NtfsVolume;
use dds_recovery::{RecoveryConfig, RecoveryEngine, RecoveryError, RecoveryReport};
use dds_wish_match::Wishlist;

use crate::case::{Case, RecoveryReportSummary};
use crate::case_id::CaseId;
use crate::output::CaseOutput;

/// 案件単位での業務的な復旧フローを実行する。
///
/// フロー:
/// 1. CaseOutput でディレクトリ構造を作成
/// 2. RecoveryEngine で復旧実行 (通常 + 削除を別々のフォルダへ)
/// 3. レポート 4 ファイルを {案件番号}/レポート/ に生成
/// 4. Case.output_dir と Case.recovery_report_summary を更新
///
/// 戻り値の `report_paths` は呼び出し元で利用可能。
/// case.json への永続化は呼び出し元の責任 (CaseStorage::save())。
pub fn execute_business_recovery<F>(
    case: &mut Case,
    drive_root: impl AsRef<Path>,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
) -> Result<BusinessRecoveryResult, BusinessRecoveryError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let case_output = CaseOutput::new(case.case_id.clone(), drive_root.as_ref().to_path_buf());
    case_output.create_all_dirs()?;
    
    // 復旧実行
    let config = RecoveryConfig::from_case_output(&case_output);
    let engine = RecoveryEngine::with_config(config);
    let report = engine.recover_files(volume, wishlist)?;
    
    // レポート生成
    let report_paths = dds_report::write_business_reports(&report, &case_output)?;
    
    // Case を更新
    case.output_dir = Some(case_output.root());
    case.recovery_report_summary = Some(summarize_report(&report));
    case.wishlist = Some(wishlist.clone());
    
    Ok(BusinessRecoveryResult {
        case_output,
        report,
        report_paths,
    })
}

/// RecoveryReport から RecoveryReportSummary (slim 版) を構築する。
fn summarize_report(report: &dds_recovery::RecoveryReport) -> RecoveryReportSummary {
    RecoveryReportSummary {
        started_at: report.started_at,
        finished_at: report.finished_at,
        duration_ms: report.duration_ms(),
        total_matched: report.total_matched,
        recovered_count: report.recovered.len(),
        failed_count: report.failed.len(),
        skipped_count: report.skipped.len(),
        validated_count: report.validated_count(),
        invalid_count: report.invalid_count(),
        uncertain_count: report.uncertain_count(),
        total_bytes_written: report.total_bytes_written(),
        recovery_success_rate: report.recovery_success_rate(),
        quality_assurance_rate: report.quality_assurance_rate(),
    }
}

/// 業務的な復旧フローの結果。
#[derive(Debug)]
pub struct BusinessRecoveryResult {
    pub case_output: CaseOutput,
    pub report: RecoveryReport,
    pub report_paths: dds_report::BusinessReportPaths,
}

/// 業務復旧オーケストレーションのエラー。
#[derive(Debug, thiserror::Error)]
pub enum BusinessRecoveryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Recovery error: {0}")]
    Recovery(#[from] RecoveryError),
    
    #[error("Report error: {0}")]
    Report(#[from] dds_report::ReportError),
}
```

`crates/case-manager/Cargo.toml` に依存追加:

```toml
[dependencies]
# 既存に追加:
dds-recovery.workspace = true
dds-report.workspace = true
```

`crates/case-manager/src/lib.rs` に追加:

```rust
pub mod orchestration;
pub use orchestration::{execute_business_recovery, BusinessRecoveryResult, BusinessRecoveryError};
```

> **注**: case-manager に recovery / report への依存が発生します。これは「業務オーケストレーション層」としての case-manager の責務拡大であり、Phase 1.5 の必要悪です。Phase 2 で「orchestrator」を別クレートに分離することも検討可能。

## 単体テスト要件 (最低 10 件)

### `CaseOutput` (最低 5 件)

```rust
#[test]
fn case_output_root_includes_case_id() {
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, "G:\\");
    assert_eq!(output.root(), PathBuf::from("G:\\260522-04"));
}

#[test]
fn case_output_live_files_dir_correct() {
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, "G:\\");
    assert_eq!(
        output.live_files_dir(),
        PathBuf::from("G:\\260522-04\\復旧データ\\通常ファイル")
    );
}

#[test]
fn case_output_deleted_files_dir_correct() {
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, "G:\\");
    assert_eq!(
        output.deleted_files_dir(),
        PathBuf::from("G:\\260522-04\\復旧データ\\削除ファイル")
    );
}

#[test]
fn case_output_japanese_report_filenames() {
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, "G:\\");
    
    assert!(output.customer_docx_path().to_string_lossy().ends_with("復旧レポート.docx"));
    assert!(output.customer_txt_path().to_string_lossy().ends_with("要確認ファイル一覧.txt"));
    assert!(output.internal_html_path().to_string_lossy().ends_with("業務管理レポート.html"));
    assert!(output.csv_path().to_string_lossy().ends_with("report.csv"));
}

#[test]
fn case_output_create_all_dirs_creates_structure() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, temp.path());
    
    output.create_all_dirs().unwrap();
    
    assert!(output.live_files_dir().exists());
    assert!(output.deleted_files_dir().exists());
    assert!(output.reports_dir().exists());
}
```

### `RecoveryConfig` (最低 2 件)

```rust
#[test]
fn recovery_config_from_case_output_uses_business_paths() {
    let case_id = CaseId::parse("260522-04").unwrap();
    let output = CaseOutput::new(case_id, "G:\\");
    let config = RecoveryConfig::from_case_output(&output);
    
    assert!(config.live_files_dir.to_string_lossy().contains("通常ファイル"));
    assert!(config.deleted_files_dir.to_string_lossy().contains("削除ファイル"));
}

#[test]
fn recovery_config_from_single_dir_keeps_legacy_structure() {
    let config = RecoveryConfig::from_single_dir("G:\\output");
    
    assert_eq!(config.live_files_dir, PathBuf::from("G:\\output\\live"));
    assert_eq!(config.deleted_files_dir, PathBuf::from("G:\\output\\deleted"));
}
```

### `write_business_reports` (最低 1 件)

```rust
#[test]
fn write_business_reports_creates_japanese_filename_files() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_id = CaseId::parse("260522-04").unwrap();
    let case_output = CaseOutput::new(case_id, temp.path());
    case_output.create_all_dirs().unwrap();
    
    let report = make_dummy_report();
    let paths = write_business_reports(&report, &case_output).unwrap();
    
    assert!(paths.customer_docx.exists());
    assert!(paths.customer_txt.exists());
    assert!(paths.internal_html.exists());
    assert!(paths.csv.exists());
    
    assert!(paths.customer_docx.to_string_lossy().contains("復旧レポート.docx"));
}
```

### `execute_business_recovery` (最低 2 件)

```rust
#[test]
fn execute_business_recovery_updates_case_output_dir() {
    // ... setup ...
    let result = execute_business_recovery(&mut case, temp.path(), &mut volume, &wishlist).unwrap();
    
    assert!(case.output_dir.is_some());
    assert_eq!(case.output_dir.as_ref().unwrap(), &result.case_output.root());
}

#[test]
fn execute_business_recovery_populates_summary() {
    // ... setup ...
    let _result = execute_business_recovery(&mut case, temp.path(), &mut volume, &wishlist).unwrap();
    
    assert!(case.recovery_report_summary.is_some());
    let summary = case.recovery_report_summary.as_ref().unwrap();
    assert!(summary.total_matched > 0);
}
```

## 結合テスト要件 (最低 2 件)

### 1. 業務フロー end-to-end

`crates/case-manager/tests/business_flow_integration.rs`:

```rust
use tempfile::TempDir;
use dds_case_manager::*;
use dds_wish_match::*;
use dds_fs_ntfs::NtfsVolume;
// ... 共通ヘルパー ...

#[test]
fn full_business_flow_from_case_creation_to_delivery() {
    // 検証 PC の C:\cases\ を tempfile で代用
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    
    // 納品 HDD (G:\) を tempfile で代用
    let delivery_drive = TempDir::new().unwrap();
    
    // 1. 案件作成
    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();
    
    // 2. NTFS ボリュームをセットアップ (fixture から)
    let img = decompress_fixture("ntfs_mixed_formats");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    // 3. Wishlist 作成
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("png".into()), "PNG 全部")
            .with_priority(Priority::High));
    
    // 4. 業務復旧実行
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.path(),
        &mut volume,
        &wishlist,
    ).unwrap();
    
    // 5. case.json 永続化
    storage.save(&case).unwrap();
    
    // 6. 検証: 出力ディレクトリ構造
    let case_root = delivery_drive.path().join("260522-04");
    assert!(case_root.join("復旧データ").join("通常ファイル").exists());
    assert!(case_root.join("復旧データ").join("削除ファイル").exists());
    assert!(case_root.join("レポート").join("復旧レポート.docx").exists());
    assert!(case_root.join("レポート").join("要確認ファイル一覧.txt").exists());
    assert!(case_root.join("レポート").join("業務管理レポート.html").exists());
    assert!(case_root.join("レポート").join("report.csv").exists());
    
    // 7. 検証: case.json (社内保存)
    let loaded = storage.load(&case_id).unwrap();
    assert!(loaded.output_dir.is_some());
    assert!(loaded.recovery_report_summary.is_some());
    assert!(loaded.wishlist.is_some());
}
```

### 2. プロダクトデモテスト (Phase 1.5 完成版)

```rust
#[test]
fn product_demo_phase_1_5_complete() {
    let internal_storage = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_storage.path());
    let delivery_drive = TempDir::new().unwrap();
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();
    
    // ... ボリュームと wishlist のセットアップ ...
    
    let result = execute_business_recovery(
        &mut case, delivery_drive.path(), &mut volume, &wishlist
    ).unwrap();
    storage.save(&case).unwrap();
    
    println!("\n=== Phase 1.5 Complete Demo (Chunk 23) ===\n");
    println!("案件番号: {}", case.case_id);
    println!();
    println!("📂 納品 HDD: {:?}", delivery_drive.path());
    println!("  └─ 260522-04/");
    println!("      ├─ 復旧データ/");
    println!("      │   ├─ 通常ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.live_files_dir()));
    println!("      │   └─ 削除ファイル/ ({} 件)",
        count_files_recursive(&result.case_output.deleted_files_dir()));
    println!("      └─ レポート/");
    println!("          ├─ 復旧レポート.docx ({} bytes)",
        result.report_paths.customer_docx.metadata().unwrap().len());
    println!("          ├─ 要確認ファイル一覧.txt ({} bytes)",
        result.report_paths.customer_txt.metadata().unwrap().len());
    println!("          ├─ 業務管理レポート.html ({} bytes)",
        result.report_paths.internal_html.metadata().unwrap().len());
    println!("          └─ report.csv ({} bytes)",
        result.report_paths.csv.metadata().unwrap().len());
    println!();
    println!("📂 社内保存: {:?}", internal_storage.path());
    println!("  └─ 260522-04/case.json (案件情報、お客様には見せない)");
    println!();
    println!("業務指標:");
    println!("  該当ファイル:      {} 件", result.report.total_matched);
    println!("  復旧成功率:        {:.1}%", result.report.recovery_success_rate());
    println!("  品質保証率:        {:.1}%", result.report.quality_assurance_rate());
    println!();
    println!("CS のフロー:");
    println!("  1. 納品 HDD を取り出す → G:\\");
    println!("  2. お客様に G:\\ を送付");
    println!("     → お客様は G:\\260522-04\\ を開くだけで全部見える");
    println!("  3. 社内には案件情報が残る (再復旧依頼に備えて)");
    println!();
    println!("=== Phase 1.5 業務統合層完成 ===");
    println!("=== Phase 2.1 (Tauri UI) への準備完了 ===");
    
    assert!(case.output_dir.is_some());
    assert!(case.recovery_report_summary.is_some());
}

fn count_files_recursive(dir: &std::path::Path) -> usize {
    if !dir.exists() { return 0; }
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count()
}
```

> 注: `walkdir` クレートが必要なら dev-dependencies に追加。`std::fs::read_dir` の再帰版でも可。

## 制約

- **行数目安**:
  - `case-manager/src/output.rs`: 100 行 + テスト 80 行
  - `case-manager/src/orchestration.rs`: 100 行 + テスト 60 行
  - `recovery/src/engine.rs` の拡張: 50 行 + テスト 30 行 (RecoveryConfig 追加)
  - `report/src/business.rs`: 60 行 + テスト 40 行
  - 統合テスト: 200 行
  - 合計: 約 510 行追加 + 210 行テスト
- **単体テスト最低 10 件**
- **結合テスト最低 2 件 (うち 1 件は product_demo)**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持**
- **既存 API は互換維持** (RecoveryEngine::new(output_dir), write_all_reports)

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-case-manager` が全パス
- [ ] `cargo test -p dds-recovery` が全パス (既存テストに影響なし)
- [ ] `cargo test -p dds-report` が全パス (既存テストに影響なし)
- [ ] `cargo test --workspace` 全体で全パス (445+ 件)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_phase_1_5_complete` が pass + 出力が見える
- [ ] 統合テスト `full_business_flow_from_case_creation_to_delivery` が pass
- [ ] 日本語ファイル名・フォルダ名が UTF-8 で正しく扱われる (Windows でも動く想定)

## 関連 FR 要件

- **FR-OUT-01** (案件番号付きディレクトリ) ← 達成
- **FR-OUT-02** (通常/削除ファイル分離) ← 達成
- **FR-OUT-03** (日本語名対応) ← 達成
- **FR-OUT-04** (社内保存と納品物分離) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1.5 完全完成**
4. 次のステップ:
   - **検証 PC で実機ドライラン** (Chouさん手動、フィクスチャでは検証できない実機特有の問題発見)
   - **Phase 2.1 着手準備** (Tauri UI 開発、約 2 ヶ月)

---

## 注意事項

### Windows での日本語フォルダ名

Windows は UTF-16 を内部で使うが、Rust の `PathBuf` は UTF-8 (Unix-like) または UTF-16 (Windows) として扱う:

- `PathBuf::from("G:\\260522-04\\復旧データ")` は Windows で正しく動作
- ファイルシステムが NTFS なら問題なし
- ファイルシステムが FAT32/exFAT でも日本語名は OK (古い ASCII 8.3 形式は使わない)

### `\\` のエスケープ

文字列リテラル内で `\\` はバックスラッシュ 1 文字を表す:
- `"G:\\260522-04"` → 実際の文字列は `G:\260522-04`
- `PathBuf::from("G:\\260522-04")` → 9 文字のパス

Rust のテストで Linux/Mac でクロスプラットフォームに動かしたい場合は `Path::new` と `join` を使う。

### 既存テストの影響

既存の Phase 1 / Chunk 21-22.5 のテストは:

- `RecoveryEngine::new(output_dir)` を使うものは**変更なし** (互換維持されている)
- `dds_report::write_all_reports(report, output_dir)` も**変更なし**
- 新規 API (`RecoveryConfig`, `execute_business_recovery`, `write_business_reports`) は追加

既存テストへの影響をゼロにすることが重要。

### case-manager の依存拡大

case-manager が recovery と report に依存するのは Phase 1.5 の必要悪:

```
変更前 (Chunk 21-22.5):
  case-manager → wish-match → core
  
変更後 (Chunk 23):
  case-manager → wish-match, recovery, report → ...
```

これは `execute_business_recovery` を case-manager に置くために必要。Phase 2 で別クレート (`dds-orchestrator` 等) に分離することも検討可能。

### Phase 1.5 で意図的に除外した機能

- **複数案件の並行管理** (1 PC 1 案件専有なので不要)
- **案件履歴の検索 UI** (Phase 2 で実装)
- **進捗のリアルタイム表示** (Phase 2 で UI と一緒に)
- **失敗時のロールバック** (現状はファイルが残る、CS が手動削除)
- **case.json と納品 HDD の同期** (Phase 2 でユースケース次第)

### Windows 環境での実機検証 (Phase 1.5 完了後)

Chunk 23 完了後の手動検証手順:

1. 検証 PC で:
   ```cmd
   cargo run --example phase_1_5_demo -- ^
     --case-id 260522-04 ^
     --source \\.\PhysicalDriveN ^
     --delivery G:\
   ```
   
   (または統合テストを直接実行)

2. エクスプローラで `G:\260522-04\` を開いて構造確認:
   - 復旧データ\通常ファイル\
   - 復旧データ\削除ファイル\
   - レポート\復旧レポート.docx (Word で開く)
   - レポート\要確認ファイル一覧.txt (Notepad で開く)

3. `C:\cases\260522-04\case.json` を確認 (社内保存)

---

## 質問が必要なケース

- RecoveryEngine の既存 API が `new(output_dir)` ではなく異なる形だった場合
- write_all_reports が既に存在しない場合 (Chunk 20.5 で削除されている?)
- Windows 環境テストで日本語パスに問題が出た場合

---

## 完了報告例

```markdown
## Chunk 23 完了報告

### 新規ファイル
- crates/case-manager/src/output.rs       (100 行 + テスト 80 行)
- crates/case-manager/src/orchestration.rs (100 行 + テスト 60 行)
- crates/report/src/business.rs            (60 行 + テスト 40 行)
- crates/case-manager/tests/business_flow_integration.rs (200 行)

### 修正ファイル
- crates/recovery/src/engine.rs (RecoveryConfig 追加、~50 行)
- crates/case-manager/src/lib.rs (re-export 追加)
- crates/case-manager/Cargo.toml (recovery, report 依存追加)
- crates/recovery/src/lib.rs (RecoveryConfig export)
- crates/report/src/lib.rs (business export)

### 公開 API
- `CaseOutput` (new, root, live_files_dir, deleted_files_dir, reports_dir, 各レポートパス, create_all_dirs)
- `RecoveryConfig` (from_single_dir, from_case_output, with_paths)
- `RecoveryEngine::with_config(config)` (新規)
- `dds_report::write_business_reports()`
- `BusinessReportPaths` struct
- `dds_case_manager::execute_business_recovery()`
- `BusinessRecoveryResult`, `BusinessRecoveryError`

### テスト統計
- 単体: 既存 + 新規 ~10 件 = **455+ 件 pass**
- 結合: 既存 + 新規 2 件 = **64+ 件 pass**
- 全 workspace: **460+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 既存 API 互換維持 (Chunk 17-22.5 のテストは無修正で全 pass)

### 業務価値の見える化 (product_demo_phase_1_5_complete)
```
=== Phase 1.5 Complete Demo (Chunk 23) ===

案件番号: 260522-04

📂 納品 HDD: /tmp/.tmpXXXXXX
  └─ 260522-04/
      ├─ 復旧データ/
      │   ├─ 通常ファイル/ (10 件)
      │   └─ 削除ファイル/ (4 件)
      └─ レポート/
          ├─ 復旧レポート.docx (5234 bytes)
          ├─ 要確認ファイル一覧.txt (412 bytes)
          ├─ 業務管理レポート.html (8932 bytes)
          └─ report.csv (5512 bytes)

📂 社内保存: /tmp/.tmpYYYYYY
  └─ 260522-04/case.json (案件情報、お客様には見せない)

業務指標:
  該当ファイル:      14 件
  復旧成功率:        100.0%
  品質保証率:        71.4%

CS のフロー:
  1. 納品 HDD を取り出す → G:\
  2. お客様に G:\ を送付
     → お客様は G:\260522-04\ を開くだけで全部見える
  3. 社内には案件情報が残る (再復旧依頼に備えて)

=== Phase 1.5 業務統合層完成 ===
=== Phase 2.1 (Tauri UI) への準備完了 ===
```

### 🎉 マイルストーン
- **Phase 1.5 完全完成**
- 業務的に整理された納品物の自動生成
- 案件番号ベースの一貫したオーケストレーション
- 社内保存と納品物の明確な分離
- 日本語フォルダ名・ファイル名で CS / お客様の使いやすさを実現

### 累計 23 chunks
- Phase 1 NTFS-α: 完成 (Chunks 1-20.5)
- Phase 1.5 業務統合: 完成 (Chunks 21-23 含む 22.6, 22.5)
- workspace tests: 460+ 件 pass
- unsafe: 0
- clippy: 0 warning

- **関連 FR**: FR-OUT-01〜04 (達成)

→ tester エージェントへ引き継ぎお願いします
→ tester 合格後、検証 PC での実機ドライランへ移行
```
