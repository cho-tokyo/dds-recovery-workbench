//! ファイルシステムタイプの検出 (Chunk 24d-2)。
//!
//! パーティションの先頭セクタ (ブートセクタ) を読み、
//! シグネチャから FS タイプを判定する。
//!
//! ## サポート FS
//! - NTFS (offset 3-10 に `"NTFS    "`)
//! - FAT32 (offset 82-89 に `"FAT32   "`)
//! - exFAT (offset 3-10 に `"EXFAT   "`)
//! - その他は [`FsType::Unknown`]
//!
//! ## 注意
//!
//! 本モジュールはシグネチャベースの**簡易判定**のみを行う。
//! FS の健全性（壊れているか否か）の判定は Chunk 24d-3 で
//! 実際に `NtfsVolume` を open することで行う。
//!
//! 関連 FR: FR-PHY-05 (FS タイプ判定)

use crate::physical::{PhysicalDrive, PhysicalDriveError};

/// ブートセクタとして読み取るバイト数。
const BOOT_SECTOR_SIZE: usize = 512;

/// FAT32 シグネチャ判定に必要な最小バイト数。
const MIN_FAT32_BOOT_LEN: usize = 90;

/// 検出可能なファイルシステムタイプ。
///
/// Phase 1.5 では NTFS / FAT32 / exFAT / Unknown をサポート。
/// ext4 / HFS+ などは Phase 2 以降で対応予定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsType {
    /// NTFS (Microsoft Windows New Technology File System)
    Ntfs,
    /// FAT32 (古典 FAT、4 GiB 上限)
    Fat32,
    /// exFAT (FAT の拡張、リムーバブル向け)
    ExFat,
    /// 未知の FS、もしくは未フォーマット領域
    Unknown,
}

impl FsType {
    /// CLI / レポート用の表示名を返す。
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ntfs => "NTFS",
            Self::Fat32 => "FAT32",
            Self::ExFat => "exFAT",
            Self::Unknown => "Unknown",
        }
    }

    /// この FS が Workbench で復旧可能か。
    ///
    /// Phase 1.5 では **NTFS のみ** が復旧対象。
    /// FAT32 / exFAT は検出のみ可能で、実際の復旧パイプラインは未実装。
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Ntfs)
    }
}

/// 指定オフセットのブートセクタを読み、FS タイプを判定する。
///
/// # 引数
/// - `drive`: 読み取り対象の物理ドライブ (read-only)
/// - `partition_offset`: パーティション先頭のバイトオフセット
///
/// # 戻り値
/// 読み取り失敗時のみ [`PhysicalDriveError`] を返す。
/// ブートセクタが 512 バイトに満たないなど、シグネチャ判定不能な場合は
/// [`FsType::Unknown`] を返す (エラーにはしない)。
pub fn detect_fs_type(
    drive: &PhysicalDrive,
    partition_offset: u64,
) -> Result<FsType, PhysicalDriveError> {
    let boot_sector = drive.read_at(partition_offset, BOOT_SECTOR_SIZE)?;

    if boot_sector.len() < BOOT_SECTOR_SIZE {
        return Ok(FsType::Unknown);
    }

    Ok(detect_from_boot_sector(&boot_sector))
}

/// ブートセクタのバイト列から FS タイプを判定する純粋関数。
///
/// 物理 I/O を伴わないため、テストや既存のセクタダンプ解析にそのまま使える。
///
/// # 判定ロジック
/// - `len < 90` → `Unknown` (判定不能)
/// - offset 3-10 が `b"NTFS    "` → `Ntfs`
/// - offset 3-10 が `b"EXFAT   "` → `ExFat`
/// - offset 82-89 が `b"FAT32   "` → `Fat32`
/// - 上記いずれにも一致しない → `Unknown`
pub fn detect_from_boot_sector(boot_sector: &[u8]) -> FsType {
    if boot_sector.len() < MIN_FAT32_BOOT_LEN {
        return FsType::Unknown;
    }

    // NTFS: offset 3-10 に "NTFS    " (4E 54 46 53 20 20 20 20)
    if &boot_sector[3..11] == b"NTFS    " {
        return FsType::Ntfs;
    }

    // exFAT: offset 3-10 に "EXFAT   " (45 58 46 41 54 20 20 20)
    if &boot_sector[3..11] == b"EXFAT   " {
        return FsType::ExFat;
    }

    // FAT32: offset 82-89 に "FAT32   " (46 41 54 33 32 20 20 20)
    if &boot_sector[82..90] == b"FAT32   " {
        return FsType::Fat32;
    }

    FsType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_ntfs_signature() {
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"NTFS    ");
        assert_eq!(detect_from_boot_sector(&boot), FsType::Ntfs);
    }

    #[test]
    fn detect_fat32_signature() {
        let mut boot = vec![0u8; 512];
        boot[82..90].copy_from_slice(b"FAT32   ");
        assert_eq!(detect_from_boot_sector(&boot), FsType::Fat32);
    }

    #[test]
    fn detect_exfat_signature() {
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"EXFAT   ");
        assert_eq!(detect_from_boot_sector(&boot), FsType::ExFat);
    }

    #[test]
    fn detect_unknown_for_random_bytes() {
        let boot = vec![0xFFu8; 512];
        assert_eq!(detect_from_boot_sector(&boot), FsType::Unknown);
    }

    #[test]
    fn detect_unknown_for_short_buffer() {
        // 90 バイト未満 → 判定不能で Unknown
        let boot = vec![0u8; 50];
        assert_eq!(detect_from_boot_sector(&boot), FsType::Unknown);
    }

    #[test]
    fn fs_type_display_names() {
        assert_eq!(FsType::Ntfs.display_name(), "NTFS");
        assert_eq!(FsType::Fat32.display_name(), "FAT32");
        assert_eq!(FsType::ExFat.display_name(), "exFAT");
        assert_eq!(FsType::Unknown.display_name(), "Unknown");
    }

    #[test]
    fn fs_type_recoverable() {
        assert!(FsType::Ntfs.is_recoverable());
        // Phase 1.5 では NTFS のみ対応
        assert!(!FsType::Fat32.is_recoverable());
        assert!(!FsType::ExFat.is_recoverable());
        assert!(!FsType::Unknown.is_recoverable());
    }

    #[test]
    fn detect_ntfs_takes_priority_over_garbage_at_fat32_offset() {
        // NTFS シグネチャが先に検出されることを確認
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"NTFS    ");
        // FAT32 オフセットにも誤って文字列があった場合でも NTFS が勝つ
        boot[82..90].copy_from_slice(b"FAT32   ");
        assert_eq!(detect_from_boot_sector(&boot), FsType::Ntfs);
    }
}
