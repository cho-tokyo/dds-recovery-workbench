# 最初のチャンク指示（Chunk 1）

このドキュメントは Claude Code が**最初に着手すべき作業**を定義します。  
`CLAUDE.md` を読了した後、ここから実装を開始してください。

---

## Chunk 1: `dds-core` 共通エラー型定義

### 目的

全クレートで共通利用するエラー型・基本型を `dds-core` クレートに実装する。これがないと他クレートが依存できないため、最優先で着手。

### 対象クレート

`crates/core/`

### 実装内容

`crates/core/src/lib.rs` に以下を実装:

#### 1. エラー型 `CoreError`（thiserror使用）

以下のバリアントを持つ enum:

- `Io(std::io::Error)` - I/Oエラー
- `Parse { context: String, reason: String }` - パースエラー（汎用）
- `InvalidArgument(String)` - 不正な引数
- `OutOfRange { what: String, value: u64, max: u64 }` - 範囲外
- `Unsupported(String)` - 未対応機能
- `Internal(String)` - 内部エラー

#### 2. 結果型エイリアス

```rust
pub type CoreResult<T> = Result<T, CoreError>;
```

#### 3. 損傷レベル enum `DamageLevel`

PRDのL1〜L6に対応する enum:

```rust
pub enum DamageLevel {
    L1_DeletionOnly,
    L2_PartitionTableDamaged,
    L3_FsMetadataPartiallyDamaged,
    L4_BothDamaged,        // Phase 2以降
    L5_FsMetadataLost,     // Phase 2以降（フルフォーマット）
    L6_SevereDamage,       // Phase 2以降
    PhysicalIssue,         // 物理障害
}
```

- `Display` 実装
- 日本語ラベル取得メソッド `display_ja(&self) -> &'static str`

#### 4. 抽出方法 enum `RecoveryMethod`

```rust
pub enum RecoveryMethod {
    L1_MetadataIntact,     // FSメタ健全
    L2_PartitionReconstructed,
    L3_FsMetadataReconstructed,
}
```

#### 5. 品質評価 enum `QualityRating`

```rust
pub enum QualityRating {
    Green,    // 健全
    Yellow,   // 軽微破損
    Orange,   // 重大破損
    Red,      // 破損
}
```

- `is_acceptable(&self) -> bool` メソッド: Green/Yellowなら true

### 単体テスト要件（最低5件）

`crates/core/src/lib.rs` の同ファイル内に `#[cfg(test)] mod tests` で配置:

1. `CoreError::Io` のディスプレイ出力テスト
2. `CoreError::OutOfRange` のメッセージにvalue/max値が含まれることのテスト
3. `DamageLevel::display_ja` の日本語ラベル正しさテスト（全バリアント）
4. `QualityRating::is_acceptable` の真理値テスト（Green/Yellow→true、Orange/Red→false）
5. `RecoveryMethod` の `Display` 出力テスト

### Cargo.toml 設定

`crates/core/Cargo.toml`:

```toml
[package]
name = "dds-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[dependencies]
thiserror.workspace = true
serde.workspace = true
```

### 制約

- 行数上限: **200行（実装+テスト合計）**
- 単体テスト最低3件 → このChunkでは5件以上を目標
- 全公開 type/enum/method に rustdoc コメント必須
- 関連FR: 設計基盤（特定のFRには対応しないが、全FRの前提）

### 完了条件チェックリスト

builder完了時点で以下を確認:

- [ ] `cd /path/to/dds-recovery-workbench && cargo check -p dds-core` がエラーなし
- [ ] `cargo test --lib -p dds-core` が全パス
- [ ] `cargo clippy -p dds-core -- -D warnings` がエラーなし
- [ ] `cargo doc -p dds-core --no-deps` が正常生成

### 完了後

1. tester エージェントへ引き継ぎ
2. tester がテスト合格を確認したら progress-tracker へ
3. progress-tracker が `docs/progress.md` に Chunk 1 完了を記録
4. 次は **Chunk 2: `dds-fs-common` のFS共通トレイト定義**（後続指示は完了時に出します）

---

## 注意事項

- このChunkは**設計の土台**なので、エラー型のバリアント命名は今後の全クレートに影響する。慎重に
- `thiserror` の `#[error("...")]` メッセージは将来CSがエラーメッセージとして見る可能性があるため、ユーザフレンドリーに
- `unsafe` ブロックは不要なはず。使わないこと
- 日本語コメント可。コードコメント（rustdoc以外）は日本語推奨

---

## 後続チャンクのプレビュー

参考までに、Phase 1の最初の数チャンクの予定:

| # | クレート | 内容 | 概算行数 |
|---|---|---|---|
| 1 | core | 共通エラー型・基本enum | 150-200 |
| 2 | fs-common | FS共通トレイト（FsReader trait等） | 150 |
| 3 | disk-io | Raw disk access抽象化（trait定義+モック実装） | 150 |
| 4 | fs-ntfs | NTFSブートセクタパーサ | 150 |
| 5 | fs-ntfs | NTFS MFTエントリヘッダパーサ | 100 |
| 6 | fs-ntfs | NTFS 属性ヘッダパーサ | 150 |
| 7 | fs-ntfs | NTFS `$STANDARD_INFORMATION` 属性 | 100 |
| 8 | fs-ntfs | NTFS `$FILE_NAME` 属性 | 150 |
| 9 | fs-ntfs | NTFS `$DATA` 常駐属性 | 100 |
| 10 | fs-ntfs | NTFS `$DATA` 非常駐(runlist) | 200 |

Chunk 1 完了後、Chunk 2 の詳細指示を別途出します。
