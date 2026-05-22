//! Chunk 21: 案件 JSON 永続化レイヤ `CaseStorage`。
//!
//! 案件は `{base_dir}/{案件番号}/case.json` というディレクトリ構造で保存される。
//! 案件ディレクトリ配下に Chunk 23 で出力構造（復旧データ、レポート等）を追加していく前提。
//!
//! 業務的制約:
//! - 既存案件の `create_new` はエラー (Q28: 上書きしない)
//! - `save` は呼び出すたびに `updated_at` を現在時刻で自動更新
//! - `list_all` は不正なディレクトリ名（CaseId としてパースできない）をスキップ
//!
//! 関連 FR: FR-CASE-03 (案件情報の永続化), FR-CASE-04 (1 PC 1 案件専有)。

use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

use crate::case::Case;
use crate::case_id::CaseId;
use crate::error::CaseError;

/// 案件 JSON ストレージ。
///
/// `base_dir` 配下に案件番号ごとのサブディレクトリを作成し、各 `case.json` を読み書きする。
pub struct CaseStorage {
    base_dir: PathBuf,
}

impl CaseStorage {
    /// 業務標準の保存先 `C:\cases` を使う `CaseStorage` を生成。
    ///
    /// テスト時は `with_base_dir` で `tempfile::TempDir` を使うこと。
    pub fn default_location() -> Self {
        Self {
            base_dir: PathBuf::from("C:\\cases"),
        }
    }

    /// 任意の base ディレクトリを指定して `CaseStorage` を生成。
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// `base_dir` への参照を返す。
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// 指定案件の `case.json` への絶対パスを返す。ファイルが実在するとは限らない。
    pub fn case_file_path(&self, case_id: &CaseId) -> PathBuf {
        self.base_dir.join(case_id.as_str()).join("case.json")
    }

    /// 指定案件のディレクトリパスを返す。ディレクトリが実在するとは限らない。
    pub fn case_dir(&self, case_id: &CaseId) -> PathBuf {
        self.base_dir.join(case_id.as_str())
    }

    /// 新規案件を作成して `case.json` に保存する。
    ///
    /// # エラー
    /// 既存の `case.json` がある場合は `CaseAlreadyExists`（Q28 業務確定動作）。
    pub fn create_new(&self, case_id: CaseId) -> Result<Case, CaseError> {
        let path = self.case_file_path(&case_id);
        if path.exists() {
            return Err(CaseError::CaseAlreadyExists {
                case_id: case_id.as_str().to_string(),
            });
        }
        let case = Case::new(case_id);
        self.save(&case)?;
        Ok(case)
    }

    /// 既存案件の `case.json` を読み込む。
    ///
    /// # エラー
    /// ファイルが存在しない場合は `CaseNotFound`。
    pub fn load(&self, case_id: &CaseId) -> Result<Case, CaseError> {
        let path = self.case_file_path(case_id);
        if !path.exists() {
            return Err(CaseError::CaseNotFound {
                case_id: case_id.as_str().to_string(),
            });
        }
        let json = fs::read_to_string(&path)?;
        let case: Case = serde_json::from_str(&json)?;
        Ok(case)
    }

    /// 案件を保存する。`updated_at` を現在時刻で自動更新したコピーを書き出す。
    ///
    /// 呼び出し側の `case.updated_at` は変更されない（`&Case` を取って内部で clone）。
    /// 親ディレクトリが存在しない場合は再帰的に作成する。
    pub fn save(&self, case: &Case) -> Result<(), CaseError> {
        let path = self.case_file_path(&case.case_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut updated = case.clone();
        updated.updated_at = Utc::now();
        let json = serde_json::to_string_pretty(&updated)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// 案件ファイル `case.json` を削除する（案件ディレクトリ自体は残る）。
    ///
    /// # エラー
    /// ファイルが存在しない場合は `CaseNotFound`。
    pub fn delete(&self, case_id: &CaseId) -> Result<(), CaseError> {
        let path = self.case_file_path(case_id);
        if !path.exists() {
            return Err(CaseError::CaseNotFound {
                case_id: case_id.as_str().to_string(),
            });
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    /// `base_dir` 配下の全案件番号をソートして返す。
    ///
    /// - `base_dir` が存在しない場合は空 `Vec`
    /// - ディレクトリ名が `CaseId` パースに失敗するものはスキップ
    /// - `case.json` が無いディレクトリもスキップ
    pub fn list_all(&self) -> Result<Vec<CaseId>, CaseError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let mut cases = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let Ok(case_id) = CaseId::parse(&name) else {
                continue;
            };
            let case_file = self.case_file_path(&case_id);
            if !case_file.exists() {
                continue;
            }
            cases.push(case_id);
        }
        cases.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(cases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, CaseStorage) {
        let temp = TempDir::new().unwrap();
        let storage = CaseStorage::with_base_dir(temp.path());
        (temp, storage)
    }

    #[test]
    fn create_new_case_succeeds_when_not_exists() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-01").unwrap();
        let case = storage.create_new(id.clone()).unwrap();
        assert_eq!(case.case_id, id);
        assert!(storage.case_file_path(&id).exists());
    }

    #[test]
    fn create_new_case_fails_when_already_exists() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-02").unwrap();
        storage.create_new(id.clone()).unwrap();
        let err = storage.create_new(id).unwrap_err();
        assert!(matches!(err, CaseError::CaseAlreadyExists { .. }));
    }

    #[test]
    fn save_creates_case_directory_structure() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-03").unwrap();
        let case = Case::new(id.clone());
        storage.save(&case).unwrap();
        assert!(storage.case_dir(&id).is_dir());
        assert!(storage.case_file_path(&id).is_file());
    }

    #[test]
    fn load_returns_saved_data() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-04").unwrap();
        let mut case = Case::new(id.clone());
        case.diagnostic_input.notes = "テスト案件".into();
        storage.save(&case).unwrap();

        let loaded = storage.load(&id).unwrap();
        assert_eq!(loaded.case_id, id);
        assert_eq!(loaded.diagnostic_input.notes, "テスト案件");
    }

    #[test]
    fn save_updates_updated_at_timestamp() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-05").unwrap();
        let case = storage.create_new(id.clone()).unwrap();
        let original_updated = case.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        storage.save(&case).unwrap();
        let loaded = storage.load(&id).unwrap();
        assert!(loaded.updated_at > original_updated);
    }

    #[test]
    fn load_nonexistent_returns_not_found() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-99").unwrap();
        let err = storage.load(&id).unwrap_err();
        assert!(matches!(err, CaseError::CaseNotFound { .. }));
    }

    #[test]
    fn delete_removes_case_file() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-06").unwrap();
        storage.create_new(id.clone()).unwrap();
        assert!(storage.case_file_path(&id).exists());
        storage.delete(&id).unwrap();
        assert!(!storage.case_file_path(&id).exists());
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let (_t, storage) = setup();
        let id = CaseId::parse("260522-07").unwrap();
        let err = storage.delete(&id).unwrap_err();
        assert!(matches!(err, CaseError::CaseNotFound { .. }));
    }

    #[test]
    fn list_all_returns_sorted_case_ids() {
        let (_t, storage) = setup();
        storage
            .create_new(CaseId::parse("260522-03").unwrap())
            .unwrap();
        storage
            .create_new(CaseId::parse("260522-01").unwrap())
            .unwrap();
        storage
            .create_new(CaseId::parse("260522-02").unwrap())
            .unwrap();
        let list = storage.list_all().unwrap();
        let names: Vec<_> = list.iter().map(|c| c.as_str().to_string()).collect();
        assert_eq!(names, vec!["260522-01", "260522-02", "260522-03"]);
    }

    #[test]
    fn list_all_ignores_invalid_directory_names() {
        let (temp, storage) = setup();
        storage
            .create_new(CaseId::parse("260522-08").unwrap())
            .unwrap();
        fs::create_dir_all(temp.path().join("not-a-case")).unwrap();
        fs::create_dir_all(temp.path().join("INVALID01")).unwrap();
        let list = storage.list_all().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].as_str(), "260522-08");
    }

    #[test]
    fn list_all_returns_empty_when_base_dir_missing() {
        let storage =
            CaseStorage::with_base_dir(PathBuf::from("C:\\definitely-does-not-exist-260522-zz"));
        let list = storage.list_all().unwrap();
        assert!(list.is_empty());
    }
}
