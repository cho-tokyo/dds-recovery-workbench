//! NTFS ボリュームの高レベル API。Chunks 4-10 の純粋関数群を束ね、`NtfsVolume::open(reader)`
//! で全 MFT エントリの列挙・ランダムアクセスを提供する。`disk-io` への直接依存を持たず、
//! `read_clusters: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>` クロージャ経由で
//! 後段（Chunk 13+）の `ReadOnlyDisk` ラッピングを可能にする疎結合設計。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04（部分）, FR-LIVE-05, FR-LIVE-06。
use crate::attribute::{AttributeError, AttributeHeader, AttributeType};
use crate::attributes::file_name::{FileName, MftReference};
use crate::attributes::{
    find_attribute, parse_entries_in_node, parse_index_root, parse_indx_block, parse_runlist,
    read_runs_with, IndexEntry, IndexError, Run, RunlistError,
};
use crate::boot_sector::{parse_boot_sector, BootSector, BootSectorError};
use crate::file::{build_file_for_record, FileContentRef, NtfsFile, NtfsFileIterator};
use crate::mft::{parse_mft_entry, MftEntry, MftError};
use thiserror::Error;

/// B+ ツリー走査時の最大深さ（破損データ防護）。NTFS 実用上 20 階層程度が上限なので
/// `32` は十分なマージン。超過時 [`VolumeError::BtreeTooDeep`]。
pub const MAX_BTREE_DEPTH: u32 = 32;

/// `NtfsVolume` 操作で発生し得るエラー。既存エラー型を `#[from]` で集約する。
/// `std::io::Error` を含むため `PartialEq` は派生しない（Chunk 10 `RunlistError` と同方針）。
#[derive(Error, Debug)]
#[allow(missing_docs)]
pub enum VolumeError {
    #[error("Boot sector error: {0}")]
    BootSector(#[from] BootSectorError),
    #[error("MFT entry error: {0}")]
    Mft(#[from] MftError),
    #[error("Attribute parse error: {0}")]
    Attribute(#[from] AttributeError),
    #[error("Runlist error: {0}")]
    Runlist(#[from] RunlistError),
    #[error("Disk I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// `$MFT` record 0 に `$DATA` 属性がない（破損ボリューム疑い、または $ATTRIBUTE_LIST 経由は Phase 1 非対応）。
    #[error(
        "$MFT record 0 has no $DATA attribute (corrupted volume or unsupported $ATTRIBUTE_LIST)"
    )]
    NoMftDataAttribute,
    /// `$MFT` の `$DATA` が常駐になっている異常（理論上ありえない）。
    #[error("$MFT $DATA attribute must be non-resident, got resident")]
    MftDataMustBeNonResident,
    /// `$MFT` runlist にスパースランが混入（$MFT は連続割当前提）。
    #[error("Unexpected sparse run in $MFT runlist")]
    SparseMftRun,
    /// MFT レコード index が `total_records` を超過。
    #[error("MFT record index out of range: {index} (total {total})")]
    RecordIndexOutOfRange { index: u64, total: u64 },
    /// 先頭バッファが 512 バイト未満でブートセクタ解析不能。
    #[error("Buffer too small for boot sector: got {got}")]
    BootSectorBufferTooSmall { got: usize },
    /// 指定 MFT エントリに `$INDEX_ROOT` 属性がない（ディレクトリではない）。
    #[error("Record {record_index} is not a directory ($INDEX_ROOT missing)")]
    NotADirectory { record_index: u64 },
    /// `$INDEX_ROOT` は仕様上常駐のはずだが非常駐になっていた。
    #[error("$INDEX_ROOT must be resident, got non-resident")]
    IndexRootNotResident,
    /// 子ノードフラグありなのに `$INDEX_ALLOCATION` 属性が見つからない。
    #[error("$INDEX_ALLOCATION attribute missing while children flag set")]
    IndexAllocationMissing,
    /// `$INDEX_ALLOCATION` が常駐になっている異常。
    #[error("$INDEX_ALLOCATION must be non-resident, got resident")]
    IndexAllocationNotNonResident,
    /// 指定 VCN（仮想クラスタ番号）が `$INDEX_ALLOCATION` の runlist 範囲外。
    #[error("Index VCN out of range: virtual_offset={virtual_offset}")]
    IndexVcnOutOfRange { virtual_offset: u64 },
    /// B+ ツリー走査が `MAX_BTREE_DEPTH` を超過。破損または悪意あるデータ疑い。
    #[error("B+ tree too deep (depth={depth}, max={MAX_BTREE_DEPTH})")]
    BtreeTooDeep { depth: u32 },
    /// パス再構築が深さ上限を超過。循環参照疑いまたは多重ハードリンク。
    #[error("Path resolution depth exceeded ({depth}) for record {record_index}")]
    PathDepthExceeded { record_index: u64, depth: u32 },
    /// MFT エントリに `$FILE_NAME` 属性がない（ルート以外でこのエラーが出るとパス再構築不可）。
    #[error("Record {record_index} has no $FILE_NAME attribute")]
    NoFileName { record_index: u64 },
    /// インデックスパース時のエラーを集約。Chunk 12 `IndexError`。
    #[error("Index parse error: {0}")]
    Index(#[from] IndexError),
}

/// ディレクトリ内の 1 エントリの情報。インデックス経由なのでライブ（生存）ファイルのみが
/// 列挙される。削除済みエントリは含まれない（書籍 Ch.12: 削除時インデックスエントリ除去）。
/// Win32 / DOS 別エントリの重複は仕様準拠でそのまま yield。呼び出し側で
/// `file_name.namespace.is_preferred_for_display()` 等を使って排除する。
/// 関連 FR: FR-LIVE-04, FR-LIVE-06。
#[derive(Debug, Clone)]
pub struct DirectoryListing {
    /// 子ファイル/ディレクトリの MFT 参照。
    pub child_ref: MftReference,
    /// `$FILE_NAME` 属性のコンテンツ（名前・属性・タイムスタンプ等を含む）。
    pub file_name: FileName,
}

impl DirectoryListing {
    /// このエントリがディレクトリか（`$FILE_NAME` の file_attributes ビット参照）。
    pub fn is_directory(&self) -> bool {
        self.file_name.file_attributes.is_directory()
    }
    /// ファイル名（短い形）。Win32 名 / DOS 名は呼び出し側で `namespace` で区別すること。
    pub fn name(&self) -> &str {
        &self.file_name.filename
    }
}

/// NTFS ボリュームの高レベル API。ブートセクタ解析 + `$MFT` bootstrap を `open()` で完了し、
/// 以降 `read_record(index)` と `iter_records()` で任意 MFT エントリへアクセス可能。
pub struct NtfsVolume<F> {
    /// 解析済みブートセクタ。
    boot_sector: BootSector,
    /// `$MFT` 自身の `$DATA` 非常駐 runlist。
    mft_runs: Vec<Run>,
    /// MFT レコードサイズ（バイト）。`BootSector::mft_record_size_bytes` 由来。
    mft_record_size: u32,
    /// クラスタサイズ（バイト）。
    cluster_size: u64,
    /// 推定総 MFT レコード数 = `mft_runs` の合計バイト / `mft_record_size`。
    total_records: u64,
    /// `(lcn, count)` を受け取り `count * cluster_size` バイトを返すクロージャ。
    read_clusters: F,
}

impl<F> NtfsVolume<F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// ボリュームを開く。`read_clusters(0, 1)` でブートセクタを読み、`$MFT` を bootstrap する。
    /// 失敗時は [`VolumeError`]。Phase 1 では `$ATTRIBUTE_LIST` 経由の `$MFT` 拡張は未対応で、
    /// `$MFT` の `$DATA` が record 0 内に収まる前提（典型的な小〜中規模ボリュームで成立）。
    pub fn open(mut read_clusters: F) -> Result<Self, VolumeError> {
        // Step 1: 先頭クラスタ → ブートセクタ解析。
        let first = read_clusters(0, 1)?;
        if first.len() < 512 {
            return Err(VolumeError::BootSectorBufferTooSmall { got: first.len() });
        }
        let boot_sector = parse_boot_sector(&first[..512])?;
        let cluster_size = u64::from(boot_sector.cluster_size_bytes());
        let mft_record_size = boot_sector.mft_record_size_bytes();
        // Step 2: mft_record_size を覆うクラスタ数だけ読んで MFT record 0 をパース。
        let need = u64::from(mft_record_size).div_ceil(cluster_size).max(1);
        let rec0_raw = read_clusters(boot_sector.mft_lcn, need)?;
        if rec0_raw.len() < mft_record_size as usize {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "MFT record 0 read incomplete",
            )));
        }
        let rec0 = parse_mft_entry(&rec0_raw[..mft_record_size as usize])?;
        // Step 3: $DATA 属性を探索。
        let data_attr = find_attribute(
            &rec0.data,
            rec0.header.first_attribute_offset as usize,
            AttributeType::Data,
        )
        .ok_or(VolumeError::NoMftDataAttribute)?;
        // Step 4: 非常駐確認 + runlist 取得（スパース混入は $MFT 破損疑い）。
        let runlist_off = match &data_attr.header {
            AttributeHeader::NonResident { non_resident, .. } => {
                non_resident.runlist_offset as usize
            }
            _ => return Err(VolumeError::MftDataMustBeNonResident),
        };
        if runlist_off >= data_attr.raw.len() {
            return Err(VolumeError::Runlist(RunlistError::BufferTooSmall {
                got: data_attr.raw.len(),
            }));
        }
        let mft_runs = parse_runlist(&data_attr.raw[runlist_off..])?;
        if mft_runs.iter().any(Run::is_sparse) {
            return Err(VolumeError::SparseMftRun);
        }
        // Step 5: 総レコード数 = 合計バイト / record_size。
        let total_bytes: u64 = mft_runs.iter().map(|r| r.byte_length(cluster_size)).sum();
        let total_records = total_bytes / u64::from(mft_record_size);
        Ok(Self {
            boot_sector,
            mft_runs,
            mft_record_size,
            cluster_size,
            total_records,
            read_clusters,
        })
    }

    /// 推定総 MFT レコード数（システム + ユーザ + 未使用全部）。
    pub fn total_records(&self) -> u64 {
        self.total_records
    }
    /// MFT レコードサイズ（バイト）。
    pub fn mft_record_size(&self) -> u32 {
        self.mft_record_size
    }
    /// クラスタサイズ（バイト）。
    pub fn cluster_size(&self) -> u64 {
        self.cluster_size
    }
    /// 解析済みブートセクタへの参照。
    pub fn boot_sector(&self) -> &BootSector {
        &self.boot_sector
    }

    /// 指定 index の MFT レコードを読み取る。範囲外は [`VolumeError::RecordIndexOutOfRange`]。
    pub fn read_record(&mut self, index: u64) -> Result<MftEntry, VolumeError> {
        if index >= self.total_records {
            return Err(VolumeError::RecordIndexOutOfRange {
                index,
                total: self.total_records,
            });
        }
        let vo = index * u64::from(self.mft_record_size);
        let (lcn, byte_in_cluster) = self.virtual_to_physical(vo)?;
        let clusters =
            (byte_in_cluster + u64::from(self.mft_record_size)).div_ceil(self.cluster_size);
        let raw = (self.read_clusters)(lcn, clusters)?;
        let (start, end) = (
            byte_in_cluster as usize,
            byte_in_cluster as usize + self.mft_record_size as usize,
        );
        if raw.len() < end {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "record bytes incomplete",
            )));
        }
        Ok(parse_mft_entry(&raw[start..end])?)
    }

    /// 仮想 MFT オフセット → 物理 `(LCN, byte_in_cluster)` 変換。多 run / 断片化 MFT に透過対応。
    fn virtual_to_physical(&self, virtual_offset: u64) -> Result<(u64, u64), VolumeError> {
        let mut cumulative: u64 = 0;
        for run in &self.mft_runs {
            let run_bytes = run.byte_length(self.cluster_size);
            if virtual_offset < cumulative + run_bytes {
                let in_run = virtual_offset - cumulative;
                let base_lcn = run.lcn.ok_or(VolumeError::SparseMftRun)?;
                return Ok((
                    base_lcn + in_run / self.cluster_size,
                    in_run % self.cluster_size,
                ));
            }
            cumulative += run_bytes;
        }
        Err(VolumeError::RecordIndexOutOfRange {
            index: virtual_offset / u64::from(self.mft_record_size),
            total: self.total_records,
        })
    }

    /// 全 MFT レコードを順次列挙するイテレータ。個別レコードのパースエラーで停止せず、
    /// `Result` で yield して継続（復旧ソフトとしての破損耐性）。
    pub fn iter_records(&mut self) -> NtfsMftIterator<'_, F> {
        NtfsMftIterator {
            volume: self,
            current: 0,
        }
    }

    /// 指定ディレクトリ MFT エントリ内の全子ファイル / ディレクトリを列挙する。
    /// `$INDEX_ROOT`（ルートノード）と `$INDEX_ALLOCATION`（B+ 子ノード INDX）を再帰走査。
    /// 値を持つ全エントリ（Win32 + DOS の重複含む）を yield。終端エントリはスキップ。
    /// 削除済みエントリはインデックスから除去されているため含まれない（Ch.12 動作）。
    /// 関連 FR: FR-LIVE-04, FR-LIVE-06。
    pub fn list_directory(
        &mut self,
        dir_record_index: u64,
    ) -> Result<Vec<DirectoryListing>, VolumeError> {
        let dir_entry = self.read_record(dir_record_index)?;
        let index_root_attr = find_attribute(
            &dir_entry.data,
            dir_entry.header.first_attribute_offset as usize,
            AttributeType::IndexRoot,
        )
        .ok_or(VolumeError::NotADirectory {
            record_index: dir_record_index,
        })?;
        let (content_offset, content_size) = match &index_root_attr.header {
            AttributeHeader::Resident { resident, .. } => (
                resident.content_offset as usize,
                resident.content_size as usize,
            ),
            _ => return Err(VolumeError::IndexRootNotResident),
        };
        let end = content_offset
            .checked_add(content_size)
            .ok_or(VolumeError::IndexRootNotResident)?;
        if end > index_root_attr.raw.len() {
            return Err(VolumeError::IndexRootNotResident);
        }
        let index_root = parse_index_root(&index_root_attr.raw[content_offset..end])?;
        let block_size = u64::from(index_root.bytes_per_index_record).max(self.cluster_size);
        // 値が有効なバイト範囲は `[first_entry_offset, end_of_entries_offset)`（node_header 起点）。
        // node_body は node_header の直後（offset 16 から）で始まるため、相対オフセットに変換。
        let first = (index_root.node_header.first_entry_offset as usize).saturating_sub(16);
        let end_off = (index_root.node_header.end_of_entries_offset as usize).saturating_sub(16);
        if end_off > index_root.node_body.len() || first > end_off {
            return Err(VolumeError::NotADirectory {
                record_index: dir_record_index,
            });
        }
        let bounded_body = &index_root.node_body[first..end_off];
        let root_entries = parse_entries_in_node(bounded_body)?;
        let mut results = Vec::new();
        self.walk_entries(&root_entries, &dir_entry, block_size, &mut results, 0)?;
        Ok(results)
    }

    /// B+ ツリーノードのエントリ列を走査し、必要に応じて子 INDX ブロックへ再帰。
    /// 値を持つエントリ（`file_name` 付き）は `results` に push、終端エントリは値を持たない。
    fn walk_entries(
        &mut self,
        entries: &[IndexEntry],
        dir_entry: &MftEntry,
        block_size: u64,
        results: &mut Vec<DirectoryListing>,
        depth: u32,
    ) -> Result<(), VolumeError> {
        if depth > MAX_BTREE_DEPTH {
            return Err(VolumeError::BtreeTooDeep { depth });
        }
        for entry in entries {
            if entry.has_child_node() {
                if let Some(vcn) = entry.child_vcn {
                    self.walk_indx_block(vcn, dir_entry, block_size, results, depth + 1)?;
                }
            }
            if entry.is_last() {
                continue;
            }
            if let Some(fn_) = &entry.file_name {
                results.push(DirectoryListing {
                    child_ref: entry.child_ref,
                    file_name: fn_.clone(),
                });
            }
        }
        Ok(())
    }

    /// `$INDEX_ALLOCATION` 内の指定 VCN の INDX ブロックを物理アドレスに変換して読み、再帰。
    fn walk_indx_block(
        &mut self,
        vcn: u64,
        dir_entry: &MftEntry,
        block_size: u64,
        results: &mut Vec<DirectoryListing>,
        depth: u32,
    ) -> Result<(), VolumeError> {
        let alloc_attr = find_attribute(
            &dir_entry.data,
            dir_entry.header.first_attribute_offset as usize,
            AttributeType::IndexAllocation,
        )
        .ok_or(VolumeError::IndexAllocationMissing)?;
        let runlist_offset = match &alloc_attr.header {
            AttributeHeader::NonResident { non_resident, .. } => {
                non_resident.runlist_offset as usize
            }
            _ => return Err(VolumeError::IndexAllocationNotNonResident),
        };
        if runlist_offset >= alloc_attr.raw.len() {
            return Err(VolumeError::Runlist(RunlistError::BufferTooSmall {
                got: alloc_attr.raw.len(),
            }));
        }
        let runs = parse_runlist(&alloc_attr.raw[runlist_offset..])?;
        let virtual_offset =
            vcn.checked_mul(block_size)
                .ok_or(VolumeError::IndexVcnOutOfRange {
                    virtual_offset: u64::MAX,
                })?;
        let (lcn, byte_in_cluster) = self.virtual_to_physical_in_runs(&runs, virtual_offset)?;
        let clusters_to_read = (byte_in_cluster + block_size)
            .div_ceil(self.cluster_size)
            .max(1);
        let raw = (self.read_clusters)(lcn, clusters_to_read)?;
        let bs = block_size as usize;
        let start = byte_in_cluster as usize;
        let end = start
            .checked_add(bs)
            .ok_or(VolumeError::IndexVcnOutOfRange { virtual_offset })?;
        if raw.len() < end {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "INDX block read incomplete",
            )));
        }
        let block_bytes = &raw[start..end];
        let sector_size = self.boot_sector.bytes_per_sector;
        let indx = parse_indx_block(block_bytes, sector_size)?;
        // INDX 内のエントリ開始位置は node_header.first_entry_offset（node_header 起点）。
        // USA を挟む場合があるので 16 とは限らない。bounded slice の終端は end_of_entries_offset。
        let first = indx.node_header.first_entry_offset as usize;
        let end_off = indx.node_header.end_of_entries_offset as usize;
        let node_hdr_abs = indx.node_header_offset; // = INDX_HDR = 0x18
        let s_abs = node_hdr_abs.saturating_add(first);
        let e_abs = node_hdr_abs.saturating_add(end_off);
        if e_abs > indx.data.len() || s_abs > e_abs {
            return Err(VolumeError::IndexVcnOutOfRange { virtual_offset });
        }
        let entries = parse_entries_in_node(&indx.data[s_abs..e_abs])?;
        self.walk_entries(&entries, dir_entry, block_size, results, depth)?;
        Ok(())
    }

    /// runlist 内の仮想オフセット → 物理 `(LCN, byte_in_cluster)` への変換。
    /// スパースランは `IndexVcnOutOfRange` 扱い（ディレクトリインデックスは通常スパース無し）。
    fn virtual_to_physical_in_runs(
        &self,
        runs: &[Run],
        virtual_offset: u64,
    ) -> Result<(u64, u64), VolumeError> {
        let mut cumulative: u64 = 0;
        for run in runs {
            let run_bytes = run.byte_length(self.cluster_size);
            if virtual_offset < cumulative + run_bytes {
                let in_run = virtual_offset - cumulative;
                let base_lcn = run
                    .lcn
                    .ok_or(VolumeError::IndexVcnOutOfRange { virtual_offset })?;
                return Ok((
                    base_lcn + in_run / self.cluster_size,
                    in_run % self.cluster_size,
                ));
            }
            cumulative += run_bytes;
        }
        Err(VolumeError::IndexVcnOutOfRange { virtual_offset })
    }

    /// 指定 MFT エントリのフルパスを解決する単発ラッパー。複数連続解決時は
    /// [`crate::path::PathResolver`] を直接使う方がキャッシュが効いて高速。
    /// 関連 FR: FR-LIVE-04, FR-LIVE-05, FR-LIVE-06。
    pub fn full_path(&mut self, record_index: u64) -> Result<String, VolumeError> {
        let mut resolver = crate::path::PathResolver::new();
        resolver.resolve(record_index, self)
    }

    /// 全 [`NtfsFile`] を順次列挙するイテレータを返す（Chunk 14）。
    ///
    /// `$FILE_NAME` 属性のないエントリは自動スキップ。個別エントリのパースエラーは `Result`
    /// として yield、イテレーションは継続。`PathResolver` キャッシュを内部共有するため
    /// N ファイル列挙が実用上 O(N) になる。業務統合層からの標準呼び出し口。
    /// 関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05, FR-LIVE-06。
    pub fn iter_files(&mut self) -> NtfsFileIterator<'_, F> {
        NtfsFileIterator::new(self)
    }

    /// 単一 MFT エントリから [`NtfsFile`] を構築する（単発呼び出し向け）。
    ///
    /// 戻り値の意味は [`crate::file::NtfsFileIterator`] と同じく `Ok(None)` で
    /// `$FILE_NAME` 欠落エントリを示す。複数件取得時は [`Self::iter_files`] の方が
    /// キャッシュが効いて効率が良い。
    /// 関連 FR: FR-LIVE-01, FR-LIVE-04。
    pub fn build_file(&mut self, record_index: u64) -> Result<Option<NtfsFile>, VolumeError> {
        let mut resolver = crate::path::PathResolver::new();
        build_file_for_record(self, record_index, &mut resolver)
    }

    /// [`NtfsFile`] のメイン `$DATA` 実バイト列を取得する（Chunk 14）。
    ///
    /// - [`FileContentRef::Resident`]: 既に bytes を保持しているので clone を返却。
    /// - [`FileContentRef::NonResident`]: ランを辿ってクラスタを読み、`real_size` で切詰め。
    /// - [`FileContentRef::None`]: 空 `Vec` を返却（ディレクトリ・$DATA 無しメタファイル等）。
    ///
    /// メモリ確保は `real_size` ぶん。Phase 1 では小〜中サイズ前提。
    /// 関連 FR: FR-LIVE-01, FR-REC-01, FR-REC-04。
    pub fn read_file_content(&mut self, file: &NtfsFile) -> Result<Vec<u8>, VolumeError> {
        match &file.content {
            FileContentRef::Resident(bytes) => Ok(bytes.clone()),
            FileContentRef::NonResident { real_size, runs } => {
                let cluster_size = self.cluster_size;
                // 分割借用: `read_clusters` フィールドだけ &mut で借りる（self 全体は借りない）。
                let read_fn = &mut self.read_clusters;
                read_runs_with(runs, cluster_size, *real_size, |lcn, count| {
                    (read_fn)(lcn, count)
                })
                .map_err(VolumeError::Runlist)
            }
            FileContentRef::None => Ok(Vec::new()),
        }
    }
}

/// 全 MFT レコードの順次イテレータ。削除エントリ・未使用エントリも全て yield。
/// 呼び出し側で `entry.header.is_deleted()` / `is_in_use()` 等で絞り込む。
pub struct NtfsMftIterator<'a, F> {
    volume: &'a mut NtfsVolume<F>,
    current: u64,
}

impl<'a, F> Iterator for NtfsMftIterator<'a, F>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// `(record_index, MftEntry または VolumeError)` のペア。エラーも yield し継続。
    type Item = (u64, Result<MftEntry, VolumeError>);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.volume.total_records {
            return None;
        }
        let idx = self.current;
        self.current += 1;
        Some((idx, self.volume.read_record(idx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // bps=512, spc=1 → cluster=512, cpmr=-10 → record=1024 B。MFT は LCN=4 から 8 clusters = 4 records。
    const CLUSTER: usize = 512;
    const RECORD: usize = 1024;
    const MFT_LCN: u64 = 4;
    const MFT_CLUSTERS: u64 = 8;
    const IMG_CLUSTERS: u64 = 32;
    const MFT_START: usize = MFT_LCN as usize * CLUSTER;

    fn put(buf: &mut [u8], off: usize, src: &[u8]) {
        buf[off..off + src.len()].copy_from_slice(src);
    }

    fn make_boot_sector() -> [u8; CLUSTER] {
        let mut b = [0u8; CLUSTER];
        put(&mut b, 3, b"NTFS    ");
        put(&mut b, 0x0B, &512u16.to_le_bytes());
        b[0x0D] = 1;
        b[0x15] = 0xF8;
        b[0x44] = 1;
        b[0x40] = (-10i8) as u8;
        put(&mut b, 0x28, &IMG_CLUSTERS.to_le_bytes());
        put(&mut b, 0x30, &MFT_LCN.to_le_bytes());
        put(&mut b, 0x38, &1u64.to_le_bytes());
        b[0x1FE] = 0x55;
        b[0x1FF] = 0xAA;
        b
    }

    // FILE シグネチャ + 最低限ヘッダ + 属性 + End マーカーを持つ MFT レコード（fixup ゼロ通過）。
    fn make_record(in_use: bool, attrs: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; RECORD];
        put(&mut r, 0, b"FILE");
        put(&mut r, 0x04, &0x30u16.to_le_bytes());
        put(&mut r, 0x06, &3u16.to_le_bytes());
        put(&mut r, 0x14, &0x38u16.to_le_bytes());
        put(&mut r, 0x16, &(if in_use { 1u16 } else { 0 }).to_le_bytes());
        put(&mut r, 0x18, &(RECORD as u32 / 2).to_le_bytes());
        put(&mut r, 0x1C, &(RECORD as u32).to_le_bytes());
        put(&mut r, 0x38, attrs);
        put(&mut r, 0x38 + attrs.len(), &0xFFFF_FFFFu32.to_le_bytes());
        r
    }

    // 非常駐 $DATA 属性 1 件: type=0x80, length=0x50, runlist_offset=0x40。
    fn nonres_data_attr(runlist: &[u8]) -> Vec<u8> {
        let mut a = vec![0u8; 0x50];
        put(&mut a, 0, &0x80u32.to_le_bytes());
        put(&mut a, 4, &0x50u32.to_le_bytes());
        a[0x08] = 1;
        put(&mut a, 0x18, &(MFT_CLUSTERS - 1).to_le_bytes());
        put(&mut a, 0x20, &0x40u16.to_le_bytes());
        let real = MFT_CLUSTERS * CLUSTER as u64;
        for off in [0x28, 0x30, 0x38] {
            put(&mut a, off, &real.to_le_bytes());
        }
        put(&mut a, 0x40, runlist);
        a
    }

    // 単一 run runlist: 0x21=(L=1B,O=2B), len=MFT_CLUSTERS, lcn=MFT_LCN(LE 2B), end=0。
    fn single_runlist() -> Vec<u8> {
        vec![0x21, MFT_CLUSTERS as u8, MFT_LCN as u8, 0x00, 0x00]
    }

    fn build_minimal_ntfs_volume() -> Vec<u8> {
        let mut img = vec![0u8; IMG_CLUSTERS as usize * CLUSTER];
        put(&mut img, 0, &make_boot_sector());
        put(
            &mut img,
            MFT_START,
            &make_record(true, &nonres_data_attr(&single_runlist())),
        );
        for i in 1..4 {
            put(&mut img, MFT_START + i * RECORD, &make_record(i != 3, &[]));
        }
        img
    }

    fn make_reader(img: Vec<u8>) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
        move |lcn, count| {
            let (s, e) = (
                lcn as usize * CLUSTER,
                (lcn as usize + count as usize) * CLUSTER,
            );
            if e > img.len() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "oob",
                ))
            } else {
                Ok(img[s..e].to_vec())
            }
        }
    }

    fn open_minimal() -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
        NtfsVolume::open(make_reader(build_minimal_ntfs_volume())).expect("open")
    }

    #[test]
    fn opens_minimal_valid_volume() {
        let v = open_minimal();
        assert_eq!(
            (v.cluster_size(), v.mft_record_size(), v.total_records()),
            (CLUSTER as u64, RECORD as u32, 4)
        );
        assert_eq!(v.boot_sector().mft_lcn, MFT_LCN);
    }

    #[test]
    fn virtual_to_physical_single_run_correct_mapping() {
        let v = open_minimal();
        assert_eq!(v.virtual_to_physical(0).unwrap(), (MFT_LCN, 0));
        assert_eq!(
            v.virtual_to_physical(2 * RECORD as u64).unwrap(),
            (MFT_LCN + 4, 0)
        );
        assert_eq!(
            v.virtual_to_physical(1500).unwrap(),
            (MFT_LCN + 2, 1500 - 1024)
        );
    }

    #[test]
    fn virtual_to_physical_multi_run_crosses_boundary() {
        // 多 run: run1=(LCN=4, 4 clusters=2048B), run2=(LCN=20, 4 clusters)
        // header=0x21 L=1B,O=2B / len=4, lcn=4(LE 2B) / len=4, delta=+16(LE 2B → lcn 20) / end=0
        let mut img = build_minimal_ntfs_volume();
        let multi = vec![0x21, 0x04, 0x04, 0x00, 0x21, 0x04, 0x10, 0x00, 0x00];
        put(
            &mut img,
            MFT_START,
            &make_record(true, &nonres_data_attr(&multi)),
        );
        let v = NtfsVolume::open(make_reader(img)).expect("open");
        assert_eq!(v.total_records(), 4);
        assert_eq!(v.virtual_to_physical(0).unwrap(), (4, 0));
        assert_eq!(v.virtual_to_physical(2048).unwrap(), (20, 0));
        assert_eq!(
            v.virtual_to_physical(3000).unwrap(),
            (21, 3000 - 2048 - 512)
        );
    }

    #[test]
    fn read_record_out_of_range_returns_error() {
        let mut v = open_minimal();
        let total = v.total_records();
        let err = v.read_record(total).err().unwrap();
        assert!(
            matches!(err, VolumeError::RecordIndexOutOfRange { index, total: t }
            if index == total && t == total)
        );
    }

    #[test]
    fn read_record_zero_returns_mft_itself() {
        let mut v = open_minimal();
        let rec0 = v.read_record(0).expect("record 0");
        assert!(rec0.header.is_in_use());
        assert!(find_attribute(
            &rec0.data,
            rec0.header.first_attribute_offset as usize,
            AttributeType::Data
        )
        .is_some());
    }

    #[test]
    fn open_fails_without_boot_sector() {
        let err = NtfsVolume::open(|_, _| Ok::<_, std::io::Error>(vec![0u8; 100]))
            .err()
            .unwrap();
        assert!(matches!(
            err,
            VolumeError::BootSectorBufferTooSmall { got: 100 }
        ));
    }

    #[test]
    fn open_fails_when_mft_data_is_resident() {
        let mut img = build_minimal_ntfs_volume();
        let mut resident = vec![0u8; 0x20];
        put(&mut resident, 0, &0x80u32.to_le_bytes());
        put(&mut resident, 4, &0x20u32.to_le_bytes());
        put(&mut resident, 0x10, &0x08u32.to_le_bytes());
        put(&mut resident, 0x14, &0x18u16.to_le_bytes());
        put(&mut img, MFT_START, &make_record(true, &resident));
        let err = NtfsVolume::open(make_reader(img)).err().unwrap();
        assert!(matches!(err, VolumeError::MftDataMustBeNonResident));
    }

    #[test]
    fn open_fails_when_no_mft_data_attribute() {
        let mut img = build_minimal_ntfs_volume();
        put(&mut img, MFT_START, &make_record(true, &[]));
        let err = NtfsVolume::open(make_reader(img)).err().unwrap();
        assert!(matches!(err, VolumeError::NoMftDataAttribute));
    }

    #[test]
    fn iter_records_yields_all_indices_in_order() {
        let mut v = open_minimal();
        let indices: Vec<u64> = v.iter_records().map(|(i, _)| i).collect();
        assert_eq!(indices, vec![0, 1, 2, 3]);
    }

    #[test]
    fn iter_records_continues_on_individual_parse_error() {
        // record 2 のシグネチャを破壊して InvalidMagic 発生、他は正常パースされ継続。
        let mut img = build_minimal_ntfs_volume();
        put(&mut img, MFT_START + 2 * RECORD, b"XXXX");
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let ok_flags: Vec<bool> = v.iter_records().map(|(_, r)| r.is_ok()).collect();
        assert_eq!(ok_flags, vec![true, true, false, true]);
    }

    #[test]
    fn sparse_mft_runlist_rejected() {
        // header=0x01 → length 1B, offset 0B = スパースラン。
        let mut img = build_minimal_ntfs_volume();
        let sparse = vec![0x01, MFT_CLUSTERS as u8, 0x00];
        put(
            &mut img,
            MFT_START,
            &make_record(true, &nonres_data_attr(&sparse)),
        );
        let err = NtfsVolume::open(make_reader(img)).err().unwrap();
        assert!(matches!(err, VolumeError::SparseMftRun));
    }

    // ---------------- Chunk 13: list_directory / full_path テスト用ヘルパ -----------------
    fn put16(b: &mut [u8], o: usize, v: u16) {
        b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// 単一の `$FILE_NAME` コンテンツ（66B + UTF-16 名）を構築。Win32 名前空間。
    fn fn_content(parent_entry: u64, name: &str) -> Vec<u8> {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut b = vec![0u8; 0x42 + utf16.len() * 2];
        b[0..8].copy_from_slice(&(parent_entry | (1u64 << 48)).to_le_bytes());
        b[0x40] = utf16.len() as u8;
        b[0x41] = 1; // Win32
        for (i, u) in utf16.iter().enumerate() {
            b[0x42 + i * 2..0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        b
    }

    /// 常駐 `$FILE_NAME` 属性（ヘッダ + コンテンツ、8 バイトアライン）を構築。
    fn resident_fn_attr(parent: u64, name: &str) -> Vec<u8> {
        let content = fn_content(parent, name);
        let (cs, hs) = (content.len() as u32, 0x18u32);
        let length = (hs + cs).div_ceil(8) * 8;
        let mut b = vec![0u8; length as usize];
        put32(&mut b, 0, 0x30); // type
        put32(&mut b, 4, length);
        put16(&mut b, 0x0A, 0x18);
        put32(&mut b, 0x10, cs);
        put16(&mut b, 0x14, hs as u16);
        b[hs as usize..hs as usize + content.len()].copy_from_slice(&content);
        b
    }

    /// 終端のみの $INDEX_ROOT エントリ列（B+ 葉ノードで子なし、ファイル列挙テスト用）を構築。
    /// `items` は (entry_number, name) のリスト。
    fn index_root_attr(items: &[(u64, &str)]) -> Vec<u8> {
        // エントリ列構築
        let mut entries: Vec<u8> = Vec::new();
        for &(no, name) in items {
            let fnc = fn_content(5, name);
            let elen = ((16 + fnc.len() + 7) & !7) as u16;
            let mut e = vec![0u8; elen as usize];
            e[0..8].copy_from_slice(&(no | (1u64 << 48)).to_le_bytes());
            put16(&mut e, 8, elen);
            put16(&mut e, 10, fnc.len() as u16);
            e[16..16 + fnc.len()].copy_from_slice(&fnc);
            entries.extend_from_slice(&e);
        }
        // 終端エントリ (16B, flags=F_LAST=0x02)
        let mut term = vec![0u8; 16];
        put16(&mut term, 8, 16);
        put32(&mut term, 12, 0x02);
        entries.extend_from_slice(&term);

        // $INDEX_ROOT コンテンツ: std_hdr(16) + node_hdr(16) + entries
        let std_hdr = 16usize;
        let node_hdr = 16usize;
        let content_size = std_hdr + node_hdr + entries.len();
        let mut content = vec![0u8; content_size];
        put32(&mut content, 0, 0x30); // index_type = FILE_NAME
        put32(&mut content, 8, 4096); // bytes_per_index_record
        content[12] = 1; // clusters_per_index_record
        let eo = (node_hdr + entries.len()) as u32;
        put32(&mut content, std_hdr, 16); // first_entry_offset
        put32(&mut content, std_hdr + 4, eo); // end_of_entries
        put32(&mut content, std_hdr + 8, eo); // end_of_buffer
        content[std_hdr + node_hdr..].copy_from_slice(&entries);

        // 属性ラッパ: type=0x90, length=hdr+content（aligned）
        let hs = 0x18u32;
        let cs = content_size as u32;
        let length = (hs + cs).div_ceil(8) * 8;
        let mut b = vec![0u8; length as usize];
        put32(&mut b, 0, 0x90); // INDEX_ROOT
        put32(&mut b, 4, length);
        put16(&mut b, 0x0A, 0x18);
        put32(&mut b, 0x10, cs);
        put16(&mut b, 0x14, hs as u16);
        b[hs as usize..hs as usize + content.len()].copy_from_slice(&content);
        b
    }

    /// `attrs_blob`（連結済み属性）を持つ MFT レコード（fixup ゼロ、in_use=true）を構築。
    fn make_record_with_attrs(attrs_blob: &[u8]) -> Vec<u8> {
        make_record(true, attrs_blob)
    }

    /// テスト用ボリュームを構築: rec0=$MFT, rec1=ディレクトリ, rec2=ファイル, rec3=ルート相当。
    /// `dir_attrs` を rec1 に詰める。
    fn build_volume_with_dir(dir_attrs: &[u8]) -> Vec<u8> {
        let mut img = vec![0u8; IMG_CLUSTERS as usize * CLUSTER];
        put(&mut img, 0, &make_boot_sector());
        put(
            &mut img,
            MFT_START,
            &make_record(true, &nonres_data_attr(&single_runlist())),
        );
        put(
            &mut img,
            MFT_START + RECORD,
            &make_record_with_attrs(dir_attrs),
        );
        // rec2: ファイル（$FILE_NAME 付き、親 = root=5）
        let file_fn = resident_fn_attr(5, "hello.txt");
        put(
            &mut img,
            MFT_START + 2 * RECORD,
            &make_record_with_attrs(&file_fn),
        );
        // rec3: 空エントリ（未使用）
        put(&mut img, MFT_START + 3 * RECORD, &make_record(false, &[]));
        img
    }

    #[test]
    fn list_directory_returns_error_for_non_directory() {
        // rec1 を $INDEX_ROOT なしのレコードにすると NotADirectory
        let img = build_volume_with_dir(&[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let err = v.list_directory(1).err().unwrap();
        assert!(matches!(
            err,
            VolumeError::NotADirectory { record_index: 1 }
        ));
    }

    #[test]
    fn list_directory_small_uses_index_root_only() {
        // rec1 を 3 ファイル分の $INDEX_ROOT 持ちディレクトリにする
        let attr = index_root_attr(&[(64, "a.txt"), (65, "b.txt"), (66, "c.txt")]);
        let img = build_volume_with_dir(&attr);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let listing = v.list_directory(1).expect("list ok");
        let names: Vec<&str> = listing.iter().map(|l| l.name()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
        // 終端エントリは含まれない（値を持たないため）
        assert_eq!(listing.len(), 3);
        assert_eq!(listing[0].child_ref.entry_number, 64);
    }

    #[test]
    fn list_directory_empty_directory_returns_empty_vec() {
        // 0 ファイルのディレクトリ（終端エントリのみ）
        let attr = index_root_attr(&[]);
        let img = build_volume_with_dir(&attr);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let listing = v.list_directory(1).expect("list ok");
        assert!(
            listing.is_empty(),
            "empty dir, got {} entries",
            listing.len()
        );
    }

    #[test]
    fn full_path_root_record_returns_backslash() {
        let img = build_volume_with_dir(&[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        // ルート (entry 5) はキャッシュで即返却（read_record されない、total<5 でも OK）
        let p = v.full_path(5).expect("ok");
        assert_eq!(p, "\\");
    }

    #[test]
    fn full_path_user_file_returns_full_path() {
        // rec2 (file with $FILE_NAME parent=5, name="hello.txt") → "\hello.txt"
        let img = build_volume_with_dir(&[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let p = v.full_path(2).expect("ok");
        assert_eq!(p, "\\hello.txt");
    }

    #[test]
    fn full_path_no_file_name_returns_error() {
        // rec1 = 属性なしのレコード → NoFileName
        let img = build_volume_with_dir(&[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let err = v.full_path(1).err().unwrap();
        assert!(matches!(err, VolumeError::NoFileName { record_index: 1 }));
    }

    #[test]
    fn directory_listing_methods_expose_is_directory_and_name() {
        let attr = index_root_attr(&[(64, "child.txt")]);
        let img = build_volume_with_dir(&attr);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let listing = v.list_directory(1).expect("list ok");
        let entry = &listing[0];
        assert_eq!(entry.name(), "child.txt");
        // テスト用 fn_content は file_attributes フィールドを 0 にしているのでディレクトリではない
        assert!(!entry.is_directory());
        assert_eq!(entry.child_ref.entry_number, 64);
    }

    // ---------------- Chunk 14: build_file テスト用ヘルパ + テスト -----------------
    /// 2026-01-01 00:00:00 UTC を FILETIME（1601 起算 100ns 単位）で表現。`$SI` 用。
    const FT_2026_JAN: u64 = 134_116_992_000_000_000;
    /// `$FILE_NAME` 用、$SI と明確に異なる遠い未来の FILETIME（$SI + 大幅 offset）。
    /// 厳密な年は問わず「$SI と FN の値が明らかに異なる」ことのみテストで検証する。
    const FT_FAR_FUTURE: u64 = FT_2026_JAN + 200_000_000_000_000_000;

    /// 4 つのタイムスタンプを埋めた `$FILE_NAME` コンテンツ。
    fn fn_content_with_times(parent_entry: u64, name: &str, base_ft: u64) -> Vec<u8> {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut b = vec![0u8; 0x42 + utf16.len() * 2];
        b[0..8].copy_from_slice(&(parent_entry | (1u64 << 48)).to_le_bytes());
        for (i, off) in [0x08usize, 0x10, 0x18, 0x20].iter().enumerate() {
            b[*off..*off + 8].copy_from_slice(&(base_ft + i as u64).to_le_bytes());
        }
        b[0x40] = utf16.len() as u8;
        b[0x41] = 1;
        for (i, u) in utf16.iter().enumerate() {
            b[0x42 + i * 2..0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        b
    }

    /// 常駐 `$FILE_NAME` 属性（タイムスタンプ付き）。
    fn resident_fn_attr_with_times(parent: u64, name: &str, base_ft: u64) -> Vec<u8> {
        let content = fn_content_with_times(parent, name, base_ft);
        let (cs, hs) = (content.len() as u32, 0x18u32);
        let length = (hs + cs).div_ceil(8) * 8;
        let mut b = vec![0u8; length as usize];
        put32(&mut b, 0, 0x30);
        put32(&mut b, 4, length);
        put16(&mut b, 0x0A, 0x18);
        put32(&mut b, 0x10, cs);
        put16(&mut b, 0x14, hs as u16);
        b[hs as usize..hs as usize + content.len()].copy_from_slice(&content);
        b
    }

    /// 常駐 `$STANDARD_INFORMATION` 属性（48B コンテンツ、4 タイムスタンプ）。
    fn resident_si_attr(base_ft: u64) -> Vec<u8> {
        let mut content = vec![0u8; 0x48];
        for (i, off) in [0x00usize, 0x08, 0x10, 0x18].iter().enumerate() {
            content[*off..*off + 8].copy_from_slice(&(base_ft + i as u64).to_le_bytes());
        }
        // file_attributes=0x20 (ARCHIVE)
        content[0x20..0x24].copy_from_slice(&0x20u32.to_le_bytes());
        let (cs, hs) = (content.len() as u32, 0x18u32);
        let length = (hs + cs).div_ceil(8) * 8;
        let mut b = vec![0u8; length as usize];
        put32(&mut b, 0, 0x10); // STANDARD_INFORMATION
        put32(&mut b, 4, length);
        put16(&mut b, 0x0A, 0x18);
        put32(&mut b, 0x10, cs);
        put16(&mut b, 0x14, hs as u16);
        b[hs as usize..hs as usize + content.len()].copy_from_slice(&content);
        b
    }

    /// rec1 を「$SI + $FILE_NAME」両持ち、rec2 を「$FILE_NAME のみ」にしたボリュームを構築。
    fn build_volume_with_si_and_fn(rec1_attrs: &[u8], rec2_attrs: &[u8]) -> Vec<u8> {
        let mut img = vec![0u8; IMG_CLUSTERS as usize * CLUSTER];
        put(&mut img, 0, &make_boot_sector());
        put(
            &mut img,
            MFT_START,
            &make_record(true, &nonres_data_attr(&single_runlist())),
        );
        put(&mut img, MFT_START + RECORD, &make_record(true, rec1_attrs));
        put(
            &mut img,
            MFT_START + 2 * RECORD,
            &make_record(true, rec2_attrs),
        );
        put(&mut img, MFT_START + 3 * RECORD, &make_record(false, &[]));
        img
    }

    #[test]
    fn build_file_returns_none_for_entry_without_filename() {
        // rec1 = 空属性（$FILE_NAME 無し） → build_file は Ok(None)
        let img = build_volume_with_dir(&[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let result = v.build_file(1).expect("build_file ok");
        assert!(
            result.is_none(),
            "expected None for entry without $FILE_NAME"
        );
    }

    #[test]
    fn build_file_extracts_all_timestamps() {
        // rec1 = $SI(FT_2026_JAN) + $FILE_NAME(FT_FAR_FUTURE)
        let si = resident_si_attr(FT_2026_JAN);
        let fnm = resident_fn_attr_with_times(5, "doc.txt", FT_FAR_FUTURE);
        let mut attrs = si;
        attrs.extend_from_slice(&fnm);
        let img = build_volume_with_si_and_fn(&attrs, &[]);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let file = v.build_file(1).expect("ok").expect("some");
        assert!(file.created.is_some());
        assert!(file.modified.is_some());
        assert!(file.accessed.is_some());
        assert!(file.mft_modified.is_some());
        // $SI 優先: 2026 年（$SI 値）が採用されるはず（$FILE_NAME は遠い未来）。
        let created_rfc = file.created.unwrap().to_rfc3339();
        assert!(
            created_rfc.starts_with("2026"),
            "expected $SI (2026) priority, got {}",
            created_rfc
        );
        assert_eq!(file.name, "doc.txt");
        assert_eq!(file.record_index, 1);
    }

    #[test]
    fn build_file_falls_back_to_filename_when_si_missing() {
        // rec2 = $FILE_NAME のみ（$SI 無し） → $FILE_NAME のタイムスタンプが採用される
        let fnm = resident_fn_attr_with_times(5, "fallback.txt", FT_FAR_FUTURE);
        let img = build_volume_with_si_and_fn(&[], &fnm);
        let mut v = NtfsVolume::open(make_reader(img)).expect("open");
        let file = v.build_file(2).expect("ok").expect("some");
        assert!(file.created.is_some());
        // $FILE_NAME のタイムスタンプ（遠い未来）が採用されている → 少なくとも 2026 ではない。
        let created_rfc = file.created.unwrap().to_rfc3339();
        assert!(
            !created_rfc.starts_with("2026"),
            "expected $FILE_NAME fallback (far future), but got 2026 (= $SI default), {}",
            created_rfc
        );
        // 具体的には数十年以上未来の値（年=2050 以上）であることを確認。
        assert!(
            file.created.unwrap().timestamp()
                > chrono::DateTime::parse_from_rfc3339("2050-01-01T00:00:00+00:00")
                    .unwrap()
                    .timestamp(),
            "$FILE_NAME fallback timestamp should be in the far future, got {}",
            created_rfc
        );
        assert_eq!(file.name, "fallback.txt");
    }
}
