# Chunk 24a 指示: お客様向けレポート簡素化 + タイムスタンプ保持

実機ドライランでのフィードバックを反映し、**業務適用品質を業界標準 (R-STUDIO 並み) に引き上げる**重要な改修。

> 🎯 完了時点で「お客様向け納品物が業務的に違和感ない」「復旧ファイルのタイムスタンプが保持される (R-STUDIO 並み)」状態に到達。Workbench が実業務で使える品質になる。

---

## 背景: 実機ドライランで判明した問題

実機テスト (2026-05-22 実施) で 5 点のフィードバック:

```
① 診断速度 (1 秒)            ← 良好
② 復旧速度 (5GB / 20 分)     ← 遅い (Chunk 24b で対応予定)
③ 復旧中の進捗が見えない     ← Chunk 24b で対応予定
④ report.csv 文字化け        ← 本チャンクで対応
⑤ 要確認/未検証もほぼ開ける  ← 業務的に「品質保証率」が誤解を生む
   → 関連表示を削除 (お客様への納品物から)
```

さらに業務観点で:

```
追加発見: 復旧ファイルのタイムスタンプが復旧日になっている
  R-STUDIO 等の業界標準: 元のタイムスタンプを保持
  → 本チャンクで対応 (★ 重要)
```

## 目的

5 つの統合された変更:

| Part | 内容 |
|---|---|
| **A** | お客様向け復旧レポート.docx の簡素化 (案 B 形式、日時削除) |
| **B** | 納品 HDD から TXT / HTML / CSV を削除 |
| **C** | 業務管理レポート.html と report.csv を社内保存に移動 |
| **D** | タイムスタンプ保持 (Creation + Modified + Accessed) |
| **E** | CSV BOM 修正 (社内保存版) |
| **F** | 「品質保証率」関連の表示削除 (内部ロジックは残す) |

## 対象クレート

- **修正**: `crates/case-manager/`, `crates/recovery/`, `crates/report/`, `crates/workbench-dryrun/`
- **新規**: `crates/recovery/src/timestamps.rs` (タイムスタンプ書き込み)
- **影響テスト**: 既存テスト約 20 件の調整

## 重要な設計原則

### 「unsafe 0」方針の限定的緩和

```
[これまで]
全 14 クレート + workbench-dryrun = unsafe 0

[Chunk 24a 以降]
recovery クレートに限定的に unsafe を許容
  - タイムスタンプ書き込み (Windows API SetFileTime) のみ
  - unsafe ブロックは 5-10 行程度
  - 専用のラッパー関数で隔離
  - 他のコードは全て safe のまま
```

理由: タイムスタンプ保持は業界標準で、お客様への業務適用品質に必須。

### お客様向けと社内向けの分離

```
[お客様への納品物] G:\260522-04\
  └ 復旧データ\
  └ レポート\
      └ 復旧レポート.docx (簡素化版のみ)

[社内保存] C:\cases\260522-04\
  └ case.json
  └ 診断結果_CRM貼り付け用.txt
  └ 業務管理レポート.html (新規移動)
  └ 復旧詳細.csv (新規移動、CSV BOM 付き)
```

### 「品質保証率」関連の表示削除 (内部ロジックは残す)

```rust
// 削除:
- DOCX / HTML / CSV からの「品質保証率」「Valid/Invalid/Uncertain」表示
- workbench-dryrun の結果表示の「品質保証率」
- product_demo の出力の「品質保証率」

// 残す:
- RecoveryReport::quality_assurance_rate() メソッド (将来用)
- ValidationStatus enum (内部判定は継続、case.json に記録)
- 業務管理レポート.html の「品質判定の件数」(社内 CS 用)
```

## 仕様参照

### ビジネス要件

- **FR-OUT-05** (お客様向け納品物の簡素化) ← 新規達成
- **FR-OUT-06** (社内・お客様向けの分離) ← 新規達成
- **FR-REC-07** (タイムスタンプ保持) ← 新規達成、業界標準

## 実装内容

### Part A: お客様向け復旧レポート.docx の簡素化

#### `crates/report/src/docx_customer.rs` の修正

```rust
pub fn render_customer_docx(report: &RecoveryReport) -> Result<Vec<u8>, ReportError> {
    let mut docx = Docx::new();
    
    // ===== タイトル =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("データ復旧レポート").size(40).bold())
    );
    
    // ===== 案件情報 =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 案件情報").size(28).bold())
    );
    docx = docx.add_table(Table::new(vec![
        make_kv_row("案件番号", &report.case_id.to_string()),
        // ★ 復旧実施日時は削除 (Chouさんの判断)
    ]));
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== 復旧結果 (★ 案 B 形式、品質判定は削除) =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ 復旧結果").size(28).bold())
    );
    
    let live_count = report.recovered.iter().filter(|e| !e.is_deleted).count();
    let deleted_count = report.recovered.iter().filter(|e| e.is_deleted).count();
    
    docx = docx.add_table(Table::new(vec![
        make_kv_row("通常ファイル", &format!("{} 件", live_count)),
        make_kv_row("削除ファイル", &format!("{} 件", deleted_count)),
        make_kv_row("合計", &format!("{} 件、{}", 
            report.recovered.len(), 
            format_bytes(report.total_bytes_written()))),
    ]));
    docx = docx.add_paragraph(Paragraph::new());
    
    // ===== ご指定優先データ (Wishlist 指定時のみ、品質判定は削除) =====
    if report.priority_count() > 0 {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text("■ ご指定優先データ").size(28).bold())
        );
        docx = docx.add_table(Table::new(vec![
            make_kv_row("該当ファイル", &format!("{} 件", report.priority_count())),
            make_kv_row("ご指定条件", &report.wish_labels.join("、")),
        ]));
        docx = docx.add_paragraph(Paragraph::new());
    }
    
    // ===== お問い合わせ先 =====
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text("■ お問い合わせ先").size(28).bold())
    );
    docx = docx.add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(
            "復旧データに関するお問い合わせは、Digital Data Solution 株式会社までご連絡ください。"
        ))
    );
    
    // ★ 品質判定セクション全体を削除 (Chouさんの判断、お客様への誤解防止)
    // ★ ※ 内部ロジックは維持 (RecoveryReport::quality_assurance_rate() メソッドは残す)
    
    Ok(docx.build()?)
}
```

### Part B: 納品 HDD から不要ファイルを削除

#### `crates/case-manager/src/output.rs` の修正

```rust
impl CaseOutput {
    // 既存: customer_docx_path は維持
    pub fn customer_docx_path(&self) -> PathBuf {
        self.reports_dir().join("復旧レポート.docx")
    }
    
    // ★ 以下のメソッドは削除 or deprecated:
    // - customer_invalid_txt_path()    ← 納品 HDD に出力しない
    // - customer_uncertain_txt_path()  ← 同上
    // - internal_html_path()           ← 社内保存先に移動
    // - csv_path()                     ← 社内保存先に移動
    
    // ★ 新規: 社内保存先 (C:\cases\案件番号\) のパス
    pub fn internal_html_path_in_storage(&self, storage_base: &Path) -> PathBuf {
        storage_base.join(self.case_id.as_str()).join("業務管理レポート.html")
    }
    
    pub fn csv_path_in_storage(&self, storage_base: &Path) -> PathBuf {
        storage_base.join(self.case_id.as_str()).join("復旧詳細.csv")
    }
}
```

#### `crates/report/src/business.rs` の修正

```rust
#[derive(Debug, Clone)]
pub struct BusinessReportPaths {
    // お客様向け (納品 HDD)
    pub customer_docx: PathBuf,
    
    // 社内保存 (C:\cases\)
    pub internal_html: PathBuf,
    pub csv: PathBuf,
    
    // ★ 削除されるフィールド:
    // - customer_invalid_txt
    // - customer_uncertain_txt
}

pub fn write_business_reports(
    report: &RecoveryReport,
    case_output: &CaseOutput,
    internal_storage_base: &Path,  // ★ 新規パラメータ: C:\cases\
) -> Result<BusinessReportPaths, ReportError> {
    // 納品 HDD 側
    std::fs::create_dir_all(case_output.reports_dir())?;
    let customer_docx = case_output.customer_docx_path();
    std::fs::write(&customer_docx, render_customer_docx(report)?)?;
    
    // 社内保存側
    let internal_case_dir = internal_storage_base.join(case_output.case_id().as_str());
    std::fs::create_dir_all(&internal_case_dir)?;
    
    let internal_html = internal_case_dir.join("業務管理レポート.html");
    std::fs::write(&internal_html, render_internal_html(report)?)?;
    
    let csv = internal_case_dir.join("復旧詳細.csv");
    let csv_content = render_csv(report)?;
    // ★ CSV BOM 付加 (UTF-8 BOM = 0xEF 0xBB 0xBF)
    let mut csv_bytes = Vec::with_capacity(3 + csv_content.len());
    csv_bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    csv_bytes.extend_from_slice(csv_content.as_bytes());
    std::fs::write(&csv, csv_bytes)?;
    
    // ★ Invalid TXT、Uncertain TXT は生成しない (削除済み)
    
    Ok(BusinessReportPaths {
        customer_docx,
        internal_html,
        csv,
    })
}
```

### Part C: 業務管理レポート.html の社内移動

#### `crates/case-manager/src/orchestration.rs` の修正

```rust
pub fn execute_business_recovery<F>(
    case: &mut Case,
    drive_root: impl AsRef<Path>,
    volume: &mut NtfsVolume<F>,
    wishlist: &Wishlist,
    exclusions: &ExclusionList,
    storage: &CaseStorage,  // ★ 新規パラメータ: 社内保存先
) -> Result<BusinessRecoveryResult, BusinessRecoveryError>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let case_output = CaseOutput::new(case.case_id.clone(), drive_root.as_ref().to_path_buf());
    case_output.create_all_dirs()?;
    
    let config = RecoveryConfig::from_case_output(&case_output);
    let engine = RecoveryEngine::with_config(config);
    let report = engine.recover_files(volume, wishlist, exclusions)?;
    
    // ★ 社内保存先を渡す
    let report_paths = dds_report::write_business_reports(
        &report, 
        &case_output, 
        storage.base_dir(),  // C:\cases\
    )?;
    
    case.output_dir = Some(case_output.root());
    case.recovery_report_summary = Some(summarize_report(&report));
    case.wishlist = Some(wishlist.clone());
    
    Ok(BusinessRecoveryResult { case_output, report, report_paths })
}
```

### Part D: タイムスタンプ保持 (★ 重要)

#### `crates/recovery/src/timestamps.rs` (新規ファイル)

```rust
//! NTFS タイムスタンプの保持 (Creation / Modified / Accessed).
//!
//! Windows の `SetFileTime` API を使用して、復旧したファイルに
//! 元のタイムスタンプを設定する。R-STUDIO 等の業界標準に準拠。
//!
//! ## 安全性
//! 
//! このモジュールは Windows API を直接呼び出すため `unsafe` を含む。
//! ただし `unsafe` ブロックは `apply_timestamps()` 関数内に限定され、
//! 引数検証と SafeHandle (RAII) で安全性を確保している。

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg(windows)]
use windows_sys::Win32::Foundation::FILETIME;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    SetFileTime, FILE_WRITE_ATTRIBUTES,
};

/// タイムスタンプ書き込みのエラー
#[derive(Debug, Error)]
pub enum TimestampError {
    #[error("ファイルを開けません: {0}")]
    Open(#[from] std::io::Error),
    
    #[error("Windows API SetFileTime が失敗しました (エラーコード: {0})")]
    Win32Error(u32),
    
    #[error("時刻の変換に失敗しました: {0}")]
    TimeConversion(String),
    
    #[cfg(not(windows))]
    #[error("タイムスタンプ書き込みは Windows のみサポートしています")]
    Unsupported,
}

/// NTFS タイムスタンプ (3 種類)
#[derive(Debug, Clone, Copy)]
pub struct NtfsTimestamps {
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub accessed: DateTime<Utc>,
}

/// 指定パスのファイルにタイムスタンプを設定する。
///
/// Windows のみ動作する (`#[cfg(windows)]` でガード)。
#[cfg(windows)]
pub fn apply_timestamps(path: &Path, timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    // ファイルを書き込み属性で開く (タイムスタンプ設定に必要)
    let file = OpenOptions::new()
        .write(true)
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .open(path)?;
    
    // chrono::DateTime → FILETIME 変換
    let creation_ft = datetime_to_filetime(timestamps.created)?;
    let modified_ft = datetime_to_filetime(timestamps.modified)?;
    let accessed_ft = datetime_to_filetime(timestamps.accessed)?;
    
    let handle = file.as_raw_handle();
    
    // SAFETY:
    // - handle は OpenOptions で取得した有効なハンドル
    // - file は本関数のスコープ内で生存している
    // - FILETIME 構造体は値型なので、参照は有効
    // - SetFileTime は Windows API の標準的な使用方法
    let result = unsafe {
        SetFileTime(
            handle as *mut std::ffi::c_void,
            &creation_ft as *const FILETIME,
            &accessed_ft as *const FILETIME,
            &modified_ft as *const FILETIME,
        )
    };
    
    if result == 0 {
        // SAFETY: GetLastError は副作用のない Windows API
        let error_code = unsafe { 
            windows_sys::Win32::Foundation::GetLastError() 
        };
        return Err(TimestampError::Win32Error(error_code));
    }
    
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_timestamps(_path: &Path, _timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    Err(TimestampError::Unsupported)
}

/// chrono::DateTime<Utc> を Windows FILETIME に変換する。
///
/// FILETIME は 1601-01-01 UTC からの 100 ナノ秒単位。
#[cfg(windows)]
fn datetime_to_filetime(dt: DateTime<Utc>) -> Result<FILETIME, TimestampError> {
    // UNIX epoch (1970-01-01) から Windows epoch (1601-01-01) までの差: 11644473600 秒
    const EPOCH_DIFFERENCE_SECONDS: i64 = 11_644_473_600;
    
    let unix_secs = dt.timestamp();
    let unix_nanos = dt.timestamp_subsec_nanos();
    
    let windows_seconds = unix_secs.checked_add(EPOCH_DIFFERENCE_SECONDS)
        .ok_or_else(|| TimestampError::TimeConversion(
            "時刻オーバーフロー".into()
        ))?;
    
    let filetime_100ns = windows_seconds.checked_mul(10_000_000)
        .and_then(|v| v.checked_add((unix_nanos / 100) as i64))
        .ok_or_else(|| TimestampError::TimeConversion(
            "FILETIME 換算オーバーフロー".into()
        ))?;
    
    let filetime_u64 = filetime_100ns as u64;
    
    Ok(FILETIME {
        dwLowDateTime: (filetime_u64 & 0xFFFFFFFF) as u32,
        dwHighDateTime: ((filetime_u64 >> 32) & 0xFFFFFFFF) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn ntfs_timestamps_struct_holds_three_dates() {
        let now = Utc::now();
        let ts = NtfsTimestamps {
            created: now,
            modified: now,
            accessed: now,
        };
        assert_eq!(ts.created, ts.modified);
    }
    
    #[cfg(windows)]
    #[test]
    fn datetime_to_filetime_roundtrip() {
        // 既知の日時で変換
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ft = datetime_to_filetime(dt).unwrap();
        
        // FILETIME → 戻し計算
        let filetime_u64 = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        let windows_seconds = (filetime_u64 / 10_000_000) as i64;
        let unix_secs = windows_seconds - 11_644_473_600;
        
        assert_eq!(unix_secs, dt.timestamp());
    }
    
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_to_actual_file() {
        use std::fs::write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        write(temp.path(), b"test").unwrap();
        
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };
        
        apply_timestamps(temp.path(), &ts).unwrap();
        
        // 書き込まれたか確認
        let metadata = std::fs::metadata(temp.path()).unwrap();
        let modified_time = metadata.modified().unwrap();
        let modified_dt: DateTime<Utc> = modified_time.into();
        
        // 秒精度で一致するはず (NTFS は 100ns 精度だが、Windows がまるめる場合あり)
        assert_eq!(modified_dt.timestamp(), dt.timestamp());
    }
}
```

#### `crates/recovery/Cargo.toml` への追加

```toml
[dependencies]
# 既存依存に追加:
windows-sys = { version = "0.52", features = ["Win32_Foundation", "Win32_Storage_FileSystem"] }
```

#### `crates/recovery/src/engine.rs` の修正

`recover_files` 内で復旧後にタイムスタンプ設定:

```rust
// 復旧実行後
let output_path = compute_output_path(&self.config, &file)?;
let bytes_written = perform_recovery(&file, &output_path)?;

// 全件精密チェック
let validation = run_validators(&output_path);

// ★ 新規: タイムスタンプ保持
let timestamps = NtfsTimestamps {
    created: file.creation_time,
    modified: file.modified_time,
    accessed: file.accessed_time,
};

if let Err(e) = crate::timestamps::apply_timestamps(&output_path, &timestamps) {
    // タイムスタンプ書き込み失敗は警告レベル (致命的ではない)
    log::warn!("タイムスタンプ書き込み失敗: {:?} ({})", output_path, e);
    // 復旧自体は成功扱い、続行
}
```

タイムスタンプ書き込みは「失敗しても致命的ではない」。警告ログのみ。

#### `crates/recovery/src/lib.rs` への追加

```rust
pub mod timestamps;
pub use timestamps::{apply_timestamps, NtfsTimestamps, TimestampError};
```

### Part E: NtfsFile からタイムスタンプ取得

#### `crates/fs-ntfs/src/file.rs` の確認

Chunk 9 で `$STANDARD_INFORMATION` を読んでいるはず。NtfsFile に以下のフィールドが既にある:

```rust
pub struct NtfsFile {
    // 既存:
    pub creation_time: DateTime<Utc>,
    pub modified_time: DateTime<Utc>,
    pub accessed_time: DateTime<Utc>,
    // ...
}
```

→ もし未実装ならこのチャンクで追加が必要。

実装確認:
```bash
grep "creation_time\|modified_time\|accessed_time" crates/fs-ntfs/src/file.rs
```

存在しない場合は本チャンクで追加 (約 30 行)。

### Part F: 「品質保証率」関連の表示削除

#### `crates/report/src/html_internal.rs` (社内向け、品質判定の表示は残す)

```rust
// 業務管理レポート.html は社内 CS 用なので品質判定の件数は維持
// ただし「品質保証率」のパーセンテージ表示は削除

// 修正前:
html.push_str(&format!("    <tr><th>品質保証率</th><td>{:.1}%</td></tr>\n", 
    report.quality_assurance_rate()));

// 修正後: 削除 (パーセンテージ表示なし、件数のみ表示)
// 件数表示は維持:
html.push_str(&format!("    <tr><td>Valid (正常)</td><td>{}</td></tr>\n", report.validated_count()));
// ...
```

#### `crates/recovery/src/report.rs` (メソッドは残す)

```rust
impl RecoveryReport {
    // ★ メソッドは維持 (将来の精度向上、内部ロジック)
    pub fn quality_assurance_rate(&self) -> f64 {
        // 既存実装のまま
    }
    
    pub fn priority_quality_assurance_rate(&self) -> f64 {
        // 既存実装のまま
    }
}
```

#### `crates/workbench-dryrun/src/commands/recover.rs` の修正

```rust
// 修正前:
println!("  品質保証率:      {:.1}%", result.report.quality_assurance_rate());

// 修正後: 削除
// 件数表示は維持:
println!("結果:");
println!("  該当ファイル:    {} 件", result.report.total_matched);
println!("  復旧成功:        {} 件", result.report.recovered.len());
println!("  復旧データ量:    {}", dds_core::format::format_bytes(result.report.total_bytes_written()));
```

### Part G: product_demo の更新

全 product_demo テストの品質保証率関連を削除:

```rust
// 修正前:
println!("  品質保証率:   {:.1}%", result.report.quality_assurance_rate());

// 修正後: 削除
```

主な対象:
- `product_demo_phase_1_5_business_aligned`
- `product_demo_phase_1_5_complete`
- `product_demo_phase_1_5_final`

## 単体テスト要件 (最低 10 件、新規)

### `timestamps.rs` (最低 4 件)

1. `ntfs_timestamps_struct_holds_three_dates`
2. `datetime_to_filetime_roundtrip` (Windows のみ)
3. `apply_timestamps_to_actual_file` (Windows のみ)
4. `apply_timestamps_returns_error_on_non_windows` (non-Windows)

### `report/src/business.rs` (最低 3 件)

5. `write_business_reports_creates_only_docx_in_delivery`
6. `write_business_reports_creates_html_and_csv_in_storage`
7. `write_business_reports_csv_includes_bom`

### `report/src/docx_customer.rs` (最低 2 件)

8. `customer_docx_omits_quality_metrics`: 品質保証率の表示がない
9. `customer_docx_omits_recovery_datetime`: 復旧実施日時の表示がない

### `case-manager` (最低 1 件)

10. `case_output_no_longer_has_internal_paths_on_delivery`: 納品 HDD のレポートディレクトリに HTML/CSV がない

## 結合テスト要件 (最低 2 件)

### 1. 納品 HDD と社内保存の分離

```rust
#[test]
fn business_reports_separated_between_delivery_and_internal() {
    let delivery_dir = TempDir::new().unwrap();
    let internal_dir = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(internal_dir.path());
    // ... setup ...
    
    let result = execute_business_recovery(
        &mut case, delivery_dir.path(), &mut volume, &wishlist, &exclusions, &storage,
    ).unwrap();
    
    // 納品 HDD: 復旧レポート.docx のみ
    let delivery_reports = delivery_dir.path().join("260522-04").join("レポート");
    assert!(delivery_reports.join("復旧レポート.docx").exists());
    assert!(!delivery_reports.join("業務管理レポート.html").exists());
    assert!(!delivery_reports.join("report.csv").exists());
    assert!(!delivery_reports.join("破損疑いファイル一覧.txt").exists());
    assert!(!delivery_reports.join("自動確認対象外ファイル一覧.txt").exists());
    
    // 社内保存: HTML と CSV
    let internal_case = internal_dir.path().join("260522-04");
    assert!(internal_case.join("業務管理レポート.html").exists());
    assert!(internal_case.join("復旧詳細.csv").exists());
    
    // CSV BOM 確認
    let csv_bytes = std::fs::read(internal_case.join("復旧詳細.csv")).unwrap();
    assert_eq!(&csv_bytes[..3], &[0xEF, 0xBB, 0xBF]);
}
```

### 2. タイムスタンプ保持の end-to-end (Windows のみ)

```rust
#[cfg(windows)]
#[test]
fn recovered_files_preserve_original_timestamps() {
    // ... setup with ntfs_mixed_formats fixture ...
    
    let result = execute_business_recovery(/* ... */).unwrap();
    
    // 復旧されたファイルのタイムスタンプを確認
    for entry in &result.report.recovered {
        let metadata = std::fs::metadata(&entry.output_path).unwrap();
        let modified: DateTime<Utc> = metadata.modified().unwrap().into();
        
        // 復旧したファイルの modified_time が、ソースの modified_time に近い
        // (NTFS は 100ns 精度、Windows がまるめる場合あり、秒精度で比較)
        // 注: 実際の比較は元の NtfsFile の modified_time と
        //     復旧後のファイルの modified time を一致確認
    }
}
```

## 制約

- **行数目安**:
  - `recovery/src/timestamps.rs` (新規): 120 行 + テスト 80 行
  - `recovery/src/engine.rs` 修正: +20 行 (タイムスタンプ呼出)
  - `report/src/docx_customer.rs` 修正: -50 行 (品質判定削除) +20 行 (新形式)
  - `report/src/business.rs` 修正: +40 行 (社内保存分離、BOM)
  - `report/src/html_internal.rs` 修正: -10 行 (率削除)
  - `case-manager/src/output.rs` 修正: +30 行 (パス分離)
  - `case-manager/src/orchestration.rs` 修正: +20 行 (storage パラメータ)
  - `workbench-dryrun/src/commands/recover.rs` 修正: -10 行 (率削除)
  - 既存テスト修正: 約 30 件、約 100 行
  - 合計: 約 350 行追加・修正
- **単体テスト新規**: 最低 10 件
- **結合テスト**: 最低 2 件 (うち 1 件は Windows 専用)
- **`unsafe` 行数**: 約 5-10 行 (timestamps.rs に限定)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス (約 520+ 件)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `unsafe` ブロックは `timestamps.rs` の `apply_timestamps()` 関数内に限定
- [ ] お客様向け復旧レポート.docx に「品質保証率」「Valid/Invalid/Uncertain」の表示なし
- [ ] お客様向け復旧レポート.docx に復旧実施日時の表示なし
- [ ] 納品 HDD に 業務管理レポート.html / report.csv / 各種 TXT がない
- [ ] 社内保存 (C:\cases\) に業務管理レポート.html と 復旧詳細.csv がある
- [ ] CSV ファイルの先頭 3 バイトが UTF-8 BOM (0xEF 0xBB 0xBF)
- [ ] 復旧したファイルの Creation/Modified/Accessed タイムスタンプが保持されている
- [ ] `RecoveryReport::quality_assurance_rate()` メソッドは存在 (内部ロジック維持)

## 関連 FR 要件

- **FR-OUT-05** (お客様向け納品物の簡素化) ← 達成
- **FR-OUT-06** (社内・お客様向けの分離) ← 達成
- **FR-REC-07** (タイムスタンプ保持) ← 達成、業界標準準拠

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1.5 業務適用品質完成 (実機ドライランフィードバック反映)**
4. **Chouさんが再度実機ドライラン実施**
   - お客様向け復旧レポート.docx の確認
   - 復旧ファイルのタイムスタンプ確認 (R-STUDIO と比較)
   - CSV 文字化け確認 (社内保存版)
5. その後 Chunk 24b (パフォーマンス改善 + 進捗表示) へ

---

## 注意事項

### `unsafe` の限定的許容

「unsafe 0」を 28 chunks 守ってきた中で、Phase 1.5 で初めて緩和:

```
[許容範囲]
crates/recovery/src/timestamps.rs の apply_timestamps() 関数内のみ
unsafe ブロック: 約 5-10 行

[安全性確保策]
- 関数の引数検証
- OpenOptions による安全なハンドル取得 (RAII)
- 純粋関数 (副作用なし、戻り値で結果)
- 完全なエラーハンドリング
- Windows 専用 (#[cfg(windows)] でガード)
- 単体テストで挙動検証
```

### タイムスタンプ書き込みの失敗時の挙動

```rust
// 復旧自体は成功扱い、警告ログのみ
if let Err(e) = apply_timestamps(...) {
    log::warn!("タイムスタンプ書き込み失敗: ...");
    // 復旧は続行
}
```

理由:
- お客様の HDD のファイルは復旧できている (内容は正しい)
- タイムスタンプだけが「今日の日付」になる
- これは「業界標準より少し劣る」状態だが、復旧失敗ではない
- 警告ログで CS が把握できる

### Phase 2.1 UI 設計への引き継ぎ

```
[UI で表示すべきこと]
- 復旧レポート.docx と同じレベルの情報 (お客様向け)
- 業務管理レポート.html と同じレベルの情報 (社内 CS 用、UI で切替)
- タイムスタンプ保持の有無 (オプション)

[Chunk 24a 完了後]
お客様向けと社内向けの情報設計が確立
→ Phase 2.1 UI で同じ情報設計を踏襲
```

### 進捗表示とパフォーマンスは Chunk 24b で

Chunk 24a 完了後、Chunk 24b で:
- 進捗表示 (CLI で 5 秒おきに「N/M (XX%)」)
- パフォーマンス改善 (4 MB/s → 40-100 MB/s)

これらは「お客様向け」ではなく「社内業務効率」の問題なので、Chunk 24a の業務適用後でも実機ドライラン可能。

---

## 質問が必要なケース

- NtfsFile に creation/modified/accessed フィールドがない場合 (Chunk 9 の実装に依存)
- windows-sys クレートのバージョン互換性問題
- Windows 以外のプラットフォーム (Linux/macOS) でのテスト戦略

---

## 完了報告例

```markdown
## Chunk 24a 完了報告

### 新規ファイル
- crates/recovery/src/timestamps.rs (120 行 + テスト 80 行)
- crates/recovery/src/timestamps.rs に unsafe ブロック 1 つ (apply_timestamps 関数内、約 6 行)

### 修正ファイル
- crates/recovery/src/engine.rs (タイムスタンプ呼出 +20 行)
- crates/report/src/docx_customer.rs (品質判定削除 -50 行、新形式 +20 行)
- crates/report/src/business.rs (社内保存分離、BOM +40 行)
- crates/report/src/html_internal.rs (率削除 -10 行)
- crates/case-manager/src/output.rs (パス分離 +30 行)
- crates/case-manager/src/orchestration.rs (storage パラメータ +20 行)
- crates/workbench-dryrun/src/commands/recover.rs (率削除 -10 行)
- 既存テスト 30 件、約 100 行修正

### unsafe 統計
- 全 workspace の unsafe 行数: 0 → 約 6 行 (timestamps.rs に限定)

### テスト統計
- 単体: 既存 + 新規 10 件 = **520+ 件 pass**
- 結合: 既存 + 新規 2 件 = **65+ 件 pass**
- 全 workspace: **520+ 件 pass**

### 品質
- clippy 0 warning
- 全公開 API に rustdoc
- 「unsafe 0」から「unsafe 6 行 (限定的)」へ意図的な変更

### 業務的成果
- お客様向け納品物の簡素化 (誤解を招く品質判定表示を削除)
- 社内向け詳細レポートは C:\cases\ に保存 (CS の業務管理用)
- CSV BOM 付加で Excel 文字化け解消
- タイムスタンプ保持で R-STUDIO 並みの業界標準品質

### 🎉 マイルストーン
- **Phase 1.5 業務適用品質完成**
- 実機ドライランフィードバック (4 項目) を反映
  ④ CSV 文字化け: 解消
  ⑤ 品質保証率の誤解: 解消 (お客様向け表示削除)
  追加: タイムスタンプ保持 (R-STUDIO 並み)
- 業界標準ツール (R-STUDIO 等) と並ぶ品質に

- **関連 FR**: FR-OUT-05、FR-OUT-06、FR-REC-07 (達成)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんによる 2 回目実機ドライランへ
→ その後、Chunk 24b (パフォーマンス + 進捗) 着手
```
