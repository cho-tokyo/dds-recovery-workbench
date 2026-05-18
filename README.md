# DDS Recovery Workbench

DDSのデータ復旧業務を効率化する内製ソフトウェア。

## 主な特徴

- **目標駆動型復旧**: お客様希望リストを中心に据えた業務フロー
- **読み込み専用設計**: ソースデバイスを書き換えない安全設計
- **品質判定統合**: 復旧ファイルの構造・コンテンツを自動評価
- **達成度レポート**: 希望×結果のマトリクスを自動生成

## ステータス

🚧 **Phase 1 開発中** - 2026年5月着手  
完成目標: 2026年10月（MVP）

## クイックスタート（Claude Code開発者向け）

このプロジェクトはClaude Codeでの開発を前提に構成されています。

```bash
# 1. プロジェクトディレクトリに移動
cd dds-recovery-workbench

# 2. Claude Codeを起動
claude

# 3. 最初のプロンプト
> CLAUDE.md を読んだ後、docs/first_chunk.md の指示に従って Chunk 1 を実装してください。
```

Claude Codeが自動的に:
1. `CLAUDE.md` を読み込み（プロジェクト全体の指示）
2. `docs/first_chunk.md` を読み込み（最初の作業指示）
3. `.claude/agents/builder.md` の builder エージェントを起動して実装
4. 完了後 `.claude/agents/tester.md` の tester でテスト実行
5. テスト合格後 `.claude/agents/progress-tracker.md` で進捗更新

## ディレクトリ構成

```
dds-recovery-workbench/
├── CLAUDE.md                   ← プロジェクト指示書（最重要）
├── .claude/
│   └── agents/                 ← 3エージェント定義
│       ├── builder.md
│       ├── tester.md
│       └── progress-tracker.md
├── docs/
│   ├── PRD.md                  ← プロダクト要求仕様
│   ├── architecture.md         ← アーキテクチャ概要
│   ├── progress.md             ← 進捗トラッカー
│   ├── first_chunk.md          ← 最初のチャンク指示
│   └── specs/                  ← FS仕様書置き場
├── crates/                     ← Rustクレート群
│   ├── core/                   ← 共通型・エラー
│   ├── disk-io/                ← Raw disk access
│   ├── fs-common/              ← FS共通トレイト
│   ├── fs-ntfs/                ← NTFSリーダ
│   ├── fs-exfat/               ← exFATリーダ
│   ├── fs-fat32/               ← FAT32リーダ
│   ├── diagnostic/             ← 診断エンジン
│   ├── wish-match/             ← 希望リスト突合
│   ├── recovery/               ← 復旧パイプライン
│   ├── quality/                ← 品質判定
│   ├── validators/             ← ファイル形式別バリデータ
│   ├── report/                 ← レポート生成
│   ├── db/                     ← SQLite操作
│   └── case-manager/           ← 案件管理
├── fixtures/                   ← テスト用ディスクイメージ
│   ├── scripts/                ← 生成スクリプト
│   └── images/                 ← 生成済みイメージ
├── tests/                      ← E2E結合テスト
└── Cargo.toml                  ← ワークスペース設定
```

## 開発要件

- Rust 1.75+
- cargo
- Linux環境（フィクスチャ生成時、Windows/macOSはWSL or VM）
- Claude Code

## ライセンス

Proprietary - DDS社内利用専用
