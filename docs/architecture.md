# アーキテクチャ概要

## 全体構成

```
┌─────────────────────────────────────────────┐
│ Tauri UI Layer                              │
│ (React + TypeScript + Tailwind)             │
│   - 案件管理画面                              │
│   - 診断・希望リスト・復旧・品質レビュー画面     │
│   - レポート画面                              │
└────────────┬────────────────────────────────┘
             │ Tauri Commands (JSON-RPC)
┌────────────▼────────────────────────────────┐
│ Application Layer (Rust)                    │
│   - case-manager: 案件ライフサイクル          │
│   - 業務ワークフロー オーケストレーション         │
│   - report: レポート生成                      │
└────────────┬────────────────────────────────┘
             │
┌────────────▼────────────────────────────────┐
│ Core Engines (Rust)                         │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│ │diagnostic│ │wish-match│ │ quality  │      │
│ └──────────┘ └──────────┘ └──────────┘      │
│ ┌──────────────────────────────────────┐    │
│ │ FS Readers (fs-ntfs/fs-exfat/fs-fat32│    │
│ │ + fs-common 共通トレイト)             │    │
│ └──────────────────────────────────────┘    │
│ ┌──────────────────────────────────────┐    │
│ │ recovery: 目標駆動抽出パイプライン      │    │
│ └──────────────────────────────────────┘    │
│ ┌──────────────────────────────────────┐    │
│ │ validators: ファイル形式別バリデータ群   │    │
│ └──────────────────────────────────────┘    │
└────────────┬────────────────────────────────┘
             │
┌────────────▼────────────────────────────────┐
│ Infrastructure Layer                        │
│ ┌──────────────┐ ┌──────────────────────┐   │
│ │ disk-io      │ │ db (SQLite)          │   │
│ │ (read-only)  │ │                      │   │
│ └──────────────┘ └──────────────────────┘   │
│ ┌──────────────────────────────────────┐    │
│ │ core: 共通型・エラー定義              │    │
│ └──────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

## クレート間依存関係

```
core ← (全クレート)

disk-io → core
fs-common → core, disk-io
fs-ntfs → core, fs-common
fs-exfat → core, fs-common
fs-fat32 → core, fs-common
diagnostic → core, fs-common, fs-ntfs, fs-exfat, fs-fat32, disk-io
wish-match → core, fs-common
recovery → core, fs-common, disk-io
quality → core, validators
validators → core
report → core, db
db → core
case-manager → core, db, diagnostic, wish-match, recovery, quality, report
```

依存方向は単方向。循環依存は禁止。

## 業務フローのコード上の表現

```
[業務フロー]              [対応モジュール]
診断                    → diagnostic
壊れた箇所特定           → diagnostic (詳細分析)
お客様希望リスト突合      → wish-match
復旧                    → recovery
品質チェック             → quality + validators
達成度確認              → case-manager (集計)
復旧完了                → report
```

## 安全性の境界

### read-only保証

- `disk-io` クレートは型レベルで `ReadOnlyDisk` トレイトのみ公開
- 書き込みAPIは存在しない（実装しない）
- 出力先は別の `OutputStorage` トレイトで明示分離

### エラー伝播

```
低レベル (disk-io)
  ↓ DiskIoError
中レベル (fs-*)
  ↓ FsError (CoreError変換)
業務層 (case-manager)
  ↓ anyhow::Error (コンテキスト付与)
UI層 (Tauri commands)
  ↓ シリアライズ可能なエラーDTO
```

## 設計原則

### 1. 単一責任
各クレートは1つの責務に集中。FS実装に診断ロジックを混ぜない、など。

### 2. インターフェース分離
共通トレイトは `fs-common` 等の中立クレートに置く。具象実装は知らない設計。

### 3. テスタビリティ
- I/Oはトレイト経由でモック可能
- パーサは純粋関数（`&[u8] -> Result<T, Error>`）を基本形に
- 副作用は最小化

### 4. 段階的開示
低レベルAPIと高レベルAPIを分離。CSが触るのは高レベルのみ。

## モジュール責務詳細

### core
- エラー型、結果型エイリアス、損傷レベル enum、品質評価 enum
- 全クレートの基盤

### disk-io
- Raw disk access の抽象化（trait）
- Windows/Linux/macOS 別の具象実装
- イメージファイル読み込み実装
- セクタキャッシュ

### fs-common
- `FsReader` トレイト（全FSが実装する共通インタフェース）
- `FsEntry`, `FsType`, `FileMetadata` などの共通型
- アクセス制御（read-only enforcement）

### fs-ntfs / fs-exfat / fs-fat32
- 各FS固有のパース処理
- `FsReader` トレイトを実装
- メタデータ生存・削除判定

### diagnostic
- ディスク状態の分類（L1〜L6）
- パーティション構造解析
- FS識別と健全性チェック
- 戦略提案・推定時間算出

### wish-match
- お客様希望リストのデータモデル
- 突合エンジン（ファイル名・パターン・パス・拡張子・日付・サイズ）
- 発見可能性スコアリング

### recovery
- 目標駆動抽出（希望リストとマッチした候補を優先抽出）
- 抽出方法の決定（L1/L2/L3）
- I/Oエラーリトライ
- 進捗通知

### quality
- 品質判定オーケストレーション
- 4段階分類（緑/黄/橙/赤）
- バリデータの呼び出し

### validators
- ファイル形式別のバリデータ群
- 構造検証・コンテンツ検証
- プラグイン式（新形式追加が容易）

### report
- PDF/Excel 出力
- 達成度マトリクスのレンダリング
- テンプレートエンジン統合

### db
- SQLite スキーマ管理
- マイグレーション
- リポジトリパターンの実装

### case-manager
- 案件ライフサイクル管理
- 業務ワークフローのオーケストレーション
- 全モジュールを統合した高レベルAPI
