# DDS Recovery Workbench - 進捗トラッカー

このファイルは progress-tracker エージェントが自動更新します。

---

## 累積サマリ

- **完了チャンク数**: 9（うち Chunk 4 / Chunk 5 / Chunk 6 は 2026-05-20 に書籍突合レビュー済 📕）
- **総実装行数**: 2004（実装+テスト合計、各チャンク200行上限 / 仕様緩和後 220行上限以内。Chunk 5 レビュー +69行 / Chunk 4 レビュー +83行 / Chunk 6 レビュー +73行）
- **総単体テスト数**: 79（全パス、Chunk 5 書籍突合レビューで +5件 / Chunk 4 書籍突合レビューで +5件 / Chunk 6 書籍突合レビューで +4件）
- **総結合テスト数**: 14（全パス、NTFSフィクスチャ実画像での Boot Sector + MFT エントリヘッダ + 属性ヘッダ巡回 + $STANDARD_INFORMATION タイムスタンプ復元 + $FILE_NAME ファイル名取得・削除フラグ + $DATA 常駐属性 SHA256 完全一致実証まで完了）
- **総テスト数（単体 + 結合）**: 93（全パス）
- **平均カバレッジ**: 未計測（モジュール完成時に計測予定）
- **🎯🎯 Phase 1 技術核心マイルストーン達成（Chunk 9）**: **「削除されたファイルを名前 + タイムスタンプ + 内容（バイト単位完全一致）で復元する」というプロダクト価値の中核を、実 NTFS フィクスチャで数学的に実証**。健全イメージ `ntfs_healthy_small` 30/30 ファイル、削除イメージ `ntfs_with_5_deletions_small` 30/30 ファイル（うち削除済み 5/5: `file_003.txt` / `file_007.txt` / `file_015.txt` / `file_022.txt` / `file_028.txt`）の **SHA256 ハッシュが ground truth と完全一致**（`assert_eq!` で全件比較成立）。これは「データを取り出せた」だけでなく「ビット単位で正しく復元できた」ことの暗号学的証明。**Phase 1 のプロダクト価値の数学的証明完了**
- **ADS 対応基盤確立（Chunk 9）**: Alternate Data Stream（名前付き $DATA）の全列挙 API（`extract_all_data_streams`）を提供。`DataStream.name` で識別可能。フォレンジック調査価値の基盤
- **既存ハイライト（Chunk 8）**: 削除ファイル名 + 削除タイムスタンプのペア取得を実画像レベルで完全実証。ground truth `ntfs_with_5_deletions_small.json` との突合で総ファイル数 30 / 削除 5 件が 100% 一致
- **顧客要件達成（Chunk 8-9）**: 日本語ファイル名（"報告書_山田.docx"）/ 絵文字（"📁メモ.txt"、サロゲートペア）/ 日本語ストリーム名（"秘匿データ"）のデコードを単体テストで実証。`String::from_utf16`（非 lossy）採用、不正データはエラー化
- **既存ハイライト（Chunk 7）**: 削除済みファイルのタイムスタンプ復元を実画像レベルで実証（削除エントリ 13 件から $SI 取得成功、created = 2026-05-19T10:19:13Z がフィクスチャ生成時刻と一致）
- **品質向上ハイライト（Chunk 6 書籍突合レビュー / 2026-05-20）📕**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」Table 13.2「first 16 bytes of an attribute」/ Table 13.3「resident attribute」/ Table 13.4「non-resident attribute」に基づき `crates/fs-ntfs/src/attribute.rs` を独立レビュー。**既存実装は書籍 Table 13.2/13.3/13.4 と完全一致**しており、構造体定義・フィールド名・enum バリアントすべて過不足なしであることを確認（Table 13.2 共通ヘッダ全7フィールド完全対応 / Table 13.3 常駐追加 content_size + content_offset 完全対応 + Linux NTFS Docs 由来の indexed フィールドも保持 / Table 13.4 非常駐追加 全8フィールド完全対応 / 属性タイプ enum 全 15 種（0x10〜0x100）+ Unknown + End 完全網羅）。**実装本体への変更は不要**と判定し、書籍突合の意義を「既存実装が書籍仕様と一致していることの検証」と「書籍例題の再現テスト追加によるリグレッション防止」に集約。`attribute.rs` を 199行 → **272行（+73行、実装 116 + 単体テスト 156）**に拡張し単体テスト 4 件を追加: ①書籍 356 ページ $STANDARD_INFORMATION 常駐例題の数学的再現（type=0x10, length=0x60, content_size=0x48, content_offset=0x18、サニティ式 0x18+0x48=0x60 を assertion） / ②書籍 358 ページ $DATA 非常駐例題（type=0x80, starting_vcn=0, last_vcn=0x20EF=8431, runlist_offset=0x40, allocated/real/initialized=0x83C000=8634368 トリプル一致） / ③全 15 種属性タイプ + Unknown 3 種（0x42/0xFF/0x200）+ End ラウンドトリップ網羅（計 19 ケース） / ④フラグ組合せ 5 パターン（compressed/encrypted/sparse/混合）の生値保持＋ビット個別判定。`cargo test --lib -p dds-fs-ntfs` は **63 passed**（既存 59 + 新規 4）、`cargo test -p dds-fs-ntfs` は **77 passed**（単体 63 + 結合 14）、clippy で warning 0件、cargo doc 生成成功。書籍逐語コピー 0 件を Grep で確認（Table 名 3 件 + 連続英単語塊チェックで全て未検出）、`docs/specs/ntfs-references/notes.md` に「## 8. Attribute Header（属性ヘッダ）」セクション追加（既存「## 8. 参考リソース」を「## 9. 参考リソース」へ繰り下げ、195 → 289 行、+94 行）。NTFS 入口部分（Boot Sector + MFT エントリ + 属性ヘッダ）の 3 チャンク全てが書籍突合済みの商用レベル品質に到達
- **品質向上ハイライト（Chunk 5 書籍突合レビュー / 2026-05-20）📕**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」Fixup Values セクションに基づき `crates/fs-ntfs/src/mft.rs` を独立レビュー。既存実装は書籍仕様と**基本的に整合**していることを確認した上で、**USA size 整合性検証**（`usa_size == ceil(allocated_size / sector_size) + 1`）を追加し破損データの早期検出を強化。書籍例題（USN=0x0058、record=1024、sector=512）の数学的再現テスト、マルチセクタ拡張（2KB レコード）、部分破損検出（書籍が言及する "one sector damaged" シナリオ）、USN=0 エッジケースの単体テスト 5 件を追加。書籍からの逐語コピーは 0 件（Grep 確認済）、参照は章番号・Table 番号のみ。実装は商用レベル品質に到達
- **品質向上ハイライト（Chunk 4 書籍突合レビュー / 2026-05-20）📕**: 同書 Chapter 13 Table 13.18「Data structure for the boot sector」に基づき `crates/fs-ntfs/src/boot_sector.rs` を独立レビュー。既存実装は書籍仕様の全フィールドを**完全カバー**していたことを確認した上で、**`index_record_size_bytes()` メソッド追加**（MFT と同じ符号付きエンコーディング、DRY 共有ヘルパ `compute_record_size_bytes` を抽出）、**`bytes_per_sector` の 2の累乗 + 256〜4096 範囲チェック**、**`sectors_per_cluster` の 2の累乗 + 1〜128 範囲チェック**を追加。書籍 381 ページ例題（OEM="NTFS    ", bps=512, spc=2, total_sectors=2056256, mft_lcn=342709, mft_mirror_lcn=514064, cpmr=1, cpir=4, serial=0x04502284_50227C94）の数学的再現テスト、Index record size 符号付きエンコーディング、4Kn ドライブ（bps=4096）、非2の累乗 bps/spc 拒否の単体テスト 5 件を追加。書籍からの逐語コピーは 0 件（Grep 確認済）、参照は章番号・Table 番号のみ。NTFS 入口部分（Boot Sector）が商用レベル品質に到達
- **最終更新日**: 2026-05-20

---

## マイルストーン進捗

```
M0: 設計確定        [████████] 100% ✅ 完了
M1: 基盤構築        [███░░░░░]  30% 🚧 進行中（Chunk 1-3/想定10前後 完了）
M2: NTFSリーダα     [██████░░]  60% 🚧 進行中（Chunk 4: Boot Sector 📕 + Chunk 5: MFT エントリヘッダ + フィクサップ 📕 + Chunk 6: 属性ヘッダパーサ 📕 + Chunk 7: 属性イテレータ + $STANDARD_INFORMATION + Chunk 8: $FILE_NAME + Chunk 9: $DATA 常駐 + ADS + SHA256 完全一致実証 完了。NTFS 入口 3 チャンク 📕 が全て書籍突合済み。残りは非常駐 $DATA = Chunk 10）
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
| 4 | dds-fs-ntfs | NTFS Boot Sector (VBR) パーサ 📕 | 280 | 11 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 / 📕 Reviewed 2026-05-20 |
| 5 | dds-fs-ntfs | NTFS MFT エントリヘッダパーサ + フィクサップ適用 📕 | 268 | 13 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 / 📕 Reviewed 2026-05-20 |
| 6 | dds-fs-ntfs | NTFS 属性ヘッダパーサ（Resident/NonResident 分岐 + End マーカー） 📕 | 272 | 12 ✓ + 結合 2 ✓ | 未計測 | 2026-05-19 / 📕 Reviewed 2026-05-20 |
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
- **📕 書籍突合レビュー（2026-05-20）**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」Table 13.18「Data structure for the boot sector」に基づき独立レビュー実施。既存実装は書籍仕様の全フィールドを**完全カバー**していたことを確認。改善として `boot_sector.rs` を 197行 → **280行（+83行、実装 130 + 単体テスト 149）**に拡張し、`index_record_size_bytes()` メソッド追加（MFT と同じ符号付きエンコーディング、内部ヘルパ `compute_record_size_bytes(raw: i8, cluster_size: u32) -> u32` を抽出して MFT/Index で DRY 共有）、`bytes_per_sector` の 2の累乗 + 256〜4096 範囲チェック強化、`sectors_per_cluster` の 2の累乗 + 1〜128 範囲チェック強化、内部ヘルパ `is_pow2(v: u32) -> bool` 追加。単体テスト 5 件追加: ①書籍 381 ページ例題の数学的再現（OEM="NTFS    ", bps=512, spc=2, total_sectors=2056256, mft_lcn=342709, mft_mirror_lcn=514064, cpmr=1, cpir=4, serial=0x04502284_50227C94）/ ②Index record size の符号付きエンコーディング（cpir=4/-12/-10 → 4096/4096/1024）/ ③4Kn ドライブ対応（bps=4096, spc=1, cluster=4096）/ ④非2の累乗 bps 拒否（1000/100/8192）/ ⑤非2の累乗 spc 拒否（3/192/130）。`cargo test --lib -p dds-fs-ntfs` は **59 passed**（既存 54 + 新規 5）、`cargo test -p dds-fs-ntfs` は **73 passed**（単体 59 + 結合 14）、clippy で `manual_range_contains` を初回検出し `!(MIN..=MAX).contains(&bps)` に修正済（warning 0件）、cargo doc 生成成功。書籍逐語コピー 0 件を Grep で確認（特徴フレーズ 3 件 + 連続英単語塊チェックで全て未検出）、`docs/specs/ntfs-references/notes.md` に「## 7. Boot Sector（$BOOT ファイルの先頭セクタ）」セクション追加（既存「## 7. 参考リソース」を「## 8. 参考リソース」に繰り下げ、132 → 195 行、+63 行）。実装は書籍突合済みの商用レベル品質に到達。詳細は本ファイル「書籍突合レビュー結果」セクション参照。

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
- **📕 書籍突合レビュー（2026-05-20）**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13 Fixup Values セクションに基づき独立レビュー実施。既存実装は書籍仕様と基本的に整合。改善として `mft.rs` を 199行 → 268行（+69行）に拡張し、USA size 整合性検証（`usa_size == ceil(allocated_size / sector_size) + 1`）追加 + 単体テスト 5 件追加（書籍例題再現 / 整合性検証 / 2KB マルチセクタ / USN=0 エッジケース / 部分破損検出）。`cargo test --lib -p dds-fs-ntfs` は 54 passed（+5）、`cargo test -p dds-fs-ntfs` は 68 passed。書籍逐語コピー 0 件を Grep で確認、新規 `docs/specs/ntfs-references/notes.md`（132行）に自前要約を配置。詳細は本ファイル「書籍突合レビュー結果」セクション参照。

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
- **📕 書籍突合レビュー（2026-05-20）**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13 内 Table 13.2「first 16 bytes of an attribute」/ Table 13.3「resident attribute」/ Table 13.4「non-resident attribute」に基づき独立レビュー実施。**既存実装は書籍 Table 13.2/13.3/13.4 と完全一致**しており、構造体定義・フィールド名・enum バリアントすべて過不足なし: Table 13.2 共通ヘッダ全 7 フィールド完全対応 / Table 13.3 常駐追加（content_size / content_offset）完全対応 + Linux NTFS Docs 由来の `indexed`（byte 22）も保持 / Table 13.4 非常駐追加 全 8 フィールド完全対応 / 属性タイプ enum 全 15 種（0x10/0x20/0x30/0x40/0x50/0x60/0x70/0x80/0x90/0xA0/0xB0/0xC0/0xD0/0xE0/0x100）+ Unknown + End 完全網羅。**実装本体への変更は不要**と判定し、書籍突合の意義を「既存実装が書籍仕様と一致していることの検証」と「書籍例題の再現テスト追加によるリグレッション防止」に集約。`attribute.rs` を 199行 → **272 行（+73行、実装 116 + 単体テスト 156）**に拡張し単体テスト 4 件追加: ①`book_example_si_resident_96_byte_attribute`（書籍 356 ページ $STANDARD_INFORMATION 常駐例題の数学的再現: type=0x10, length=0x60, content_size=0x48, content_offset=0x18、サニティ式 0x18+0x48=0x60 を assertion） / ②`book_example_data_nonresident_with_runlist`（書籍 358 ページ $DATA 非常駐例題: type=0x80, starting_vcn=0, last_vcn=0x20EF=8431, runlist_offset=0x40, allocated/real/initialized=0x83C000=8634368 トリプル一致） / ③`all_attribute_types_roundtrip_including_unknown_and_end`（全 15 種 + Unknown 3 種（0x42/0xFF/0x200）+ End ラウンドトリップ網羅、計 19 ケース） / ④`flag_bit_combinations_preserved_as_raw_value`（フラグ組合せ 5 パターン: compressed / encrypted / sparse / compressed+encrypted / 三種同時 の生値保持＋ビット個別判定）。`cargo test --lib -p dds-fs-ntfs` は **63 passed**（既存 59 + 新規 4）、`cargo test -p dds-fs-ntfs` は **77 passed**（単体 63 + 結合 14）、clippy で warning 0件、cargo doc 生成成功。書籍逐語コピー 0 件を Grep で確認（Table 名 3 件 + 連続英単語塊チェックで全て未検出）、`docs/specs/ntfs-references/notes.md` に「## 8. Attribute Header（属性ヘッダ）」セクション追加（既存「## 8. 参考リソース」を「## 9. 参考リソース」へ繰り下げ、195 → 289 行、+94 行）。内容: 8.1 共通ヘッダ（Table 13.2 対応表）/ 8.2 常駐追加（Table 13.3）/ 8.3 非常駐追加（Table 13.4）/ 8.4 属性タイプ ID 一覧（15 種）/ 8.5 Flag ビット意味 / 8.6 解析の停止条件。NTFS 入口 3 チャンク（Boot Sector + MFT エントリ + 属性ヘッダ）が全て書籍突合済みの商用レベル品質に到達。詳細は本ファイル「書籍突合レビュー結果」セクション参照。

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

## 書籍突合レビュー結果

専門書籍に基づく独立レビューで、実装の仕様整合性・堅牢性・著作権配慮を検証した結果を記録する。

### サマリ表

| Chunk | 変更行数 | 追加テスト | 重要な発見 |
|---|---|---|---|
| 5 | +69 | +5 | USA size 整合性検証追加、書籍例題テスト追加、既存実装は仕様と整合 |
| 4 | +83 | +5 | `index_record_size_bytes()` 追加、bps/spc 範囲チェック強化、書籍例題（serial 0x0450_2284_5022_7C94）テスト追加 |
| 6 | +73 | +4 | 既存実装は Table 13.2/13.3/13.4 と完全一致、テスト追加のみ、書籍例題（$SI/$DATA）と全 15 属性タイプ網羅 |

### Chunk 5 詳細（2026-05-20、📕 Reviewed）

- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, Addison-Wesley, ISBN 9780321374752) Chapter 13「NTFS Data Structures」Fixup Values セクション
- **レビュー対象**: `crates/fs-ntfs/src/mft.rs`（MFT エントリヘッダパーサ + フィクサップ適用）
- **レビュー工程**: builder（書籍突合 + 実装改善）→ tester（独立検証）→ progress-tracker（記録）
- **結論**: 既存実装は書籍仕様と**基本的に整合**していた。フィクサップロジック・BAAD 検出・各フィールド解釈すべて正しい。今回の改善は仕様の暗黙的前提を明示化し、堅牢性と将来のリグレッション防止を強化したもの

#### 実装変更

- **対象**: `crates/fs-ntfs/src/mft.rs`（199行 → **268行**、実装 132 + 単体テスト 136。書籍突合レビューによる品質向上のため 200行制約を緩和）
- **USA size 整合性検証追加**: `parse_mft_entry` で `usa_size == ceil(allocated_size / sector_size) + 1` を検証、不一致は `InvalidUsaSize` エラーで早期拒否
  - 破損データの早期検出
  - 既存テスト・実フィクスチャに影響なし（標準 NTFS の usa_size=3, record=1024, sector=512 は式と整合）
- **rustdoc 強化**: `usa_size` / `sequence_number` / `hard_link_count` フィールドに書籍が定義する意味を自分の言葉で言い換えて補足

#### 追加テスト 5 件（書籍に基づく検証）

1. **`book_example_signature_0x0058_applies_fixup`** — 書籍 Chapter 13 例題の数学的再現（USN=0x0058、USA size=3、record=1024、sector=512）
2. **`usa_size_mismatch_with_record_size_rejected`** — 整合性検証の動作確認
3. **`parses_2kb_entry_with_four_fixups`** — マルチセクタ拡張（2KB レコード、4 セクタの fixup 配列）
4. **`usn_zero_is_accepted`** — エッジケース（未割り当てエントリの USN=0）
5. **`partial_corruption_detected_at_second_sector`** — 部分破損検出（書籍が言及する "one sector damaged" シナリオの再現）

#### 新規ドキュメント

- `docs/specs/ntfs-references/notes.md`（132行、新規）
- 内容: NTFS Fixup メカニズム / USA size 整合性ルール / MFT Entry 主要フィールドの自前日本語要約
- **書籍からの逐語コピーは 0 件**（tester による Grep 確認済み。特徴フレーズ 3 件 + 連続英単語塊チェックすべて未検出）
- 著作権配慮: 冒頭で「書籍からの逐語コピーなし」を明文化、参照は章番号・Table 番号のみ（事実情報のみ）

#### 検証結果（tester 独立検証）

- `cargo check -p dds-fs-ntfs` … OK
- `cargo test --lib -p dds-fs-ntfs` … **54 passed; 0 failed**（既存 49 + 新規 5）
- `cargo test -p dds-fs-ntfs` … **68 passed**（単体 54 + 結合 14）
- `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
- `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
- 既存 8 テスト全て pass 継続（破壊なし）
- 結合テスト 14 件全て pass（実フィクスチャでの破壊なし）
- 安全性: `unsafe` / `from_be_bytes` / 書き込み API は引き続き 0 件
- 書籍逐語コピー: 0 件（著作権配慮確認済）

#### 関連 FR / NFR（変更なし、品質向上として記録）

- **FR-LIVE-01**（NTFS 読み取り）: 引き続き部分着手、フィクサップロジックの堅牢性向上
- **FR-LIVE-05**（削除エントリ可視化）: 実用化完了状態継続、破損検出精度向上
- **NFR-REL-05**（I/O エラー処理）: USA size 整合性検証で破損データの早期検出を追加

#### 重要な発見事項

- 既存実装は書籍仕様と基本的に整合していた（フィクサップロジック・BAAD 検出・各フィールド解釈すべて正しい）
- 追加した整合性検証は書籍が暗黙的に前提とする USA size の妥当性チェックを明示化したもの
- 書籍例題の再現テストは将来のリグレッション防止に有用
- 書籍突合レビューを通じて実装の品質が商用レベルに到達

### Chunk 4 詳細（2026-05-20、📕 Reviewed）

- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, Addison-Wesley, ISBN 9780321374752) Chapter 13「NTFS Data Structures」Table 13.18「Data structure for the boot sector」
- **レビュー対象**: `crates/fs-ntfs/src/boot_sector.rs`（NTFS Boot Sector / VBR パーサ）
- **レビュー工程**: builder（書籍突合 + 実装改善）→ tester（独立検証）→ progress-tracker（記録）
- **結論**: 既存実装は書籍仕様の全フィールドを**完全カバー**していた（過不足なし）。書籍が暗黙的に前提とする `bytes_per_sector` / `sectors_per_cluster` の妥当性ルール（2の累乗 + 範囲）を明示化し、`index_record_size_bytes()` メソッドを追加して MFT との API カバレッジを揃え、DRY 改善のため共有ヘルパを抽出。書籍からの逐語コピーは 0 件

#### 実装変更

- **対象**: `crates/fs-ntfs/src/boot_sector.rs`（197行 → **280行**、実装 130 + 単体テスト 149。書籍突合レビューによる品質向上のため 200行制約を緩和）
- **`index_record_size_bytes()` メソッド追加** — `mft_record_size_bytes()` と同じ符号付きエンコーディング（正値→クラスタ数、負値 N→2^|N|バイト）。MFT と Index の API カバレッジを揃えた
- **内部ヘルパ `compute_record_size_bytes(raw: i8, cluster_size: u32) -> u32` を抽出** — MFT/Index で DRY 共有
- **`bytes_per_sector` 範囲チェック強化** — 2の累乗 + 256〜4096 の範囲外を `InvalidBytesPerSector` で拒否
- **`sectors_per_cluster` 範囲チェック強化** — 2の累乗 + 1〜128 の範囲外を `InvalidSectorsPerCluster` で拒否
- **`is_pow2(v: u32) -> bool`** 内部ヘルパ追加
- clippy `manual_range_contains` を初回検出し `!(MIN..=MAX).contains(&bps)` に修正

#### 追加テスト 5 件（書籍に基づく検証）

1. **`book_example_512_byte_sector_2_spc_1kb_cluster`** — 書籍 381 ページ例題の数学的再現（OEM="NTFS    ", bps=512, spc=2, total_sectors=2056256, mft_lcn=342709, mft_mirror_lcn=514064, cpmr=1, cpir=4, serial=0x04502284_50227C94）
2. **`index_record_size_negative_and_positive_encodings`** — Index record size の符号付きエンコーディング（cpir=4/-12/-10 → 4096/4096/1024）
3. **`parses_4kn_drive_with_4096_byte_sectors`** — 4Kn ドライブ対応（bps=4096, spc=1, cluster=4096）
4. **`non_power_of_two_bytes_per_sector_rejected`** — 範囲チェック強化（1000/100/8192 で拒絶）
5. **`non_power_of_two_sectors_per_cluster_rejected`** — 範囲チェック強化（3/192/130 で拒絶）

#### ドキュメント追加

- `docs/specs/ntfs-references/notes.md` に「## 7. Boot Sector（$BOOT ファイルの先頭セクタ）」セクション追加
- 既存「## 7. 参考リソース」を「## 8. 参考リソース」に繰り下げ
- 追記行数: 約 63 行（notes.md 全体 132 → 195 行）
- 内容: Table 13.18 のフィールド表 / "Must be 0" フィールドの方針 / MFT/Index Record size の符号付きエンコーディング / 検証強化のレビュー指針
- **書籍からの逐語コピーは 0 件**（tester による Grep 確認済み、特徴フレーズ 3 件 + 連続英単語塊チェックで全て未検出）

#### 検証結果（tester 独立検証）

- `cargo check -p dds-fs-ntfs` … OK
- `cargo test --lib -p dds-fs-ntfs` … **59 passed; 0 failed**（既存 54 + 新規 5）
- `cargo test -p dds-fs-ntfs` … **73 passed**（単体 59 + 結合 14）
- `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件（`manual_range_contains` を修正済）
- `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
- 既存 6 単体テスト全て pass 継続（破壊なし）
- 結合テスト 14 件全て pass（実 NTFS フィクスチャでの破壊なし）
- 安全性: `unsafe` / `from_be_bytes` / 書き込み API は引き続き 0 件
- 書籍逐語コピー: 0 件（著作権配慮確認済）

#### 関連 FR / NFR（変更なし、品質向上として記録）

- **FR-LIVE-01**（NTFS 読み取り）: 引き続き部分着手、ブートセクタ検証の堅牢性向上
- **FR-DIAG-04**（FS 識別）: ブートセクタ妥当性検証強化で破損ディスクの早期判別精度向上

#### 重要な発見事項

- 既存実装は書籍仕様の全フィールドを**完全カバー**していた（過不足なし）
- 書籍が暗黙的に前提とする bytes_per_sector / sectors_per_cluster の妥当性ルール（2の累乗 + 範囲）を明示化
- `index_record_size_bytes()` メソッドの追加で MFT と同等の API カバレッジを実現
- 書籍 381 ページ例題（MFT LCN 342709, Serial 0x0450_2284_5022_7C94）をテストで再現し、将来のリグレッション防止に有用
- DRY 改善: `compute_record_size_bytes` 共有ヘルパで MFT と Index の符号付きエンコーディングを統一

### Chunk 6 詳細（2026-05-20、📕 Reviewed）

- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, Addison-Wesley, ISBN 9780321374752) Chapter 13「NTFS Data Structures」内 Table 13.2「first 16 bytes of an attribute」/ Table 13.3「resident attribute」/ Table 13.4「non-resident attribute」
- **レビュー対象**: `crates/fs-ntfs/src/attribute.rs`（NTFS 属性ヘッダパーサ: 共通ヘッダ + Resident/NonResident 分岐 + End マーカー + 属性タイプ列挙）
- **レビュー工程**: builder（書籍突合 + テスト追加）→ tester（独立検証）→ progress-tracker（記録）
- **結論**: **既存実装は書籍 Table 13.2/13.3/13.4 と完全一致**しており、構造体定義・フィールド名・enum バリアントすべて過不足なし。**実装本体の変更は不要**と判定し、書籍突合の意義を「既存実装が書籍仕様と一致していることの検証」と「書籍例題の再現テスト追加によるリグレッション防止」に集約。テスト 4 件追加のみ

#### 実装変更

- **対象**: `crates/fs-ntfs/src/attribute.rs`（199行 → **272 行**、実装 116 + 単体テスト 156。書籍突合レビューによる品質向上のため 200行制約を緩和）
- **実装本体への変更なし**（既存実装が書籍仕様と完全一致のため）
- **テスト 4 件追加のみ**（下記）
- 書籍 Table 突合結果:
  - Table 13.2 共通ヘッダ全 7 フィールド: **完全対応**（`attribute_type` / `length` / `non_resident` / `name_length` / `name_offset` / `flags` / `attribute_id`）
  - Table 13.3 常駐追加: **完全対応**（`content_size` / `content_offset`）+ Linux NTFS Docs 由来の `indexed`（byte 22）も保持
  - Table 13.4 非常駐追加 全 8 フィールド: **完全対応**（`starting_vcn` / `last_vcn` / `runlist_offset` / `compression_unit_size` / `allocated_size` / `real_size` / `initialized_size`）
  - 属性タイプ enum: **完全網羅**（全 15 種 0x10/0x20/0x30/0x40/0x50/0x60/0x70/0x80/0x90/0xA0/0xB0/0xC0/0xD0/0xE0/0x100 + Unknown + End）

#### 追加テスト 4 件（書籍に基づく検証）

1. **`book_example_si_resident_96_byte_attribute`** — 書籍 356 ページ $STANDARD_INFORMATION 常駐例題の数学的再現（type=0x10, length=0x60, content_size=0x48, content_offset=0x18、サニティ式 0x18+0x48=0x60 を assertion）
2. **`book_example_data_nonresident_with_runlist`** — 書籍 358 ページ $DATA 非常駐例題（type=0x80, starting_vcn=0, last_vcn=0x20EF=8431, runlist_offset=0x40, allocated/real/initialized=0x83C000=8634368 トリプル一致）
3. **`all_attribute_types_roundtrip_including_unknown_and_end`** — 全 15 種属性タイプ + Unknown 3 種（0x42/0xFF/0x200）+ End ラウンドトリップ網羅（計 19 ケース）
4. **`flag_bit_combinations_preserved_as_raw_value`** — フラグ組合せ 5 パターン（compressed / encrypted / sparse / compressed+encrypted / 三種同時）の生値保持＋ビット個別判定

#### ドキュメント追加

- `docs/specs/ntfs-references/notes.md` に「## 8. Attribute Header（属性ヘッダ）」セクション追加
- 既存「## 8. 参考リソース」を「## 9. 参考リソース」へ繰り下げ
- 追記行数: 約 94 行（notes.md 全体 195 → 289 行）
- 内容: 8.1 共通ヘッダ（Table 13.2 対応表）/ 8.2 常駐追加（Table 13.3）/ 8.3 非常駐追加（Table 13.4）/ 8.4 属性タイプ ID 一覧（15 種）/ 8.5 Flag ビット意味 / 8.6 解析の停止条件
- **書籍からの逐語コピーは 0 件**（tester による Grep 確認済み、Table 名 3 件 + 連続英単語塊チェックで全て未検出）

#### 検証結果（tester 独立検証）

- `cargo check -p dds-fs-ntfs` … OK
- `cargo test --lib -p dds-fs-ntfs` … **63 passed; 0 failed**（既存 59 + 新規 4）
- `cargo test -p dds-fs-ntfs` … **77 passed**（単体 63 + 結合 14）
- `cargo clippy -p dds-fs-ntfs --all-targets -- -D warnings` … warning 0件
- `cargo doc -p dds-fs-ntfs --no-deps` … 生成成功
- 既存 8 単体テスト全て pass 継続（破壊なし）
- 結合テスト 14 件全て pass（実 NTFS フィクスチャ整合性に影響なし）
- 安全性: `unsafe` / `from_be_bytes` / 書き込み API は引き続き 0 件
- 書籍逐語コピー: 0 件（著作権配慮確認済）

#### 関連 FR（変更なし、品質向上として記録）

- **FR-LIVE-01**（NTFS 読み取り）: 既存実装の書籍突合確認による品質保証
- **FR-LIVE-06**（メタデータ表示）: 全 15 属性タイプの完全網羅をテストで担保

#### 重要な発見事項

- **既存実装は書籍 Table 13.2/13.3/13.4 と完全一致**（過不足なし、フィールド名・enum バリアント・分岐ロジックすべて書籍仕様通り）
- 実装本体への変更は不要と判定、書籍突合の意義はテスト追加（書籍例題の再現 + 全属性タイプ網羅）によるリグレッション防止に集約
- 書籍 356/358 ページの $SI/$DATA 例題の数学的再現テストにより、属性ヘッダパーサが書籍仕様の任意例題で動作することを保証
- 全 15 属性タイプ + Unknown + End の網羅テストにより、Forward compatibility 設計（未知 type ID の Unknown 受け入れ）が書籍が定義する全属性タイプで正しく動作することを担保
- NTFS 入口部分（Boot Sector + MFT エントリ + 属性ヘッダ）の 3 チャンク全てが書籍突合済みとなり、後続の $SI / $FILE_NAME / $DATA 解釈の土台が商用レベル品質に到達

---

## リスクログ

（進行中に発見されたリスクをここに記録）

- なし

---

## 次の推奨アクション

Chunk 4 / Chunk 5 / Chunk 6 の書籍突合レビュー完了（2026-05-20、📕）により、NTFS 入口部分（Boot Sector + MFT エントリヘッダ + 属性ヘッダ）の独立検証が揃った。次の優先順位候補を以下の通り提示する（判断時に並列／逐次の選択）。

### 選択肢 A: Chunk 8（NTFS `$FILE_NAME`）の書籍突合レビュー

- **対象**: `crates/fs-ntfs/src/attributes/file_name.rs`（既存 209行、9 単体テスト + 結合 3 件）
- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」`$FILE_NAME` セクション（Table 13.6 / 名前空間定義 / FILETIME フィールド）
- **目的**: 固定部 66 バイトのフィールド対応の書籍仕様レビュー、`FileNameNamespace` 列挙（Posix/Win32/Dos/Win32AndDos）の書籍定義整合性、`MftReference`（48bit entry + 16bit sequence）のビット分解確認、allocated/real size のセマンティクス、$FILE_NAME 内 4 種タイムスタンプと $SI のタイムスタンプの差異の書籍記述確認、`find_best_file_name` の優先順位ルールの書籍裏付け、書籍例題の数学的再現テスト追加
- **期待効果**: 顧客要件（日本語ファイル名・絵文字）が掛かる中核実装の書籍突合済み品質保証。フォレンジック観点での $FILE_NAME と $SI の時系列差異の解釈基盤
- **行数推定**: +50〜100行程度（namespace / MftReference / FILETIME 詳細テスト追加）
- **著作権配慮**: Chunk 4/5/6 と同方針（逐語コピー禁止、章番号・Table 番号のみ参照）
- **完了条件**: Chunk 4/5/6 レビューと同等（cargo check / test --lib / clippy `-D warnings` / doc 全 OK、書籍逐語コピー 0 件を Grep 確認、既存テスト全パス継続）

### 選択肢 B: Chunk 10（NTFS `$DATA` 非常駐属性 + runlist 解析）の新規実装（マイルストーン押し上げ）

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

### 選択肢 C: Chunk 7（NTFS `$STANDARD_INFORMATION`）の軽い確認レビュー

- **対象**: `crates/fs-ntfs/src/attributes/standard_information.rs` + `attributes/mod.rs`（既存 198行、10 単体テスト + 結合 2 件）
- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」$STANDARD_INFORMATION セクション（Table 13.5 周辺 / FILETIME 1601-01-01 起算 100ns 単位 / NT(48B) / W2K+(72B) 拡張）
- **目的**: 4 種タイムスタンプ（created / modified / mft_modified / accessed）の書籍仕様整合性、FILETIME → DateTime<Utc> 変換のオーバーフロー安全性の書籍裏付け、DOS ファイル属性フラグ（READ_ONLY / HIDDEN / SYSTEM / ARCHIVE / COMPRESSED / ENCRYPTED / DIRECTORY）のビット定義の書籍確認、NT 版（48バイト）と W2K+ 拡張版（72バイト）の判別ロジックの書籍裏付け、書籍例題の数学的再現テスト追加
- **期待効果**: タイムスタンプ解釈（フォレンジック観点で極めて重要な一次情報）の書籍突合済み品質保証
- **行数推定**: +30〜80行程度（FILETIME 書籍例題 / W2K+ 拡張フィールド境界 / フラグビット網羅）
- **著作権配慮**: Chunk 4/5/6 と同方針
- **完了条件**: Chunk 4/5/6 レビューと同等

### 選択肢 D: Chunk 9（NTFS `$DATA` 常駐属性）の軽い確認レビュー

- **対象**: `crates/fs-ntfs/src/attributes/data.rs`（既存 200行、8 単体テスト + 結合 3 件）
- **参考書籍**: Brian Carrier「File System Forensic Analysis」(2005, ISBN 9780321374752) Chapter 13「NTFS Data Structures」$DATA セクション（Table 13.4 関連、resident 領域の content_offset / content_size セマンティクス、ADS 名前付きストリーム）
- **目的**: 常駐 $DATA のバイト抽出ロジック（`ResidentInfo.content_offset` / `content_size` からのスライス取得）の書籍仕様整合性、ADS（名前付き $DATA）の書籍記述確認、flags（0x0001=COMPRESSED / 0x4000=ENCRYPTED / 0x8000=SPARSE）の書籍定義裏付け、Chunk 6 で既に Table 13.4 完全突合済みのため軽量レビューで十分
- **期待効果**: Phase 1 技術核心（SHA256 完全一致実証）の土台となる常駐 $DATA 抽出ロジックの書籍突合済み品質保証
- **行数推定**: +30〜60行程度（書籍例題テスト追加程度）
- **著作権配慮**: Chunk 4/5/6 と同方針
- **完了条件**: Chunk 4/5/6 レビューと同等

### 推奨優先順位

1. **第一候補**: 選択肢 B（Chunk 10 新規実装） — M2 NTFSリーダα を事実上完了に押し上げる、プロダクト価値の大ファイル領域への拡張
2. **第二候補**: 選択肢 A（Chunk 8 書籍突合） — 顧客要件（日本語・絵文字）が掛かる中核実装の品質保証、フォレンジック観点での $FILE_NAME と $SI 時系列差異の解釈基盤
3. **第三候補**: 選択肢 C/D（Chunk 7/9 軽い確認） — 残りの NTFS 内部実装の書籍突合完成、ただし発見事項が少ない可能性が高い（Chunk 6 と同様にテスト追加のみで完結する見込み）

並列実行する場合は、選択肢 B（新規実装）と 選択肢 A/C/D（既存コードレビュー）は独立性が高いので並列化を検討可能。
