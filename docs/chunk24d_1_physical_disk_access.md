# Chunk 24d-1 指示: 物理ディスクアクセス層

Phase 1.5 拡張 (壊れた FS の HDD 対応) の **第 1 段階**。物理ドライブを raw レベルでアクセスする基盤を構築する。

> 🎯 完了時点で「Windows が認識する物理ドライブを workbench-dryrun が列挙し、基本情報を取得できる」状態に到達。続く Chunk 24d-2 (パーティション解析)、24d-3 (NtfsVolume 統合) の基盤になる。

---

## 全体像 (Chunk 24d シリーズ)

```
[Chunk 24d-1] 物理ディスクアクセス層 ← 本指示書、3-4 日
  - 物理ドライブ列挙、open/close
  - 基本情報取得 (サイズ、Vendor、Product)
  - raw read API

[Chunk 24d-2] パーティションテーブル解析 (次の指示書、2-3 日)
  - MBR / GPT パーサー
  - パーティション内 FS タイプ判定

[Chunk 24d-3] 既存システムとの統合 (3-4 日)
  - NtfsVolume を物理パーティションで open
  - workbench-dryrun の --physical オプション完全対応

[Chunk 24d-4] 実機テストとフィードバック反映 (2-3 日)
  - 壊れた FS の HDD でドライラン
  - 微調整、README 更新
```

本チャンクは段階の最初。**スコープを厳密に限定**することで、リスクを抑えて進める。

## 背景: なぜ物理ディスクモードが必要か

実機ドライランで判明:
```
[問題]
壊れた FS の HDD:
  Windows がドライブレターを割り当てる (認識)
  しかし NTFS マウントできない (FS 壊れ)
  ↓
  workbench-dryrun の現状: 「NTFS ドライブが見つかりません」エラー
  ↓
  業務的に致命的 (壊れた FS こそ復旧対象)
```

業界標準 (R-STUDIO 等) は物理ドライブから raw アクセスする。Workbench も同じ方式を実装する必要がある。

## 本チャンクのスコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | 物理ドライブ列挙 (`\\.\PhysicalDrive0` ~ `\\.\PhysicalDrive15`) |
| **B** | 物理ドライブの open / close (RAII) |
| **C** | 基本情報取得 (サイズ、Vendor、Product、Serial、BusType) |
| **D** | raw read API (オフセット指定で読み取り) |
| **E** | workbench-dryrun list-drives に `--physical` オプション (最小実装) |

### 含まないもの (次以降のチャンクで実装)

```
✗ パーティションテーブル解析 (Chunk 24d-2)
✗ FS タイプ判定 (Chunk 24d-2)
✗ NtfsVolume との統合 (Chunk 24d-3)
✗ diagnose / recover の --physical 対応 (Chunk 24d-3)
✗ 不良セクタ対応 (Chunk 24d-3 or 別チャンク)
```

これらは本チャンクのスコープ外。混入を避ける。

## 対象クレート

- **修正**: `crates/disk-io/` (既存クレートに `physical` モジュール追加)
- **修正**: `crates/workbench-dryrun/` (`--physical` オプション)
- **新規依存**: `windows-sys` の追加 features (既に Chunk 24a で使用、機能追加のみ)

## 重要な設計原則

### unsafe の追加範囲

```
[Chunk 24a 後の現状]
crates/recovery/src/timestamps.rs: 5-10 行 (タイムスタンプ書き込み)

[Chunk 24d-1 後]
crates/disk-io/src/physical.rs: 約 30 行追加
  - CreateFileW (物理ドライブ open)
  - ReadFile (raw read)
  - DeviceIoControl (IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_QUERY_PROPERTY)
  
合計: 約 35-40 行 (業務的に必要な範囲)
```

### 安全性確保策

```
1. unsafe は physical.rs 内のラッパー関数に限定
2. RAII による HANDLE の自動 close (Drop trait)
3. 引数検証 (offset, size の妥当性チェック)
4. エラーハンドリング (Win32 エラーコードを Rust エラーに変換)
5. 単体テストで主要 API の動作検証
```

### Windows 専用

物理ドライブアクセスは Windows API 必須。`#[cfg(windows)]` でガード。Linux/macOS では `Unsupported` エラーを返す。

### 非破壊原則

```
[絶対に守ること]
物理ドライブへの書き込みを一切しない
- CreateFileW は GENERIC_READ のみ (GENERIC_WRITE 禁止)
- IOCTL の write 系操作禁止
- 既存のお客様の HDD を絶対に壊さない
```

これは業務的に最重要。データ復旧ツールがソース HDD を壊したら業務終了。

## 仕様参照

### ビジネス要件

- **FR-PHY-01** (物理ドライブ列挙) ← 新規達成
- **FR-PHY-02** (物理ドライブ raw 読み取り) ← 新規達成
- **FR-PHY-03** (ソース HDD への書き込み禁止) ← 既存原則、本チャンクでも厳守

## 実装内容

### Part A: `crates/disk-io/src/physical.rs` (新規ファイル)

```rust
//! 物理ディスクの raw アクセス (Windows 専用).
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
//! - **書き込み一切禁止**: open は `GENERIC_READ` のみ
//! - **Windows 専用**: 他 OS では `Unsupported` エラー

use std::path::PathBuf;
use thiserror::Error;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::ffi::OsStr;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    ERROR_FILE_NOT_FOUND, ERROR_ACCESS_DENIED,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
#[cfg(windows)]
use windows_sys::Win32::System::IO::DeviceIoControl;
#[cfg(windows)]
use windows_sys::Win32::System::Ioctl::{
    IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_QUERY_PROPERTY,
    GET_LENGTH_INFORMATION, STORAGE_PROPERTY_QUERY,
    STORAGE_DEVICE_DESCRIPTOR, StorageDeviceProperty, PropertyStandardQuery,
};
#[cfg(windows)]
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
#[cfg(windows)]
const GENERIC_READ: u32 = 0x80000000;

/// 物理ドライブアクセスのエラー
#[derive(Debug, Error)]
pub enum PhysicalDriveError {
    #[error("物理ドライブを open できません: {path} (エラーコード: {code})")]
    OpenFailed { path: String, code: u32 },
    
    #[error("ドライブが見つかりません: {path}")]
    NotFound { path: String },
    
    #[error("アクセス拒否: {path} (管理者権限が必要な可能性)")]
    AccessDenied { path: String },
    
    #[error("読み取りエラー (offset={offset}, size={size}): エラーコード {code}")]
    ReadFailed { offset: u64, size: usize, code: u32 },
    
    #[error("ドライブ情報取得失敗: エラーコード {0}")]
    QueryInfoFailed(u32),
    
    #[error("オフセットまたはサイズが無効: offset={offset}, size={size}")]
    InvalidArgs { offset: u64, size: usize },
    
    #[cfg(not(windows))]
    #[error("物理ドライブアクセスは Windows のみサポートしています")]
    Unsupported,
}

/// 物理ドライブの基本情報
#[derive(Debug, Clone)]
pub struct PhysicalDriveInfo {
    /// パス (例: `\\.\PhysicalDrive0`)
    pub path: PathBuf,
    
    /// ドライブ番号 (例: 0)
    pub drive_number: u32,
    
    /// 総バイトサイズ
    pub total_bytes: u64,
    
    /// Vendor ID (例: "Seagate")
    pub vendor_id: Option<String>,
    
    /// Product ID (例: "ST1000DM003")
    pub product_id: Option<String>,
    
    /// Serial Number
    pub serial_number: Option<String>,
    
    /// Bus Type (例: "USB", "SATA", "NVMe")
    pub bus_type: BusType,
}

/// バスタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    Unknown,
    Scsi,
    Atapi,
    Ata,
    Ieee1394,
    Ssa,
    Fibre,
    Usb,
    Raid,
    iScsi,
    Sas,
    Sata,
    Sd,
    Mmc,
    Virtual,
    FileBackedVirtual,
    Spaces,
    Nvme,
}

impl BusType {
    /// 業務的に「リムーバブル」とみなせるか
    pub fn is_removable(&self) -> bool {
        matches!(self, Self::Usb | Self::Ieee1394 | Self::Sd | Self::Mmc)
    }
    
    /// 業務的に表示用の文字列
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

/// 物理ドライブハンドル (RAII で自動 close)
#[cfg(windows)]
pub struct PhysicalDrive {
    handle: HANDLE,
    info: PhysicalDriveInfo,
}

#[cfg(windows)]
impl PhysicalDrive {
    /// 指定パスの物理ドライブを read-only で open する。
    ///
    /// 管理者権限が必要。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, PhysicalDriveError> {
        let path: PathBuf = path.into();
        let path_str = path.to_string_lossy().to_string();
        
        // パスを wide string に変換
        let wide_path: Vec<u16> = OsStr::new(&path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        
        // SAFETY:
        // - wide_path はヌル終端されている
        // - 他の引数は Windows API の仕様通り
        // - GENERIC_READ のみで open (書き込み禁止の強制)
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,  // 他プロセスとの共有
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut::<std::ffi::c_void>() as HANDLE,
            )
        };
        
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: GetLastError は副作用のない Windows API
            let code = unsafe { GetLastError() };
            return Err(match code {
                ERROR_FILE_NOT_FOUND => PhysicalDriveError::NotFound { path: path_str },
                ERROR_ACCESS_DENIED => PhysicalDriveError::AccessDenied { path: path_str },
                _ => PhysicalDriveError::OpenFailed { path: path_str, code },
            });
        }
        
        // 基本情報取得
        let drive_number = parse_drive_number(&path)?;
        let total_bytes = get_drive_length(handle)?;
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
    
    /// 基本情報を取得
    pub fn info(&self) -> &PhysicalDriveInfo {
        &self.info
    }
    
    /// オフセットからサイズ分のデータを読み取る
    ///
    /// `offset` と `size` は物理セクタ境界 (通常 512 bytes) に
    /// アライメントされている必要はない (Windows 内部でアライメントされる)
    pub fn read_at(&self, offset: u64, size: usize) -> Result<Vec<u8>, PhysicalDriveError> {
        if size == 0 {
            return Ok(Vec::new());
        }
        
        if offset.checked_add(size as u64).is_none() {
            return Err(PhysicalDriveError::InvalidArgs { offset, size });
        }
        
        if offset + (size as u64) > self.info.total_bytes {
            return Err(PhysicalDriveError::InvalidArgs { offset, size });
        }
        
        let mut buffer = vec![0u8; size];
        
        // ファイルポインタを offset に移動
        let high = ((offset >> 32) & 0xFFFFFFFF) as i32;
        let low = (offset & 0xFFFFFFFF) as u32;
        let high_ptr = &high as *const i32 as *mut i32;
        
        // SAFETY:
        // - self.handle は open で取得した有効なハンドル
        // - high_ptr は high: i32 への有効なポインタ
        // - SetFilePointer は Windows API の標準的な使用方法
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFilePointer(
                self.handle,
                low as i32,
                high_ptr,
                windows_sys::Win32::Storage::FileSystem::FILE_BEGIN,
            )
        };
        
        if result == windows_sys::Win32::Storage::FileSystem::INVALID_SET_FILE_POINTER {
            // SAFETY: GetLastError は副作用のない Windows API
            let code = unsafe { GetLastError() };
            return Err(PhysicalDriveError::ReadFailed { offset, size, code });
        }
        
        // 読み取り
        let mut bytes_read: u32 = 0;
        // SAFETY:
        // - self.handle は有効なハンドル
        // - buffer は size バイト確保済み
        // - bytes_read は有効なポインタ
        let success = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                size as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };
        
        if success == 0 {
            // SAFETY: GetLastError は副作用のない Windows API
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
        if self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: handle は open で取得した有効なハンドル
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(not(windows))]
pub struct PhysicalDrive;

#[cfg(not(windows))]
impl PhysicalDrive {
    pub fn open(_path: impl Into<PathBuf>) -> Result<Self, PhysicalDriveError> {
        Err(PhysicalDriveError::Unsupported)
    }
    
    pub fn info(&self) -> &PhysicalDriveInfo {
        unimplemented!()
    }
    
    pub fn read_at(&self, _offset: u64, _size: usize) -> Result<Vec<u8>, PhysicalDriveError> {
        Err(PhysicalDriveError::Unsupported)
    }
}

/// 接続中の物理ドライブを列挙する。
///
/// `\\.\PhysicalDrive0` から `\\.\PhysicalDrive15` まで順次 open を試み、
/// 成功したものを返す。
///
/// 管理者権限が必要 (権限不足のドライブはスキップ)。
#[cfg(windows)]
pub fn enumerate_physical_drives() -> Vec<PhysicalDriveInfo> {
    let mut drives = Vec::new();
    
    for i in 0..16u32 {
        let path = format!(r"\\.\PhysicalDrive{}", i);
        
        match PhysicalDrive::open(&path) {
            Ok(drive) => {
                drives.push(drive.info().clone());
                // drive は drop で自動 close
            }
            Err(PhysicalDriveError::NotFound { .. }) => {
                // ドライブが存在しない、列挙終了の可能性
                // ただし続けて試す (歯抜けの可能性)
                continue;
            }
            Err(PhysicalDriveError::AccessDenied { .. }) => {
                // 権限不足、スキップ
                continue;
            }
            Err(_) => {
                // その他のエラー、スキップ
                continue;
            }
        }
    }
    
    drives
}

#[cfg(not(windows))]
pub fn enumerate_physical_drives() -> Vec<PhysicalDriveInfo> {
    Vec::new()
}

/// パスからドライブ番号を抽出する。
///
/// 例: `\\.\PhysicalDrive3` → `3`
fn parse_drive_number(path: &std::path::Path) -> Result<u32, PhysicalDriveError> {
    let path_str = path.to_string_lossy();
    let prefix = r"\\.\PhysicalDrive";
    
    if let Some(num_str) = path_str.strip_prefix(prefix) {
        num_str.parse::<u32>().map_err(|_| {
            PhysicalDriveError::OpenFailed {
                path: path_str.into_owned(),
                code: 0,
            }
        })
    } else {
        Err(PhysicalDriveError::OpenFailed {
            path: path_str.into_owned(),
            code: 0,
        })
    }
}

/// ドライブの総バイトサイズを取得
#[cfg(windows)]
fn get_drive_length(handle: HANDLE) -> Result<u64, PhysicalDriveError> {
    let mut length_info: GET_LENGTH_INFORMATION = unsafe { std::mem::zeroed() };
    let mut bytes_returned: u32 = 0;
    
    // SAFETY:
    // - handle は有効なハンドル
    // - length_info は zeroed initialized
    // - bytes_returned は有効なポインタ
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            std::ptr::null_mut(),
            0,
            &mut length_info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    
    if success == 0 {
        let code = unsafe { GetLastError() };
        return Err(PhysicalDriveError::QueryInfoFailed(code));
    }
    
    Ok(length_info.Length as u64)
}

/// ドライブの Vendor/Product/Serial 等を取得
#[cfg(windows)]
fn get_storage_descriptor(
    handle: HANDLE,
) -> (Option<String>, Option<String>, Option<String>, BusType) {
    // STORAGE_PROPERTY_QUERY を準備
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0; 1],
    };
    
    // 大きめのバッファを確保 (STORAGE_DEVICE_DESCRIPTOR + 可変長文字列)
    let mut buffer = vec![0u8; 1024];
    let mut bytes_returned: u32 = 0;
    
    // SAFETY:
    // - handle は有効
    // - query, buffer は初期化済み
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *mut std::ffi::c_void,
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            buffer.len() as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    
    if success == 0 || (bytes_returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
        return (None, None, None, BusType::Unknown);
    }
    
    // SAFETY:
    // - buffer は STORAGE_DEVICE_DESCRIPTOR より大きい
    // - データは Windows API から返された有効な値
    let descriptor = unsafe {
        &*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR)
    };
    
    let extract_string = |offset: u32| -> Option<String> {
        if offset == 0 { return None; }
        let start = offset as usize;
        if start >= buffer.len() { return None; }
        
        // ヌル終端まで読む
        let mut end = start;
        while end < buffer.len() && buffer[end] != 0 {
            end += 1;
        }
        
        if start == end { return None; }
        
        std::str::from_utf8(&buffer[start..end])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    
    let vendor_id = extract_string(descriptor.VendorIdOffset);
    let product_id = extract_string(descriptor.ProductIdOffset);
    let serial_number = extract_string(descriptor.SerialNumberOffset);
    
    let bus_type = match descriptor.BusType {
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
    };
    
    (vendor_id, product_id, serial_number, bus_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn parse_drive_number_extracts_correctly() {
        assert_eq!(parse_drive_number(std::path::Path::new(r"\\.\PhysicalDrive0")).unwrap(), 0);
        assert_eq!(parse_drive_number(std::path::Path::new(r"\\.\PhysicalDrive5")).unwrap(), 5);
        assert_eq!(parse_drive_number(std::path::Path::new(r"\\.\PhysicalDrive15")).unwrap(), 15);
    }
    
    #[test]
    fn parse_drive_number_invalid_returns_error() {
        assert!(parse_drive_number(std::path::Path::new(r"\\.\C:")).is_err());
        assert!(parse_drive_number(std::path::Path::new(r"\\.\PhysicalDriveXX")).is_err());
    }
    
    #[test]
    fn bus_type_display_names() {
        assert_eq!(BusType::Usb.display_name(), "USB");
        assert_eq!(BusType::Sata.display_name(), "SATA");
        assert_eq!(BusType::Nvme.display_name(), "NVMe");
    }
    
    #[test]
    fn bus_type_removable_detection() {
        assert!(BusType::Usb.is_removable());
        assert!(BusType::Sd.is_removable());
        assert!(!BusType::Sata.is_removable());
        assert!(!BusType::Nvme.is_removable());
    }
    
    #[cfg(windows)]
    #[test]
    fn enumerate_returns_at_least_system_drive() {
        // 管理者権限で実行されている場合、最低でも PhysicalDrive0 (C: 含む) が見える
        // CI 環境では権限不足の可能性があるので、エラーにはしない
        let drives = enumerate_physical_drives();
        println!("検出された物理ドライブ: {} 個", drives.len());
        for drive in &drives {
            println!("  {:?}", drive);
        }
        // 管理者権限なしでも、エラーで panic しないことだけ確認
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
}
```

### Part B: `crates/disk-io/src/lib.rs` への追加

```rust
// 既存:
pub mod logical;

// 新規追加:
pub mod physical;

// 公開 API:
pub use physical::{
    enumerate_physical_drives, PhysicalDrive, PhysicalDriveError, PhysicalDriveInfo, BusType,
};
```

### Part C: `crates/disk-io/Cargo.toml` の依存追加

```toml
[dependencies]
# 既存:
windows-sys = { version = "0.52", features = ["Win32_Foundation", "Win32_Storage_FileSystem"] }

# 追加 features:
windows-sys = { version = "0.52", features = [
    "Win32_Foundation",
    "Win32_Storage_FileSystem",
    "Win32_System_IO",
    "Win32_System_Ioctl",          # 新規
    "Win32_Security",              # 新規
] }
```

### Part D: workbench-dryrun の `list-drives --physical` 対応

`crates/workbench-dryrun/src/commands/list_drives.rs`:

```rust
use clap::Args;
use anyhow::Result;
use dds_disk_io::{enumerate_physical_drives, BusType};

use crate::drives::list_logical_drives;

#[derive(Args, Debug)]
pub struct ListDrivesArgs {
    /// 物理ドライブを列挙 (壊れた FS の HDD 等)
    #[arg(long)]
    pub physical: bool,
}

pub fn run(args: &ListDrivesArgs) -> Result<()> {
    if args.physical {
        run_physical()
    } else {
        run_logical()
    }
}

fn run_logical() -> Result<()> {
    // 既存実装 (論理ドライブ列挙)
    println!("論理ドライブ:");
    let drives = list_logical_drives();
    // ... 既存表示 ...
    Ok(())
}

fn run_physical() -> Result<()> {
    println!("物理ドライブ:");
    println!("---------------------------------------------");
    
    let drives = enumerate_physical_drives();
    
    if drives.is_empty() {
        println!("物理ドライブが検出されませんでした。");
        println!();
        println!("考えられる原因:");
        println!("  1. 管理者権限がない (「管理者として実行」で起動してください)");
        println!("  2. 物理的に接続されているドライブがない");
        println!("  3. ドライバの問題");
        return Ok(());
    }
    
    println!("検出された物理ドライブ: {} 個", drives.len());
    println!();
    
    for drive in &drives {
        println!("[{}] {}", drive.drive_number, drive.path.display());
        println!("    サイズ:    {}", dds_core::format::format_bytes(drive.total_bytes));
        if let Some(vendor) = &drive.vendor_id {
            println!("    Vendor:    {}", vendor);
        }
        if let Some(product) = &drive.product_id {
            println!("    Product:   {}", product);
        }
        if let Some(serial) = &drive.serial_number {
            println!("    Serial:    {}", serial);
        }
        println!("    Bus Type:  {} {}", 
            drive.bus_type.display_name(),
            if drive.bus_type.is_removable() { "(リムーバブル)" } else { "" });
        println!();
    }
    
    println!("---------------------------------------------");
    println!("注: パーティション情報は Chunk 24d-2 で追加予定");
    println!("    現状は物理ドライブの一覧のみ");
    
    Ok(())
}
```

### Part E: `crates/workbench-dryrun/src/main.rs` の修正

```rust
// 既存の list-drives コマンドを ListDrivesArgs を受け取るように変更
#[derive(Subcommand)]
enum Commands {
    ListDrives(commands::list_drives::ListDrivesArgs),  // ★ Args を受け取る
    Diagnose(commands::diagnose::DiagnoseArgs),
    Recover(commands::recover::RecoverArgs),
    Show(commands::show::ShowArgs),
}

// main の dispatch を更新
Commands::ListDrives(args) => commands::list_drives::run(&args),
```

## 単体テスト要件 (最低 8 件)

`crates/disk-io/src/physical.rs` に含まれるテスト (上記コード参照):

1. `parse_drive_number_extracts_correctly`
2. `parse_drive_number_invalid_returns_error`
3. `bus_type_display_names`
4. `bus_type_removable_detection`
5. `enumerate_returns_at_least_system_drive` (Windows のみ、管理者権限あれば最低 1 つ)
6. `non_windows_returns_empty_list` (non-Windows のみ)
7. `non_windows_open_returns_unsupported` (non-Windows のみ)

### 統合テスト (任意、Windows + 管理者権限のみ)

```rust
#[cfg(windows)]
#[test]
#[ignore]  // CI では実行しない、ローカル検証用
fn integration_open_and_read_physical_drive_0() {
    // 管理者権限で実行
    let result = PhysicalDrive::open(r"\\.\PhysicalDrive0");
    if let Err(PhysicalDriveError::AccessDenied { .. }) = result {
        println!("管理者権限が必要、スキップ");
        return;
    }
    
    let drive = result.unwrap();
    println!("Drive 0: {:?}", drive.info());
    
    // 先頭 512 バイト読み取り (MBR)
    let mbr = drive.read_at(0, 512).unwrap();
    assert_eq!(mbr.len(), 512);
    
    // MBR シグネチャ (0x55AA at offset 510)
    println!("MBR シグネチャ: 0x{:02X}{:02X}", mbr[511], mbr[510]);
}
```

## 制約

- **行数目安**:
  - `crates/disk-io/src/physical.rs` (新規): 約 500 行 + テスト 100 行
  - `crates/disk-io/src/lib.rs` 修正: +5 行 (公開 API)
  - `crates/disk-io/Cargo.toml` 修正: +3 行 (features)
  - `crates/workbench-dryrun/src/commands/list_drives.rs` 修正: +50 行 (--physical 対応)
  - `crates/workbench-dryrun/src/main.rs` 修正: +5 行
  - 合計: 約 660 行追加・修正
- **単体テスト新規**: 最低 7 件
- **統合テスト**: 1 件 (`#[ignore]` 付き、ローカル検証用)
- **`unsafe` 追加行数**: 約 30 行 (physical.rs 内のラッパー関数群)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] `unsafe` ブロックは `crates/disk-io/src/physical.rs` の関数内に限定
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (24a の 5-10 行 + 24d-1 の 30 行)
- [ ] `workbench-dryrun list-drives --physical` が動作 (管理者権限あれば)
- [ ] 物理ドライブ列挙でシステムドライブ (PhysicalDrive0) を検出
- [ ] open は read-only のみ (`GENERIC_READ` のみ、`GENERIC_WRITE` 含まず)

## 関連 FR 要件

- **FR-PHY-01** (物理ドライブ列挙) ← 達成
- **FR-PHY-02** (物理ドライブ raw 読み取り) ← 達成
- **FR-PHY-03** (ソース HDD への書き込み禁止) ← 厳守

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **Chouさんが軽い動作確認 (オプション)**:
   ```cmd
   > workbench-dryrun list-drives --physical
   [物理ドライブ一覧が表示されるか確認]
   ```
4. **次のチャンク: Chunk 24d-2 (パーティションテーブル解析)**

---

## 注意事項

### 物理ドライブ列挙の手法選択

```
[選択: 方法 A]
\\.\PhysicalDrive0 〜 15 を順次 open 試行

[他の選択肢 (採用せず)]
- 方法 B: SetupAPI (デバイスマネージャと同じ) → 複雑
- 方法 C: WMI 経由 → COM 依存、複雑
```

方法 A の理由:
- シンプル、信頼性高い
- 多くのデータ復旧ツールで採用
- 16 個は通常十分 (業務 PC で 16 ドライブ以上は稀)

### 管理者権限について

```
[必要な権限]
物理ドライブの open には Administrator 権限が必須

[ない場合の挙動]
ERROR_ACCESS_DENIED → AccessDenied エラー
workbench-dryrun list-drives --physical: 「権限不足」を表示
```

Chouさんへの説明: 必ず「管理者として実行」で cmd を起動する。

### 重要: 書き込み禁止の徹底

```rust
// CreateFileW のアクセスフラグ
GENERIC_READ  // ← これだけ
// GENERIC_WRITE は絶対に追加しない
```

業務的: お客様の HDD を絶対に壊さない。read-only アクセスが Phase 1.5 の根本原則。

### Windows のドライブ番号と論理ドライブの関係

```
[Windows の構造]
PhysicalDrive0 (物理): 物理 HDD 1 (例: 1TB)
  ├─ Partition 1: 512 MB EFI (見えない)
  ├─ Partition 2: 16 MB Microsoft Reserved (見えない)
  └─ Partition 3: 999 GB NTFS → C: (論理ドライブ)

PhysicalDrive1 (物理): 物理 HDD 2 (例: USB HDD)
  └─ Partition 1: 1.8 TB NTFS → E: (論理ドライブ)

PhysicalDrive2 (物理): 物理 HDD 3 (例: 壊れた USB HDD)
  └─ Partition 1: ??? (壊れた、Windows 認識せず)
  ※ ドライブレターなし、論理ドライブとしては見えない
  ※ 物理ドライブとしては見える ← Chunk 24d-1 で扱う対象
```

### 物理ドライブと論理ドライブの混同を避ける

```rust
// 論理ドライブ列挙 (既存):
list_logical_drives() → ["C:", "E:", ...] (Windows がマウントできるもの)

// 物理ドライブ列挙 (新規):
enumerate_physical_drives() → [PhysicalDrive0, 1, 2, ...] (物理 HDD)
```

異なる概念なので、API も分離する。

### Phase 2.1 UI への引き継ぎ

```
[Tauri UI で必要]
- 物理ドライブ一覧表示
- パーティション情報表示 (Chunk 24d-2 で実装)
- 「壊れた FS」の警告表示
- ドライブ選択 UI

[Chunk 24d-1 で公開する API]
enumerate_physical_drives() → UI から呼び出し可能
PhysicalDriveInfo → JSON シリアライズ可能 (Tauri command 経由)
```

### 拡張性

```
[本チャンクでは扱わない]
- 不良セクタ対応
- USB 切断検出
- 動的なドライブ列挙 (接続/切断イベント)

これらは Phase 2 で対応。Phase 1.5 では「実行時の物理ドライブ列挙」のみ。
```

---

## 質問が必要なケース

- windows-sys クレートの features が既存の Chunk 24a と衝突する場合
- IOCTL_STORAGE_QUERY_PROPERTY が想定外の挙動を示す場合 (古い Windows バージョン)
- 物理ドライブ番号の歯抜け (PhysicalDrive2 が存在するが、1 が存在しない等)

---

## 完了報告例

```markdown
## Chunk 24d-1 完了報告

### 新規ファイル
- crates/disk-io/src/physical.rs (約 500 行 + テスト 100 行)

### 修正ファイル
- crates/disk-io/src/lib.rs (+5 行 公開 API)
- crates/disk-io/Cargo.toml (+3 行 features)
- crates/workbench-dryrun/src/commands/list_drives.rs (+50 行 --physical)
- crates/workbench-dryrun/src/main.rs (+5 行)

### 新規 API
- enumerate_physical_drives() -> Vec<PhysicalDriveInfo>
- PhysicalDrive::open(path) -> Result<PhysicalDrive, PhysicalDriveError>
- PhysicalDrive::info() -> &PhysicalDriveInfo
- PhysicalDrive::read_at(offset, size) -> Result<Vec<u8>, PhysicalDriveError>

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行
  - recovery/src/timestamps.rs: 5-10 行 (Chunk 24a)
  - disk-io/src/physical.rs: 約 30 行 (Chunk 24d-1)

### テスト統計
- 単体: 既存 + 新規 7 件
- 統合: 1 件 (#[ignore]、ローカル検証用)
- 全 workspace: 全パス

### 動作確認サンプル (管理者として実行)
```
> workbench-dryrun list-drives --physical
物理ドライブ:
---------------------------------------------
検出された物理ドライブ: 2 個

[0] \\.\PhysicalDrive0
    サイズ:    931.5 GB
    Vendor:    SAMSUNG
    Product:   MZVL2512HCJQ-00B07
    Serial:    S64ANJ0RA00012
    Bus Type:  NVMe

[1] \\.\PhysicalDrive1
    サイズ:    1.8 TB
    Vendor:    Seagate
    Product:   ST2000DM006-2DM164
    Serial:    Z7E1234
    Bus Type:  USB (リムーバブル)

---------------------------------------------
注: パーティション情報は Chunk 24d-2 で追加予定
```

### 🎯 達成事項
- 物理ドライブを raw レベルで認識可能に
- 壊れた FS の HDD でも、物理ドライブとしては見える
- 業界標準 (R-STUDIO 等) と同等の認識能力の基盤完成

### 次のステップ
Chunk 24d-2 (パーティションテーブル解析) で:
- MBR / GPT パーサー
- 各パーティションの FS タイプ判定
- 「壊れた NTFS パーティション」の検出

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんが管理者として cmd で動作確認 (任意)
→ Chunk 24d-2 の指示書を私に依頼
```
