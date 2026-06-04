//! Chunk 24d-4-1: BitLocker 暗号化の検出 (簡易判定)。
//!
//! BitLocker To Go (Windows 7+) で暗号化されたボリュームは、ブートセクタ
//! オフセット 3-10 に "-FVE-FS-" シグネチャを持つ。これを検出して業務的に
//! 「回復キーが必要」と報告する。
//!
//! ## 業務的原則
//!
//! 「BitLocker 暗号化を検出」は決して「受注不可」を意味しない。回復キーがあれば
//! 復旧可能であり、受注可否は人間 (営業) が判断する。
//!
//! ## 設計判断
//!
//! `NtfsVolume` が open できている時点で、ブートセクタは NTFS シグネチャ
//! ("NTFS    ") を持つことが保証される。BitLocker for Windows との
//! 並走シナリオ等は Phase 1.5 スコープ外なので、open 成功時は基本的に
//! [`BitLockerStatus::NotEncrypted`] を返す。業務的により厳密な判定が
//! 必要なら raw bytes を `bytes_contain_bitlocker_signature` に渡す。
//!
//! 関連 FR: FR-DIAG-05 (BitLocker 検出)。

use dds_case_manager::BitLockerStatus;
use dds_fs_ntfs::NtfsVolume;

/// BitLocker To Go のシグネチャ (ブートセクタ offset 3-11)。
const BITLOCKER_SIGNATURE: &[u8; 8] = b"-FVE-FS-";

/// 指定されたブートセクタバイト列に BitLocker シグネチャが含まれるかを判定する。
///
/// 業務的に「ボリューム先頭 512 バイトの offset 3〜11」に `-FVE-FS-` が
/// 存在する場合のみ BitLocker と判定する。
///
/// 関連 FR: FR-DIAG-05。
pub fn bytes_contain_bitlocker_signature(boot_sector: &[u8]) -> bool {
    boot_sector.len() >= 11 && &boot_sector[3..11] == BITLOCKER_SIGNATURE
}

/// `NtfsVolume` から BitLocker 暗号化の有無を判定する。
///
/// `NtfsVolume::open` が成功している = NTFS シグネチャ確認済みなので、
/// 通常は [`BitLockerStatus::NotEncrypted`] を返す。BitLocker 検出は
/// NtfsVolume::open が失敗した直後に raw bytes を
/// [`bytes_contain_bitlocker_signature`] に渡す呼び出し側で行う想定。
///
/// 関連 FR: FR-DIAG-05。
pub fn check_bitlocker<F>(_volume: &mut NtfsVolume<F>) -> BitLockerStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    // NtfsVolume が open 済み = NTFS OEM ID 確認済み = BitLocker To Go ではない。
    BitLockerStatus::NotEncrypted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitlocker_status_business_messages() {
        assert!(BitLockerStatus::NotEncrypted
            .business_message()
            .contains("なし"));
        assert!(BitLockerStatus::Encrypted
            .business_message()
            .contains("回復キー"));
        assert!(BitLockerStatus::Unknown
            .business_message()
            .contains("判定不能"));
    }

    #[test]
    fn bitlocker_signature_detection_from_bytes() {
        // モックの BitLocker ブートセクタを作成 ("-FVE-FS-")
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"-FVE-FS-");
        assert!(bytes_contain_bitlocker_signature(&boot));
    }

    #[test]
    fn bitlocker_signature_absent_in_ntfs_boot() {
        // NTFS ブートセクタは offset 3 に "NTFS    "
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"NTFS    ");
        assert!(!bytes_contain_bitlocker_signature(&boot));
    }

    #[test]
    fn bitlocker_signature_short_buffer_returns_false() {
        let short = vec![0u8; 8];
        assert!(!bytes_contain_bitlocker_signature(&short));
    }
}
