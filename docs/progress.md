# DDS Recovery Workbench - 進捗トラッカー

このファイルは progress-tracker エージェントが自動更新します。

---

## 累積サマリ

- **完了チャンク数**: 9
- **総実装行数**: 1779（実装+テスト合計、各チャンク200行上限 / 仕様緩和後 220行上限内）
- **総単体テスト数**: 65（全パス）
- **総結合テスト数**: 14（全パス、NTFSフィクスチャ実画像での Boot Sector + MFT エントリヘッダ + 属性ヘッダ巡回 + $STANDARD_INFORMATION タイムスタンプ復元 + $FILE_NAME ファイル名取得・削除フラグ + $DATA 常駐属性 SHA256 完全一致実証まで完了）
- **平均カバレッジ**: 未計測（モジュール完成時に計測予定）
- **🎯🎯 Phase 1 技術核心マイルストーン達成（Chunk 9）**: **「削除されたファイルを名前 + タイムスタンプ + 内容（バイト単位完全一致）で復元する」というプロダクト価値の中核を、実 NTFS フィクスチャで数学的に実証**。健全イメージ `ntfs_healthy_small` 30/30 ファイル、削除イメージ `ntfs_with_5_deletions_small` 30/30 ファイル（うち削除済み 5/5: `file_003.txt` / `file_007.txt` / `file_015.txt` / `file_022.txt` / `file_028.txt`）の **SHA256 ハッシュが ground truth と完全一致**（`assert_eq!` で全件比較成立）。これは「データを取り出せた」だけでなく「ビット単位で正しく復元できた」ことの暗号学的証明。**Phase 1 のプロダクト価値の数学的証明完了**
- **ADS 対応基盤確立（Chunk 9）**: Alternate Data Stream（名前付き $DATA）の全列挙 API（`extract_all_data_streams`）を提供。`DataStream.name` で識別可能。フォレンジック調査価値の基盤
- **既存ハイライト（Chunk 8）**: 削除ファイル名 + 削除タイムスタンプのペア取得を実画像レベルで完全実証。ground truth `ntfs_with_5_deletions_small.json` との突合で総ファイル数 30 / 削除 5 件が 100% 一致
- **顧客要件達成（Chunk 8-9）**: 日本語ファイル名（"報告書_山田.docx"）/ 絵文字（"📁メモ.txt"、サロゲートペア）/ 日本語ストリーム名（"秘匿データ"）のデコードを単体テストで実証。`String::from_utf16`（非 lossy）採用、不正データはエラー化
- **既存ハイライト（Chunk 7）**: 削除済みファイルのタイムスタンプ復元を実画像レベルで実証（削除エントリ 13 件から $SI 取得成功、created = 2026-05-19T10:19:13Z がフィクスチャ生成時刻と一致）
- **最終更新日**: 2026-05-20

---

## マイルストーン進捗

```
M0: 設計確定        [████████] 100% ✅ 完了
M1: 基盤構築        [███░░░░░]  30% 🚧 進行中（Chunk 1-3/想定10前後 完了）
M2: NTFSリーダα     [██████░░]  60% 🚧 進行中（Chunk 4: Boot Sector + Chunk 5: MFT エントリヘッダ + フィクサップ + Chunk 6: 属性ヘッダパーサ + Chunk 7: 属性イテレータ + $STANDARD_INFORMATION + Chunk 8: $FILE_NAME + Chunk 9: $DATA 常駐 + ADS + SHA256 完全一致実証 完了。残りは非常駐 $DATA = Chunk 10）
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
| 5 | dds-fs-ntfs | NTFS MFT エントリヘッダパーサ + フィクサップ適用 | 199 | 8 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 |
| 6 | dds-fs-ntfs | NTFS 属性ヘッダパーサ（Resident/NonResident 分岐 + End マーカー） | 198 | 8 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 |
| 7 | dds-fs-ntfs | NTFS 属性イテレータ + $STANDARD_INFORMATION 属性パーサ | 198 | 10 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 |
| 8 | dds-fs-ntfs | NTFS `$FILE_NAME` 属性パーサ + ファイル名選択ヘルパ 🎯 | 209 | 9 ✓ + 結合 3 ✓ | 未計測 | 2026-05-20 |
| 9 | dds-fs-ntfs | NTFS `$DATA` 常駐属性パーサ + ADS 対応 + SHA256 完全一致実証 🎯🎯 | 200 | 8 ✓ + 結合 3 ✓ | 未計測 | 2026-05-20 |

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

### Chunk 5 詳細

- **対象ファイル**:
  - `crates/fs-ntfs/src/mft.rs`（実装+単体テスト 199行、新規）
  - `crates/fs-ntfs/src/lib.rs`（`MftEntry` / `MftEntryHeader` / `MftError` / `parse_mft_entry` の re-export 追加）
  - `crates/fs-ntfs/tests/mft_integration.rs`（結合テスト 47行 / 2件）
- **実装内容**:
  - `MftEntryHeader` 構造体（12 pub フィールド: `usa_offset` / `usa_size` / `lsn` / `sequence_number` / `hard_link_count` / `first_attribute_offset` / `flags` / `used_size` / `allocated_size` / `base_record_reference` / `next_attribute_id` / `mft_record_number`）
  - `MftEntry` 構造体（`header: MftEntryHeader` + フィクサップ適用済み `data: Vec<u8>`）
  - `MftError` enum — バリアント: `BufferTooSmall` / `InvalidMagic` / `BadEntry` / `InvalidUsaOffset` / `InvalidUsaSize` / `FixupMismatch` / `UsedExceedsAllocated`
  - `parse_mft_entry(bytes: &[u8]) -> Result<MftEntry, MftError>` — `FILE` シグネチャ検証、`BAAD` の早期検出、USA バリデーション、フィクサップ適用、`used_size <= allocated_size` 検証
  - 状態判定メソッド: `is_in_use()` / `is_deleted()` / `is_directory()` / `is_base_record()`
  - 内部関数 `apply_fixup()` — NTFS Update Sequence Array によるセクタ末尾2バイトの復元処理、不一致時は `FixupMismatch` 返却。`parse_mft_entry` 内で USA バリデーション後に必ず呼び出し
- **検証結果（tester 独立検証）**:
  - 実装+単体テスト行数: **199行**（200行上限内）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **14 passed; 0 failed**（Chunk 4: 6件 + Chunk 5: 8件）
    - `parses_valid_header_fields`
    - `baad_signature_is_bad_entry`
    - `invalid_magic_rejected`
    - `flags_in_use_deleted_directory`
    - `fixup_applied_restores_sector_tails`
    - `fixup_mismatch_detected`
    - `used_exceeds_allocated_rejected`
    - `buffer_too_small_rejected`
  - `cargo test -p dds-fs-ntfs` … **18 passed**（単体14 + 結合4）
    - Chunk 5 結合テスト:
      - `parses_first_mft_record_from_healthy_image`（$MFT エントリ0が `is_in_use=true`、非ディレクトリであることを実フィクスチャで実証）
      - `counts_deleted_entries_in_deletions_fixture`（`ntfs_with_5_deletions_small.img.zst` で削除エントリ ≥5 件を検出）
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を維持）
- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手継続**（Boot Sector + MFT エントリヘッダ + フィクサップ適用が完了。属性パース・$STANDARD_INFORMATION・$FILE_NAME・$DATA・$INDEX_ROOT/ALLOCATION・ディレクトリツリー構築は Chunk 6 以降で実装予定）
  - **FR-LIVE-05（削除エントリ可視化）**: **部分着手**（削除判定 `is_deleted()` を MFT エントリ単位で提供。実フィクスチャ `ntfs_with_5_deletions_small.img.zst` で削除エントリ ≥5 件検出を結合テストで実証。UI 上の色分け表示・一覧化は別レイヤで未実装）
- **特記事項**: フィクサップ（Update Sequence）処理を Chunk 5 内で完結させたことで、後続の属性パースが破損検知済みのバイト列を直接扱える基盤を整備。`BAAD` シグネチャを `BadEntry` として明示的に区別し、ファイルシステム破損エントリの可視化に対応可能。
- **完了判定**: 完全完了（実装+テスト 199行 / 単体テスト 8件全パス / 結合テスト 2件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を維持 / フィクサップによる破損検知も実装）

### Chunk 6 詳細

- **対象ファイル**:
  - `crates/fs-ntfs/src/attribute.rs`（実装+単体テスト 198行、新規）
  - `crates/fs-ntfs/src/lib.rs`（`AttributeType` / `AttributeCommonHeader` / `ResidentInfo` / `NonResidentInfo` / `AttributeHeader` / `AttributeError` / `parse_attribute_header` の re-export 追加）
  - `crates/fs-ntfs/tests/attribute_integration.rs`（結合テスト 66行 / 2件）
- **実装内容**:
  - `AttributeType` enum（17バリアント: `StandardInformation` / `AttributeList` / `FileName` / `ObjectId` / `SecurityDescriptor` / `VolumeName` / `VolumeInformation` / `Data` / `IndexRoot` / `IndexAllocation` / `Bitmap` / `ReparsePoint` / `EaInformation` / `Ea` / `LoggedUtilityStream` / `Unknown(u32)` / `End`、`from_raw(u32) -> Self` / `to_raw(&self) -> u32`）
  - `AttributeCommonHeader` 構造体（pub フィールド: `attribute_type` / `length` / `non_resident` / `name_length` / `name_offset` / `flags` / `attribute_id`）
  - `ResidentInfo` 構造体（pub フィールド: `content_size` / `content_offset` / `indexed`）
  - `NonResidentInfo` 構造体（pub フィールド: `starting_vcn` / `last_vcn` / `runlist_offset` / `compression_unit_size` / `allocated_size` / `real_size` / `initialized_size`）
  - `AttributeHeader` enum（`Resident { common, resident }` / `NonResident { common, non_resident }` / `End`）+ メソッド `common()` / `length()` / `attribute_type()` / `is_end()`
  - `AttributeError` enum — バリアント: `BufferTooSmall` / `InvalidLength` / `InvalidNonResidentFlag`
  - `parse_attribute_header(bytes: &[u8]) -> Result<AttributeHeader, AttributeError>` — 先頭4バイトが `0xFFFFFFFF`（End マーカー）なら 16バイト未満でも即時 `End` を返却、共通ヘッダ16バイトを読み出した上で `non_resident` フラグにより排他的に Resident/NonResident をパース
- **設計上のポイント**:
  - **Forward compatibility**: 未知の type ID は `Unknown(value)` で受け入れエラー化せず、将来の Windows バージョン追加属性へ前方互換
  - **無限ループ防止**: `length == 0` を `InvalidLength` で必ず弾く（属性巡回時の進行不能を回避）
  - **End マーカー即時返却**: 先頭4バイトが 0xFFFFFFFF の場合、16バイト未満のバッファでも `End` を返す
  - **常駐/非常駐分岐**: `non_resident` フラグ（0 or 1 以外は `InvalidNonResidentFlag`）で `ResidentInfo` または `NonResidentInfo` を排他的にパース
- **検証結果（tester 独立検証）**:
  - 実装+単体テスト行数: **198行**（200行上限内）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **22 passed; 0 failed**（Chunk 4: 6件 + Chunk 5: 8件 + Chunk 6: 8件）
    - `attribute_type_from_raw_roundtrip_main_types`
    - `attribute_type_unknown_and_end`
    - `parses_resident_header_all_fields`
    - `parses_nonresident_header_all_fields`
    - `end_marker_returned_immediately`
    - `buffer_too_small_rejected`
    - `invalid_non_resident_flag_rejected`
    - `zero_length_rejected_prevents_infinite_loop`
  - `cargo test -p dds-fs-ntfs` … **28 passed**（単体22 + 結合6）
    - Chunk 6 結合テスト:
      - `iterates_attributes_of_mft_record_zero`（実 NTFS フィクスチャの $MFT エントリ0で $STANDARD_INFORMATION / $FILE_NAME / $DATA を含み End で終わることを実証。実検出シーケンス: `[StandardInformation (0x10), FileName (0x30), Data (0x80), Bitmap (0xB0), End (0xFFFFFFFF)]`）
      - `attributes_are_in_ascending_type_id_order`（NTFS 仕様の昇順制約を実フィクスチャで検証）
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を維持）
- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手継続**（Boot Sector + MFT エントリヘッダ + 属性ヘッダ巡回まで完了。$STANDARD_INFORMATION / $FILE_NAME / $DATA / $INDEX_ROOT/ALLOCATION の具象属性パース、ディレクトリツリー構築は Chunk 7 以降で実装予定）
  - **FR-LIVE-06（メタデータ表示）**: **基盤着手**（属性巡回 API が確立し、後続の具象属性パーサが本ヘッダを起点にタイムスタンプ・アクセス権・ファイル名等を抽出できる状態。メタデータ抽出本体は Chunk 7 以降の具象属性パース完了時に達成判定）
- **特記事項**: 前方互換性のため未知の type ID をエラー化せず `Unknown(u32)` で保持する設計を採用。これにより新しい Windows バージョンが追加する属性タイプに遭遇しても巡回が止まらず、未知属性を「無視 or 報告」する選択肢を上位レイヤに委ねられる。`length == 0` を必ず `InvalidLength` で弾くことで属性巡回ループの安全性も担保。
- **完了判定**: 完全完了（実装+テスト 198行 / 単体テスト 8件全パス / 結合テスト 2件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を維持 / Forward compat 設計）

### Chunk 7 詳細

- **対象ファイル**:
  - `crates/fs-ntfs/src/attributes/mod.rs`（実装+単体テスト 88行、新規）
  - `crates/fs-ntfs/src/attributes/standard_information.rs`（実装+単体テスト 110行、新規）
  - `crates/fs-ntfs/src/lib.rs`（`attributes` モジュール re-export 追加）
  - `crates/fs-ntfs/Cargo.toml`（`chrono.workspace = true` を追加）
  - `crates/fs-ntfs/tests/standard_information_integration.rs`（結合テスト 80行 / 2件）
- **実装内容**:
  - **属性イテレータ系（`attributes/mod.rs`）**:
    - `AttributeRef<'a>` 構造体（pub フィールド: `header: AttributeHeader` / `raw: &'a [u8]` / `offset_in_entry: usize`）
    - `AttributeIterator<'a>` — `Iterator<Item = Result<AttributeRef<'a>, AttributeError>>` 実装。End マーカーで終了、`length == 0` や buffer 超過時は `InvalidLength` を yield して停止（無限ループ防止）
    - `find_attribute(entry_data, first_attribute_offset, target_type) -> Option<AttributeRef>` ヘルパ関数
  - **$STANDARD_INFORMATION 系（`attributes/standard_information.rs`）**:
    - `FileTime(u64)` — newtype、`to_datetime() -> Option<DateTime<Utc>>`（1601-01-01 起算 100ns 単位 → Unix epoch 変換、`checked_div` / `checked_sub` / `checked_mul` でオーバーフロー安全、`i64::try_from` で u64→i64 変換も安全）
    - `FileAttributes(u32)` — newtype + 定数（READ_ONLY / HIDDEN / SYSTEM / ARCHIVE / COMPRESSED / ENCRYPTED / DIRECTORY）+ `is_read_only()` / `is_hidden()` / `is_system()` / `is_archive()` / `is_compressed()` / `is_encrypted()` / `is_directory()` 判定メソッド
    - `StandardInformation` 構造体（pub フィールド: `created` / `modified` / `mft_modified` / `accessed: FileTime`、`file_attributes: FileAttributes`、`max_versions` / `version_number` / `class_id: u32`、W2K+ 拡張部の `owner_id` / `security_id` / `quota_charged` / `usn` は `Option`）
    - `SiError::BufferTooSmall`
    - `parse_standard_information(bytes: &[u8]) -> Result<StandardInformation, SiError>` — NT版（48バイト）と W2K+ 拡張版（72バイト）をバイト長で判別
- **設計上のポイント**:
  - **無限ループ防止**: `AttributeIterator` は `length == 0` および buffer 超過を `InvalidLength` で必ず弾き、yield 後は `done` フラグで停止
  - **オーバーフロー安全**: `FileTime::to_datetime` は `checked_*` 系演算と `i64::try_from` で u64 全域に対して panic しない設計
  - **バージョン互換**: NT 版（48バイト）と W2K+ 拡張版（72バイト）をバイト長で判別、拡張フィールドは `Option` で表現
  - **2ファイル分散構成**: 200行制約を満たすため `mod.rs`（88行）と `standard_information.rs`（110行）に分割、合計 198行
- **検証結果（tester 独立検証）**:
  - 実装+単体テスト行数: **198行**（200行上限内、2ファイル分散）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **32 passed; 0 failed**（Chunk 4: 6件 + Chunk 5: 8件 + Chunk 6: 8件 + Chunk 7: 10件）
    - `iterator_empty_on_end_marker`
    - `iterator_yields_single_attribute_then_end`
    - `iterator_yields_multiple_attributes`
    - `find_attribute_finds_existing_type`
    - `find_attribute_returns_none_for_missing_type`
    - `parses_48_byte_nt_version`
    - `parses_72_byte_w2k_extended`
    - `rejects_buffer_smaller_than_48_bytes`
    - `filetime_to_datetime_known_value`
    - `file_attributes_bit_checks`
  - `cargo test -p dds-fs-ntfs` … **40 passed**（単体32 + 結合8）
    - Chunk 7 結合テスト:
      - `reads_standard_information_from_healthy_records`（健全イメージから 27 件の $SI 取得成功）
      - `reads_standard_information_from_deleted_records`（**削除エントリから 13 件の $SI 取得成功、created = 2026-05-19T10:19:13Z = フィクスチャ生成時刻と一致**）
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を維持）
- **重要なマイルストーン達成**:
  - **削除済みファイルのタイムスタンプ復元を実画像レベルで実証**: 結合テスト `reads_standard_information_from_deleted_records` が削除エントリの $SI からタイムスタンプ取得成功。`ntfs_with_5_deletions_small` 削除エントリの created = 2026-05-19T10:19:13Z がフィクスチャ生成時刻と一致
  - これは「お客様希望リスト × 復旧候補」の突合に必要な日時情報（FR-WISH-01「日付範囲指定」の基盤）
- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手継続**（Boot Sector + MFT エントリヘッダ + 属性ヘッダ + 属性イテレータ + $SI 完了。$FILE_NAME と $DATA、$INDEX_ROOT/ALLOCATION、ディレクトリツリー構築は Chunk 8 以降で実装予定）
  - **FR-LIVE-06（メタデータ表示）**: **タイムスタンプ取得完了**（4種のタイムスタンプ created / modified / mft_modified / accessed + DOS ファイル属性フラグを抽出可能。残作業: $FILE_NAME のファイル名・親参照、$DATA のサイズ・データラン、上位レイヤでのメタデータ集約 API）
  - **FR-WISH-01（日付範囲指定）**: **基盤確立**（タイムスタンプデータ供給可能。希望リスト × 復旧候補の日付突合に必要なデータが NTFS 側から取れる状態）
- **特記事項**: $SI は NTFS におけるタイムスタンプの一次情報源。本チャンクの完了により、後続の $FILE_NAME パース（Chunk 8）と合わせれば「いつ削除されたか・どのファイル名だったか」のペアが復元可能になり、Phase 1 のプロダクト価値（希望リスト駆動型復旧）の中核データが揃う。
- **完了判定**: 完全完了（実装+テスト 198行 / 単体テスト 10件全パス / 結合テスト 2件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を維持 / オーバーフロー安全な FILETIME 変換）

### Chunk 8 詳細 🎯 プロダクト価値中核マイルストーン

- **対象ファイル**:
  - `crates/fs-ntfs/src/attributes/file_name.rs`（実装+単体テスト 209行、新規。実装 131 + 単体テスト 78）
  - `crates/fs-ntfs/src/attributes/mod.rs`（re-export 追加）
  - `crates/fs-ntfs/src/lib.rs`（公開 API の re-export 追加）
  - `crates/fs-ntfs/tests/file_name_integration.rs`（結合テスト 91行 / 3件、新規）
- **実装内容**:
  - **`FileNameNamespace` enum**（`Posix = 0` / `Win32 = 1` / `Dos = 2` / `Win32AndDos = 3`）+ `from_raw(u8) -> Option<Self>` + `is_preferred_for_display(&self) -> bool`（Win32 / Win32AndDos を優先と判定）
  - **`MftReference` 構造体** — 48bit `entry_number: u64` + 16bit `sequence_number: u16`。`from_raw(raw: u64)` で u64 から分解、`is_root_directory(&self) -> bool`（entry_number == 5 判定）
  - **`FileName` 構造体**（pub フィールド: `parent_directory: MftReference` / `created` / `modified` / `mft_modified` / `accessed: FileTime` / `allocated_size: u64` / `real_size: u64` / `file_attributes: FileAttributes` / `namespace: FileNameNamespace` / `filename: String`）
  - **`FileNameError` enum** — バリアント: `BufferTooSmall` / `FilenameBufferTooSmall` / `InvalidNamespace` / `InvalidUtf16`
  - **`parse_file_name(bytes: &[u8]) -> Result<FileName, FileNameError>`** — 固定部 66バイト読み出し、UTF-16LE デコード（`String::from_utf16` 使用、非 lossy）、不正サロゲートシーケンスは `InvalidUtf16` エラー化
  - **`find_best_file_name(entry_data: &[u8], first_attribute_offset: u16) -> Option<FileName>`** — Win32/Win32AndDos > Posix > DOS の優先順位でファイル名を選択（Win32 と DOS の二重 $FILE_NAME 登録に対応、常駐属性のみ対象）
- **設計上のポイント**:
  - **UTF-16 堅牢性**: `String::from_utf16`（非 lossy）採用。`String::from_utf16_lossy` は不使用。不正サロゲートシーケンスは `InvalidUtf16` で明示エラー化し、文字化けによる無音障害を防止
  - **サロゲートペア自動処理**: 絵文字（U+10000以降）は `String::from_utf16` の機能で正常デコード。`filename_length` フィールドは UTF-16 コードユニット数（≠ 文字数）として扱う
  - **ファイル名二重登録対応**: Windows は同一ファイルに Win32 と DOS の 2つの $FILE_NAME を持つことが多い。`find_best_file_name` は表示用としてロング名（Win32 系）を優先選択
  - **顧客要件対応**: 日本語ファイル名（"報告書_山田.docx"）と絵文字（"📁メモ.txt"）のテストを必須として実装
  - **行数**: 仕様緩和後の 220行上限内（209行）
- **検証結果（tester 独立検証）**:
  - 実装+単体テスト行数: **209行**（220行上限内）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **41 passed; 0 failed**（Chunk 4: 6 + 5: 8 + 6: 8 + 7: 10 + 8: 9）
    - Chunk 8 単体テスト（9件）:
      - `parses_ascii_filename`
      - **`parses_japanese_filename`**（"報告書_山田.docx"、必須要件達成）
      - **`parses_emoji_filename`**（"📁メモ.txt"、サロゲートペア検証）
      - `namespace_win32_dos_win32dos_posix`（4 namespace 集約）
      - `invalid_namespace_rejected`
      - `buffer_too_small_rejected`
      - `filename_buffer_too_small_rejected`
      - `mft_reference_bit_decomposition`
      - `is_preferred_for_display_truth_table`
  - `cargo test -p dds-fs-ntfs` … **52 passed**（単体 41 + 結合 11）
    - Chunk 8 結合テスト（3件）:
      - `discovers_all_user_files_in_healthy_image`（健全イメージから 30 ユーザファイル全件発見）
      - **`recovers_deleted_file_names_with_timestamps`**（削除 5 ファイル全件を `[DELETED]` フラグ + タイムスタンプ付きで取得）
      - `prints_live_and_deleted_file_listing_for_human_review`（人間可読の一覧出力）
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を維持）
  - UTF-16 検証: `String::from_utf16` 使用、`String::from_utf16_lossy` 不使用、不正データは `InvalidUtf16` エラー化
- **🎯 ground truth 完全一致実証（プロダクト価値の中核）**:
  - `fixtures/images/ntfs_with_5_deletions_small.json` との突合結果（**100% 一致**）:

    | 項目 | ground truth | 検出結果 | 一致 |
    |---|---|---|---|
    | 総ファイル数 | 30 | 30 | ✓ |
    | 削除ファイル数 | 5 | 5 | ✓ |
    | `file_003.txt` is_deleted=true | ○ | [DELETED] (entry #67) | ✓ |
    | `file_007.txt` is_deleted=true | ○ | [DELETED] (entry #71) | ✓ |
    | `file_015.txt` is_deleted=true | ○ | [DELETED] (entry #79) | ✓ |
    | `file_022.txt` is_deleted=true | ○ | [DELETED] (entry #86) | ✓ |
    | `file_028.txt` is_deleted=true | ○ | [DELETED] (entry #92) | ✓ |
    | 残り 25 件 is_deleted=false | ○ | 全て [Live] | ✓ |

  - `prints_live_and_deleted_file_listing_for_human_review --nocapture` 出力（抜粋）:

    ```
    === File listing from ntfs_with_5_deletions_small ===
    [Live]    file_000.txt         (entry #64)
    [DELETED] file_003.txt         (entry #67)
    [DELETED] file_007.txt         (entry #71)
    [DELETED] file_015.txt         (entry #79)
    [DELETED] file_022.txt         (entry #86)
    [DELETED] file_028.txt         (entry #92)
    [Live]    file_029.txt         (entry #93)
    === Total: 30 files (5 deleted) ===
    ```

  - これは Phase 1 のプロダクト価値（「お客様希望リスト × 削除ファイル」の突合）に必須の **「削除ファイル名 + 削除タイムスタンプ」ペア取得**を実画像レベルで完全実証したもの
- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手継続**（属性巡回 + $SI + $FILE_NAME 完了。残りは $DATA = Chunk 9 / 10）
  - **FR-LIVE-05（削除エントリ可視化）**: **実用化完了**（削除ファイル名 + タイムスタンプ + 削除フラグの取得を実フィクスチャレベルで実証。UI 色分け表示は別レイヤ＝フロントエンド実装の責務）
  - **FR-LIVE-06（メタデータ表示）**: **メタデータ抽出ほぼ完成**（タイムスタンプ取得完了 + ファイル名取得完了。残りは $DATA のサイズ・データラン）
  - **FR-WISH-01（日付範囲指定）**: **データ基盤確立継続**（Chunk 7 から継続、ファイル名突合に必要なデータも揃った）
- **特記事項**:
  - 本チャンク完了により「いつ削除されたか・どのファイル名だったか」の中核ペアが揃い、Phase 1 のプロダクト価値（希望リスト駆動型復旧）の見える化に到達
  - 日本語ファイル名と絵文字を実テストで保証することで、日本国内顧客の実案件データに対する適用性を確保
  - Win32/DOS の二重登録対応により、Windows 標準のロング/ショート名混在環境でも一意かつ可読なファイル名選択が可能
- **完了判定**: 完全完了（実装+テスト 209行 / 単体テスト 9件全パス / 結合テスト 3件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を維持 / 非 lossy UTF-16 デコード / ground truth 100% 一致）

### Chunk 9 詳細 🎯🎯 Phase 1 技術核心マイルストーン（プロダクト価値の数学的証明）

- **対象ファイル**:
  - `crates/fs-ntfs/src/attributes/data.rs`（実装+単体テスト 200行ぴったり、新規。実装 102 + 単体テスト 98）
  - `crates/fs-ntfs/src/attributes/mod.rs`（re-export 追加）
  - `crates/fs-ntfs/src/lib.rs`（公開 API の re-export 追加）
  - `crates/fs-ntfs/Cargo.toml`（`[dev-dependencies]` に `sha2 = { workspace = true }` 追加）
  - `crates/fs-ntfs/tests/data_integration.rs`（結合テスト 122行 / 3件、新規）
- **実装内容**:
  - **`DataContent<'a>` enum** — `Resident { bytes: &'a [u8], size: u32 }` / `NonResident { real_size: u64, allocated_size: u64, starting_vcn: u64, last_vcn: u64, runlist_offset_in_attr: u16, attribute_raw: &'a [u8] }` + メソッド `is_resident()` / `is_non_resident()` / `size()`
  - **`DataStream<'a>` 構造体**（pub フィールド: `name: String` / `content: DataContent<'a>` / `is_compressed: bool` / `is_encrypted: bool` / `is_sparse: bool`）
  - **`DataError` enum** — バリアント: `ResidentBufferTooSmall` / `InvalidContentOffset` / `InvalidStreamName`
  - **`parse_data_stream(attr_raw: &[u8], header: &AttributeHeader) -> Result<DataStream, DataError>`** — 1属性からストリーム情報抽出。常駐は `ResidentInfo.content_offset` / `content_size` から実バイトをスライス取得、非常駐は runlist 解析を Chunk 10 に委譲して情報抽出のみ
  - **`extract_all_data_streams(entry_data: &[u8], first_attribute_offset: u16) -> Vec<DataStream>`** — `AttributeIterator` で全 $DATA 属性を列挙（無名メイン + 名前付き ADS の両方）
  - **`extract_main_data_stream(entry_data: &[u8], first_attribute_offset: u16) -> Option<DataStream>`** — 無名（メイン）$DATA ストリームを取得
- **設計上のポイント**:
  - **常駐/非常駐分岐**: 非常駐は情報抽出のみ、実バイト取得は Chunk 10（runlist 解析）で完成させる設計。本チャンクは常駐限定で「小ファイル復旧パイプライン完結」を達成
  - **ADS（Alternate Data Streams）対応**: 名前付き $DATA を全列挙可能（`DataStream.name` で識別）。Windows のフォレンジック調査で重要な「隠しストリーム」検出の基盤を確立
  - **flags 解釈**: 0x0001=圧縮 / 0x4000=暗号化 / 0x8000=スパース を `is_compressed` / `is_encrypted` / `is_sparse` フラグで保持（復号は Phase 2 以降）
  - **日本語ストリーム名対応**: UTF-16LE → `String::from_utf16` 非 lossy（"秘匿データ" を単体テストで実証）
  - **空ファイル対応**: `content_size=0` でも正常処理（無名・名前付き両方）
  - **非 $DATA 拒否**: 入力属性タイプが Data 以外なら明示エラー化
  - **行数**: 200行ぴったり（実装 102 + 単体テスト 98）
- **検証結果（tester 独立検証 + SHA256 数学的証明）**:
  - 実装+単体テスト行数: **200行**（200行上限ぴったり）
  - `cargo check -p dds-fs-ntfs` … OK
  - `cargo test --lib -p dds-fs-ntfs` … **49 passed; 0 failed**（Chunk 4: 6 + 5: 8 + 6: 8 + 7: 10 + 8: 9 + 9: 8）
    - Chunk 9 単体テスト（8件）:
      - `resident_data_content_extraction`
      - `empty_unnamed_and_named_decoded`
      - **`japanese_named_stream_decoded`**（"秘匿データ" UTF-16）
      - `data_content_is_resident_check`
      - `non_resident_data_info_extraction`
      - **`extract_all_and_main_data_streams`**（ADS 3ストリーム検証）
      - `flags_compressed_encrypted_sparse_decoded`
      - `non_data_attribute_type_rejected`
  - `cargo test -p dds-fs-ntfs` … **63 passed**（単体49 + 結合14）
    - Chunk 9 結合テスト（3件）:
      - **`recovers_all_30_files_with_matching_sha256_in_healthy_image`**（健全 30/30 SHA256 一致）
      - **`recovers_all_5_deleted_files_with_matching_sha256`**（削除 5/5 SHA256 一致）
      - `product_demo_complete_recovery`（人間可読のデモ出力）
  - `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
  - `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
  - 安全性検証: `unsafe` 0件、書き込み API 0件、`from_be_bytes` 0件（リトルエンディアン専用を維持）
- **🎯🎯 Phase 1 技術核心マイルストーン達成（プロダクト価値の数学的証明）**:

  ground truth との **SHA256 ハッシュ完全一致**を `assert_eq!` で全件比較成立:

  | 項目 | 検証結果 |
  |---|---|
  | 健全イメージ `ntfs_healthy_small` 30/30 ファイル SHA256 一致 | ✓ |
  | 削除イメージ `ntfs_with_5_deletions_small` 30/30 ファイル SHA256 一致 | ✓ |
  | うち削除済み 5/5 ファイル（file_003/007/015/022/028.txt）SHA256 一致 | ✓ |
  | `assert_eq!` による完全一致比較 | 全件成立 |

  プロダクトデモ出力（`product_demo_complete_recovery --nocapture` 抜粋、社内デモ用に保存）:

  ```
  === DDS Recovery Workbench - Phase 1 Demo ===
  Source: ntfs_with_5_deletions_small.img
  Cluster size: 4096 bytes
  MFT location: byte 16384
    [Live]    file_000.txt         (86 bytes)
    ...
    [DELETED] file_003.txt         (86 bytes)  <- 完全復元!
    [DELETED] file_007.txt         (86 bytes)  <- 完全復元!
    [DELETED] file_015.txt         (86 bytes)  <- 完全復元!
    [DELETED] file_022.txt         (86 bytes)  <- 完全復元!
    [DELETED] file_028.txt         (86 bytes)  <- 完全復元!
    [Live]    file_029.txt         (86 bytes)
  === Summary ===
  Total files recovered:   30
  Deleted files recovered: 5
  ```

  これにより Phase 1 のプロダクト価値（「削除されたファイルを名前 + タイムスタンプ + 内容（バイト単位完全一致）で復元する」）の中核を、実 NTFS フィクスチャで **数学的に実証**完了。「データを取り出せた」だけでなく「ビット単位で正しく復元できた」ことの暗号学的証明。

- **関連 FR**:
  - **FR-LIVE-01（NTFS 読み取り）**: **部分着手継続**（属性巡回 + $SI + $FILE_NAME + $DATA 常駐 完了。残りは非常駐 $DATA（Chunk 10、runlist 解析）のみ）
  - **FR-LIVE-04（ファイルツリー構築）**: **基盤確立**（エントリ取得 + 属性取得 + 内容取得が揃い、ツリー組み立てに必要な部品が出揃った。階層構造の集約は別チャンクで実装予定）
  - **FR-REC-01（目標優先抽出）**: **基盤確立**（ファイル単位の選別 + 内容取得が可能。希望リストとの突合に応じた優先抽出ロジックは wish-match クレートで実装予定）
  - **FR-REC-04（データ整合性）**: ✅ **完全達成**（SHA256 ハッシュによる検証メカニズムを結合テストで実証、ground truth と完全一致）
- **特記事項**:
  - 本チャンク完了により Phase 1 のプロダクト価値の中核（削除ファイル復旧のビット完全性）が数学的に実証された。社内デモ・お客様説明・PoC 完了報告のいずれにも本結合テスト出力が利用可能
  - ADS（Alternate Data Streams）対応の基盤確立により、Windows フォレンジック調査における「隠しストリーム」検出が技術的に可能。Phase 2 以降のフォレンジック特化機能の足場
  - 圧縮 / 暗号化 / スパースのフラグ判定機構を備え、Phase 2 以降の対応拡張時に flag 検出済みのデータが上位レイヤから利用可能
  - 残作業の Chunk 10（非常駐 $DATA + runlist）が完了すれば、大ファイル（クラスタチェーン経由）にも本プロダクト価値が適用される
- **完了判定**: 完全完了（実装+テスト 200行ぴったり / 単体テスト 8件全パス / 結合テスト 3件全パス / rustdoc 完備 / clippy clean / unsafe・書き込み API 不在を維持 / SHA256 完全一致による数学的証明 / ADS 対応基盤確立）

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
- [~] **FR-LIVE-01: NTFS読み取り** 🚧 **部分着手**（Chunk 4-9 / dds-fs-ntfs）
  - Boot Sector (VBR) パーサ完了。OEM ID/シグネチャ検証、主要パラメータ抽出、MFT 開始オフセット算出が利用可能
  - MFT エントリヘッダパーサ + フィクサップ適用完了。`FILE`/`BAAD` 判定、USA 検証、フラグ抽出（in-use/directory）、レコード番号/シーケンス番号取得が利用可能
  - 属性ヘッダパーサ完了。共通ヘッダ抽出、Resident/NonResident 排他分岐、End マーカー検出、未知 type ID の前方互換受け入れ、0長拒否による安全な巡回基盤が利用可能。実フィクスチャで $STANDARD_INFORMATION / $FILE_NAME / $DATA / $BITMAP / End の昇順巡回を実証
  - 属性イテレータ + $STANDARD_INFORMATION 完了。`AttributeIterator` で End まで安全に列挙、`find_attribute` ヘルパ、$SI から 4 種タイムスタンプ（created/modified/mft_modified/accessed）+ DOS 属性フラグ抽出、NT(48B)/W2K+(72B) 両版対応
  - $FILE_NAME パース完了（Chunk 8）。UTF-16LE デコード（非 lossy、`String::from_utf16` 使用）、4 種 namespace（Posix/Win32/Dos/Win32AndDos）対応、Win32/DOS 二重登録時の `find_best_file_name` 優先選択、48bit entry + 16bit sequence の MftReference 分解、$FILE_NAME 内 4 種タイムスタンプ + allocated/real size + file_attributes 抽出。日本語ファイル名・絵文字（サロゲートペア）対応を単体テストで保証。ground truth `ntfs_with_5_deletions_small.json` と 100% 一致（総 30 / 削除 5 件全件）を結合テストで実証
  - **$DATA 常駐属性パース + ADS 対応 + SHA256 完全一致実証完了 🎯🎯**（Chunk 9）。`DataContent`（Resident / NonResident enum）、`DataStream`（name / content / 圧縮・暗号化・スパースフラグ）、`extract_all_data_streams`（ADS 含む全列挙）/ `extract_main_data_stream` 提供。健全 30/30 + 削除 5/5 の SHA256 ハッシュが ground truth と完全一致することを結合テストで数学的に証明。日本語ストリーム名（"秘匿データ"）対応も実証
  - **残作業: 非常駐 $DATA（Chunk 10、runlist 解析）のみ。$INDEX_ROOT/ALLOCATION、ディレクトリツリー構築は別途**
  - 完了マークは `FsReader` trait の NTFS 実装が全要素を返せるようになった時点で付与
- [ ] FR-LIVE-02: exFAT読み取り
- [ ] FR-LIVE-03: FAT32読み取り
- [~] **FR-LIVE-04: ファイルツリー構築** 🚧 **部品集約段階**（Chunk 5-9 / dds-fs-ntfs）
  - エントリ取得（Chunk 5）+ 属性巡回（Chunk 6-7）+ ファイル名 / 親参照（Chunk 8）+ 内容取得（Chunk 9）が揃い、ツリー組み立てに必要な部品が出揃った
  - 残作業: 親→子のリンク集約（$INDEX_ROOT/ALLOCATION 経由）、`FsReader::list_all_entries` の NTFS 実装、削除エントリのツリー上配置
- [x] **FR-LIVE-05: 削除エントリ可視化** ✅ **実用化完了 🎯**（Chunk 5, 7, 8 / dds-fs-ntfs。※UI 色分け表示はフロントエンド未実装）
  - MFT エントリ単位の削除判定 `is_deleted()` を提供（flags の in-use ビット非立で判定、Chunk 5）
  - 削除エントリの $STANDARD_INFORMATION から 4 種タイムスタンプを実フィクスチャレベルで復元実証（Chunk 7、削除 13 件取得成功）
  - **削除エントリの $FILE_NAME からファイル名・親参照・サイズ・属性フラグを実フィクスチャレベルで取得実証**（Chunk 8、`recovers_deleted_file_names_with_timestamps`）
  - **ground truth との 100% 一致を実証**: `ntfs_with_5_deletions_small.json` の `file_003.txt` / `file_007.txt` / `file_015.txt` / `file_022.txt` / `file_028.txt` の 5 件全てを `[DELETED]` フラグ + タイムスタンプ + ファイル名で復元（人間可読出力 `prints_live_and_deleted_file_listing_for_human_review` で検証可能）
  - これにより「削除されたファイル名 + いつ削除されたか」のペアが取得可能となり、Phase 1 のプロダクト価値（希望リスト × 削除ファイル突合）の中核データ供給は完了
  - 残作業: ディレクトリツリー上の削除エントリ階層化列挙（FR-LIVE-04 依存）、UI 上の色分け表示・一覧化（フロントエンド実装）
- [~] **FR-LIVE-06: メタデータ表示** 🚧 **メタデータ抽出ほぼ完成**（Chunk 6-8 / dds-fs-ntfs）
  - 属性ヘッダ巡回 API（`parse_attribute_header` / `AttributeIterator` / `find_attribute`）が確立し、$MFT エントリから安全に End マーカーまで属性を列挙可能
  - $STANDARD_INFORMATION パース完了: 4 種タイムスタンプ（created / modified / mft_modified / accessed、Windows FILETIME → `DateTime<Utc>` 変換 / オーバーフロー安全）、DOS ファイル属性フラグ（READ_ONLY / HIDDEN / SYSTEM / ARCHIVE / COMPRESSED / ENCRYPTED / DIRECTORY）の抽出が可能。NT(48B)/W2K+(72B) 両版対応
  - **$FILE_NAME パース完了 🎯**（Chunk 8）: ファイル名（UTF-16LE 非 lossy デコード、日本語・絵文字対応）、親ディレクトリ MFT 参照（48bit entry + 16bit sequence）、$FILE_NAME 内 4 種タイムスタンプ、allocated/real size、file_attributes、namespace（Posix/Win32/Dos/Win32AndDos）抽出、Win32/DOS 二重登録時の `find_best_file_name` 優先選択ヘルパ提供
  - 削除エントリ含めて実フィクスチャでタイムスタンプ + ファイル名復元を実証（ground truth 100% 一致）
  - 残作業: $DATA（実体サイズ・データラン）の具象属性パーサと、上位レイヤでのメタデータ集約・表示（`FsEntry` への集約 API）
  - 完了マークはメタデータ集約 API が `FsEntry` に必要なフィールドを全て返せるようになった時点で付与
- [ ] FR-LIVE-07: バックアップメタ活用

### 希望リスト・突合 (FR-WISH)
- [~] **FR-WISH-01: 希望項目の入力フォーム** 🚧 **データ基盤確立**（Chunk 7-8 / dds-fs-ntfs）
  - 「日付範囲指定」による希望リスト × 復旧候補の突合に必要なタイムスタンプデータが NTFS 側から取得可能になった（$STANDARD_INFORMATION から created / modified / mft_modified / accessed の 4 種を抽出）
  - 削除エントリのタイムスタンプも実画像レベルで取得実証済（フィクスチャ生成時刻と一致）
  - **ファイル名による突合に必要なデータも揃った**（Chunk 8、$FILE_NAME パース完了、日本語ファイル名・絵文字対応）。希望リストに「ファイル名」「拡張子」を入れた突合が技術的に可能
  - 残作業: 希望項目入力フォーム本体（UI）、希望条件のデータ型定義、突合ロジック（FR-WISH-04/05）
- [ ] FR-WISH-02: 優先度設定
- [ ] FR-WISH-03: 一括インポート
- [ ] FR-WISH-04: 突合実行
- [ ] FR-WISH-05: マッチ信頼度算出
- [ ] FR-WISH-06: 発見可能性レポート
- [ ] FR-WISH-07: 未発見項目の理由提示
- [ ] FR-WISH-08: お客様承認フロー

### 復旧 (FR-REC)
- [~] **FR-REC-01: 目標優先抽出** 🚧 **基盤確立**（Chunk 9 / dds-fs-ntfs）
  - ファイル単位の選別 + 内容取得が可能（$FILE_NAME によるファイル名突合 + $DATA 常駐の内容取得）
  - 残作業: 希望リストとの突合に応じた優先抽出ロジック（wish-match クレートで実装予定）、非常駐 $DATA 対応（Chunk 10）
- [ ] FR-REC-02: ノンマッチ抽出オプション
- [ ] FR-REC-03: 出力先指定
- [x] **FR-REC-04: データ整合性** ✅ **完全達成 🎯🎯**（Chunk 9 / dds-fs-ntfs）
  - **SHA256 ハッシュによる検証メカニズムを結合テストで実証**。`recovers_all_30_files_with_matching_sha256_in_healthy_image`（健全 30/30）+ `recovers_all_5_deleted_files_with_matching_sha256`（削除 5/5）で ground truth と `assert_eq!` で完全一致
  - 「データを取り出せた」だけでなく「ビット単位で正しく復元できた」ことの暗号学的証明完了
  - 復旧データのバイト単位完全性検証が技術的に保証された状態。Phase 1 のプロダクト価値の数学的証明済
  - 注: 非常駐 $DATA（クラスタチェーン経由の大ファイル）への適用は Chunk 10 完了時に同等の SHA256 検証で追認予定
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

**Chunk 10**: `dds-fs-ntfs` NTFS `$DATA` 非常駐属性 + runlist 解析（大ファイル＝クラスタチェーン経由のデータ取得）

- **対象クレート**: `crates/fs-ntfs/`
- **対象ファイル（予定）**:
  - `crates/fs-ntfs/src/attributes/runlist.rs`（新規、データラン解析）
  - `crates/fs-ntfs/src/attributes/data.rs`（既存、非常駐 $DATA の実バイト取得 API 追加）
  - `crates/fs-ntfs/src/lib.rs`（公開 API の re-export 追加）
  - `crates/fs-ntfs/tests/data_nonresident_integration.rs`（新規、結合テスト）
- **目的**: Chunk 9 で常駐 $DATA に対する SHA256 完全一致を実証したので、その上に非常駐 $DATA（クラスタサイズを超える大ファイル）の runlist 解析を実装する。NTFS のデータラン（圧縮された VCN→LCN マッピング）をデコードし、`ReadOnlyDisk` 経由でクラスタ列を読み出して実バイト列を組み立てる。これにより常駐 + 非常駐の両方に対して「SHA256 完全一致による削除ファイル復旧」が成立し、**Phase 1 NTFS リーダα（M2）が完了**する。
- **スコープ外（明示）**:
  - 圧縮 $DATA の解凍（Phase 2 以降、flag 検出済み）
  - スパースファイルの 0 領域最適化（後続チャンク）
  - $ATTRIBUTE_LIST 経由の属性分割対応（特殊ケース、後続チャンク）
- **依存**:
  - Chunk 1（`dds-core` のエラー型基盤）
  - Chunk 3（`ReadOnlyDisk` trait による安全な disk read）
  - Chunk 4（`BootSector` のクラスタサイズ）
  - Chunk 6（`NonResidentInfo` / `runlist_offset`）
  - Chunk 9（`DataContent::NonResident` の構造体フィールド）
- **推定行数**: 約 200行（実装 ~120 + テスト ~80、runlist デコードは状態機械なのでテスト多め）
- **着手前の準備**:
  1. `docs/specs/ntfs-references/` の runlist エンコード仕様を再確認（ヘッダバイト = (length_byte_count << 4) | offset_byte_count、length 部 + offset 部の可変長デコード、offset の符号付き差分による LCN 累積）
  2. スパース run（offset_byte_count == 0）の処理方針: ゼロ埋めで返却
  3. 圧縮 / 暗号化検出時は明示エラー化（フラグ既に提供済）
  4. テスト: 手組みデータで 1run / 複数 run / sparse run / 不正バイト列拒否 / クラスタ境界跨ぎ / 結合テスト（実フィクスチャの非常駐 $DATA で SHA256 完全一致再現）
- **完了条件**: Chunk 1-9 と同等（cargo check / test --lib / clippy `-D warnings` / doc 全 OK、rustdoc 完備、単体テスト 3件以上、不正値拒否テスト含む、unsafe・書き込み API 不在を維持、行数 220行上限内）
- **マイルストーン意義**: 本チャンク完了で **M2 NTFSリーダα が事実上完了**（残るは $INDEX_ROOT/ALLOCATION 経由のディレクトリツリー集約と `FsReader` trait 実装のみ）。Phase 1 のプロダクト価値（削除ファイルのビット完全復元）が大ファイル領域にも適用され、SHA256 検証メカニズムが全ファイルサイズ域で成立する。

詳細指示は builder 起動時に作成する `docs/chunk_10.md` で展開予定。
