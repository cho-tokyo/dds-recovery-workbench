# Chunk 23.6 指示 (改訂版): workbench-dryrun の業務フロー対応

> 📝 この指示書は Chunk 23.7 完了後に再作成された **改訂版** です。Wishlist の新しい意味 (お客様優先データ) と ExclusionList の概念を反映しています。

Chunk 23.5 で実装した workbench-dryrun を、**業務フローと Chunk 23.7 の新しい設計**に合わせて改修します:

1. **「いきなり recover」を許容** (案件 JSON 不在時は新規作成)
2. **既存出力の検出と確認** (同じ案件で 2 回目 recover の対応)
3. **README の業務フロー強化** (Wishlist の新意味、ExclusionList の説明、診断 PC vs 復旧 PC)

> 🎯 完了時点で「復旧 PC で workbench-dryrun を独立して動かせる」状態になる。Chunk 23.7 の業務適用品質と統合された最終形に到達。

---

## 背景

### Chunk 23.5 完了時点の状態

workbench-dryrun CLI は動くが:
- 案件 JSON が無いとエラーで止まる
- 同じ案件で 2 回目 recover した時の挙動が未定義
- README が業務フローを反映していない

### Chunk 23.7 で大きな変更が入った

- Wishlist の意味再定義 (復旧対象 → お客様優先データ)
- ExclusionList が新規導入
- 全件復旧 + システムファイル除外が標準動作

### Chunk 23.6 改訂版が必要な理由

```
[23.5 完了時点の workbench-dryrun]
- 23.5 のロジックで実装
- 業務フロー未対応

[23.7 完了時点]
- workbench-dryrun は 23.7 の API 更新には追従済み (execute_business_recovery が exclusions を取る)
- ただし「いきなり recover」と「既存出力検出」は未対応
- README は古い (Wishlist の旧意味のまま)

[23.6 改訂版で対応]
- いきなり recover 対応
- 既存出力検出 + 確認
- README を Chunk 23.7 の新仕様で全面更新
```

## 業務フローの正しい理解

```
[診断 PC] (1 台、時分割で複数案件)
   workbench-dryrun diagnose
   → C:\cases\{案件番号}\case.json
   → CRM 貼り付けテキスト
   → ソース HDD 取り外し

[CRM 経由でお客様正式依頼]

[復旧 PC] (50 台、1 案件専有)
   ← 診断 PC の case.json は届かない
   ← 「いきなり workbench-dryrun recover」が標準
   
   workbench-dryrun recover
   → 案件 JSON を復旧 PC でゼロから作成
   → 全 user file を復旧 (R-STUDIO 風)
   → Wishlist で「お客様優先データ」を強調
   → ExclusionList でシステムファイル除外
   → 納品 HDD へ業務構造で出力
```

## 目的

3 つの改修:

### A. `recover` コマンドの「いきなり実行」対応

```rust
// 修正後:
let case_already_exists = storage.case_file_path(&case_id).exists();
let mut case = if case_already_exists {
    println!("既存案件を読み込みます: {}", case_id);
    storage.load(&case_id)?
} else {
    println!("新規案件を作成します: {}", case_id);
    storage.create_new(case_id.clone())?
};
```

### B. 既存出力の検出と確認

```rust
let case_output_root = delivery_drive.mount_point.join(case_id.as_str());
if case_output_root.exists() {
    println!();
    println!("⚠ 納品先に既にこの案件のフォルダが存在します:");
    println!("    {}", case_output_root.display());
    println!();
    println!("これは 2 回目以降の納品ですか? (お客様への優先納品など)");
    if !confirm("続行すると既存ファイルが上書きされる可能性があります。続行しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }
}
```

### C. README の Chunk 23.7 対応版

- Wishlist は「お客様優先データ」と明記
- ExclusionList の役割を説明
- 「全体 vs 優先データ」の二重表示を説明
- 業務フロー (診断 PC vs 復旧 PC) を維持

## 対象クレート

`crates/workbench-dryrun/` (Chunk 23.5/23.7 で実装、本チャンクで改修)

## 実装内容

### Part A: `commands/recover.rs` の改修

```rust
use std::path::PathBuf;
use anyhow::{anyhow, Context, Result};
use dds_case_manager::{
    execute_business_recovery, CaseOutput, CaseStorage,
};
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number, prompt_string};
use crate::volume::open_ntfs_volume;

pub fn run() -> Result<()> {
    println!("🔧 復旧モード");
    println!("---------------------------------------------");
    println!();
    
    // Step 1: 案件番号入力
    let case_id = prompt_case_id()?;
    println!();
    
    // Step 2: 案件 JSON の load または新規作成 (★ 改修部分)
    let storage = CaseStorage::default_location();
    let case_file = storage.case_file_path(&case_id);
    let case_already_exists = case_file.exists();
    
    let mut case = if case_already_exists {
        println!("📂 既存の案件を読み込みます: {}", case_id);
        let loaded = storage.load(&case_id)?;
        if loaded.diagnostic_input.diagnosed_at.is_some() {
            println!("  診断済み (この PC で診断 → 復旧のフロー)");
        }
        if loaded.recovery_report_summary.is_some() {
            println!("  ⚠ この案件は既に 1 回以上復旧されています。");
            println!("     2 回目以降の納品の場合は続行してください。");
        }
        loaded
    } else {
        println!("📝 案件が見つかりません。新規作成して復旧を進めます。");
        println!("  (復旧 PC では「いきなり復旧」が標準フローです)");
        storage.create_new(case_id.clone())?
    };
    println!();
    
    // Step 3: ソース HDD と納品先 HDD の選択
    println!("接続中の NTFS ドライブ:");
    let drives: Vec<_> = list_drives().into_iter()
        .filter(|d| d.is_ntfs() && !d.is_system_drive())
        .collect();
    
    if drives.is_empty() {
        return Err(anyhow!("操作可能な NTFS ドライブが見つかりません。\n少なくとも 2 つの USB HDD (ソースと納品先) を接続してください。"));
    }
    
    if drives.len() < 2 {
        return Err(anyhow!("ソース HDD と納品先 HDD の両方を接続してください。\n現在 {} 個のドライブのみ検出されています。", drives.len()));
    }
    
    for (i, drive) in drives.iter().enumerate() {
        println!("  [{}] {} ({}, {})",
            i + 1,
            drive.drive_letter,
            drive.label,
            dds_core::format::format_bytes(drive.total_bytes));
    }
    println!();
    println!("⚠ ソース = お客様の HDD (読み取り専用でアクセス)");
    println!("⚠ 納品先 = 復旧データを書き出す HDD (お客様への納品物)");
    println!();
    
    let src_sel = prompt_number("ソース HDD (お客様の HDD) を選択", 1, drives.len())?;
    let source_drive = drives[src_sel - 1].clone();
    
    let dst_sel = prompt_number("納品先 HDD を選択", 1, drives.len())?;
    let delivery_drive = drives[dst_sel - 1].clone();
    
    if source_drive.drive_letter == delivery_drive.drive_letter {
        return Err(anyhow!("ソースと納品先が同じドライブです。別のドライブを選択してください。"));
    }
    
    // Step 4: 既存出力の検出 (★ 改修部分)
    let case_output_root = delivery_drive.mount_point.join(case_id.as_str());
    
    if case_output_root.exists() {
        println!();
        println!("⚠ 納品先に既にこの案件のフォルダが存在します:");
        println!("    {}", case_output_root.display());
        println!();
        println!("考えられるケース:");
        println!("  1. 2 回目以降の納品 (優先納品など)");
        println!("  2. 別の案件で同じ番号を使ってしまった");
        println!("  3. 前回の復旧が失敗した残骸");
        println!();
        println!("続行すると既存ファイルが上書きされる可能性があります。");
        if !confirm("続行しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }
    
    // Step 5: Wishlist 入力 (★ Chunk 23.7 で意味が変わった)
    println!();
    println!("Wishlist の入力 (お客様優先データの指定):");
    println!("  ※ Workbench は全 user file を復旧します");
    println!("  ※ Wishlist はレポートで「優先データ」として強調表示する対象です");
    println!();
    let wishlist = prompt_wishlist()?;
    
    if wishlist.wishes.is_empty() {
        println!("⚠ Wishlist が空です。「全体」のみのレポートになります (優先データセクションなし)。");
        if !confirm("続行しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }
    
    // Step 6: ExclusionList (デフォルト、業務標準) を使用
    let exclusions = ExclusionList::default_system_exclusions();
    
    // Step 7: 確認 (★ 改修部分: 除外パターン表示)
    println!();
    println!("---------------------------------------------");
    println!("確認:");
    println!("  案件番号:       {}", case_id);
    println!("  ソース:         {} ({})", source_drive.drive_letter, source_drive.label);
    println!("  納品先:         {} ({})", delivery_drive.drive_letter, delivery_drive.label);
    println!();
    println!("  Wishlist (優先データ): {}", 
        if wishlist.wishes.is_empty() { "なし".to_string() } else { format!("{} 項目", wishlist.wishes.len()) });
    for (i, wish) in wishlist.wishes.iter().enumerate() {
        println!("    {}: 「{}」", i + 1, wish.label);
    }
    println!();
    println!("  除外パターン (システムファイル):");
    println!("    - Windows / Program Files フォルダ");
    println!("    - $Recycle.Bin, System Volume Information");
    println!("    - $ で始まるシステムファイル");
    println!();
    println!("出力先: {}\\", case_output_root.display());
    println!("---------------------------------------------");
    println!();
    
    if !confirm("復旧を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }
    
    // Step 8: 復旧実行
    println!();
    println!("[復旧中... 完了まで時間がかかる場合があります]");
    let start = std::time::Instant::now();
    
    let mut volume = open_ntfs_volume(&source_drive.access_path)?;
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
        &exclusions,
    ).context("復旧の実行に失敗しました")?;
    
    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();
    
    // Step 9: 結果表示 (★ Chunk 23.7 の二重表示)
    println!("結果 (全体):");
    println!("  該当ファイル:    {} 件", result.report.total_matched);
    println!("  復旧成功:        {} 件 ({:.1}%)",
        result.report.recovered.len(),
        result.report.recovery_success_rate());
    println!("  品質保証率:      {:.1}%", result.report.quality_assurance_rate());
    println!("  復旧データ量:    {}", dds_core::format::format_bytes(result.report.total_bytes_written()));
    println!();
    
    let priority_count = result.report.priority_count();
    if priority_count > 0 {
        println!("結果 (お客様優先データ):");
        println!("  該当ファイル:    {} 件", priority_count);
        println!("  品質保証率:      {:.1}%", result.report.priority_quality_assurance_rate());
        println!();
    }
    
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
    
    // Step 10: case.json 永続化
    storage.save(&case)?;
    println!("✓ 案件情報を保存しました: {}", storage.case_file_path(&case_id).display());
    
    Ok(())
}

// prompt_wishlist 関数は Chunk 23.5 のまま (変更なし)
// ...
```

### Part B: README.md の業務フロー強化

`crates/workbench-dryrun/README.md` を以下の構成に書き換え:

```markdown
# workbench-dryrun - 実機ドライラン用 CLI

DDS Recovery Workbench Phase 1.5 の機能を実機 HDD で試すためのツール。
Phase 2.1 (Tauri UI) 完成までの暫定品。

## Phase 1.5 の業務設計

Workbench Phase 1.5 は **R-STUDIO 風の業務フロー** を採用しています:

```
[復旧範囲]
- 全 user file を復旧 (NTFS システムファイルを除く)
- システムフォルダ (Windows, Program Files 等) は自動除外
- すべての user file は復旧されます

[Wishlist の役割]
- お客様優先データのラベリング (品質チェック強調用)
- 復旧範囲には影響しません
- レポートで「優先データ」として強調表示されます
```

## 業務フロー

DDS の標準業務フロー:

```
[診断 PC] (1 台、時分割で複数案件)
   │
   │ workbench-dryrun diagnose
   │   ↓
   │ C:\cases\{案件番号}\case.json 生成
   │ CRM 貼り付けテキスト生成
   │   ↓
   │ CRM へ手動コピー&ペースト
   │
   └─→ お客様への見積もり → 正式依頼受領
   
[復旧 PC] (50 台、1 案件専有)
   │
   │ ※ 診断 PC の case.json は受け取らない
   │ ※ いきなり workbench-dryrun recover が標準
   │
   │ workbench-dryrun recover
   │   ↓
   │ 案件 JSON を復旧 PC でゼロから作成
   │ 全 user file 復旧 + Wishlist で優先データを強調
   │   ↓
   │ 納品 HDD へ業務構造で出力
   │   ↓
   │ お客様へ納品
```

## 必要な準備

1. **管理者として実行**: コマンドプロンプトを「管理者として実行」で起動
2. **ソース HDD**: お客様の NTFS HDD (USB 接続)
3. **納品先 HDD**: 別の USB HDD (4TB 程度を推奨)

## 使い方

### [診断 PC] 接続中のドライブ確認

```cmd
> workbench-dryrun list-drives
```

### [診断 PC] 診断

```cmd
> workbench-dryrun diagnose

案件番号 (yymmdd-NN 形式): 260522-04
[NTFS ドライブ選択]
[診断確認]
[診断実行 - 30~60 秒]
[CRM 貼り付けテキスト表示]
```

完了後、`C:\cases\260522-04\` に保存:
- `case.json` (案件情報)
- `診断結果_CRM貼り付け用.txt`

→ CS が `診断結果_CRM貼り付け用.txt` を開いて CRM の入力欄にコピペ。

### [復旧 PC] 復旧

```cmd
> workbench-dryrun recover

案件番号: 260522-04
  ※ 復旧 PC では新規案件として扱われます (これが標準です)
  ※ 既存出力検出時は確認プロンプトが出ます

[NTFS ドライブ選択]
  ⚠ ソース = お客様の HDD (read-only)
  ⚠ 納品先 = 復旧データの書き出し先

ソース HDD を選択: 1
納品先 HDD を選択: 2

[Wishlist 作成 (対話 or JSON)]
  ※ Wishlist はお客様優先データの指定です (復旧範囲ではない)
  ※ 全 user file は自動的に復旧されます
  ※ Wishlist 指定のファイルは「優先データ」として強調

[復旧確認]
  - ソース/納品先の最終確認
  - Wishlist (優先データ) の確認
  - 除外パターン (システムファイル) の表示

[復旧実行]
```

完了後、納品先 HDD に `{案件番号}\` フォルダ構造で出力:

```
G:\260522-04\
├ 復旧データ\
│  ├ 通常ファイル\
│  └ 削除ファイル\
└ レポート\
   ├ 復旧レポート.docx          ← お客様向け、Word で開く
   ├ 要確認ファイル一覧.txt     ← お客様向け、Notepad で開く
   ├ 業務管理レポート.html      ← 社内用、ブラウザで開く
   └ report.csv                  ← 外部システム連携用
```

社内には `C:\cases\260522-04\case.json` が残ります (再復旧依頼に備えて)。

### [復旧 PC] 案件情報の表示

```cmd
> workbench-dryrun show

案件番号: 260522-04
[案件情報表示]
```

## 復旧範囲とシステムファイルの扱い

Workbench は **全 user file を復旧** します。ただし以下は自動的に除外されます:

### 除外されるシステムファイル

```
パスベース除外:
  \Windows\               (Windows OS)
  \Program Files\         (アプリケーション)
  \Program Files (x86)\   (32-bit アプリケーション)
  \$Recycle.Bin\          (ゴミ箱)
  \System Volume Information\ (System Restore データ)
  \$Extend\               (NTFS メタデータ)

ファイル名ベース除外:
  $ で始まるファイル    ($MFT, $Bitmap, $Boot などの NTFS システムファイル)
```

### Wishlist の役割

```
Wishlist は「お客様優先データ」のラベリングです:

[例] お客様の主訴: 「写真データだけ重要」
  → Wishlist: Extension("jpg"), Extension("png")
  
  Workbench は:
    - 全 user file を復旧 (写真以外も含めて)
    - 写真ファイルは「優先データ」として is_priority = true
    - レポートで「優先データ」と「全体」の二重表示

[例] お客様の主訴: 「全部復旧してほしい」
  → Wishlist: 空 (or 何も指定しない)
  
  Workbench は:
    - 全 user file を復旧
    - 優先データはなし
    - レポートは「全体」のみ表示
```

## 複数 HDD への分割納品 (優先納品)

お客様要望による優先納品 (例: 「写真だけ先に納品して残りは後日」):

### 1 回目の復旧

```cmd
> workbench-dryrun recover

案件番号: 260522-04
ソース: 1 (お客様 HDD)
納品先: 2 (G:\ の HDD)
Wishlist: 写真データのみ
```

→ G: 上に `G:\260522-04\` 構造で出力 → お客様へ納品

### 2 回目の復旧 (別の納品先 HDD)

```cmd
> workbench-dryrun recover

案件番号: 260522-04
  ※ 「この案件は既に 1 回以上復旧されています」と表示される
  ※ 続行で OK

ソース: 1 (お客様 HDD、再接続)
納品先: 3 (H:\ の別 HDD)
Wishlist: Office、PDF など
```

→ H: 上に `H:\260522-04\` 構造で出力 → お客様へ納品

⚠ 容量超過による分割納品 (例: 5TB を 4TB の HDD 2 つに分ける) は Phase 2 で対応予定。
   Phase 1.5 では Wishlist で論理的に分けて複数回復旧する運用。

## 注意事項

- **管理者権限が必須** (HDD 直接アクセスのため)
- **システムドライブ (C:) は対象外** (ソース・納品先とも)
- **対象は NTFS のみ** (exFAT/FAT32 は Phase 2 以降)
- ソース HDD は **read-only** でアクセス (書き込みなし)
- 納品先 HDD には `{案件番号}\` フォルダが作成されます
- 同じ案件番号で 2 回目以降の recover は、既存ディレクトリ上書き警告が出ます

## Wishlist JSON フォーマット

```json
{
  "wishes": [
    {
      "label": "写真データ (お客様優先)",
      "item": { "kind": "Extension", "value": "jpg" },
      "priority": "High"
    },
    {
      "label": "Office ファイル",
      "item": { "kind": "Extension", "value": "docx" },
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

### 「案件が見つかりません」エラー (recover で)
→ Chunk 23.6 以降では発生しません (自動的に新規作成)。古いバージョンを使っている場合は更新が必要。

### 「納品先に既にこの案件のフォルダが存在します」警告
→ 2 回目以降の納品の場合は続行で OK。意図しない場合は別の案件番号 / 別の納品先を選択。

### 「Wishlist が空です」警告
→ お客様の主訴が「全部復旧」の場合は続行で OK。優先データなしのレポートになります。

### 診断が遅い
→ HDD のサイズに比例。1TB HDD で約 30-60 秒、2TB で 60-120 秒が目安。
   不良セクタが多い場合はさらに時間がかかる。

### 復旧件数が予想より多い
→ Phase 1.5 は全 user file を復旧する設計です (R-STUDIO 風)。
   Wishlist は優先データのラベリングのみで、復旧範囲には影響しません。
```

### Part C: 新規テスト

`crates/workbench-dryrun/src/commands/recover.rs` のテスト追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dds_case_manager::{CaseId, CaseStorage};
    use tempfile::TempDir;
    
    #[test]
    fn recover_creates_new_case_when_not_exists() {
        let temp = TempDir::new().unwrap();
        let storage = CaseStorage::with_base_dir(temp.path());
        let case_id = CaseId::parse("260522-04").unwrap();
        
        assert!(!storage.case_file_path(&case_id).exists());
        
        let case = storage.create_new(case_id.clone()).unwrap();
        assert_eq!(case.case_id, case_id);
        assert!(case.diagnostic_input.diagnosed_at.is_none());
    }
    
    #[test]
    fn recover_loads_existing_case_when_present() {
        let temp = TempDir::new().unwrap();
        let storage = CaseStorage::with_base_dir(temp.path());
        let case_id = CaseId::parse("260522-04").unwrap();
        
        let case = storage.create_new(case_id.clone()).unwrap();
        storage.save(&case).unwrap();
        
        let loaded = storage.load(&case_id).unwrap();
        assert_eq!(loaded.case_id, case_id);
    }
    
    #[test]
    fn existing_output_directory_detection_logic() {
        let temp = TempDir::new().unwrap();
        let case_id = CaseId::parse("260522-04").unwrap();
        let case_output_root = temp.path().join(case_id.as_str());
        
        assert!(!case_output_root.exists());
        
        std::fs::create_dir_all(&case_output_root).unwrap();
        assert!(case_output_root.exists());  // 既存検出が動く
    }
}
```

## 制約

- **行数目安**:
  - `commands/recover.rs`: +50 行 (改修部分)
  - `README.md`: +100 行 (業務フロー説明強化、Chunk 23.7 反映)
  - テスト: +30 行
- **単体テスト**: +3 件
- **`unsafe` 0 件** (Chunk 23.5 から維持)
- **既存テスト**: 影響なし
- **Windows 専用** (Chunk 23.5 から維持)

## 完了条件チェックリスト

- [ ] `cargo check -p dds-workbench-dryrun` がエラーなし
- [ ] `cargo build --release -p dds-workbench-dryrun` が成功
- [ ] `cargo test -p dds-workbench-dryrun` が全パス
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy -p dds-workbench-dryrun -- -D warnings`: warning 0
- [ ] README.md に Chunk 23.7 の新仕様 (Wishlist の意味、ExclusionList) が明記されている
- [ ] README.md に業務フロー (診断 PC vs 復旧 PC) が明記されている
- [ ] README.md に複数 HDD 分割納品の運用方法が明記されている
- [ ] recover コマンドで案件 JSON 不在時に新規作成される
- [ ] recover コマンドで既存出力検出時に確認プロンプトが出る
- [ ] recover コマンドの確認画面で除外パターン (システムファイル) が表示される
- [ ] 結果表示で「全体」と「お客様優先データ」の二重表示が出る

## 関連 FR 要件

- **FR-CLI-05** (復旧 PC 独立運用) ← 達成
- **FR-CLI-06** (複数回 recover 対応) ← 達成
- **FR-CLI-07** (Chunk 23.7 新仕様の反映) ← 新規達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **🎉 workbench-dryrun が業務フローと Chunk 23.7 新仕様に完全対応**
4. 次のステップ:
   - **Chunk 23.8**: Uncertain 理由分類 + TXT 分割
   - Chouさんが検証 PC で実機ドライラン (Chunk 23.8 完了後 or 23.6 完了後でも可能)

---

## 注意事項

### Chunk 23.7 で既に実装済みの可能性

Chunk 23.7 で `workbench-dryrun/src/commands/recover.rs` も更新されているはず:
- `execute_business_recovery(&mut case, ..., &wishlist, &exclusions)` への対応
- ExclusionList::default_system_exclusions() の使用

実装前に現状を確認:
```bash
grep -n "exclusions" crates/workbench-dryrun/src/commands/recover.rs
grep -n "ExclusionList" crates/workbench-dryrun/src/commands/recover.rs
```

これが既に実装されているなら、本 Chunk 23.6 では:
- 案件 JSON 不在対応 (A)
- 既存出力検出 (B)
- README 更新 (C)

の 3 つに集中。Chunk 23.7 で実装された部分は触らない。

### 案件番号の重複チェック

「いきなり recover」を許容することで、案件番号の取り違えリスクが上がる:
- 復旧 PC で誤って別の案件番号を入力 → 別の案件として新規作成
- → 業務的に混乱

対策:
- 復旧 PC の CS が必ず CRM 画面と案件番号を照合する運用 (人的)
- workbench-dryrun の確認画面で案件番号を再表示 (実装済み)

技術的な防御は限定的。運用ルールで補完。

### 既存出力の検出ロジック

```rust
let output_root = delivery_drive.mount_point.join(case_id.as_str());
if output_root.exists() { ... }
```

→ 単純なディレクトリ存在チェック。完璧ではないが業務的に十分:
- 「同じ案件で 2 回目」は検出できる
- 「別の案件で同じ番号」も検出できる (CS が判断)
- 「前回の失敗の残骸」も検出できる (CS が判断)

### Wishlist が空の許容

Chunk 23.7 で「Wishlist が空 = 全件復旧、優先データなし」が正式な業務シナリオに:
- お客様の主訴: 「全部復旧してほしい」
- → CS は Wishlist を作らず recover 実行
- → レポートは「全体」のみ、「優先データ」セクションは省略

これに対応:
- 確認プロンプトで「Wishlist 空ですが続行しますか?」と聞く
- 続行可能 (エラーにしない)

### diagnose を診断 PC で実施した場合の連携

業務的に重要だが、現状非対応:
- 診断 PC で `C:\cases\260522-04\case.json` 生成
- 復旧 PC へ手動転送 (USB メモリ等)?
- ネットワーク共有フォルダ?

これは workbench-dryrun の責務外。組織の運用ルールで決める:
- 案 A: 連携しない (復旧 PC は独立、診断結果は CRM 経由)
- 案 B: ネットワーク共有 (`\\fileserver\cases\`)
- 案 C: 手動コピー

DDS の現状: 案 A (CRM 経由) で運用、Workbench は何もしない。

### Phase 2.1 UI への影響

workbench-dryrun の改修内容は Phase 2.1 UI でも同じ業務フローを実装する必要:
- 「いきなり recover」のフロー
- 既存出力検出と確認ダイアログ
- 複数 HDD 分割納品の UX
- Wishlist は「お客様優先データ」と明示
- ExclusionList の編集 UI (デフォルト + カスタム)

これらは UI で実装するときの仕様として固める。

---

## 質問が必要なケース

- 業務的に「2 回目 recover」の確認プロンプトが煩わしい場合 (毎回 OK を押す手間)
- 既存出力検出のロジックを強化したい場合 (タイムスタンプ比較など)
- Chunk 23.7 で workbench-dryrun の updates が既に入っているか確認したい場合

---

## 完了報告例

```markdown
## Chunk 23.6 (改訂版) 完了報告

### 修正ファイル
- crates/workbench-dryrun/src/commands/recover.rs (案件 JSON 不在対応、既存出力検出、+50 行)
- crates/workbench-dryrun/README.md (Chunk 23.7 反映、業務フロー強化、+100 行)

### 新規テスト
- recover_creates_new_case_when_not_exists
- recover_loads_existing_case_when_present
- existing_output_directory_detection_logic

### テスト統計
- 単体: 既存 + 新規 3 件 = **493+ 件 pass**
- 全 workspace: **493+ 件 pass**

### 品質
- clippy 0 warning, unsafe 0
- 既存 Chunk 23.5/23.7 機能への破壊的変更なし

### 業務フロー対応 (新)
- 復旧 PC で「いきなり recover」が動作
- 案件 JSON 不在時の自動新規作成
- 既存出力検出時の確認プロンプト
- 複数 HDD 分割納品の運用手順を README に明記
- Wishlist は「お客様優先データ」の意味を README で明示
- ExclusionList の役割を README で明示

### 🎉 マイルストーン
- **workbench-dryrun が業務フローと Chunk 23.7 新仕様に完全対応**
- 診断 PC と復旧 PC の役割分離が実装に反映
- 検証 PC での実機ドライラン準備が業務適用品質に到達

- **関連 FR**: FR-CLI-05、FR-CLI-06、FR-CLI-07 (達成)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chunk 23.8 (Uncertain 分類 + TXT 分割) または検証 PC ドライラン
```
