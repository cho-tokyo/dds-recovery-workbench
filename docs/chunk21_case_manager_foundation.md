# Chunk 21 指示: case-manager 基盤（Case 構造体 + 案件 JSON 永続化）

Phase 1.5 の最初のチャンク。**「案件」という業務概念**をコードに導入し、案件情報を JSON ファイルで永続化する基盤を構築します。

> 🎯 完了時点で「案件番号 260522-04 という単位で業務情報が管理可能」になる。Chunks 22-23 (診断 + 出力構造) の土台が完成。

---

## 目的

案件管理の基盤を構築する:

1. **`Case` 構造体**: 案件のすべての業務情報を保持
2. **`CaseId`**: yymmdd-NN 形式の案件番号、バリデーション付き
3. **`Symptom` enum**: 削除/フォーマット/FS 異常/複合/異常なし
4. **`DiagnosticInput`** (placeholder): Chunk 22 で内容を埋める器
5. **`CaseStorage`**: `C:\cases\{案件番号}\case.json` 形式での読み書き
6. **エラー処理**: 重複作成は拒否 (Q28 確定)
7. **案件一覧**: 既存案件のリスト取得

## 対象クレート

`crates/case-manager/` (Chunk 1 で空スケルトン作成済み、本実装)

## 重要な設計原則

### 業務ドメインの言葉でモデル化

```rust
✗ 技術的な命名:
  - ContentEntry, ContentRecord, FsObject

○ 業務ドメインの命名:
  - Case, CaseId, Symptom, DiagnosticInput
```

CRM / お客様 / CS が使う言葉に近い命名で、コードを読めば業務フローが分かる。

### case-manager は薄い層

```
誤った設計:
  case-manager が顧客 DB / 進捗管理 / 履歴検索を全部持つ
  
正しい設計:
  case-manager は「現在進行中の 1 案件の情報を JSON で永続化」のみ
  顧客 DB、進捗管理、履歴は CRM の役割
```

DDS の業務実態 (1 PC 1 案件専有、CRM が案件全体管理) に合わせる。

### 依存方向

```
workbench-cli (Phase 1.5 後半)
    ├─→ case-manager  (本チャンク)
    ├─→ recovery
    ├─→ report
    └─→ fs-ntfs

case-manager
    └─→ wish-match    (Wishlist を Case 内に格納)
        └─→ core
```

**case-manager は recovery / report に依存しない**。整合性は CLI / UI 層で取る。

## 仕様参照

### ビジネス要件

- **FR-CASE-01**: 案件単位での業務情報管理
- **FR-CASE-02**: 案件番号 (yymmdd-NN) による識別
- **FR-CASE-03**: 案件情報の永続化 (PC ローカル、社内保存)
- **FR-CASE-04**: 1 PC 1 案件専有の業務フロー対応

### 案件番号の形式

```
yymmdd-NN
  yymmdd: 6 桁の日付 (例: 260522 = 2026 年 5 月 22 日)
  -: ハイフン
  NN: 2 桁の連番 (00-99)
  
例:
  260522-04  (2026-05-22 の 4 番目の案件)
  260601-12  (2026-06-01 の 12 番目の案件)
```

文字列長は厳密に 9 文字。`yymmdd` 部分の日付妥当性は **緩く**チェック (例: 999999-99 のような明らかな不正値だけ拒否、月日の組み合わせ妥当性は CRM 側で担保)。

## 実装内容

### モジュール構成

```
crates/case-manager/
├── Cargo.toml
└── src/
    ├── lib.rs           ← re-export
    ├── error.rs         ← CaseError
    ├── case_id.rs       ← CaseId (yymmdd-NN)
    ├── symptom.rs       ← Symptom + FsAnomaly
    ├── diagnostic.rs    ← DiagnosticInput + DeletedFileStats (placeholder)
    ├── case.rs          ← Case 構造体本体
    └── storage.rs       ← CaseStorage (CRUD)
```

### Cargo.toml

```toml
[package]
name = "dds-case-manager"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true
dds-core.workspace = true
dds-wish-match.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

ワークスペースルートに `serde_json = "1.0"` が無ければ追加。

### 1. `error.rs`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CaseError {
    #[error("Invalid case ID '{input}': {reason}")]
    InvalidCaseId { input: String, reason: String },
    
    #[error("Case already exists: {case_id}")]
    CaseAlreadyExists { case_id: String },
    
    #[error("Case not found: {case_id}")]
    CaseNotFound { case_id: String },
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
```

### 2. `case_id.rs`

newtype wrapper で yymmdd-NN 形式を強制:

```rust
use std::fmt;
use serde::{Deserialize, Serialize};
use crate::error::CaseError;

/// 案件番号 (yymmdd-NN 形式)。
///
/// 形式:
/// - 全 9 文字
/// - 先頭 6 文字: 日付 (yymmdd、すべて数字)
/// - 7 文字目: ハイフン '-'
/// - 末尾 2 文字: 連番 (00-99)
///
/// 例: "260522-04"
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaseId(String);

impl CaseId {
    pub fn parse(s: &str) -> Result<Self, CaseError> {
        if s.len() != 9 {
            return Err(CaseError::InvalidCaseId {
                input: s.to_string(),
                reason: format!("length must be exactly 9, got {}", s.len()),
            });
        }
        
        let bytes = s.as_bytes();
        
        for i in 0..6 {
            if !bytes[i].is_ascii_digit() {
                return Err(CaseError::InvalidCaseId {
                    input: s.to_string(),
                    reason: format!("position {} must be a digit, got '{}'", i, bytes[i] as char),
                });
            }
        }
        
        if bytes[6] != b'-' {
            return Err(CaseError::InvalidCaseId {
                input: s.to_string(),
                reason: format!("position 6 must be '-', got '{}'", bytes[6] as char),
            });
        }
        
        for i in 7..9 {
            if !bytes[i].is_ascii_digit() {
                return Err(CaseError::InvalidCaseId {
                    input: s.to_string(),
                    reason: format!("position {} must be a digit, got '{}'", i, bytes[i] as char),
                });
            }
        }
        
        Ok(Self(s.to_string()))
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for CaseId {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CaseId {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        CaseId::parse(&s).map_err(serde::de::Error::custom)
    }
}
```

### 3. `symptom.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Symptom {
    None,
    Deleted,
    Formatted {
        current_mft_entries: usize,
        old_mft_recoverability_hint: Option<f64>,
    },
    FilesystemError {
        anomalies: Vec<FsAnomaly>,
    },
    Mixed {
        symptoms: Vec<Symptom>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FsAnomaly {
    MftEntryCorrupted { count: usize },
    InvalidRunList { count: usize },
    BootSectorAnomaly { description: String },
    InvalidVolumeSerial,
    Other { description: String },
}

impl Symptom {
    /// 業務的な「主症状」を日本語で返す
    pub fn primary_label(&self) -> &str {
        match self {
            Symptom::None => "異常なし",
            Symptom::Deleted => "削除",
            Symptom::Formatted { .. } => "フォーマット",
            Symptom::FilesystemError { .. } => "ファイルシステム異常",
            Symptom::Mixed { symptoms } => {
                if symptoms.iter().any(|s| matches!(s, Symptom::FilesystemError { .. })) {
                    "ファイルシステム異常 (複合)"
                } else if symptoms.iter().any(|s| matches!(s, Symptom::Formatted { .. })) {
                    "フォーマット (複合)"
                } else {
                    "削除 (複合)"
                }
            }
        }
    }
}
```

### 4. `diagnostic.rs` (placeholder for Chunk 22)

```rust
use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::symptom::Symptom;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticInput {
    pub diagnosed_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<u64>,
    
    pub filesystem_type: Option<String>,
    pub symptom: Option<Symptom>,
    
    pub total_files: usize,
    pub deleted_files: usize,
    pub total_size_bytes: u64,
    
    pub deleted_file_stats: Option<DeletedFileStats>,
    
    pub notes: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeletedFileStats {
    pub total_count: usize,
    pub by_extension: BTreeMap<String, usize>,
    pub by_folder: Vec<(String, usize)>,
    pub estimated_total_size: u64,
    
    /// Chunk 22.5 で埋まる
    pub recoverability_estimate: Option<RecoverabilityEstimate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoverabilityEstimate {
    pub high_confidence: usize,
    pub medium_confidence: usize,
    pub low_confidence: usize,
}
```

### 5. `case.rs`

```rust
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use dds_wish_match::Wishlist;

use crate::case_id::CaseId;
use crate::diagnostic::DiagnosticInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    pub case_id: CaseId,
    
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    
    #[serde(default)]
    pub diagnostic_input: DiagnosticInput,
    
    pub wishlist: Option<Wishlist>,
    
    pub recovery_report_summary: Option<RecoveryReportSummary>,
    
    pub output_dir: Option<PathBuf>,
}

impl Case {
    pub fn new(case_id: CaseId) -> Self {
        let now = Utc::now();
        Self {
            case_id,
            created_at: now,
            updated_at: now,
            diagnostic_input: DiagnosticInput::default(),
            wishlist: None,
            recovery_report_summary: None,
            output_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryReportSummary {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    
    pub total_matched: usize,
    pub recovered_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    
    pub validated_count: usize,
    pub invalid_count: usize,
    pub uncertain_count: usize,
    
    pub total_bytes_written: u64,
    
    pub recovery_success_rate: f64,
    pub quality_assurance_rate: f64,
}
```

### 6. `storage.rs`

```rust
use std::fs;
use std::path::PathBuf;
use chrono::Utc;

use crate::case::Case;
use crate::case_id::CaseId;
use crate::error::CaseError;

pub struct CaseStorage {
    base_dir: PathBuf,
}

impl CaseStorage {
    pub fn default_location() -> Self {
        Self {
            base_dir: PathBuf::from("C:\\cases"),
        }
    }
    
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
    
    pub fn case_file_path(&self, case_id: &CaseId) -> PathBuf {
        self.base_dir.join(case_id.as_str()).join("case.json")
    }
    
    pub fn case_dir(&self, case_id: &CaseId) -> PathBuf {
        self.base_dir.join(case_id.as_str())
    }
    
    /// 新規案件を作成。既存ならエラー (Q28: A の挙動)
    pub fn create_new(&self, case_id: CaseId) -> Result<Case, CaseError> {
        let path = self.case_file_path(&case_id);
        if path.exists() {
            return Err(CaseError::CaseAlreadyExists {
                case_id: case_id.as_str().to_string(),
            });
        }
        let case = Case::new(case_id);
        self.save(&case)?;
        Ok(case)
    }
    
    pub fn load(&self, case_id: &CaseId) -> Result<Case, CaseError> {
        let path = self.case_file_path(case_id);
        if !path.exists() {
            return Err(CaseError::CaseNotFound {
                case_id: case_id.as_str().to_string(),
            });
        }
        let json = fs::read_to_string(&path)?;
        let case: Case = serde_json::from_str(&json)?;
        Ok(case)
    }
    
    pub fn save(&self, case: &Case) -> Result<(), CaseError> {
        let path = self.case_file_path(&case.case_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut updated = case.clone();
        updated.updated_at = Utc::now();
        let json = serde_json::to_string_pretty(&updated)?;
        fs::write(&path, json)?;
        Ok(())
    }
    
    pub fn delete(&self, case_id: &CaseId) -> Result<(), CaseError> {
        let path = self.case_file_path(case_id);
        if !path.exists() {
            return Err(CaseError::CaseNotFound {
                case_id: case_id.as_str().to_string(),
            });
        }
        fs::remove_file(&path)?;
        Ok(())
    }
    
    pub fn list_all(&self) -> Result<Vec<CaseId>, CaseError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let mut cases = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let Ok(case_id) = CaseId::parse(&name) else {
                continue;
            };
            let case_file = self.case_file_path(&case_id);
            if !case_file.exists() {
                continue;
            }
            cases.push(case_id);
        }
        cases.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(cases)
    }
}
```

### 7. `lib.rs`

```rust
//! DDS 案件管理クレート。
//!
//! 1 案件 = 1 つの `Case` = 1 つの `case.json` ファイル。
//!
//! 案件は yymmdd-NN 形式の番号で識別され、`C:\cases\{案件番号}\case.json`
//! に永続化される。
//!
//! 業務的な責務:
//! - 案件単位の情報管理 (診断結果、Wishlist、復旧結果サマリ)
//! - 案件番号のバリデーション
//! - 案件 JSON の CRUD
//!
//! 業務的に**担わない**こと (CRM や上位層の責務):
//! - 顧客情報の管理
//! - 案件番号の採番
//! - 進捗管理 / 状態遷移
//! - 案件横断の検索 / 集計

pub mod case;
pub mod case_id;
pub mod diagnostic;
pub mod error;
pub mod storage;
pub mod symptom;

pub use case::{Case, RecoveryReportSummary};
pub use case_id::CaseId;
pub use diagnostic::{DeletedFileStats, DiagnosticInput, RecoverabilityEstimate};
pub use error::CaseError;
pub use storage::CaseStorage;
pub use symptom::{FsAnomaly, Symptom};
```

## 単体テスト要件（最低 20 件）

### `case_id.rs` (最低 8 件)

1. `case_id_parses_valid_format`
2. `case_id_rejects_short_input`
3. `case_id_rejects_long_input`
4. `case_id_rejects_missing_hyphen`
5. `case_id_rejects_non_digit_in_date_part`
6. `case_id_rejects_non_digit_in_sequence_part`
7. `case_id_serializes_as_plain_string`
8. `case_id_deserializes_from_string`

### `symptom.rs` (最低 4 件)

9. `symptom_none_serializes_with_type_tag`
10. `symptom_deleted_serializes_correctly`
11. `symptom_formatted_includes_fields`
12. `symptom_primary_label_returns_japanese`
13. `symptom_mixed_primary_label_prioritizes_fs_error`

### `storage.rs` (最低 10 件、tempfile 使用)

14. `create_new_case_succeeds_when_not_exists`
15. `create_new_case_fails_when_already_exists`
16. `save_creates_case_directory_structure`
17. `load_returns_saved_data`
18. `save_updates_updated_at_timestamp`
19. `load_nonexistent_returns_not_found`
20. `delete_removes_case_file`
21. `delete_nonexistent_returns_not_found`
22. `list_all_returns_sorted_case_ids`
23. `list_all_ignores_invalid_directory_names`
24. `list_all_returns_empty_when_base_dir_missing`

### `case.rs` + `diagnostic.rs` (最低 3 件)

25. `case_new_initializes_with_defaults`
26. `case_roundtrip_preserves_all_fields`
27. `diagnostic_input_default_has_empty_stats`

## 結合テスト要件（最低 2 件）

`crates/case-manager/tests/case_lifecycle_integration.rs`:

### 1. 案件のライフサイクル完全シミュレーション

```rust
#[test]
fn full_case_lifecycle_create_diagnose_recover() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    let case_id = CaseId::parse("260522-04").unwrap();
    
    // Step 1: 案件作成
    let mut case = storage.create_new(case_id.clone()).unwrap();
    
    // Step 2: 診断結果反映 (Chunk 22 を想定して手動で埋める)
    case.diagnostic_input.filesystem_type = Some("NTFS".into());
    case.diagnostic_input.symptom = Some(Symptom::Deleted);
    case.diagnostic_input.total_files = 12847;
    case.diagnostic_input.deleted_files = 234;
    storage.save(&case).unwrap();
    
    // Step 3: Wishlist 追加
    case.wishlist = Some(Wishlist::new().add(
        Wish::new(WishItem::Extension("docx".into()), "Word ファイル全部")
            .with_priority(Priority::High)
    ));
    storage.save(&case).unwrap();
    
    // Step 4: 復旧結果サマリ追加
    case.recovery_report_summary = Some(RecoveryReportSummary { /* ... */ });
    case.output_dir = Some(PathBuf::from("G:\\260522-04"));
    storage.save(&case).unwrap();
    
    // Step 5: 再読み込みで全情報保持
    let reloaded = storage.load(&case_id).unwrap();
    assert_eq!(reloaded.diagnostic_input.filesystem_type, Some("NTFS".into()));
    assert!(reloaded.wishlist.is_some());
    assert!(reloaded.recovery_report_summary.is_some());
    assert_eq!(reloaded.output_dir, Some(PathBuf::from("G:\\260522-04")));
}
```

### 2. プロダクトデモテスト

```rust
#[test]
fn product_demo_case_management_basics() {
    let temp = TempDir::new().unwrap();
    let storage = CaseStorage::with_base_dir(temp.path());
    
    // 1 日分の業務シミュレーション: 3 案件を順次受領
    for seq in 1..=3 {
        let case_id = CaseId::parse(&format!("260522-{:02}", seq)).unwrap();
        let case = storage.create_new(case_id).unwrap();
        println!("案件 {} 作成", case.case_id);
    }
    
    let list = storage.list_all().unwrap();
    
    println!("\n=== Phase 1.5 Case Management Demo (Chunk 21) ===\n");
    println!("保存先: {:?}", temp.path());
    println!("登録案件数: {}", list.len());
    println!();
    println!("案件一覧:");
    for case_id in &list {
        let case = storage.load(case_id).unwrap();
        println!("  {} (作成: {})", case.case_id, case.created_at.format("%Y-%m-%d %H:%M"));
    }
    println!("\n=== Case Manager 基盤完成 ===");
    
    assert_eq!(list.len(), 3);
}
```

## 制約

- **行数目安**:
  - `case_id.rs`: ~85 行 + テスト 60 行
  - `symptom.rs`: ~95 行 + テスト 50 行
  - `diagnostic.rs`: ~75 行 (placeholder)
  - `case.rs`: ~70 行
  - `storage.rs`: ~140 行 + テスト 130 行
  - `error.rs`: ~25 行
  - `lib.rs`: ~30 行
  - 合計: 約 500 行コード + 240 行テスト
- **単体テスト最低 20 件**
- **結合テスト最低 2 件**
- **全公開 type/method に rustdoc 必須**
- **`unsafe` 0 件**
- **依存方向**: `case-manager → wish-match → core` のみ (recovery / report / fs-ntfs に依存しない)

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test -p dds-case-manager` が全パス (≥20 件単体 + ≥2 件結合)
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `product_demo_case_management_basics` が pass + 出力が見える
- [ ] case.json サンプルを手動で確認 (整形済 JSON、日本語が UTF-8 で読める)
- [ ] `grep -r 'unsafe' crates/case-manager/src/` で 0 件
- [ ] case-manager の依存が `wish-match` 経由のもののみ (recovery / report / fs-ntfs 不依存)

## 関連 FR 要件

- **FR-CASE-01** (案件単位管理) ← 基盤達成
- **FR-CASE-02** (yymmdd-NN 形式案件番号) ← 達成
- **FR-CASE-03** (PC ローカル永続化) ← 達成
- **FR-CASE-04** (1 PC 1 案件専有対応) ← 基盤達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 Phase 1.5 開始、案件管理基盤完成**
4. 次のステップ:
   - **Chunk 22**: 診断エンジン + CRM 貼り付けテキスト生成
   - **Chunk 22.5**: 削除ファイルの復旧可能性推定
   - **Chunk 23**: 業務向け出力ディレクトリ構造

---

## 注意事項

### case_id バリデーションの緩さ

日付部分 (`yymmdd`) の妥当性は**緩くチェック**:
- 数字 6 桁であることは要求
- 月 (mm) が 01-12 か、日 (dd) が 01-31 かはチェックしない
- 「999999-04」は受け入れる (CRM 側のミスは CRM で気づく)

理由:
- CRM が採番する責任 (Workbench は形式のみ検証)
- 過度な検証は番号体系変更への柔軟性を奪う

### updated_at の自動更新

`save()` が呼ばれるたびに `updated_at` が現在時刻になる。

### `diagnostic_input` が Option ではない理由

`Option<DiagnosticInput>` ではなく `DiagnosticInput`:
- 各フィールドが Optional なので、空 Default 状態を表現可能
- JSON 構造がシンプル

### Phase 1 → 1.5 のテスト統合性

Chunk 21 は既存 Phase 1 テストに**影響を与えない**:
- 新規クレート `case-manager` を追加のみ
- 既存クレートに変更なし
- `cargo test --workspace` で既存 383+ テストが全 pass を維持

### Wishlist の依存

`case-manager` が `wish-match` に依存:
- `Case::wishlist` に `Wishlist` を直接格納
- 静的型による安全性

### Windows パスの扱い

`default_location()` は `C:\cases` (Windows 形式):
- テストでは `with_base_dir()` で tempfile を使う
- WSL 開発環境で `C:\` を使わない

### Phase 1.5 で意図的に除外した機能

- 案件アーカイブ (CRM の役割)
- 案件状態遷移の強制 (CRM の役割)
- 複数プロセス同時アクセス (1 PC 1 プロセス前提)
- case.json の暗号化 (BitLocker で担保)

---

## 完了報告例

```markdown
## Chunk 21 完了報告

### 新規ファイル
- crates/case-manager/src/lib.rs          (35 行)
- crates/case-manager/src/error.rs         (30 行)
- crates/case-manager/src/case_id.rs       (85 行 + テスト 60 行)
- crates/case-manager/src/symptom.rs       (95 行 + テスト 50 行)
- crates/case-manager/src/diagnostic.rs    (75 行)
- crates/case-manager/src/case.rs          (75 行 + テスト 30 行)
- crates/case-manager/src/storage.rs       (145 行 + テスト 130 行)
- crates/case-manager/Cargo.toml
- crates/case-manager/tests/case_lifecycle_integration.rs (130 行)

### 公開 API
- Case, CaseId, Symptom, FsAnomaly
- DiagnosticInput, DeletedFileStats, RecoverabilityEstimate
- CaseStorage (default_location / with_base_dir / create_new / load / save / delete / list_all)
- RecoveryReportSummary
- CaseError

### テスト統計
- 単体: 既存 328 + 新規 24 = **352 件 pass**
- 結合: 既存 55 + 新規 2 = **57 件 pass**
- 全 workspace: **409+ 件 pass**

### サンプル case.json
```json
{
  "case_id": "260522-04",
  "created_at": "2026-05-22T09:30:00Z",
  "updated_at": "2026-05-22T14:15:30Z",
  "diagnostic_input": {
    "diagnosed_at": null,
    "duration_secs": null,
    "filesystem_type": null,
    "symptom": null,
    "total_files": 0,
    "deleted_files": 0,
    "total_size_bytes": 0,
    "deleted_file_stats": null,
    "notes": ""
  },
  "wishlist": null,
  "recovery_report_summary": null,
  "output_dir": null
}
```

### 🎉 マイルストーン
- **Phase 1.5 開始**
- 「案件」概念がコードで表現可能に
- yymmdd-NN 形式の案件番号管理
- JSON 永続化で 1 PC 1 案件専有の業務フローに対応

- **関連 FR**: FR-CASE-01〜04 (基盤完成)

→ tester エージェントへ引き継ぎお願いします
```
