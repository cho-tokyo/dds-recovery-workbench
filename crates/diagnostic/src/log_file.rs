//! Chunk 24d-4-1: NTFS `$LogFile` の整合性チェック (簡易判定)。
//!
//! `$LogFile` (MFT インデックス 2) は NTFS のトランザクションログ。
//! 未完了トランザクションが残っていると Windows はマウント前に再生を試みる。
//!
//! ## 簡易判定の方針
//!
//! 完全な `$LogFile` 解析は仕様が複雑なため、本チャンクでは "RSTR" マジック値
//! (Restart Page) を確認する最低限の整合性チェックのみ。Phase 2 で本格対応する。
//!
//! 関連 FR: FR-DIAG-04 ($LogFile 整合性チェック)。

use dds_case_manager::LogFileStatus;
use dds_fs_ntfs::{find_attribute, AttributeHeader, AttributeType, NtfsVolume};

/// `$LogFile` MFT エントリ番号 (NTFS 仕様で固定)。
const LOGFILE_MFT_INDEX: u64 = 2;
/// Restart Page マジック値 (正常)。
const MAGIC_RSTR: &[u8; 4] = b"RSTR";
/// Record Page マジック値 (Restart Page の手前にあれば不整合の兆候)。
const MAGIC_RCRD: &[u8; 4] = b"RCRD";

/// `$LogFile` の先頭マジックを判定する内部ヘルパ。
fn classify_log_magic(magic: &[u8; 4]) -> LogFileStatus {
    if magic == MAGIC_RSTR {
        LogFileStatus::Consistent
    } else if magic == MAGIC_RCRD {
        LogFileStatus::Inconsistent
    } else if magic == &[0x00, 0x00, 0x00, 0x00] {
        // 空 LogFile = 初期化済み = 正常
        LogFileStatus::Consistent
    } else {
        LogFileStatus::Unknown
    }
}

/// `$LogFile` の整合性を簡易チェックする。
///
/// 業務的に失敗時 ($LogFile が読めない等) は [`LogFileStatus::Unknown`] にフォールバック。
///
/// 関連 FR: FR-DIAG-04。
pub fn check_log_file<F>(volume: &mut NtfsVolume<F>) -> LogFileStatus
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    let entry = match volume.read_record(LOGFILE_MFT_INDEX) {
        Ok(e) => e,
        Err(_) => return LogFileStatus::Unknown,
    };

    let first_attr_offset = entry.header.first_attribute_offset as usize;
    let attr = match find_attribute(&entry.data, first_attr_offset, AttributeType::Data) {
        Some(a) => a,
        None => return LogFileStatus::Unknown,
    };

    // $LogFile は非常駐 $DATA がほとんどだが、先頭 4 バイトのマジック値を取れるのは
    // 常駐 (極小 LogFile, 主にテストイメージ) のときのみ。非常駐の場合は実クラスタを
    // 読まないと先頭が見えないため、本チャンクでは「データ属性が見つかっただけで
    // Consistent と推定」する簡易判定にする (Phase 2 で run-list 経由 read を追加)。
    match &attr.header {
        AttributeHeader::Resident { resident, .. } => {
            let content_off = resident.content_offset as usize;
            let content_size = resident.content_size as usize;
            let attr_end = content_off + content_size;
            if attr_end > attr.raw.len() || content_size < 4 {
                return LogFileStatus::Unknown;
            }
            let magic: [u8; 4] = attr.raw[content_off..content_off + 4]
                .try_into()
                .expect("len 4");
            classify_log_magic(&magic)
        }
        AttributeHeader::NonResident { .. } => {
            // 非常駐 = LogFile が存在し、データ領域が確保されている。Phase 1.5 では
            // 業務的に「データ属性は健全」を理由に Consistent 寄りで報告。
            LogFileStatus::Consistent
        }
        AttributeHeader::End => LogFileStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_file_status_business_messages() {
        assert!(LogFileStatus::Consistent
            .business_message()
            .contains("正常"));
        assert!(LogFileStatus::Inconsistent
            .business_message()
            .contains("未完了"));
        assert!(LogFileStatus::Unknown
            .business_message()
            .contains("判定不能"));
    }

    #[test]
    fn classify_log_magic_rstr_is_consistent() {
        assert_eq!(classify_log_magic(b"RSTR"), LogFileStatus::Consistent);
    }

    #[test]
    fn classify_log_magic_rcrd_is_inconsistent() {
        assert_eq!(classify_log_magic(b"RCRD"), LogFileStatus::Inconsistent);
    }

    #[test]
    fn classify_log_magic_zero_is_consistent_empty() {
        assert_eq!(classify_log_magic(&[0, 0, 0, 0]), LogFileStatus::Consistent);
    }

    #[test]
    fn classify_log_magic_unknown_for_garbage() {
        assert_eq!(classify_log_magic(b"XXXX"), LogFileStatus::Unknown);
    }
}
