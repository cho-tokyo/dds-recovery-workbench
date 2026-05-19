# DDS Recovery Workbench - 進捗トラッカー

このファイルは progress-tracker エージェントが自動更新します。

---

## 累積サマリ

- **完了チャンク数**: 1
- **総実装行数**: 197（実装+テスト合計、200行上限内）
- **総テスト数**: 5（全パス）
- **平均カバレッジ**: 未計測（モジュール完成時に計測予定）
- **最終更新日**: 2026-05-19

---

## マイルストーン進捗

```
M0: 設計確定        [████████] 100% ✅ 完了
M1: 基盤構築        [█░░░░░░░]  10% 🚧 進行中（Chunk 1/想定10前後 完了）
M2: NTFSリーダα     [░░░░░░░░]   0% ⏳ 未着手
M3: 希望突合エンジン  [░░░░░░░░]   0% ⏳ 未着手
M4: 復旧 + 品質判定  [░░░░░░░░]   0% ⏳ 未着手
M5: NTFS-α リリース [░░░░░░░░]   0% ⏳ 未着手
M6: exFAT/FAT32追加 [░░░░░░░░]   0% ⏳ 未着手
M7: バリデータ拡充   [░░░░░░░░]   0% ⏳ 未着手
M8: レポート完成     [░░░░░░░░]   0% ⏳ 未着手
M9: ベータリリース   [░░░░░░░░]   0% ⏳ 未着手
M10: 改善 + MVP    [░░░░░░░░]   0% ⏳ 未着手
```

---

## チャンク完了履歴

| # | クレート | 名前 | 行数 | テスト | カバレッジ | 完了日 |
|---|---|---|---|---|---|---|
| 1 | dds-core | 共通エラー型・基本enum定義 | 197 | 5 ✓ | 未計測 | 2026-05-19 |

### Chunk 1 詳細

- **対象ファイル**: `crates/core/src/lib.rs`
- **実装内容**:
  - `CoreError` enum（thiserror 派生、6バリアント: `Io` / `Parse{context,reason}` / `InvalidArgument` / `OutOfRange{what,value,max}` / `Unsupported` / `Internal`）
  - `CoreResult<T>` 型エイリアス
  - `DamageLevel` enum（L1_DeletionOnly 〜 L6_SevereDamage + PhysicalIssue、`Display` 実装、`display_ja(&self) -> &'static str`、Serialize/Deserialize 派生）
  - `RecoveryMethod` enum（L1_MetadataIntact / L2_PartitionReconstructed / L3_FsMetadataReconstructed、`Display` + Serialize/Deserialize）
  - `QualityRating` enum（Green / Yellow / Orange / Red、`is_acceptable(&self) -> bool`、Serialize/Deserialize）
- **検証結果（tester 独立検証）**:
  - `cargo check -p dds-core` … OK
  - `cargo test --lib -p dds-core` … **5 passed; 0 failed**
    - `core_error_io_display_contains_inner_message`
    - `core_error_out_of_range_includes_value_and_max`
    - `damage_level_display_ja_all_variants`
    - `quality_rating_is_acceptable_truth_table`
    - `recovery_method_display_outputs_japanese_label`
  - `cargo clippy -p dds-core -- -D warnings` … warning 0件
  - `cargo doc -p dds-core --no-deps` … 生成成功
  - cargo: 1.95.0 (f2d3ce0bd 2026-03-21)
- **関連 FR**: 設計基盤（全 FR の前提）。本チャンク単独では特定の FR-XXX 完了マークは付与しない。後続チャンクで本クレートが利用されることで間接的に貢献。
- **完了判定**: 完全完了（実装/単体テスト3件以上/rustdoc/clippy clean を全て満たす）

---

## FR要件達成マトリクス

### 案件管理 (FR-CASE)
- [ ] FR-CASE-01: 案件の新規作成
- [ ] FR-CASE-02: 案件一覧表示
- [ ] FR-CASE-03: 案件詳細表示
- [ ] FR-CASE-04: 案件履歴の永続化
- [ ] FR-CASE-05: 案件のエクスポート

### 診断 (FR-DIAG)
- [ ] FR-DIAG-01: デバイス検出
- [ ] FR-DIAG-02: デバイス情報取得
- [ ] FR-DIAG-03: PT解析
- [ ] FR-DIAG-04: FS識別
- [ ] FR-DIAG-05: 損傷分類
- [ ] FR-DIAG-06: 戦略提案
- [ ] FR-DIAG-07: 診断レポート生成

### ライブモード (FR-LIVE)
- [ ] FR-LIVE-01: NTFS読み取り
- [ ] FR-LIVE-02: exFAT読み取り
- [ ] FR-LIVE-03: FAT32読み取り
- [ ] FR-LIVE-04: ファイルツリー構築
- [ ] FR-LIVE-05: 削除エントリ可視化
- [ ] FR-LIVE-06: メタデータ表示
- [ ] FR-LIVE-07: バックアップメタ活用

### 希望リスト・突合 (FR-WISH)
- [ ] FR-WISH-01: 希望項目の入力フォーム
- [ ] FR-WISH-02: 優先度設定
- [ ] FR-WISH-03: 一括インポート
- [ ] FR-WISH-04: 突合実行
- [ ] FR-WISH-05: マッチ信頼度算出
- [ ] FR-WISH-06: 発見可能性レポート
- [ ] FR-WISH-07: 未発見項目の理由提示
- [ ] FR-WISH-08: お客様承認フロー

### 復旧 (FR-REC)
- [ ] FR-REC-01: 目標優先抽出
- [ ] FR-REC-02: ノンマッチ抽出オプション
- [ ] FR-REC-03: 出力先指定
- [ ] FR-REC-04: データ整合性
- [ ] FR-REC-05: 進捗表示
- [ ] FR-REC-06: リトライ機構
- [ ] FR-REC-07: 抽出方法の記録

### 品質判定 (FR-QA)
- [ ] FR-QA-01: ファイル形式検証
- [ ] FR-QA-02: 構造的整合性
- [ ] FR-QA-03: コンテンツレベル検証
- [ ] FR-QA-04: 4段階分類
- [ ] FR-QA-05: 判定結果のDB記録
- [ ] FR-QA-06: プラグイン式バリデータ

### 達成度評価 (FR-ACH)
- [ ] FR-ACH-01: 希望×結果マトリクス生成
- [ ] FR-ACH-02: 達成率算出
- [ ] FR-ACH-03: カテゴリ別集計
- [ ] FR-ACH-04: 視覚化

### レポート (FR-REP)
- [ ] FR-REP-01: 内部用詳細レポート（Excel）
- [ ] FR-REP-02: お客様向けサマリレポート（PDF）
- [ ] FR-REP-03: 復旧ファイル一覧（CSV）
- [ ] FR-REP-04: カスタムテンプレート
- [ ] FR-REP-05: 多言語対応の基盤

---

## リスクログ

（進行中に発見されたリスクをここに記録）

- なし

---

## 次の推奨アクション

**Chunk 2**: `dds-fs-common` FS共通トレイト定義

- **対象クレート**: `crates/fs-common/`
- **目的**: 全 FS リーダ（NTFS / exFAT / FAT32）が共通実装すべき `FsReader` 系トレイトおよび FS 共通データ型（FileEntry, Attributes 等）を定義し、後続の `fs-ntfs` 等が依存できる土台を作る
- **依存**: Chunk 1（`dds-core` のエラー型・基本 enum）
- **推定行数**: 約150行（trait 定義中心 + 単体テスト 3件以上）
- **着手前の準備**:
  1. PRD `docs/PRD.md` の FR-LIVE-01〜07 を再確認（読み取り系要件の共通項を抽出）
  2. `docs/architecture.md` の fs-common 責務記述を確認
  3. 後続チャンク 4〜10（NTFS実装群）を見据えた抽象化レベルの調整
- **完了条件**: Chunk 1 と同等（cargo check / test --lib / clippy / doc 全 OK、rustdoc 完備、単体テスト 3件以上）

詳細指示は builder 起動時に作成する `docs/chunk_2.md`（または first_chunk.md と同形式の別ファイル）で展開予定。
