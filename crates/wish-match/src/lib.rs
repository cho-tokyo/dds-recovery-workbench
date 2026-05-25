//! # dds-wish-match
//!
//! Chunk 15: お客様の希望リスト × ファイル群のパターンマッチエンジン基盤。
//!
//! NTFS 技術実装層 (Chunks 4-14) の上に乗る、業務統合層の最初の一歩。
//! 各 FS リーダが提供するファイル情報を [`FileInfo`] という FS 非依存の汎用型に
//! 揃え、お客様の `Wishlist`（希望リスト）と突合する。
//!
//! ## Phase 1.5 Chunk 23.7 での意味再定義
//!
//! Phase 1（Chunks 15-23.6）までは「Wishlist = 復旧対象の指定（Inclusion フィルタ）」
//! だったが、R-STUDIO 風の業務フローに合わせて **「Wishlist = お客様優先データのラベリング」**
//! に再定義。復旧範囲は [`ExclusionList`] で除外しない全 user file が対象となり、
//! Wishlist にマッチしたファイルはレポート上で「お客様優先データ」として強調表示される。
//!
//! ## 設計方針
//!
//! - **FS 独立**: 本クレートは fs-ntfs / fs-exfat / fs-fat32 に依存しない。
//!   `From<&NtfsFile> for FileInfo` のような変換は各 FS クレート側に置く
//!   （依存方向: fs-ntfs → wish-match の単方向）。
//! - **JSON 互換**: `Wishlist` / `Wish` / `WishItem` / `Priority` / `ExclusionList`
//!   はすべて `serde::{Serialize, Deserialize}` 実装。将来の Tauri UI 連携で
//!   JSON ファイルとして受け渡す。
//! - **大文字小文字非区別**: ASCII 範囲のみ（Phase 1 制約）。
//!
//! 関連 FR: FR-REC-01 (目標優先抽出), FR-REC-05 (全件復旧), FR-REC-06 (システム除外),
//! FR-WISH-01 (希望リスト管理), FR-WISH-02 (パターン突合)。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod error;
pub mod exclusion;
pub mod file_info;
pub mod matcher;
pub mod wishlist;

pub use error::WishMatchError;
pub use exclusion::{ExclusionList, ExclusionPattern};
pub use file_info::FileInfo;
pub use matcher::{match_file, match_files, matches_item, matches_wish, MatchResult};
pub use wishlist::{Priority, Wish, WishItem, Wishlist};
