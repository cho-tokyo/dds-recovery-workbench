//! Chunk 14: NTFS の 1 つのファイル/ディレクトリを 1 つの owned 型 [`NtfsFile`] に統合する高
//! レベル API。Chunks 4-13 で構築した個別パーサ群（`parse_mft_entry` / `find_best_file_name` /
//! `parse_standard_information` / `extract_main_data_stream` / `PathResolver`）を 1 つの
//! ビルダ関数 `build_file_for_record` に集約し、`Vec<NtfsFile>` で集めて後段の業務統合層
//! （wish-match, recovery）に引き渡せる形にする。Phase 1 NTFS リーダー実装の最終形。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05, FR-LIVE-06, FR-REC-01, FR-REC-04。
use crate::attribute::{AttributeHeader, AttributeType};
use crate::attributes::{
    extract_all_data_streams, find_attribute, find_best_file_name, parse_runlist,
    parse_standard_information, DataContent, FileAttributes, FileName, MftReference, Run,
};
use crate::mft::MftEntry;
use crate::path::PathResolver;
use crate::volume::{NtfsVolume, VolumeError};
use chrono::{DateTime, Utc};

/// `NtfsFile` が保持するファイル内容情報（owned）。
///
/// 常駐の場合はバイト列を直接所有し、非常駐の場合は読み取りに必要な `runs` と論理サイズ
/// `real_size` のみ保持する。実バイト列の取得は `NtfsVolume::read_file_content` で行う
/// （メモリ効率: 列挙時には実データを読まず、必要なファイルだけ後で読む設計）。
/// 関連 FR: FR-LIVE-01, FR-REC-04。
#[derive(Debug, Clone)]
pub enum FileContentRef {
    /// MFT エントリ内に格納された小ファイル。実バイト列を直接保持。
    Resident(Vec<u8>),
    /// クラスタに分散保存された大ファイル。実バイトは `runs` を辿って読む。
    NonResident {
        /// `$DATA` 非常駐コンテンツの論理サイズ（バイト）。
        real_size: u64,
        /// runlist デコード済みのラン列（VCN 連続順）。
        runs: Vec<Run>,
    },
    /// `$DATA` 属性なし（ディレクトリ・$MFT 等のメタファイル・空ファイル）。
    None,
}

impl FileContentRef {
    /// 常駐かどうか。
    pub fn is_resident(&self) -> bool {
        matches!(self, FileContentRef::Resident(_))
    }
    /// 論理サイズ（バイト）。常駐は実バイト列長、非常駐は `real_size`、None は 0。
    pub fn size(&self) -> u64 {
        match self {
            FileContentRef::Resident(bytes) => bytes.len() as u64,
            FileContentRef::NonResident { real_size, .. } => *real_size,
            FileContentRef::None => 0,
        }
    }
}

/// 1 つの NTFS ファイル/ディレクトリの統合情報（owned）。
///
/// MFT エントリから抽出した全情報を 1 つの所有データ型に束ねる。ライフタイムを持たないため
/// `Vec<NtfsFile>` に集めて後段モジュール（wish-match, recovery）へそのまま受け渡せる。
/// 関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05, FR-LIVE-06。
#[derive(Debug, Clone)]
pub struct NtfsFile {
    /// MFT エントリ番号（このファイルの一意 ID）。
    pub record_index: u64,
    /// NTFS 形式のフルパス（例: `\dir1\sub2\file.txt`）。ルートは `\`。
    pub path: String,
    /// ファイル名のみ（例: `file.txt`）。`find_best_file_name` の選択結果。
    pub name: String,
    /// 親ディレクトリの MFT 参照（パス再構築・ハードリンク列挙用）。
    pub parent: MftReference,
    /// ディレクトリかどうか（MFT ヘッダのフラグ由来）。
    pub is_directory: bool,
    /// 削除済みエントリ（In Use フラグ = 0）かどうか。
    pub is_deleted: bool,
    /// 作成日時（`$STANDARD_INFORMATION` 由来、欠落時は `$FILE_NAME` で代替）。
    pub created: Option<DateTime<Utc>>,
    /// 内容更新日時。
    pub modified: Option<DateTime<Utc>>,
    /// アクセス日時。
    pub accessed: Option<DateTime<Utc>>,
    /// MFT エントリ自体の更新日時。
    pub mft_modified: Option<DateTime<Utc>>,
    /// DOS ファイル属性フラグ（`$SI` が無ければ `$FILE_NAME` 由来）。
    pub file_attributes: FileAttributes,
    /// Alternate Data Stream を持つか。
    pub has_alternate_streams: bool,
    /// 圧縮属性が立っているか（メイン `$DATA` の flags）。
    pub is_compressed: bool,
    /// 暗号化属性が立っているか。
    pub is_encrypted: bool,
    /// スパース属性が立っているか。
    pub is_sparse: bool,
    /// メイン `$DATA` ストリーム内容の参照（実バイトは [`NtfsVolume::read_file_content`] で取得）。
    pub content: FileContentRef,
    /// ファイルサイズ（メイン `$DATA` の `real_size` or `content_size`、無ければ 0）。
    pub size: u64,
}

impl NtfsFile {
    /// NTFS のルートディレクトリ（MFT entry 5）かどうか。
    pub fn is_root(&self) -> bool {
        self.record_index == 5
    }
    /// システムメタファイル（MFT entry 0〜23）かどうか。書籍 Ch.13: 0〜15 が予約、16〜23 が拡張用予約。
    pub fn is_system_metafile(&self) -> bool {
        self.record_index < 24
    }
    /// ユーザファイルかどうか（削除済みでもユーザファイル扱い、復旧対象）。
    pub fn is_user_file(&self) -> bool {
        !self.is_directory && !self.is_system_metafile()
    }
    /// ファイル拡張子（小文字、ドット除く）。なければ `None`。
    pub fn extension(&self) -> Option<String> {
        self.name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_lowercase())
    }
    /// 復旧優先度判定用: 削除 + ユーザファイル + 非圧縮 + 非暗号化のみ true。
    pub fn is_simple_deleted_user_file(&self) -> bool {
        self.is_deleted && self.is_user_file() && !self.is_compressed && !self.is_encrypted
    }
    /// 名前が `$` で始まるか（システムファイルまたはゴミ箱内ファイル）。
    ///
    /// 注意: `$RECYCLE.BIN` 配下の削除済みユーザファイル（`$I*` / `$R*` 命名）も該当する。
    /// このメソッドだけで「ユーザファイル除外」してはいけない。業務統合層がオプトインで
    /// 使うフィルタ。関連 FR: FR-LIVE-05 (削除エントリ可視化)。
    pub fn has_system_name_prefix(&self) -> bool {
        self.name.starts_with('$')
    }
}

impl From<&NtfsFile> for dds_wish_match::FileInfo {
    /// `NtfsFile` を FS 非依存の [`dds_wish_match::FileInfo`] に変換する。
    ///
    /// `source_id` には `"NTFS#<record_index>"` 形式の識別子を設定し、復旧フェーズで
    /// 原本 MFT エントリを再特定できるようにする。関連 FR: FR-REC-01 (目標優先抽出)。
    fn from(file: &NtfsFile) -> Self {
        dds_wish_match::FileInfo {
            path: file.path.clone(),
            name: file.name.clone(),
            extension: file.extension(),
            size: file.size,
            created: file.created,
            modified: file.modified,
            accessed: file.accessed,
            is_deleted: file.is_deleted,
            is_directory: file.is_directory,
            source_id: format!("NTFS#{}", file.record_index),
        }
    }
}

/// 指定 MFT エントリから [`NtfsFile`] を構築する内部ヘルパ。
///
/// 戻り値:
/// - `Ok(Some(file))`: 構築成功。
/// - `Ok(None)`: `$FILE_NAME` 属性なし（未使用エントリ等）→ 呼び出し側でスキップ推奨。
/// - `Err(e)`: パースエラー。
pub(crate) fn build_file_for_record<F>(
    volume: &mut NtfsVolume<F>,
    record_index: u64,
    resolver: &mut PathResolver,
) -> Result<Option<NtfsFile>, VolumeError>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let entry = volume.read_record(record_index)?;
    let first_attr_offset = entry.header.first_attribute_offset as usize;
    let Some(fn_) = find_best_file_name(&entry.data, first_attr_offset) else {
        return Ok(None);
    };
    let is_deleted = entry.header.is_deleted();
    let is_directory = entry.header.is_directory();
    let parent = fn_.parent_directory;
    let name = fn_.filename.clone();
    // ルート(5)はリゾルバキャッシュで即返却。削除済みは親が再利用済みで失敗の余地があるので
    // ファイル名のみのフォールバック (`\<name>`) で補完する（部分復旧の一形態として許容）。
    let path = if record_index == 5 {
        "\\".to_string()
    } else {
        resolver
            .resolve(record_index, volume)
            .unwrap_or_else(|_| format!("\\{}", name))
    };
    let (created, modified, accessed, mft_modified, file_attrs) =
        extract_si_or_fallback(&entry, first_attr_offset, &fn_);
    let data_streams = extract_all_data_streams(&entry.data, first_attr_offset);
    let main_stream = data_streams.iter().find(|s| s.name.is_empty());
    let has_alternate_streams = data_streams.iter().any(|s| !s.name.is_empty());
    let (content, size, is_compressed, is_encrypted, is_sparse) = match main_stream {
        Some(stream) => {
            let (content, size) = match &stream.content {
                DataContent::Resident { bytes, size } => {
                    (FileContentRef::Resident(bytes.to_vec()), u64::from(*size))
                }
                DataContent::NonResident {
                    real_size,
                    runlist_offset_in_attr,
                    attribute_raw,
                    ..
                } => {
                    // 即時 runlist パース。`read_file_content` 時に再パースしない設計。
                    let runlist_bytes = attribute_raw.get(*runlist_offset_in_attr..).ok_or(
                        VolumeError::Runlist(crate::attributes::RunlistError::BufferTooSmall {
                            got: attribute_raw.len(),
                        }),
                    )?;
                    let runs = parse_runlist(runlist_bytes)?;
                    (
                        FileContentRef::NonResident {
                            real_size: *real_size,
                            runs,
                        },
                        *real_size,
                    )
                }
            };
            (
                content,
                size,
                stream.is_compressed,
                stream.is_encrypted,
                stream.is_sparse,
            )
        }
        None => (FileContentRef::None, 0, false, false, false),
    };
    Ok(Some(NtfsFile {
        record_index,
        path,
        name,
        parent,
        is_directory,
        is_deleted,
        created,
        modified,
        accessed,
        mft_modified,
        file_attributes: file_attrs,
        has_alternate_streams,
        is_compressed,
        is_encrypted,
        is_sparse,
        content,
        size,
    }))
}

/// (created, modified, accessed, mft_modified, file_attributes) のタプル型。
/// `extract_si_or_fallback` の戻り値専用。
type TimestampsAndAttrs = (
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    FileAttributes,
);

/// `$STANDARD_INFORMATION` 抽出。欠落・破損時は `$FILE_NAME` 内のタイムスタンプで代替。
fn extract_si_or_fallback(
    entry: &MftEntry,
    first_attr_offset: usize,
    fn_: &FileName,
) -> TimestampsAndAttrs {
    let fallback = || {
        (
            fn_.created.to_datetime(),
            fn_.modified.to_datetime(),
            fn_.accessed.to_datetime(),
            fn_.mft_modified.to_datetime(),
            fn_.file_attributes,
        )
    };
    let Some(si_attr) = find_attribute(
        &entry.data,
        first_attr_offset,
        AttributeType::StandardInformation,
    ) else {
        return fallback();
    };
    let AttributeHeader::Resident { resident, .. } = &si_attr.header else {
        return fallback();
    };
    let content_start = resident.content_offset as usize;
    let Some(content_end) = content_start.checked_add(resident.content_size as usize) else {
        return fallback();
    };
    if content_end > si_attr.raw.len() {
        return fallback();
    }
    match parse_standard_information(&si_attr.raw[content_start..content_end]) {
        Ok(si) => (
            si.created.to_datetime(),
            si.modified.to_datetime(),
            si.accessed.to_datetime(),
            si.mft_modified.to_datetime(),
            si.file_attributes,
        ),
        Err(_) => fallback(),
    }
}

/// 全 `NtfsFile` を順次列挙するイテレータ。
///
/// `$FILE_NAME` 属性のないエントリは自動的にスキップ。個別エントリのパースエラーは `Result` として
/// yield され、イテレーション自体は継続（破損耐性、復旧ソフトとして必須）。
/// `PathResolver` を内部で 1 つだけ持ちキャッシュ共有することで N ファイル列挙を実用上 O(N) に保つ。
pub struct NtfsFileIterator<'a, F> {
    volume: &'a mut NtfsVolume<F>,
    current: u64,
    resolver: PathResolver,
}

impl<'a, F> NtfsFileIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// 新規イテレータを生成。`current=0` から `volume.total_records()` まで走査する。
    pub fn new(volume: &'a mut NtfsVolume<F>) -> Self {
        Self {
            volume,
            current: 0,
            resolver: PathResolver::new(),
        }
    }
}

impl<'a, F> Iterator for NtfsFileIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    type Item = Result<NtfsFile, VolumeError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current >= self.volume.total_records() {
                return None;
            }
            let idx = self.current;
            self.current += 1;
            match build_file_for_record(self.volume, idx, &mut self.resolver) {
                Ok(Some(file)) => return Some(Ok(file)),
                Ok(None) => continue,
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::FileAttributes as FA;
    use chrono::TimeZone;

    fn make_file(
        record_index: u64,
        name: &str,
        is_directory: bool,
        is_deleted: bool,
        is_compressed: bool,
        is_encrypted: bool,
    ) -> NtfsFile {
        NtfsFile {
            record_index,
            path: format!("\\{}", name),
            name: name.to_string(),
            parent: MftReference {
                entry_number: 5,
                sequence_number: 1,
            },
            is_directory,
            is_deleted,
            created: None,
            modified: None,
            accessed: None,
            mft_modified: None,
            file_attributes: FA(0),
            has_alternate_streams: false,
            is_compressed,
            is_encrypted,
            is_sparse: false,
            content: FileContentRef::None,
            size: 0,
        }
    }

    #[test]
    fn is_root_returns_true_for_record_5() {
        assert!(make_file(5, "any", false, false, false, false).is_root());
        assert!(!make_file(6, "any", false, false, false, false).is_root());
        assert!(!make_file(0, "any", false, false, false, false).is_root());
    }

    #[test]
    fn is_system_metafile_for_records_0_to_23() {
        assert!(make_file(0, "$MFT", false, false, false, false).is_system_metafile());
        assert!(make_file(23, "x", false, false, false, false).is_system_metafile());
        assert!(!make_file(24, "x", false, false, false, false).is_system_metafile());
        assert!(!make_file(100, "user.txt", false, false, false, false).is_system_metafile());
    }

    #[test]
    fn is_user_file_excludes_directory_and_system() {
        // ディレクトリは false
        assert!(!make_file(100, "dir", true, false, false, false).is_user_file());
        // システムは false
        assert!(!make_file(5, "x", false, false, false, false).is_user_file());
        // 通常ユーザファイルは true
        assert!(make_file(100, "x", false, false, false, false).is_user_file());
        // 削除済みでも user file 扱い（復旧対象）
        assert!(make_file(100, "x", false, true, false, false).is_user_file());
    }

    #[test]
    fn extension_basic_cases() {
        assert_eq!(
            make_file(100, "foo.txt", false, false, false, false).extension(),
            Some("txt".to_string())
        );
        assert_eq!(
            make_file(100, "foo.TXT", false, false, false, false).extension(),
            Some("txt".to_string())
        );
        assert_eq!(
            make_file(100, "foo", false, false, false, false).extension(),
            None
        );
        assert_eq!(
            make_file(100, "foo.tar.gz", false, false, false, false).extension(),
            Some("gz".to_string())
        );
    }

    #[test]
    fn is_simple_deleted_user_file_combinations() {
        // 全て満たす → true
        assert!(make_file(100, "x", false, true, false, false).is_simple_deleted_user_file());
        // 非削除 → false
        assert!(!make_file(100, "x", false, false, false, false).is_simple_deleted_user_file());
        // 削除 + 圧縮 → false
        assert!(!make_file(100, "x", false, true, true, false).is_simple_deleted_user_file());
        // 削除 + 暗号化 → false
        assert!(!make_file(100, "x", false, true, false, true).is_simple_deleted_user_file());
        // 削除 + ディレクトリ → user_file ではないので false
        assert!(!make_file(100, "x", true, true, false, false).is_simple_deleted_user_file());
        // 削除 + システム → false
        assert!(!make_file(5, "x", false, true, false, false).is_simple_deleted_user_file());
    }

    #[test]
    fn file_content_ref_size_correct() {
        assert_eq!(FileContentRef::Resident(vec![0u8; 50]).size(), 50);
        assert_eq!(
            FileContentRef::NonResident {
                real_size: 1024,
                runs: vec![],
            }
            .size(),
            1024
        );
        assert_eq!(FileContentRef::None.size(), 0);
        assert_eq!(FileContentRef::Resident(Vec::new()).size(), 0);
    }

    #[test]
    fn file_content_ref_is_resident() {
        assert!(FileContentRef::Resident(vec![1, 2]).is_resident());
        assert!(!FileContentRef::NonResident {
            real_size: 100,
            runs: vec![],
        }
        .is_resident());
        assert!(!FileContentRef::None.is_resident());
    }

    // Chunk 15: NtfsFile 業務統合層向け拡張のテスト ----------------------------

    #[test]
    fn has_system_name_prefix_true_for_mft() {
        let f = make_file(0, "$MFT", false, false, false, false);
        assert!(f.has_system_name_prefix());
    }

    #[test]
    fn has_system_name_prefix_true_for_recycle_bin_entries() {
        // $RECYCLE.BIN 配下の削除済みファイル命名規約 ($I* / $R*) も該当。
        let f_i = make_file(200, "$IABC123.docx", false, false, false, false);
        let f_r = make_file(201, "$RABC123.docx", false, false, false, false);
        assert!(f_i.has_system_name_prefix());
        assert!(f_r.has_system_name_prefix());
    }

    #[test]
    fn has_system_name_prefix_false_for_normal_files() {
        let f = make_file(100, "report.docx", false, false, false, false);
        assert!(!f.has_system_name_prefix());
    }

    #[test]
    fn from_ntfs_file_to_file_info_preserves_all_fields() {
        let mut ntfs = make_file(67, "report.docx", false, false, false, false);
        ntfs.path = "\\Users\\Chou\\report.docx".to_string();
        ntfs.size = 4096;
        let ts = chrono::Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
        ntfs.created = Some(ts);
        ntfs.modified = Some(ts);
        ntfs.accessed = Some(ts);
        let fi: dds_wish_match::FileInfo = (&ntfs).into();
        assert_eq!(fi.path, "\\Users\\Chou\\report.docx");
        assert_eq!(fi.name, "report.docx");
        assert_eq!(fi.extension, Some("docx".to_string()));
        assert_eq!(fi.size, 4096);
        assert_eq!(fi.created, Some(ts));
        assert_eq!(fi.modified, Some(ts));
        assert_eq!(fi.accessed, Some(ts));
        assert!(!fi.is_deleted);
        assert!(!fi.is_directory);
    }

    #[test]
    fn from_ntfs_file_sets_correct_source_id() {
        let ntfs = make_file(67, "foo.txt", false, true, false, false);
        let fi: dds_wish_match::FileInfo = (&ntfs).into();
        assert_eq!(fi.source_id, "NTFS#67");
        assert!(fi.is_deleted);
    }
}
