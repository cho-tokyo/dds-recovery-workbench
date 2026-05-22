# Chunk 22 指示: 診断エンジン + CRM 貼り付けテキスト生成

Phase 1.5 の業務的に最も価値の高いチャンク。**NTFS 論理診断を自動化し、CRM へコピペで貼り付け可能な業務テキストを生成**します。

> 🎯 完了時点で「HDD 接続 → 1 コマンド実行 → 47 秒後に CRM 貼り付けテキストが画面に表示」が動く。月 800 件の診断業務の手間が大幅削減。

---

## 目的

論理診断エンジンを構築する:

1. **`DiagnosticEngine`**: NtfsVolume を読み込んで包括的な診断レポートを生成
2. **`DiagnosticReport`**: 診断結果のフル構造体 (ハードウェア/FS/症状/統計/異常)
3. **症状判定ロジック**: None / Deleted / Formatted / FilesystemError / Mixed の自動判定
4. **形式別ブレイクダウン**: 拡張子別の件数 + 合計サイズ
5. **フォルダ別ブレイクダウン**: 上位 10 フォルダ (件数順)
6. **削除ファイル統計**: 形式別 + フォルダ別の内訳
7. **FS 異常検出**: MFT 破損、不正 run-list の件数
8. **フォーマット痕跡検出**: 基本版 (新 MFT サイズ判定)
9. **CRM 貼り付けテキスト生成**: 業務的に読みやすい日本語フォーマット
10. **DiagnosticReport → DiagnosticInput 変換**: case.json への保存用

## 対象クレート

`crates/diagnostic/` (Chunk 1 で空スケルトン作成済み、本実装)

## 重要な設計原則

### 単一パスでの統計収集

MFT スキャンは I/O が重い。**1 回の iter で全統計を取得**する設計:

```rust
✗ 悪い設計:
  - 全ファイル統計のために 1 回 iter
  - 形式別集計のために 1 回 iter
  - フォルダ別集計のために 1 回 iter
  - 削除ファイル統計のために 1 回 iter
  
○ 良い設計:
  iter_files() を 1 回だけ呼び、すべての集計を並行して計算
```

### 「論理」診断のみ

物理セクタ走査は範囲外 (Q20 確定)。MFT 読み取りのみで完結する診断:

- 健康な 2TB HDD: 約 30-60 秒
- 業務的に許容できる時間 (Q21: 1 分以内)

### case-manager との関係

```
diagnostic crate
    ├ DiagnosticEngine: NtfsVolume → DiagnosticReport を生成
    └ DiagnosticReport.to_diagnostic_input() → case.json 保存用に変換

case-manager crate
    └ DiagnosticInput: case.json に格納される slim 版
```

`DiagnosticReport` (full、in-memory) と `DiagnosticInput` (slim、永続化) を明確に分離。

## 仕様参照

### ビジネス要件

- **FR-DIAG-01**: NTFS 論理診断 (MFT 読み取りベース)
- **FR-DIAG-02**: 症状の自動判定
- **FR-DIAG-03**: 削除ファイル統計 (形式・フォルダ別)
- **FR-DIAG-04**: CRM 貼り付け用テキスト生成
- **FR-DIAG-05**: 1 分以内の診断完了 (健康な HDD 前提)

### 既存実装の参照

- `dds-fs-ntfs::NtfsVolume`, `NtfsFile`: MFT 読み取り基盤 (Chunks 4-14)
- `dds-case-manager::DiagnosticInput`, `Symptom`, `DeletedFileStats`: 永続化用構造体 (Chunk 21)

## 実装内容

### モジュール構成

```
crates/diagnostic/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-export + DiagnosticEngine entry point
    ├── error.rs            ← DiagnosticError
    ├── report.rs           ← DiagnosticReport, HardwareInfo, FilesystemInfo, etc.
    ├── aggregator.rs       ← 単一パスでの統計集計
    ├── symptom_detector.rs ← 症状判定ロジック
    └── crm_text.rs         ← CRM 貼り付けテキスト生成
```

### Cargo.toml

```toml
[package]
name = "dds-diagnostic"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true

dds-core.workspace = true
dds-fs-ntfs.workspace = true
dds-case-manager.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

### 1. `error.rs`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiagnosticError {
    #[error("Volume error: {0}")]
    Volume(#[from] dds_fs_ntfs::VolumeError),
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Diagnostic timeout: exceeded {limit_secs} seconds")]
    Timeout { limit_secs: u64 },
}
```

### 2. `report.rs`

```rust
use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use dds_case_manager::{
    CaseId, DeletedFileStats, DiagnosticInput, FsAnomaly, Symptom,
};

/// 診断結果のフル構造体。in-memory で全情報を保持。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub case_id: CaseId,
    pub diagnosed_at: DateTime<Utc>,
    pub duration_secs: u64,
    
    pub hardware: HardwareInfo,
    pub filesystem: FilesystemInfo,
    pub symptom: Symptom,
    
    pub file_stats: FileStatistics,
    pub format_breakdown: BTreeMap<String, FormatCount>,
    pub folder_breakdown: Vec<FolderCount>,
    
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub anomalies: FsAnomalyReport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// HDD のモデル名 (例: "WDC WD20EZRX-00DC0B0")。Phase 1.5 では None
    pub model: Option<String>,
    /// HDD のハードウェアシリアル (例: "WD-WCC4N1234567")。Phase 1.5 では None
    pub serial: Option<String>,
    /// パーティションサイズ (バイト)
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemInfo {
    pub fs_type: String,                // "NTFS"
    pub volume_serial: Option<String>,   // 16 進文字列 (例: "A1B2C3D4")
    pub cluster_size_bytes: u32,
    pub total_clusters: u64,
    pub used_clusters: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileStatistics {
    pub total_files: usize,
    pub live_files: usize,
    pub deleted_files: usize,
    pub directories: usize,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormatCount {
    pub count: usize,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCount {
    pub path: String,
    pub file_count: usize,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FsAnomalyReport {
    /// MFT エントリ読み取りで失敗した件数 (構造的破損の指標)
    pub mft_corrupted_count: usize,
    /// run-list 解析失敗の件数 (データクラスタ参照異常)
    pub invalid_runlist_count: usize,
    /// Boot sector の異常 (backup と不一致など)
    pub boot_sector_issues: Vec<String>,
    /// その他の異常
    pub other_issues: Vec<String>,
}

impl FsAnomalyReport {
    pub fn has_any_anomaly(&self) -> bool {
        self.mft_corrupted_count > 0
            || self.invalid_runlist_count > 0
            || !self.boot_sector_issues.is_empty()
            || !self.other_issues.is_empty()
    }
    
    /// FsAnomaly enum の Vec に変換 (Symptom::FilesystemError に渡す用)
    pub fn to_anomaly_list(&self) -> Vec<FsAnomaly> {
        let mut list = Vec::new();
        if self.mft_corrupted_count > 0 {
            list.push(FsAnomaly::MftEntryCorrupted { count: self.mft_corrupted_count });
        }
        if self.invalid_runlist_count > 0 {
            list.push(FsAnomaly::InvalidRunList { count: self.invalid_runlist_count });
        }
        for issue in &self.boot_sector_issues {
            list.push(FsAnomaly::BootSectorAnomaly { description: issue.clone() });
        }
        for issue in &self.other_issues {
            list.push(FsAnomaly::Other { description: issue.clone() });
        }
        list
    }
}

impl DiagnosticReport {
    /// case.json 保存用の slim 版に変換
    pub fn to_diagnostic_input(&self) -> DiagnosticInput {
        DiagnosticInput {
            diagnosed_at: Some(self.diagnosed_at),
            duration_secs: Some(self.duration_secs),
            filesystem_type: Some(self.filesystem.fs_type.clone()),
            symptom: Some(self.symptom.clone()),
            total_files: self.file_stats.total_files,
            deleted_files: self.file_stats.deleted_files,
            total_size_bytes: self.file_stats.total_size_bytes,
            deleted_file_stats: self.deleted_file_stats.clone(),
            notes: String::new(),
        }
    }
    
    /// CRM 貼り付け用テキストを生成 (crm_text.rs 経由)
    pub fn to_crm_text(&self) -> String {
        crate::crm_text::render(self)
    }
}
```

### 3. `aggregator.rs`

単一パスで全統計を集計する核心ロジック:

```rust
use std::collections::{BTreeMap, HashMap};

use dds_fs_ntfs::{NtfsFile, NtfsVolume};
use dds_case_manager::DeletedFileStats;

use crate::error::DiagnosticError;
use crate::report::{FileStatistics, FolderCount, FormatCount, FsAnomalyReport};

/// 単一パスでの集計結果
pub struct AggregateResult {
    pub file_stats: FileStatistics,
    pub format_breakdown: BTreeMap<String, FormatCount>,
    pub folder_breakdown: Vec<FolderCount>,
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub anomalies: FsAnomalyReport,
}

/// volume の全 MFT エントリを 1 回だけ走査し、全統計を集計する。
pub fn aggregate_all<F>(volume: &mut NtfsVolume<F>) -> Result<AggregateResult, DiagnosticError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let mut file_stats = FileStatistics::default();
    let mut format_breakdown: BTreeMap<String, FormatCount> = BTreeMap::new();
    let mut all_folders: HashMap<String, (usize, u64)> = HashMap::new();
    
    // 削除専用トラッキング
    let mut deleted_by_ext: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleted_by_folder: HashMap<String, usize> = HashMap::new();
    let mut deleted_total_size: u64 = 0;
    let mut deleted_count: usize = 0;
    
    // 異常トラッキング
    let mut anomalies = FsAnomalyReport::default();
    
    // 単一パス
    for result in volume.iter_files() {
        match result {
            Ok(file) => {
                if !file.is_user_file() {
                    continue;
                }
                
                file_stats.total_files += 1;
                file_stats.total_size_bytes = file_stats.total_size_bytes.saturating_add(file.size);
                
                if file.is_deleted {
                    file_stats.deleted_files += 1;
                } else {
                    file_stats.live_files += 1;
                }
                
                if file.is_directory {
                    file_stats.directories += 1;
                    continue;  // ディレクトリは形式・フォルダ集計から除外
                }
                
                // 形式別集計
                let ext = file
                    .extension()
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(なし)".to_string());
                let entry = format_breakdown.entry(ext.clone()).or_default();
                entry.count += 1;
                entry.total_size_bytes = entry.total_size_bytes.saturating_add(file.size);
                
                // フォルダ別集計
                let folder = extract_folder(&file.path);
                let f_entry = all_folders.entry(folder.clone()).or_insert((0, 0));
                f_entry.0 += 1;
                f_entry.1 = f_entry.1.saturating_add(file.size);
                
                // 削除専用統計
                if file.is_deleted {
                    deleted_count += 1;
                    deleted_total_size = deleted_total_size.saturating_add(file.size);
                    if ext != "(なし)" {
                        *deleted_by_ext.entry(ext).or_insert(0) += 1;
                    }
                    *deleted_by_folder.entry(folder).or_insert(0) += 1;
                }
            }
            Err(e) => {
                classify_error(&e, &mut anomalies);
            }
        }
    }
    
    // フォルダ Top 10 を抽出
    let mut folder_vec: Vec<FolderCount> = all_folders
        .into_iter()
        .map(|(path, (count, size))| FolderCount {
            path,
            file_count: count,
            total_size_bytes: size,
        })
        .collect();
    folder_vec.sort_by(|a, b| b.file_count.cmp(&a.file_count));
    folder_vec.truncate(10);
    
    // 削除統計を構築 (件数 > 0 の場合のみ)
    let deleted_file_stats = if deleted_count > 0 {
        let mut df_vec: Vec<(String, usize)> = deleted_by_folder.into_iter().collect();
        df_vec.sort_by(|a, b| b.1.cmp(&a.1));
        df_vec.truncate(5);  // Top 5 フォルダ
        
        Some(DeletedFileStats {
            total_count: deleted_count,
            by_extension: deleted_by_ext,
            by_folder: df_vec,
            estimated_total_size: deleted_total_size,
            recoverability_estimate: None,  // Chunk 22.5 で埋まる
        })
    } else {
        None
    };
    
    Ok(AggregateResult {
        file_stats,
        format_breakdown,
        folder_breakdown: folder_vec,
        deleted_file_stats,
        anomalies,
    })
}

/// パスからフォルダ部分を抽出する。
/// 例: "\Users\Chou\file.txt" → "\Users\Chou"
///     "\file.txt"             → "\"
///     "file.txt"               → "(root)"
fn extract_folder(path: &str) -> String {
    match path.rfind('\\') {
        Some(0) => "\\".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => "(root)".to_string(),
    }
}

/// エラーメッセージを分類して FsAnomalyReport に集計
fn classify_error(e: &dds_fs_ntfs::VolumeError, anomalies: &mut FsAnomalyReport) {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    
    if lower.contains("mft") || lower.contains("entry") || lower.contains("record") {
        anomalies.mft_corrupted_count += 1;
    } else if lower.contains("runlist") || lower.contains("run-list") || lower.contains("data run") {
        anomalies.invalid_runlist_count += 1;
    } else {
        anomalies.other_issues.push(msg);
    }
}
```

### 4. `symptom_detector.rs`

```rust
use dds_case_manager::{Symptom};

use crate::report::{FileStatistics, FsAnomalyReport};

/// 統計結果から症状を判定する。
///
/// 優先順位:
/// 1. FS 異常 (MFT 破損、不正 run-list 等) → FilesystemError
/// 2. フォーマット痕跡 (新 MFT が非常に小さい) → Formatted
/// 3. 削除エントリあり → Deleted
/// 4. 上記なし → None
///
/// 複数該当する場合は Symptom::Mixed
pub fn detect_symptom(
    file_stats: &FileStatistics,
    anomalies: &FsAnomalyReport,
    has_deleted: bool,
) -> Symptom {
    let mut symptoms = Vec::new();
    
    // FS 異常チェック
    if anomalies.has_any_anomaly() {
        symptoms.push(Symptom::FilesystemError {
            anomalies: anomalies.to_anomaly_list(),
        });
    }
    
    // フォーマット痕跡チェック (Phase 1 簡易版)
    // ヒューリスティック: 全ファイル数が非常に少ない (50 未満) +
    //                   ディレクトリ数も少ない (10 未満)
    // → クイックフォーマット後の新 MFT と推定
    if file_stats.total_files < 50 && file_stats.directories < 10 {
        symptoms.push(Symptom::Formatted {
            current_mft_entries: file_stats.total_files,
            old_mft_recoverability_hint: None,  // Phase 2 で MFT カービング実装時に
        });
    }
    
    // 削除チェック
    if has_deleted {
        symptoms.push(Symptom::Deleted);
    }
    
    match symptoms.len() {
        0 => Symptom::None,
        1 => symptoms.into_iter().next().unwrap(),
        _ => Symptom::Mixed { symptoms },
    }
}
```

### 5. `crm_text.rs`

```rust
use std::fmt::Write;

use dds_case_manager::Symptom;
use dds_core::format::format_bytes;  // dds-core に追加する予定 (注意事項参照)

use crate::report::DiagnosticReport;

/// CRM 貼り付け用の業務テキストを生成する。
pub fn render(report: &DiagnosticReport) -> String {
    let mut s = String::with_capacity(2048);
    
    // ヘッダー
    let _ = writeln!(s, "=== 論理診断結果 (案件 {}) ===", report.case_id);
    let _ = writeln!(s, "診断日時: {}", report.diagnosed_at.format("%Y-%m-%d %H:%M"));
    let _ = writeln!(s, "診断時間: {} 秒", report.duration_secs);
    let _ = writeln!(s, "※物理診断は別途実施済み");
    let _ = writeln!(s);
    
    // ハードウェア
    let _ = writeln!(s, "【ハードウェア】");
    if let Some(model) = &report.hardware.model {
        let _ = writeln!(s, "HDD: {}", model);
    }
    if let Some(serial) = &report.hardware.serial {
        let _ = writeln!(s, "シリアル: {}", serial);
    }
    let _ = writeln!(s, "容量: {}", format_bytes(report.hardware.size_bytes));
    let _ = writeln!(s);
    
    // ファイルシステム
    let _ = writeln!(s, "【ファイルシステム】");
    let _ = writeln!(s, "種類: {}", report.filesystem.fs_type);
    if let Some(vsn) = &report.filesystem.volume_serial {
        let _ = writeln!(s, "ボリュームシリアル: {}", vsn);
    }
    let _ = writeln!(s, "クラスタサイズ: {} bytes", report.filesystem.cluster_size_bytes);
    let used_bytes = report.filesystem.used_clusters
        .saturating_mul(report.filesystem.cluster_size_bytes as u64);
    let total_bytes = report.filesystem.total_clusters
        .saturating_mul(report.filesystem.cluster_size_bytes as u64);
    let usage_pct = if total_bytes > 0 {
        (used_bytes as f64) / (total_bytes as f64) * 100.0
    } else { 0.0 };
    let _ = writeln!(s, "使用率: {} / {} ({:.1}%)",
        format_bytes(used_bytes), format_bytes(total_bytes), usage_pct);
    let _ = writeln!(s);
    
    // 症状判定
    let _ = writeln!(s, "【症状判定】");
    let _ = writeln!(s, "主症状: {}", report.symptom.primary_label());
    render_symptom_details(&mut s, &report.symptom);
    let _ = writeln!(s);
    
    // ファイル統計
    let _ = writeln!(s, "【ファイル統計】");
    let _ = writeln!(s, "全ファイル: {} 件 ({})",
        report.file_stats.total_files, format_bytes(report.file_stats.total_size_bytes));
    let _ = writeln!(s, "  - 通常 (生存): {} 件", report.file_stats.live_files);
    let _ = writeln!(s, "  - 削除済み: {} 件", report.file_stats.deleted_files);
    let _ = writeln!(s, "ディレクトリ: {} 件", report.file_stats.directories);
    let _ = writeln!(s);
    
    // 削除ファイル内訳 (主症状が「削除」 or 「フォーマット」の場合に表示)
    if let Some(deleted) = &report.deleted_file_stats {
        let _ = writeln!(s, "【削除ファイルの内訳】");
        if !deleted.by_extension.is_empty() {
            let _ = writeln!(s, "形式別:");
            let mut ext_vec: Vec<(&String, &usize)> = deleted.by_extension.iter().collect();
            ext_vec.sort_by(|a, b| b.1.cmp(a.1));
            for (ext, count) in ext_vec.iter().take(10) {
                let _ = writeln!(s, "  {}: {} 件", ext.to_uppercase(), count);
            }
        }
        if !deleted.by_folder.is_empty() {
            let _ = writeln!(s);
            let _ = writeln!(s, "フォルダ別:");
            for (folder, count) in deleted.by_folder.iter().take(5) {
                let _ = writeln!(s, "  {}: {} 件", folder, count);
            }
        }
        let _ = writeln!(s, "推定合計サイズ: {}", format_bytes(deleted.estimated_total_size));
        let _ = writeln!(s);
    }
    
    // 生存ファイル統計
    let _ = writeln!(s, "【生存ファイル統計】(参考、主要形式)");
    let mut formats: Vec<(&String, &crate::report::FormatCount)> =
        report.format_breakdown.iter().collect();
    formats.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    for (ext, count) in formats.iter().take(10) {
        let _ = writeln!(s, "  {}: {} 件 / {}",
            ext.to_uppercase(),
            count.count,
            format_bytes(count.total_size_bytes));
    }
    let _ = writeln!(s);
    
    // 主なフォルダ
    if !report.folder_breakdown.is_empty() {
        let _ = writeln!(s, "【主なフォルダ】(上位 10)");
        for folder in report.folder_breakdown.iter().take(10) {
            let _ = writeln!(s, "  {}: {} 件 / {}",
                folder.path, folder.file_count, format_bytes(folder.total_size_bytes));
        }
        let _ = writeln!(s);
    }
    
    // FS 異常
    let _ = writeln!(s, "【ファイルシステムの破損】");
    let _ = writeln!(s, "MFT エントリ破損: {} 件", report.anomalies.mft_corrupted_count);
    let _ = writeln!(s, "不正な run-list: {} 件", report.anomalies.invalid_runlist_count);
    if report.anomalies.boot_sector_issues.is_empty() {
        let _ = writeln!(s, "Boot sector: 正常");
    } else {
        let _ = writeln!(s, "Boot sector の異常: {} 件", report.anomalies.boot_sector_issues.len());
    }
    let _ = writeln!(s);
    
    // 物理不良チェック
    let _ = writeln!(s, "【物理不良チェック】");
    let _ = writeln!(s, "未実施 (Phase 2 で対応予定)");
    let _ = writeln!(s);
    
    let _ = writeln!(s, "=== 診断完了 ===");
    
    s
}

fn render_symptom_details(s: &mut String, symptom: &Symptom) {
    match symptom {
        Symptom::None => {
            let _ = writeln!(s, "- ファイルシステム署名: 正常 (NTFS 認識成功)");
            let _ = writeln!(s, "- MFT 構造: 正常");
            let _ = writeln!(s, "- 削除エントリ: なし");
            let _ = writeln!(s, "- フォーマット痕跡: なし");
        }
        Symptom::Deleted => {
            let _ = writeln!(s, "- ファイルシステム署名: 正常");
            let _ = writeln!(s, "- MFT 構造: 正常");
            let _ = writeln!(s, "- フォーマット痕跡: なし");
            let _ = writeln!(s, "  ※削除エントリ検出 (件数は下記「削除ファイル」参照)");
        }
        Symptom::Formatted { current_mft_entries, old_mft_recoverability_hint } => {
            let _ = writeln!(s, "- 新 MFT エントリ数: {} 件 (初期化された MFT と推定)", current_mft_entries);
            if let Some(hint) = old_mft_recoverability_hint {
                let _ = writeln!(s, "- 旧 MFT 残存度: {:.1}%", hint * 100.0);
            } else {
                let _ = writeln!(s, "- 旧 MFT 残存度: 未計測 (Phase 2 で対応予定)");
            }
            let _ = writeln!(s, "  ※フォーマット前ファイルの復旧には MFT カービング機能が必要 (Phase 2)");
        }
        Symptom::FilesystemError { anomalies } => {
            let _ = writeln!(s, "- 検出された異常:");
            for a in anomalies {
                let _ = writeln!(s, "  ・{}", anomaly_label(a));
            }
        }
        Symptom::Mixed { symptoms } => {
            let _ = writeln!(s, "- 複合症状:");
            for sub in symptoms {
                let _ = writeln!(s, "  ・{}", sub.primary_label());
            }
        }
    }
}

fn anomaly_label(a: &dds_case_manager::FsAnomaly) -> String {
    use dds_case_manager::FsAnomaly::*;
    match a {
        MftEntryCorrupted { count } => format!("MFT エントリ破損 {} 件", count),
        InvalidRunList { count } => format!("不正な run-list {} 件", count),
        BootSectorAnomaly { description } => format!("Boot sector: {}", description),
        InvalidVolumeSerial => "Volume Serial Number 異常".to_string(),
        Other { description } => description.clone(),
    }
}
```

### 6. `lib.rs`

```rust
//! NTFS 論理診断エンジン。
//!
//! 単一パスで MFT を走査し、ファイル統計・形式別内訳・フォルダ別内訳・
//! 削除ファイル統計・FS 異常を集計、症状を自動判定する。
//!
//! CRM 貼り付け用テキストと、case.json 保存用の slim 版 (DiagnosticInput) を生成。

pub mod aggregator;
pub mod crm_text;
pub mod error;
pub mod report;
pub mod symptom_detector;

pub use error::DiagnosticError;
pub use report::{
    DiagnosticReport, FileStatistics, FilesystemInfo, FolderCount, FormatCount,
    FsAnomalyReport, HardwareInfo,
};

use chrono::Utc;

use dds_case_manager::CaseId;
use dds_fs_ntfs::NtfsVolume;

/// 診断エンジン本体。
pub struct DiagnosticEngine;

impl DiagnosticEngine {
    /// NtfsVolume を診断し、DiagnosticReport を返す。
    ///
    /// 内部で MFT を 1 回だけ走査して全統計を集計する。
    /// 健康な 2TB HDD で約 30-60 秒の想定。
    pub fn diagnose<F>(
        volume: &mut NtfsVolume<F>,
        case_id: CaseId,
    ) -> Result<DiagnosticReport, DiagnosticError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();
        
        // ハードウェア情報 (Phase 1.5 では最小限)
        let hardware = HardwareInfo {
            model: None,
            serial: None,
            size_bytes: 0,  // 後で filesystem 情報から計算
        };
        
        // ファイルシステム情報
        let filesystem = gather_filesystem_info(volume)?;
        
        // 単一パスで集計
        let aggregate = aggregator::aggregate_all(volume)?;
        
        // 症状判定
        let symptom = symptom_detector::detect_symptom(
            &aggregate.file_stats,
            &aggregate.anomalies,
            aggregate.deleted_file_stats.is_some(),
        );
        
        let finished_at = Utc::now();
        let duration_secs = (finished_at - started_at).num_seconds().max(0) as u64;
        
        // パーティションサイズを計算
        let size_bytes = filesystem.total_clusters
            .saturating_mul(filesystem.cluster_size_bytes as u64);
        let hardware = HardwareInfo { size_bytes, ..hardware };
        
        Ok(DiagnosticReport {
            case_id,
            diagnosed_at: started_at,
            duration_secs,
            hardware,
            filesystem,
            symptom,
            file_stats: aggregate.file_stats,
            format_breakdown: aggregate.format_breakdown,
            folder_breakdown: aggregate.folder_breakdown,
            deleted_file_stats: aggregate.deleted_file_stats,
            anomalies: aggregate.anomalies,
        })
    }
}

fn gather_filesystem_info<F>(volume: &NtfsVolume<F>) -> Result<FilesystemInfo, DiagnosticError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // NtfsVolume が公開している情報を取得
    // - cluster_size (u32 bytes)
    // - total_clusters (u64)
    // - volume_serial_number (u64 or hex)
    //
    // 既存 API の正確な名前は実装時に確認 (Chunks 4-14 の NtfsVolume を参照)
    
    Ok(FilesystemInfo {
        fs_type: "NTFS".to_string(),
        volume_serial: Some(format!("{:08X}", volume.volume_serial_number())),
        cluster_size_bytes: volume.cluster_size_bytes(),
        total_clusters: volume.total_clusters(),
        used_clusters: 0,  // Phase 1.5 では正確な値が取れなければ 0、または推定値
    })
}
```

## 単体テスト要件 (最低 15 件)

### `aggregator.rs` (最低 5 件)

1. `aggregate_healthy_returns_no_deletions`: ntfs_healthy_small で deleted=0
2. `aggregate_with_deletions_counts_correctly`: ntfs_with_5_deletions_small で deleted=5
3. `aggregate_format_breakdown_groups_by_extension`: ntfs_mixed_formats で各形式が正しい件数
4. `aggregate_folder_breakdown_top_10_sorted`: ntfs_directories で 上位 10 フォルダ件数順
5. `aggregate_handles_empty_volume_gracefully`: 空ボリュームでもパニックしない
6. `extract_folder_handles_root_files`: パスから root / フォルダの抽出
7. `extract_folder_handles_root_slash`: "\file.txt" → "\"

### `symptom_detector.rs` (最低 4 件)

8. `detect_none_when_clean`: 異常なし、削除なし → Symptom::None
9. `detect_deleted_when_deleted_files_present`: 削除あり → Symptom::Deleted
10. `detect_filesystem_error_when_anomalies`: MFT 破損 → FilesystemError
11. `detect_formatted_when_very_few_files`: 全ファイル < 50 → Formatted
12. `detect_mixed_when_multiple_conditions`: FS 異常 + 削除 → Mixed
13. `detect_prioritizes_fs_error_over_deletion`: FS 異常が優先される

### `crm_text.rs` (最低 4 件)

14. `crm_text_contains_case_id`: 案件番号が含まれる
15. `crm_text_uses_japanese_symptom_label`: 「削除」「フォーマット」等が含まれる
16. `crm_text_includes_format_breakdown`: 形式別件数が含まれる
17. `crm_text_omits_deleted_section_when_no_deletions`: 削除なし時は当該セクションを省略
18. `crm_text_renders_size_in_human_readable_format`: バイト数が "5.2 MB" 等に整形

### `report.rs` / `lib.rs` (最低 2 件)

19. `to_diagnostic_input_preserves_symptom_and_stats`: DiagnosticReport → DiagnosticInput 変換
20. `diagnostic_engine_completes_within_1_minute_on_small_fixture`: 小フィクスチャで 1 分以内

## 結合テスト要件 (最低 3 件)

`crates/diagnostic/tests/diagnostic_integration.rs`:

### 1. 削除案件の診断

```rust
#[test]
fn diagnose_deleted_fixture_produces_deleted_symptom() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).unwrap();
    
    assert_eq!(report.symptom, Symptom::Deleted);
    assert_eq!(report.file_stats.deleted_files, 5);
    assert!(report.deleted_file_stats.is_some());
    assert_eq!(report.deleted_file_stats.as_ref().unwrap().total_count, 5);
}
```

### 2. 健康ディスクの診断

```rust
#[test]
fn diagnose_healthy_fixture_produces_none_symptom() {
    let img = decompress_fixture("ntfs_healthy_small");
    // ... setup ...
    let report = DiagnosticEngine::diagnose(&mut volume, CaseId::parse("260522-01").unwrap()).unwrap();
    
    assert_eq!(report.symptom, Symptom::None);
    assert_eq!(report.file_stats.deleted_files, 0);
    assert!(report.deleted_file_stats.is_none());
    assert!(!report.anomalies.has_any_anomaly());
}
```

### 3. プロダクトデモテスト

```rust
#[test]
fn product_demo_diagnose_with_crm_text() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).unwrap();
    
    let crm_text = report.to_crm_text();
    
    println!("\n=== Phase 1.5 Diagnostic Engine Demo (Chunk 22) ===\n");
    println!("案件: 260522-04");
    println!("診断時間: {} 秒", report.duration_secs);
    println!("主症状: {}", report.symptom.primary_label());
    println!();
    println!("--- CRM 貼り付けテキスト ---");
    println!("{}", crm_text);
    println!("--- ここまで ---");
    println!();
    println!("=== 診断エンジン完成 ===");
    
    // 基本検証
    assert!(crm_text.contains("260522-04"));
    assert!(crm_text.contains("削除"));
    assert!(crm_text.contains("【ファイル統計】"));
    assert!(report.duration_secs < 60);
}
```

### 4. case.json 統合テスト

```rust
#[test]
fn diagnose_result_can_be_saved_to_case() {
    use dds_case_manager::CaseStorage;
    use tempfile::TempDir;
    
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let mut case = storage.create_new(case_id.clone()).unwrap();
    
    // 診断実行
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    let cluster_size = parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes() as u64;
    let mut volume = NtfsVolume::open(make_image_reader(img, cluster_size)).unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id.clone()).unwrap();
    
    // case に反映
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(&case).unwrap();
    
    // 再読み込みで保持されている
    let reloaded = storage.load(&case_id).unwrap();
    assert_eq!(reloaded.diagnostic_input.filesystem_type, Some("NTFS".into()));
    assert_eq!(reloaded.diagnostic_input.symptom, Some(Symptom::Deleted));
    assert_eq!(reloaded.diagnostic_input.deleted_files, 5);
}
```

## 制約

- **行数目安**:
  - `aggregator.rs`: 130 行 + テスト 80 行
  - `symptom_detector.rs`: 60 行 + テスト 50 行
  - `crm_text.rs`: 180 行 + テスト 60 行
  - `report.rs`: 130 行
  - `lib.rs`: 90 行 + テスト 30 行
  - `error.rs`: 30 行
  - 合計: 約 620 行コード + 220 行テスト
- **単体テスト最低 15 件**
- **結合テスト最低 3 件 (うち 1 件は product_demo)**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持** (Phase 1 の 383+ + Chunk 21 の 24 件)
- **診断時間: 小フィクスチャで 1 秒以内** (NTFS 規模に応じて)

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-diagnostic` が全パス (≥18 件)
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_diagnose_with_crm_text` が pass + 出力が見える
- [ ] CRM 貼り付けテキストの実物 (フィクスチャ ntfs_with_5_deletions_small 由来) を確認
- [ ] `diagnose_result_can_be_saved_to_case` が pass (case-manager 統合)
- [ ] `grep -r 'unsafe' crates/diagnostic/src/` で 0 件

## 関連 FR 要件

- **FR-DIAG-01** (NTFS 論理診断) ← 達成
- **FR-DIAG-02** (症状自動判定) ← 達成
- **FR-DIAG-03** (削除ファイル統計) ← 達成
- **FR-DIAG-04** (CRM 貼り付けテキスト) ← 達成
- **FR-DIAG-05** (1 分以内) ← フィクスチャで実証、実機は Chunk 23-24 で

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 論理診断の自動化達成、業務的価値が顕在化**
4. 次のステップ:
   - **Chunk 22.5**: 削除ファイル復旧可能性推定 (高/中/低 ラベリング)
   - **Chunk 23**: 業務向け出力ディレクトリ構造

---

## 注意事項

### `dds-core` に `format_bytes` 関数の追加

Chunk 20.5 で `dds-report::format::format_bytes` を実装したが、diagnostic からも同じ関数が必要。

選択肢:
- **A**: `dds-core` に `format` モジュールを追加 (`dds_core::format::format_bytes`) ⭐推奨
- **B**: diagnostic 内で重複実装
- **C**: diagnostic → report の依存追加 (依存方向的に NG)

**A を採用**:
- `crates/core/src/format.rs` を新規作成
- `format_bytes(u64) -> String` を移動
- `crates/report/src/format.rs` から `dds_core::format::format_bytes` を re-export して既存 API 維持
- diagnostic からは `dds_core::format::format_bytes` を直接使用

これにより既存のテストは無変更で動作 (`dds_report::format_bytes` も `dds_report::format::format_bytes` も pub use で生きる)。

実装の流れ:
1. `crates/core/src/format.rs` 作成、`format_bytes` 実装
2. `crates/core/src/lib.rs` で `pub mod format;` 追加
3. `crates/report/src/format.rs` の `format_bytes` 中身を `dds_core::format::format_bytes` への delegate に変更
4. `crates/diagnostic/src/crm_text.rs` で `use dds_core::format::format_bytes;`

### NtfsVolume の API

`gather_filesystem_info` で使用する想定の API:
- `volume.cluster_size_bytes() -> u32`
- `volume.total_clusters() -> u64`
- `volume.volume_serial_number() -> u64` (8 桁 hex として表示)

これらが NtfsVolume にない場合、Chunks 4-14 で実装した `BootSector` 構造体経由でアクセス、または NtfsVolume に getter を追加。

`used_clusters` は NTFS の `$Bitmap` ファイルから取得すべきですが、Phase 1.5 では実装が重い場合は 0 (未計測) で OK。CRM テキスト上は「使用率: 未計測」と表示するなど。

### `NtfsFile::is_directory` の有無

`NtfsFile` に `is_directory` フィールド or メソッドがあるか未確認。なければ追加が必要 (1-2 行)。

実装場所:
- `crates/fs-ntfs/src/file.rs` (or 該当ファイル)
- MFT エントリの `FILE_NAME` または `STANDARD_INFORMATION` の `file_attributes` で判定
- `0x10000000` (FILE_ATTRIBUTE_DIRECTORY) ビットが立っていれば directory

### 単一パスの重要性

`volume.iter_files()` を 2 回呼ぶと、内部で MFT を再走査する可能性がある (実装次第)。

性能を優先するなら、aggregator.rs の 1 ループで全データを取る設計を遵守。複数ループにしないこと。

### フォーマット痕跡検出の限界

Phase 1 のフォーマット検出は「ファイル数が異常に少ない」というヒューリスティック。これは:

- 偽陽性: 空に近い HDD (新品など) でフォーマットと誤判定する可能性
- 偽陰性: フォーマット後にデータ書き戻しが進んだ場合に検出失敗

業務的には:
- 偽陽性: 「フォーマット案件と判定したが、実は新品」→ お客様に確認できる
- 偽陰性: 「フォーマットなのに検出されない」→ CS が経験で気づく

Phase 2 で MFT カービング機能を実装する際に、旧 MFT クラスタを発見できれば、より確実な判定が可能になる。

### エラー分類の精度

`classify_error` のエラーメッセージマッチは粗い。本番運用で:
- 「MFT」「entry」「record」を含む → MFT 破損カウント
- 「runlist」「data run」を含む → run-list カウント
- それ以外 → other_issues

`dds-fs-ntfs` の VolumeError バリアントを直接 match できれば理想的。Chunk 22 では文字列マッチで実装し、Phase 2 で構造化エラーにリファクタリング検討。

### CRM テキストの調整余地

サンプルテキストは私の提案。DDS の CRM 入力欄で使う実際の用語に合わせる必要があれば、`crm_text.rs::render` を調整。

特に「主症状」の文字列 (「削除」「フォーマット」「ファイルシステム異常」) は CRM 側で受け入れる用語と一致しているか確認すべき。

### Phase 1.5 で意図的に除外した機能

- **物理セクタ走査** (Phase 2 で実装)
- **MFT カービング** (フォーマット前ファイル復旧、Phase 2)
- **HDD ハードウェア情報自動取得** (Windows API 経由、Phase 2)
- **`$Bitmap` ベースの正確な used_clusters** (Phase 2 で正確化)
- **Boot sector の backup との詳細比較** (Phase 2)

---

## 質問が必要なケース

- CRM の症状用語が「削除」「フォーマット」「ファイルシステム異常」と一致しない場合
- ファイル数閾値 (フォーマット判定の 50 件) を業務的に変更したい場合
- `NtfsFile::is_directory` の実装が複雑な場合

---

## 完了報告例

```markdown
## Chunk 22 完了報告

### 新規ファイル
- crates/diagnostic/src/lib.rs              (90 行 + テスト 30 行)
- crates/diagnostic/src/error.rs             (30 行)
- crates/diagnostic/src/report.rs            (130 行)
- crates/diagnostic/src/aggregator.rs        (135 行 + テスト 85 行)
- crates/diagnostic/src/symptom_detector.rs  (65 行 + テスト 50 行)
- crates/diagnostic/src/crm_text.rs          (185 行 + テスト 60 行)
- crates/diagnostic/Cargo.toml
- crates/diagnostic/tests/diagnostic_integration.rs (180 行)

### 既存ファイル変更
- crates/core/src/format.rs (新規)
- crates/core/src/lib.rs    (format モジュール追加)
- crates/report/src/format.rs (dds_core への delegate に変更)
- crates/fs-ntfs/src/file.rs (is_directory メソッド追加、必要なら)

### 公開 API
- `DiagnosticEngine::diagnose(&mut volume, case_id) -> DiagnosticReport`
- `DiagnosticReport` (フル構造体)
- `DiagnosticReport::to_crm_text() -> String`
- `DiagnosticReport::to_diagnostic_input() -> DiagnosticInput`
- `HardwareInfo`, `FilesystemInfo`, `FileStatistics`, `FormatCount`, `FolderCount`, `FsAnomalyReport`

### テスト統計
- 単体: 既存 352 + 新規 19 = **371 件 pass**
- 結合: 既存 57 + 新規 4 = **61 件 pass**
- 全 workspace: **432+ 件 pass**

### 業務価値の見える化 (`product_demo_diagnose_with_crm_text`)

```
=== Phase 1.5 Diagnostic Engine Demo (Chunk 22) ===

案件: 260522-04
診断時間: 0 秒
主症状: 削除

--- CRM 貼り付けテキスト ---
=== 論理診断結果 (案件 260522-04) ===
診断日時: 2026-05-23 14:30
診断時間: 0 秒
※物理診断は別途実施済み

【ハードウェア】
容量: 4.78 MB

【ファイルシステム】
種類: NTFS
ボリュームシリアル: A1B2C3D4
クラスタサイズ: 4096 bytes
使用率: 0 B / 4.78 MB (0.0%)

【症状判定】
主症状: 削除
- ファイルシステム署名: 正常
- MFT 構造: 正常
- フォーマット痕跡: なし
  ※削除エントリ検出 (件数は下記「削除ファイル」参照)

【ファイル統計】
全ファイル: 30 件 (1.46 KB)
  - 通常 (生存): 25 件
  - 削除済み: 5 件
ディレクトリ: 0 件

【削除ファイルの内訳】
形式別:
  TXT: 5 件

フォルダ別:
  \: 5 件
推定合計サイズ: 250 B

【生存ファイル統計】(参考、主要形式)
  TXT: 25 件 / 1.22 KB

【主なフォルダ】(上位 10)
  \: 30 件 / 1.46 KB

【ファイルシステムの破損】
MFT エントリ破損: 0 件
不正な run-list: 0 件
Boot sector: 正常

【物理不良チェック】
未実施 (Phase 2 で対応予定)

=== 診断完了 ===
--- ここまで ---

=== 診断エンジン完成 ===
```

### 🎉 マイルストーン
- **論理診断の自動化達成**
- CRM 貼り付けテキストが業務適用可能な形に到達
- フィクスチャでの診断時間 = 数ミリ秒 (実機でも 1 分以内の見込み)
- Phase 1.5 の最重要機能完成

- **関連 FR**: FR-DIAG-01〜05 (達成)

→ tester エージェントへ引き継ぎお願いします
```
