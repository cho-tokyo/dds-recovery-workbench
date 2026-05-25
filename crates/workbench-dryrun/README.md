# workbench-dryrun - 実機ドライラン用 CLI

DDS Recovery Workbench の Phase 1.5 機能を実機 HDD で試すためのツール。
Phase 2.1 (Tauri UI) 完成までの暫定品。完成後は予備品として残します。

## 必要な準備

1. **管理者として実行**: コマンドプロンプトまたは PowerShell を「管理者として実行」で起動
2. **テスト用 HDD**: 不要な NTFS USB HDD (テストデータ入り) を 1 台
3. **納品先 HDD**: 別の USB HDD (G: ドライブとして認識される想定)

## 使い方

### 接続中のドライブ確認

```cmd
> workbench-dryrun list-drives
```

接続中の全論理ドライブと、NTFS / システムドライブのマーカーを表示します。

### 診断

```cmd
> workbench-dryrun diagnose

案件番号 (yymmdd-NN 形式、例: 260522-04): 260522-04
[ドライブ選択画面]
[診断確認画面]
[診断実行]
[CRM 貼り付けテキスト表示]
```

完了後、`C:\cases\260522-04\` に以下が保存されます:

- `case.json` (案件情報)
- `診断結果_CRM貼り付け用.txt` (CRM 貼り付け用テキスト)

### 復旧

```cmd
> workbench-dryrun recover

案件番号: 260522-04
[ソース HDD 選択]
[納品先 HDD 選択]
[Wishlist 作成 (対話 or JSON)]
[復旧確認画面]
[復旧実行]
```

完了後、納品先 HDD に以下の構造で出力されます:

```
{納品先ドライブ}\{案件番号}\
  ├ 復旧データ\
  │   ├ 通常ファイル\
  │   └ 削除ファイル\
  └ レポート\
      ├ 復旧レポート.docx
      ├ 要確認ファイル一覧.txt
      ├ 業務管理レポート.html
      └ report.csv
```

### 案件情報の表示

```cmd
> workbench-dryrun show

案件番号: 260522-04
[案件情報表示]
```

## 注意事項

- **管理者権限が必須**（HDD 直接アクセスのため）
- **システムドライブ (C:) は対象外**（ソース・納品先とも）
- **対象は NTFS のみ**（exFAT/FAT32 は Phase 2 以降）
- ソース HDD は **read-only** でアクセス（書き込みなし）
- 納品先 HDD には `{案件番号}\` フォルダが自動作成されます

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

優先度は `"Critical"` / `"High"` / `"Normal"` / `"Low"` のいずれか。

## 対話形式 Wishlist 入力時の優先度

`recover` の対話モードでは、以下の入力に対応します（大小区別なし）:

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

### 診断が遅い

HDD のサイズに比例します。1TB HDD で約 30〜60 秒、2TB で 60〜120 秒が目安。
不良セクタが多い場合はさらに時間がかかります。

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
