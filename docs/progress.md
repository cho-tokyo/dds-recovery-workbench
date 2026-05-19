# DDS Recovery Workbench - 進捗トラッカー

このファイルは progress-tracker エージェントが自動更新します。

---

## 累積サマリ

- **完了チャンク数**: 4
- **総実装行数**: 775（実装+テスト合計、各チャンク200行上限内）
- **総単体テスト数**: 22（全パス）
- **総結合テスト数**: 2（全パス、NTFSフィクスチャ実画像での Boot Sector パース）
- **平均カバレッジ**: 未計測（モジュール完成時に計測予定）
- **最終更新日**: 2026-05-19

---

## マイルストーン進捗

```
M0: 設計確定        [████████] 100% ✅ 完了
M1: 基盤構築        [███░░░░░]  30% 🚧 進行中（Chunk 1-3/想定10前後 完了）
M2: NTFSリーダα     [█░░░░░░░]  10% 🚧 進行中（Chunk 4: Boot Sector パーサ完了）
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
| 2 | dds-fs-common | FS共通トレイト・データ型定義 | 200 | 5 ✓ | 未計測 | 2026-05-19 |
| 3 | dds-disk-io | ReadOnlyDisk trait + FileBackedDisk 実装 | 181 | 6 ✓ | 未計測 | 2026-05-19 |
| 4 | dds-fs-ntfs | NTFS Boot Sector (VBR) パーサ | 197 | 6 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 |

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

### Chunk 2 詳細

- **対象ファイル**: `crates/fs-common/src/lib.rs`、`crates/fs-common/Cargo.toml`
- **実装内容**:
  - `FsType` enum（Ntfs / ExFat / Fat32 / Unknown、`Display` 実装、`FromStr`（大文字小文字非依存）、`label_ja(&self) -> &'static str`、Serialize/Deserialize 派生）
  - `EntryKind` enum（File / Directory / Symlink / Other、`is_directory(&self) -> bool`、`is_regular_file(&self) -> bool`）
  - `FsTimestamps` struct（`created` / `modified` / `accessed`: `Option<i64>`、`empty()` コンストラクタ、`Default` 派生）
  - `FsEntry` struct（`record_id` / `parent_record_id` / `name` / `full_path` / `size_bytes` / `kind` / `is_deleted` / `timestamps` / `fs_type`、`is_deleted(&self) -> bool` / `is_directory(&self) -> bool`、`Default` 派生せず明示構築強制）
  - `FsReader` trait — **read-only 型レベル保証**。公開メソッドは `fs_type()` / `root_record_id()` / `read_entry(record_id) -> CoreResult<Option<FsEntry>>` / `list_all_entries() -> CoreResult<Vec<FsEntry>>` の 4 つのみ。書き込み系 API（write/save/flush/truncate 等）はトレイトに一切定義されていないことを Grep で検証済み。
  - `Cargo.toml` に `serde.workspace = true` を追加
- **検証結果（tester 独立検証）**:
  - `cargo check -p dds-fs-common` … OK
  - `cargo test --lib -p dds-fs-common` … **5 passed; 0 failed**
    - `fs_type_display_outputs_correct_labels`
    - `fs_type_from_str_accepts_case_insensitive`
    - `entry_kind_helpers`
    - `fs_entry_default_is_alive_and_anonymous`
    - `fs_reader_trait_via_stub`
  - `cargo clippy -p dds-fs-common --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-common --no-deps` … 生成成功
  - 書き込み API 不在を Grep で確認 → **read-only 型レベル保証成立**
- **関連 FR**:
  - FR-LIVE-01〜07 の **基盤定義**として貢献（型・インタフェース確立）。具象FS実装後にあらためて達成判定するため、本チャンクでは完了マーク付与なし。
  - NFR-REL-01（書込禁止の型レベル制約）の **設計貢献**（FsReader trait に書き込みAPIが存在しない設計）
- **完了判定**: 完全完了（実装+テスト 200行ぴったり / 単体テスト 5件全パス / rustdoc 完備 / clippy clean / read-only 制約検証済）

### Chunk 3 詳細

- **対象ファイル**: `crates/disk-io/src/lib.rs`
- **実装内容**:
  - `DEFAULT_SECTOR_SIZE: u32 = 512` 定数
  - `ReadOnlyDisk` trait — **read-only 型レベル保証**。公開メソッドは `sector_size()` / `total_size()` / `read_at(offset, buf)` / `read_sector(sector_index, buf)` の 4 つのみ。書き込み系 API（write/save/flush/truncate/sync 等）はトレイトに一切定義されていないことを Grep で検証済み。
  - `FileBackedDisk` struct（`File` + `total_size: u64` + `sector_size: u32`、`#[derive(Debug)]`）
    - `open(path)` — read-only でファイルオープン、デフォルトセクタサイズ（512）を採用
    - `open_with_sector_size(path, sector_size)` — セクタサイズ指定（2の累乗バリデーション、`CoreError::InvalidArgument` で拒否）
    - `impl ReadOnlyDisk for FileBackedDisk` — 範囲外オフセット/インデックスは `CoreError::OutOfRange`、I/O エラーは `CoreError::Io` 経由
  - 内部ヘルパ `is_power_of_two`
- **検証結果（tester インライン検証）**:
  - `cargo check -p dds-disk-io` … OK
  - `cargo test --lib -p dds-disk-io` … **6 passed; 0 failed**
    - `file_backed_disk_open_reports_size_and_sector_size`
    - `file_backed_disk_read_at_returns_expected_bytes`
    - `file_backed_disk_read_at_out_of_range_returns_error`
    - `read_sector_validates_buffer_size`
    - `open_with_invalid_sector_size_returns_error`
    - `is_power_of_two_truth_table`
  - `cargo clippy -p dds-disk-io --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-disk-io --no-deps` … 生成成功
  - 書き込み API 不在を Grep で確認: 実装本体に `fn write` / `truncate` / `flush` / `sync` および `OpenOptions::new().write` / `File::create` は一切なし。`File::create` のヒットは `#[cfg(test)]` モジュール内のテストフィクスチャ作成のみ → **NFR-REL-01 完全担保**
- **関連 FR / NFR**:
  - **NFR-REL-01（ソースデバイス書込禁止）**: **完全達成**（型レベル + 実装レベル両方で書き込み API を排除。`ReadOnlyDisk` trait は書き込みメソッドを持たず、`FileBackedDisk` の実装は `File::open` のみ使用）→ FR要件達成マトリクスで反映
  - FR-LIVE-01〜07 の **基盤**として貢献（後続の FS リーダ群が `ReadOnlyDisk` を介してディスクアクセスする）
  - FR-DIAG-01〜02 の **基盤**として貢献（デバイス検出/情報取得の入口となる抽象層）
- **特記事項**: 当初 worktree 経由で builder が実装したコードが一度失われたため、main ブランチ直接で再構築。tester による独立 worktree 検証はスキップし、インライン検証で代替（テスト全件パス・clippy clean・doc 生成・書き込み API 不在 Grep を確認）。
- **完了判定**: 完全完了（実装+テスト 181行 / 単体テスト 6件全パス / rustdoc 完備 / clippy clean / read-only 制約を型レベル+実装レベル両方で検証済）

### Chunk 4 詳細

- **対象ファイル**:
  - `crates/fs-ntfs/src/boot_sector.rs`（実装+単体テスト 197行）
  - `crates/fs-ntfs/src/lib.rs`（`BootSector` / `BootSectorError` / `parse_boot_sector` の re-export）
  - `crates/fs-ntfs/tests/boot_sector_integration.rs`（結合テスト 29行 / 2件）
  - `crates/fs-ntfs/tests/common/mod.rs`（フィクスチャヘルパ 27行、zstd 解凍 + ground truth JSON ロード）
  - `crates/fs-ntfs/Cargo.toml`（`dds-fs-common.workspace = true` 追加、`[dev-dependencies]` に `zstd = "0.13"` と `serde_json` を追加）
- **実装内容**:
  - `BootSector` 構造体 — フィールド: `bytes_per_sector` / `sectors_per_cluster` / `media_descriptor` / `total_sectors` / `mft_lcn` / `mft_mirror_lcn` / `clusters_per_mft_record` / `clusters_per_index_record` / `volume_serial`
  - `BootSectorError` enum — バリアント: `BufferTooSmall` / `InvalidOemId` / `InvalidSignature` / `InvalidBytesPerSector` / `InvalidSectorsPerCluster`
  - `parse_boot_sector(bytes: &[u8]) -> Result<BootSector, BootSectorError>` — リトルエンディアン専用、OEM ID（`"NTFS    "`）/ 終端シグネチャ（`0x55 0xAA`）/ bytes-per-sector 非ゼロ / sectors-per-cluster 非ゼロを検証
  - `BootSector::cluster_size_bytes()` — クラスタサイズ（バイト単位）算出
  - `BootSector::mft_record_size_bytes()` — `clusters_per_mft_record` の符号付きエンコード対応（正値→クラスタ数、負値 N→2^|N|バイト）
  - `BootSector::mft_byte_offset()` — MFT 開始バイトオフセット算出
- **検証結果（tester 独立検証）**:
  - 実装+単体テスト行数: **197行**（200行上限内）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **6 passed; 0 failed**
    - `parses_valid_boot_sector_all_fields`
    - `rejects_short_buffer`
    - `rejects_invalid_oem_id_and_signature`
    - `rejects_zero_bps_and_zero_spc`
    - `mft_record_size_negative_and_positive_encodings`
    - `cluster_size_various_combinations`
  - `cargo test -p dds-fs-ntfs` … **8 passed**（単体6 + 結合2）
    - 結合: `parses_healthy_small_fixture_boot_sector` / `cluster_size_within_typical_range_for_fixtures`
    - フィクスチャ（`ntfs_healthy_small.img.zst`、`ntfs_with_5_deletions_small.img.zst`）を実際に zstd 解凍してパース成功
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を担保）
- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手**（Boot Sector 段階完了。$MFT 解析・属性パース・ディレクトリツリー構築は Chunk 5〜10 で実装予定）
  - FR-DIAG-04（FS 識別）への基盤貢献（OEM ID/シグネチャ検証ロジック）
- **完了判定**: 完全完了（実装+テスト 197行 / 単体テスト 6件全パス / 結合テスト 2件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を Grep で検証済）

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
- [~] **FR-LIVE-01: NTFS読み取り** 🚧 **部分着手**（Chunk 4 / dds-fs-ntfs）
  - Boot Sector (VBR) パーサ完了。OEM ID/シグネチャ検証、主要パラメータ抽出、MFT 開始オフセット算出が利用可能
  - 残作業: MFT エントリヘッダ（Chunk 5）、属性ヘッダ、$STANDARD_INFORMATION、$FILE_NAME、$DATA、$INDEX_ROOT/ALLOCATION、ディレクトリツリー構築、削除エントリ検出
  - 完了マークは `FsReader` trait の NTFS 実装が全要素を返せるようになった時点で付与
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

### 非機能要件 (NFR)
- [x] **NFR-REL-01: ソースデバイス書込禁止** ✅ **達成**（Chunk 3 / dds-disk-io）
  - 型レベル: `ReadOnlyDisk` trait に書き込み API 一切なし（4メソッドのみ）
  - 実装レベル: `FileBackedDisk` は `File::open`（read-only）のみ使用、書き込み API 不在を Grep で確認
  - 後続 FS リーダ群はこの抽象を介してディスクへアクセスするため、disk-io レベルで担保完了
  - 注: アプリ全体（出力先分離、Tauri 側の安全要件等）は別レイヤで継続検証
- [ ] NFR-REL-02: 出力先強制分離（ソースと同一なら拒否）
- [ ] NFR-REL-03: I/O エラー時のソース無影響
- [ ] NFR-REL-04: 監査ログ（tracing 構造化）

---

## リスクログ

（進行中に発見されたリスクをここに記録）

- なし

---

## 次の推奨アクション

**Chunk 5**: `dds-fs-ntfs` NTFS MFT エントリヘッダパーサ

- **対象クレート**: `crates/fs-ntfs/`
- **対象ファイル（予定）**: `crates/fs-ntfs/src/mft.rs`
- **目的**: $MFT 内の固定長 MFT レコード（FILE レコード）の先頭ヘッダをパースし、後続の属性パース処理の前提を確立する。`FILE` シグネチャ確認、Update Sequence Number/Array によるセクタ補正、レコード使用中/未使用フラグ、ディレクトリ/ファイル種別フラグ、レコード番号、シーケンス番号、属性開始オフセット、使用サイズ/割当サイズ等を抽出。
- **依存**:
  - Chunk 1（`dds-core` のエラー型 `CoreError`）
  - Chunk 4（`dds-fs-ntfs::BootSector` の `mft_record_size_bytes()` によるレコードサイズ決定）
- **推定行数**: 約 100行（実装 ~60 + テスト ~40）
- **着手前の準備**:
  1. `docs/specs/ntfs-references/` の MFT FILE レコードヘッダ仕様を再確認
  2. Update Sequence (Fixup) の処理仕様（各セクタ末尾2バイトを USA 配列で置換）に注意。Chunk 5 ではヘッダパースまでに留め、Fixup 適用は別チャンクで分離してもよい（200行制限を考慮し判断）
  3. レコードフラグ（0x01=in-use / 0x02=directory）の意味と削除レコード検出の関係を整理
  4. テスト: 手組みの 1024B バッファで正常/異常（不正シグネチャ / バッファ短すぎ / フラグ各種）をカバー
- **完了条件**: Chunk 1-4 と同等（cargo check / test --lib / clippy `-D warnings` / doc 全 OK、rustdoc 完備、単体テスト 3件以上、不正シグネチャ拒否のテスト含む）

詳細指示は builder 起動時に作成する `docs/chunk_5.md` で展開予定。
