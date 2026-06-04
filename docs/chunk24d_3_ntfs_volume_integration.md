# Chunk 24d-3 指示: NtfsVolume との統合 + diagnose/recover の --physical 対応

Phase 1.5 拡張の **第 3 段階、最重要チャンク**。物理パーティションを既存の NtfsVolume API と統合し、`diagnose --physical` と `recover --physical` を実装する。

> 🎯 完了時点で「壊れた FS の HDD から実際に復旧できる」状態に到達。Phase 1.5 拡張の業務的価値が実現する。

---

## 全体像 (Chunk 24d シリーズ)

```
✅ Chunk 24d-1: 物理ディスクアクセス層 (完了)
✅ Chunk 24d-2: パーティションテーブル解析 (完了)
🚧 Chunk 24d-3: NtfsVolume との統合 ← 本指示書、最重要
⏳ Chunk 24d-4: 実機ドライランとフィードバック反映
```

## 本チャンクの業務的意義

```
[これまでの状態]
✓ 物理ドライブが見える (24d-1)
✓ パーティション構造が見える (24d-2)
✗ 実際に復旧はできない

[Chunk 24d-3 完了後]
✓ 物理パーティションから NtfsVolume を open
✓ diagnose --physical で診断
✓ recover --physical で復旧
✓ 壊れた FS の HDD から実際に復旧できる
→ Phase 1.5 拡張の業務的価値が実現
```

## スコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | `PhysicalPartitionReader` (物理パーティション → NtfsVolume の reader) |
| **B** | `diagnose --physical` コマンドの実装 |
| **C** | `recover --physical` コマンドの実装 |
| **D** | パーティション選択 UX (対話形式) |
| **E** | エラーハンドリング (壊れた NTFS の検出と業務的メッセージ) |

### 含まないもの

```
✗ 不良セクタ対応 (Phase 2 で本格対応、24d-3 では「読めなければエラー」)
✗ FAT32 / exFAT の復旧 (Phase 1.5 は NTFS のみ)
✗ 暗号化 NTFS の対応 (Phase 2 以降)
✗ 動的ディスクの対応 (Phase 2 以降)
```

## 対象クレート

- **新規ファイル**: `crates/disk-io/src/physical_partition.rs`
- **修正**: `crates/disk-io/src/lib.rs`
- **修正**: `crates/workbench-dryrun/src/commands/diagnose.rs`
- **修正**: `crates/workbench-dryrun/src/commands/recover.rs`
- **修正**: `crates/workbench-dryrun/src/commands/list_drives.rs` (パーティション選択時のヒント追加)

## 重要な設計原則

### NtfsVolume の reader 抽象化を活用

既存の `NtfsVolume<F>` は `F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>` を受け取る設計。これは Chunk 1-2 で確立された抽象化。

```rust
// 既存の NtfsVolume
pub struct NtfsVolume<F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    reader: F,
    // ...
}
```

物理パーティションも `F` の closure として渡すだけで、既存の NtfsVolume が動く。

```rust
// 論理ドライブの場合 (既存):
let volume = open_ntfs_volume(r"\\.\E:")?;
// 内部で File::open + FileReader を作る

// 物理パーティションの場合 (新規):
let physical_drive = PhysicalDrive::open(r"\\.\PhysicalDrive1")?;
let partition_reader = PhysicalPartitionReader::new(physical_drive, partition_offset);
let volume = NtfsVolume::new(partition_reader.into_closure())?;
```

これで既存の復旧ロジックがそのまま動く。

### 業務的なエラーメッセージ

```
[壊れた NTFS の検出]
- ブートセクタは NTFS シグネチャあり (24d-2 で検出)
- しかし $MFT が読めない / 破損している
- 業務メンバーへの分かりやすい説明が必要

[エラーメッセージ例]
× "NTFS volume open failed: invalid mft signature"
○ "NTFS の管理領域 ($MFT) が破損している可能性があります。
   別ツール (R-STUDIO 等) での復旧をご検討ください。"
```

業務メンバーが「次に何をすべきか」分かるメッセージにする。

### 既存 API への影響を最小化

```
[既存]
diagnose --drive E (論理ドライブモード、既存)
recover --drive E (既存)

[追加]
diagnose --physical N --partition M (新規)
recover --physical N --partition M (新規)

両方共存。既存テストへの破壊的変更なし。
```

## 仕様参照

### ビジネス要件

- **FR-PHY-06** (物理パーティションからの NtfsVolume open) ← 達成
- **FR-PHY-07** (diagnose/recover の --physical 対応) ← 達成
- **FR-PHY-08** (壊れた NTFS の業務的なエラー表示) ← 達成

## 実装内容

### Part A: `crates/disk-io/src/physical_partition.rs` (新規ファイル)

```rust
//! 物理ドライブのパーティションを NtfsVolume の reader として使うアダプタ.
//!
//! 既存の NtfsVolume は `F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>`
//! という reader closure を受け取る。物理パーティションへのアクセスを
//! この interface に合わせる。
//!
//! ## 動作
//!
//! NtfsVolume はパーティション内の論理オフセット (0 から) で read を要求する。
//! `PhysicalPartitionReader` はそのオフセットに `partition_start_offset` を
//! 加算して、物理ドライブの絶対オフセットに変換する。

use std::io;
use std::sync::{Arc, Mutex};

use crate::physical::PhysicalDrive;

/// 物理パーティション内のオフセットを物理ドライブの絶対オフセットに変換する reader.
///
/// NtfsVolume の closure として渡すために使う。
pub struct PhysicalPartitionReader {
    drive: Arc<Mutex<PhysicalDrive>>,
    partition_start_offset: u64,
    partition_size: u64,
}

impl PhysicalPartitionReader {
    /// 新規作成
    ///
    /// - `drive`: 物理ドライブ (Arc<Mutex<...>> で複数の reader 間で共有)
    /// - `partition_start_offset`: パーティション開始の絶対オフセット (バイト)
    /// - `partition_size`: パーティションサイズ (バイト)
    pub fn new(drive: PhysicalDrive, partition_start_offset: u64, partition_size: u64) -> Self {
        Self {
            drive: Arc::new(Mutex::new(drive)),
            partition_start_offset,
            partition_size,
        }
    }
    
    /// NtfsVolume の reader closure に変換する
    pub fn into_closure(self) -> impl FnMut(u64, u64) -> Result<Vec<u8>, io::Error> {
        let drive = self.drive.clone();
        let start = self.partition_start_offset;
        let size = self.partition_size;
        
        move |offset: u64, length: u64| -> Result<Vec<u8>, io::Error> {
            // パーティション境界チェック
            if offset >= size {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("オフセットがパーティション境界を超えています: offset={}, partition_size={}",
                        offset, size),
                ));
            }
            
            // 読み取り長を境界内に制限
            let max_len = size - offset;
            let read_len = length.min(max_len) as usize;
            
            if read_len == 0 {
                return Ok(Vec::new());
            }
            
            // 絶対オフセットに変換
            let absolute_offset = start.checked_add(offset)
                .ok_or_else(|| io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "オフセット計算でオーバーフロー",
                ))?;
            
            // 物理ドライブから読む
            let drive_guard = drive.lock().map_err(|_| io::Error::new(
                io::ErrorKind::Other,
                "drive lock poisoned",
            ))?;
            
            drive_guard.read_at(absolute_offset, read_len)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::{PhysicalDrive, PhysicalDriveInfo, BusType};
    use std::path::PathBuf;
    
    // 注: PhysicalDrive は実機のハンドルを保持するため、ユニットテストは困難。
    // 結合テストで実機の物理ドライブを使う、または NtfsVolume の closure 抽象化を
    // 別途モックで検証する。
    //
    // ここでは「コンセプト的なテスト」のみ。
    // 実機テストは Chunk 24d-4 で実施。
    
    #[test]
    fn partition_reader_offset_calculation_concept() {
        // パーティションが offset 1MB から始まる場合:
        // パーティション内 offset 100 → 物理 offset 1MB + 100
        let partition_start: u64 = 1024 * 1024;  // 1 MB
        let logical_offset: u64 = 100;
        let absolute_offset = partition_start + logical_offset;
        
        assert_eq!(absolute_offset, 1024 * 1024 + 100);
    }
    
    #[test]
    fn partition_reader_boundary_check_concept() {
        let partition_size: u64 = 1024;  // 1 KB のパーティション
        
        // 正常な範囲
        let offset: u64 = 500;
        let length: u64 = 200;
        assert!(offset + length <= partition_size);
        
        // 境界を超える
        let offset: u64 = 900;
        let length: u64 = 200;
        let max_len = partition_size - offset;  // 124 バイトまで読める
        let actual_len = length.min(max_len);
        assert_eq!(actual_len, 124);
    }
}
```

### Part B: `crates/disk-io/src/lib.rs` への追加

```rust
// 既存:
pub mod logical;
pub mod physical;
pub mod partition;
pub mod fs_detection;

// 新規追加:
pub mod physical_partition;

// 公開 API:
pub use physical_partition::PhysicalPartitionReader;
```

### Part C: workbench-dryrun の diagnose --physical 実装

`crates/workbench-dryrun/src/commands/diagnose.rs` の修正:

```rust
use clap::Args;
use anyhow::{anyhow, Context, Result};
use dds_disk_io::{
    PhysicalDrive, enumerate_physical_drives, PhysicalPartitionReader,
    FsType,
};
use dds_diagnostic::{run_diagnostic, DiagnosticEngine};
use dds_fs_ntfs::NtfsVolume;
// ...

#[derive(Args, Debug)]
pub struct DiagnoseArgs {
    /// 物理ドライブ番号 (例: 1 for \\.\PhysicalDrive1)
    #[arg(long)]
    pub physical: Option<u32>,
    
    /// パーティション番号 (1 ベース、--physical と共に使う)
    #[arg(long)]
    pub partition: Option<u32>,
}

pub fn run(args: &DiagnoseArgs) -> Result<()> {
    println!("🔧 診断モード");
    println!("---------------------------------------------");
    println!();
    
    // Step 1: 案件番号入力
    let case_id = prompt_case_id()?;
    println!();
    
    // Step 2: モード判定
    let mode = match (args.physical, args.partition) {
        (Some(drive_num), Some(part_num)) => DiagnoseMode::Physical { drive_num, part_num },
        (Some(_), None) => {
            return Err(anyhow!("--physical を指定する場合は --partition も必要です"));
        }
        (None, Some(_)) => {
            return Err(anyhow!("--partition を指定する場合は --physical も必要です"));
        }
        (None, None) => DiagnoseMode::Logical,
    };
    
    // Step 3: モードに応じた処理
    let (case, _volume_info) = match mode {
        DiagnoseMode::Logical => diagnose_logical(case_id)?,
        DiagnoseMode::Physical { drive_num, part_num } => 
            diagnose_physical(case_id, drive_num, part_num)?,
    };
    
    // Step 4: 結果表示
    show_diagnostic_result(&case)?;
    
    Ok(())
}

enum DiagnoseMode {
    Logical,
    Physical { drive_num: u32, part_num: u32 },
}

fn diagnose_logical(case_id: CaseId) -> Result<(Case, String)> {
    // 既存の論理ドライブ診断 (変更なし)
    // ...
}

fn diagnose_physical(case_id: CaseId, drive_num: u32, part_num: u32) -> Result<(Case, String)> {
    println!("📡 物理ドライブモードで診断します");
    println!("  物理ドライブ: \\\\.\\PhysicalDrive{}", drive_num);
    println!("  パーティション: {}", part_num);
    println!();
    
    // 物理ドライブ列挙
    let drives = enumerate_physical_drives();
    let drive_info = drives.iter()
        .find(|d| d.drive_number == drive_num)
        .ok_or_else(|| anyhow!("物理ドライブ {} が見つかりません。\nlist-drives --physical で確認してください。", drive_num))?;
    
    println!("ドライブ情報:");
    println!("  サイズ:    {}", dds_core::format::format_bytes(drive_info.total_bytes));
    if let Some(vendor) = &drive_info.vendor_id {
        println!("  Vendor:    {}", vendor);
    }
    if let Some(product) = &drive_info.product_id {
        println!("  Product:   {}", product);
    }
    println!();
    
    // 物理ドライブを open
    let drive = PhysicalDrive::open(&drive_info.path)
        .with_context(|| format!("物理ドライブ {} を open できません", drive_num))?;
    
    // パーティション一覧
    let partitions = drive.list_partitions()
        .with_context(|| "パーティション情報を取得できません")?;
    
    if partitions.is_empty() {
        return Err(anyhow!("パーティションが検出されませんでした。\n物理ドライブに有効なパーティションテーブルがない可能性があります。"));
    }
    
    // 指定パーティションを取得
    let partition = partitions.iter()
        .find(|p| p.number == part_num)
        .ok_or_else(|| anyhow!("パーティション {} が見つかりません。\n利用可能: {}",
            part_num,
            partitions.iter().map(|p| p.number.to_string()).collect::<Vec<_>>().join(", ")))?;
    
    println!("選択されたパーティション:");
    println!("  パーティション {}: {}, {}, {}",
        partition.number,
        partition.partition_type.display_name(),
        dds_core::format::format_bytes(partition.size),
        partition.fs_type.display_name(),
    );
    println!();
    
    // FS タイプチェック
    if partition.fs_type != FsType::Ntfs {
        return Err(anyhow!(
            "選択されたパーティションは {} です。\nPhase 1.5 では NTFS のみ復旧可能です。",
            partition.fs_type.display_name()
        ));
    }
    
    // 物理ドライブを再度 open (closure 内で move されるため)
    let drive_for_reader = PhysicalDrive::open(&drive_info.path)
        .with_context(|| "物理ドライブの再 open に失敗")?;
    
    let reader = PhysicalPartitionReader::new(
        drive_for_reader,
        partition.start_offset,
        partition.size,
    );
    let closure = reader.into_closure();
    
    // NtfsVolume を open
    println!("[NTFS ボリュームを open しています...]");
    let mut volume = match NtfsVolume::new(closure) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow!(
                "NTFS ボリュームを open できませんでした。\n\
                原因: {}\n\
                \n\
                考えられる状況:\n\
                  • NTFS の管理領域 ($MFT) が破損している\n\
                  • パーティションテーブルは残っているが、FS が深刻に壊れている\n\
                  • 別ツール (R-STUDIO 等) での復旧をご検討ください",
                e
            ));
        }
    };
    println!("✓ NTFS ボリューム open 成功");
    println!();
    
    // 案件作成
    let storage = CaseStorage::default_location();
    let mut case = storage.create_new(case_id.clone())?;
    
    // 診断実行
    println!("[診断中...]");
    let start = std::time::Instant::now();
    let engine = DiagnosticEngine::new();
    let diagnostic_input = engine.diagnose(&mut volume)?;
    let elapsed = start.elapsed();
    println!("[診断完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();
    
    // case に保存
    case.diagnostic_input = diagnostic_input;
    storage.save(&case)?;
    
    let volume_info = format!("\\\\.\\PhysicalDrive{} Partition {}", drive_num, part_num);
    Ok((case, volume_info))
}

fn show_diagnostic_result(case: &Case) -> Result<()> {
    // 既存の表示ロジック
    // ...
    Ok(())
}
```

### Part D: workbench-dryrun の recover --physical 実装

`crates/workbench-dryrun/src/commands/recover.rs` の修正:

```rust
#[derive(Args, Debug)]
pub struct RecoverArgs {
    /// 物理ドライブ番号 (例: 1 for \\.\PhysicalDrive1)
    #[arg(long)]
    pub physical: Option<u32>,
    
    /// パーティション番号 (1 ベース、--physical と共に使う)
    #[arg(long)]
    pub partition: Option<u32>,
}

pub fn run(args: &RecoverArgs) -> Result<()> {
    println!("🔧 復旧モード");
    println!("---------------------------------------------");
    println!();
    
    // モード判定 (diagnose と同じロジック)
    let mode = match (args.physical, args.partition) {
        (Some(drive_num), Some(part_num)) => RecoverMode::Physical { drive_num, part_num },
        (Some(_), None) | (None, Some(_)) => {
            return Err(anyhow!("--physical と --partition は両方指定してください"));
        }
        (None, None) => RecoverMode::Logical,
    };
    
    match mode {
        RecoverMode::Logical => recover_logical()?,
        RecoverMode::Physical { drive_num, part_num } => 
            recover_physical(drive_num, part_num)?,
    }
    
    Ok(())
}

enum RecoverMode {
    Logical,
    Physical { drive_num: u32, part_num: u32 },
}

fn recover_logical() -> Result<()> {
    // 既存の論理ドライブ復旧 (変更なし)
    // ...
}

fn recover_physical(drive_num: u32, part_num: u32) -> Result<()> {
    println!("📡 物理ドライブモードで復旧します");
    println!("  ソース: \\\\.\\PhysicalDrive{} Partition {}", drive_num, part_num);
    println!();
    
    // Step 1: 案件番号入力
    let case_id = prompt_case_id()?;
    println!();
    
    // Step 2: 案件 JSON の load または新規作成
    let storage = CaseStorage::default_location();
    let case_file = storage.case_file_path(&case_id);
    let mut case = if case_file.exists() {
        println!("既存の案件を読み込みます: {}", case_id);
        storage.load(&case_id)?
    } else {
        println!("新規案件を作成します: {}", case_id);
        storage.create_new(case_id.clone())?
    };
    println!();
    
    // Step 3: ソース (物理パーティション) を準備
    let drives = enumerate_physical_drives();
    let drive_info = drives.iter()
        .find(|d| d.drive_number == drive_num)
        .ok_or_else(|| anyhow!("物理ドライブ {} が見つかりません", drive_num))?;
    
    println!("ソース ドライブ情報:");
    println!("  パス:      {}", drive_info.path.display());
    println!("  サイズ:    {}", dds_core::format::format_bytes(drive_info.total_bytes));
    if let Some(vendor) = &drive_info.vendor_id {
        println!("  Vendor:    {}", vendor);
    }
    if let Some(product) = &drive_info.product_id {
        println!("  Product:   {}", product);
    }
    
    let drive = PhysicalDrive::open(&drive_info.path)?;
    let partitions = drive.list_partitions()?;
    
    let partition = partitions.iter()
        .find(|p| p.number == part_num)
        .ok_or_else(|| anyhow!("パーティション {} が見つかりません", part_num))?
        .clone();
    
    println!("  パーティション: {} ({}, {})",
        partition.number,
        partition.fs_type.display_name(),
        dds_core::format::format_bytes(partition.size),
    );
    println!();
    
    // FS タイプチェック
    if partition.fs_type != FsType::Ntfs {
        return Err(anyhow!(
            "パーティション {} は {} です。\nPhase 1.5 では NTFS のみ復旧可能です。",
            part_num, partition.fs_type.display_name()
        ));
    }
    
    // Step 4: 納品先 HDD 選択 (論理ドライブから)
    println!("納品先の論理ドライブを選択してください:");
    let logical_drives: Vec<_> = list_logical_drives().into_iter()
        .filter(|d| !d.is_system_drive())
        .collect();
    
    if logical_drives.is_empty() {
        return Err(anyhow!("納品先となる論理ドライブが見つかりません。\n別の USB HDD を接続してください。"));
    }
    
    for (i, ld) in logical_drives.iter().enumerate() {
        println!("  [{}] {} ({}, {})", 
            i + 1, ld.drive_letter, ld.label, 
            dds_core::format::format_bytes(ld.total_bytes));
    }
    println!();
    
    let dst_sel = prompt_number("納品先 HDD を選択", 1, logical_drives.len())?;
    let delivery_drive = logical_drives[dst_sel - 1].clone();
    
    println!();
    
    // Step 5: 既存出力検出
    let case_output_root = delivery_drive.mount_point.join(case_id.as_str());
    if case_output_root.exists() {
        println!("⚠ 納品先に既にこの案件のフォルダが存在します:");
        println!("    {}", case_output_root.display());
        if !confirm("続行しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }
    
    // Step 6: Wishlist 作成
    println!();
    println!("Wishlist の入力 (お客様優先データの指定):");
    let wishlist = prompt_wishlist()?;
    println!();
    
    let exclusions = ExclusionList::default_system_exclusions();
    
    // Step 7: 確認
    println!("---------------------------------------------");
    println!("確認:");
    println!("  案件番号:       {}", case_id);
    println!("  ソース (物理):  \\\\.\\PhysicalDrive{} Partition {}", drive_num, part_num);
    println!("    {}", drive_info.path.display());
    println!("    FS: {} ({})", partition.fs_type.display_name(), 
        dds_core::format::format_bytes(partition.size));
    println!("  納品先:         {} ({})", delivery_drive.drive_letter, delivery_drive.label);
    println!();
    println!("  Wishlist:       {} 項目", wishlist.wishes.len());
    println!("  除外:           システムファイル (デフォルト)");
    println!();
    println!("出力先: {}\\", case_output_root.display());
    println!("---------------------------------------------");
    println!();
    
    if !confirm("復旧を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }
    
    // Step 8: 物理パーティションから NtfsVolume を open
    println!();
    println!("[NTFS ボリュームを open しています...]");
    let drive_for_reader = PhysicalDrive::open(&drive_info.path)?;
    let reader = PhysicalPartitionReader::new(
        drive_for_reader,
        partition.start_offset,
        partition.size,
    );
    let closure = reader.into_closure();
    
    let mut volume = match NtfsVolume::new(closure) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow!(
                "NTFS ボリュームを open できませんでした。\n\
                原因: {}\n\
                \n\
                推奨対応:\n\
                  1. パーティションが本当に NTFS か確認 (list-drives --physical)\n\
                  2. 別ツール (R-STUDIO 等) の使用を検討",
                e
            ));
        }
    };
    
    println!("✓ NTFS ボリューム open 成功");
    println!();
    println!("[復旧開始]");
    
    let progress = ConsoleProgressReporter::new();
    let start = std::time::Instant::now();
    
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
        &exclusions,
        &storage,
        &progress,
    ).context("復旧の実行に失敗しました")?;
    
    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    
    let mb_per_sec = (result.report.total_bytes_written() as f64 / 1_048_576.0)
        / elapsed.as_secs_f64().max(0.001);
    println!("  速度: {:.1} MB/s", mb_per_sec);
    println!();
    
    // Step 9: 結果表示 (既存と同じ)
    println!("結果 (全体):");
    println!("  該当ファイル:    {} 件", result.report.total_matched);
    println!("  復旧成功:        {} 件", result.report.recovered.len());
    println!("  復旧データ量:    {}", 
        dds_core::format::format_bytes(result.report.total_bytes_written()));
    println!();
    
    if result.report.priority_count() > 0 {
        println!("結果 (お客様優先データ):");
        println!("  該当ファイル:    {} 件", result.report.priority_count());
        println!("  復旧データ量:    {}", 
            dds_core::format::format_bytes(result.report.priority_bytes_written()));
        println!();
    }
    
    println!("生成ファイル:");
    println!("  納品 HDD ({}\\{}):", delivery_drive.drive_letter, case_id);
    println!("    └─ 復旧データ/通常ファイル/");
    println!("    └─ 復旧データ/削除ファイル/");
    println!("    └─ レポート/復旧レポート.docx");
    println!();
    println!("  社内保存 ({}\\{}):", storage.base_dir().display(), case_id);
    println!("    └─ 業務管理レポート.html");
    println!("    └─ 復旧詳細.csv");
    println!();
    
    storage.save(&case)?;
    
    Ok(())
}
```

### Part E: list-drives --physical の業務的ヒント追加

`crates/workbench-dryrun/src/commands/list_drives.rs` の `run_physical()` の末尾を更新:

```rust
// 既存の表示の後...

println!("---------------------------------------------");
println!();
println!("使い方:");
println!("  診断: workbench-dryrun diagnose --physical N --partition M");
println!("  復旧: workbench-dryrun recover --physical N --partition M");
println!();
println!("例:");
if let Some(drive_with_ntfs) = drives.iter().find(|d| {
    PhysicalDrive::open(&d.path).ok()
        .and_then(|drive| drive.list_partitions().ok())
        .map(|parts| parts.iter().any(|p| p.fs_type == FsType::Ntfs))
        .unwrap_or(false)
}) {
    // NTFS パーティションを持つドライブから例を生成
    println!("  workbench-dryrun diagnose --physical {} --partition 1", 
        drive_with_ntfs.drive_number);
} else {
    println!("  workbench-dryrun diagnose --physical 1 --partition 1");
}

Ok(())
```

## 単体テスト要件 (最低 6 件)

### `physical_partition.rs` (最低 2 件)

1. `partition_reader_offset_calculation_concept`
2. `partition_reader_boundary_check_concept`

### `workbench-dryrun` の引数パース (最低 2 件)

3. `diagnose_args_physical_requires_partition`: `--physical` のみだとエラー
4. `recover_args_physical_requires_partition`: 同上

### 統合テスト (Windows + 管理者権限のみ、`#[ignore]` 付き、最低 2 件)

5. `integration_open_ntfs_via_physical_partition`: システム HDD のパーティションを physical で open
6. `integration_diagnose_via_physical`: 実機 USB HDD を physical で診断

## 制約

- **行数目安**:
  - `crates/disk-io/src/physical_partition.rs` (新規): 約 130 行 + テスト 30 行
  - `crates/disk-io/src/lib.rs` 修正: +3 行
  - `crates/workbench-dryrun/src/commands/diagnose.rs` 修正: +130 行 (physical モード)
  - `crates/workbench-dryrun/src/commands/recover.rs` 修正: +200 行 (physical モード)
  - `crates/workbench-dryrun/src/commands/list_drives.rs` 修正: +20 行 (業務ヒント)
  - 合計: 約 510 行追加・修正
- **単体テスト新規**: 最低 6 件
- **統合テスト**: 2 件 (`#[ignore]` 付き)
- **`unsafe` 追加行数**: 0 (既存の Chunk 24d-1 の 30 行のまま)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (Chunk 24d-1/24d-2 から変化なし)
- [ ] `diagnose --physical N --partition M` が動作
- [ ] `recover --physical N --partition M` が動作
- [ ] 論理ドライブモード (既存) も引き続き動作
- [ ] FS タイプが NTFS 以外の場合、業務的に分かりやすいエラー
- [ ] NTFS open 失敗時、業務的に「次に何をすべきか」分かるエラー
- [ ] list-drives --physical の末尾に使い方ヒント

## 関連 FR 要件

- **FR-PHY-06** (物理パーティションからの NtfsVolume open) ← 達成
- **FR-PHY-07** (diagnose/recover の --physical 対応) ← 達成
- **FR-PHY-08** (壊れた NTFS の業務的なエラー表示) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **次のチャンク: Chunk 24d-4 (実機ドライランとフィードバック反映)**

---

## 注意事項

### Arc<Mutex<PhysicalDrive>> の理由

```rust
pub struct PhysicalPartitionReader {
    drive: Arc<Mutex<PhysicalDrive>>,
    // ...
}
```

なぜ Arc<Mutex<...>> が必要か:
- NtfsVolume の closure (`FnMut`) は move される
- PhysicalDrive を closure に直接 move すると、testing 等で複数アクセス困難
- Arc<Mutex<...>> で内部可変性 + 共有を実現

並列化 (Chunk 24b で導入) との関係:
- NtfsVolume の reader はシリアル制約
- 並列化は post-processing (write/SHA256/validate) で発生
- read 自体は単一スレッドなので Mutex 競合なし

### PhysicalDrive を 2 回 open する理由

```rust
// 1 回目: パーティション一覧取得用
let drive = PhysicalDrive::open(&path)?;
let partitions = drive.list_partitions()?;

// 2 回目: NtfsVolume の reader 用 (drive が move されるため)
let drive_for_reader = PhysicalDrive::open(&path)?;
let reader = PhysicalPartitionReader::new(drive_for_reader, ...);
```

これは Phase 1.5 の実装をシンプルに保つための妥協。
将来 (Phase 2) は PhysicalDrive を Clone 可能にする等で改善可能。

### 業務的なエラーメッセージの設計

NtfsVolume::new() が失敗するケースは複数:
- NTFS シグネチャはあるが、ブートセクタが壊れている
- $MFT のオフセットが異常
- $MFT 自体が読めない
- 暗号化されている

これらを技術的に区別するのは複雑。Phase 1.5 では一律「業務的なメッセージ」を表示:

```
NTFS ボリュームを open できませんでした。
原因: <技術的エラー>

推奨対応:
  1. パーティションが本当に NTFS か確認 (list-drives --physical)
  2. 別ツール (R-STUDIO 等) の使用を検討
```

業務メンバーが「次の一手」を判断できるメッセージ。

### 既存の論理ドライブモードとの共存

```cmd
[論理ドライブモード - 既存]
> workbench-dryrun diagnose
> workbench-dryrun recover

[物理ドライブモード - 新規]
> workbench-dryrun diagnose --physical N --partition M
> workbench-dryrun recover --physical N --partition M
```

両方とも動作する。Chouさんが状況に応じて使い分け:
- 正常な NTFS の HDD → 論理ドライブモード (シンプル)
- 壊れた FS の HDD → 物理ドライブモード (Phase 1.5 拡張の真価)

### Phase 2.1 UI への引き継ぎ

```
[Tauri UI で表示する選択肢]
1. 「論理ドライブモード」ボタン → 現在の UI
2. 「物理ドライブモード」ボタン → 新規 UI
   - 物理ドライブ一覧
   - パーティション選択
   - 復旧開始

[Chunk 24d-3 で公開する API]
PhysicalDrive::open, list_partitions
PhysicalPartitionReader::new, into_closure
→ UI から呼び出して NtfsVolume 構築
```

---

## 質問が必要なケース

- 既存の NtfsVolume のシグネチャが想定と違う場合
- PhysicalDrive::open を 2 回呼ぶことの問題 (Windows の HANDLE 制約等)
- `Arc<Mutex<...>>` のロック競合が問題になる場合

---

## 完了報告例

```markdown
## Chunk 24d-3 完了報告

### 新規ファイル
- crates/disk-io/src/physical_partition.rs (約 130 行 + テスト 30 行)

### 修正ファイル
- crates/disk-io/src/lib.rs (+3 行)
- crates/workbench-dryrun/src/commands/diagnose.rs (+130 行 physical モード)
- crates/workbench-dryrun/src/commands/recover.rs (+200 行 physical モード)
- crates/workbench-dryrun/src/commands/list_drives.rs (+20 行 業務ヒント)

### 新規 API
- PhysicalPartitionReader::new(drive, start_offset, size)
- PhysicalPartitionReader::into_closure() → FnMut(u64, u64) -> Result<Vec<u8>>

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)

### テスト統計
- 単体: 既存 + 新規 4 件
- 統合: 2 件 (#[ignore]、ローカル検証用)
- 全 workspace: 全パス

### 動作確認サンプル (管理者として実行)
```
> workbench-dryrun list-drives --physical
[既存表示 + 使い方ヒント]

> workbench-dryrun diagnose --physical 1 --partition 1
🔧 診断モード
案件番号: 260530-01

📡 物理ドライブモードで診断します
  物理ドライブ: \\.\PhysicalDrive1
  パーティション: 1

ドライブ情報:
  サイズ:    1.8 TB
  Vendor:    Seagate

選択されたパーティション:
  パーティション 1: NTFS/exFAT/HPFS, 1.8 TB, NTFS

[NTFS ボリュームを open しています...]
✓ NTFS ボリューム open 成功

[診断中...]
[診断完了 - 1.23 秒]
... (診断結果)
```

### 🎯 達成事項
- 物理パーティションから NtfsVolume を open できる
- diagnose --physical / recover --physical が動作
- 論理ドライブモード (既存) も維持
- 壊れた NTFS の場合、業務的に分かりやすいエラー

### Phase 1.5 拡張の業務的価値が実現:
- 壊れた FS の HDD でも復旧可能に
- R-STUDIO の代替候補として真剣に評価可能

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chunk 24d-4 (実機ドライランとフィードバック反映) に移行
→ Chouさんが論理ドライブ + 物理ドライブ両方でドライランを実施
```
