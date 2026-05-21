//! # dds-wish-match
//!
//! Chunk 15: お客様の希望リスト × ファイル群のパターンマッチエンジン基盤。
//!
//! NTFS 技術実装層 (Chunks 4-14) の上に乗る、業務統合層の最初の一歩。
//! 各 FS リーダが提供するファイル情報を [`FileInfo`] という FS 非依存の汎用型に
//! 揃え、お客様の `Wishlist`（希望リスト）と突合する。マッチ結果は優先度スコア
//! 降順でソートされ、Chunk 17 の復旧パイプラインが「優先抽出順」を決めるのに使う。
//!
//! ## 設計方針
//!
//! - **FS 独立**: 本クレートは fs-ntfs / fs-exfat / fs-fat32 に依存しない。
//!   `From<&NtfsFile> for FileInfo` のような変換は各 FS クレート側に置く
//!   （依存方向: fs-ntfs → wish-match の単方向）。
//! - **JSON 互換**: `Wishlist` / `Wish` / `WishItem` / `Priority` はすべて
//!   `serde::{Serialize, Deserialize}` 実装。将来の Tauri UI 連携で
//!   希望リストを JSON ファイルとして受け渡す。
//! - **大文字小文字非区別**: ASCII 範囲のみ（Phase 1 制約）。
//!
//! 関連 FR: FR-REC-01 (目標優先抽出), FR-WISH-01 (希望リスト管理), FR-WISH-02 (パターン突合)。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod error;
pub mod file_info;
pub mod matcher;
pub mod wishlist;

pub use error::WishMatchError;
pub use file_info::FileInfo;
pub use matcher::{match_file, match_files, matches_item, matches_wish, MatchResult};
pub use wishlist::{Priority, Wish, WishItem, Wishlist};
