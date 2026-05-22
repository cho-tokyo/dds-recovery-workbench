# Chunk 22.6 指示: 診断レポートの業務適用化（症状判定削除 + 構造再編）

Chunk 22 完了後の業務フィードバックを反映:
1. **症状判定セクションを完全削除** (Workbench は「事実」のみ報告、「判定」は CRM/CS の責務)
2. **「ファイルシステムの破損」を上位に移動** (ファイル統計より前に表示)
3. **「MFT エントリ数」を事実として明示** (フォーマット案件の参考情報)

> 🎯 完了時点で CRM 貼り付けテキストが業務フローと完全整合。誤判定 (例: 「フォーマット (複合)」を削除案件で出してしまう) が解消。

---

## 背景: 業務フィードバックと現状の問題

Chunk 22 完了時の実出力で判明した問題:

```
[フィクスチャ ntfs_with_5_deletions_small (削除案件用) の診断出力]

【症状判定】
主症状: フォーマット (複合)
- 複合症状:
  ・フォーマット
  ・削除
```

これは **業務的に誤った判定** です:
- フィクスチャはあくまで削除案件のテストデータ
- ヒューリスティック「全ファイル数 < 50」がフィクスチャの小ささに反応
- 結果: 偽陽性で「フォーマット (複合)」と判定

### 業務的な原因

Workbench に「症状判定」の責務を持たせるのが業務フローに反していた:

```
[業務の実態]
1. ヒアリング (CS / お客様) → 主訴確定 ("削除" "フォーマット" "FS 異常")
2. 物理診断 (別ツール)
3. 論理診断 (Workbench)
   ← 主訴は既に判明
   役割: 「事実 (件数・破損状態) を報告」のみ
   判定: 不要 (既に CS が決めている)
```

Workbench が「判定」してしまうと、CS のヒアリング内容と食い違って混乱を招く。

## 目的

3 つの統合された改善:

### A. 症状判定の完全削除

- `dds-case-manager`: `Symptom` enum / `FsAnomaly` enum / `DiagnosticInput.symptom` フィールドを削除
- `dds-diagnostic`: `symptom_detector.rs` を削除、`DiagnosticReport.symptom` フィールドを削除

### B. FilesystemFindings 構造体の導入

「ファイルシステムの破損」を構造化された事実として表現:

```rust
pub struct FilesystemFindings {
    pub signature_valid: bool,
    pub mft_corrupted_count: usize,
    pub invalid_runlist_count: usize,
    pub boot_sector_ok: bool,
    pub other_issues: Vec<String>,
}
```

### C. CRM 貼り付けテキストの構造変更

```
旧構造:                          新構造:
【ハードウェア】                  【ハードウェア】
【ファイルシステム】              【ファイルシステム】
【症状判定】          ← 削除      【ファイルシステムの破損】 ← 上に移動
【ファイル統計】                  【MFT エントリ統計】       ← 新規 (フォーマット案件参考)
【削除ファイルの内訳】            【ファイル統計】
【生存ファイル統計】              【削除エントリの詳細】 (削除あれば)
【主なフォルダ】                  【生存ファイル統計】
【ファイルシステムの破損】        【主なフォルダ】
【物理不良チェック】              【物理不良チェック】
```

## 対象クレート

- **主**: `crates/diagnostic/` (Chunk 22 で実装、本チャンクで改訂)
- **副**: `crates/case-manager/` (Chunk 21 で実装、Symptom 関連を削除)

## 重要な設計原則

### Workbench は「判定者」ではなく「事実提供者」

```rust
✗ 削除する設計:
  - Symptom enum で「これは削除案件」「これはフォーマット案件」と Workbench が判定
  - 複合判定 (Mixed) のような業務的に意味の薄い情報

○ 残す設計:
  - 「削除エントリが N 件検出」(事実)
  - 「MFT エントリ数 X 件」(事実、フォーマット案件で参考になる)
  - 「FS 破損 Y 件」(事実)
  
  → CS が事実を読んで業務判断する
```

## 仕様参照

### ビジネス要件 (改訂)

- **FR-DIAG-04** (CRM 貼り付けテキスト): 業務フローと整合 ← 達成基準を厳格化
- **FR-DIAG-06** (事実ベースの報告): 判定ロジックを排除 ← 新規

### 既存実装の参照

- Chunk 21 の `case-manager` (Symptom 削除対象)
- Chunk 22 の `diagnostic` (再編対象)

## 実装内容

### Part A: case-manager の修正

#### A-1. `crates/case-manager/src/symptom.rs` を**削除**

ファイル丸ごと削除。`Symptom` enum と `FsAnomaly` enum を完全に取り除く。

#### A-2. `crates/case-manager/src/lib.rs` の修正

```rust
// 削除:
pub mod symptom;
pub use symptom::{FsAnomaly, Symptom};

// 残す:
pub mod case;
pub mod case_id;
pub mod diagnostic;
pub mod error;
pub mod storage;
```

#### A-3. `crates/case-manager/src/diagnostic.rs` の修正

```rust
// 旧:
pub struct DiagnosticInput {
    pub diagnosed_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<u64>,
    pub filesystem_type: Option<String>,
    pub symptom: Option<Symptom>,           // ← 削除
    pub total_files: usize,
    pub deleted_files: usize,
    pub total_size_bytes: u64,
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub notes: String,
}

// 新:
pub struct DiagnosticInput {
    pub diagnosed_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<u64>,
    pub filesystem_type: Option<String>,
    pub filesystem_findings: Option<FilesystemFindings>,  // ← 新規追加
    pub total_files: usize,
    pub deleted_files: usize,
    pub total_size_bytes: u64,
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub notes: String,
}

/// ファイルシステムの破損状態 (事実のみ、判定なし)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesystemFindings {
    /// NTFS シグネチャが有効か (true = 認識成功)
    pub signature_valid: bool,
    /// 読み取りに失敗した MFT エントリ数
    pub mft_corrupted_count: usize,
    /// 不正な run-list の検出件数
    pub invalid_runlist_count: usize,
    /// Boot sector に異常なし
    pub boot_sector_ok: bool,
    /// その他の異常 (説明文の Vec)
    pub other_issues: Vec<String>,
}

impl FilesystemFindings {
    /// 何らかの異常があるか
    pub fn has_any_issue(&self) -> bool {
        !self.signature_valid
            || self.mft_corrupted_count > 0
            || self.invalid_runlist_count > 0
            || !self.boot_sector_ok
            || !self.other_issues.is_empty()
    }
}
```

#### A-4. `crates/case-manager/src/case.rs` の確認

`Case` 構造体は Symptom を直接参照していなかったので変更なし (DiagnosticInput 経由のみ)。

### Part B: diagnostic の修正

#### B-1. `crates/diagnostic/src/symptom_detector.rs` を**削除**

ファイル丸ごと削除。判定ロジックを完全に排除。

#### B-2. `crates/diagnostic/src/lib.rs` の修正

```rust
// 削除:
pub mod symptom_detector;

// DiagnosticEngine::diagnose() 内の症状判定コードも削除
```

#### B-3. `crates/diagnostic/src/report.rs` の修正

```rust
// 削除: symptom フィールド
pub struct DiagnosticReport {
    pub case_id: CaseId,
    pub diagnosed_at: DateTime<Utc>,
    pub duration_secs: u64,
    
    pub hardware: HardwareInfo,
    pub filesystem: FilesystemInfo,
    // pub symptom: Symptom,  ← 削除
    pub filesystem_findings: FilesystemFindings,  // ← 新規追加 (case-manager から re-use)
    
    pub file_stats: FileStatistics,
    pub format_breakdown: BTreeMap<String, FormatCount>,
    pub folder_breakdown: Vec<FolderCount>,
    
    pub deleted_file_stats: Option<DeletedFileStats>,
    pub anomalies: FsAnomalyReport,  // ← 残す (より詳細な事実情報のため)
}

// FsAnomalyReport も残す (FilesystemFindings は slim 版、FsAnomalyReport は詳細版)
// to_anomaly_list() メソッドは削除 (Symptom 関連だったため)
impl FsAnomalyReport {
    /// FilesystemFindings に変換 (slim 版、case.json 保存用)
    pub fn to_findings(&self) -> FilesystemFindings {
        FilesystemFindings {
            signature_valid: true,  // 現状は MFT 読み取り成功なら true
            mft_corrupted_count: self.mft_corrupted_count,
            invalid_runlist_count: self.invalid_runlist_count,
            boot_sector_ok: self.boot_sector_issues.is_empty(),
            other_issues: self.other_issues.clone(),
        }
    }
}

// DiagnosticReport の変換メソッド更新
impl DiagnosticReport {
    pub fn to_diagnostic_input(&self) -> DiagnosticInput {
        DiagnosticInput {
            diagnosed_at: Some(self.diagnosed_at),
            duration_secs: Some(self.duration_secs),
            filesystem_type: Some(self.filesystem.fs_type.clone()),
            filesystem_findings: Some(self.filesystem_findings.clone()),  // ← 変更
            total_files: self.file_stats.total_files,
            deleted_files: self.file_stats.deleted_files,
            total_size_bytes: self.file_stats.total_size_bytes,
            deleted_file_stats: self.deleted_file_stats.clone(),
            notes: String::new(),
        }
    }
}
```

#### B-4. `crates/diagnostic/src/lib.rs` (engine の修正)

```rust
impl DiagnosticEngine {
    pub fn diagnose<F>(
        volume: &mut NtfsVolume<F>,
        case_id: CaseId,
    ) -> Result<DiagnosticReport, DiagnosticError>
    where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        let started_at = Utc::now();
        
        let hardware = HardwareInfo {
            model: None,
            serial: None,
            size_bytes: 0,
        };
        
        let filesystem = gather_filesystem_info(volume)?;
        let aggregate = aggregator::aggregate_all(volume)?;
        
        // 削除: 症状判定
        // let symptom = symptom_detector::detect_symptom(...)
        
        // 新規: FilesystemFindings を構築
        let filesystem_findings = aggregate.anomalies.to_findings();
        
        let finished_at = Utc::now();
        let duration_secs = (finished_at - started_at).num_seconds().max(0) as u64;
        
        let size_bytes = filesystem.total_clusters
            .saturating_mul(filesystem.cluster_size_bytes as u64);
        let hardware = HardwareInfo { size_bytes, ..hardware };
        
        Ok(DiagnosticReport {
            case_id,
            diagnosed_at: started_at,
            duration_secs,
            hardware,
            filesystem,
            filesystem_findings,  // ← 新規
            file_stats: aggregate.file_stats,
            format_breakdown: aggregate.format_breakdown,
            folder_breakdown: aggregate.folder_breakdown,
            deleted_file_stats: aggregate.deleted_file_stats,
            anomalies: aggregate.anomalies,
        })
    }
}
```

### Part C: CRM 貼り付けテキストの再編

#### `crates/diagnostic/src/crm_text.rs` を**全面書き換え**

```rust
use std::fmt::Write;

use dds_core::format::format_bytes;

use crate::report::DiagnosticReport;

/// CRM 貼り付け用の業務テキストを生成する。
///
/// 構造:
///   1. ヘッダー (案件番号、診断日時)
///   2. ハードウェア
///   3. ファイルシステム
///   4. ファイルシステムの破損 ← 上位に移動
///   5. MFT エントリ統計 ← 新規 (フォーマット案件の参考)
///   6. ファイル統計
///   7. 削除エントリの詳細 (削除エントリあれば)
///   8. 生存ファイル統計
///   9. 主なフォルダ
///   10. 物理不良チェック
///
/// 「症状判定」セクションは削除されている。
/// Workbench は事実のみ報告、症状判定は CS の責務。
pub fn render(report: &DiagnosticReport) -> String {
    let mut s = String::with_capacity(2048);
    
    // 1. ヘッダー
    let _ = writeln!(s, "=== 論理診断結果 (案件 {}) ===", report.case_id);
    let _ = writeln!(s, "診断日時: {}", report.diagnosed_at.format("%Y-%m-%d %H:%M"));
    let _ = writeln!(s, "診断時間: {} 秒", report.duration_secs);
    let _ = writeln!(s, "※物理診断は別途実施済み");
    let _ = writeln!(s);
    
    // 2. ハードウェア
    let _ = writeln!(s, "【ハードウェア】");
    if let Some(model) = &report.hardware.model {
        let _ = writeln!(s, "HDD: {}", model);
    }
    if let Some(serial) = &report.hardware.serial {
        let _ = writeln!(s, "シリアル: {}", serial);
    }
    let _ = writeln!(s, "容量: {}", format_bytes(report.hardware.size_bytes));
    let _ = writeln!(s);
    
    // 3. ファイルシステム
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
    } else {
        0.0
    };
    let _ = writeln!(s, "使用率: {} / {} ({:.1}%)",
        format_bytes(used_bytes), format_bytes(total_bytes), usage_pct);
    let _ = writeln!(s);
    
    // 4. ファイルシステムの破損 ← 上位に移動
    let _ = writeln!(s, "【ファイルシステムの破損】");
    let findings = &report.filesystem_findings;
    if findings.signature_valid {
        let _ = writeln!(s, "ファイルシステム署名: 正常 (NTFS 認識成功)");
    } else {
        let _ = writeln!(s, "ファイルシステム署名: 異常");
    }
    let _ = writeln!(s, "MFT エントリ破損: {} 件", findings.mft_corrupted_count);
    let _ = writeln!(s, "不正な run-list: {} 件", findings.invalid_runlist_count);
    if findings.boot_sector_ok {
        let _ = writeln!(s, "Boot sector: 正常");
    } else {
        let _ = writeln!(s, "Boot sector: 異常");
    }
    if !findings.other_issues.is_empty() {
        let _ = writeln!(s, "その他の異常: {} 件", findings.other_issues.len());
    }
    let _ = writeln!(s);
    
    // 5. MFT エントリ統計 ← 新規 (フォーマット案件の参考)
    let _ = writeln!(s, "【MFT エントリ統計】");
    let _ = writeln!(s, "全エントリ数: {} 件", report.file_stats.total_files);
    let _ = writeln!(s, "※ フォーマット案件の場合、エントリ数の極端な少なさが参考になります");
    let _ = writeln!(s, "※ 旧 MFT 残存度の計測は Phase 2 で対応予定");
    let _ = writeln!(s);
    
    // 6. ファイル統計
    let _ = writeln!(s, "【ファイル統計】");
    let _ = writeln!(s, "全ファイル: {} 件 ({})",
        report.file_stats.total_files, format_bytes(report.file_stats.total_size_bytes));
    let _ = writeln!(s, "  - 通常 (生存): {} 件", report.file_stats.live_files);
    let _ = writeln!(s, "  - 削除済み: {} 件", report.file_stats.deleted_files);
    let _ = writeln!(s, "ディレクトリ: {} 件", report.file_stats.directories);
    let _ = writeln!(s);
    
    // 7. 削除エントリの詳細 (削除エントリあれば表示)
    if let Some(deleted) = &report.deleted_file_stats {
        let _ = writeln!(s, "【削除エントリの詳細】");
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
    
    // 8. 生存ファイル統計 (参考)
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
    
    // 9. 主なフォルダ
    if !report.folder_breakdown.is_empty() {
        let _ = writeln!(s, "【主なフォルダ】(上位 10)");
        for folder in report.folder_breakdown.iter().take(10) {
            let _ = writeln!(s, "  {}: {} 件 / {}",
                folder.path, folder.file_count, format_bytes(folder.total_size_bytes));
        }
        let _ = writeln!(s);
    }
    
    // 10. 物理不良チェック
    let _ = writeln!(s, "【物理不良チェック】");
    let _ = writeln!(s, "未実施 (Phase 2 で対応予定)");
    let _ = writeln!(s);
    
    let _ = writeln!(s, "=== 診断完了 ===");
    
    s
}
```

**変更点まとめ**:
- 「症状判定」セクション削除
- 「ファイルシステムの破損」をハードウェア/ファイルシステムの直後に上位移動
- 「MFT エントリ統計」セクションを新規追加 (フォーマット案件の参考用)
- `render_symptom_details` 関数削除
- `anomaly_label` 関数削除
- `Symptom` import 削除

## 既存テストのマイグレーション

Chunk 21 と Chunk 22 で書いた以下のテストを削除/修正:

### case-manager (Chunk 21) のテスト

```rust
// 削除:
- symptom_none_serializes_with_type_tag
- symptom_deleted_serializes_correctly
- symptom_formatted_includes_fields
- symptom_primary_label_returns_japanese
- symptom_mixed_primary_label_prioritizes_fs_error
- (その他 Symptom 関連)

// 既存テストで Symptom を使っているもの (例: case_roundtrip_preserves_all_fields):
- Symptom フィールドへの代入を削除
- 代わりに filesystem_findings をテスト
```

### diagnostic (Chunk 22) のテスト

```rust
// 削除:
- detect_none_when_clean
- detect_deleted_when_deleted_files_present
- detect_filesystem_error_when_anomalies
- detect_formatted_when_very_few_files
- detect_mixed_when_multiple_conditions
- detect_prioritizes_fs_error_over_deletion
- (symptom_detector.rs 内のすべてのテスト)

// 修正:
- crm_text_uses_japanese_symptom_label → 削除 (該当機能なし)
- product_demo_diagnose_with_crm_text → 出力に「症状判定」を含まないことを確認
- diagnose_deleted_fixture_produces_deleted_symptom → 
    diagnose_deleted_fixture_detects_5_deleted_entries に rename
    assertion を symptom チェックから deleted_file_stats チェックに変更
```

## 新規テスト要件 (最低 6 件)

### case-manager (filesystem_findings 関連)

1. `filesystem_findings_default_has_no_issues`: Default で issue なし
2. `filesystem_findings_has_any_issue_detects_mft_corruption`: mft_corrupted_count > 0 で true
3. `filesystem_findings_serializes_correctly`: JSON ラウンドトリップ

### diagnostic (FilesystemFindings 統合)

4. `diagnose_populates_filesystem_findings`: DiagnosticEngine 実行後、findings が埋まる
5. `to_diagnostic_input_includes_filesystem_findings`: case.json 保存形式に含まれる
6. `crm_text_no_longer_contains_symptom_section`: 「症状判定」「主症状」が出力に含まれない
7. `crm_text_shows_filesystem_findings_above_file_stats`: ファイルシステムの破損が ファイル統計より前に出る

### 削除案件の業務シナリオ (最重要)

8. `product_demo_deleted_case_no_format_misdetection`:
   ```rust
   // ntfs_with_5_deletions_small で診断
   // 出力に「フォーマット」という単語が含まれないことを確認
   // (Chunk 22 で発生していた誤判定の回帰防止)
   let crm = report.to_crm_text();
   assert!(!crm.contains("フォーマット (複合)"));
   assert!(!crm.contains("主症状: フォーマット"));
   ```

## 制約

- **行数目安**:
  - 削除: 約 250 行 (symptom.rs, symptom_detector.rs, 関連テスト)
  - 追加: 約 150 行 (FilesystemFindings, 新 crm_text, 新テスト)
  - 修正: 約 100 行 (既存テストの調整)
  - 合計: 約 -100 行 (ネットでは減る)
- **単体テスト**: 既存削除 約 -10 件 + 新規 約 +7 件 = ネット -3 件
- **結合テスト**: 既存修正、新規 1 件 (削除案件で誤判定が出ない確認)
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **`cargo test --workspace` 全パス維持**

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-case-manager` が全パス (Symptom 関連テスト削除済み)
- [ ] `cargo test -p dds-diagnostic` が全パス (症状判定テスト削除済み)
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_deleted_case_no_format_misdetection` が pass
- [ ] `grep -r 'Symptom\|symptom_detector\|FsAnomaly' crates/` で参照 0 件
- [ ] 新 CRM テキスト出力に「症状判定」セクションが含まれない
- [ ] 新 CRM テキスト出力で「ファイルシステムの破損」が「ファイル統計」より前

## 関連 FR 要件

- **FR-DIAG-04** (CRM 貼り付けテキスト) ← 業務適用品質に到達
- **FR-DIAG-06** (事実ベースの報告) ← 新規達成 (判定ロジック排除)

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 診断レポートが業務フローと完全整合**
4. 次のステップ:
   - **Chunk 22.5**: 削除ファイル復旧可能性推定
   - **Chunk 23**: 業務向け出力ディレクトリ構造

---

## 注意事項

### Symptom 削除の影響範囲

`grep -r 'Symptom' crates/` で全参照箇所を確認:
- `case-manager/src/symptom.rs` (ファイル削除)
- `case-manager/src/lib.rs` (pub use 削除)
- `case-manager/src/diagnostic.rs` (symptom フィールド削除)
- `case-manager/tests/*.rs` (Symptom 使用箇所削除)
- `diagnostic/src/symptom_detector.rs` (ファイル削除)
- `diagnostic/src/lib.rs` (pub mod 削除)
- `diagnostic/src/report.rs` (symptom フィールド削除)
- `diagnostic/src/crm_text.rs` (render_symptom_details, anomaly_label 関数削除)
- `diagnostic/tests/*.rs` (Symptom 関連テスト削除)

### FilesystemFindings vs FsAnomalyReport の使い分け

両方を残す理由:

- **FsAnomalyReport** (diagnostic クレート内、in-memory 用)
  - 詳細な内部表現
  - aggregator が直接書く
  - other_issues に詳細メッセージを格納
- **FilesystemFindings** (case-manager クレート、永続化用)
  - case.json に保存する slim 版
  - boot_sector_ok のような bool フラグ
  - JSON で読みやすい

変換: `FsAnomalyReport::to_findings() -> FilesystemFindings`

### case.json の互換性

既存の case.json (Chunk 21 で生成されたもの) は `symptom: Option<Symptom>` を持っているが、新スキーマでは削除。

選択肢:
- **A**: 既存 case.json を再生成 (Phase 1.5 開発中なので実害なし)
- **B**: serde の `#[serde(default)]` で symptom フィールドを無視 + 削除
- **C**: マイグレーションスクリプトを書く

**A を採用**: Phase 1.5 はまだ業務適用前なので、既存 case.json があれば手動削除で OK。

### CRM テキストでのフォーマット情報の表現

「フォーマット案件」を判定で出さない代わりに、事実として:

```
【MFT エントリ統計】
全エントリ数: 25 件
※ フォーマット案件の場合、エントリ数の極端な少なさが参考になります
```

を出すことで、CS は「お客様の主訴がフォーマット → MFT エントリが 25 件 → 整合しているか?」を判断できる。

業務的な使い方:
- お客様: 「フォーマットしてしまった」(主訴)
- Workbench: 「MFT エントリ 25 件」(事実)
- CS: 「うん、フォーマット直後の MFT 量と整合する。復旧見積もり OK」

Workbench は「これはフォーマットです!」と言わない。CS が事実を読んで業務判断する。

### 既存テストの削除/修正の作業順序

推奨順序:

1. `crates/diagnostic/src/symptom_detector.rs` を削除
2. `crates/diagnostic/src/lib.rs` から `pub mod symptom_detector;` を削除
3. `crates/diagnostic/src/report.rs` の `symptom` フィールドを削除
4. `crates/diagnostic/src/lib.rs` の `DiagnosticEngine::diagnose` から症状判定削除
5. `crates/case-manager/src/symptom.rs` を削除
6. `crates/case-manager/src/lib.rs` から `pub mod symptom;` を削除
7. `crates/case-manager/src/diagnostic.rs` の `symptom` を `filesystem_findings` に置換
8. 全コンパイルエラーを潰す
9. テストを順次削除/修正
10. `cargo test --workspace` で全パス確認

### Phase 1.5 で意図的に保持する設計

- `DiagnosticReport.anomalies: FsAnomalyReport` (詳細情報、内部用)
- `DiagnosticInput.filesystem_findings: Option<FilesystemFindings>` (slim 版、永続化用)
- 両者の変換は `FsAnomalyReport::to_findings()`

将来 Phase 2 で UI を作るとき、UI 側でも `FilesystemFindings` を表示するので、case-manager に持つのが正しい。

---

## 完了報告例

```markdown
## Chunk 22.6 完了報告

### 削除ファイル
- crates/case-manager/src/symptom.rs
- crates/diagnostic/src/symptom_detector.rs

### 修正ファイル
- crates/case-manager/src/lib.rs (Symptom/FsAnomaly export 削除)
- crates/case-manager/src/diagnostic.rs (symptom → filesystem_findings)
- crates/diagnostic/src/lib.rs (症状判定処理削除)
- crates/diagnostic/src/report.rs (symptom フィールド削除、to_findings 追加)
- crates/diagnostic/src/crm_text.rs (全面書き換え、症状判定セクション削除)

### 新規追加
- FilesystemFindings struct (case-manager)
- FsAnomalyReport::to_findings() method (diagnostic)

### テスト統計
- 削除: 約 10 件 (Symptom 関連、症状判定関連)
- 新規: 約 8 件 (FilesystemFindings、新 CRM テキスト構造、誤判定回帰防止)
- 合計: 既存 432+ - 10 + 8 = **430+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- grep で Symptom/symptom_detector/FsAnomaly の参照 0 件
- 「フォーマット (複合)」のような誤判定が出ないことを機械的に確認

### 業務価値の見える化 (新 CRM テキスト)
```
=== 論理診断結果 (案件 260522-04) ===
診断日時: 2026-05-22 10:04
診断時間: 0 秒
※物理診断は別途実施済み

【ハードウェア】
容量: 20.00 MB

【ファイルシステム】
種類: NTFS
ボリュームシリアル: 0815187447FAC69A
クラスタサイズ: 4096 bytes
使用率: 0 B / 20.00 MB (0.0%)

【ファイルシステムの破損】
ファイルシステム署名: 正常 (NTFS 認識成功)
MFT エントリ破損: 0 件
不正な run-list: 0 件
Boot sector: 正常

【MFT エントリ統計】
全エントリ数: 33 件
※ フォーマット案件の場合、エントリ数の極端な少なさが参考になります
※ 旧 MFT 残存度の計測は Phase 2 で対応予定

【ファイル統計】
全ファイル: 33 件 (2.52 KB)
  - 通常 (生存): 28 件
  - 削除済み: 5 件
ディレクトリ: 0 件

【削除エントリの詳細】
形式別:
  TXT: 5 件
フォルダ別:
  \: 5 件
推定合計サイズ: 430 B

【生存ファイル統計】(参考、主要形式)
  TXT: 30 件 / 2.52 KB
  (なし): 3 件 / 0 B

【主なフォルダ】(上位 10)
  \: 30 件 / 2.52 KB
  \$Extend: 3 件 / 0 B

【物理不良チェック】
未実施 (Phase 2 で対応予定)

=== 診断完了 ===
```

### 🎉 マイルストーン
- **診断レポートが業務フローと完全整合**
- 「症状判定」セクションの誤判定問題を解消
- 「ファイルシステムの破損」を上位に配置、業務的に重要な情報を先に表示
- Workbench は「事実」のみ報告、CS が業務判断する役割分担を明確化

- **関連 FR**: FR-DIAG-04 (業務適用品質)、FR-DIAG-06 (事実ベース報告、新規)

→ tester エージェントへ引き継ぎお願いします
```
