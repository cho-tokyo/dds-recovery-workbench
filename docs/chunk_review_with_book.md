# Chunk 1-9 見直し指示書（書籍突合レビュー）

書籍 (`docs/specs/ntfs-references/_private/9780321374752.pdf`) の入手により、Chunk 4-9 の実装に対し、信頼できる仕様根拠との突合レビューが可能になりました。

このドキュメントは Claude Code に渡して、実装の妥当性を確認・改善するためのガイドです。

---

## 全体方針

### レビュー対象

| Chunk | 対象 | レビュー必要度 |
|---|---|---|
| 1 | dds-core 共通型 | ✗ 不要（NTFS無関係） |
| 2 | dds-fs-common トレイト | ✗ 不要 |
| 3 | dds-disk-io | ✗ 不要 |
| **4** | NTFSブートセクタ | ★★ 推奨 |
| **5** | MFTエントリ + フィクサップ | ★★★ **強く推奨**（フィクサップは事故多発） |
| **6** | 属性ヘッダ | ★★ 推奨 |
| **7** | 属性イテレータ + $SI | ★★ 推奨 |
| **8** | $FILE_NAME | ★★ 推奨 |
| **9** | $DATA 常駐 + ADS | ★ 軽い確認 |

### レビューの基本フロー（各Chunkに適用）

1. 書籍の該当セクション（後述）を読む
2. 現在の実装（該当する .rs ファイル）を読む
3. 以下を確認:
   - 構造体フィールドの過不足
   - フィールドサイズ・オフセットの正しさ
   - エンディアン処理
   - エッジケースの取りこぼし
   - エラー検出条件
4. 単体テストに追加すべきケースを抽出
5. 実装の更新（必要なら）
6. テスト追加・再実行
7. progress.md にレビュー結果を記録

### 書籍内の参照位置の探し方

書籍は PDF なので、Acrobat や Edge の検索機能で以下のキーワードで該当箇所に飛べます:

- "Boot Sector" / "VBR" → Chunk 4 関連
- "MFT Entry" / "FILE Record" / "Fixup" / "Update Sequence" → Chunk 5
- "Attribute Header" / "Resident" / "Non-Resident" → Chunk 6
- "Standard Information" / "FILETIME" → Chunk 7
- "File Name Attribute" / "Namespace" / "POSIX" → Chunk 8
- "Data Attribute" / "Alternate Data Stream" / "ADS" → Chunk 9

---

## Chunk 4: NTFS ブートセクタ - 見直し項目

### 書籍参照箇所

- 「NTFS Concepts」章の「File System Category」と「Boot Sector」セクション
- 「NTFS Data Structures」章の「Boot Sector」セクション（バイト単位の詳細表）

### 確認項目（実装ファイル: `crates/fs-ntfs/src/boot_sector.rs`）

#### 1. フィールドの過不足
- `Total Sectors` の前にある「Reserved (0x10〜0x14)」「Always 0x80008000 (0x24〜0x27)」は実装に必要か（通常スキップしてOK）
- 書籍に記載されている全フィールドが構造体に含まれているか確認

#### 2. MFT Record Size エンコーディングの厳密性
- 書籍では `signed value` として明示されている
- 現実装で **正の値 = クラスタ数、負の値 = 2^|x| バイト** の判別が正しく i8 で行われているか
- 書籍の例（typically -10 → 1024バイト）が実装で再現できるかテスト追加

#### 3. Index Record Size（同じエンコーディング）
- `clusters_per_index_record` も同じ符号付きエンコーディング
- 同様のメソッド `index_record_size_bytes()` が必要なら追加

#### 4. ブートセクタの整合性チェック
- 書籍にある以下のチェックが現実装にあるか:
  - `bytes_per_sector` が 2の累乗かつ 256〜4096 の範囲
  - `sectors_per_cluster` が 2の累乗かつ 1〜128 の範囲
  - `OEM ID` の正確な比較（"NTFS    " - 末尾3つスペース）
  - `signature` (0x55, 0xAA) の検証

#### 5. テスト追加候補

書籍のサンプル値や図表を参考に:
- 異なるクラスタサイズ（512B/2KB/4KB/8KB/16KB/64KB）のテスト
- 4Kn ドライブ（bytes_per_sector = 4096）のテスト
- MFT Record Size の正値・負値のテスト網羅

### 期待される変更規模

- 実装変更: 小（数行追加 or 関数1個）
- テスト追加: 3〜5件

---

## Chunk 5: MFTエントリ + フィクサップ - 見直し項目【最重要】

### 書籍参照箇所

- 「NTFS Concepts」章「File System Metadata Files」セクション
- 「NTFS Data Structures」章「MFT Entry」セクション
- **「Fixup Values」セクション（重要）**

### 確認項目（実装ファイル: `crates/fs-ntfs/src/mft.rs`）

#### 1. フィクサップ実装の厳密性【最重要】

書籍の Fixup Values セクションを精読し、以下を確認:

- ✓ Update Sequence Number (USN) が**各セクタの最後の2バイト**と比較される
- ✓ USN は USA 配列の**最初の2バイト**
- ✓ フィクサップ値は USA 配列の**2番目以降**（USN自身を含むので個数は usa_size - 1）
- ✓ 不一致時の挙動: 書籍はエラー扱いか、警告継続か
- ✓ セクタサイズの取得元（ブートセクタからか、固定512か）

書籍の例題があれば、それを再現するテストを追加。

#### 2. フィクサップ範囲のチェック

書籍に記載されている安全チェック:
- USA offset が MFT エントリヘッダサイズより大きいこと
- USA offset + USA size * 2 が allocated_size を超えないこと
- USA size が `(allocated_size / sector_size) + 1` と一致すること

これらのチェックが現実装にあるか確認、なければ追加。

#### 3. BAAD シグネチャの扱い

書籍に "BAAD" 検出時の推奨動作が記載されている可能性あり:
- 現実装はエラーで停止
- 書籍が「警告付きで部分読み取り」を推奨している場合、将来チャンク用のオプション化を検討

#### 4. Sequence Number の意味

書籍では Sequence Number が「エントリ再利用検出」に使われると明記。実装では値を保持するだけになっていれば、用途のドキュメント化が必要。

#### 5. テスト追加候補

- フィクサップ不一致でも複数セクタのうち**最初のセクタだけ正常**の場合（部分破損）
- USA offset が異常値（例: ヘッダ内部を指す）
- USA size が `(record_size / 512) + 1` 以外の異常値
- USN 自体が 0（書籍に「無効値の可能性」記載があれば）

### 期待される変更規模

- 実装変更: 中（フィクサップ検証ロジックの強化）
- テスト追加: 4〜6件

---

## Chunk 6: 属性ヘッダ - 見直し項目

### 書籍参照箇所

- 「NTFS Concepts」章「Attributes Concept」セクション
- 「NTFS Data Structures」章「Attribute Header」セクション
- 「Resident vs Non-resident Attributes」セクション

### 確認項目（実装ファイル: `crates/fs-ntfs/src/attribute.rs`）

#### 1. 属性タイプ網羅性

書籍の Type ID 一覧表と現実装の `AttributeType` enum を突合:
- 0x10 〜 0x100 まで全部カバーされているか
- 書籍に記載されているが現実装にない属性タイプの有無（特に `$EA`, `$EA_INFORMATION`, `$LOGGED_UTILITY_STREAM` あたり）

#### 2. 非常駐ヘッダのフィールド

書籍の非常駐ヘッダ詳細表で:
- `Compression Unit Size` の値域・意味
- `Padding` の位置と扱い
- `Allocated Size`, `Real Size`, `Initialized Size` の意味の違い

特に **Initialized Size**: 「実データが書かれた範囲（その先はゼロ扱い）」という意味を確認。Phase 1 のスパース対応に関係。

#### 3. フラグビット定義

書籍に flags の完全なビット定義表があれば、現実装の `0x0001=圧縮、0x4000=暗号化、0x8000=スパース` 以外に取りこぼしがないか:
- ビット 0x0002, 0x0004 など中間ビットの定義（あれば）
- 「マウントポイント」「シンボリックリンク」関連ビット

#### 4. テスト追加候補

- 各 Type ID の `from_raw` / `to_raw` ラウンドトリップテスト
- フラグの組み合わせ（圧縮+暗号化 等）
- 名前付き属性のヘッダ長計算（name_offset の妥当性）

### 期待される変更規模

- 実装変更: 小（enum バリアント追加程度）
- テスト追加: 2〜4件

---

## Chunk 7: 属性イテレータ + $SI - 見直し項目

### 書籍参照箇所

- 「Standard Information Attribute」セクション
- 「FILETIME format」または「Time Values」セクション（書籍内のどこか）

### 確認項目

#### 実装ファイル: `crates/fs-ntfs/src/attributes/standard_information.rs`

#### 1. NT版 vs W2K+版の判別

書籍に正確な判別基準があるはず:
- 現実装: `content_size >= 72` で W2K+
- 書籍: 別の基準（バージョン番号フィールド等）を使うか?

#### 2. 全フィールドの解釈

書籍の構造体定義と現実装を全フィールド突合:
- `Maximum Versions`, `Version Number`, `Class ID` の正確な意味
- W2K+ 拡張の `Quota Charged` のセマンティクス
- USN フィールドが `$LogFile` の何を指しているか

#### 3. FILETIME 変換の厳密性

書籍に FILETIME → DateTime 変換の正確な式が記載されているはず:
- 起点: 1601-01-01 00:00:00 UTC
- 単位: 100ナノ秒
- うるう秒の扱い（書籍に言及があるか）

現実装のテスト9番（既知 FILETIME → DateTime）を、書籍の例題で検証。

#### 4. DOS File Attributes ビット一覧

書籍の完全なビット一覧と現実装の `FileAttributes` を突合:
- 取りこぼしビットの有無（特に DEVICE, INTEGRITY_STREAM, VIRTUAL, NO_SCRUB_DATA 等の新しめのビット）
- DIRECTORY ビット（0x10000000）が NTFS 独自である点の確認

#### 実装ファイル: `crates/fs-ntfs/src/attributes/mod.rs`

#### 5. AttributeIterator の終端処理

書籍に「属性は Type ID 昇順で並ぶ」「終端マーカーで終わる」と明示されているか確認。現実装の終端判定が正しいか。

#### 6. テスト追加候補

- 書籍の例題を再現するテスト
- 異常値の FILETIME（例: u64::MAX）の挙動
- NT-only 形式と W2K+ 形式の境界値テスト

### 期待される変更規模

- 実装変更: 小〜中
- テスト追加: 3〜5件

---

## Chunk 8: $FILE_NAME - 見直し項目

### 書籍参照箇所

- 「File Name Attribute」セクション
- 「Hard Links」セクション（複数 $FILE_NAME の文脈）
- 「Directory Entries」または「Index B-Tree」関連（親ディレクトリ参照の文脈）

### 確認項目（実装ファイル: `crates/fs-ntfs/src/attributes/file_name.rs`）

#### 1. Parent Directory MFT Reference の bit 分解

書籍に正確な bit 分解仕様があるはず:
- 現実装: 下位48bit = エントリ番号、上位16bit = シーケンス番号
- 書籍がエンディアンや順序を明示しているか確認（リトルエンディアンなら下位48bit が先頭6バイト）

#### 2. ハードリンク対応

書籍の「Hard Links」セクションで:
- 1つのファイルが複数の親ディレクトリを持つ場合の挙動
- 各 $FILE_NAME が別々の親を指す
- 現実装の `find_best_file_name()` は最初の1つしか返さない → ハードリンク認識を逃す可能性

→ 改善案: ハードリンク数（MFTヘッダの `hard_link_count`）が 2以上なら全 $FILE_NAME を取得する API を追加

#### 3. Namespace の優先度

書籍の推奨優先度と現実装の `find_best_file_name()` を比較:
- 現実装: Win32 > Win32+DOS > POSIX > DOS
- 書籍: 別の優先度を推奨している可能性

特に Win32 と Win32+DOS の優先順位は微妙（後者の方が情報量が少ないため）。

#### 4. ファイル名の長さ上限

書籍に NTFS の最大ファイル名長（255文字）の明示があれば、検証ロジックを追加:
- `name_length > 255` でエラー

#### 5. Reparse Tag フィールド

書籍に `EA/Reparse value` (offset 0x3C) の意味の詳細記載があれば、現実装のフィールド命名・処理を見直し。

#### 6. テスト追加候補

- ハードリンクを持つファイル（複数 $FILE_NAME）の処理
- 名前長 255 のテスト
- 名前長 0 のテスト（仕様上 valid か?）
- 親ディレクトリ参照がルート（エントリ番号5）でない場合

### 期待される変更規模

- 実装変更: 中（ハードリンク対応の追加可能性）
- テスト追加: 3〜5件

---

## Chunk 9: $DATA 常駐 + ADS - 見直し項目

### 書籍参照箇所

- 「Data Attribute」セクション
- 「Alternate Data Streams」セクション
- 「Resident vs Non-resident threshold」関連記述

### 確認項目（実装ファイル: `crates/fs-ntfs/src/attributes/data.rs`）

#### 1. 常駐 / 非常駐の境界値

書籍に「何バイトを超えると非常駐になるか」の明示記述があれば、テストの参考値として使用。

#### 2. ADS の命名規則

書籍の ADS セクションで:
- ADS 名の文字制限（NTFS 仕様）
- 予約名 (`$Zone.Identifier` 等) の存在
- 現実装の `String` 表現が NTFS の制限と整合するか

#### 3. 圧縮ファイルの検出

書籍に LZNT1 圧縮の概要が記載されているはず。Phase 1 では復号しないが:
- 圧縮ファイルの判定基準（フラグだけか、追加チェックが必要か）
- 圧縮ファイルをそのまま raw データとして返すと「ゴミ」になる注意喚起

#### 4. スパースファイル

書籍のスパースファイル節で:
- 常駐スパースが実在するか（通常は非常駐）
- スパースフラグ立ちで常駐の場合のデータ解釈

#### 5. テスト追加候補

- ADS の網羅テスト（名前付き複数 + 無名）
- 名前付きストリームのデコード（書籍に例題があれば再現）
- 圧縮フラグ立ちのファイルが警告付きで処理される

### 期待される変更規模

- 実装変更: 小（コメント追加程度）
- テスト追加: 1〜3件

---

## 全Chunk共通: 見直しの実行方法

### Step 1: PDFビューアで書籍を開く

```
docs/specs/ntfs-references/_private/9780321374752.pdf
```

Acrobat または Edge で開く。

### Step 2: Claude Code に Chunk ごとに以下の指示

```
docs/chunk_review_with_book.md の「Chunk N: 見直し項目」セクションを読んで、
書籍 docs/specs/ntfs-references/_private/9780321374752.pdf の該当箇所と
現在の実装 (crates/fs-ntfs/src/<該当ファイル>.rs) を突合してください。

以下の手順:
1. 書籍の該当セクションを読む
2. 現実装と書籍の仕様を比較
3. 差分・改善点を整理
4. 単体テストに追加すべきケースを抽出
5. 改善実装 + テスト追加
6. tester で全テスト再実行
7. progress-tracker で「Chunk N レビュー完了」を記録
```

### Step 3: 推奨レビュー順序

優先度の高い順に:

1. **Chunk 5（フィクサップ）** ← 最優先、事故発生時の被害が最大
2. **Chunk 4（ブートセクタ）** ← 全 NTFS 処理の基盤
3. **Chunk 6（属性ヘッダ）** ← 後続全部の基盤
4. **Chunk 8（$FILE_NAME）** ← ハードリンク対応が大きな差分の可能性
5. **Chunk 7（$SI）** ← 取りこぼし属性の確認
6. **Chunk 9（$DATA 常駐）** ← 軽い確認で十分

### Step 4: 全体まとめ

全 Chunk のレビュー後、`docs/progress.md` に**レビュー結果サマリ**を追記:

```markdown
## 書籍突合レビュー結果（YYYY-MM-DD）

| Chunk | 変更行数 | 追加テスト | 重要な発見 |
|---|---|---|---|
| 4 | +12 | +3 | クラスタサイズ範囲チェック追加 |
| 5 | +45 | +5 | フィクサップの USA size 検証強化 |
| 6 | +8  | +2 | $LOGGED_UTILITY_STREAM 追加 |
| 7 | +20 | +4 | DEVICE 属性ビット追加 |
| 8 | +60 | +5 | ハードリンク対応 API 追加 |
| 9 | +5  | +1 | ADS 命名規則ドキュメント化 |
```

---

## 注意事項

### 著作権配慮

- 書籍は **`_private/`** 配下に置いてあり、`.gitignore` で除外されている
- 書籍の内容を**そのままコピーして**コード コメントや README に貼り付けない
- 「自分の言葉で言い換えた仕様メモ」を `docs/specs/ntfs-references/notes.md`（コミット可）に書く形を推奨
- テストデータは書籍の例題を**そのまま使うのは可**（事実の利用、表現の利用ではない）

### レビューにかかる時間

全 Chunk レビュー込みで **6〜10時間**程度を見込む。  
1日にまとめてではなく、Chunk 単位で空き時間に進める方が集中力を維持できる。

### 既存テストの破壊チェック

実装変更後は必ず `cargo test --workspace` を流し、既存テストが破壊されていないか確認。仮に破壊された場合は:
- 既存テストの期待値が間違っていた（書籍に従って修正）
- 新実装にバグが入った（実装を修正）

の切り分けが必要。

### レビューの一括 vs 逐次

**逐次推奨**:
- Chunk 5 をレビューしてテストを通す → 次に Chunk 4 → ... の順で
- 並列で全部レビューするとコンテキストが混乱する

### 進捗管理

各 Chunk レビュー完了時に `docs/progress.md` の該当チャンク行に "📕 Reviewed" マーク追加:

```markdown
| 5 | fs-ntfs | MFTエントリヘッダ | 230行 | 8件 ✓ | 95% | 2026-05-XX 📕 |
```

---

## レビュー完了後の効果

このレビューを完遂すると:

1. **Phase 1 NTFS 実装の品質が商用レベルに到達**
2. **エッジケース耐性が大幅向上** ← 実顧客HDDで遭遇する異常データへの耐性
3. **Chunk 10 着手前にコア部分が堅固に** ← 最難関 Chunk 10 のデバッグが楽になる
4. **将来の Chunk 11+（ディレクトリツリー等）の前提が固まる**

レビュー後に Chunk 10 へ進むのが、長期的には最速ルートです。
