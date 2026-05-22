//! # dds-case-manager
//!
//! Chunk 21: DDS の案件管理基盤クレート（Phase 1.5 開始チャンク）。
//!
//! 1 案件 = 1 つの [`Case`] = `C:\cases\{案件番号}\case.json` ファイル 1 つ。
//! 案件は yymmdd-NN 形式の [`CaseId`] で識別される。
//!
//! ## 業務的な責務
//! - 案件単位の情報管理（診断結果 / お客様希望リスト / 復旧結果サマリ）
//! - 案件番号の形式バリデーション
//! - 案件 JSON の CRUD（[`CaseStorage`] 経由）
//!
//! ## 業務的に**担わない**こと
//! 以下は CRM や上位層（CLI / UI / DB）の責務:
//! - 顧客情報の管理
//! - 案件番号の採番
//! - 進捗管理 / ステータス遷移
//! - 案件横断の検索 / 集計 / 履歴
//!
//! 「DDS は 1 PC 1 案件専有のフロー」「CRM が案件全体管理」という業務実態に合わせ、
//! 本クレートは**薄い層**として「現在進行中の 1 案件の情報を JSON で永続化」のみを担う。
//!
//! ## 依存方向
//! `case-manager → wish-match → core` の単方向のみ。
//! `recovery` / `report` / `fs-ntfs` / `validators` には**依存しない**（整合性は CLI / UI 層で取る）。
//!
//! 関連 FR: FR-CASE-01 ~ FR-CASE-04。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod case;
pub mod case_id;
pub mod diagnostic;
pub mod error;
pub mod storage;
pub mod symptom;

pub use case::{Case, RecoveryReportSummary};
pub use case_id::CaseId;
pub use diagnostic::{DeletedFileStats, DiagnosticInput, RecoverabilityEstimate};
pub use error::CaseError;
pub use storage::CaseStorage;
pub use symptom::{FsAnomaly, Symptom};
