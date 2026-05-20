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

## 7. 参考リソース

- 書籍 9780321374752 第 13 章（NTFS Data Structures）— 本メモの主参照源。
- Linux NTFS Documentation Project（公開ウェブ資料）
- libfsntfs ドキュメント（公開）

書籍からの逐語コピーがないことは Grep で確認すること:
`rg -F "<書籍の特徴的フレーズ>" docs/specs/ntfs-references/notes.md`
