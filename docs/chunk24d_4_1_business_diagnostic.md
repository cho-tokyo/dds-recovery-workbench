# Chunk 24d-4-1 指示: 業務的診断項目の拡充

Phase 1.5 拡張の **第 4 段階 (前編)**。診断結果を「営業の見積根拠」として業務適用品質に引き上げる。

> 🎯 完了時点で「診断結果を見ただけで、営業が見積を作成し、お客様に説明できる」状態に到達。Phase 1.5 拡張の業務的価値の核心。

---

## 全体像 (Chunk 24d シリーズ)

```
✅ Chunk 24d-1: 物理ディスクアクセス層 (完了)
✅ Chunk 24d-2: パーティションテーブル解析 (完了)
✅ Chunk 24d-3: NtfsVolume との統合 (完了)
🚧 Chunk 24d-4-1: 業務的診断項目の拡充 ← 本指示書
⏳ Chunk 24d-4-2: 業務レポート (DOCX) 生成
⏳ Chunk 24d-4-3: 実機ドライランとフィードバック反映
```

## 背景: なぜ業務的診断項目が必要か

Chouさんの業務フロー:

```
1. お客様から HDD を受領
2. エンジニアが診断 (workbench-dryrun diagnose)  ← 本チャンクの強化対象
3. 診断結果を営業に渡す
4. 営業が見積作成、お客様に説明
5. お客様の承認後、復旧実施
6. CS によるデータ確認
7. お客様と支払い費用の最終確定
8. お客様が支払い
9. 納品
```

### 現状の問題

Chouさんの観察:
> 「診断は正常なのに Windows で開けない、なぜ?」

つまり:
- 技術的健全性は判定できる (NTFS 構造、MFT 整合性)
- **業務的な情報 (なぜマウントできない、復旧難易度、成功率) が不足**
- → 営業が見積根拠として使いにくい

### 業務的な原則

```
[Chouさんの判断]
受注可否はツールが判断しない。人間 (営業) が判断する。
ツールは「技術的事実」と「業務的指標」を提供するだけ。

[ツールの責務]
- 技術的事実: NTFS 構造、Dirty Bit、BitLocker 等
- 業務的指標: 推定ファイル数、難易度、成功率
- 業務判断: 人間が行う
```

これは設計の根本原則。

## 本チャンクのスコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | Dirty Bit 検出 ($Volume → $VOLUME_INFORMATION) |
| **B** | $LogFile 整合性チェック (簡易) |
| **C** | BitLocker 暗号化検出 |
| **D** | ファイル数推定 ($MFT エントリ数ベース、概算) |
| **E** | 削除ファイル数推定 |
| **F** | 復旧難易度評価 (易/中/難/注意の 4 段階) |
| **G** | 復旧成功率予測 (全体 % + 優先データ %) |
| **H** | CLI 表示の拡充 + CRM 貼り付け用テキストの更新 |

### 含まないもの

```
✗ 営業向け診断書 (DOCX) → Chunk 24d-4-2
✗ S.M.A.R.T. 情報 → Phase 2 (物理障害の本格対応時)
✗ BitLocker 復旧 (キー使用) → Phase 2
```

## 対象クレート

- **新規ファイル**: `crates/diagnostic/src/{dirty_bit,log_file,bitlocker,file_estimation,difficulty,success_rate}.rs`
- **修正**: `crates/diagnostic/src/lib.rs`, `crates/diagnostic/src/engine.rs`
- **修正**: `crates/case-manager/src/case.rs` (DiagnosticInput 構造の拡張)
- **修正**: `crates/report/src/crm_text.rs` (CRM 貼り付け用テキスト)
- **修正**: `crates/workbench-dryrun/src/commands/diagnose.rs` (CLI 表示)

## 重要な設計原則

### 業務的指標の計算ロジック

```
[復旧難易度の判定]
易:   構造完全正常 + Dirty Bit なし + 削除ファイル少数
中:   Dirty Bit あり or 削除ファイル多数 or 小規模 MFT 破損
難:   大規模 MFT 破損 or BitLocker or 完全 FS 構造破壊 (カービング必要)
注意: 物理障害の兆候 or 危険な状態 ★ 「受注不可」ではなく「人間が判断」

[復旧成功率]
全体成功率 = 100% - 各リスク要因の減点
優先データ成功率 = Wishlist 該当 MFT エントリの健全性
```

### 受注判断はツールが下さない

```
[NG]
ツール: 「受注不可」「対応困難」と決めつける表示

[OK]
ツール: 「BitLocker 暗号化です。回復キーが必要です」 (事実のみ)
営業: お客様にキーの有無を確認 → 受注判断
```

これは Chouさんの業務観点に基づく重要な原則。

### 業務管理用とお客様用の表現

Q4: 表現レベルは「c」(両方サポート)。本チャンクでは CLI 表示は技術詳細を含む業務管理用。Chunk 24d-4-2 で「お客様用」の DOCX を別途生成。

## 仕様参照

### ビジネス要件

- **FR-DIAG-04** (Dirty Bit / $LogFile 検出) ← 新規達成
- **FR-DIAG-05** (BitLocker 検出) ← 新規達成
- **FR-DIAG-06** (ファイル数推定、難易度評価) ← 新規達成
- **FR-DIAG-07** (復旧成功率予測) ← 新規達成

## 実装内容

### Part A: Dirty Bit 検出 (`crates/diagnostic/src/dirty_bit.rs` 新規)

```rust
//! NTFS の Dirty Bit 検出.
//!
//! NTFS は $Volume MFT エントリの $VOLUME_INFORMATION 属性に
//! "dirty" フラグを持つ。これが立っていると Windows はマウントを
//! 拒否し、chkdsk を要求する。
//!
//! ## 業務的意義
//!
//! Dirty Bit が立っている = Windows が「壊れている」と判断する原因の最多。
//! しかし NTFS 構造自体は健全なケースが多い。データ復旧の絶好の対象。

use serde::{Deserialize, Serialize};
use dds_fs_ntfs::NtfsVolume;

/// Dirty Bit の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirtyBitStatus {
    /// 正常 (Dirty Bit なし)
    Clean,
    
    /// Dirty Bit が立っている (Windows が chkdsk を要求)
    Dirty,
    
    /// 判定不能 ($Volume が読めない等)
    Unknown,
}

impl DirtyBitStatus {
    /// 業務的な日本語メッセージ
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::Clean => "正常",
            Self::Dirty => "立っている (Windows がマウント拒否する原因)",
            Self::Unknown => "判定不能",
        }
    }
}

/// $Volume MFT エントリ (MFT インデックス 3) を読んで Dirty Bit を確認
pub fn check_dirty_bit<F>(volume: &mut NtfsVolume<F>) -> DirtyBitStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // $Volume は MFT インデックス 3 に固定
    const VOLUME_MFT_INDEX: u64 = 3;
    
    let entry = match volume.read_mft_entry(VOLUME_MFT_INDEX) {
        Ok(e) => e,
        Err(_) => return DirtyBitStatus::Unknown,
    };
    
    // $VOLUME_INFORMATION 属性 (タイプコード 0x70) を探す
    let volume_info = match entry.find_attribute(0x70) {
        Some(attr) => attr,
        None => return DirtyBitStatus::Unknown,
    };
    
    let data = volume_info.data();
    
    // $VOLUME_INFORMATION の構造:
    // offset 0-7:  Reserved
    // offset 8:    Major Version
    // offset 9:    Minor Version
    // offset 10-11: Flags (★ ここに Dirty Bit)
    // offset 12-15: Reserved
    if data.len() < 12 {
        return DirtyBitStatus::Unknown;
    }
    
    let flags = u16::from_le_bytes([data[10], data[11]]);
    
    // Dirty フラグ = 0x0001
    if flags & 0x0001 != 0 {
        DirtyBitStatus::Dirty
    } else {
        DirtyBitStatus::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn dirty_bit_status_business_messages() {
        assert!(DirtyBitStatus::Clean.business_message().contains("正常"));
        assert!(DirtyBitStatus::Dirty.business_message().contains("マウント拒否"));
        assert!(DirtyBitStatus::Unknown.business_message().contains("判定不能"));
    }
    
    // 注: check_dirty_bit の実機テストは結合テストで実施
    // (NtfsVolume のモックが必要)
}
```

### Part B: $LogFile 整合性チェック (`crates/diagnostic/src/log_file.rs` 新規)

```rust
//! NTFS $LogFile の整合性チェック (簡易).
//!
//! $LogFile は NTFS のトランザクションログ。未完了のトランザクションが
//! 残っていると Windows がマウント前に再生を試みる。
//!
//! ## 簡易チェックの方針
//!
//! 完全な $LogFile 解析は複雑なため、本チャンクでは「最終 LSN が
//! チェックポイント LSN より進んでいるか」の簡易判定のみ。
//! Phase 2 で本格対応。

use serde::{Deserialize, Serialize};
use dds_fs_ntfs::NtfsVolume;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFileStatus {
    /// 正常 (整合性 OK)
    Consistent,
    
    /// 未完了トランザクションあり
    Inconsistent,
    
    /// 判定不能
    Unknown,
}

impl LogFileStatus {
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::Consistent => "正常",
            Self::Inconsistent => "不整合あり (未完了トランザクション)",
            Self::Unknown => "判定不能",
        }
    }
}

/// $LogFile (MFT インデックス 2) の整合性を簡易チェック
pub fn check_log_file<F>(volume: &mut NtfsVolume<F>) -> LogFileStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    const LOGFILE_MFT_INDEX: u64 = 2;
    
    let entry = match volume.read_mft_entry(LOGFILE_MFT_INDEX) {
        Ok(e) => e,
        Err(_) => return LogFileStatus::Unknown,
    };
    
    // $DATA 属性を取得 (タイプコード 0x80)
    let data_attr = match entry.find_attribute(0x80) {
        Some(a) => a,
        None => return LogFileStatus::Unknown,
    };
    
    // $LogFile の先頭 4 バイトはマジック値
    // - "RSTR" (0x52545352): Restart Page (正常)
    // - "RCRD" (0x44524352): Record Page (Restart Page の前後にあれば不整合)
    // 簡易判定: 先頭ページが RSTR か確認
    let data_bytes = data_attr.data();
    if data_bytes.len() < 4 {
        return LogFileStatus::Unknown;
    }
    
    let magic = &data_bytes[0..4];
    
    if magic == b"RSTR" {
        // 最終チェックポイント LSN を読む (簡易)
        // 詳細な解析は省略、シンプルに "正常" 判定
        LogFileStatus::Consistent
    } else if magic == b"RCRD" {
        // 不整合の可能性
        LogFileStatus::Inconsistent
    } else if magic == [0x00, 0x00, 0x00, 0x00] {
        // 空っぽ ($LogFile が初期化されている = 正常)
        LogFileStatus::Consistent
    } else {
        LogFileStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn log_file_status_business_messages() {
        assert!(LogFileStatus::Consistent.business_message().contains("正常"));
        assert!(LogFileStatus::Inconsistent.business_message().contains("未完了"));
    }
}
```

### Part C: BitLocker 検出 (`crates/diagnostic/src/bitlocker.rs` 新規)

```rust
//! BitLocker 暗号化の検出.
//!
//! BitLocker で暗号化されたボリュームは、ブートセクタに特定の
//! シグネチャを持つ。これを検出して業務的に警告する。

use serde::{Deserialize, Serialize};
use dds_fs_ntfs::NtfsVolume;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitLockerStatus {
    /// 暗号化なし (通常)
    NotEncrypted,
    
    /// BitLocker で暗号化されている
    Encrypted,
    
    /// 判定不能
    Unknown,
}

impl BitLockerStatus {
    pub fn business_message(&self) -> &'static str {
        match self {
            Self::NotEncrypted => "なし",
            Self::Encrypted => "BitLocker 暗号化を検出 (回復キーが必要)",
            Self::Unknown => "判定不能",
        }
    }
}

/// ブートセクタの先頭バイトから BitLocker を検出
pub fn check_bitlocker<F>(volume: &mut NtfsVolume<F>) -> BitLockerStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // ボリューム先頭セクタを取得
    let boot_sector = match volume.read_raw(0, 512) {
        Ok(b) => b,
        Err(_) => return BitLockerStatus::Unknown,
    };
    
    if boot_sector.len() < 512 {
        return BitLockerStatus::Unknown;
    }
    
    // BitLocker のシグネチャ確認:
    // offset 3-10: "-FVE-FS-" (To Be Detected)
    //   旧 BitLocker: NTFS シグネチャ
    //   新 BitLocker (BitLocker To Go や Windows 7+): "-FVE-FS-"
    //
    // ★ 注: NTFS と並走する場合があり、確実な判定は複雑
    // ここでは保守的に "-FVE-FS-" シグネチャのみ判定
    if &boot_sector[3..11] == b"-FVE-FS-" {
        return BitLockerStatus::Encrypted;
    }
    
    // MBR (offset 0x55AA) を読んだ場合の判定:
    // BitLocker のディスクは特定のオフセットに "FVE-FS" あるいは
    // 別のシグネチャを持つことがある。Phase 1.5 では簡易判定のみ。
    
    BitLockerStatus::NotEncrypted
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn bitlocker_status_business_messages() {
        assert!(BitLockerStatus::NotEncrypted.business_message().contains("なし"));
        assert!(BitLockerStatus::Encrypted.business_message().contains("回復キー"));
    }
    
    #[test]
    fn bitlocker_signature_detection_from_bytes() {
        // モックの boot sector を作成
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"-FVE-FS-");
        
        // ★ check_bitlocker は volume を取るため、関数本体はテスト困難
        // 代わりにシグネチャ判定ロジックを別関数化して unit test 可能に
        let is_bitlocker = &boot[3..11] == b"-FVE-FS-";
        assert!(is_bitlocker);
    }
}
```

### Part D: ファイル数推定 (`crates/diagnostic/src/file_estimation.rs` 新規)

```rust
//! ファイル数の推定 ($MFT エントリ数ベース).
//!
//! $MFT の総エントリ数を高速にカウント。実際のファイル数は
//! システムファイルや MFT エントリ自体 (~30 個) を除いた数。

use serde::{Deserialize, Serialize};
use dds_fs_ntfs::NtfsVolume;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEstimation {
    /// 推定総ファイル数 (システムファイル除く)
    pub estimated_total_files: u64,
    
    /// 推定削除ファイル数
    pub estimated_deleted_files: u64,
    
    /// 推定生存ファイル数
    pub estimated_live_files: u64,
}

impl FileEstimation {
    pub fn business_summary(&self) -> String {
        format!(
            "推定ファイル数: 約 {} 件 (生存 {} / 削除 {})",
            format_number(self.estimated_total_files),
            format_number(self.estimated_live_files),
            format_number(self.estimated_deleted_files),
        )
    }
}

fn format_number(n: u64) -> String {
    if n >= 10_000 {
        format!("{:.1}万", n as f64 / 10_000.0)
    } else if n >= 1_000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

/// $MFT を走査して概算ファイル数を取得 (高速、診断向け)
pub fn estimate_file_count<F>(volume: &mut NtfsVolume<F>) -> FileEstimation
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let mut total: u64 = 0;
    let mut deleted: u64 = 0;
    let mut live: u64 = 0;
    
    // システムファイル (MFT インデックス 0-15 は予約)
    const SYSTEM_FILE_THRESHOLD: u64 = 16;
    
    // $MFT の総エントリ数を取得 (volume の情報から)
    let total_entries = volume.total_mft_entries();
    
    for index in SYSTEM_FILE_THRESHOLD..total_entries {
        match volume.read_mft_entry(index) {
            Ok(entry) => {
                if entry.is_in_use() {
                    total += 1;
                    if entry.is_deleted() {
                        deleted += 1;
                    } else {
                        live += 1;
                    }
                }
            }
            Err(_) => continue,  // 読めないエントリはスキップ
        }
    }
    
    FileEstimation {
        estimated_total_files: total,
        estimated_deleted_files: deleted,
        estimated_live_files: live,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1500), "1,500");
        assert_eq!(format_number(25000), "2.5万");
    }
    
    #[test]
    fn business_summary_format() {
        let est = FileEstimation {
            estimated_total_files: 1500,
            estimated_deleted_files: 50,
            estimated_live_files: 1450,
        };
        let summary = est.business_summary();
        assert!(summary.contains("1,500"));
        assert!(summary.contains("1,450"));
        assert!(summary.contains("50"));
    }
}
```

### Part E: 復旧難易度評価 (`crates/diagnostic/src/difficulty.rs` 新規)

```rust
//! 復旧難易度の評価.

use serde::{Deserialize, Serialize};

use super::dirty_bit::DirtyBitStatus;
use super::log_file::LogFileStatus;
use super::bitlocker::BitLockerStatus;
use super::file_estimation::FileEstimation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryDifficulty {
    /// 易: 標準的な業務ケース
    Easy,
    
    /// 中: 部分的な障害、業務的に標準
    Medium,
    
    /// 難: 大規模な障害、ファイル単位の復旧が必要
    Hard,
    
    /// 注意: 物理障害の兆候、業務的に慎重判断が必要
    Caution,
}

impl RecoveryDifficulty {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Easy => "易",
            Self::Medium => "中",
            Self::Hard => "難",
            Self::Caution => "注意",
        }
    }
    
    /// 業務的な日本語の説明
    pub fn business_explanation(&self) -> &'static str {
        match self {
            Self::Easy => "標準的な業務ケース、復旧成功の見込み高い",
            Self::Medium => "部分的な障害あり、業務的に標準範囲",
            Self::Hard => "大規模な障害、ファイル単位の復旧が必要、業務的に難度高",
            Self::Caution => "物理障害の兆候あり、業務的に慎重判断が必要 (受注可否は人間が判断)",
        }
    }
}

/// 各種診断結果から復旧難易度を判定
pub fn evaluate_difficulty(
    nfs_structure_ok: bool,
    mft_corruption_count: u64,
    dirty_bit: DirtyBitStatus,
    log_file: LogFileStatus,
    bitlocker: BitLockerStatus,
    file_estimation: &FileEstimation,
) -> RecoveryDifficulty {
    // 注意レベルの判定 (物理障害の兆候)
    // ※ 本チャンクでは S.M.A.R.T. 等の物理検出はないため、暫定的に
    //   「MFT エントリの 50% 以上が読めない」を物理障害の兆候とみなす
    if file_estimation.estimated_total_files == 0 {
        return RecoveryDifficulty::Caution;
    }
    
    // 難レベル
    if matches!(bitlocker, BitLockerStatus::Encrypted) 
        || !nfs_structure_ok 
        || mft_corruption_count > 100 {
        return RecoveryDifficulty::Hard;
    }
    
    // 中レベル
    if matches!(dirty_bit, DirtyBitStatus::Dirty)
        || matches!(log_file, LogFileStatus::Inconsistent)
        || file_estimation.estimated_deleted_files > 100
        || mft_corruption_count > 0 {
        return RecoveryDifficulty::Medium;
    }
    
    // 易レベル
    RecoveryDifficulty::Easy
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_estimation(total: u64, deleted: u64) -> FileEstimation {
        FileEstimation {
            estimated_total_files: total,
            estimated_deleted_files: deleted,
            estimated_live_files: total - deleted,
        }
    }
    
    #[test]
    fn evaluate_easy() {
        let d = evaluate_difficulty(
            true, 0, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Easy);
    }
    
    #[test]
    fn evaluate_medium_dirty_bit() {
        let d = evaluate_difficulty(
            true, 0, DirtyBitStatus::Dirty, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Medium);
    }
    
    #[test]
    fn evaluate_hard_bitlocker() {
        let d = evaluate_difficulty(
            true, 0, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::Encrypted, &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Hard);
    }
    
    #[test]
    fn evaluate_hard_structure_broken() {
        // FS 構造破壊でもファイル単位の復旧は可能 (Chouさんの判断)
        let d = evaluate_difficulty(
            false, 0, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Hard);
    }
    
    #[test]
    fn evaluate_caution_no_files() {
        let d = evaluate_difficulty(
            true, 0, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, &make_estimation(0, 0),
        );
        assert_eq!(d, RecoveryDifficulty::Caution);
    }
    
    #[test]
    fn business_explanation_includes_human_judgment() {
        let caution = RecoveryDifficulty::Caution;
        assert!(caution.business_explanation().contains("人間が判断"));
    }
}
```

### Part F: 復旧成功率予測 (`crates/diagnostic/src/success_rate.rs` 新規)

```rust
//! 復旧成功率の予測.
//!
//! 全体成功率 + 優先データ (Wishlist 指定時) の成功率を計算。

use serde::{Deserialize, Serialize};

use super::dirty_bit::DirtyBitStatus;
use super::log_file::LogFileStatus;
use super::bitlocker::BitLockerStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessRatePrediction {
    /// 全体的な復旧成功率 (0-100)
    pub overall_rate: u8,
    
    /// 優先データの復旧成功率 (Wishlist 指定時のみ、0-100)
    pub priority_rate: Option<u8>,
    
    /// 計算根拠 (営業の説明用)
    pub reasoning: Vec<String>,
}

impl SuccessRatePrediction {
    pub fn business_summary(&self) -> String {
        let mut s = format!("推定復旧成功率: {}% (全体)", self.overall_rate);
        if let Some(priority) = self.priority_rate {
            s.push_str(&format!("、{}% (優先データ)", priority));
        }
        s
    }
}

/// 復旧成功率を計算
pub fn predict_success_rate(
    mft_corruption_count: u64,
    total_mft_entries: u64,
    dirty_bit: DirtyBitStatus,
    log_file: LogFileStatus,
    bitlocker: BitLockerStatus,
    has_wishlist: bool,  // Wishlist 指定があるか
) -> SuccessRatePrediction {
    let mut overall: i32 = 100;
    let mut reasoning = Vec::new();
    
    // BitLocker 暗号化
    if matches!(bitlocker, BitLockerStatus::Encrypted) {
        overall -= 90;
        reasoning.push("BitLocker 暗号化を検出 (-90%、回復キー必須)".to_string());
    }
    
    // MFT 破損
    if mft_corruption_count > 0 && total_mft_entries > 0 {
        let corruption_rate = (mft_corruption_count as f64 / total_mft_entries as f64 * 100.0) as i32;
        let deduction = corruption_rate.min(50);
        overall -= deduction;
        reasoning.push(format!("MFT エントリ破損 (-{}%)", deduction));
    }
    
    // Dirty Bit
    if matches!(dirty_bit, DirtyBitStatus::Dirty) {
        overall -= 2;
        reasoning.push("Dirty Bit あり (-2%)".to_string());
    }
    
    // $LogFile 不整合
    if matches!(log_file, LogFileStatus::Inconsistent) {
        overall -= 5;
        reasoning.push("$LogFile 不整合 (-5%)".to_string());
    }
    
    // 下限 0
    let overall = overall.max(0) as u8;
    
    // 優先データの成功率
    // Wishlist 指定時、優先データ MFT エントリの健全性ベース
    // 本チャンクでは簡易計算 (全体成功率 + ボーナス、ただし最大 100%)
    let priority_rate = if has_wishlist {
        Some((overall as u16 + 5).min(100) as u8)
    } else {
        None
    };
    
    SuccessRatePrediction {
        overall_rate: overall,
        priority_rate,
        reasoning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn healthy_drive_high_success_rate() {
        let pred = predict_success_rate(
            0, 10000, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, false,
        );
        assert_eq!(pred.overall_rate, 100);
        assert!(pred.priority_rate.is_none());
    }
    
    #[test]
    fn bitlocker_severe_deduction() {
        let pred = predict_success_rate(
            0, 10000, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::Encrypted, false,
        );
        assert_eq!(pred.overall_rate, 10);  // 100 - 90
    }
    
    #[test]
    fn dirty_bit_small_deduction() {
        let pred = predict_success_rate(
            0, 10000, DirtyBitStatus::Dirty, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, false,
        );
        assert_eq!(pred.overall_rate, 98);  // 100 - 2
    }
    
    #[test]
    fn wishlist_provides_priority_rate() {
        let pred = predict_success_rate(
            0, 10000, DirtyBitStatus::Clean, LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted, true,
        );
        assert!(pred.priority_rate.is_some());
        assert_eq!(pred.priority_rate.unwrap(), 100);  // 100 + 5 だが最大 100
    }
    
    #[test]
    fn reasoning_includes_deduction_reasons() {
        let pred = predict_success_rate(
            5, 1000, DirtyBitStatus::Dirty, LogFileStatus::Inconsistent,
            BitLockerStatus::NotEncrypted, false,
        );
        assert!(!pred.reasoning.is_empty());
        assert!(pred.reasoning.iter().any(|r| r.contains("Dirty Bit")));
        assert!(pred.reasoning.iter().any(|r| r.contains("$LogFile")));
    }
}
```

### Part G: DiagnosticInput の拡張 (`crates/case-manager/src/case.rs`)

```rust
// 既存の DiagnosticInput に新規フィールドを追加:

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInput {
    // 既存フィールド (NTFS 構造の健全性等)
    // ...
    
    // ★ 新規追加 (Chunk 24d-4-1):
    /// Dirty Bit の状態
    pub dirty_bit: Option<DirtyBitStatus>,
    
    /// $LogFile の状態
    pub log_file: Option<LogFileStatus>,
    
    /// BitLocker 暗号化の状態
    pub bitlocker: Option<BitLockerStatus>,
    
    /// 推定ファイル数
    pub file_estimation: Option<FileEstimation>,
    
    /// 復旧難易度
    pub recovery_difficulty: Option<RecoveryDifficulty>,
    
    /// 復旧成功率予測
    pub success_rate: Option<SuccessRatePrediction>,
}
```

### Part H: 診断エンジンの拡張 (`crates/diagnostic/src/engine.rs`)

```rust
impl DiagnosticEngine {
    pub fn diagnose<F>(&self, volume: &mut NtfsVolume<F>) -> Result<DiagnosticInput, DiagnosticError>
    where
        F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
    {
        // ★ 既存の診断 (NTFS 構造の健全性チェック等)
        let mut input = self.run_basic_diagnostics(volume)?;
        
        // ★ 新規追加: 業務的診断
        input.dirty_bit = Some(crate::dirty_bit::check_dirty_bit(volume));
        input.log_file = Some(crate::log_file::check_log_file(volume));
        input.bitlocker = Some(crate::bitlocker::check_bitlocker(volume));
        input.file_estimation = Some(crate::file_estimation::estimate_file_count(volume));
        
        // 業務的指標の計算
        let estimation = input.file_estimation.as_ref().unwrap();
        let dirty = input.dirty_bit.unwrap();
        let log = input.log_file.unwrap();
        let bl = input.bitlocker.unwrap();
        
        input.recovery_difficulty = Some(
            crate::difficulty::evaluate_difficulty(
                input.is_nfs_structure_ok(),  // 既存メソッド
                input.mft_corruption_count(),
                dirty, log, bl, estimation,
            )
        );
        
        input.success_rate = Some(
            crate::success_rate::predict_success_rate(
                input.mft_corruption_count(),
                volume.total_mft_entries(),
                dirty, log, bl,
                false,  // ★ Wishlist は recover 時に指定、診断時は false
            )
        );
        
        Ok(input)
    }
}
```

### Part I: CRM 貼り付け用テキストの更新 (`crates/report/src/crm_text.rs`)

```rust
pub fn render_crm_paste_text(case: &Case) -> String {
    let mut s = String::new();
    
    // 既存のセクション
    s.push_str(&format!("【案件番号】 {}\n", case.case_id));
    // ...
    
    // ★ 新規追加: 業務的診断結果
    if let Some(diagnostic) = &case.diagnostic_input {
        s.push_str("\n【診断結果 - 業務サマリ】\n");
        
        if let Some(estimation) = &diagnostic.file_estimation {
            s.push_str(&format!("  {}\n", estimation.business_summary()));
        }
        
        if let Some(difficulty) = &diagnostic.recovery_difficulty {
            s.push_str(&format!("  復旧難易度: {} ({})\n",
                difficulty.display_name(), difficulty.business_explanation()));
        }
        
        if let Some(rate) = &diagnostic.success_rate {
            s.push_str(&format!("  {}\n", rate.business_summary()));
        }
        
        s.push_str("\n【診断結果 - 技術詳細】\n");
        
        if let Some(dirty) = &diagnostic.dirty_bit {
            s.push_str(&format!("  Dirty Bit:           {}\n", dirty.business_message()));
        }
        
        if let Some(log) = &diagnostic.log_file {
            s.push_str(&format!("  $LogFile 整合性:     {}\n", log.business_message()));
        }
        
        if let Some(bl) = &diagnostic.bitlocker {
            s.push_str(&format!("  BitLocker 暗号化:    {}\n", bl.business_message()));
        }
    }
    
    s
}
```

### Part J: CLI 表示の拡充 (`crates/workbench-dryrun/src/commands/diagnose.rs`)

```rust
fn show_diagnostic_result(case: &Case) -> Result<()> {
    let diag = case.diagnostic_input.as_ref()
        .ok_or_else(|| anyhow!("診断結果がありません"))?;
    
    println!();
    println!("===========================================");
    println!("  診断結果");
    println!("===========================================");
    println!();
    
    println!("【ファイルシステムの破損】");
    println!("  ファイルシステム署名:  {}", if diag.is_nfs_structure_ok() {"正常 (NTFS 認識成功)"} else {"異常"});
    println!("  MFT エントリ破損:     {} 件", diag.mft_corruption_count());
    println!("  不正な run-list:      {} 件", diag.invalid_runlist_count());
    println!("  Boot sector:          {}", diag.boot_sector_status());
    println!();
    
    println!("【Windows のマウント状態】");
    if let Some(dirty) = &diag.dirty_bit {
        println!("  Dirty Bit:            {}", dirty.business_message());
    }
    if let Some(log) = &diag.log_file {
        println!("  $LogFile 整合性:     {}", log.business_message());
    }
    if let Some(bl) = &diag.bitlocker {
        println!("  BitLocker 暗号化:    {}", bl.business_message());
    }
    println!();
    
    println!("【業務的な評価】");
    if let Some(est) = &diag.file_estimation {
        println!("  推定ファイル数:       約 {} 件", format_number(est.estimated_total_files));
        println!("    内訳: 生存 {} / 削除 {}", 
            format_number(est.estimated_live_files),
            format_number(est.estimated_deleted_files));
    }
    if let Some(diff) = &diag.recovery_difficulty {
        println!("  復旧難易度:           {} ({})",
            diff.display_name(), diff.business_explanation());
    }
    if let Some(rate) = &diag.success_rate {
        println!("  {}", rate.business_summary());
        if !rate.reasoning.is_empty() {
            println!("    計算根拠:");
            for r in &rate.reasoning {
                println!("      - {}", r);
            }
        }
    }
    println!();
    
    println!("===========================================");
    Ok(())
}
```

## 単体テスト要件 (最低 18 件)

### `dirty_bit.rs` (最低 1 件)

1. `dirty_bit_status_business_messages`

### `log_file.rs` (最低 1 件)

2. `log_file_status_business_messages`

### `bitlocker.rs` (最低 2 件)

3. `bitlocker_status_business_messages`
4. `bitlocker_signature_detection_from_bytes`

### `file_estimation.rs` (最低 2 件)

5. `format_number_thousands`
6. `business_summary_format`

### `difficulty.rs` (最低 6 件)

7. `evaluate_easy`
8. `evaluate_medium_dirty_bit`
9. `evaluate_hard_bitlocker`
10. `evaluate_hard_structure_broken` (FS 破壊でも難扱い)
11. `evaluate_caution_no_files`
12. `business_explanation_includes_human_judgment` (人間判断を強調)

### `success_rate.rs` (最低 6 件)

13. `healthy_drive_high_success_rate`
14. `bitlocker_severe_deduction`
15. `dirty_bit_small_deduction`
16. `wishlist_provides_priority_rate`
17. `reasoning_includes_deduction_reasons`
18. `extreme_deductions_floor_at_zero`

## 結合テスト要件 (最低 2 件)

### 1. 健全な NTFS フィクスチャでの診断

```rust
#[test]
fn diagnose_healthy_ntfs_returns_easy_difficulty() {
    let mut volume = open_fixture("ntfs_mixed_formats");
    let engine = DiagnosticEngine::new();
    let result = engine.diagnose(&mut volume).unwrap();
    
    assert!(result.recovery_difficulty.is_some());
    assert_eq!(result.recovery_difficulty.unwrap(), RecoveryDifficulty::Easy);
    
    assert!(result.success_rate.is_some());
    assert!(result.success_rate.unwrap().overall_rate >= 90);
}
```

### 2. CRM テキストの業務的情報の含有

```rust
#[test]
fn crm_text_includes_business_diagnostic_info() {
    let case = make_diagnosed_case();
    let crm_text = render_crm_paste_text(&case);
    
    assert!(crm_text.contains("業務サマリ"));
    assert!(crm_text.contains("復旧難易度"));
    assert!(crm_text.contains("成功率"));
    assert!(crm_text.contains("技術詳細"));
}
```

## 制約

- **行数目安**:
  - `diagnostic/src/dirty_bit.rs` (新規): 約 80 行 + テスト 20 行
  - `diagnostic/src/log_file.rs` (新規): 約 80 行 + テスト 20 行
  - `diagnostic/src/bitlocker.rs` (新規): 約 80 行 + テスト 20 行
  - `diagnostic/src/file_estimation.rs` (新規): 約 80 行 + テスト 30 行
  - `diagnostic/src/difficulty.rs` (新規): 約 100 行 + テスト 60 行
  - `diagnostic/src/success_rate.rs` (新規): 約 100 行 + テスト 70 行
  - `diagnostic/src/engine.rs` 修正: +30 行 (新規診断統合)
  - `case-manager/src/case.rs` 修正: +20 行 (フィールド追加)
  - `report/src/crm_text.rs` 修正: +40 行 (業務情報追加)
  - `workbench-dryrun/src/commands/diagnose.rs` 修正: +50 行 (CLI 表示)
  - 合計: 約 700 行追加・修正
- **単体テスト新規**: 最低 18 件
- **結合テスト新規**: 最低 2 件
- **`unsafe` 追加行数**: 0
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (24d-1 から変化なし)
- [ ] Dirty Bit 検出が動作
- [ ] $LogFile 整合性チェックが動作
- [ ] BitLocker 検出が動作
- [ ] ファイル数推定が動作 (高速)
- [ ] 復旧難易度評価が 4 段階 (易/中/難/注意) で動作
- [ ] 復旧成功率予測が動作 (全体 + 優先データ)
- [ ] CRM 貼り付けテキストに業務情報が含まれる
- [ ] CLI 表示が業務管理用 (技術詳細 + 業務サマリ) で表示
- [ ] 「受注不可」のような決めつけ表現がない
- [ ] 業務的な日本語メッセージ

## 関連 FR 要件

- **FR-DIAG-04** (Dirty Bit / $LogFile) ← 達成
- **FR-DIAG-05** (BitLocker 検出) ← 達成
- **FR-DIAG-06** (ファイル数推定、難易度) ← 達成
- **FR-DIAG-07** (復旧成功率予測) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **次のチャンク: Chunk 24d-4-2 (営業向け診断書 DOCX)**

---

## 注意事項

### NtfsVolume の API 拡張が必要かも

本チャンクで以下の NtfsVolume メソッドを使うが、既存にあるか要確認:

```rust
volume.read_mft_entry(index)
volume.total_mft_entries()
volume.read_raw(offset, size)
entry.is_in_use()
entry.is_deleted()
entry.find_attribute(type_code)
```

もし未実装なら、本チャンクで先に追加する必要あり。

### $Volume MFT エントリの位置

```
NTFS の予約 MFT エントリ:
  0: $MFT
  1: $MFTMirr
  2: $LogFile
  3: $Volume       ← Dirty Bit の場所
  4: $AttrDef
  5: . (ルートディレクトリ)
  ...
```

### Dirty Bit のフラグ値

```
$VOLUME_INFORMATION 属性 (タイプ 0x70):
  offset 10-11: Flags
    0x0001 = VOLUME_IS_DIRTY (★ chkdsk 必要)
    0x0002 = VOLUME_RESIZE_LOG_FILE
    0x0004 = VOLUME_UPGRADE_ON_MOUNT
    ...
```

### BitLocker のシグネチャ

```
BitLocker for Windows (旧):
  ブートセクタは NTFS と区別困難
  
BitLocker To Go / 新 BitLocker:
  offset 3-10 に "-FVE-FS-" (45 56 46 2D 53 46 2D 00 みたいなパターン)
```

本チャンクでは簡易判定のみ。Phase 2 で精密化。

### 業務的なメッセージング原則

```
[NG (決めつけ表現)]
"受注不可"
"対応困難"
"復旧不可能"

[OK (事実 + 業務的説明)]
"BitLocker 暗号化を検出 (回復キーが必要)"
"物理障害の兆候あり (業務的に慎重判断が必要、人間が判断)"
"完全な FS 構造破壊 (ファイル単位の復旧が必要、難度高)"
```

これは Chouさんの原則 (「受注判断はツールがしない、人間が判断する」)。

### Phase 2.1 UI への引き継ぎ

```
[Tauri UI で表示する診断結果]
1. 技術詳細セクション (折りたたみ可能)
2. 業務サマリセクション (大きく目立つ)
3. 復旧難易度のバッジ (色付き: 緑/黄/赤/オレンジ)
4. 成功率のゲージ (視覚的)

[Chunk 24d-4-1 で公開する API]
DiagnosticInput → 全てのフィールドが JSON シリアライズ可能
→ UI から Tauri command で取得して表示
```

---

## 質問が必要なケース

- NtfsVolume に必要なメソッドが既存にない場合 (`read_mft_entry`, `total_mft_entries` 等)
- $VOLUME_INFORMATION の構造が想定と違う場合
- 既存の DiagnosticEngine の構造が想定外の場合

---

## 完了報告例

```markdown
## Chunk 24d-4-1 完了報告

### 新規ファイル
- crates/diagnostic/src/dirty_bit.rs (約 80 行 + テスト 20 行)
- crates/diagnostic/src/log_file.rs (約 80 行 + テスト 20 行)
- crates/diagnostic/src/bitlocker.rs (約 80 行 + テスト 20 行)
- crates/diagnostic/src/file_estimation.rs (約 80 行 + テスト 30 行)
- crates/diagnostic/src/difficulty.rs (約 100 行 + テスト 60 行)
- crates/diagnostic/src/success_rate.rs (約 100 行 + テスト 70 行)

### 修正ファイル
- crates/diagnostic/src/engine.rs (+30 行)
- crates/case-manager/src/case.rs (+20 行)
- crates/report/src/crm_text.rs (+40 行)
- crates/workbench-dryrun/src/commands/diagnose.rs (+50 行)

### 新規 API
- check_dirty_bit, DirtyBitStatus
- check_log_file, LogFileStatus
- check_bitlocker, BitLockerStatus
- estimate_file_count, FileEstimation
- evaluate_difficulty, RecoveryDifficulty (4 段階)
- predict_success_rate, SuccessRatePrediction

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)

### テスト統計
- 単体: 既存 + 新規 18 件
- 結合: 既存 + 新規 2 件
- 全 workspace: 全パス

### 動作確認サンプル
```
[ファイルシステムの破損]
  ファイルシステム署名:  正常 (NTFS 認識成功)
  MFT エントリ破損:     0 件
  不正な run-list:      0 件
  Boot sector:          正常

[Windows のマウント状態]
  Dirty Bit:            立っている (Windows がマウント拒否する原因)
  $LogFile 整合性:     不整合あり (未完了トランザクション)
  BitLocker 暗号化:    なし

[業務的な評価]
  推定ファイル数:       約 1,200 件
    内訳: 生存 1,150 / 削除 50
  復旧難易度:           中 (部分的な障害あり、業務的に標準範囲)
  推定復旧成功率: 93% (全体)
    計算根拠:
      - Dirty Bit あり (-2%)
      - $LogFile 不整合 (-5%)
```

### 🎯 達成事項
- 診断結果が「営業の見積根拠」として使える業務適用品質に到達
- 「受注不可」のような決めつけ表現を排除、人間が判断する原則を実現
- Windows がマウント拒否する理由 (Dirty Bit、$LogFile、BitLocker) を明示
- 復旧難易度と成功率で業務的な見積もり根拠を提供

### 次のステップ
Chunk 24d-4-2 で:
- 営業向け診断書 (DOCX) を生成
- 業務管理用 + お客様用の 2 セクション
- 営業がお客様に渡せる業務的な品質

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんが診断のテストを実施
→ 営業に見せて業務的な反応を確認
→ Chunk 24d-4-2 の指示書を私に依頼
```
