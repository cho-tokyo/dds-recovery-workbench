# データ復旧ソフトウェア Phase 1 MVP - Product Requirements Document

**プロジェクト名（仮）**: DDS Recovery Workbench  
**バージョン**: 0.1 (Draft)  
**作成日**: 2026-05-16  
**ステータス**: Phase 1 設計確定前ドラフト  

---

## 1. エグゼクティブサマリー

DDSのデータ復旧業務（特に削除・初期化案件）を効率化する内製ソフトウェアの開発計画。市販復旧ソフトと一線を画す**「お客様希望リスト駆動型」**のワークフローを実装し、CS業務工数を1案件あたり30分以上削減することを目指す。

Phase 1 では Windows 環境で NTFS / exFAT / FAT32 の**削除ファイル復旧**に対応し、案件管理・希望リスト突合・品質判定・達成度レポート生成までの一気通貫の業務基盤を構築する。

---

## 2. 背景と課題

### 2.1 現状の課題

DDSの現行データ復旧業務には以下の課題がある:

1. **市販ツール依存のコスト**: 商用復旧ソフト（UFS Explorer / R-Studio等）のライセンス費が継続的に発生し、複数案件並行時の同時起動制限がボトルネック
2. **業務フロー外の機能過剰**: 市販ツールは「とにかく全部拾う」設計で、DDSの「お客様希望に基づく目標達成」ワークフローと整合しない
3. **CS工数の肥大**: 復旧結果とお客様希望の突合・説明資料作成を手動で実施しており、1案件あたり30〜120分のCS工数が発生
4. **品質判定が属人化**: 復旧したファイルが実際に開けるかの確認が手作業で、CS担当者のスキル差が結果に出る
5. **事前期待値調整の困難**: お客様が何を望み、そのうち何が実際に復旧可能かを事前に定量化できず、納品後のクレーム要因になる

### 2.2 期待される効果

- CS業務工数を1案件あたり**30分〜2時間短縮**
- 復旧結果のお客様向け説明が**定量化・標準化**
- 事前突合により**復旧不能項目に起因するクレーム激減**
- 市販ソフトライセンス費の段階的削減
- フォレンジック事業・SOC事業との内部ツール統合余地

---

## 3. プロダクトビジョン

> **「お客様の復旧希望を中心に据えた、目標駆動型のデータ復旧ワークベンチ」**

汎用復旧ソフトが「データを最大限拾い上げるツール」であるのに対し、DDS Recovery Workbench は **「お客様が望むデータを最大限・最高品質で復旧し、その達成度を定量報告するシステム」** である。

### 設計哲学

1. **読み込み専用**: ソースデバイスへの書き込みは一切行わない。修復は仮想的に実施
2. **目標駆動**: お客様希望リストを中心オブジェクトとし、全モジュールはその達成度で評価される
3. **段階的フォールバック**: 軽量処理（ライブモード）を先行し、必要時のみ重い処理に進む
4. **定量レポート**: 全結果は希望リストとの達成度マトリクスとして出力される
5. **業務統合**: 単独の復旧ツールではなく、案件管理から納品レポートまでの業務基盤

---

## 4. ターゲットユーザー

### 4.1 プライマリユーザー: CS復旧アドバイザー

- データ復旧の依頼受付・お客様対応・進捗報告を担当
- 技術的詳細よりUI/UXの分かりやすさを重視
- 1日に複数案件を並行処理
- Windows Explorer レベルのITリテラシー想定

**ユーザーニーズ**:
- 案件を素早く立ち上げ、お客様希望を入力したい
- 復旧可能性を即座にお客様に伝えたい
- 復旧結果を分かりやすい形でお客様に納品したい

### 4.2 セカンダリユーザー: 復旧エンジニア

- 重度損傷案件のエスカレーション対応
- ツールの限界を超える案件で手動介入
- 技術的詳細・ログを必要とする

**ユーザーニーズ**:
- 損傷状態の詳細を確認したい
- スキャン戦略を手動調整したい
- 復旧ファイルの内部構造を検証したい

### 4.3 ターシャリユーザー: 管理者・経営層

- 案件処理状況・達成率の俯瞰
- KPIモニタリング

**ユーザーニーズ**:
- 案件別・期間別の達成率レポート
- スループット・ボトルネック分析

---

## 5. ユーザーストーリー（Phase 1 主要シナリオ）

### US-01: 標準的な削除ファイル復旧

> CS復旧アドバイザーとして、お客様が誤って削除した10個のファイル名を入力し、5分以内に発見可能性レポートを生成し、お客様に納期と料金見積もりを提示したい。

### US-02: パターン指定での復旧

> CS復旧アドバイザーとして、お客様が「2025年10月に作成した売上関連の.xlsxファイル全部」と希望した場合、パターン入力でマッチング候補を一覧化したい。

### US-03: 復旧前の期待値調整

> CS復旧アドバイザーとして、お客様希望のうち何件が確実に復旧可能か、何件が困難かを事前に定量化し、お客様承認を得てから実際の復旧作業に進みたい。

### US-04: 品質付き納品レポート

> CS復旧アドバイザーとして、復旧完了後、各ファイルの品質ステータス（健全/部分破損/破損）を含むPDFレポートをお客様に納品したい。

### US-05: 達成度の俯瞰

> 復旧エンジニアとして、案件全体の達成率（希望項目のうち何%が緑判定か）を一目で把握したい。

### US-06: 案件履歴の参照

> CS復旧アドバイザーとして、過去案件の検索・参照を通じて、類似ケースの処理時間や達成率を見積もりに活用したい。

---

## 6. Phase 1 スコープ

### 6.1 対象範囲（Phase 1で実装する機能）

**対応ファイルシステム**:
- NTFS（読み取り、ライブモード）
- exFAT（読み取り、ライブモード）
- FAT32（読み取り、ライブモード）

**対応復旧シナリオ**:
- L1: 削除（FSメタデータ健全）
- L2: パーティションテーブル破損（メタデータ生存時の仮想再構築）
- L3: FSメタデータ部分破損（メタデータ既知位置からの仮想復元）

**対象OS**:
- Windows 10/11 (x64)

**主要機能**:
1. 案件管理（作成・参照・履歴）
2. ディスク・パーティション認識
3. 診断モジュール（損傷分類）
4. ライブモードFSリーダ（削除エントリ可視化）
5. お客様希望リスト管理・入力UI
6. 希望リスト × FSエントリ突合エンジン
7. 復旧実行（目標優先抽出）
8. 品質判定エンジン（10種以上のファイル形式に対応）
9. 達成度マトリクス生成
10. 納品レポート出力（PDF + Excel）

**対応ファイル形式（品質判定）** - 最低10種:
- Office: DOCX, XLSX, PPTX
- PDF
- 画像: JPEG, PNG, GIF, TIFF
- メディア: MP4, MOV
- アーカイブ: ZIP

### 6.2 Phase 1 対象外（Phase 2以降）

**対応FS拡張**: ext4, APFS, HFS+, Btrfs, XFS, ReFS, F2FS, UDF, ZFS
**スキャン機能**: 全領域カービング、クイックフォーマット復旧（旧MFT探索）、ハイブリッド復旧
**RAID対応**: RAID自動検出、SHR、mdadm、LVM、ハードRAID
**物理障害**: ddrescue型イメージング、不良セクタ高度処理
**MacOS / Linux版**: クロスプラットフォーム対応
**マルチユーザー**: 複数CSからの同時案件共有、サーバ集中管理
**ML機能**: 機械学習による品質判定、曖昧マッチング
**フォレンジック統合**: 証拠保全モード、ハッシュ計算、タイムライン分析
**ランサムウェア対応**: 暗号化検出、復号支援

---

## 7. 機能要件

### 7.1 案件管理モジュール

**FR-CASE-01** 案件の新規作成: 案件ID（自動採番）、お客様名、デバイス情報（自動取得 + 手動補完）、担当CS、ステータスを登録

**FR-CASE-02** 案件一覧表示: ステータス・日付・担当者でフィルタリング可能なリスト

**FR-CASE-03** 案件詳細表示: 各案件の全モジュールの結果を1画面で参照

**FR-CASE-04** 案件履歴の永続化: 全案件のメタデータ・希望リスト・結果サマリをSQLiteで保存

**FR-CASE-05** 案件のエクスポート: 個別案件を独立フォルダ（DB + ファイル + レポート）として書き出し可能

### 7.2 診断モジュール

**FR-DIAG-01** デバイス検出: 接続中の物理ディスク・パーティション一覧を表示

**FR-DIAG-02** デバイス情報取得: 容量、モデル、シリアル、SMART情報（取得可能な範囲）

**FR-DIAG-03** PT解析: MBR/GPTを読み込み、パーティション構造を判定

**FR-DIAG-04** FS識別: 各パーティションのFSタイプを自動判定（NTFS/exFAT/FAT32/その他）

**FR-DIAG-05** 損傷分類: L1/L2/L3 のいずれかに分類（L4-L6 はPhase 1では「スキャン要 - 未対応」と表示）

**FR-DIAG-06** 戦略提案: 損傷分類に基づき推奨処理方法と推定時間を表示

**FR-DIAG-07** 診断レポート生成: 案件に紐付けて診断結果を保存

### 7.3 ライブモードFSリーダ

**FR-LIVE-01** NTFS読み取り: $MFT を解析し、全エントリ（生存+削除）を列挙

**FR-LIVE-02** exFAT読み取り: ディレクトリエントリを走査し、削除済みエントリ（InUseビット=0）も含めて列挙

**FR-LIVE-03** FAT32読み取り: ディレクトリエントリ走査、削除済みエントリ（先頭バイト0xE5）も含めて列挙

**FR-LIVE-04** ファイルツリー構築: 全エントリを階層ツリーとしてメモリ + SQLiteに展開

**FR-LIVE-05** 削除エントリ可視化: ツリーUI上で削除済みエントリを色分け表示

**FR-LIVE-06** メタデータ表示: 各エントリのサイズ、作成日時、変更日時、ファイルタイプを表示

**FR-LIVE-07** バックアップメタ活用: NTFS `$MFTMirr`、exFAT/FAT32 のFATコピー等から補完

### 7.4 希望リスト管理・突合エンジン

**FR-WISH-01** 希望項目の入力フォーム: 以下の入力タイプをサポート
- ファイル名指定（完全一致）
- パターン指定（ワイルドカード `*`, `?`）
- パス指定（ディレクトリ単位）
- 拡張子指定（複数選択可）
- 日付範囲指定（作成日 / 変更日）
- サイズ範囲指定

**FR-WISH-02** 各項目の優先度設定: must / should / nice の3段階

**FR-WISH-03** 一括インポート: CSV/Excel から希望リストを取込

**FR-WISH-04** 突合実行: 各希望項目に対し、FSエントリツリーから候補を抽出

**FR-WISH-05** マッチ信頼度算出: 完全一致 / パターン一致 / 部分一致 を区別してスコアリング

**FR-WISH-06** 発見可能性レポート: 「希望N件中M件発見可能」を視覚化（円グラフ + 項目別表）

**FR-WISH-07** 未発見項目の理由提示: 候補ゼロの理由（メタ上書き / FS範囲外 等）を可能な範囲で説明

**FR-WISH-08** お客様承認フロー: 発見可能性レポートをPDF出力し、お客様承認後に復旧実行に進むワークフロー

### 7.5 復旧実行エンジン

**FR-REC-01** 目標優先抽出: 希望リストとマッチした候補から順に抽出（無関係ファイルは抽出しない）

**FR-REC-02** ノンマッチ抽出オプション: お客様承認時のみ、希望外の削除済みファイルも抽出可能（オプション）

**FR-REC-03** 出力先指定: 復旧データの書き出し先を明示的に指定（ソースと別ストレージを強制）

**FR-REC-04** データ整合性: 抽出時にCRC/ハッシュを計算し、抽出済みファイルの破損を検出

**FR-REC-05** 進捗表示: ファイル単位の進捗、残り時間予測、エラーログをリアルタイム表示

**FR-REC-06** リトライ機構: 一時的なI/Oエラーは指定回数リトライ、永続エラーはスキップしてログ記録

**FR-REC-07** 抽出方法の記録: 各ファイルがL1〜L3のどの方法で抽出されたかをDB記録

### 7.6 品質判定エンジン

**FR-QA-01** ファイル形式検証: マジックバイト判定、ヘッダ/フッタ整合性チェック

**FR-QA-02** 構造的整合性: 各ファイル形式の内部構造を検証
- PDF: xrefテーブル整合性
- Office (DOCX/XLSX/PPTX): ZIP内構造 + XMLパース
- JPEG: SOI/EOI存在、DHT/DQT/SOF整合性
- PNG: IHDR/IDAT/IEND整合性
- MP4: atomツリー整合性
- ZIP: ローカルヘッダ + セントラルディレクトリ整合性

**FR-QA-03** コンテンツレベル検証:
- 画像: デコード成功率
- PDF: ページ数取得・テキスト抽出成否
- Office: ドキュメント開封可能性
- 動画: 主要フレームデコード成功率（ffmpeg連携）

**FR-QA-04** 4段階分類:
- 緑（健全）: 構造OK + コンテンツOK
- 黄（軽微破損）: 構造OK + コンテンツ部分異常
- 橙（重大破損）: 構造異常
- 赤（破損）: マジックバイト不一致 / 全くデコード不可

**FR-QA-05** 判定結果のDB記録: 全復旧ファイルに品質スコアを紐付け

**FR-QA-06** プラグイン式バリデータ: 新規ファイル形式追加が容易な設計

### 7.7 達成度評価モジュール

**FR-ACH-01** 希望×結果マトリクス生成: 各希望項目に対し、発見状況・抽出方法・品質を一覧化

**FR-ACH-02** 達成率算出:
- 緑+黄を達成と見なす「総合達成率」
- 緑のみを達成と見なす「完全達成率」
- must項目のみの達成率（重要度別評価）

**FR-ACH-03** カテゴリ別集計: ファイル種別・サイズ別・日付別の達成状況

**FR-ACH-04** 視覚化: 円グラフ・棒グラフでの達成度表示（UI上）

### 7.8 レポート生成

**FR-REP-01** 内部用詳細レポート（Excel）: 全希望項目 × 全マッチ × 全復旧結果 × 全品質判定を含む詳細データ

**FR-REP-02** お客様向けサマリレポート（PDF）: 達成率サマリ、項目別達成状況、品質凡例、復旧不能項目の理由

**FR-REP-03** 復旧ファイル一覧（CSV）: 復旧データに同梱する索引ファイル

**FR-REP-04** カスタムテンプレート: DDSブランディング（ロゴ・配色）を反映したテンプレート

**FR-REP-05** 多言語対応の基盤: Phase 1は日本語のみ、将来英語追加可能な設計

---

## 8. 非機能要件

### 8.1 性能要件

| 指標 | 目標値 | 備考 |
|---|---|---|
| 1TB HDD ライブモード読み込み（L1） | 5分以内 | 7200rpm SATA直結時 |
| 1TB HDD 診断完了 | 5分以内 | サンプル読み込み中心 |
| 希望リスト突合（10万エントリ × 100希望項目） | 10秒以内 | SQLiteインデックス活用 |
| 品質判定（1ファイル） | 数百ms〜数秒 | ファイルサイズに依存 |
| UI応答性 | 操作後100ms以内に反応 | スキャン中もUIフリーズ禁止 |
| メモリ使用量（典型ケース） | 8GB以内 | 1TB HDDで100万エントリ規模 |
| メモリ使用量（最大） | 32GB以内 | 大規模案件・複数案件並行 |

### 8.2 信頼性・安全性

**NFR-REL-01** ソースデバイス書込禁止: 全I/Oは `O_RDONLY` 相当。書込APIへの誤呼出しを防ぐ型レベル制約

**NFR-REL-02** 異常終了時の状態保護: クラッシュ時もDBが破損しない（WALモード + チェックポイント）

**NFR-REL-03** 部分結果の保存: 復旧途中で中断した場合も、それまでの結果は保存される

**NFR-REL-04** 再開可能性: 中断した案件を後から再開可能

**NFR-REL-05** I/Oエラー処理: 不良セクタは複数回リトライ後にスキップ、ログ記録

### 8.3 セキュリティ

**NFR-SEC-01** 管理者権限: Raw disk access には管理者権限が必要（UACマニフェスト）

**NFR-SEC-02** 出力データ暗号化: 復旧データの出力先を暗号化（オプション、AES-256）

**NFR-SEC-03** PII保護: ログ送信時にお客様名・ファイル名等のPIIをマスキング

**NFR-SEC-04** コード署名: 配布時はEV/OVコード署名証明書で署名

**NFR-SEC-05** 改竄検出: 案件レポートにはハッシュチェックサム埋め込み

### 8.4 監査・ロギング

**NFR-LOG-01** 全操作の監査ログ: 案件作成・希望入力・スキャン実行・復旧実行・レポート生成を記録

**NFR-LOG-02** タイムスタンプ: 全ログにISO 8601形式のUTC時刻

**NFR-LOG-03** ログ保持期間: 案件単位で最低7年保持（フォレンジック要件）

**NFR-LOG-04** 改竄不能化: ログファイルへの追記のみ許可、過去ログの変更不可

**NFR-LOG-05** 構造化ログ: `tracing` クレートによる構造化JSON出力

### 8.5 ユーザビリティ

**NFR-UX-01** 学習コスト: Windows Explorerが使えるCSが2時間以内に主要機能を習得

**NFR-UX-02** 操作の最小化: 標準ケース（削除復旧）は5ステップ以内で完了

**NFR-UX-03** エラーメッセージ: 専門用語を避け、CS視点で次に何をすべきか明示

**NFR-UX-04** プログレッシブディスクロージャ: 詳細情報はデフォルト非表示、エンジニア向けに展開可能

**NFR-UX-05** ダークモード対応: 長時間作業の負荷軽減

### 8.6 保守性

**NFR-MAINT-01** モジュール分離: 各FS、各バリデータが独立クレートとして交換可能

**NFR-MAINT-02** テストカバレッジ: コアモジュール80%以上、UI除く

**NFR-MAINT-03** ドキュメント: 主要モジュールに `cargo doc` 互換のドキュメントコメント

**NFR-MAINT-04** バージョニング: セマンティックバージョニング遵守

---

## 9. アーキテクチャ概要

### 9.1 全体構成

```
┌─────────────────────────────────────────────┐
│ Tauri UI Layer                              │
│ (React + TypeScript + Tailwind)             │
│   - 案件管理画面                              │
│   - 診断画面                                  │
│   - 希望リスト入力画面                         │
│   - ファイルツリー画面                         │
│   - 品質レビュー画面                           │
│   - レポート画面                              │
└────────────┬────────────────────────────────┘
             │ Tauri Commands (JSON-RPC)
┌────────────▼────────────────────────────────┐
│ Application Layer (Rust)                    │
│   - Case Manager                            │
│   - Workflow Orchestrator                   │
│   - Report Generator                        │
└────────────┬────────────────────────────────┘
             │
┌────────────▼────────────────────────────────┐
│ Core Engines (Rust)                         │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│ │Diagnostic│ │ WishMatch│ │ Quality  │      │
│ │ Engine   │ │ Engine   │ │ Engine   │      │
│ └──────────┘ └──────────┘ └──────────┘      │
│ ┌──────────────────────────────────────┐    │
│ │ FS Readers (NTFS / exFAT / FAT32)    │    │
│ └──────────────────────────────────────┘    │
│ ┌──────────────────────────────────────┐    │
│ │ Recovery Pipeline (Targeted)         │    │
│ └──────────────────────────────────────┘    │
└────────────┬────────────────────────────────┘
             │
┌────────────▼────────────────────────────────┐
│ Infrastructure Layer                        │
│ ┌──────────────┐ ┌──────────────────────┐   │
│ │ Raw Disk I/O │ │ SQLite DB Layer      │   │
│ │ (windows-rs) │ │ (rusqlite)           │   │
│ └──────────────┘ └──────────────────────┘   │
└─────────────────────────────────────────────┘
```

### 9.2 Cargoワークスペース構成

```
dds-recovery-workbench/
├── crates/
│   ├── core/              ← ライブラリ群の共通型
│   ├── disk-io/           ← Raw disk access抽象化
│   ├── fs-ntfs/           ← NTFS reader
│   ├── fs-exfat/          ← exFAT reader
│   ├── fs-fat32/          ← FAT32 reader
│   ├── fs-common/         ← FS共通インタフェース
│   ├── diagnostic/        ← 診断エンジン
│   ├── wish-match/        ← 希望リスト突合
│   ├── recovery/          ← 復旧パイプライン
│   ├── quality/           ← 品質判定エンジン
│   ├── validators/        ← ファイル形式別バリデータ
│   ├── report/            ← レポート生成
│   ├── db/                ← SQLite操作
│   └── case-manager/      ← 案件管理
├── src-tauri/             ← Tauri統合層
├── ui/                    ← React + TypeScript
├── tests/                 ← 統合テスト
├── fixtures/              ← テスト用ディスクイメージ
├── docs/
│   ├── specs/             ← FS仕様書（PDF等）
│   ├── design/            ← 設計文書
│   └── runbooks/          ← 運用手順
├── CLAUDE.md              ← Claude Code指示書
└── README.md
```

---

## 10. データモデル

### 10.1 主要エンティティ

```sql
-- 案件
CREATE TABLE cases (
  case_id           TEXT PRIMARY KEY,
  customer_name     TEXT NOT NULL,
  customer_contact  TEXT,
  drive_serial      TEXT,
  drive_model       TEXT,
  drive_capacity    INTEGER,
  case_status       TEXT NOT NULL, -- created/diagnosed/matched/recovering/qa/done
  assigned_cs       TEXT,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  completed_at      INTEGER
);

-- 診断結果
CREATE TABLE diagnostics (
  case_id              TEXT PRIMARY KEY,
  damage_level         TEXT NOT NULL,  -- L1/L2/L3/L4+ (L4+はPhase 1未対応)
  has_physical_issues  INTEGER NOT NULL,
  partition_count      INTEGER,
  detected_fs_types    TEXT,  -- JSON配列
  damage_map           TEXT,  -- JSON
  strategy             TEXT,
  estimated_minutes    INTEGER,
  diagnosed_at         INTEGER NOT NULL,
  FOREIGN KEY (case_id) REFERENCES cases(case_id)
);

-- お客様希望項目
CREATE TABLE wished_items (
  item_id       TEXT PRIMARY KEY,
  case_id       TEXT NOT NULL,
  match_type    TEXT NOT NULL,  -- exact/pattern/path/extension/daterange/size
  match_value   TEXT NOT NULL,
  priority      TEXT NOT NULL,  -- must/should/nice
  description   TEXT,
  created_at    INTEGER NOT NULL,
  FOREIGN KEY (case_id) REFERENCES cases(case_id)
);

-- FSエントリ（スキャン/ライブモードで発見した全ファイル）
CREATE TABLE fs_entries (
  entry_id        INTEGER PRIMARY KEY AUTOINCREMENT,
  case_id         TEXT NOT NULL,
  partition_id    INTEGER NOT NULL,
  fs_type         TEXT NOT NULL,
  fs_record_id    INTEGER,  -- MFT entry number等
  file_name       TEXT,
  full_path       TEXT,
  size_bytes      INTEGER,
  created_at      INTEGER,
  modified_at     INTEGER,
  accessed_at     INTEGER,
  is_deleted      INTEGER NOT NULL,
  is_directory    INTEGER NOT NULL,
  data_runs       TEXT,  -- JSON: クラスタ配置情報
  FOREIGN KEY (case_id) REFERENCES cases(case_id)
);
CREATE INDEX idx_fs_entries_case        ON fs_entries(case_id);
CREATE INDEX idx_fs_entries_name        ON fs_entries(case_id, file_name);
CREATE INDEX idx_fs_entries_path        ON fs_entries(case_id, full_path);
CREATE INDEX idx_fs_entries_modified    ON fs_entries(case_id, modified_at);
CREATE INDEX idx_fs_entries_extension   ON fs_entries(case_id, file_name COLLATE NOCASE);

-- 希望項目 × FSエントリのマッチング
CREATE TABLE wish_matches (
  match_id          INTEGER PRIMARY KEY AUTOINCREMENT,
  item_id           TEXT NOT NULL,
  entry_id          INTEGER NOT NULL,
  match_confidence  REAL NOT NULL,  -- 0.0-1.0
  match_method      TEXT NOT NULL,
  FOREIGN KEY (item_id)  REFERENCES wished_items(item_id),
  FOREIGN KEY (entry_id) REFERENCES fs_entries(entry_id)
);

-- 復旧結果
CREATE TABLE recovered_files (
  recovered_id      TEXT PRIMARY KEY,
  case_id           TEXT NOT NULL,
  entry_id          INTEGER,
  item_id           TEXT,
  output_path       TEXT NOT NULL,
  recovery_method   TEXT NOT NULL,  -- L1/L2/L3
  size_bytes        INTEGER,
  sha256            TEXT,
  recovered_at      INTEGER NOT NULL,
  FOREIGN KEY (case_id)  REFERENCES cases(case_id),
  FOREIGN KEY (entry_id) REFERENCES fs_entries(entry_id)
);

-- 品質判定結果
CREATE TABLE quality_results (
  recovered_id     TEXT PRIMARY KEY,
  structure_valid  INTEGER NOT NULL,
  content_score    REAL,
  rating           TEXT NOT NULL,  -- green/yellow/orange/red
  validator_used   TEXT,
  validation_log   TEXT,  -- JSON
  validated_at     INTEGER NOT NULL,
  FOREIGN KEY (recovered_id) REFERENCES recovered_files(recovered_id)
);

-- 案件達成度サマリ
CREATE TABLE case_outcomes (
  case_id              TEXT PRIMARY KEY,
  total_wished         INTEGER NOT NULL,
  found_healthy        INTEGER NOT NULL,
  found_damaged        INTEGER NOT NULL,
  found_corrupt        INTEGER NOT NULL,
  not_found            INTEGER NOT NULL,
  achievement_rate     REAL,
  complete_rate        REAL,  -- 緑のみ
  must_achievement     REAL,  -- must項目達成率
  report_pdf_path      TEXT,
  report_excel_path    TEXT,
  finalized_at         INTEGER,
  FOREIGN KEY (case_id) REFERENCES cases(case_id)
);

-- 監査ログ
CREATE TABLE audit_logs (
  log_id      INTEGER PRIMARY KEY AUTOINCREMENT,
  case_id     TEXT,
  user_id     TEXT,
  action      TEXT NOT NULL,
  details     TEXT,  -- JSON
  occurred_at INTEGER NOT NULL
);
CREATE INDEX idx_audit_case ON audit_logs(case_id, occurred_at);
```

---

## 11. 技術スタック

| レイヤ | 技術 |
|---|---|
| 言語（コア） | Rust 1.75+ |
| 言語（UI） | TypeScript 5.x |
| UIフレームワーク | Tauri 2.x |
| フロントエンドフレームワーク | React 18 |
| CSS | Tailwind CSS 3.x |
| DB | SQLite 3.40+ (WALモード) |
| DBアクセス | rusqlite + r2d2 |
| バイナリパース | binrw |
| 非同期ランタイム | tokio |
| 並列処理 | rayon |
| パターンマッチ | aho-corasick |
| ロギング | tracing + tracing-subscriber |
| 設定 | serde + TOML |
| PDF生成 | typst |
| Excel生成 | rust_xlsxwriter |
| エラー型 | thiserror, anyhow |
| テスト | cargo test, proptest, insta |
| ファジング | cargo-fuzz |

---

## 12. UI/UX要件

### 12.1 主要画面

1. **ダッシュボード**: 進行中案件一覧、未着手案件、本日完了案件
2. **新規案件作成**: ウィザード形式（お客様情報 → デバイス選択 → 診断開始）
3. **診断結果**: 損傷状態の視覚化 + 戦略提案
4. **希望リスト入力**: フォーム + CSV取込 + プレビュー
5. **発見可能性レポート**: 円グラフ + 項目別表 + PDF出力ボタン
6. **ファイルツリー**: 階層表示 + 削除エントリ色分け + 検索フィルタ
7. **復旧実行**: 進捗バー + リアルタイムログ + 中断/再開ボタン
8. **品質レビュー**: 復旧ファイル一覧 + 品質ステータス + 個別プレビュー
9. **達成度マトリクス**: 希望×結果のクロス表 + サマリ統計
10. **レポート出力**: テンプレート選択 + プレビュー + 出力

### 12.2 デザイン原則

- **色覚多様性配慮**: 4段階品質判定は色 + アイコン + ラベルの三重表現
- **タッチターゲット**: ボタンは最小44×44px
- **キーボードナビゲーション**: 全機能をマウスなしで操作可能
- **ヘルプテキスト**: 各入力フィールドに用途説明をツールチップで提供

---

## 13. 成功指標（KPI）

### 13.1 機能KPI

| 指標 | Phase 1目標 |
|---|---|
| 対応FS数 | 3（NTFS / exFAT / FAT32） |
| 対応復旧シナリオ | L1, L2, L3 |
| ファイル形式バリデータ数 | 10種以上 |
| 1TB HDD ライブモード処理時間 | 5分以内 |
| 希望リスト突合精度（正解率） | 90%以上 |

### 13.2 業務KPI

| 指標 | 現状（推定） | Phase 1目標 |
|---|---|---|
| 1案件あたりCS工数 | 90分 | 60分（▲33%） |
| 結果説明資料作成時間 | 30分 | 5分（▲83%） |
| お客様事前期待値調整実施率 | 不明 | 80%以上 |
| 復旧不能項目関連クレーム件数 | 不明 | 月次計測開始 |

### 13.3 技術KPI

| 指標 | 目標 |
|---|---|
| コアモジュールテストカバレッジ | 80%以上 |
| 重大バグ（データ破損系）リリース後発生数 | 0件 |
| 平均クラッシュ間隔 | 100案件以上 |

---

## 14. リスクと対策

### R-01: NTFS仕様の不完全性
**リスク**: Microsoftが完全仕様を公開していないため、未対応のNTFS機能で復旧失敗の可能性  
**影響**: 高  
**確率**: 中  
**対策**: Brian Carrier著書 + ntfs-3g/TSK ソース読解 + 検証用テストイメージで網羅的テスト

### R-02: 大規模ディスクでのメモリ不足
**リスク**: 10TB級ディスクで100万エントリ超になり、メモリ枯渇の可能性  
**影響**: 中  
**確率**: 中  
**対策**: ファイルツリーをSQLite永続化 + LRUキャッシュで対応。Phase 1は4TB上限を実用範囲と明示

### R-03: 開発スケジュール遅延
**リスク**: Rust未経験開発者、Claude Code単独運用のリスク  
**影響**: 高  
**確率**: 中  
**対策**: Phase 1スコープを段階的にリリース（α: NTFS削除のみ → β: 全FS → MVP: 品質判定統合）

### R-04: お客様データ破損事故
**リスク**: 書込ロジックのバグでソースHDDが破損する致命的事故  
**影響**: 致命的  
**確率**: 低  
**対策**: 
- 全I/Oを型レベルで読込専用に制約（writeメソッドを実装しない）
- ハードウェアライトブロッカ併用を社内標準化
- リリース前のフォーマット系テスト100ケース合格を必須化

### R-05: ライセンス汚染
**リスク**: GPL OSSコードの誤利用で商用化阻害  
**影響**: 中  
**確率**: 低  
**対策**: 法務レビュー + 全外部依存のSPDX識別子チェックをCIに組込

### R-06: パフォーマンス目標未達
**リスク**: ライブモード5分以内目標が達成できない  
**影響**: 中  
**確率**: 低  
**対策**: 早期にプロファイリング + ベンチマーク自動化。Critical pathのSIMD/並列化準備

---

## 15. 開発計画

### 15.1 マイルストーン

| マイルストーン | 期間目安 | 成果物 |
|---|---|---|
| **M0: 設計確定** | Week 0 | PRD承認、CLAUDE.md確定、ワークスペース初期化 |
| **M1: 基盤構築** | Week 1-3 | Cargo workspace、CI/CD、SQLite基盤、Tauri骨格 |
| **M2: NTFSリーダα** | Week 4-7 | NTFS $MFT解析、削除エントリ列挙、テストイメージ検証 |
| **M3: 希望突合エンジン** | Week 8-9 | 希望リスト入力、突合ロジック、発見可能性レポート |
| **M4: 復旧 + 品質判定** | Week 10-12 | 目標抽出、5種類のバリデータ、品質スコアリング |
| **M5: NTFS-α リリース** | Week 13 | 内部テスト用α版、NTFS削除案件のみ対応 |
| **M6: exFAT/FAT32追加** | Week 14-16 | 残り2FSの実装 |
| **M7: バリデータ拡充** | Week 17-18 | 10種以上のファイル形式対応 |
| **M8: レポート完成** | Week 19-20 | PDF/Excelレポートテンプレート完成 |
| **M9: ベータリリース** | Week 21 | 限定CS数名でのドッグフード開始 |
| **M10: 改善 + MVP** | Week 22-26 | フィードバック反映、本番リリース |

**Phase 1 総期間: 約26週（6ヶ月）**

### 15.2 体制案

| 役割 | リソース |
|---|---|
| プロダクトオーナー | Chou（DDS）= 要件定義、優先度判断、業務知識提供 |
| 開発主担当 | Claude Code + Chou（実装、レビュー、テスト） |
| FSスペシャリスト | 外部アドバイザー（NTFS設計レビュー、月数時間）※推奨 |
| QAリード | DDS内部CSメンバー（β段階から） |

---

## 16. リリース計画

### 16.1 リリースフェーズ

| フェーズ | 対象 | 範囲 |
|---|---|---|
| **α (内部)** | 開発担当者のみ | NTFS削除のみ、UIは簡素 |
| **β (社内ドッグフード)** | CS 2-3名 | NTFS/exFAT/FAT32、簡易品質判定 |
| **MVP (本番)** | 全CS | 全機能、安定運用 |

### 16.2 ロールアウト計画

- α→β: 5案件以上の検証成功後
- β→MVP: 20案件処理 + 重大バグなし状態が2週間継続

---

## 17. Phase 2以降の展望（参考）

### Phase 2: スキャンモード追加 (+3-4ヶ月)
- 全領域カービングエンジン
- クイックフォーマット復旧（旧MFT探索）
- ハイブリッド復旧（メタ + カービング）
- 5+ファイル形式のシグネチャ追加

### Phase 3: 多FS対応 (+3-4ヶ月)
- ext4, APFS, HFS+
- macOS版リリース

### Phase 4: NAS/RAID対応 (+6ヶ月)
- Synology SHR
- mdadm/LVM
- RAID 0/1/5 解析検出
- Btrfs

### Phase 5: 高度機能 (+α)
- 機械学習による品質判定
- フォレンジック証拠保全モード
- マルチユーザー・サーバ集中管理
- ランサムウェア対応統合

---

## 18. 付録

### 18.1 参考資料

- Brian Carrier『File System Forensic Analysis』
- Microsoft exFAT File System Specification (2019)
- Microsoft FAT32 File System Specification
- Linux kernel.org ext4 documentation
- Apple File System Reference
- SNIA Common RAID Disk Data Format
- The Sleuth Kit Documentation

### 18.2 用語集

| 用語 | 意味 |
|---|---|
| ライブモード | FSメタデータをそのまま読み、削除エントリ含めて列挙する処理 |
| スキャンモード | カービング等の全領域走査（Phase 2以降） |
| L1〜L6 | 損傷レベル分類。L1=削除のみ、L6=重度破損 |
| 希望リスト | お客様が復旧を希望するファイルの指定リスト |
| 突合 | 希望項目とFSエントリのマッチング処理 |
| 達成度マトリクス | 希望×結果のクロス集計表 |
| L1抽出 / L2抽出 / L3抽出 | 各損傷レベルに対応した復旧手法 |
| 4段階品質 | 緑(健全)/黄(軽微)/橙(重大)/赤(破損) |
| MFT | Master File Table（NTFSのメタデータ領域） |
| ライトブロッカ | ソースデバイスへの書込を物理的に防ぐハードウェア |

---

**ドキュメント変更履歴**

| 版 | 日付 | 変更内容 | 担当 |
|---|---|---|---|
| 0.1 | 2026-05-16 | 初版作成 | - |
