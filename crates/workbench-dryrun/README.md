# workbench-dryrun - 実機ドライラン用 CLI

DDS Recovery Workbench Phase 1.5 の機能を実機 HDD で試すための CLI ツール。
Phase 2.1 (Tauri UI) 完成までの暫定品。完成後は予備品として残します。

## Phase 1.5 の業務設計

Workbench Phase 1.5 は **R-STUDIO 風の業務フロー** を採用しています:

```
[復旧範囲]
  全 user file を復旧 (NTFS システムファイルを除く)
  システムフォルダ (Windows, Program Files 等) は自動除外
  すべての user file は復旧されます

[Wishlist の役割]
  お客様優先データのラベリング (品質チェック強調用)
  復旧範囲には影響しません
  レポートで「優先データ」として強調表示されます
```

## 業務フロー

DDS の標準業務フロー (診断 PC と復旧 PC は **別の物理 PC**):

```
[診断 PC] (1 台、時分割で複数案件)
   |
   | workbench-dryrun diagnose
   |   |
   |   v
   | C:\cases\{案件番号}\case.json 生成
   | 診断結果_CRM貼り付け用.txt 生成
   |   |
   |   v
   | CS が CRM へ手動コピー&ペースト
   |
   +-> お客様への見積もり -> 正式依頼受領

[復旧 PC] (50 台、1 案件専有)
   |
   | ※ 診断 PC の case.json は受け取らない
   | ※ いきなり workbench-dryrun recover が標準
   |
   | workbench-dryrun recover
   |   |
   |   v
   | 案件 JSON を復旧 PC でゼロから作成
   | 全 user file 復旧 + Wishlist で優先データを強調
   |   |
   |   v
   | 納品 HDD へ業務構造で出力
   |   |
   |   v
   | お客様へ納品
```

## 必要な準備

1. **管理者として実行**: コマンドプロンプトまたは PowerShell を「管理者として実行」で起動
2. **ソース HDD**: お客様の NTFS HDD (USB 接続)
3. **納品先 HDD**: 別の USB HDD (4TB 程度を推奨)

## 使い方

### [診断 PC] 接続中のドライブ確認

```cmd
> workbench-dryrun list-drives
```

接続中の全論理ドライブと、NTFS / システムドライブのマーカーを表示します。

### [診断 PC] 診断

```cmd
> workbench-dryrun diagnose

案件番号 (yymmdd-NN 形式、例: 260522-04): 260522-04
[NTFS ドライブ選択]
[診断確認]
[診断実行 - 30~60 秒]
[CRM 貼り付けテキスト表示]
```

完了後、`C:\cases\260522-04\` に保存:

- `case.json` (案件情報)
- `診断結果_CRM貼り付け用.txt` (CRM 貼り付け用テキスト)

→ CS が `診断結果_CRM貼り付け用.txt` を開いて CRM の入力欄にコピペします。

### [復旧 PC] 復旧

```cmd
> workbench-dryrun recover

案件番号: 260522-04
  ※ 復旧 PC では新規案件として自動作成されます (これが標準フロー)
  ※ 既存出力が検出されたら確認プロンプトが出ます

[NTFS ドライブ選択]
  - ソース  = お客様の HDD (read-only)
  - 納品先  = 復旧データを書き出す HDD (お客様への納品物)

ソース HDD を選択: 1
納品先 HDD を選択: 2

[Wishlist 作成 (対話 or JSON)]
  ※ Wishlist はお客様優先データの指定です (復旧範囲ではない)
  ※ 全 user file は自動的に復旧されます
  ※ Wishlist 指定のファイルは「優先データ」として強調表示されます

[復旧確認]
  - ソース / 納品先の最終確認
  - Wishlist (優先データ) の確認
  - 除外パターン (システムファイル) の表示

[復旧実行]
[結果表示: 全体 + お客様優先データの二重表示]
```

完了後、納品先 HDD に `{案件番号}\` フォルダ構造で出力:

```
G:\260522-04\
  ├ 復旧データ\
  │   ├ 通常ファイル\
  │   └ 削除ファイル\
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
[案件情報表示: 診断結果 / Wishlist / 復旧結果サマリ]
```

## 復旧範囲とシステムファイルの扱い

Workbench は **全 user file を復旧** します。ただし以下は自動的に除外されます。

### 除外されるシステムファイル (ExclusionList)

パスベース除外:

- `\Windows\`               (Windows OS)
- `\Program Files\`         (アプリケーション)
- `\Program Files (x86)\`   (32-bit アプリケーション)
- `\$Recycle.Bin\`          (ゴミ箱)
- `\System Volume Information\` (System Restore データ)
- `\$Extend\`               (NTFS メタデータ)

ファイル名ベース除外:

- `$` で始まるファイル (`$MFT`, `$Bitmap`, `$Boot` などの NTFS システムファイル)

### Wishlist の役割

Wishlist は **お客様優先データのラベリング** です (Chunk 23.7 以降):

```
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

お客様要望による優先納品 (例: 「写真だけ先に納品、残りは後日」):

### 1 回目の復旧 (優先データのみ)

```cmd
> workbench-dryrun recover

案件番号: 260522-04
ソース: 1 (お客様 HDD)
納品先: 2 (G:\ の HDD)
Wishlist: 写真データのみ
```

→ G:\ 上に `G:\260522-04\` 構造で出力 → お客様へ納品

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

→ H:\ 上に `H:\260522-04\` 構造で出力 → お客様へ納品

注: 容量超過による分割納品 (例: 5TB を 4TB の HDD 2 つに分ける) は Phase 2 で対応予定。
Phase 1.5 では Wishlist で論理的に分けて複数回復旧する運用です。

## 注意事項

- **管理者権限が必須** (HDD 直接アクセスのため)
- **システムドライブ (C:) は対象外** (ソース・納品先とも)
- **対象は NTFS のみ** (exFAT/FAT32 は Phase 2 以降)
- ソース HDD は **read-only** でアクセス (書き込みなし)
- 納品先 HDD には `{案件番号}\` フォルダが自動作成されます
- 同じ案件番号で 2 回目以降の recover は、既存ディレクトリ上書き警告が出ます

## Wishlist JSON フォーマット

```json
{
  "wishes": [
    {
      "label": "写真データ (お客様優先)",
      "item": { "Extension": "jpg" },
      "priority": "High"
    },
    {
      "label": "Office ファイル",
      "item": { "Extension": "docx" },
      "priority": "High"
    }
  ]
}
```

優先度は `"Critical"` / `"High"` / `"Normal"` / `"Low"` のいずれか。

## 対話形式 Wishlist 入力時の優先度

`recover` の対話モードでは、以下の入力に対応します (大小区別なし):

- `critical` / `c` → Critical
- `high` (デフォルト) → High
- `normal` / `n` → Normal
- `low` / `l` → Low

未知の値はデフォルトの High になります。

## トラブルシューティング

### 「ドライブを開けません」エラー

管理者として実行していない可能性があります。コマンドプロンプトを
「管理者として実行」で起動し直してください。

### 「NTFS ボリュームの open に失敗」エラー

対象 HDD が NTFS でない、または FS が壊れている可能性があります。
`list-drives` で FS を確認してください。

### 「案件が見つかりません」エラー (recover で)

Chunk 23.6 以降では発生しません (自動的に新規作成)。古いバージョンを使っている
場合は更新が必要です。

### 「納品先に既にこの案件のフォルダが存在します」警告

2 回目以降の納品の場合は続行で OK。意図しない場合は別の案件番号 /
別の納品先を選択してください。

### 「Wishlist が空です」警告

お客様の主訴が「全部復旧」の場合は続行で OK。優先データなしのレポートになります。

### 診断が遅い

HDD のサイズに比例します。1TB HDD で約 30〜60 秒、2TB で 60〜120 秒が目安。
不良セクタが多い場合はさらに時間がかかります。

### 復旧件数が予想より多い

Phase 1.5 は **全 user file を復旧** する設計です (R-STUDIO 風)。
Wishlist は優先データのラベリングのみで、復旧範囲には影響しません。
復旧件数が少ない場合は ExclusionList の除外パターンで対応してください。

## 手動テスト手順 (検証 PC)

1. 検証 PC で管理者として cmd または PowerShell を開く
2. `workbench-dryrun list-drives` でドライブ一覧を確認
3. テスト用 NTFS USB HDD を接続
4. `workbench-dryrun diagnose` で診断実行
5. 出力された CRM 貼り付けテキストを実際の CRM フォームに貼ってレビュー
6. 納品先 HDD を接続
7. `workbench-dryrun recover` で復旧実行
8. 納品先 HDD の `{案件番号}\` 構造をエクスプローラで確認
9. `workbench-dryrun show` で案件情報を再確認

## 制約 / 既知の制限

- Windows 専用 (Linux/Mac でビルド不可)
- 論理ドライブのみサポート (`\\.\PhysicalDriveN` は Phase 2.1)
- Wishlist 対話入力は拡張子ベースのみ (フル機能は Phase 2.1 UI)
- 進捗バーなし (黙々と実行)
- 案件番号の照合は人的運用 (CRM 画面と CS による目視確認)
