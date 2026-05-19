# DDS Recovery Workbench - 進捗トラッカー

このファイルは progress-tracker エージェントが自動更新します。

---

## 累積サマリ

- **完了チャンク数**: 7
- **総実装行数**: 1370（実装+テスト合計、各チャンク200行上限内）
- **総単体テスト数**: 48（全パス）
- **総結合テスト数**: 8（全パス、NTFSフィクスチャ実画像での Boot Sector + MFT エントリヘッダ + 属性ヘッダ巡回 + $STANDARD_INFORMATION タイムスタンプ復元まで実証）
- **平均カバレッジ**: 未計測（モジュール完成時に計測予定）
- **ハイライト**: **削除済みファイルのタイムスタンプ復元を実画像レベルで実証**（Chunk 7 結合テストで削除エントリ 13 件から $SI 取得成功、created = 2026-05-19T10:19:13Z がフィクスチャ生成時刻と一致）
- **最終更新日**: 2026-05-19

---

## マイルストーン進捗

```
M0: 設計確定        [████████] 100% ✅ 完了
M1: 基盤構築        [███░░░░░]  30% 🚧 進行中（Chunk 1-3/想定10前後 完了）
M2: NTFSリーダα     [████░░░░]  40% 🚧 進行中（Chunk 4: Boot Sector パーサ + Chunk 5: MFT エントリヘッダ + フィクサップ + Chunk 6: 属性ヘッダパーサ + Chunk 7: 属性イテレータ + $STANDARD_INFORMATION 完了）
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
- [~] **FR-LIVE-01: NTFS読み取り** 🚧 **部分着手**（Chunk 4-7 / dds-fs-ntfs）
  - Boot Sector (VBR) パーサ完了。OEM ID/シグネチャ検証、主要パラメータ抽出、MFT 開始オフセット算出が利用可能
  - MFT エントリヘッダパーサ + フィクサップ適用完了。`FILE`/`BAAD` 判定、USA 検証、フラグ抽出（in-use/directory）、レコード番号/シーケンス番号取得が利用可能
  - 属性ヘッダパーサ完了。共通ヘッダ抽出、Resident/NonResident 排他分岐、End マーカー検出、未知 type ID の前方互換受け入れ、0長拒否による安全な巡回基盤が利用可能。実フィクスチャで $STANDARD_INFORMATION / $FILE_NAME / $DATA / $BITMAP / End の昇順巡回を実証
  - **属性イテレータ + $STANDARD_INFORMATION 完了**。`AttributeIterator` で End まで安全に列挙、`find_attribute` ヘルパ、$SI から 4 種タイムスタンプ（created/modified/mft_modified/accessed）+ DOS 属性フラグ抽出、NT(48B)/W2K+(72B) 両版対応
  - 残作業: $FILE_NAME（Chunk 8）、$DATA、$INDEX_ROOT/ALLOCATION、ディレクトリツリー構築
  - 完了マークは `FsReader` trait の NTFS 実装が全要素を返せるようになった時点で付与
- [ ] FR-LIVE-02: exFAT読み取り
- [ ] FR-LIVE-03: FAT32読み取り
- [ ] FR-LIVE-04: ファイルツリー構築
- [~] **FR-LIVE-05: 削除エントリ可視化** 🚧 **部分着手**（Chunk 5 / dds-fs-ntfs）
  - MFT エントリ単位の削除判定 `is_deleted()` を提供（flags の in-use ビット非立で判定）
  - 結合テスト `counts_deleted_entries_in_deletions_fixture` で実 NTFS フィクスチャから削除エントリ ≥5 件検出を実証
  - 残作業: ディレクトリツリー上の削除エントリ列挙、UI 上の色分け表示・一覧化、削除済みファイル名の復元（$FILE_NAME 属性パース依存）
- [~] **FR-LIVE-06: メタデータ表示** 🚧 **タイムスタンプ取得完了**（Chunk 6-7 / dds-fs-ntfs）
  - 属性ヘッダ巡回 API（`parse_attribute_header` / `AttributeIterator` / `find_attribute`）が確立し、$MFT エントリから安全に End マーカーまで属性を列挙可能
  - **$STANDARD_INFORMATION パース完了**: 4 種タイムスタンプ（created / modified / mft_modified / accessed、Windows FILETIME → `DateTime<Utc>` 変換 / オーバーフロー安全）、DOS ファイル属性フラグ（READ_ONLY / HIDDEN / SYSTEM / ARCHIVE / COMPRESSED / ENCRYPTED / DIRECTORY）の抽出が可能。NT(48B)/W2K+(72B) 両版対応
  - **削除エントリ含めて実フィクスチャでタイムスタンプ復元を実証**（健全 27 件 + 削除 13 件）
  - 残作業: $FILE_NAME（ファイル名・親参照）、$DATA（実体サイズ・データラン）の具象属性パーサと、上位レイヤでのメタデータ集約・表示
  - 完了マークはメタデータ集約 API が `FsEntry` に必要なフィールドを全て返せるようになった時点で付与
- [ ] FR-LIVE-07: バックアップメタ活用

### 希望リスト・突合 (FR-WISH)
- [~] **FR-WISH-01: 希望項目の入力フォーム** 🚧 **データ基盤確立**（Chunk 7 / dds-fs-ntfs）
  - 「日付範囲指定」による希望リスト × 復旧候補の突合に必要なタイムスタンプデータが NTFS 側から取得可能になった（$STANDARD_INFORMATION から created / modified / mft_modified / accessed の 4 種を抽出）
  - 削除エントリのタイムスタンプも実画像レベルで取得実証済（フィクスチャ生成時刻と一致）
  - 残作業: 希望項目入力フォーム本体（UI）、希望条件のデータ型定義、突合ロジック（FR-WISH-04/05）
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

**Chunk 8**: `dds-fs-ntfs` NTFS `$FILE_NAME` 属性パーサ（ファイル名取得 — プロダクト価値の見える化マイルストーン）

- **対象クレート**: `crates/fs-ntfs/`
- **対象ファイル（予定）**:
  - `crates/fs-ntfs/src/attributes/file_name.rs`（新規、$FILE_NAME の具象パース）
  - `crates/fs-ntfs/src/attributes/mod.rs`（既存、re-export 追加）
  - `crates/fs-ntfs/src/lib.rs`（公開 API の re-export 追加）
  - `crates/fs-ntfs/tests/file_name_integration.rs`（新規、結合テスト）
- **目的**: Chunk 7 で確立した属性イテレータの上に、最重要の具象属性 $FILE_NAME（type ID 0x30、常駐属性）を実装する。ファイル名本体（UTF-16 LE エンコード）、親ディレクトリへの MFT 参照（48ビット record number + 16ビット sequence number）、ファイル名種別（POSIX / Win32 / DOS / Win32&DOS）、$SI 同等の 4 種タイムスタンプ（$FILE_NAME 内にも別途格納されている）、論理サイズ / 物理サイズ、ファイル属性フラグを抽出。これにより「いつ削除されたか・どのファイル名だったか」の中核ペアが揃い、Phase 1 のプロダクト価値（希望リスト駆動型復旧）の見える化に到達。
- **依存**:
  - Chunk 1（`dds-core` のエラー型基盤）
  - Chunk 5（`MftEntry` / `MftEntryHeader`）
  - Chunk 6（`AttributeHeader` / `AttributeType::FileName` / `ResidentInfo`）
  - Chunk 7（`AttributeIterator` / `find_attribute` / `FileTime` / `FileAttributes`）
- **推定行数**: 約 150〜180行（実装 ~80〜110 + テスト ~50〜70）
- **着手前の準備**:
  1. `docs/specs/ntfs-references/` の $FILE_NAME 仕様を再確認（固定部 66バイト + 可変長ファイル名 UTF-16 LE、parent reference のエンコード、name type 列挙値）
  2. UTF-16 LE → String 変換の方針整理（不正サロゲートペアのハンドリング、`String::from_utf16` / `from_utf16_lossy` の選択）
  3. 親ディレクトリ参照（u64 のうち下位 48bit が record number、上位 16bit が sequence number）の分解ロジック
  4. ファイル名種別 enum 定義（POSIX=0 / Win32=1 / DOS=2 / Win32AndDos=3）
  5. テスト: 手組みデータで ASCII / UTF-16（日本語含む）/ 4 種 name type / 不正長拒否 / parent reference 分解 / 結合テスト（実フィクスチャから $FILE_NAME 取得、ground truth JSON のファイル名と照合）
- **完了条件**: Chunk 1-7 と同等（cargo check / test --lib / clippy `-D warnings` / doc 全 OK、rustdoc 完備、単体テスト 3件以上、不正値拒否テスト含む、unsafe・書き込み API 不在を維持）
- **マイルストーン意義**: 本チャンク完了時点で「削除エントリの created タイムスタンプ + ファイル名」のペアが取得可能になり、FR-WISH-01「日付範囲指定」とファイル名検索の両軸での突合データが揃う。M2 NTFSリーダα の見える化マイルストーン。

詳細指示は builder 起動時に作成する `docs/chunk_8.md` で展開予定。
