//! NTFS ボリュームの高レベル API。Chunks 4-10 の純粋関数群を束ね、`NtfsVolume::open(reader)`
//! で全 MFT エントリの列挙・ランダムアクセスを提供する。`disk-io` への直接依存を持たず、
//! `read_clusters: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>` クロージャ経由で
//! 後段（Chunk 13+）の `ReadOnlyDisk` ラッピングを可能にする疎結合設計。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04（部分）, FR-LIVE-05, FR-LIVE-06。
use crate::attribute::{AttributeError, AttributeHeader, AttributeType};
use crate::attributes::{find_attribute, parse_runlist, Run, RunlistError};
use crate::boot_sector::{parse_boot_sector, BootSector, BootSectorError};
use crate::mft::{parse_mft_entry, MftEntry, MftError};
use thiserror::Error;

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
    #[error("$MFT record 0 has no $DATA attribute (corrupted volume or unsupported $ATTRIBUTE_LIST)")]
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
                std::io::ErrorKind::UnexpectedEof, "MFT record 0 read incomplete")));
        }
        let rec0 = parse_mft_entry(&rec0_raw[..mft_record_size as usize])?;
        // Step 3: $DATA 属性を探索。
        let data_attr = find_attribute(&rec0.data,
            rec0.header.first_attribute_offset as usize, AttributeType::Data)
            .ok_or(VolumeError::NoMftDataAttribute)?;
        // Step 4: 非常駐確認 + runlist 取得（スパース混入は $MFT 破損疑い）。
        let runlist_off = match &data_attr.header {
            AttributeHeader::NonResident { non_resident, .. } => non_resident.runlist_offset as usize,
            _ => return Err(VolumeError::MftDataMustBeNonResident),
        };
        if runlist_off >= data_attr.raw.len() {
            return Err(VolumeError::Runlist(RunlistError::BufferTooSmall { got: data_attr.raw.len() }));
        }
        let mft_runs = parse_runlist(&data_attr.raw[runlist_off..])?;
        if mft_runs.iter().any(Run::is_sparse) { return Err(VolumeError::SparseMftRun); }
        // Step 5: 総レコード数 = 合計バイト / record_size。
        let total_bytes: u64 = mft_runs.iter().map(|r| r.byte_length(cluster_size)).sum();
        let total_records = total_bytes / u64::from(mft_record_size);
        Ok(Self { boot_sector, mft_runs, mft_record_size, cluster_size, total_records, read_clusters })
    }

    /// 推定総 MFT レコード数（システム + ユーザ + 未使用全部）。
    pub fn total_records(&self) -> u64 { self.total_records }
    /// MFT レコードサイズ（バイト）。
    pub fn mft_record_size(&self) -> u32 { self.mft_record_size }
    /// クラスタサイズ（バイト）。
    pub fn cluster_size(&self) -> u64 { self.cluster_size }
    /// 解析済みブートセクタへの参照。
    pub fn boot_sector(&self) -> &BootSector { &self.boot_sector }

    /// 指定 index の MFT レコードを読み取る。範囲外は [`VolumeError::RecordIndexOutOfRange`]。
    pub fn read_record(&mut self, index: u64) -> Result<MftEntry, VolumeError> {
        if index >= self.total_records {
            return Err(VolumeError::RecordIndexOutOfRange { index, total: self.total_records });
        }
        let vo = index * u64::from(self.mft_record_size);
        let (lcn, byte_in_cluster) = self.virtual_to_physical(vo)?;
        let clusters = (byte_in_cluster + u64::from(self.mft_record_size)).div_ceil(self.cluster_size);
        let raw = (self.read_clusters)(lcn, clusters)?;
        let (start, end) = (byte_in_cluster as usize, byte_in_cluster as usize + self.mft_record_size as usize);
        if raw.len() < end {
            return Err(VolumeError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof, "record bytes incomplete")));
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
                return Ok((base_lcn + in_run / self.cluster_size, in_run % self.cluster_size));
            }
            cumulative += run_bytes;
        }
        Err(VolumeError::RecordIndexOutOfRange {
            index: virtual_offset / u64::from(self.mft_record_size), total: self.total_records })
    }

    /// 全 MFT レコードを順次列挙するイテレータ。個別レコードのパースエラーで停止せず、
    /// `Result` で yield して継続（復旧ソフトとしての破損耐性）。
    pub fn iter_records(&mut self) -> NtfsMftIterator<'_, F> {
        NtfsMftIterator { volume: self, current: 0 }
    }
}

/// 全 MFT レコードの順次イテレータ。削除エントリ・未使用エントリも全て yield。
/// 呼び出し側で `entry.header.is_deleted()` / `is_in_use()` 等で絞り込む。
pub struct NtfsMftIterator<'a, F> {
    volume: &'a mut NtfsVolume<F>,
    current: u64,
}

impl<'a, F> Iterator for NtfsMftIterator<'a, F>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    /// `(record_index, MftEntry または VolumeError)` のペア。エラーも yield し継続。
    type Item = (u64, Result<MftEntry, VolumeError>);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.volume.total_records { return None; }
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

    fn put(buf: &mut [u8], off: usize, src: &[u8]) { buf[off..off + src.len()].copy_from_slice(src); }

    fn make_boot_sector() -> [u8; CLUSTER] {
        let mut b = [0u8; CLUSTER];
        put(&mut b, 3, b"NTFS    ");
        put(&mut b, 0x0B, &512u16.to_le_bytes());
        b[0x0D] = 1; b[0x15] = 0xF8; b[0x44] = 1; b[0x40] = (-10i8) as u8;
        put(&mut b, 0x28, &IMG_CLUSTERS.to_le_bytes());
        put(&mut b, 0x30, &MFT_LCN.to_le_bytes());
        put(&mut b, 0x38, &1u64.to_le_bytes());
        b[0x1FE] = 0x55; b[0x1FF] = 0xAA;
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
        for off in [0x28, 0x30, 0x38] { put(&mut a, off, &real.to_le_bytes()); }
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
        put(&mut img, MFT_START, &make_record(true, &nonres_data_attr(&single_runlist())));
        for i in 1..4 {
            put(&mut img, MFT_START + i * RECORD, &make_record(i != 3, &[]));
        }
        img
    }

    fn make_reader(img: Vec<u8>) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
        move |lcn, count| {
            let (s, e) = (lcn as usize * CLUSTER, (lcn as usize + count as usize) * CLUSTER);
            if e > img.len() { Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "oob")) }
            else { Ok(img[s..e].to_vec()) }
        }
    }

    fn open_minimal() -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
        NtfsVolume::open(make_reader(build_minimal_ntfs_volume())).expect("open")
    }

    #[test]
    fn opens_minimal_valid_volume() {
        let v = open_minimal();
        assert_eq!((v.cluster_size(), v.mft_record_size(), v.total_records()),
            (CLUSTER as u64, RECORD as u32, 4));
        assert_eq!(v.boot_sector().mft_lcn, MFT_LCN);
    }

    #[test]
    fn virtual_to_physical_single_run_correct_mapping() {
        let v = open_minimal();
        assert_eq!(v.virtual_to_physical(0).unwrap(), (MFT_LCN, 0));
        assert_eq!(v.virtual_to_physical(2 * RECORD as u64).unwrap(), (MFT_LCN + 4, 0));
        assert_eq!(v.virtual_to_physical(1500).unwrap(), (MFT_LCN + 2, 1500 - 1024));
    }

    #[test]
    fn virtual_to_physical_multi_run_crosses_boundary() {
        // 多 run: run1=(LCN=4, 4 clusters=2048B), run2=(LCN=20, 4 clusters)
        // header=0x21 L=1B,O=2B / len=4, lcn=4(LE 2B) / len=4, delta=+16(LE 2B → lcn 20) / end=0
        let mut img = build_minimal_ntfs_volume();
        let multi = vec![0x21, 0x04, 0x04, 0x00, 0x21, 0x04, 0x10, 0x00, 0x00];
        put(&mut img, MFT_START, &make_record(true, &nonres_data_attr(&multi)));
        let v = NtfsVolume::open(make_reader(img)).expect("open");
        assert_eq!(v.total_records(), 4);
        assert_eq!(v.virtual_to_physical(0).unwrap(), (4, 0));
        assert_eq!(v.virtual_to_physical(2048).unwrap(), (20, 0));
        assert_eq!(v.virtual_to_physical(3000).unwrap(), (21, 3000 - 2048 - 512));
    }

    #[test]
    fn read_record_out_of_range_returns_error() {
        let mut v = open_minimal();
        let total = v.total_records();
        let err = v.read_record(total).err().unwrap();
        assert!(matches!(err, VolumeError::RecordIndexOutOfRange { index, total: t }
            if index == total && t == total));
    }

    #[test]
    fn read_record_zero_returns_mft_itself() {
        let mut v = open_minimal();
        let rec0 = v.read_record(0).expect("record 0");
        assert!(rec0.header.is_in_use());
        assert!(find_attribute(&rec0.data, rec0.header.first_attribute_offset as usize,
            AttributeType::Data).is_some());
    }

    #[test]
    fn open_fails_without_boot_sector() {
        let err = NtfsVolume::open(|_, _| Ok::<_, std::io::Error>(vec![0u8; 100]))
            .err().unwrap();
        assert!(matches!(err, VolumeError::BootSectorBufferTooSmall { got: 100 }));
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
        put(&mut img, MFT_START, &make_record(true, &nonres_data_attr(&sparse)));
        let err = NtfsVolume::open(make_reader(img)).err().unwrap();
        assert!(matches!(err, VolumeError::SparseMftRun));
    }
}
