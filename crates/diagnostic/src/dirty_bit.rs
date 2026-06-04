//! Chunk 24d-4-1: NTFS の Dirty Bit 検出。
//!
//! NTFS は `$Volume` MFT エントリ (インデックス 3) の `$VOLUME_INFORMATION`
//! 属性 (タイプ 0x70) に "dirty" フラグを持つ。これが立っていると Windows は
//! マウントを拒否し、chkdsk を要求する。
//!
//! ## 業務的意義
//!
//! Dirty Bit が立っている = Windows が「壊れている」と判断する原因の最多。
//! しかし NTFS 構造自体は健全なケースが多く、データ復旧の絶好の対象。
//! 営業はお客様に「Windows がマウントを拒否している理由が判明しました」と説明できる。
//!
//! 関連 FR: FR-DIAG-04 (Dirty Bit 検出)。

use dds_case_manager::DirtyBitStatus;
use dds_fs_ntfs::{find_attribute, AttributeHeader, AttributeType, NtfsVolume};

/// `$Volume` MFT エントリは NTFS 仕様で固定。
const VOLUME_MFT_INDEX: u64 = 3;
/// `$VOLUME_INFORMATION` 属性内の Flags フィールドのオフセット。
const VOLUME_INFO_FLAGS_OFFSET: usize = 8;
/// VOLUME_IS_DIRTY フラグビット。
const VOLUME_IS_DIRTY: u16 = 0x0001;

/// `$Volume` MFT エントリを読んで Dirty Bit を確認する。
///
/// 業務的な失敗ケース ($Volume が読めない、属性が見つからない等) は
/// [`DirtyBitStatus::Unknown`] にフォールバックし、診断全体は止めない。
///
/// 関連 FR: FR-DIAG-04。
pub fn check_dirty_bit<F>(volume: &mut NtfsVolume<F>) -> DirtyBitStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let entry = match volume.read_record(VOLUME_MFT_INDEX) {
        Ok(e) => e,
        Err(_) => return DirtyBitStatus::Unknown,
    };

    let first_attr_offset = entry.header.first_attribute_offset as usize;
    let attr = match find_attribute(
        &entry.data,
        first_attr_offset,
        AttributeType::VolumeInformation,
    ) {
        Some(a) => a,
        None => return DirtyBitStatus::Unknown,
    };

    // $VOLUME_INFORMATION は常駐属性。content_offset / content_size でデータ本体を取り出す。
    let resident = match &attr.header {
        AttributeHeader::Resident { resident, .. } => resident,
        _ => return DirtyBitStatus::Unknown,
    };

    let content_off = resident.content_offset as usize;
    let content_size = resident.content_size as usize;
    let attr_end = content_off + content_size;
    if attr_end > attr.raw.len() {
        return DirtyBitStatus::Unknown;
    }
    let content = &attr.raw[content_off..attr_end];

    // $VOLUME_INFORMATION 内構造:
    //   offset 0-7:  Reserved
    //   offset 8-9:  Major / Minor Version (u8 x 2)
    //   offset 10-11: Flags (u16, ★ Dirty Bit はここ)
    //   offset 12-15: Reserved
    // 仕様: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/volume_information.html>
    let flags_offset = VOLUME_INFO_FLAGS_OFFSET + 2; // version 2 バイト後
    if content.len() < flags_offset + 2 {
        return DirtyBitStatus::Unknown;
    }
    let flags = u16::from_le_bytes([content[flags_offset], content[flags_offset + 1]]);

    if flags & VOLUME_IS_DIRTY != 0 {
        DirtyBitStatus::Dirty
    } else {
        DirtyBitStatus::Clean
    }
}

/// Flags ワードから Dirty Bit を判定する純関数 (unit test 用)。
///
/// 関連 FR: FR-DIAG-04。
pub fn is_dirty_from_flags(flags: u16) -> bool {
    flags & VOLUME_IS_DIRTY != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_bit_status_business_messages() {
        assert!(DirtyBitStatus::Clean.business_message().contains("正常"));
        assert!(DirtyBitStatus::Dirty
            .business_message()
            .contains("マウント拒否"));
        assert!(DirtyBitStatus::Unknown
            .business_message()
            .contains("判定不能"));
    }

    #[test]
    fn is_dirty_from_flags_detects_dirty_bit() {
        assert!(is_dirty_from_flags(0x0001));
        assert!(is_dirty_from_flags(0x0003)); // VOLUME_IS_DIRTY + 他フラグ
    }

    #[test]
    fn is_dirty_from_flags_clean_when_zero() {
        assert!(!is_dirty_from_flags(0x0000));
        assert!(!is_dirty_from_flags(0x0002)); // 他フラグのみ
        assert!(!is_dirty_from_flags(0x0004));
    }
}
