//! # dds-disk-io
//!
//! Raw disk access の抽象化を提供するクレートです。
//! ソースデバイスへの**書き込みは型レベルで禁止**されており、
//! 本クレートで定義する trait/struct には write 系メソッドを一切実装しません。
//!
//! 関連要件:
//! - NFR-REL-01: ソースデバイス書込禁止（全 I/O は read-only）
//! - architecture.md: disk-io 責務（Raw disk 抽象 + イメージファイル読込）

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod fs_detection;
pub mod partition;
pub mod physical;
pub mod physical_partition;

pub use fs_detection::{detect_from_boot_sector, FsType};
pub use partition::{Partition, PartitionError, PartitionType};
pub use physical::{
    enumerate_physical_drives, BusType, PhysicalDrive, PhysicalDriveError, PhysicalDriveInfo,
};
pub use physical_partition::PhysicalPartitionReader;

use dds_core::{CoreError, CoreResult};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// `FileBackedDisk::open` 時に採用する既定セクタサイズ（バイト）。
pub const DEFAULT_SECTOR_SIZE: u32 = 512;

/// 読み込み専用ディスクの抽象トレイト。
///
/// 関連要件 NFR-REL-01:
/// ソースデバイスへの書き込みは厳禁のため、本トレイトには
/// `write_at` / `flush` / `truncate` などの書き込みメソッドを**意図的に定義しません**。
/// 実装側に独自の書き込みメソッドを追加することも禁止します。
pub trait ReadOnlyDisk {
    /// 物理セクタサイズ（典型は 512 または 4096 バイト）。
    fn sector_size(&self) -> u32;

    /// ディスク総サイズ（バイト）。
    fn total_size(&self) -> u64;

    /// 指定オフセットから `buf.len()` バイトを完全に充填して読み込みます。
    ///
    /// 範囲外の場合は `CoreError::OutOfRange` を返します。
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> CoreResult<()>;

    /// セクタ単位の便利メソッド。`buf.len()` がセクタサイズと一致する必要があります。
    fn read_sector(&mut self, sector_index: u64, buf: &mut [u8]) -> CoreResult<()> {
        let sector = self.sector_size() as u64;
        if buf.len() as u64 != sector {
            return Err(CoreError::InvalidArgument(format!(
                "buffer size {} != sector size {}",
                buf.len(),
                sector
            )));
        }
        self.read_at(sector_index.saturating_mul(sector), buf)
    }
}

/// ファイルをディスクとして扱う `ReadOnlyDisk` 実装。
///
/// テスト用イメージファイルや、`.dd` / `.img` ダンプを読み込むために使用します。
/// 関連要件 NFR-REL-01: ファイルは必ず `OpenOptions::new().read(true)` で開きます。
#[derive(Debug)]
pub struct FileBackedDisk {
    file: File,
    total_size: u64,
    sector_size: u32,
}

impl FileBackedDisk {
    /// 既定セクタサイズ（[`DEFAULT_SECTOR_SIZE`]）でファイルを read-only に開きます。
    pub fn open<P: AsRef<Path>>(path: P) -> CoreResult<Self> {
        Self::open_with_sector_size(path, DEFAULT_SECTOR_SIZE)
    }

    /// セクタサイズを明示してファイルを read-only に開きます。
    ///
    /// `sector_size` は 1 以上かつ 2 の累乗である必要があります。
    pub fn open_with_sector_size<P: AsRef<Path>>(path: P, sector_size: u32) -> CoreResult<Self> {
        if sector_size == 0 || !is_power_of_two(sector_size) {
            return Err(CoreError::InvalidArgument(format!(
                "sector_size must be a positive power of two (got {})",
                sector_size
            )));
        }
        let file = OpenOptions::new().read(true).open(path.as_ref())?;
        let total_size = file.metadata()?.len();
        Ok(Self {
            file,
            total_size,
            sector_size,
        })
    }
}

impl ReadOnlyDisk for FileBackedDisk {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> CoreResult<()> {
        let end = offset.saturating_add(buf.len() as u64);
        if end > self.total_size {
            return Err(CoreError::OutOfRange {
                what: "disk offset".into(),
                value: offset,
                max: self.total_size,
            });
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)?;
        Ok(())
    }
}

/// `n` が正の 2 の累乗かどうかを返します。
fn is_power_of_two(n: u32) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// テスト専用のテンポラリファイル作成ヘルパ。終了時に削除する責務は呼び出し側。
    fn make_temp_file(contents: &[u8], tag: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let mut path = std::env::temp_dir();
        path.push(format!("dds-disk-io-test-{}-{}-{}.bin", tag, pid, id));
        let mut f = File::create(&path).expect("create temp file");
        f.write_all(contents).expect("write temp file");
        f.sync_all().ok();
        path
    }

    #[test]
    fn file_backed_disk_open_reports_size_and_sector_size() {
        let path = make_temp_file(&vec![0u8; 1024], "size");
        let disk = FileBackedDisk::open(&path).expect("open");
        assert_eq!(disk.total_size(), 1024);
        assert_eq!(disk.sector_size(), DEFAULT_SECTOR_SIZE);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_backed_disk_read_at_returns_expected_bytes() {
        let data: Vec<u8> = (0u8..=63).collect();
        let path = make_temp_file(&data, "read");
        let mut disk = FileBackedDisk::open(&path).expect("open");
        let mut buf = [0u8; 8];
        disk.read_at(10, &mut buf).expect("read_at");
        assert_eq!(buf, [10, 11, 12, 13, 14, 15, 16, 17]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_backed_disk_read_at_out_of_range_returns_error() {
        let path = make_temp_file(&[0u8; 32], "oor");
        let mut disk = FileBackedDisk::open(&path).expect("open");
        let mut buf = [0u8; 8];
        let err = disk.read_at(100, &mut buf).expect_err("must fail");
        match err {
            CoreError::OutOfRange { .. } | CoreError::Io(_) => {}
            other => panic!("unexpected error variant: {:?}", other),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_sector_validates_buffer_size() {
        let path = make_temp_file(&vec![0u8; 1024], "sector");
        let mut disk = FileBackedDisk::open(&path).expect("open");
        let mut buf = [0u8; 100]; // 512 != 100
        let err = disk.read_sector(0, &mut buf).expect_err("must fail");
        assert!(matches!(err, CoreError::InvalidArgument(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_with_invalid_sector_size_returns_error() {
        let path = make_temp_file(&[0u8; 16], "ss");
        let err = FileBackedDisk::open_with_sector_size(&path, 0).expect_err("zero");
        assert!(matches!(err, CoreError::InvalidArgument(_)));
        let err = FileBackedDisk::open_with_sector_size(&path, 300).expect_err("non-pow2");
        assert!(matches!(err, CoreError::InvalidArgument(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn is_power_of_two_truth_table() {
        assert!(!is_power_of_two(0));
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(512));
        assert!(is_power_of_two(4096));
        assert!(!is_power_of_two(300));
        assert!(!is_power_of_two(513));
    }
}
