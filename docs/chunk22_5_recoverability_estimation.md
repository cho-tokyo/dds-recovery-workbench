# Chunk 22.5 指示: 削除ファイルの復旧可能性推定（High/Medium/Low）

削除案件の見積もり精度を上げる機能。各削除エントリについて「**確実復旧可能 / 部分復旧 / メタデータのみ**」を判定し、CRM 貼り付けテキストに反映します。

> 🎯 完了時点で「削除エントリ 234 件中、確実復旧可能 198 件、部分復旧 30 件、メタのみ 6 件」のような具体的な見積もりが自動生成される。お客様への提示時に大きな安心感を与えられる。

---

## 背景: 業務的な価値

### 現状 (Chunk 22.6 まで) の限界

```
お客様への見積もり:
  「削除エントリ 234 件検出」
  ↓
  CS: 「復旧できるかは実際にやってみないと分かりません」
  お客様: 「不安です...」
```

### Chunk 22.5 完了後

```
お客様への見積もり:
  「削除エントリ 234 件
   うち復旧可能性 高 198 件、中 30 件、低 6 件」
  ↓
  CS: 「198 件は確実に復旧、合計 228 件の復旧が見込まれます」
  お客様: 「具体的な数字で安心」
```

月 800 件のうち約 30% が削除案件と仮定すると、月 240 件で「見積もり精度の向上」効果が出ます。

## 目的

削除ファイルの復旧可能性推定機能を実装する:

1. **判定アルゴリズム**: NTFS の技術的事実から復旧可能性を推定
2. **クラスタ占有マップ**: 生存ファイルが占有しているクラスタを記録
3. **resident attribute 判定**: 小さいファイル (MFT 内データ完結) を区別
4. **CRM テキストへの統合**: 「復旧可能性 (推定)」セクション追加
5. **既存テストへの統合**: ntfs_with_5_deletions_small で動作確認

## 対象クレート

- **主**: `crates/diagnostic/` (Chunk 22 で実装、本チャンクで拡張)
- **副**: `crates/fs-ntfs/` (NtfsFile に `is_resident()` 等のメソッド追加が必要)

## 判定基準

### High (確実復旧可能)

以下のいずれか:

1. **resident attribute**: ファイルデータが MFT エントリ内に格納されている (NTFS の小さいファイル、~600 bytes 程度まで)
   - MFT エントリさえ読めれば 100% 復旧可能
2. **non-resident + run-list 完全 + クラスタ完全保持**: 
   - run-list が正常に解析でき、占有していたクラスタが他のファイルに割り当てられていない
   - 物理クラスタからデータを読み戻せば復旧可能

### Medium (部分復旧の可能性)

- **non-resident + run-list 完全 + 部分上書き**:
  - run-list は読める
  - 占有クラスタの一部 (1 クラスタ以上) が現在の生存ファイルに割り当てられている
  - 上書きされたクラスタのデータは復旧不能、未上書きクラスタは復旧可能
  - 結果としてファイルは部分的に復旧 (内容の信頼性は低い)

### Low (メタデータのみ)

以下のいずれか:

1. **run-list 破損**: MFT エントリは読めるが、$DATA attribute の run-list 解析に失敗
2. **全クラスタ上書き**: run-list は読めるが、占有していた全クラスタが他のファイルに割り当てられている
3. **MFT エントリ破損**: ファイル名・サイズなどメタデータは取得できるが、データ位置情報が取れない

→ ファイル名やサイズの「存在情報」は分かるが、内容は復旧できない

## 重要な設計原則

### NTFS の技術的事実に基づく

ヒューリスティックではなく、NTFS 仕様に基づいた判定:

- **resident vs non-resident**: $DATA attribute のフラグで判定
- **run-list 完全性**: 解析時のエラーの有無
- **クラスタ占有**: 生存ファイル群のクラスタ範囲との集合演算

### 単一パスでの判定

aggregator が MFT を 1 回走査する中で、以下を同時に収集:

1. 生存ファイル: 占有クラスタ範囲を記録 → 占有マップ構築
2. 削除ファイル: run-list 情報 + resident フラグを保存

走査完了後、削除ファイルそれぞれを占有マップと照合して判定。

### Phase 1 の現実的なスコープ

- ✅ resident attribute 検出
- ✅ run-list 完全性チェック
- ✅ 生存ファイルとの上書き判定
- ❌ **削除ファイル間の上書き判定はしない** (現実的に不可能、削除時点のクラスタ占有履歴はない)
- ❌ ジャーナル ($LogFile) からの履歴解析 (Phase 2 以降の高度機能)

## 仕様参照

### ビジネス要件

- **FR-DIAG-07**: 削除ファイルの復旧可能性推定 ← 新規
- **FR-DIAG-08**: 業務見積もりへの活用 ← 新規

### 既存実装の参照

- `dds-fs-ntfs::NtfsFile`: ファイル情報 (Chunks 4-14)
- `dds-case-manager::RecoverabilityEstimate`: 既に Chunk 21 で定義済み

## 実装内容

### Part A: NtfsFile への機能追加

`crates/fs-ntfs/src/file.rs` (または該当ファイル) に以下を追加:

```rust
impl NtfsFile {
    /// $DATA attribute が resident (MFT エントリ内にデータ格納) か判定。
    /// resident なファイルは MFT エントリさえ読めれば完全復旧可能。
    pub fn is_resident(&self) -> bool {
        // $DATA attribute の non_resident_flag を確認
        // (実装は既存の attribute 解析コードから取得)
        // 該当 attribute がない場合は false (例: ディレクトリ)
        ...
    }
    
    /// 占有しているクラスタの範囲リスト。
    /// non-resident な $DATA attribute から取得。
    /// resident な場合 or $DATA がない場合は空 Vec。
    /// run-list 解析失敗の場合は Err。
    pub fn occupied_cluster_ranges(&self) -> Result<Vec<ClusterRange>, VolumeError> {
        // 既存の run-list 解析ロジックを利用
        // 既に read_data() の実装内で run-list 処理しているはず
        // それを抽出してメソッド化
        ...
    }
}

/// クラスタ範囲を表す
#[derive(Debug, Clone, Copy)]
pub struct ClusterRange {
    pub start_lcn: u64,   // 開始 Logical Cluster Number
    pub length: u64,       // クラスタ数
}

impl ClusterRange {
    pub fn end_lcn(&self) -> u64 {
        self.start_lcn + self.length
    }
    
    pub fn contains(&self, lcn: u64) -> bool {
        lcn >= self.start_lcn && lcn < self.end_lcn()
    }
}
```

**実装の参考**:
- `read_data()` 関数内で run-list を走査している部分を流用
- run-list 解析失敗は VolumeError として返却
- resident vs non-resident は MFT entry header の attribute header の `non_resident_flag` (1 byte) で判定

### Part B: aggregator.rs の拡張

`crates/diagnostic/src/aggregator.rs` を拡張:

```rust
use std::collections::BTreeMap;
use roaring::RoaringTreemap;  // クラスタ占有マップ用 (オプション)

/// 単一パスでの集計結果 (拡張版)
pub struct AggregateResult {
    pub file_stats: FileStatistics,
    pub format_breakdown: BTreeMap<String, FormatCount>,
    pub folder_breakdown: Vec<FolderCount>,
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub anomalies: FsAnomalyReport,
    
    // 新規: 復旧可能性推定用の中間データ
    pub cluster_occupancy: ClusterOccupancyMap,
    pub deleted_file_metadata: Vec<DeletedFileMetadata>,
}

/// 生存ファイル群のクラスタ占有マップ
///
/// 削除ファイルが占有していたクラスタが、現在生存ファイルに割り当てられているかを
/// 判定するために使う。
pub struct ClusterOccupancyMap {
    /// 占有されているクラスタの集合 (BTreeSet で実装、Phase 2 で Bitmap 化検討)
    occupied: std::collections::BTreeSet<u64>,
}

impl ClusterOccupancyMap {
    pub fn new() -> Self {
        Self { occupied: Default::default() }
    }
    
    /// クラスタ範囲を「占有済み」として登録
    pub fn mark_range(&mut self, start: u64, length: u64) {
        for lcn in start..start + length {
            self.occupied.insert(lcn);
        }
    }
    
    /// 指定クラスタが占有されているか
    pub fn is_occupied(&self, lcn: u64) -> bool {
        self.occupied.contains(&lcn)
    }
    
    /// 範囲のうち占有されているクラスタ数をカウント
    pub fn count_overlapping(&self, start: u64, length: u64) -> u64 {
        (start..start + length).filter(|lcn| self.occupied.contains(lcn)).count() as u64
    }
}

/// 削除ファイルの判定用メタデータ
pub struct DeletedFileMetadata {
    pub file_id: u64,  // MFT entry index
    pub is_resident: bool,
    pub run_list_valid: bool,
    pub cluster_ranges: Vec<ClusterRange>,  // 占有していたクラスタ範囲
}

pub fn aggregate_all<F>(volume: &mut NtfsVolume<F>) -> Result<AggregateResult, DiagnosticError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let mut file_stats = FileStatistics::default();
    let mut format_breakdown: BTreeMap<String, FormatCount> = BTreeMap::new();
    let mut all_folders: HashMap<String, (usize, u64)> = HashMap::new();
    let mut deleted_by_ext: BTreeMap<String, usize> = BTreeMap::new();
    let mut deleted_by_folder: HashMap<String, usize> = HashMap::new();
    let mut deleted_total_size: u64 = 0;
    let mut deleted_count: usize = 0;
    let mut anomalies = FsAnomalyReport::default();
    
    // 新規: 復旧可能性推定用
    let mut cluster_occupancy = ClusterOccupancyMap::new();
    let mut deleted_metadata: Vec<DeletedFileMetadata> = Vec::new();
    
    for result in volume.iter_files() {
        match result {
            Ok(file) => {
                if !file.is_user_file() {
                    continue;
                }
                
                // 既存の統計集計 (Chunk 22 のコードのまま)
                file_stats.total_files += 1;
                file_stats.total_size_bytes = file_stats.total_size_bytes.saturating_add(file.size);
                
                if file.is_deleted {
                    file_stats.deleted_files += 1;
                } else {
                    file_stats.live_files += 1;
                }
                
                if file.is_directory {
                    file_stats.directories += 1;
                    continue;
                }
                
                // ... (既存の format / folder / deleted 集計コード) ...
                
                // 新規: 復旧可能性のための追加処理
                if file.is_deleted {
                    // 削除ファイルのメタデータを保存
                    let is_resident = file.is_resident();
                    let (cluster_ranges, run_list_valid) = match file.occupied_cluster_ranges() {
                        Ok(ranges) => (ranges, true),
                        Err(_) => (Vec::new(), false),
                    };
                    
                    deleted_metadata.push(DeletedFileMetadata {
                        file_id: file.entry_index,  // または file.id()
                        is_resident,
                        run_list_valid,
                        cluster_ranges,
                    });
                } else {
                    // 生存ファイルのクラスタを占有マップに登録
                    if !file.is_resident() {
                        if let Ok(ranges) = file.occupied_cluster_ranges() {
                            for range in ranges {
                                cluster_occupancy.mark_range(range.start_lcn, range.length);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                classify_error(&e, &mut anomalies);
            }
        }
    }
    
    // 既存の Top-N folder 構築コード
    // ...
    
    // 既存の deleted_file_stats 構築コード
    // ※ recoverability_estimate は Part C の recoverability::estimate() で後から埋める
    
    Ok(AggregateResult {
        file_stats,
        format_breakdown,
        folder_breakdown: folder_vec,
        deleted_file_stats,
        anomalies,
        cluster_occupancy,
        deleted_file_metadata: deleted_metadata,
    })
}
```

### Part C: recoverability.rs (新規ファイル)

`crates/diagnostic/src/recoverability.rs` を新規作成:

```rust
//! 削除ファイルの復旧可能性推定。
//!
//! NTFS の技術的事実 (resident attribute, run-list の完全性,
//! クラスタの上書き状態) に基づいて、各削除エントリを
//! High / Medium / Low に分類する。

use dds_case_manager::RecoverabilityEstimate;

use crate::aggregator::{ClusterOccupancyMap, DeletedFileMetadata};

/// 削除ファイル群の復旧可能性を推定する。
///
/// 判定基準:
/// - High: resident attribute OR (run-list 完全 + 全クラスタ未上書き)
/// - Medium: run-list 完全 + 部分上書き (1 クラスタ以上残存)
/// - Low: run-list 破損 OR 全クラスタ上書き
pub fn estimate(
    deleted_files: &[DeletedFileMetadata],
    occupancy: &ClusterOccupancyMap,
) -> RecoverabilityEstimate {
    let mut high = 0usize;
    let mut medium = 0usize;
    let mut low = 0usize;
    
    for file in deleted_files {
        let category = categorize(file, occupancy);
        match category {
            Category::High => high += 1,
            Category::Medium => medium += 1,
            Category::Low => low += 1,
        }
    }
    
    RecoverabilityEstimate {
        high_confidence: high,
        medium_confidence: medium,
        low_confidence: low,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    High,
    Medium,
    Low,
}

fn categorize(file: &DeletedFileMetadata, occupancy: &ClusterOccupancyMap) -> Category {
    // resident: MFT 内データ完結、ほぼ確実に復旧可能
    if file.is_resident {
        return Category::High;
    }
    
    // run-list 破損: メタデータのみ
    if !file.run_list_valid {
        return Category::Low;
    }
    
    // run-list 完全 + クラスタ占有状態をチェック
    let total_clusters: u64 = file.cluster_ranges.iter().map(|r| r.length).sum();
    
    if total_clusters == 0 {
        // 占有クラスタなし (0 バイトファイル等)、メタのみ存在
        return Category::High;  // 0 バイトでも復旧扱い (ファイル存在情報あり)
    }
    
    let overwritten: u64 = file.cluster_ranges.iter()
        .map(|r| occupancy.count_overlapping(r.start_lcn, r.length))
        .sum();
    
    if overwritten == 0 {
        Category::High
    } else if overwritten >= total_clusters {
        Category::Low
    } else {
        Category::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator::ClusterRange;
    
    fn make_metadata(is_resident: bool, run_list_valid: bool, ranges: Vec<(u64, u64)>) -> DeletedFileMetadata {
        DeletedFileMetadata {
            file_id: 0,
            is_resident,
            run_list_valid,
            cluster_ranges: ranges.into_iter()
                .map(|(start, length)| ClusterRange { start_lcn: start, length })
                .collect(),
        }
    }
    
    #[test]
    fn resident_file_is_high() {
        let files = vec![make_metadata(true, true, vec![])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
    }
    
    #[test]
    fn invalid_runlist_is_low() {
        let files = vec![make_metadata(false, false, vec![])];
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.low_confidence, 1);
    }
    
    #[test]
    fn non_overwritten_clusters_is_high() {
        let files = vec![make_metadata(false, true, vec![(100, 5)])];
        let occupancy = ClusterOccupancyMap::new();  // 何も占有されていない
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
    }
    
    #[test]
    fn fully_overwritten_clusters_is_low() {
        let files = vec![make_metadata(false, true, vec![(100, 5)])];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(100, 5);  // 全クラスタが他で占有されている
        let est = estimate(&files, &occupancy);
        assert_eq!(est.low_confidence, 1);
    }
    
    #[test]
    fn partially_overwritten_clusters_is_medium() {
        let files = vec![make_metadata(false, true, vec![(100, 10)])];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(105, 3);  // 10 クラスタのうち 3 が上書き
        let est = estimate(&files, &occupancy);
        assert_eq!(est.medium_confidence, 1);
    }
    
    #[test]
    fn zero_byte_file_is_high() {
        let files = vec![make_metadata(false, true, vec![])];  // クラスタ占有なし
        let occupancy = ClusterOccupancyMap::new();
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 1);
    }
    
    #[test]
    fn mixed_categories_counted_separately() {
        let files = vec![
            make_metadata(true, true, vec![]),                  // High (resident)
            make_metadata(false, false, vec![]),                 // Low (run-list 破損)
            make_metadata(false, true, vec![(200, 5)]),         // High (未上書き)
            make_metadata(false, true, vec![(300, 5)]),         // Medium (部分上書き予定)
        ];
        let mut occupancy = ClusterOccupancyMap::new();
        occupancy.mark_range(302, 2);  // 4 番目のファイルの一部を占有
        
        let est = estimate(&files, &occupancy);
        assert_eq!(est.high_confidence, 2);
        assert_eq!(est.medium_confidence, 1);
        assert_eq!(est.low_confidence, 1);
    }
}
```

### Part D: lib.rs の修正

`crates/diagnostic/src/lib.rs`:

```rust
pub mod aggregator;
pub mod crm_text;
pub mod error;
pub mod recoverability;  // 新規
pub mod report;

pub use error::DiagnosticError;
pub use report::{
    DiagnosticReport, FileStatistics, FilesystemInfo, FolderCount, FormatCount,
    FsAnomalyReport, HardwareInfo,
};
pub use aggregator::{ClusterOccupancyMap, ClusterRange, DeletedFileMetadata};  // 必要に応じて

impl DiagnosticEngine {
    pub fn diagnose<F>(
        volume: &mut NtfsVolume<F>,
        case_id: CaseId,
    ) -> Result<DiagnosticReport, DiagnosticError>
    where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        // ... (既存コード) ...
        
        let aggregate = aggregator::aggregate_all(volume)?;
        let filesystem_findings = aggregate.anomalies.to_findings();
        
        // 新規: 復旧可能性推定
        let recoverability = recoverability::estimate(
            &aggregate.deleted_file_metadata,
            &aggregate.cluster_occupancy,
        );
        
        // deleted_file_stats に推定結果を追加
        let mut deleted_file_stats = aggregate.deleted_file_stats;
        if let Some(stats) = &mut deleted_file_stats {
            stats.recoverability_estimate = Some(recoverability);
        }
        
        // ... (既存コード) ...
        
        Ok(DiagnosticReport {
            // ... 
            deleted_file_stats,
            // ...
        })
    }
}
```

### Part E: crm_text.rs の修正

`crates/diagnostic/src/crm_text.rs` の「削除エントリの詳細」セクションを拡張:

```rust
// 「削除エントリの詳細」セクション内に追加
if let Some(deleted) = &report.deleted_file_stats {
    // ... 既存の形式別 / フォルダ別 / 推定合計サイズ ...
    
    // 新規: 復旧可能性 (推定)
    if let Some(est) = &deleted.recoverability_estimate {
        let _ = writeln!(s);
        let _ = writeln!(s, "復旧可能性 (推定):");
        let _ = writeln!(s, "  高 (確実復旧可能): {} 件", est.high_confidence);
        let _ = writeln!(s, "  中 (部分復旧の可能性): {} 件", est.medium_confidence);
        let _ = writeln!(s, "  低 (メタデータのみ): {} 件", est.low_confidence);
        let _ = writeln!(s, "  ※ 判定基準:");
        let _ = writeln!(s, "    高: ファイル内容が MFT 内に完結、または占有クラスタが上書きされていない");
        let _ = writeln!(s, "    中: 占有クラスタの一部が他のファイルで上書きされている");
        let _ = writeln!(s, "    低: run-list 解析失敗、または全クラスタが上書き済み");
    }
    let _ = writeln!(s);
}
```

## 単体テスト要件 (最低 10 件)

### `recoverability.rs` (最低 7 件)

1. `resident_file_is_high`
2. `invalid_runlist_is_low`
3. `non_overwritten_clusters_is_high`
4. `fully_overwritten_clusters_is_low`
5. `partially_overwritten_clusters_is_medium`
6. `zero_byte_file_is_high`
7. `mixed_categories_counted_separately`

### `aggregator.rs` (最低 2 件)

8. `aggregator_collects_cluster_occupancy_from_live_files`: 生存ファイルのクラスタが占有マップに記録される
9. `aggregator_collects_deleted_file_metadata`: 削除ファイルのメタデータ (resident, run-list, ranges) が収集される

### `crm_text.rs` (最低 1 件)

10. `crm_text_includes_recoverability_section_when_estimate_present`: 推定結果ありの場合「復旧可能性 (推定)」セクションが含まれる
11. `crm_text_omits_recoverability_when_no_deletions`: 削除なし時はセクション省略

### `ClusterOccupancyMap` (最低 2 件)

12. `occupancy_mark_and_check_range`: 範囲を mark してから contains 確認
13. `occupancy_count_overlapping_partial`: 部分重なりのカウント

## 結合テスト要件 (最低 2 件)

### 1. 削除フィクスチャでの復旧可能性推定

`crates/diagnostic/tests/recoverability_integration.rs`:

```rust
#[test]
fn diagnose_5_deletions_estimates_recoverability() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    // ... setup ...
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).unwrap();
    
    let deleted_stats = report.deleted_file_stats.as_ref().unwrap();
    let est = deleted_stats.recoverability_estimate.as_ref()
        .expect("Recoverability estimate should be present");
    
    // 5 件すべて何らかのカテゴリに分類されている
    assert_eq!(est.high_confidence + est.medium_confidence + est.low_confidence, 5);
    
    // 小さい TXT ファイルなのですべて resident または非上書きで High と推定される想定
    // (フィクスチャの実装次第で調整)
}
```

### 2. プロダクトデモ (更新)

```rust
#[test]
fn product_demo_diagnose_with_recoverability_estimate() {
    let img = decompress_fixture("ntfs_with_5_deletions_small");
    // ... setup ...
    
    let case_id = CaseId::parse("260522-04").unwrap();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id).unwrap();
    
    let crm_text = report.to_crm_text();
    
    println!("\n=== Phase 1.5 Recoverability Estimate Demo (Chunk 22.5) ===\n");
    println!("案件: 260522-04");
    println!();
    
    if let Some(deleted) = &report.deleted_file_stats {
        if let Some(est) = &deleted.recoverability_estimate {
            println!("削除エントリ数: {}", deleted.total_count);
            println!("復旧可能性:");
            println!("  高: {} 件", est.high_confidence);
            println!("  中: {} 件", est.medium_confidence);
            println!("  低: {} 件", est.low_confidence);
        }
    }
    println!();
    println!("--- CRM 貼り付けテキスト (抜粋) ---");
    // 「復旧可能性 (推定)」セクション以降を表示
    if let Some(idx) = crm_text.find("復旧可能性 (推定)") {
        println!("{}", &crm_text[idx..idx.saturating_add(500)]);
    }
    println!("--- ここまで ---");
    println!();
    println!("=== 復旧可能性推定機能完成 ===");
    
    assert!(crm_text.contains("復旧可能性 (推定)"));
    assert!(crm_text.contains("高 (確実復旧可能)"));
}
```

## 制約

- **行数目安**:
  - `recoverability.rs`: 80 行 + テスト 100 行
  - `aggregator.rs` 拡張: 60 行 + テスト 40 行
  - `crm_text.rs` 拡張: 25 行 + テスト 30 行
  - `fs-ntfs/file.rs` 拡張: 30-50 行 (既存実装次第)
  - 合計: 約 230 行追加 + 170 行テスト
- **単体テスト最低 10 件 (新規)**
- **結合テスト最低 2 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持**

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-diagnostic` が全パス (新規 ≥10 件)
- [ ] `cargo test -p dds-fs-ntfs` が全パス (NtfsFile 拡張後)
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_diagnose_with_recoverability_estimate` が pass + 出力が見える
- [ ] CRM 貼り付けテキストに「復旧可能性 (推定)」セクションが含まれる
- [ ] ntfs_with_5_deletions_small で 5 件の判定合計が deleted_count と一致
- [ ] `grep -r 'unsafe' crates/diagnostic/src/` で 0 件

## 関連 FR 要件

- **FR-DIAG-07** (削除ファイル復旧可能性推定) ← 達成
- **FR-DIAG-08** (業務見積もりへの活用) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 削除案件の見積もり精度向上**
4. 次のステップ:
   - **Chunk 23**: 業務向け出力ディレクトリ構造 (Phase 1.5 最終)

---

## 注意事項

### NtfsFile の API 確認

実装前に NtfsFile が以下の情報を提供しているか確認:

```rust
// 必要な情報:
- file.is_deleted: bool                      ← 既存 (Chunks 5, 14)
- file.is_directory: bool                    ← Chunk 22 で追加済み or 確認
- file.is_resident: bool                     ← 新規追加 (本チャンク)
- file.occupied_cluster_ranges(): Result<...> ← 新規追加 (本チャンク)
- file.entry_index: u64 (or .id())            ← 既存 (Chunks 11, 14)
```

`is_resident()` と `occupied_cluster_ranges()` は既存の `read_data()` 内のロジックを抽出して新メソッドにする。

実装の参考:
- `read_data` 内で attribute header を解析している箇所
- `non_resident_flag` (0x01 byte at offset 8 of attribute header)
- run-list は `non_resident_flag == 1` の場合のみ存在

### resident attribute のサイズ目安

NTFS の resident な $DATA attribute は最大約 700 bytes 程度 (cluster size による):

- 4 KB クラスタ: ~600-700 bytes が resident 限界
- 8 KB クラスタ: ~1500 bytes が resident 限界

ntfs_with_5_deletions_small のフィクスチャのファイルサイズ (50 バイト程度の TXT) は確実に resident。
→ 5 件すべて High と判定される想定。

### クラスタ占有マップのメモリ効率

`BTreeSet<u64>` で実装:
- 1 クラスタあたり ~40 bytes (BTreeSet のノードオーバーヘッド)
- 1TB HDD (~250M クラスタ) で 10GB 程度 → 非現実的

ただし、Phase 1.5 では:
- フィクスチャは ~5MB 程度
- 5MB / 4KB = ~1300 クラスタ
- 実用範囲

Phase 2 で大規模 HDD 対応する際は:
- `RoaringTreemap` クレートで圧縮 (約 100KB-10MB に圧縮)
- またはビットマップ (`Vec<u64>` で 1 ビット = 1 クラスタ、~30MB for 1TB)

本チャンクでは BTreeSet で OK。

### Phase 1.5 で意図的に除外した機能

- **削除ファイル間の上書き判定**: 「削除ファイル A のクラスタが、後で削除ファイル B が使った」というケース。技術的に削除時系列が分からないため不可能。
- **$LogFile からの履歴解析**: ジャーナルログを読めば過去のクラスタ占有履歴が分かるが、複雑な実装。Phase 2 以降。
- **読み戻し検証**: 「High と判定したファイルが実際に正常に読めるか」のサンプル検証。Phase 2 で復旧プレビュー機能と一緒に。

### 業務的な判定基準の妥当性

| 判定 | 業務的な意味 | 復旧率の目安 |
|---|---|---|
| High | 確実に復旧、内容も信頼できる | 95-100% |
| Medium | 部分復旧、内容の一部欠損可能性 | 30-80% |
| Low | ファイル名のみ復旧、内容は不可 | 0-10% |

実機でのキャリブレーション (推定値と実復旧率の比較) は Phase 2 で実施。
Phase 1.5 では理論値で運用。

### CRM 貼り付け時の追加考慮

CRM の「主訴: 削除」の入力欄に貼り付ける際、CS は以下のように使う想定:

```
[CRM 入力例]
主訴: 削除
診断結果サマリ:
  削除エントリ 5 件 (合計 430 B)
  復旧可能性: 高 5 件 / 中 0 件 / 低 0 件
  → 5 件全件復旧見込み、業務的に納品可能
```

「高 5 件」のような数字が CRM に残ることで、後日の振り返り (見積もりと実復旧の差) が可能になる。

### Phase 2 への引き継ぎ

Phase 2.1 (UI) で実装する画面イメージ:

```
[診断結果画面 (UI)]

削除エントリ: 234 件

復旧可能性:
  ┌────────────────────────────────┐
  │ ●●●●●●●●●●●●●●●●●  ●●●  │  198 高 / 30 中 / 6 低
  └────────────────────────────────┘
  
  [▼ 詳細表示]
  
  個別ファイル一覧:
  ・ファイル A (High): \Users\Chou\file1.txt
  ・ファイル B (Medium): \Users\Chou\file2.docx (50% 上書き)
  ・ファイル C (Low): \Users\Chou\file3.xlsx (run-list 破損)
  ...
```

UI 表示時に「個別ファイルの判定理由」が必要なら、`DeletedFileMetadata` を `DiagnosticReport` 内に保持する設計に拡張可能。

Phase 1.5 では集計値のみ。

---

## 質問が必要なケース

- NtfsFile の既存実装で resident / run-list を分離するのが困難な場合
- 大量のクラスタ範囲で BTreeSet のパフォーマンスが問題になる場合 (実機で確認)
- 「High」の閾値を「95% 未上書き」など緩めたい業務要望

---

## 完了報告例

```markdown
## Chunk 22.5 完了報告

### 新規ファイル
- crates/diagnostic/src/recoverability.rs (80 行 + テスト 100 行)

### 修正ファイル
- crates/diagnostic/src/aggregator.rs (クラスタ占有マップ + メタデータ収集追加、~60 行)
- crates/diagnostic/src/lib.rs (recoverability 統合、~15 行)
- crates/diagnostic/src/crm_text.rs (復旧可能性セクション追加、~25 行)
- crates/fs-ntfs/src/file.rs (is_resident, occupied_cluster_ranges 追加、~40 行)

### 公開 API
- `recoverability::estimate(&[DeletedFileMetadata], &ClusterOccupancyMap) -> RecoverabilityEstimate`
- `ClusterOccupancyMap` (new, mark_range, is_occupied, count_overlapping)
- `ClusterRange` struct
- `DeletedFileMetadata` struct
- `NtfsFile::is_resident() -> bool`
- `NtfsFile::occupied_cluster_ranges() -> Result<Vec<ClusterRange>, VolumeError>`

### テスト統計
- 単体: 既存 + 新規 ~13 件 = **443+ 件 pass**
- 結合: 既存 + 新規 2 件 = **62+ 件 pass**
- 全 workspace: **445+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- recoverability 判定が論理的に検証済み

### 業務価値の見える化 (product_demo_diagnose_with_recoverability_estimate)
```
=== Phase 1.5 Recoverability Estimate Demo (Chunk 22.5) ===

案件: 260522-04

削除エントリ数: 5
復旧可能性:
  高: 5 件
  中: 0 件
  低: 0 件

--- CRM 貼り付けテキスト (抜粋) ---
復旧可能性 (推定):
  高 (確実復旧可能): 5 件
  中 (部分復旧の可能性): 0 件
  低 (メタデータのみ): 0 件
  ※ 判定基準:
    高: ファイル内容が MFT 内に完結、または占有クラスタが上書きされていない
    中: 占有クラスタの一部が他のファイルで上書きされている
    低: run-list 解析失敗、または全クラスタが上書き済み
--- ここまで ---

=== 復旧可能性推定機能完成 ===
```

### 🎉 マイルストーン
- **削除案件の見積もり精度が大幅向上**
- お客様への提示時に「N 件中 M 件は確実復旧」と具体的に伝達可能
- NTFS 技術的事実に基づく判定 (resident attribute、run-list、クラスタ占有)
- 業務見積もり → 実復旧結果の振り返りデータ蓄積が可能

- **関連 FR**: FR-DIAG-07、FR-DIAG-08 (達成)

→ tester エージェントへ引き継ぎお願いします
```
