# NTFS 実装メモ

このメモは Brian Carrier 著『File System Forensic Analysis』第 13 章
（NTFS Data Structures）を読み込んだうえで、実装で必要となる要点を
**自前の言葉で再構成**したものである。書籍からの逐語コピーは含まない。
章番号・Table 番号への言及は事実情報なので可とする。

参照書籍: 9780321374752（`_private/` 配下、コミット対象外）

---

## 1. このメモの位置付け

- 著作権配慮として書籍本文を一切コピーしない。要約・言い換え・自前作図のみ記載する。
- 実装担当者が手元コードを書くときに参照する「言い換え版チートシート」。
- 不明点があれば必ず原典に当たること。このメモは原典の代替ではない。

---

## 2. Fixup（Update Sequence）メカニズム

NTFS では「MFT エントリ」「INDX レコード」など複数セクタにまたがる
構造体を扱う。途中で電源断などにより一部セクタだけ書き込まれた状態
を検出するため、書き込み時に各セクタ末尾 2 バイトを共通の signature
value（USN）で上書きする仕組みが Fixup である。

書き込み手順（読み取り側からは関係ないが理解のため）:

1. 構造体全体をメモリ上で組み立てる。
2. 各セクタ末尾 2 バイトの「本来の値」を Update Sequence Array (USA) に退避。
3. 各セクタ末尾 2 バイトを USN（USA[0]）で上書き。
4. ディスクへ書き出す。

読み取り側の手順（本実装が担当）:

1. USA の先頭ワードを USN として取得する。
2. 各セクタ末尾 2 バイトが USN と一致することを確認する。
   - 不一致なら「部分破損」とみなしてエラーを返す（`FixupMismatch`）。
3. 一致を確認したセクタ末尾を USA[i+1] の値で上書きし、本来の値に戻す。

破損済みエントリは `"BAAD"` シグネチャでマーキングされている。これは
ファイルシステム自身が「このエントリは信用ならない」と宣言したもの。
本実装では magic 検査の段階で `BadEntry` を返す。

---

## 3. USA size の整合性ルール

書籍では USA のワード数が次式を満たすことを期待する旨が説明されている:

```
usa_size = ceil(allocated_size / sector_size) + 1
```

- 末項 `+1` は USA[0]（USN そのもの）の分。
- `ceil(allocated_size / sector_size)` は fixup される（=末尾を持つ）セクタ数。

代表例:

| allocated_size | sector_size | usa_size |
|---|---|---|
| 1024 | 512 | 3 |
| 2048 | 512 | 5 |
| 4096 | 512 | 9 |
| 4096 | 4096 | 2 |

実装ではこの式から外れる値を `InvalidUsaSize` として弾く（`mft.rs::parse_mft_entry`）。

---

## 4. MFT Entry 主要フィールド（書籍 Table 13.1 対応の自前再構成）

| 仕様で使われる呼称 | 実装上のフィールド名 | サイズ | 備考 |
|---|---|---|---|
| Signature | (magic) | 4B | `"FILE"` または `"BAAD"` |
| Offset to fixup array | `usa_offset` | 2B | レコード先頭からの相対オフセット |
| Number of entries in fixup array | `usa_size` | 2B | USN + fixup 件数 |
| $LogFile sequence number | `lsn` | 8B | ジャーナリング用 |
| Sequence value | `sequence_number` | 2B | 世代カウンタ。後述 5.1 |
| Link count | `hard_link_count` | 2B | 参照ディレクトリエントリ数。後述 5.2 |
| Offset to first attribute | `first_attribute_offset` | 2B | 通常 0x38 付近 |
| Flags | `flags` | 2B | bit0=in-use, bit1=directory |
| Used size of MFT entry | `used_size` | 4B | 実使用バイト数 |
| Allocated size of MFT entry | `allocated_size` | 4B | 通常 1024 または 4096 |
| File reference to base record | `base_record_reference` | 8B | 0 ならベース |
| Next attribute id | `next_attribute_id` | 2B | 属性追加時の重複防止 |
| MFT record number | `mft_record_number` | 4B | XP 以降。0 のとき None |

---

## 5. Sequence number と Hard link count のニュアンス

### 5.1 Sequence number（世代カウンタ）

- 同一 MFT レコード番号は再利用される。あるエントリが削除されたあと、
  別のファイル用に再割当される場合がある。
- `sequence_number` はその「世代」を区別するカウンタで、割当または
  解放のたびに +1 される。
- 用途: ディレクトリエントリや index entry が保持している MFT 参照
  （レコード番号 + sequence value のペア）が、現在指しているエントリ
  と本当に同じファイルを指しているかを検証する。
- 古い参照を持っているだけで実体が別ファイルになっている場合、
  sequence value が一致しないので「ぶら下がり参照」として検出できる。

### 5.2 Hard link count

- そのレコードを参照しているディレクトリエントリの数。
- ファイルが 1 つの名前しか持たないなら通常 1。
- ハードリンクで別名を追加するたびに +1。
- 値が 0 の場合、そのエントリは「どのディレクトリからも参照されていない」
  ことを意味する（基本的に削除済み）。

---

## 6. 実装上の制約まとめ

- **read-only**: ソースディスクへの書き込み API は実装しない。
- `unsafe` 禁止、`from_be_bytes` 禁止（NTFS は完全リトルエンディアン）。
- 例外を投げず必ず `Result` で返す。`unwrap`/`expect` は本番コード禁止。
- 部分破損（fixup mismatch）は明示的にエラー型で返す。サイレント
  リカバリは行わない（呼び出し側で復旧戦略を選ばせる）。

---

## 7. Boot Sector（$BOOT ファイルの先頭セクタ）

書籍 第 13 章「$BOOT FILE」セクション Table 13.18「Data structure for the boot sector」
を自分の言葉で再構成したフィールド一覧。NTFS 解析の起点となる構造。

### 7.1 フィールド表（先頭 0x54 バイトのみ実装で参照）

| オフセット | サイズ | フィールド名 | 実装での扱い |
|---|---|---|---|
| 0x00 | 3B | Jump instruction | 検査せず（任意の x86 jmp） |
| 0x03 | 8B | OEM ID | `"NTFS    "` と完全一致を要求 |
| 0x0B | 2B | Bytes per sector | 2 累乗かつ 256〜4096。一般は 512 / 4096 |
| 0x0D | 1B | Sectors per cluster | 2 累乗かつ 1〜128 |
| 0x0E〜0x14 | 7B | "Must be 0" 群 | 緩い検証（Phase 1 では未チェック） |
| 0x15 | 1B | Media descriptor | 値保持のみ（通常 0xF8） |
| 0x16〜0x17 | 2B | "Must be 0" | 同上 |
| 0x18〜0x23 | 12B | CHS / Hidden sectors 等 | 値保持・検査せず |
| 0x24〜0x27 | 4B | "Must be 0" | 同上 |
| 0x28 | 8B | Total sectors | u64 として保持 |
| 0x30 | 8B | $MFT cluster (LCN) | u64 として保持 |
| 0x38 | 8B | $MFTMirr cluster (LCN) | u64 として保持 |
| 0x40 | 1B (i8) | Clusters per MFT record | 符号付きエンコーディング（後述） |
| 0x41〜0x43 | 3B | 予約 / 未使用 | 検査せず |
| 0x44 | 1B (i8) | Clusters per index record | 同じ符号付きエンコーディング |
| 0x45〜0x47 | 3B | 予約 | 検査せず |
| 0x48 | 8B | Volume serial number | u64 として保持 |
| 0x50〜0x53 | 4B | チェックサム | 検査せず |
| 0x54〜0x1FD | ブートコード | 検査せず |
| 0x1FE | 2B | Boot signature | `0xAA55` を要求 |

### 7.2 "Must be 0" フィールドの方針

Table 13.18 には複数の「値は 0 でなければならない」フィールドがあるが、
実環境のディスクでは一部のツールが非 0 を書くケースが報告されている。
Phase 1 では**緩い検証**（OEM ID / signature / bps / spc のみ厳格に検査）に
留め、後続フェーズで `tracing::warn!` ベースの soft warning を出す予定。
ソースディスクの read-only 原則上、ここで panic させる利益はない。

### 7.3 MFT/Index Record size の符号付きエンコーディング

書籍 380 ページの本文要約: `clusters_per_mft_record` および
`clusters_per_index_record` は i8 として読む。

- 値が **0 より大きい**場合: その値はそのまま **クラスタ数**。
  実バイト数 = `value * cluster_size_bytes()`。
- 値が **0 より小さい**場合: その値の絶対値が「2 の指数（log2 of bytes）」。
  実バイト数 = `1 << (-value)`。

書籍 381 ページの例題: byte 0x40 = -10 (i8) → 2^10 = 1024 byte。
                       byte 0x44 = 4  (i8) →  4 × 1 KB cluster = 4096 byte。

実装は `compute_record_size_bytes(raw: i8, cluster_size: u32) -> u32` という
1 関数に集約し、MFT 用と Index 用の両アクセサから共有する。

### 7.4 検証強化のレビュー指針

書籍 第 10 章 FAT（Table 10.1）が `bps ∈ {512, 1024, 2048, 4096}` と
列挙しているのに準じ、NTFS の bps も同範囲＋2 累乗で弾く。spc も同様に
2 累乗かつ ≤128（実 NTFS 最大）で弾く。これにより破損ブートセクタを
早期に拒否でき、後段の overflow / 0 除算リスクを抑える。

---

## 8. Attribute Header（属性ヘッダ）

書籍 第 13 章「Attributes」セクション Table 13.2/13.3/13.4 を読み込み、
実装で必要なフィールドを**自前の言葉で再構成**した。属性は MFT エントリ
の本体を構成する単位で、すべて共通の 16 バイトヘッダで始まり、その後に
常駐／非常駐の区別に応じた追加ヘッダが続く。

### 8.1 共通ヘッダ（Table 13.2 対応、先頭 16 バイト）

| オフセット | サイズ | フィールド | 実装での扱い |
|---|---|---|---|
| 0x00 | 4B | Attribute type identifier | `AttributeType::from_raw` で enum 化 |
| 0x04 | 4B | Length of attribute | 次属性までの相対オフセット。0 は不正 |
| 0x08 | 1B | Non-resident flag | 0=resident / 1=non-resident、それ以外はエラー |
| 0x09 | 1B | Length of name | 名前の Unicode 文字数（バイトでなく code units） |
| 0x0A | 2B | Offset to name | 属性先頭からの相対オフセット |
| 0x0C | 2B | Flags | bit0=compressed, bit14=encrypted, bit15=sparse |
| 0x0E | 2B | Attribute identifier | エントリ内一意の連番 |

### 8.2 常駐属性追加ヘッダ（Table 13.3 対応、offset 0x10〜）

| オフセット | サイズ | フィールド | 実装での扱い |
|---|---|---|---|
| 0x10 | 4B | Content size | コンテンツ本体のバイト数 |
| 0x14 | 2B | Content offset | 属性先頭からのコンテンツ開始位置 |
| 0x16 | 1B | Indexed flag | Linux NTFS Docs 由来。0/1 で保持 |
| 0x17 | 1B | Padding | 検査せず |

サニティ条件: `content_offset + content_size == length`（書籍 356 ページの
$STANDARD_INFORMATION 例では 0x18 + 0x48 == 0x60）。

### 8.3 非常駐属性追加ヘッダ（Table 13.4 対応、offset 0x10〜0x40）

| オフセット | サイズ | フィールド | 実装での扱い |
|---|---|---|---|
| 0x10 | 8B | Starting VCN | データ範囲の先頭仮想クラスタ番号 |
| 0x18 | 8B | Ending VCN | データ範囲の末尾仮想クラスタ番号 |
| 0x20 | 2B | Offset to runlist | 属性先頭からの runlist 開始位置 |
| 0x22 | 2B | Compression unit size | 2 の累乗で表現（0 なら非圧縮） |
| 0x24 | 4B | Padding | 検査せず |
| 0x28 | 8B | Allocated size of content | クラスタ単位に切り上げた割当バイト数 |
| 0x30 | 8B | Actual size of content | 実バイト数（real_size） |
| 0x38 | 8B | Initialized size of content | ゼロ埋め境界まで初期化されたバイト数 |

書籍 358 ページの $DATA 例: starting_vcn=0, ending_vcn=0x20EF (8431),
runlist_offset=0x40, allocated=actual=initialized=0x83C000 (8634368)。

### 8.4 属性タイプ ID 一覧（Chapter 13 で言及される 15 種）

| ID | 名称 | 概要 |
|---|---|---|
| 0x10 | $STANDARD_INFORMATION | タイムスタンプ・ファイル属性ビット |
| 0x20 | $ATTRIBUTE_LIST | エントリ溢れ時の他レコード参照リスト |
| 0x30 | $FILE_NAME | 名前・親ディレクトリ参照・名前空間 |
| 0x40 | $OBJECT_ID | NTFS オブジェクト識別 GUID |
| 0x50 | $SECURITY_DESCRIPTOR | アクセス制御情報（旧形式・通常は $Secure に集約） |
| 0x60 | $VOLUME_NAME | ボリュームラベル（$Volume のみ） |
| 0x70 | $VOLUME_INFORMATION | NTFS バージョン・フラグ（$Volume のみ） |
| 0x80 | $DATA | ファイル本体データ |
| 0x90 | $INDEX_ROOT | B-tree ルートノード（ディレクトリ等） |
| 0xA0 | $INDEX_ALLOCATION | B-tree 非ルートノード本体 |
| 0xB0 | $BITMAP | $INDEX_ALLOCATION 用クラスタアロケーションビット |
| 0xC0 | $REPARSE_POINT | シンボリックリンク・ジャンクション等 |
| 0xD0 | $EA_INFORMATION | OS/2 拡張属性メタデータ |
| 0xE0 | $EA | OS/2 拡張属性本体 |
| 0x100 | $LOGGED_UTILITY_STREAM | EFS 等の暗号化メタストリーム |

未知の type ID は `AttributeType::Unknown(raw)` で生値保持し、エラーには
しない（forward compatibility）。`0xFFFFFFFF` は属性連の終端マーカー。

### 8.5 Flag ビット意味

| ビット | 16 進値 | 意味 |
|---|---|---|
| 0 | 0x0001 | Compressed（圧縮属性。$DATA のみで通常用いる） |
| 14 | 0x4000 | Encrypted（EFS 暗号化） |
| 15 | 0x8000 | Sparse（スパースファイル） |

実装は `flags: u16` を生値で保持し、デコードは呼び出し側（`data.rs` 等）の
責務とする。これにより、複数ビット同時セット（例: compressed+encrypted は
NTFS では非推奨だが、解析時に観測される可能性がある）をビット演算で
自由に判定できる。

### 8.6 解析の停止条件

属性巡回ループは以下のいずれかで停止:

1. type ID = 0xFFFFFFFF → `AttributeHeader::End` を返して終了。
2. `length == 0` → `InvalidLength` エラー（無限ループ防止）。
3. バッファ長不足 → `BufferTooSmall { got, need }` エラー。
4. non-resident flag が 0/1 以外 → `InvalidNonResidentFlag` エラー。

---

## 9. $FILE_NAME 属性とハードリンク

書籍 第 13 章「$FILE_NAME Attribute」セクション Table 13.7／Table 13.8、
および 第 12 章「Links to Files and Directories」ハードリンク節を
読み込み、実装観点で必要な要点を**自前の言葉で再構成**したもの。
書籍本文の逐語コピーはなし。原典への言及（章番号・Table 番号・
ページ番号）は事実情報として記載。

### 9.1 フィールド表（Table 13.7 対応の自前再構成）

`$FILE_NAME`（属性タイプ 0x30）のコンテンツ部はヘッダ込み 66 バイト固定の
プリアンブル + 可変長 UTF-16LE 名前で構成される。常駐属性として格納され、
ファイル名・親ディレクトリ参照・作成時スナップショットのサイズと
タイムスタンプを保持する。

| オフセット | サイズ | フィールド名 | 実装での扱い |
|---|---|---|---|
| 0x00 | 8B | Parent directory reference (MFT ref) | `MftReference::from_raw` で 48bit+16bit に分解 |
| 0x08 | 8B | File creation time | `FileTime`（FILETIME 100ns since 1601） |
| 0x10 | 8B | File modification time | 同上 |
| 0x18 | 8B | MFT entry modification time | 同上 |
| 0x20 | 8B | File access time | 同上 |
| 0x28 | 8B | Allocated size of file | 作成時スナップショット。実値は `$DATA` 参照 |
| 0x30 | 8B | Real size of file | 同上 |
| 0x38 | 4B | Flags（ファイル属性ビット） | `FileAttributes(u32)` で生値保持 |
| 0x3C | 4B | Reparse value | Reparse Point タグ（通常 0、Mount Point は 0xA0000003 等） |
| 0x40 | 1B | Length of name | UTF-16 コードユニット数（バイトでなく u16 単位） |
| 0x41 | 1B | Namespace | 0〜3 のいずれか。後述 9.2 |
| 0x42〜 | 2N B | Name（UTF-16LE） | サロゲートペア含む可。`String::from_utf16` で変換 |

名前長は 1 バイトのため最大 255 コードユニット（UTF-16）が上限。実装では
`name_length: u8` の型自体が物理的にこれを保証する。

### 9.2 名前空間（Table 13.8 対応の自前再構成）

NTFS のファイル名は 4 種類の名前空間のうちのいずれかに属する。1 つの
MFT エントリが Win32 名 + DOS 名のように複数の `$FILE_NAME` を持つことが
普通にあるため、表示時はどれを選ぶかの優先順位が必要になる。

| 値 | 名前空間 | 意味 |
|---|---|---|
| 0 | POSIX | 大文字小文字を区別、ほぼ全ての文字を許容（区切りと NUL 除く） |
| 1 | Win32 | 大文字小文字は保持するが比較は非区別、Windows 規則の禁則文字あり |
| 2 | DOS | 8.3 短縮名（全大文字 ASCII）。表示には通常使わない |
| 3 | Win32 & DOS | Win32 名と DOS 名が同一文字列にできた場合の 1 件統合 |

実装での表示優先順位は次の順:

1. Win32 または Win32&DOS（ロング名としてユーザ向け）
2. POSIX（次点）
3. DOS（最終フォールバック。短縮名なので非推奨）

`find_best_file_name` はこの優先順位で 1 件選ぶ。

### 9.3 ハードリンクの考え方

書籍 第 12 章「Links to Files and Directories」のハードリンク節を要約:

- NTFS では同一ファイルを別ディレクトリエントリから複数の名前で参照できる。
  これがハードリンク。
- ハードリンクされたファイルは MFT エントリを 1 つだけ持ち、その中に
  ハードリンク件数分の `$FILE_NAME` 属性が並列に格納される。
- MFT エントリヘッダの `hard_link_count`（5.2 節）はリンクが増えるたびに +1。
- 1 件のファイルに対する全ハードリンク名を列挙するには、エントリ内の
  `$FILE_NAME` 属性を全て取り出す必要がある。先頭 1 件だけを見ると
  表示用の名前は取れるが、ハードリンク全体像は失う。

実装上の対応:

- `find_all_file_names`: 常駐 `$FILE_NAME` を全て列挙し `Vec<FileName>` を返す。
  ハードリンク対応 API。パース失敗は黙ってスキップ（解析停止はしない）。
- `find_best_file_name`: 上記から表示用の 1 件を選ぶ（既存 API、現状互換）。

### 9.4 Win32 と DOS の二重登録パターン

書籍が触れる典型例:

- ロング名が DOS 8.3 規則に収まらない場合、Windows は自動で
  ロング名（namespace=1: Win32）と短縮名（namespace=2: DOS）の
  2 つの `$FILE_NAME` を作成する。
- 例: ロング名 `57398408d01.tmp`（11 文字＋拡張子 3 文字、DOS の 8.3
  には収まらない長さや文字を含むケース）に対し、DOS 名 `573984~1.TMP`
  のような ~1 サフィックス形式の短縮名が追加される。
- ロング名が偶然 8.3 規則にぴったり合致する場合は 1 件にまとまり、
  namespace=3（Win32 & DOS）として格納される。

表示・希望リスト突合・レポート生成では Win32 名を主とし、DOS 名は補助
情報として保持する。`find_best_file_name` がこのポリシーを実装。

### 9.5 Reparse Value（offset 0x3C-0x3F）

`$FILE_NAME` の reparse value フィールドはファイルが Reparse Point
（シンボリックリンク、ジャンクション、マウントポイント等）の場合に
タグ値を保持する。通常ファイルでは 0。実装では `reparse_value: u32` として
生値で保持し、解釈は呼び出し側（`$REPARSE_POINT` 属性パーサや表示層）に
委ねる。

代表的タグ値（Windows ドキュメントより、書籍に列挙はないが
Reparse Point の理解に有用なので参考）:

| タグ値 | 意味 |
|---|---|
| 0xA0000003 | IO_REPARSE_TAG_MOUNT_POINT（ボリュームマウントポイント） |
| 0xA000000C | IO_REPARSE_TAG_SYMLINK（シンボリックリンク） |

---

## 10. $STANDARD_INFORMATION 属性

書籍 第 13 章「$STANDARD_INFORMATION Attribute」セクション Table 13.5（フィールド
レイアウト）／Table 13.6（フラグビット）を読み込み、実装観点で必要な要点を
**自前の言葉で再構成**した。書籍本文の逐語コピーはなし。原典への言及（章番号・
Table 番号・ページ番号）は事実情報として記載。

属性タイプ 0x10。MFT エントリ内に常駐属性として 1 件だけ存在する必須属性で、
作成・最終更新・MFT 更新・最終アクセスの 4 タイムスタンプと、DOS 由来の
ファイル属性ビット、世代管理・所有者・セキュリティ識別子・クォータ・USN 等を
保持する。

### 10.1 フィールド表（Table 13.5 対応の自前再構成）

| オフセット | サイズ | フィールド名 | 実装での扱い |
|---|---|---|---|
| 0x00 | 8B | File creation time | `FileTime`（FILETIME 100ns since 1601） |
| 0x08 | 8B | File modification time | 同上 |
| 0x10 | 8B | MFT entry modification time | 同上 |
| 0x18 | 8B | File access time | 同上 |
| 0x20 | 4B | Flags（ファイル属性ビット） | `FileAttributes(u32)` で生値保持 |
| 0x24 | 4B | Maximum number of versions | 通常 0（バージョン機能は未使用） |
| 0x28 | 4B | Version number | 通常 0 |
| 0x2C | 4B | Class identifier | 通常 0 |
| 0x30 | 4B | Owner identifier | W2K+ 拡張部の先頭。`Option<u32>` |
| 0x34 | 4B | Security identifier | `$Secure` ファイルへのキー。`Option<u32>` |
| 0x38 | 8B | Quota charged | 課金量バイト数。`Option<u64>` |
| 0x40 | 8B | Update Sequence Number (USN) | `$UsnJrnl` ジャーナルレコードへの参照。`Option<u64>` |

### 10.2 NT 版（48 バイト）と W2K+ 版（72 バイト）の判別

書籍は NT 4.0 までは offset 0x30 以降が存在せず合計 48 バイト、Windows 2000
以降は 72 バイトに拡張された旨を説明している。実装は属性ヘッダの content_size
を見るのではなく、実バイト長で分岐する:

- `bytes.len() < 48` → `BufferTooSmall` エラー
- `bytes.len() >= 0x34` で `owner_id = Some(_)`
- `bytes.len() >= 0x38` で `security_id = Some(_)`
- `bytes.len() >= 0x40` で `quota_charged = Some(_)`
- `bytes.len() >= 0x48` で `usn = Some(_)`

部分書き込み済みエントリ（例: 0x34 と 0x38 の間で切れている）に対しても
オーバーランしないよう、各フィールドを段階的に検査する。

### 10.3 Flag ビット完全列挙（Table 13.6 + NTFS 独自）

Table 13.6 は 13 種類のビットを列挙する。実装はそれらすべてに `const` と
`is_*` メソッドを定義する。NTFS 独自の `DIRECTORY` ビットは書籍には載って
いないが Linux NTFS Documentation Project で正当性が確認されている。

| 16 進値 | 定数名 | 意味 |
|---|---|---|
| 0x0001 | READ_ONLY | 読み取り専用 |
| 0x0002 | HIDDEN | 隠しファイル |
| 0x0004 | SYSTEM | システムファイル |
| 0x0020 | ARCHIVE | アーカイブ対象 |
| 0x0040 | DEVICE | デバイスファイル（予約） |
| 0x0080 | NORMAL | 他属性が一切セットされていない通常ファイル |
| 0x0100 | TEMPORARY | 一時ファイル |
| 0x0200 | SPARSE_FILE | スパース割当ファイル |
| 0x0400 | REPARSE_POINT | Reparse Point（シンボリックリンク・ジャンクション等） |
| 0x0800 | COMPRESSED | NTFS 圧縮 |
| 0x1000 | OFFLINE | オフライン記憶域に退避済み（HSM） |
| 0x2000 | NOT_CONTENT_INDEXED | インデックスサービス除外 |
| 0x4000 | ENCRYPTED | EFS 暗号化 |
| 0x1000_0000 | DIRECTORY | ディレクトリ（NTFS 独自、書籍非掲載） |

### 10.4 FILETIME 変換の正確性

FILETIME は 1601-01-01 00:00:00 UTC を起点とする 100 ナノ秒単位の 64bit 値。
Unix エポック（1970-01-01）との秒差は固定 `11_644_473_600` 秒。実装は
i64 にキャストしてから `checked_div` / `checked_sub` / `checked_mul` で
段階的に変換し、範囲外は `None` を返す。`u64::MAX` 入力でもパニックしない
ことを単体テストで保証する。

### 10.5 書籍 $MFT 例題の検証値

書籍 361 ページの $MFT 自身（MFT エントリ 0）の $STANDARD_INFORMATION を
例題として再現する:

- 4 つのタイムスタンプは全て同じ FILETIME 値（フォーマット時刻）
- flags = 0x0000_0006（HIDDEN | SYSTEM。`$MFT` はシステム隠しファイル）
- max_versions = 0, version_number = 0, class_id = 0
- owner_id = 0, security_id = 1, quota_charged = 0, usn = 0

W2K+ 拡張部は全て格納されているが、本質的に未使用フィールドはゼロ。
解析結果として `is_hidden() && is_system()` が真、`is_read_only()` が偽。

---

## 11. 参考リソース

- 書籍 9780321374752 第 12 章（リンク）/ 第 13 章（NTFS Data Structures）— 本メモの主参照源。
- Linux NTFS Documentation Project（公開ウェブ資料）
- libfsntfs ドキュメント（公開）

書籍からの逐語コピーがないことは Grep で確認すること:
`rg -F "<書籍の特徴的フレーズ>" docs/specs/ntfs-references/notes.md`
