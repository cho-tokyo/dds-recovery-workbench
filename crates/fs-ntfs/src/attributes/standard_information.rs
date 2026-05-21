//! `$STANDARD_INFORMATION` 属性（タイプ 0x10）のコンテンツパーサ。タイムスタンプ（FILETIME）と
//! DOS ファイル属性、W2K+ 拡張部を取得する。関連 FR: FR-LIVE-01, FR-LIVE-06。
//! 仕様: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/standard_information.html>
use chrono::{DateTime, Utc};
use thiserror::Error;
const MIN_SIZE: usize = 48;
const FILETIME_EPOCH_DIFF_SECS: i64 = 11_644_473_600; // 1601→1970 の秒数
/// FILETIME（1601-01-01 UTC 起算 100ns 単位）。関連 FR: FR-LIVE-06。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct FileTime(pub u64);
impl FileTime {
    /// `chrono::DateTime<Utc>` に変換する。範囲外（i64 オーバーフロー等）は `None`。
    pub fn to_datetime(&self) -> Option<DateTime<Utc>> {
        let total = i64::try_from(self.0).ok()?;
        let secs = total
            .checked_div(10_000_000)?
            .checked_sub(FILETIME_EPOCH_DIFF_SECS)?;
        let nanos = ((total % 10_000_000) as u32).checked_mul(100)?;
        DateTime::from_timestamp(secs, nanos)
    }
}
/// DOS ファイル属性ビットフラグ。各 `is_*` で個別ビットを判定する。関連 FR: FR-LIVE-06。
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct FileAttributes(pub u32);
macro_rules! fa_bits { ($($name:ident = $val:expr => $fn:ident),* $(,)?) => {
    #[allow(missing_docs)] impl FileAttributes { $(pub const $name: u32 = $val;)*
        $(pub fn $fn(&self) -> bool { self.0 & Self::$name != 0 })* }
}; }
fa_bits!(READ_ONLY = 0x0001 => is_read_only, HIDDEN = 0x0002 => is_hidden,
    SYSTEM = 0x0004 => is_system, ARCHIVE = 0x0020 => is_archive,
    DEVICE = 0x0040 => is_device, NORMAL = 0x0080 => is_normal,
    TEMPORARY = 0x0100 => is_temporary, SPARSE_FILE = 0x0200 => is_sparse_file,
    REPARSE_POINT = 0x0400 => is_reparse_point,
    COMPRESSED = 0x0800 => is_compressed,
    OFFLINE = 0x1000 => is_offline,
    NOT_CONTENT_INDEXED = 0x2000 => is_not_content_indexed,
    ENCRYPTED = 0x4000 => is_encrypted,
    DIRECTORY = 0x1000_0000 => is_directory);
/// `$STANDARD_INFORMATION` 属性のコンテンツ。Option フィールドは W2K+ 拡張版でのみ Some。
/// 関連 FR: FR-LIVE-01, FR-LIVE-06。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct StandardInformation {
    pub created: FileTime,
    pub modified: FileTime,
    pub mft_modified: FileTime,
    pub accessed: FileTime,
    pub file_attributes: FileAttributes,
    pub max_versions: u32,
    pub version_number: u32,
    pub class_id: u32,
    pub owner_id: Option<u32>,
    pub security_id: Option<u32>,
    pub quota_charged: Option<u64>,
    pub usn: Option<u64>,
}
/// `parse_standard_information` が返すエラー型。
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum SiError {
    #[error("Buffer too small for $STANDARD_INFORMATION: got {got}, need at least 48")]
    BufferTooSmall { got: usize },
}
/// `$STANDARD_INFORMATION` のコンテンツ部分（ヘッダ除外）をパースする。
/// 48 バイト未満は `BufferTooSmall`、W2K+ 拡張は実バイト長で判別。関連 FR: FR-LIVE-01, FR-LIVE-06。
pub fn parse_standard_information(bytes: &[u8]) -> Result<StandardInformation, SiError> {
    if bytes.len() < MIN_SIZE {
        return Err(SiError::BufferTooSmall { got: bytes.len() });
    }
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    let u64le = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().expect("len 8"));
    Ok(StandardInformation {
        created: FileTime(u64le(0x00)),
        modified: FileTime(u64le(0x08)),
        mft_modified: FileTime(u64le(0x10)),
        accessed: FileTime(u64le(0x18)),
        file_attributes: FileAttributes(u32le(0x20)),
        max_versions: u32le(0x24),
        version_number: u32le(0x28),
        class_id: u32le(0x2C),
        owner_id: (bytes.len() >= 0x34).then(|| u32le(0x30)),
        security_id: (bytes.len() >= 0x38).then(|| u32le(0x34)),
        quota_charged: (bytes.len() >= 0x40).then(|| u64le(0x38)),
        usn: (bytes.len() >= 0x48).then(|| u64le(0x40)),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    const FT_2026: u64 = 134_116_992_000_000_000; // 2026-01-01 00:00:00 UTC
    fn build_si(ext: bool) -> Vec<u8> {
        let mut b = vec![0u8; if ext { 0x48 } else { MIN_SIZE }];
        for (i, off) in [0x00usize, 0x08, 0x10, 0x18].iter().enumerate() {
            b[*off..*off + 8].copy_from_slice(&(FT_2026 + i as u64).to_le_bytes());
        }
        b[0x20..0x24].copy_from_slice(&(0x0020u32 | 0x0002).to_le_bytes());
        if ext {
            b[0x30..0x34].copy_from_slice(&111u32.to_le_bytes());
            b[0x34..0x38].copy_from_slice(&222u32.to_le_bytes());
            b[0x38..0x40].copy_from_slice(&333u64.to_le_bytes());
            b[0x40..0x48].copy_from_slice(&444u64.to_le_bytes());
        }
        b
    }
    #[test]
    fn parses_48_byte_nt_version() {
        let si = parse_standard_information(&build_si(false)).expect("parse");
        assert_eq!(si.created.0, FT_2026);
        assert!(si.file_attributes.is_archive() && si.file_attributes.is_hidden());
        assert!(
            si.owner_id.is_none()
                && si.security_id.is_none()
                && si.quota_charged.is_none()
                && si.usn.is_none()
        );
    }
    #[test]
    fn parses_72_byte_w2k_extended() {
        let si = parse_standard_information(&build_si(true)).expect("parse");
        assert_eq!(
            (si.owner_id, si.security_id, si.quota_charged, si.usn),
            (Some(111), Some(222), Some(333), Some(444))
        );
    }
    #[test]
    fn rejects_buffer_smaller_than_48_bytes() {
        assert_eq!(
            parse_standard_information(&[0u8; 47]).unwrap_err(),
            SiError::BufferTooSmall { got: 47 }
        );
    }
    #[test]
    fn filetime_to_datetime_known_value() {
        let dt = FileTime(FT_2026).to_datetime().expect("datetime");
        assert_eq!(dt.to_rfc3339(), "2026-01-01T00:00:00+00:00");
        assert!(FileTime(0).to_datetime().is_some());
    }
    #[test]
    fn file_attributes_bit_checks() {
        let a = FileAttributes(
            FileAttributes::READ_ONLY
                | FileAttributes::HIDDEN
                | FileAttributes::SYSTEM
                | FileAttributes::ARCHIVE
                | FileAttributes::COMPRESSED
                | FileAttributes::ENCRYPTED
                | FileAttributes::DIRECTORY,
        );
        assert!(
            a.is_read_only()
                && a.is_hidden()
                && a.is_system()
                && a.is_archive()
                && a.is_compressed()
                && a.is_encrypted()
                && a.is_directory()
        );
        assert!(!FileAttributes(0).is_read_only() && !FileAttributes(0).is_directory());
    }
    /// 書籍 Table 13.6 に列挙された追加 7 ビットが個別に判定できることを確認する。
    #[test]
    fn extended_file_attribute_bits_book_table_13_6() {
        type Predicate = fn(&FileAttributes) -> bool;
        let pairs: [(u32, Predicate); 7] = [
            (FileAttributes::DEVICE, FileAttributes::is_device),
            (FileAttributes::NORMAL, FileAttributes::is_normal),
            (FileAttributes::TEMPORARY, FileAttributes::is_temporary),
            (FileAttributes::SPARSE_FILE, FileAttributes::is_sparse_file),
            (
                FileAttributes::REPARSE_POINT,
                FileAttributes::is_reparse_point,
            ),
            (FileAttributes::OFFLINE, FileAttributes::is_offline),
            (
                FileAttributes::NOT_CONTENT_INDEXED,
                FileAttributes::is_not_content_indexed,
            ),
        ];
        for (bit, check) in pairs {
            assert!(
                check(&FileAttributes(bit)),
                "bit {bit:#06x} should set its predicate"
            );
            assert!(
                !check(&FileAttributes(!bit)),
                "negated mask {bit:#06x} should clear pred"
            );
        }
        assert_eq!(FileAttributes::DEVICE, 0x0040);
        assert_eq!(FileAttributes::NOT_CONTENT_INDEXED, 0x2000);
    }
    /// 書籍 361 ページの $MFT 自身の $STANDARD_INFORMATION を再現する。
    /// 4 タイムスタンプは同一、flags=0x06（HIDDEN+SYSTEM）、security_id=1。
    #[test]
    fn book_example_mft_standard_information() {
        let mut b = vec![0u8; 0x48];
        for off in [0x00usize, 0x08, 0x10, 0x18] {
            b[off..off + 8].copy_from_slice(&FT_2026.to_le_bytes());
        }
        b[0x20..0x24].copy_from_slice(&0x0000_0006u32.to_le_bytes());
        // max_versions=0, version_number=0, class_id=0 はゼロ初期化のまま。
        // owner_id=0 もゼロ初期化のまま。security_id=1 のみ書き込む。
        b[0x34..0x38].copy_from_slice(&1u32.to_le_bytes());
        // quota_charged=0, usn=0 もゼロ初期化のまま。
        let si = parse_standard_information(&b).expect("parse $MFT example");
        assert!(si.file_attributes.is_hidden() && si.file_attributes.is_system());
        assert!(!si.file_attributes.is_read_only() && !si.file_attributes.is_archive());
        assert_eq!(si.file_attributes.0, 0x0000_0006);
        assert_eq!(si.created.0, si.modified.0);
        assert_eq!(si.modified.0, si.mft_modified.0);
        assert_eq!(si.mft_modified.0, si.accessed.0);
        assert_eq!(si.max_versions, 0);
        assert_eq!(si.version_number, 0);
        assert_eq!(si.class_id, 0);
        assert_eq!(si.owner_id, Some(0));
        assert_eq!(si.security_id, Some(1));
        assert_eq!(si.quota_charged, Some(0));
        assert_eq!(si.usn, Some(0));
    }
    /// 極端な FILETIME（u64::MAX）でもパニックせず None を返すことを確認する。
    #[test]
    fn filetime_overflow_safely_returns_none() {
        assert!(FileTime(u64::MAX).to_datetime().is_none());
        // i64::MAX の手前境界（i64::try_from は通る）。秒換算後の sub で None になり得る。
        let near_i64_max = i64::MAX as u64;
        let _ = FileTime(near_i64_max).to_datetime(); // パニックしないこと自体が成功
    }
}
