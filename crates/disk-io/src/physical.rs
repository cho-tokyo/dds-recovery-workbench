//! 物理ディスクの raw アクセス (Windows 専用)。
//!
//! `\\.\PhysicalDriveN` を直接 open して、パーティションテーブルや
//! 壊れた FS にもアクセスできるようにする。R-STUDIO 等の業界標準に倣う。
//!
//! ## アクセスパターン
//!
//! 1. [`enumerate_physical_drives`] で接続中の物理ドライブを列挙
//! 2. [`PhysicalDrive::open`] で個別のドライブを open (read-only)
//! 3. [`PhysicalDrive::read_at`] で raw データを読み取り
//! 4. Drop で自動 close
//!
//! ## 安全性
//!
//! `unsafe` ブロックは本モジュール内のラッパー関数に限定。
//! 引数検証と RAII による自動 close で安全性を確保。
//!
//! ## 重要な制約
//!
//! - **書き込み一切禁止**: open は `GENERIC_READ` のみ (NFR-REL-01 / FR-PHY-03)
//! - **Windows 専用**: 他 OS では `Unsupported` エラー
//!
//! 関連 FR: FR-PHY-01 (物理ドライブ列挙) / FR-PHY-02 (raw 読み取り) / FR-PHY-03 (書込禁止)

use std::path::{Path, PathBuf};
use thiserror::Error;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, HANDLE,
    INVALID_HANDLE_VALUE,
};
#[cfg(windows)]
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, SetFilePointerEx, FILE_BEGIN, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
#[cfg(windows)]
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, GET_LENGTH_INFORMATION,
    IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR,
    STORAGE_PROPERTY_QUERY,
};
#[cfg(windows)]
use windows_sys::Win32::System::IO::DeviceIoControl;

/// `CreateFileW` の `dwDesiredAccess` に渡す読み取り専用アクセスフラグ。
///
/// windows-sys 0.59 では `GENERIC_READ` 定数が公開されていないため、
/// 仕様値 (0x80000000) を直接定義する。**`GENERIC_WRITE` を追加しないこと**。
#[cfg(windows)]
const GENERIC_READ: u32 = 0x8000_0000;

/// `\\.\PhysicalDriveN` パスのプレフィックス。
const PHYSICAL_DRIVE_PREFIX: &str = r"\\.\PhysicalDrive";

/// 物理ドライブ列挙時に試行する最大番号 (排他)。0..16 を順に試す。
const MAX_PHYSICAL_DRIVE_INDEX: u32 = 16;

/// 物理ドライブアクセスのエラー種別。
#[derive(Debug, Error)]
pub enum PhysicalDriveError {
    /// `CreateFileW` がエラーコード付きで失敗した。
    #[error("物理ドライブを open できません: {path} (エラーコード: {code})")]
    OpenFailed {
        /// 対象パス
        path: String,
        /// Win32 エラーコード
        code: u32,
    },

    /// 指定パスのドライブが存在しない (`ERROR_FILE_NOT_FOUND`)。
    #[error("ドライブが見つかりません: {path}")]
    NotFound {
        /// 対象パス
        path: String,
    },

    /// 権限不足で open できなかった (`ERROR_ACCESS_DENIED`)。
    #[error("アクセス拒否: {path} (管理者権限が必要な可能性)")]
    AccessDenied {
        /// 対象パス
        path: String,
    },

    /// `ReadFile` / `SetFilePointerEx` が失敗した。
    #[error("読み取りエラー (offset={offset}, size={size}): エラーコード {code}")]
    ReadFailed {
        /// 読み取り開始オフセット
        offset: u64,
        /// 要求サイズ
        size: usize,
        /// Win32 エラーコード
        code: u32,
    },

    /// `DeviceIoControl` 経由の情報取得に失敗した。
    #[error("ドライブ情報取得失敗: エラーコード {0}")]
    QueryInfoFailed(u32),

    /// `read_at` の引数が無効 (オーバーフロー、範囲外、等)。
    #[error("オフセットまたはサイズが無効: offset={offset}, size={size}")]
    InvalidArgs {
        /// オフセット
        offset: u64,
        /// サイズ
        size: usize,
    },

    /// 非 Windows プラットフォームでの呼び出し。
    #[cfg(not(windows))]
    #[error("物理ドライブアクセスは Windows のみサポートしています")]
    Unsupported,
}

/// 物理ドライブの基本情報スナップショット。
#[derive(Debug, Clone)]
pub struct PhysicalDriveInfo {
    /// パス (例: `\\.\PhysicalDrive0`)
    pub path: PathBuf,
    /// ドライブ番号 (例: `0`)
    pub drive_number: u32,
    /// 総バイトサイズ
    pub total_bytes: u64,
    /// Vendor ID (例: `"Seagate"`)
    pub vendor_id: Option<String>,
    /// Product ID (例: `"ST1000DM003"`)
    pub product_id: Option<String>,
    /// シリアル番号
    pub serial_number: Option<String>,
    /// バスタイプ (USB / SATA / NVMe 等)
    pub bus_type: BusType,
}

/// バスタイプ。`STORAGE_BUS_TYPE` を業務的な表示に変換するための列挙。
#[allow(non_camel_case_types)] // iScsi は Microsoft 表記に合わせる
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    /// 不明 / 未分類
    Unknown,
    /// SCSI
    Scsi,
    /// ATAPI
    Atapi,
    /// ATA
    Ata,
    /// IEEE 1394 (FireWire)
    Ieee1394,
    /// SSA (Serial Storage Architecture)
    Ssa,
    /// Fibre Channel
    Fibre,
    /// USB
    Usb,
    /// RAID
    Raid,
    /// iSCSI
    iScsi,
    /// SAS
    Sas,
    /// SATA
    Sata,
    /// SD カード
    Sd,
    /// MMC
    Mmc,
    /// 仮想ドライブ
    Virtual,
    /// ファイルバック仮想ドライブ
    FileBackedVirtual,
    /// Storage Spaces
    Spaces,
    /// NVMe
    Nvme,
}

impl BusType {
    /// 業務的に「リムーバブル」とみなせるか。
    pub fn is_removable(&self) -> bool {
        matches!(self, Self::Usb | Self::Ieee1394 | Self::Sd | Self::Mmc)
    }

    /// 業務的に表示用の文字列。
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Scsi => "SCSI",
            Self::Atapi => "ATAPI",
            Self::Ata => "ATA",
            Self::Ieee1394 => "1394",
            Self::Ssa => "SSA",
            Self::Fibre => "Fibre",
            Self::Usb => "USB",
            Self::Raid => "RAID",
            Self::iScsi => "iSCSI",
            Self::Sas => "SAS",
            Self::Sata => "SATA",
            Self::Sd => "SD",
            Self::Mmc => "MMC",
            Self::Virtual => "Virtual",
            Self::FileBackedVirtual => "File-Backed Virtual",
            Self::Spaces => "Storage Spaces",
            Self::Nvme => "NVMe",
        }
    }
}

/// 物理ドライブハンドル (RAII で自動 close)。
///
/// `Drop` で `CloseHandle` を呼び出すため、明示的な close は不要。
///
/// **書き込みは型レベルで禁止**: `write_at` 系メソッドは実装しない (NFR-REL-01)。
#[cfg(windows)]
pub struct PhysicalDrive {
    handle: HANDLE,
    info: PhysicalDriveInfo,
}

#[cfg(windows)]
impl PhysicalDrive {
    /// 指定パスの物理ドライブを read-only で open する。
    ///
    /// 管理者権限が必要。`GENERIC_READ` のみで open し、書き込みアクセスは
    /// 一切要求しない (NFR-REL-01 / FR-PHY-03)。
    ///
    /// # 引数
    /// - `path`: `\\.\PhysicalDriveN` 形式のパス
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, PhysicalDriveError> {
        let path: PathBuf = path.into();
        let path_str = path.to_string_lossy().to_string();

        // パスを wide string に変換 (ヌル終端必須)
        let wide_path: Vec<u16> = OsStr::new(&path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY:
        // - wide_path はヌル終端済みかつ as_ptr() で取得した有効なポインタ。
        // - dwDesiredAccess は GENERIC_READ のみ (書き込み一切なし)。
        // - lpSecurityAttributes に NULL を渡すのは Win32 API の通常用法。
        // - htemplatefile に std::ptr::null_mut() を渡すのは Win32 仕様通り (NULL HANDLE)。
        // - 戻り値は INVALID_HANDLE_VALUE チェックで検証する。
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: GetLastError は副作用のない Win32 API。
            let code = unsafe { GetLastError() };
            return Err(match code {
                ERROR_FILE_NOT_FOUND => PhysicalDriveError::NotFound { path: path_str },
                ERROR_ACCESS_DENIED => PhysicalDriveError::AccessDenied { path: path_str },
                _ => PhysicalDriveError::OpenFailed {
                    path: path_str,
                    code,
                },
            });
        }

        // 基本情報取得。失敗時はハンドルを確実に閉じる。
        let drive_number = match parse_drive_number(&path) {
            Ok(n) => n,
            Err(e) => {
                close_handle_silent(handle);
                return Err(e);
            }
        };
        let total_bytes = match get_drive_length(handle) {
            Ok(n) => n,
            Err(e) => {
                close_handle_silent(handle);
                return Err(e);
            }
        };
        let (vendor_id, product_id, serial_number, bus_type) = get_storage_descriptor(handle);

        Ok(Self {
            handle,
            info: PhysicalDriveInfo {
                path,
                drive_number,
                total_bytes,
                vendor_id,
                product_id,
                serial_number,
                bus_type,
            },
        })
    }

    /// 基本情報を取得。
    pub fn info(&self) -> &PhysicalDriveInfo {
        &self.info
    }

    /// オフセットから `size` バイトを読み取る。
    ///
    /// `offset` と `size` は物理セクタ境界 (通常 512 bytes) にアライメント
    /// されている必要はない (Windows 内部でアライメントされる)。ただし、
    /// 物理ドライブのアライメント要件に違反すると `ReadFailed` が返る場合がある。
    pub fn read_at(&self, offset: u64, size: usize) -> Result<Vec<u8>, PhysicalDriveError> {
        if size == 0 {
            return Ok(Vec::new());
        }

        // オーバーフロー検証
        let end = offset
            .checked_add(size as u64)
            .ok_or(PhysicalDriveError::InvalidArgs { offset, size })?;

        if end > self.info.total_bytes {
            return Err(PhysicalDriveError::InvalidArgs { offset, size });
        }

        let mut buffer = vec![0u8; size];

        // ファイルポインタを offset に移動 (i64 で安全に扱える範囲のみ)
        if offset > i64::MAX as u64 {
            return Err(PhysicalDriveError::InvalidArgs { offset, size });
        }
        let distance: i64 = offset as i64;

        // SAFETY:
        // - self.handle は open で取得した有効なハンドル (INVALID_HANDLE_VALUE でないことを保証)。
        // - lpNewFilePointer に NULL を渡すのは Win32 仕様で許可されている。
        // - SetFilePointerEx は副作用がファイルポインタ移動のみで、ディスクには書き込まない。
        let success =
            unsafe { SetFilePointerEx(self.handle, distance, std::ptr::null_mut(), FILE_BEGIN) };

        if success == 0 {
            // SAFETY: GetLastError は副作用なし。
            let code = unsafe { GetLastError() };
            return Err(PhysicalDriveError::ReadFailed { offset, size, code });
        }

        // 読み取り (read-only、WriteFile は一切呼ばない)
        let mut bytes_read: u32 = 0;
        // SAFETY:
        // - self.handle は有効。
        // - buffer は size バイト確保済み。as_mut_ptr() は有効なポインタ。
        // - size as u32 はバウンダリ確認 (32bit 範囲) を前提とする。
        //   実用上 read_at は数 KiB 〜 数 MiB 程度で呼ぶため安全。
        let size_u32 =
            u32::try_from(size).map_err(|_| PhysicalDriveError::InvalidArgs { offset, size })?;
        let ok = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr(),
                size_u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        if ok == 0 {
            // SAFETY: GetLastError は副作用なし。
            let code = unsafe { GetLastError() };
            return Err(PhysicalDriveError::ReadFailed { offset, size, code });
        }

        buffer.truncate(bytes_read as usize);
        Ok(buffer)
    }
}

#[cfg(windows)]
impl Drop for PhysicalDrive {
    fn drop(&mut self) {
        close_handle_silent(self.handle);
    }
}

#[cfg(windows)]
fn close_handle_silent(handle: HANDLE) {
    if handle != INVALID_HANDLE_VALUE && !handle.is_null() {
        // SAFETY: handle が INVALID_HANDLE_VALUE/NULL でないことを直前に確認。
        // CloseHandle の失敗は無視 (Drop コンテキストで panic できないため)。
        unsafe {
            CloseHandle(handle);
        }
    }
}

/// 非 Windows プラットフォーム用のスタブ実装。
#[cfg(not(windows))]
#[derive(Debug)]
pub struct PhysicalDrive;

#[cfg(not(windows))]
impl PhysicalDrive {
    /// 非 Windows では常に `Unsupported` を返す。
    pub fn open(_path: impl Into<PathBuf>) -> Result<Self, PhysicalDriveError> {
        Err(PhysicalDriveError::Unsupported)
    }

    /// 非 Windows では呼び出し不能。
    pub fn info(&self) -> &PhysicalDriveInfo {
        unreachable!("non-Windows PhysicalDrive cannot be instantiated")
    }

    /// 非 Windows では常に `Unsupported` を返す。
    pub fn read_at(&self, _offset: u64, _size: usize) -> Result<Vec<u8>, PhysicalDriveError> {
        Err(PhysicalDriveError::Unsupported)
    }
}

/// 接続中の物理ドライブを列挙する (Windows 版)。
///
/// `\\.\PhysicalDrive0` から `\\.\PhysicalDrive15` まで順次 open を試み、
/// 成功したドライブの [`PhysicalDriveInfo`] を返す。
///
/// 管理者権限が必要 (権限不足のドライブはスキップ)。
/// 関連 FR: FR-PHY-01。
#[cfg(windows)]
pub fn enumerate_physical_drives() -> Vec<PhysicalDriveInfo> {
    let mut drives = Vec::new();

    for i in 0..MAX_PHYSICAL_DRIVE_INDEX {
        let path = format!("{}{}", PHYSICAL_DRIVE_PREFIX, i);
        match PhysicalDrive::open(&path) {
            Ok(drive) => {
                drives.push(drive.info().clone());
                // drive は Drop で auto close
            }
            Err(_) => {
                // NotFound / AccessDenied / その他いずれもスキップして継続。
                // 歯抜けの可能性があるため break しない。
                continue;
            }
        }
    }

    drives
}

/// 接続中の物理ドライブを列挙する (非 Windows スタブ、常に空)。
#[cfg(not(windows))]
pub fn enumerate_physical_drives() -> Vec<PhysicalDriveInfo> {
    Vec::new()
}

/// パスからドライブ番号を抽出する (例: `\\.\PhysicalDrive3` → `3`)。
fn parse_drive_number(path: &Path) -> Result<u32, PhysicalDriveError> {
    let path_str = path.to_string_lossy();
    if let Some(num_str) = path_str.strip_prefix(PHYSICAL_DRIVE_PREFIX) {
        num_str
            .parse::<u32>()
            .map_err(|_| PhysicalDriveError::OpenFailed {
                path: path_str.into_owned(),
                code: 0,
            })
    } else {
        Err(PhysicalDriveError::OpenFailed {
            path: path_str.into_owned(),
            code: 0,
        })
    }
}

/// `IOCTL_DISK_GET_LENGTH_INFO` 経由でドライブの総バイトサイズを取得する。
#[cfg(windows)]
fn get_drive_length(handle: HANDLE) -> Result<u64, PhysicalDriveError> {
    // SAFETY: GET_LENGTH_INFORMATION は POD (i64 1 つ)。zeroed で安全に初期化可能。
    let mut length_info: GET_LENGTH_INFORMATION = unsafe { std::mem::zeroed() };
    let mut bytes_returned: u32 = 0;

    // SAFETY:
    // - handle は有効。
    // - IOCTL_DISK_GET_LENGTH_INFO は read-only IOCTL (書き込みなし、ディスクへの影響なし)。
    // - 出力バッファは GET_LENGTH_INFORMATION のサイズ分確保済み。
    // - bytes_returned は有効なポインタ。
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            std::ptr::null(),
            0,
            (&mut length_info as *mut GET_LENGTH_INFORMATION).cast::<std::ffi::c_void>(),
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    if ok == 0 {
        // SAFETY: GetLastError は副作用なし。
        let code = unsafe { GetLastError() };
        return Err(PhysicalDriveError::QueryInfoFailed(code));
    }

    Ok(length_info.Length as u64)
}

/// `IOCTL_STORAGE_QUERY_PROPERTY` 経由で Vendor/Product/Serial/BusType を取得する。
///
/// 失敗時は `(None, None, None, BusType::Unknown)` を返し、エラーで panic しない。
#[cfg(windows)]
fn get_storage_descriptor(
    handle: HANDLE,
) -> (Option<String>, Option<String>, Option<String>, BusType) {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };

    // 大きめのバッファ (STORAGE_DEVICE_DESCRIPTOR + 可変長文字列領域)
    let mut buffer = vec![0u8; 1024];
    let mut bytes_returned: u32 = 0;

    // SAFETY:
    // - handle は有効。
    // - IOCTL_STORAGE_QUERY_PROPERTY は read-only IOCTL。
    // - query/buffer は十分なサイズで初期化済み。
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&query as *const STORAGE_PROPERTY_QUERY).cast::<std::ffi::c_void>(),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buffer.as_mut_ptr().cast::<std::ffi::c_void>(),
            buffer.len() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };

    if ok == 0 || (bytes_returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
        return (None, None, None, BusType::Unknown);
    }

    // SAFETY:
    // - buffer は STORAGE_DEVICE_DESCRIPTOR より十分大きい (1024 バイト)。
    // - DeviceIoControl が成功し、bytes_returned で descriptor サイズ以上が保証されている。
    // - STORAGE_DEVICE_DESCRIPTOR は #[repr(C)]、ライフタイムは buffer に紐付く。
    let descriptor = unsafe { &*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };

    let vendor_id = extract_descriptor_string(&buffer, descriptor.VendorIdOffset);
    let product_id = extract_descriptor_string(&buffer, descriptor.ProductIdOffset);
    let serial_number = extract_descriptor_string(&buffer, descriptor.SerialNumberOffset);
    let bus_type = map_bus_type(descriptor.BusType);

    (vendor_id, product_id, serial_number, bus_type)
}

/// STORAGE_DEVICE_DESCRIPTOR の文字列フィールドを安全に読み取るヘルパ。
///
/// `offset` が 0 または範囲外の場合は `None` を返す。ヌル終端まで読み取り、
/// UTF-8 として妥当な部分文字列を `String` に変換する。
#[cfg(windows)]
fn extract_descriptor_string(buffer: &[u8], offset: u32) -> Option<String> {
    if offset == 0 {
        return None;
    }
    let start = offset as usize;
    if start >= buffer.len() {
        return None;
    }
    let mut end = start;
    while end < buffer.len() && buffer[end] != 0 {
        end += 1;
    }
    if start == end {
        return None;
    }
    std::str::from_utf8(&buffer[start..end])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Windows の `STORAGE_BUS_TYPE` 値を [`BusType`] に変換する。
#[cfg(windows)]
fn map_bus_type(value: i32) -> BusType {
    match value {
        1 => BusType::Scsi,
        2 => BusType::Atapi,
        3 => BusType::Ata,
        4 => BusType::Ieee1394,
        5 => BusType::Ssa,
        6 => BusType::Fibre,
        7 => BusType::Usb,
        8 => BusType::Raid,
        9 => BusType::iScsi,
        10 => BusType::Sas,
        11 => BusType::Sata,
        12 => BusType::Sd,
        13 => BusType::Mmc,
        14 => BusType::Virtual,
        15 => BusType::FileBackedVirtual,
        16 => BusType::Spaces,
        17 => BusType::Nvme,
        _ => BusType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_drive_number_extracts_correctly() {
        assert_eq!(
            parse_drive_number(Path::new(r"\\.\PhysicalDrive0")).unwrap(),
            0
        );
        assert_eq!(
            parse_drive_number(Path::new(r"\\.\PhysicalDrive5")).unwrap(),
            5
        );
        assert_eq!(
            parse_drive_number(Path::new(r"\\.\PhysicalDrive15")).unwrap(),
            15
        );
    }

    #[test]
    fn parse_drive_number_invalid_returns_error() {
        assert!(parse_drive_number(Path::new(r"\\.\C:")).is_err());
        assert!(parse_drive_number(Path::new(r"\\.\PhysicalDriveXX")).is_err());
        assert!(parse_drive_number(Path::new(r"\\.\PhysicalDrive")).is_err());
    }

    #[test]
    fn bus_type_display_names() {
        assert_eq!(BusType::Usb.display_name(), "USB");
        assert_eq!(BusType::Sata.display_name(), "SATA");
        assert_eq!(BusType::Nvme.display_name(), "NVMe");
        assert_eq!(BusType::Unknown.display_name(), "Unknown");
        assert_eq!(BusType::iScsi.display_name(), "iSCSI");
    }

    #[test]
    fn bus_type_removable_detection() {
        assert!(BusType::Usb.is_removable());
        assert!(BusType::Sd.is_removable());
        assert!(BusType::Ieee1394.is_removable());
        assert!(BusType::Mmc.is_removable());
        assert!(!BusType::Sata.is_removable());
        assert!(!BusType::Nvme.is_removable());
        assert!(!BusType::Unknown.is_removable());
    }

    #[cfg(windows)]
    #[test]
    fn map_bus_type_covers_known_values() {
        assert_eq!(map_bus_type(7), BusType::Usb);
        assert_eq!(map_bus_type(11), BusType::Sata);
        assert_eq!(map_bus_type(17), BusType::Nvme);
        assert_eq!(map_bus_type(0), BusType::Unknown);
        assert_eq!(map_bus_type(99), BusType::Unknown);
    }

    #[cfg(windows)]
    #[test]
    fn enumerate_returns_at_least_system_drive() {
        // 管理者権限なし環境では空 Vec が返ることもあるため、panic しないことだけ確認。
        let drives = enumerate_physical_drives();
        println!("検出された物理ドライブ: {} 個", drives.len());
        for drive in &drives {
            println!("  {:?}", drive);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_returns_empty_list() {
        let drives = enumerate_physical_drives();
        assert!(drives.is_empty());
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_open_returns_unsupported() {
        let result = PhysicalDrive::open(r"\\.\PhysicalDrive0");
        assert!(matches!(result, Err(PhysicalDriveError::Unsupported)));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "管理者権限が必要なローカル検証用テスト"]
    fn integration_open_and_read_physical_drive_0() {
        // 管理者権限で実行することを想定 (CI ではスキップ)
        let result = PhysicalDrive::open(r"\\.\PhysicalDrive0");
        let drive = match result {
            Err(PhysicalDriveError::AccessDenied { .. }) => {
                println!("管理者権限なし、スキップ");
                return;
            }
            Err(e) => panic!("open に失敗: {:?}", e),
            Ok(d) => d,
        };

        println!("Drive 0: {:?}", drive.info());
        let mbr = drive.read_at(0, 512).expect("MBR 読み取り失敗");
        assert_eq!(mbr.len(), 512);
        // MBR シグネチャ (0x55AA at offset 510)
        println!("MBR シグネチャ: 0x{:02X}{:02X}", mbr[511], mbr[510]);
    }
}
