# Chunk 23.5 指示: workbench-dryrun（実機ドライラン用の暫定 CLI）

Phase 1.5 完成機能を**検証 PC で実機 HDD に対して試す**ための暫定実行可能ツール。Phase 2.1 (Tauri UI) 完成までの中継ぎとして機能。

> 🎯 完了時点で「検証 PC で `workbench-dryrun diagnose` → 実機 HDD → CRM 貼り付けテキスト」「`workbench-dryrun recover` → 実機復旧 → 納品 HDD 出力」が動く。**実機特有の問題発見**と**業務適用前のリアリティチェック**が可能に。

---

## 背景

Phase 1.5 完成 (Chunk 23) で全機能はライブラリとして揃いましたが、**実行可能なバイナリはまだありません**:

```
[現状]
✅ ライブラリとしての機能: 完成 (case-manager, diagnostic, recovery, report)
✅ 統合テスト: フィクスチャでパス
✗ 実機 HDD を読むバイナリ: 未実装
✗ コマンドラインから呼び出す手段: なし
```

Phase 2.1 (Tauri UI) 着手前に、**実機で 1 回動かす**ことの価値:

1. フィクスチャでは見えない実機特有の問題発見
2. CRM 貼り付けテキストを実際の CRM フォームに貼って業務適用性確認
3. 大容量 HDD でのパフォーマンス確認
4. CS / エンジニアからの初期フィードバック収集
5. Phase 2.1 UI 設計の精度向上

そのため、**暫定 CLI ツール**を作って実機ドライランを実施します。Phase 2.1 UI 完成後は workbench-dryrun は予備品として残し、本番運用は UI に移行。

## 目的

実機ドライラン用の暫定 CLI を構築する:

1. **`list-drives`**: 接続中の論理ドライブを表示
2. **`diagnose`**: 対話形式で診断実行 + CRM 貼り付けテキスト生成
3. **`recover`**: 対話形式で復旧実行
4. **`show`**: 案件 JSON の整形表示
5. **対話形式の UX**: エンジニアが引数を覚えなくても動かせる

## 対象クレート

`crates/workbench-dryrun/` (新規クレート)

## 重要な設計原則

### 暫定品としての設計

```rust
✗ Phase 2.1 UI を不要にする本格的な CLI を作る
○ 実機ドライランに必要な最小機能を素早く実装、UI 完成後は予備品
```

- テストは最小限 (Phase 2.1 UI 完成までの暫定)
- エラー処理はシンプル (anyhow で OK)
- 美しい進捗バーは不要 (黙々と実行して完了)
- Wishlist 作成も最小限の対話 (フル機能は UI で)

### 論理ドライブベースのアクセス

```
✗ \\.\PhysicalDriveN (物理ドライブ、unsafe Windows API 必要)
○ \\.\E: (論理ドライブ、標準 file open でアクセス可能)
```

USB HDD は Windows が自動マウントしてドライブレター (E:, F: 等) を割り当てる。`\\.\E:` でパーティション本体にアクセス可能で、unsafe 不要。

物理ドライブ直接アクセスは Phase 2.1 で本格対応 (壊れた HDD でドライブレター未割り当てのケース)。

### Windows 専用

検証 PC は Windows 前提 (Q13)。Linux / Mac でのビルドは想定しない:

```rust
#[cfg(not(windows))]
compile_error!("workbench-dryrun は Windows のみサポートしています");
```

### 管理者権限の前提

`\\.\E:` 形式のアクセスは管理者権限が必要な場合がある。実行方法はドキュメントに明記:

```
> workbench-dryrun を実行するには、コマンドプロンプトを
> 「管理者として実行」で起動してください。
```

## 仕様参照

### ビジネス要件

- **FR-CLI-01**: 実機ドライラン用の最小 CLI
- **FR-CLI-02**: 対話形式の UX
- **FR-CLI-03**: 案件情報の保存 (`C:\cases\{案件番号}\`)
- **FR-CLI-04**: 業務向け出力構造への対応

## 実装内容

### Cargo.toml

```toml
[package]
name = "dds-workbench-dryrun"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "workbench-dryrun"
path = "src/main.rs"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
sysinfo = "0.32"
anyhow = "1.0"

dds-core.workspace = true
dds-case-manager.workspace = true
dds-diagnostic.workspace = true
dds-disk-io.workspace = true
dds-fs-ntfs.workspace = true
dds-recovery.workspace = true
dds-report.workspace = true
dds-wish-match.workspace = true

chrono.workspace = true
serde.workspace = true
serde_json.workspace = true

[dev-dependencies]
tempfile = "3.10"
```

### モジュール構成

```
crates/workbench-dryrun/
├── Cargo.toml
├── README.md             ← 使い方ガイド (CS / エンジニア向け)
└── src/
    ├── main.rs           ← 引数パース + サブコマンド分岐
    ├── prompts.rs        ← 対話形式の helpers
    ├── drives.rs         ← 論理ドライブ列挙
    ├── volume.rs         ← NTFS ボリュームの open
    └── commands/
        ├── mod.rs
        ├── list_drives.rs
        ├── diagnose.rs
        ├── recover.rs
        └── show.rs
```

### 1. `main.rs`

```rust
//! DDS Recovery Workbench - 実機ドライラン用暫定 CLI
//!
//! Phase 1.5 完成機能を検証 PC で実機 HDD に対して試すためのツール。
//! Phase 2.1 (Tauri UI) 完成後は予備品として残ります。

#[cfg(not(windows))]
compile_error!("workbench-dryrun は Windows のみサポートしています");

use clap::{Parser, Subcommand};
use anyhow::Result;

mod commands;
mod drives;
mod prompts;
mod volume;

#[derive(Parser)]
#[command(name = "workbench-dryrun")]
#[command(about = "DDS Recovery Workbench - 実機ドライラン用 CLI (Phase 1.5 暫定)")]
#[command(long_about = "
DDS Recovery Workbench の Phase 1.5 機能を実機 HDD で試すための暫定 CLI です。

使用例:
  workbench-dryrun list-drives        # 接続中ドライブの一覧
  workbench-dryrun diagnose           # 対話形式で診断
  workbench-dryrun recover            # 対話形式で復旧
  workbench-dryrun show               # 案件情報の表示

⚠ 注意:
  ・物理ドライブへのアクセスには管理者権限が必要です
  ・「管理者として実行」で開いたコマンドプロンプトから実行してください
")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 接続中の論理ドライブを一覧表示
    ListDrives,
    
    /// 案件を作成し、対象 HDD を診断する
    Diagnose,
    
    /// 既存の案件に対して復旧を実行する
    Recover,
    
    /// 案件情報を表示する
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    println!();
    println!("DDS Recovery Workbench (Phase 1.5)");
    println!("============================================");
    println!();
    
    match cli.command {
        Commands::ListDrives => commands::list_drives::run(),
        Commands::Diagnose => commands::diagnose::run(),
        Commands::Recover => commands::recover::run(),
        Commands::Show => commands::show::run(),
    }
}
```

### 2. `prompts.rs`

対話形式の helpers:

```rust
use std::io::{self, Write};
use anyhow::{anyhow, Result};

/// 文字列入力を求める。
pub fn prompt_string(message: &str) -> Result<String> {
    print!("{}: ", message);
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

/// 数値入力を求める。範囲チェック付き。
pub fn prompt_number(message: &str, min: usize, max: usize) -> Result<usize> {
    loop {
        let input = prompt_string(&format!("{} ({}-{})", message, min, max))?;
        match input.parse::<usize>() {
            Ok(n) if n >= min && n <= max => return Ok(n),
            Ok(_) => println!("  ⚠ 範囲外です。{}-{} の値を入力してください。", min, max),
            Err(_) => println!("  ⚠ 数値として読み取れませんでした。"),
        }
    }
}

/// Yes/No 確認を求める (デフォルト Yes)。
pub fn confirm(message: &str) -> Result<bool> {
    let input = prompt_string(&format!("{} [Y/n]", message))?;
    let lower = input.to_lowercase();
    Ok(lower.is_empty() || lower == "y" || lower == "yes")
}

/// 案件番号入力 (yymmdd-NN 形式の検証付き)
pub fn prompt_case_id() -> Result<dds_case_manager::CaseId> {
    loop {
        let input = prompt_string("案件番号 (yymmdd-NN 形式、例: 260522-04)")?;
        match dds_case_manager::CaseId::parse(&input) {
            Ok(id) => return Ok(id),
            Err(e) => println!("  ⚠ {}", e),
        }
    }
}
```

### 3. `drives.rs`

論理ドライブ列挙:

```rust
use std::path::PathBuf;
use sysinfo::Disks;

/// 論理ドライブの情報
#[derive(Debug, Clone)]
pub struct DriveInfo {
    /// ドライブパス (例: "E:")
    pub drive_letter: String,
    /// マウントポイント (例: "E:\")
    pub mount_point: PathBuf,
    /// ボリュームラベル (例: "USB_HDD")
    pub label: String,
    /// 容量 (バイト)
    pub total_bytes: u64,
    /// 空き容量 (バイト)
    pub available_bytes: u64,
    /// ファイルシステム (例: "NTFS", "FAT32")
    pub file_system: String,
    /// アクセス用のパス (例: "\\.\E:")
    pub access_path: String,
}

impl DriveInfo {
    /// パーティションが NTFS かどうか
    pub fn is_ntfs(&self) -> bool {
        self.file_system.eq_ignore_ascii_case("NTFS")
    }
    
    /// システムドライブ (通常 C:) かどうか
    pub fn is_system_drive(&self) -> bool {
        self.drive_letter == "C:"
    }
}

/// 接続中の論理ドライブを列挙する
pub fn list_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    
    disks.list().iter().map(|disk| {
        let mount = disk.mount_point().to_path_buf();
        let mount_str = mount.to_string_lossy().to_string();
        
        // ドライブレターを抽出 (例: "E:\" → "E:")
        let drive_letter = if mount_str.len() >= 2 && mount_str.chars().nth(1) == Some(':') {
            mount_str[..2].to_string()
        } else {
            mount_str.clone()
        };
        
        let access_path = format!("\\\\.\\{}", drive_letter);
        
        DriveInfo {
            drive_letter,
            mount_point: mount,
            label: disk.name().to_string_lossy().to_string(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
            file_system: disk.file_system().to_string_lossy().to_string(),
            access_path,
        }
    }).collect()
}
```

### 4. `volume.rs`

NTFS ボリューム open helper:

```rust
use std::fs::File;
use anyhow::{Context, Result};
use dds_fs_ntfs::NtfsVolume;

/// 論理ドライブパス (例: "\\\\.\\E:") から NtfsVolume を開く
pub fn open_ntfs_volume(access_path: &str) -> Result<NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(access_path)
        .with_context(|| format!("ドライブを開けません: {}\n\n管理者として実行する必要があるかもしれません。", access_path))?;
    
    // disk-io クレートの FileDisk または同等の reader を構築
    let reader = make_file_reader(file);
    
    // クラスタサイズはブートセクタから読み取る (既存実装)
    NtfsVolume::open(reader)
        .context("NTFS ボリュームの open に失敗しました")
}

/// File から read closure を作る (Chunks 4-14 の既存パターン)
fn make_file_reader(file: File) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
    use std::io::{Read, Seek, SeekFrom};
    use std::sync::Mutex;
    
    let file = Mutex::new(file);
    move |offset, length| {
        let mut f = file.lock().unwrap();
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; length as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }
}
```

> **実装上の注**: `make_file_reader` は disk-io クレートに既存実装があれば再利用。なければ inline で実装。`Mutex` を使うのは `FnMut` で `&mut self` 的なアクセスをするため。

### 5. `commands/list_drives.rs`

```rust
use anyhow::Result;
use dds_core::format::format_bytes;
use crate::drives::list_drives;

pub fn run() -> Result<()> {
    println!("接続中の論理ドライブ:");
    println!();
    
    let drives = list_drives();
    if drives.is_empty() {
        println!("  ドライブが見つかりませんでした。");
        return Ok(());
    }
    
    for (i, drive) in drives.iter().enumerate() {
        let system_marker = if drive.is_system_drive() { " [システム]" } else { "" };
        let ntfs_marker = if drive.is_ntfs() { " ✓ NTFS" } else { "" };
        
        println!("  [{}] {} {}{}{}",
            i + 1,
            drive.drive_letter,
            drive.label,
            system_marker,
            ntfs_marker);
        println!("       容量:       {}", format_bytes(drive.total_bytes));
        println!("       空き容量:   {}", format_bytes(drive.available_bytes));
        println!("       FS:         {}", drive.file_system);
        println!("       アクセス:   {}", drive.access_path);
        println!();
    }
    
    println!("対象 HDD を Workbench で読み込むには、上記の「アクセス」パスを使用します。");
    Ok(())
}
```

### 6. `commands/diagnose.rs`

```rust
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use dds_case_manager::{CaseStorage, DiagnosticEngine};
use dds_diagnostic::DiagnosticEngine as DEngine;
use crate::drives::{list_drives, DriveInfo};
use crate::prompts::{confirm, prompt_case_id, prompt_number};
use crate::volume::open_ntfs_volume;

pub fn run() -> Result<()> {
    println!("📋 診断モード");
    println!("---------------------------------------------");
    println!();
    
    // Step 1: 案件番号入力
    let case_id = prompt_case_id()?;
    println!();
    
    // Step 2: 案件作成 or 既存案件チェック
    let storage = CaseStorage::default_location();  // C:\cases\
    
    let case_already_exists = storage.case_file_path(&case_id).exists();
    let mut case = if case_already_exists {
        println!("⚠ この案件はすでに存在しています: {}", case_id);
        if !confirm("既存の案件を更新しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
        storage.load(&case_id)?
    } else {
        storage.create_new(case_id.clone())?
    };
    
    // Step 3: 対象ドライブの選択
    println!();
    println!("接続中の NTFS ドライブ:");
    let drives: Vec<_> = list_drives().into_iter()
        .filter(|d| d.is_ntfs() && !d.is_system_drive())
        .collect();
    
    if drives.is_empty() {
        return Err(anyhow!("操作可能な NTFS ドライブが見つかりませんでした。\nシステムドライブ (C:) は対象外です。"));
    }
    
    for (i, drive) in drives.iter().enumerate() {
        println!("  [{}] {} ({}, {})",
            i + 1,
            drive.drive_letter,
            drive.label,
            dds_core::format::format_bytes(drive.total_bytes));
    }
    println!();
    
    let selection = prompt_number("診断対象を選択", 1, drives.len())?;
    let selected_drive = &drives[selection - 1];
    
    // Step 4: 確認
    println!();
    println!("確認:");
    println!("  案件番号: {}", case_id);
    println!("  対象ドライブ: {} ({})", selected_drive.drive_letter, selected_drive.label);
    println!("  アクセスパス: {}", selected_drive.access_path);
    println!();
    
    if !confirm("診断を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }
    
    // Step 5: 診断実行
    println!();
    println!("[診断中... MFT 読み取り中、少々お待ちください]");
    let start = std::time::Instant::now();
    
    let mut volume = open_ntfs_volume(&selected_drive.access_path)?;
    let report = DEngine::diagnose(&mut volume, case_id.clone())
        .context("診断の実行に失敗しました")?;
    
    let elapsed = start.elapsed();
    println!("[診断完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();
    
    // Step 6: 結果表示
    println!("結果サマリ:");
    println!("  全ファイル: {} 件", report.file_stats.total_files);
    println!("  通常 (生存): {} 件", report.file_stats.live_files);
    println!("  削除済み: {} 件", report.file_stats.deleted_files);
    if !report.filesystem_findings.signature_valid {
        println!("  ⚠ ファイルシステム署名異常");
    }
    if report.filesystem_findings.mft_corrupted_count > 0 {
        println!("  ⚠ MFT エントリ破損: {} 件", report.filesystem_findings.mft_corrupted_count);
    }
    println!();
    
    // Step 7: CRM 貼り付けテキスト生成と保存
    let crm_text = report.to_crm_text();
    let case_dir = storage.case_dir(&case_id);
    std::fs::create_dir_all(&case_dir)?;
    let crm_text_path = case_dir.join("診断結果_CRM貼り付け用.txt");
    std::fs::write(&crm_text_path, &crm_text)?;
    
    println!("CRM 貼り付けテキスト:");
    println!("---------------------------------------------");
    println!("{}", crm_text);
    println!("---------------------------------------------");
    println!();
    
    // Step 8: case.json 更新
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(&case)?;
    
    println!("保存先:");
    println!("  案件 JSON:      {}", storage.case_file_path(&case_id).display());
    println!("  CRM 貼り付け用: {}", crm_text_path.display());
    println!();
    println!("✓ 診断完了。CRM 貼り付けテキストをコピーして CRM に貼り付けてください。");
    
    Ok(())
}
```

### 7. `commands/recover.rs`

```rust
use std::path::PathBuf;
use anyhow::{anyhow, Context, Result};
use dds_case_manager::{
    execute_business_recovery, CaseStorage,
};
use dds_wish_match::{Priority, Wish, WishItem, Wishlist};

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number, prompt_string};
use crate::volume::open_ntfs_volume;

pub fn run() -> Result<()> {
    println!("🔧 復旧モード");
    println!("---------------------------------------------");
    println!();
    
    // Step 1: 案件番号入力 + 案件読み込み
    let case_id = prompt_case_id()?;
    let storage = CaseStorage::default_location();
    let mut case = storage.load(&case_id)
        .context("案件が見つかりません。先に diagnose で案件を作成してください")?;
    
    println!();
    if case.diagnostic_input.diagnosed_at.is_none() {
        println!("⚠ この案件はまだ診断されていません。");
        if !confirm("診断なしで復旧を進めますか? (推奨されません)")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }
    
    // Step 2: 対象 HDD の選択
    println!("接続中の NTFS ドライブ:");
    let drives: Vec<_> = list_drives().into_iter()
        .filter(|d| d.is_ntfs() && !d.is_system_drive())
        .collect();
    
    for (i, drive) in drives.iter().enumerate() {
        println!("  [{}] {} ({}, {})",
            i + 1,
            drive.drive_letter,
            drive.label,
            dds_core::format::format_bytes(drive.total_bytes));
    }
    println!();
    
    let src_sel = prompt_number("ソース HDD (お客様の HDD) を選択", 1, drives.len())?;
    let source_drive = drives[src_sel - 1].clone();
    
    let dst_sel = prompt_number("納品先 HDD (G:\\ など) を選択", 1, drives.len())?;
    let delivery_drive = drives[dst_sel - 1].clone();
    
    if source_drive.drive_letter == delivery_drive.drive_letter {
        return Err(anyhow!("ソースと納品先が同じドライブです。別のドライブを選択してください。"));
    }
    
    // Step 3: Wishlist 入力
    let wishlist = prompt_wishlist()?;
    
    if wishlist.wishes.is_empty() {
        return Err(anyhow!("Wishlist が空です。少なくとも 1 つの希望が必要です。"));
    }
    
    // Step 4: 確認
    println!();
    println!("確認:");
    println!("  案件番号:       {}", case_id);
    println!("  ソース:         {} ({})", source_drive.drive_letter, source_drive.label);
    println!("  納品先:         {} ({})", delivery_drive.drive_letter, delivery_drive.label);
    println!("  希望データ数:   {}", wishlist.wishes.len());
    for (i, wish) in wishlist.wishes.iter().enumerate() {
        println!("    {}: 「{}」", i + 1, wish.label);
    }
    println!();
    println!("出力先: {}\\{}\\", delivery_drive.drive_letter, case_id);
    println!();
    
    if !confirm("復旧を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }
    
    // Step 5: 復旧実行
    println!();
    println!("[復旧中... 完了まで時間がかかる場合があります]");
    let start = std::time::Instant::now();
    
    let mut volume = open_ntfs_volume(&source_drive.access_path)?;
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
    ).context("復旧の実行に失敗しました")?;
    
    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();
    
    // Step 6: 結果表示
    println!("結果:");
    println!("  該当ファイル:    {} 件", result.report.total_matched);
    println!("  復旧成功:        {} 件 ({:.1}%)",
        result.report.recovered.len(),
        result.report.recovery_success_rate());
    println!("  品質保証率:      {:.1}%", result.report.quality_assurance_rate());
    println!("  復旧データ量:    {}", dds_core::format::format_bytes(result.report.total_bytes_written()));
    println!();
    
    println!("生成ファイル:");
    println!("  📂 {}", result.case_output.root().display());
    println!("    └─ 復旧データ/");
    println!("        ├─ 通常ファイル/");
    println!("        └─ 削除ファイル/");
    println!("    └─ レポート/");
    println!("        ├─ 復旧レポート.docx");
    println!("        ├─ 要確認ファイル一覧.txt");
    println!("        ├─ 業務管理レポート.html");
    println!("        └─ report.csv");
    println!();
    
    // Step 7: case.json 永続化
    storage.save(&case)?;
    println!("✓ 案件情報を保存しました: {}", storage.case_file_path(&case_id).display());
    
    Ok(())
}

fn prompt_wishlist() -> Result<Wishlist> {
    println!();
    println!("Wishlist の作成:");
    println!("  1. 対話形式で入力 (拡張子ベース、シンプル)");
    println!("  2. JSON ファイルから読み込み");
    let method = prompt_number("作成方法を選択", 1, 2)?;
    
    match method {
        1 => prompt_interactive_wishlist(),
        2 => load_wishlist_from_json(),
        _ => unreachable!(),
    }
}

fn prompt_interactive_wishlist() -> Result<Wishlist> {
    println!();
    println!("希望データを 1 つずつ入力します。完了するには「ラベル」で空 Enter を押してください。");
    println!();
    
    let mut wishlist = Wishlist::new();
    let mut count = 1;
    
    loop {
        println!("希望 {}:", count);
        let label = prompt_string("  ラベル (例: Word ファイル、空 Enter で終了)")?;
        if label.is_empty() {
            break;
        }
        
        let ext = prompt_string("  拡張子 (例: docx、ピリオドなし)")?;
        let priority_input = prompt_string("  優先度 (high/medium/low、デフォルト high)")?;
        let priority = match priority_input.to_lowercase().as_str() {
            "medium" | "m" => Priority::Medium,
            "low" | "l" => Priority::Low,
            _ => Priority::High,
        };
        
        wishlist = wishlist.add(
            Wish::new(WishItem::Extension(ext.to_lowercase()), &label)
                .with_priority(priority)
        );
        count += 1;
        println!();
    }
    
    Ok(wishlist)
}

fn load_wishlist_from_json() -> Result<Wishlist> {
    let path = prompt_string("Wishlist JSON ファイルのパス")?;
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("ファイルを読めません: {}", path))?;
    let wishlist: Wishlist = serde_json::from_str(&json)
        .context("Wishlist JSON のパースに失敗しました")?;
    Ok(wishlist)
}
```

### 8. `commands/show.rs`

```rust
use anyhow::Result;
use dds_case_manager::CaseStorage;
use dds_core::format::format_bytes;
use crate::prompts::prompt_case_id;

pub fn run() -> Result<()> {
    println!("📄 案件情報の表示");
    println!("---------------------------------------------");
    println!();
    
    let case_id = prompt_case_id()?;
    let storage = CaseStorage::default_location();
    let case = storage.load(&case_id)?;
    
    println!();
    println!("案件番号: {}", case.case_id);
    println!("作成日時: {}", case.created_at.format("%Y-%m-%d %H:%M"));
    println!("最終更新: {}", case.updated_at.format("%Y-%m-%d %H:%M"));
    println!();
    
    // 診断結果
    if case.diagnostic_input.diagnosed_at.is_some() {
        println!("【診断結果】");
        if let Some(at) = case.diagnostic_input.diagnosed_at {
            println!("  実施日時:       {}", at.format("%Y-%m-%d %H:%M"));
        }
        if let Some(secs) = case.diagnostic_input.duration_secs {
            println!("  診断時間:       {} 秒", secs);
        }
        if let Some(fs) = &case.diagnostic_input.filesystem_type {
            println!("  FS:             {}", fs);
        }
        println!("  全ファイル:     {} 件", case.diagnostic_input.total_files);
        println!("  削除ファイル:   {} 件", case.diagnostic_input.deleted_files);
        println!();
    } else {
        println!("【診断結果】 未実施");
        println!();
    }
    
    // Wishlist
    if let Some(wishlist) = &case.wishlist {
        println!("【Wishlist】");
        for (i, wish) in wishlist.wishes.iter().enumerate() {
            println!("  {}: 「{}」 ({:?})", i + 1, wish.label, wish.priority);
        }
        println!();
    } else {
        println!("【Wishlist】 未作成");
        println!();
    }
    
    // 復旧結果
    if let Some(summary) = &case.recovery_report_summary {
        println!("【復旧結果】");
        println!("  該当ファイル:   {} 件", summary.total_matched);
        println!("  復旧成功:       {} 件 ({:.1}%)",
            summary.recovered_count, summary.recovery_success_rate);
        println!("  品質保証率:     {:.1}%", summary.quality_assurance_rate);
        println!("  復旧データ量:   {}", format_bytes(summary.total_bytes_written));
        if let Some(output) = &case.output_dir {
            println!("  出力先:         {}", output.display());
        }
        println!();
    } else {
        println!("【復旧結果】 未実施");
        println!();
    }
    
    Ok(())
}
```

### 9. `README.md`

```markdown
# workbench-dryrun - 実機ドライラン用 CLI

DDS Recovery Workbench の Phase 1.5 機能を実機 HDD で試すためのツール。
Phase 2.1 (Tauri UI) 完成までの暫定品。

## 必要な準備

1. **管理者として実行**: コマンドプロンプトを「管理者として実行」で起動
2. **テスト用 HDD**: 不要な NTFS USB HDD (テストデータ入り)
3. **納品先 HDD**: 別の USB HDD (G: ドライブとして認識)

## 使い方

### 接続中のドライブ確認

```cmd
> workbench-dryrun list-drives
```

### 診断

```cmd
> workbench-dryrun diagnose

案件番号 (yymmdd-NN 形式): 260522-04
[ドライブ選択画面]
[診断確認画面]
[診断実行]
[CRM 貼り付けテキスト表示]
```

完了後、`C:\cases\260522-04\` に以下が保存されます:
- `case.json` (案件情報)
- `診断結果_CRM貼り付け用.txt` (CRM 貼り付け用)

### 復旧

```cmd
> workbench-dryrun recover

案件番号: 260522-04
[ソースドライブ選択]
[納品先ドライブ選択]
[Wishlist 作成 (対話 or JSON)]
[復旧確認画面]
[復旧実行]
```

完了後、納品先 HDD に `{案件番号}\` のフォルダ構造で出力されます。

### 案件情報の表示

```cmd
> workbench-dryrun show

案件番号: 260522-04
[案件情報表示]
```

## 注意事項

- **管理者権限が必須** (HDD 直接アクセスのため)
- **システムドライブ (C:) は対象外** (ソース・納品先とも)
- **対象は NTFS のみ** (exFAT/FAT32 は Phase 2 以降)
- ソース HDD は **read-only** でアクセス (書き込みなし)
- 納品先 HDD には `{案件番号}\` フォルダが作成されます

## Wishlist JSON フォーマット

```json
{
  "wishes": [
    {
      "label": "Word ファイル全部",
      "item": { "Extension": "docx" },
      "priority": "High"
    },
    {
      "label": "写真データ",
      "item": { "Extension": "jpg" },
      "priority": "High"
    }
  ]
}
```

## トラブルシューティング

### 「ドライブを開けません」エラー
→ 管理者として実行していない可能性。コマンドプロンプトを「管理者として実行」で起動し直す。

### 「NTFS ボリュームの open に失敗」エラー
→ 対象 HDD が NTFS でない、または FS が壊れている。`list-drives` で FS を確認。

### 診断が遅い
→ HDD のサイズに比例。1TB HDD で約 30-60 秒、2TB で 60-120 秒が目安。
   不良セクタが多い場合はさらに時間がかかる。
```

## 単体テスト要件 (最低 8 件)

dryrun は実機を扱うため、自動テストは限定的:

### `drives.rs` (最低 2 件)

```rust
#[test]
fn list_drives_returns_at_least_system_drive() {
    let drives = list_drives();
    // システム上に少なくとも 1 つドライブはあるはず
    assert!(!drives.is_empty());
}

#[test]
fn drive_info_correctly_identifies_ntfs() {
    let drive = DriveInfo {
        drive_letter: "E:".to_string(),
        mount_point: "E:\\".into(),
        label: "test".to_string(),
        total_bytes: 0,
        available_bytes: 0,
        file_system: "NTFS".to_string(),
        access_path: "\\\\.\\E:".to_string(),
    };
    assert!(drive.is_ntfs());
    assert!(!drive.is_system_drive());
}
```

### `prompts.rs` (最低 4 件)

stdin / stdout のモックは複雑なので、bool 等の単純な helpers のテストに留める。または io トレイトを抽象化:

```rust
// 内部実装で BufRead trait を取れるようにすれば mock 可能
// テストの細かい実装は実装者に委ねる
```

### `commands/show.rs` (最低 1 件)

```rust
#[test]
fn show_handles_missing_case_gracefully() {
    // 存在しない case_id で show を呼ぶ → エラーが返る (パニックしない)
}
```

### 引数パース (最低 1 件)

```rust
#[test]
fn cli_parses_list_drives_command() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["workbench-dryrun", "list-drives"]).unwrap();
    matches!(cli.command, Commands::ListDrives);
}
```

## 結合テスト要件

実機テストとなるため、機械的な統合テストは**実施しない**。
代わりに、**README に手動テスト手順を記載**:

```
[手動テスト手順]
1. 検証 PC で管理者として cmd を開く
2. cargo run --release -p dds-workbench-dryrun -- list-drives
   → ドライブ一覧が表示されること
3. テスト用 USB HDD を接続
4. cargo run --release -p dds-workbench-dryrun -- diagnose
   → 対話 → 診断完了 → CRM テキスト表示
5. 納品先 HDD を接続
6. cargo run --release -p dds-workbench-dryrun -- recover
   → 対話 → 復旧完了 → 出力構造確認
7. cargo run --release -p dds-workbench-dryrun -- show
   → 案件情報表示
```

## 制約

- **行数目安**:
  - main.rs: 60 行
  - prompts.rs: 80 行
  - drives.rs: 80 行
  - volume.rs: 50 行
  - commands/list_drives.rs: 50 行
  - commands/diagnose.rs: 180 行
  - commands/recover.rs: 200 行
  - commands/show.rs: 100 行
  - 合計: 約 800 行コード + テスト 80 行 + README 100 行
- **単体テスト最低 8 件**
- **`unsafe` 0 件** (sysinfo + 標準 file open のみ使用)
- **Windows 専用** (`#[cfg(not(windows))] compile_error!`)
- **既存クレートへの変更ゼロ** (workbench-dryrun は独立)

## 完了条件チェックリスト

- [ ] `cargo check -p dds-workbench-dryrun` がエラーなし (Windows 環境)
- [ ] `cargo build --release -p dds-workbench-dryrun` が成功
- [ ] `cargo test -p dds-workbench-dryrun` が全パス (≥8 件)
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy -p dds-workbench-dryrun -- -D warnings`: warning 0
- [ ] README.md が読みやすく、実機テスト手順が明確
- [ ] `target/release/workbench-dryrun.exe list-drives` が手動実行できる
- [ ] バイナリサイズが妥当 (10MB 以下が目安)

## 関連 FR 要件

- **FR-CLI-01** (実機ドライラン用 CLI) ← 達成
- **FR-CLI-02** (対話形式 UX) ← 達成
- **FR-CLI-03** (案件情報の保存) ← Chunk 21 と統合
- **FR-CLI-04** (業務向け出力構造) ← Chunk 23 と統合

## 完了後

1. tester エージェントへ引き継ぎ (Windows 環境でビルド + 単体テスト)
2. テスト合格後、progress-tracker へ
3. **🎉 実機ドライラン準備完了**
4. **Chouさんが検証 PC で実機ドライラン実施** (約半日〜1 日)
5. フィードバック収集 → 必要なら Chunk 23.6 で微調整
6. **Phase 2.1 着手準備完了**

---

## 注意事項

### 物理ドライブ列挙の選択

`\\.\PhysicalDriveN` の直接アクセスは unsafe Windows API が必要なため、Phase 1.5 では:
- **論理ドライブ (\\.\E:)** ベースでアクセス
- USB HDD は Windows が自動マウントするので、ほとんどのケースで動作

例外ケース (Phase 2.1 で対応):
- HDD が破損していて Windows がマウントできない
- パーティションテーブル破損で論理ドライブが認識されない

実機ドライランの初回は健康な HDD で OK。問題のある HDD のテストは Phase 2.1 後。

### 管理者権限の必要性

`\\.\E:` 形式のアクセスは通常管理者権限が必要:
- Windows の保護機能
- 読み取り専用でも管理者要求

ドキュメントに明記、エラー時にメッセージで誘導。

### Wishlist の対話入力の制限

Phase 1.5 では拡張子ベースのみサポート:
- `WishItem::Extension(String)` のみ
- `WishItem::Path` 等は JSON 経由で

これで実機ドライランの大部分のケースをカバー。フル機能の Wishlist 編集は Phase 2.1 UI で。

### バイナリ配布

ビルド成果物:
- `target/release/workbench-dryrun.exe` (Windows バイナリ)

配布方法:
- 検証 PC に直接コピー (USB メモリ等)
- 必要なら installer 化 (Phase 2 で検討)

### Phase 2.1 への引き継ぎ

workbench-dryrun のコードは Phase 2.1 で:
- バックエンド (Tauri) として再利用可能なロジックは流用
- CLI 固有のコード (prompts.rs 等) は破棄

主要な再利用ポイント:
- `drives.rs`: ドライブ列挙ロジック (Tauri バックエンドでも同じ)
- `volume.rs`: NTFS ボリューム open (Tauri バックエンドでも同じ)
- コマンド処理ロジック (diagnose / recover / show) は Tauri Commands に置き換え

### 実機ドライラン後の振り返り項目

実機テスト後、以下を確認:

1. **診断時間**: 1TB / 2TB HDD で何秒だったか
2. **CRM 貼り付けテキストの実用性**: 実際の CRM フォームに貼って違和感ないか
3. **日本語フォルダ名**: 納品先 HDD の `{案件番号}\` 構造が業務的に OK か
4. **エラーケース**: 想定外のエラー (NTFS 認識失敗、書き込み失敗等)
5. **業務フロー全体**: CS の作業時間短縮効果

これらが Phase 2.1 UI 設計の重要なインプット。

---

## 質問が必要なケース

- sysinfo クレートのバージョン互換性問題
- NTFS ボリュームの open API がさらに必要な API を要求する場合
- Wishlist JSON フォーマットが既存と異なる場合

---

## 完了報告例

```markdown
## Chunk 23.5 完了報告

### 新規ファイル
- crates/workbench-dryrun/Cargo.toml
- crates/workbench-dryrun/README.md
- crates/workbench-dryrun/src/main.rs              (60 行)
- crates/workbench-dryrun/src/prompts.rs            (80 行)
- crates/workbench-dryrun/src/drives.rs             (80 行)
- crates/workbench-dryrun/src/volume.rs             (50 行)
- crates/workbench-dryrun/src/commands/mod.rs       (10 行)
- crates/workbench-dryrun/src/commands/list_drives.rs (50 行)
- crates/workbench-dryrun/src/commands/diagnose.rs  (180 行)
- crates/workbench-dryrun/src/commands/recover.rs   (200 行)
- crates/workbench-dryrun/src/commands/show.rs      (100 行)

### バイナリ
- target/release/workbench-dryrun.exe (Windows 専用)

### サブコマンド
- workbench-dryrun list-drives
- workbench-dryrun diagnose
- workbench-dryrun recover
- workbench-dryrun show

### テスト統計
- 単体: 新規 ~10 件 (基本的なヘルパーと引数パース)
- 統合: なし (実機テストで代替)
- 全 workspace: **475+ 件 pass**

### 品質
- clippy 0 warning
- unsafe 0
- Windows 専用 (Linux/Mac でビルド失敗、明示的)

### 検証 PC でのテスト手順
1. リポジトリを検証 PC にクローン or バイナリをコピー
2. 管理者として cmd を開く
3. `workbench-dryrun list-drives` でドライブ確認
4. テスト用 NTFS USB HDD 接続
5. `workbench-dryrun diagnose` で診断実行
6. 出力された CRM 貼り付けテキストを実際の CRM フォームに貼ってレビュー
7. 納品先 HDD 接続
8. `workbench-dryrun recover` で復旧実行
9. 納品先 HDD の `{案件番号}\` 構造をエクスプローラで確認

### 🎉 マイルストーン
- **実機ドライラン準備完了**
- Phase 1.5 完成機能を検証 PC で試せる状態
- CS / エンジニアからの初期フィードバック収集可能
- Phase 2.1 UI 設計のための実機データ取得可能

- **関連 FR**: FR-CLI-01〜04 (達成)

→ tester エージェントへ引き継ぎ (Windows でのビルドと単体テスト)
→ tester 合格後、Chouさんによる検証 PC ドライランへ移行
```
