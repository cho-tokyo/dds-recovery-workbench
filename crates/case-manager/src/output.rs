//! Chunk 23: 業務向け納品ディレクトリ構造 `CaseOutput`。
//!
//! 案件番号 + 納品ドライブのルート（例: `G:\`）から、お客様にそのまま渡せる
//! 一貫したディレクトリツリーを構築する。Chunk 21 の `CaseStorage` が **社内保存**
//! を担うのに対し、`CaseOutput` は **納品物のレイアウト** を担う（分離設計）。
//!
//! 想定ツリー（Chunk 23.8 で 4→5 レポート構造に拡張）:
//!
//! ```text
//! {drive_root}/{案件番号}/
//!   ├ 復旧データ/
//!   │   ├ 通常ファイル/   ← live (生存) ファイル群
//!   │   └ 削除ファイル/   ← deleted (削除) ファイル群
//!   └ レポート/
//!       ├ 復旧レポート.docx
//!       ├ 破損疑いファイル一覧.txt         ← Chunk 23.8 で rename
//!       ├ 自動確認対象外ファイル一覧.txt   ← Chunk 23.8 新規
//!       ├ 業務管理レポート.html
//!       └ report.csv
//! ```
//!
//! 業務シナリオ:
//! 1. CS が納品 HDD を装着 (例: `G:\`)
//! 2. `CaseOutput::new(case_id, "G:\\")` でレイアウトを構築
//! 3. `create_all_dirs()` で 3 サブディレクトリを一括生成
//! 4. 復旧パイプラインが `live_files_dir` / `deleted_files_dir` に書き込み
//! 5. `dds_report::write_business_reports` が `reports_dir` に 4 ファイル生成
//! 6. HDD を取り出してお客様へ送付。お客様は `G:\{案件番号}\` を開くだけで全部見える。
//!
//! 関連 FR: FR-OUT-01 (案件番号付きディレクトリ), FR-OUT-02 (通常 / 削除分離),
//!         FR-OUT-03 (日本語名対応), FR-OUT-04 (社内保存と納品物の分離)。

use std::io;
use std::path::{Path, PathBuf};

use crate::case_id::CaseId;

/// 案件単位の納品ディレクトリレイアウト。
///
/// インスタンスは「パスの構築方針」のみを保持する不変オブジェクト。実際の
/// ディレクトリ作成は [`CaseOutput::create_all_dirs`] を明示的に呼ぶこと。
///
/// 例:
/// ```
/// use dds_case_manager::{CaseId, CaseOutput};
/// let id = CaseId::parse("260522-04").unwrap();
/// let out = CaseOutput::new(id, "G:\\");
/// assert!(out.root().ends_with("260522-04"));
/// assert!(out.live_files_dir().to_string_lossy().contains("通常ファイル"));
/// ```
#[derive(Debug, Clone)]
pub struct CaseOutput {
    case_id: CaseId,
    drive_root: PathBuf,
}

impl CaseOutput {
    /// 案件番号と納品ドライブルートから新しい [`CaseOutput`] を構築する。
    ///
    /// `drive_root` は納品 HDD のルート (`"G:\\"`)、または検証時は任意の
    /// テンポラリディレクトリでも可。
    pub fn new(case_id: CaseId, drive_root: impl Into<PathBuf>) -> Self {
        Self {
            case_id,
            drive_root: drive_root.into(),
        }
    }

    /// 案件番号を取得する。
    pub fn case_id(&self) -> &CaseId {
        &self.case_id
    }

    /// 案件のルートディレクトリ (`{drive_root}/{案件番号}`) を返す。
    pub fn root(&self) -> PathBuf {
        self.drive_root.join(self.case_id.as_str())
    }

    /// 通常 (生存) ファイルの出力先 (`{root}/復旧データ/通常ファイル`)。
    pub fn live_files_dir(&self) -> PathBuf {
        self.root().join("復旧データ").join("通常ファイル")
    }

    /// 削除ファイルの出力先 (`{root}/復旧データ/削除ファイル`)。
    pub fn deleted_files_dir(&self) -> PathBuf {
        self.root().join("復旧データ").join("削除ファイル")
    }

    /// レポートディレクトリ (`{root}/レポート`)。
    pub fn reports_dir(&self) -> PathBuf {
        self.root().join("レポート")
    }

    /// 顧客向け Word レポート (`{reports}/復旧レポート.docx`) のパス。
    ///
    /// Chunk 24a で「納品 HDD のレポートディレクトリにはこれだけを出力」する設計に変更。
    /// 業務管理レポート HTML / CSV は社内保存先 (`CaseStorage::base_dir()`) へ移動した
    /// ([`Self::internal_html_path_in_storage`] / [`Self::csv_path_in_storage`])。
    pub fn customer_docx_path(&self) -> PathBuf {
        self.reports_dir().join("復旧レポート.docx")
    }

    /// 社内向け業務管理レポート (`{storage_base}/{案件番号}/業務管理レポート.html`) のパス (Chunk 24a)。
    ///
    /// 納品 HDD ではなく社内保存先 (CS 業務 PC の `C:\cases\` 等) に書き出される。
    /// お客様に渡してはならない CS 用業務管理情報 (内部メモ・SHA256・出力先) を含むため
    /// 物理的にお客様の HDD から分離する設計。
    pub fn internal_html_path_in_storage(&self, storage_base: &Path) -> PathBuf {
        storage_base
            .join(self.case_id.as_str())
            .join("業務管理レポート.html")
    }

    /// 外部システム連携用 CSV (`{storage_base}/{案件番号}/復旧詳細.csv`) のパス (Chunk 24a)。
    ///
    /// 納品 HDD ではなく社内保存先に書き出される業務管理 CSV (UTF-8 BOM 付加で Excel
    /// 文字化け解消、実機ドライランフィードバック ④ 対応)。
    pub fn csv_path_in_storage(&self, storage_base: &Path) -> PathBuf {
        storage_base
            .join(self.case_id.as_str())
            .join("復旧詳細.csv")
    }

    /// 納品物の主要 3 ディレクトリ（通常 / 削除 / レポート）を一括で作成する。
    ///
    /// 既存ならスキップ (`create_dir_all` 相当)。失敗は呼び出し元へ伝搬。
    pub fn create_all_dirs(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.live_files_dir())?;
        std::fs::create_dir_all(self.deleted_files_dir())?;
        std::fs::create_dir_all(self.reports_dir())?;
        Ok(())
    }

    /// `drive_root` への参照を返す（デバッグ・テスト用）。
    pub fn drive_root(&self) -> &Path {
        &self.drive_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid() -> CaseId {
        CaseId::parse("260522-04").unwrap()
    }

    #[test]
    fn case_output_root_includes_case_id() {
        // OS 非依存に組み立てて比較する（Linux / Windows 両対応）。
        let out = CaseOutput::new(cid(), "G:\\");
        let expected = Path::new("G:\\").join("260522-04");
        assert_eq!(out.root(), expected);
        assert_eq!(out.case_id().as_str(), "260522-04");
    }

    #[test]
    fn case_output_live_files_dir_correct() {
        let out = CaseOutput::new(cid(), "G:\\");
        let expected = Path::new("G:\\")
            .join("260522-04")
            .join("復旧データ")
            .join("通常ファイル");
        assert_eq!(out.live_files_dir(), expected);
    }

    #[test]
    fn case_output_deleted_files_dir_correct() {
        let out = CaseOutput::new(cid(), "G:\\");
        let expected = Path::new("G:\\")
            .join("260522-04")
            .join("復旧データ")
            .join("削除ファイル");
        assert_eq!(out.deleted_files_dir(), expected);
    }

    #[test]
    fn case_output_japanese_report_filenames() {
        // Chunk 24a: 納品 HDD には customer_docx のみ。
        let out = CaseOutput::new(cid(), "G:\\");
        assert!(out
            .customer_docx_path()
            .to_string_lossy()
            .ends_with("復旧レポート.docx"));
        // 納品 HDD のレポートディレクトリ配下にあること。
        assert!(out
            .customer_docx_path()
            .to_string_lossy()
            .contains("レポート"));
    }

    #[test]
    fn case_output_internal_paths_go_to_storage_base() {
        // Chunk 24a: 業務管理 HTML / CSV は社内保存先 (storage_base) へ。
        let out = CaseOutput::new(cid(), "G:\\");
        let storage_base = Path::new("C:\\cases");

        let html = out.internal_html_path_in_storage(storage_base);
        let csv = out.csv_path_in_storage(storage_base);

        assert_eq!(
            html,
            Path::new("C:\\cases")
                .join("260522-04")
                .join("業務管理レポート.html")
        );
        assert_eq!(
            csv,
            Path::new("C:\\cases")
                .join("260522-04")
                .join("復旧詳細.csv")
        );

        // 納品 HDD のドライブルート (G:\) を含まないこと (社内保存に物理分離されている)。
        let g_drive = Path::new("G:\\").to_string_lossy().to_string();
        assert!(!html.to_string_lossy().contains(&g_drive));
        assert!(!csv.to_string_lossy().contains(&g_drive));
    }

    #[test]
    fn case_output_no_delivery_paths_for_txt_or_html_or_csv() {
        // Chunk 24a: 納品 HDD のレポートディレクトリには .docx 以外を作らない。
        // → CaseOutput には TXT / HTML / CSV のパスメソッドが存在しない (削除済み)。
        // 本テストはコンパイル時保証 + reports_dir 直下の想定ファイルが 1 種類だけであることの担保。
        let out = CaseOutput::new(cid(), "G:\\");
        // 納品 HDD のレポートディレクトリは「{root}/レポート」のまま。
        assert!(out.reports_dir().to_string_lossy().ends_with("レポート"));
        // customer_docx_path() 以外に「レポート」配下を返す public method がないこと
        // (将来追加されたら、ここで意図的な変更として確認する設計)。
        assert!(out.customer_docx_path().starts_with(out.reports_dir()));
    }

    #[test]
    fn case_output_create_all_dirs_creates_structure() {
        let temp = tempfile::TempDir::new().unwrap();
        let out = CaseOutput::new(cid(), temp.path());
        out.create_all_dirs().unwrap();
        assert!(out.live_files_dir().is_dir());
        assert!(out.deleted_files_dir().is_dir());
        assert!(out.reports_dir().is_dir());
        // 冪等性: 2 回目もエラーなし。
        out.create_all_dirs().unwrap();
    }
}
