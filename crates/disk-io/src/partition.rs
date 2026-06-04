//! パーティションテーブル (MBR / GPT) の解析 (Chunk 24d-2)。
//!
//! 物理ドライブの先頭セクタを読み、パーティションテーブルを解析する。
//! MBR と GPT の両方をサポート。
//!
//! ## 解析の流れ
//!
//! 1. 先頭 512 バイト (LBA 0) を読む
//! 2. MBR シグネチャ (`0x55AA`) を確認
//! 3. MBR パーティションテーブルから最大 4 つのパーティションを取得
//! 4. パーティションタイプが `0xEE` (Protective MBR) ならば GPT を解析:
//!    - LBA 1 から GPT ヘッダを読む
//!    - パーティションエントリ配列を読む
//!    - 各エントリからパーティション情報を抽出
//!
//! ## 安全性
//!
//! バイト列の解釈のみを行うため `unsafe` ブロックは一切不要。
//! 物理ドライブへの書き込みも行わない (read-only 経由のみ)。
//!
//! 関連 FR: FR-PHY-04 (パーティションテーブル解析) / FR-PHY-05 (FS タイプ判定)

use thiserror::Error;

use crate::fs_detection::{detect_fs_type, FsType};
use crate::physical::{PhysicalDrive, PhysicalDriveError};

/// 1 セクタのバイト数 (LBA → バイトオフセット変換用)。
const SECTOR_SIZE: u64 = 512;

/// MBR のセクタ末尾 2 バイトに格納されるシグネチャ (LE で読むと `0xAA55`)。
const MBR_SIGNATURE: u16 = 0xAA55;

/// MBR パーティションテーブルの開始オフセット。
const MBR_PARTITION_TABLE_OFFSET: usize = 446;

/// MBR パーティションエントリのサイズ。
const MBR_ENTRY_SIZE: usize = 16;

/// MBR パーティションエントリの数。
const MBR_ENTRY_COUNT: usize = 4;

/// "Protective MBR" を示すパーティションタイプバイト。GPT 判定に使う。
const GPT_PROTECTIVE_MBR_TYPE: u8 = 0xEE;

/// GPT ヘッダの先頭 8 バイトに格納されるシグネチャ。
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

/// GPT パーティションエントリのサイズ (UEFI 仕様の最小値)。
const GPT_ENTRY_MIN_SIZE: u32 = 128;

/// GPT パーティションエントリのサイズの上限。
/// UEFI 仕様上 `entry_size` は 128 の倍数で、現実的には 4096 を超えない。
const GPT_ENTRY_MAX_SIZE: u32 = 4096;

/// GPT パーティションエントリ数の上限 (UEFI 仕様での慣習値)。
const GPT_ENTRY_MAX_COUNT: u32 = 128;

/// パーティション解析エラー。
#[derive(Debug, Error)]
pub enum PartitionError {
    /// 物理ドライブの読み取りに失敗。
    #[error("物理ドライブの読み取りに失敗: {0}")]
    Read(#[from] PhysicalDriveError),

    /// MBR のシグネチャ (`0x55AA`) が一致しない。
    #[error("MBR シグネチャが無効: 期待 0xAA55、実際 0x{0:04X}")]
    InvalidMbrSignature(u16),

    /// GPT ヘッダの先頭が `EFI PART` でない。
    #[error("GPT ヘッダのシグネチャが無効")]
    InvalidGptSignature,

    /// GPT ヘッダの CRC が一致しない (将来の検証用)。
    #[error("GPT ヘッダの CRC が無効")]
    InvalidGptCrc,

    /// パーティションテーブルの値が範囲外など破損が疑われる。
    #[error("パーティションテーブルが破損している可能性: {0}")]
    Corrupted(String),

    /// 読み取ったバイト列が必要量に満たない。
    #[error("バイト列の長さが不足: 必要 {required}、実際 {actual}")]
    InsufficientData {
        /// 必要バイト数
        required: usize,
        /// 実際に読めたバイト数
        actual: usize,
    },
}

/// パーティション情報。
#[derive(Debug, Clone)]
pub struct Partition {
    /// パーティション番号 (1 ベース、Windows 風)。
    pub number: u32,

    /// パーティション開始位置 (バイト)。
    pub start_offset: u64,

    /// パーティションサイズ (バイト)。
    pub size: u64,

    /// パーティションタイプ (MBR の場合) または UUID (GPT の場合)。
    pub partition_type: PartitionType,

    /// 検出された FS タイプ (シグネチャベース)。
    pub fs_type: FsType,
}

/// パーティションタイプ。
#[derive(Debug, Clone)]
pub enum PartitionType {
    /// MBR パーティションタイプ ID
    /// (例: `0x07` = NTFS/HPFS, `0x0B`/`0x0C` = FAT32, `0x83` = Linux)
    MbrType(u8),

    /// GPT パーティションタイプ UUID
    /// (例: `EBD0A0A2-B9E5-4433-87C0-68B6B72699C7` = Microsoft Basic Data, NTFS含む)
    GptType(uuid::Uuid),
}

impl PartitionType {
    /// CLI / レポート用の表示名を返す。
    pub fn display_name(&self) -> String {
        match self {
            Self::MbrType(0x00) => "Empty".to_string(),
            Self::MbrType(0x07) => "NTFS/exFAT/HPFS".to_string(),
            Self::MbrType(0x0B) => "FAT32 (CHS)".to_string(),
            Self::MbrType(0x0C) => "FAT32 (LBA)".to_string(),
            Self::MbrType(0x83) => "Linux".to_string(),
            Self::MbrType(0xEE) => "GPT Protective".to_string(),
            Self::MbrType(0xEF) => "EFI System".to_string(),
            Self::MbrType(byte) => format!("MBR Type 0x{:02X}", byte),
            Self::GptType(uuid) => gpt_type_name(uuid).to_string(),
        }
    }
}

/// GPT パーティションタイプ UUID から既知の名前を取得する。
///
/// 既知でない場合は `"Unknown"` を返す。
fn gpt_type_name(uuid: &uuid::Uuid) -> &'static str {
    match uuid.to_string().to_uppercase().as_str() {
        "00000000-0000-0000-0000-000000000000" => "Unused",
        "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => "EFI System",
        "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => "Microsoft Reserved",
        "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => "Microsoft Basic Data",
        "DE94BBA4-06D1-4D40-A16A-BFD50179D6AC" => "Windows Recovery",
        "0FC63DAF-8483-4772-8E79-3D69D8477DE4" => "Linux Filesystem",
        "21686148-6449-6E6F-744E-656564454649" => "BIOS Boot",
        _ => "Unknown",
    }
}

/// ドライブのパーティション一覧を解析する。
///
/// MBR の最初のパーティションタイプが `0xEE` の場合は自動的に GPT として再解析する。
///
/// 関連 FR: FR-PHY-04
pub fn read_partitions(drive: &PhysicalDrive) -> Result<Vec<Partition>, PartitionError> {
    // 先頭セクタ (MBR) を読む
    let mbr_bytes = drive.read_at(0, 512)?;

    if mbr_bytes.len() < 512 {
        return Err(PartitionError::InsufficientData {
            required: 512,
            actual: mbr_bytes.len(),
        });
    }

    // MBR シグネチャ (offset 510, LE で読むと 0xAA55)
    let signature = u16::from_le_bytes([mbr_bytes[510], mbr_bytes[511]]);
    if signature != MBR_SIGNATURE {
        return Err(PartitionError::InvalidMbrSignature(signature));
    }

    // MBR パーティションテーブル
    let mbr_partitions = parse_mbr_partitions(&mbr_bytes)?;

    // GPT 判定 (どこかのパーティションタイプが 0xEE)
    let is_gpt = mbr_partitions.iter().any(|p| {
        matches!(
            p.partition_type,
            PartitionType::MbrType(GPT_PROTECTIVE_MBR_TYPE)
        )
    });

    if is_gpt {
        parse_gpt(drive)
    } else {
        // MBR のまま FS 判定して返す
        let mut result = Vec::new();
        for partition in mbr_partitions.into_iter() {
            if partition.size == 0 {
                continue;
            }
            let fs_type = detect_fs_type(drive, partition.start_offset).unwrap_or(FsType::Unknown);
            result.push(Partition {
                fs_type,
                ..partition
            });
        }
        Ok(result)
    }
}

/// MBR パーティションテーブルを解析する (FS 判定は行わない)。
///
/// 戻り値の `fs_type` は常に [`FsType::Unknown`]。FS 判定は呼び出し側で
/// [`detect_fs_type`] を使って別途行う。
fn parse_mbr_partitions(mbr: &[u8]) -> Result<Vec<Partition>, PartitionError> {
    if mbr.len() < 512 {
        return Err(PartitionError::InsufficientData {
            required: 512,
            actual: mbr.len(),
        });
    }

    let mut partitions = Vec::new();

    for i in 0..MBR_ENTRY_COUNT {
        let entry_offset = MBR_PARTITION_TABLE_OFFSET + i * MBR_ENTRY_SIZE;
        let entry = &mbr[entry_offset..entry_offset + MBR_ENTRY_SIZE];

        let partition_type_byte = entry[4];
        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
        let num_sectors = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);

        // 空エントリは無視
        if partition_type_byte == 0x00 || num_sectors == 0 {
            continue;
        }

        partitions.push(Partition {
            number: (i as u32) + 1, // 1 ベース
            start_offset: (start_lba as u64) * SECTOR_SIZE,
            size: (num_sectors as u64) * SECTOR_SIZE,
            partition_type: PartitionType::MbrType(partition_type_byte),
            fs_type: FsType::Unknown,
        });
    }

    Ok(partitions)
}

/// GPT パーティションテーブルを解析する。
fn parse_gpt(drive: &PhysicalDrive) -> Result<Vec<Partition>, PartitionError> {
    // GPT ヘッダは LBA 1 (オフセット 512 バイト)
    let gpt_header_bytes = drive.read_at(SECTOR_SIZE, 512)?;

    if gpt_header_bytes.len() < 92 {
        return Err(PartitionError::InsufficientData {
            required: 92,
            actual: gpt_header_bytes.len(),
        });
    }

    // シグネチャ確認 ("EFI PART")
    if &gpt_header_bytes[0..8] != GPT_SIGNATURE {
        return Err(PartitionError::InvalidGptSignature);
    }

    // パーティションエントリ配列の位置 (LBA)
    let partition_entries_lba = u64::from_le_bytes([
        gpt_header_bytes[72],
        gpt_header_bytes[73],
        gpt_header_bytes[74],
        gpt_header_bytes[75],
        gpt_header_bytes[76],
        gpt_header_bytes[77],
        gpt_header_bytes[78],
        gpt_header_bytes[79],
    ]);
    let num_entries = u32::from_le_bytes([
        gpt_header_bytes[80],
        gpt_header_bytes[81],
        gpt_header_bytes[82],
        gpt_header_bytes[83],
    ]);
    let entry_size = u32::from_le_bytes([
        gpt_header_bytes[84],
        gpt_header_bytes[85],
        gpt_header_bytes[86],
        gpt_header_bytes[87],
    ]);

    // 妥当性チェック
    if !(GPT_ENTRY_MIN_SIZE..=GPT_ENTRY_MAX_SIZE).contains(&entry_size) {
        return Err(PartitionError::Corrupted(format!(
            "GPT エントリサイズが異常: {}",
            entry_size
        )));
    }
    if num_entries > GPT_ENTRY_MAX_COUNT {
        return Err(PartitionError::Corrupted(format!(
            "GPT エントリ数が異常: {}",
            num_entries
        )));
    }

    let total_size = (num_entries as u64) * (entry_size as u64);
    let entries_offset = partition_entries_lba * SECTOR_SIZE;

    let entries_bytes = drive.read_at(entries_offset, total_size as usize)?;

    let mut partitions = Vec::new();
    let mut partition_number: u32 = 1;

    for i in 0..(num_entries as usize) {
        let entry_offset = i * (entry_size as usize);
        if entry_offset + 128 > entries_bytes.len() {
            break;
        }
        let entry = &entries_bytes[entry_offset..entry_offset + 128];

        // 型 UUID (offset 0-15)
        let mut type_uuid_bytes = [0u8; 16];
        type_uuid_bytes.copy_from_slice(&entry[0..16]);

        // 未使用エントリ (UUID が all-zero) はスキップ
        if type_uuid_bytes.iter().all(|&b| b == 0) {
            continue;
        }

        let type_uuid = uuid_from_le_bytes(&type_uuid_bytes);

        // 開始 LBA、終了 LBA
        let start_lba = u64::from_le_bytes([
            entry[32], entry[33], entry[34], entry[35], entry[36], entry[37], entry[38], entry[39],
        ]);
        let end_lba = u64::from_le_bytes([
            entry[40], entry[41], entry[42], entry[43], entry[44], entry[45], entry[46], entry[47],
        ]);

        if end_lba < start_lba {
            continue;
        }

        let start_offset = start_lba * SECTOR_SIZE;
        let size = (end_lba - start_lba + 1) * SECTOR_SIZE;

        let fs_type = detect_fs_type(drive, start_offset).unwrap_or(FsType::Unknown);

        partitions.push(Partition {
            number: partition_number,
            start_offset,
            size,
            partition_type: PartitionType::GptType(type_uuid),
            fs_type,
        });

        partition_number += 1;
    }

    Ok(partitions)
}

/// GPT のバイト列形式の UUID を [`uuid::Uuid`] に変換する。
///
/// GPT の UUID は最初の 3 フィールド (4 + 2 + 2 バイト) が
/// **リトルエンディアン**で、残りの 8 バイトは **ビッグエンディアン**のまま。
///
/// 例: `C12A7328-F81F-11D2-BA4B-00A0C93EC93B` のバイト列:
/// `28 73 2A C1 | 1F F8 | D2 11 | BA 4B | 00 A0 C9 3E C9 3B`
fn uuid_from_le_bytes(bytes: &[u8; 16]) -> uuid::Uuid {
    let d1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let d2 = u16::from_le_bytes([bytes[4], bytes[5]]);
    let d3 = u16::from_le_bytes([bytes[6], bytes[7]]);

    let mut be_bytes = [0u8; 16];
    be_bytes[0..4].copy_from_slice(&d1.to_be_bytes());
    be_bytes[4..6].copy_from_slice(&d2.to_be_bytes());
    be_bytes[6..8].copy_from_slice(&d3.to_be_bytes());
    be_bytes[8..16].copy_from_slice(&bytes[8..16]);

    uuid::Uuid::from_bytes(be_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbr_partition_parse_valid() {
        // 最小限の MBR (空のテーブル + シグネチャ)
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;

        // パーティション 1: タイプ 0x07 (NTFS)、LBA 2048、サイズ 100MB
        let entry_offset = MBR_PARTITION_TABLE_OFFSET;
        mbr[entry_offset + 4] = 0x07;
        mbr[entry_offset + 8..entry_offset + 12].copy_from_slice(&2048u32.to_le_bytes());
        let sectors_100mb: u32 = 100 * 1024 * 1024 / 512;
        mbr[entry_offset + 12..entry_offset + 16].copy_from_slice(&sectors_100mb.to_le_bytes());

        let partitions = parse_mbr_partitions(&mbr).unwrap();
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].number, 1);
        assert_eq!(partitions[0].start_offset, 2048 * 512);
        assert_eq!(partitions[0].size, 100 * 1024 * 1024);
        assert!(matches!(
            partitions[0].partition_type,
            PartitionType::MbrType(0x07)
        ));
    }

    #[test]
    fn mbr_partition_parse_empty_entries_skipped() {
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        // 全エントリ空 (タイプ 0、サイズ 0)
        let partitions = parse_mbr_partitions(&mbr).unwrap();
        assert!(partitions.is_empty());
    }

    #[test]
    fn partition_type_display_names() {
        assert_eq!(
            PartitionType::MbrType(0x07).display_name(),
            "NTFS/exFAT/HPFS"
        );
        assert_eq!(PartitionType::MbrType(0x0C).display_name(), "FAT32 (LBA)");
        assert_eq!(
            PartitionType::MbrType(0xEE).display_name(),
            "GPT Protective"
        );
        assert_eq!(PartitionType::MbrType(0xAB).display_name(), "MBR Type 0xAB");
    }

    #[test]
    fn gpt_uuid_from_le_bytes_correct() {
        // EFI System UUID: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
        let bytes: [u8; 16] = [
            0x28, 0x73, 0x2A, 0xC1, // C12A7328 (LE)
            0x1F, 0xF8, // F81F (LE)
            0xD2, 0x11, // 11D2 (LE)
            0xBA, 0x4B, // BA4B (BE のまま)
            0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B, // 00A0C93EC93B
        ];
        let uuid = uuid_from_le_bytes(&bytes);
        assert_eq!(
            uuid.to_string().to_uppercase(),
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B"
        );
    }

    #[test]
    fn gpt_type_name_known_uuids() {
        let efi = uuid::Uuid::parse_str("C12A7328-F81F-11D2-BA4B-00A0C93EC93B").unwrap();
        assert_eq!(gpt_type_name(&efi), "EFI System");

        let basic_data = uuid::Uuid::parse_str("EBD0A0A2-B9E5-4433-87C0-68B6B72699C7").unwrap();
        assert_eq!(gpt_type_name(&basic_data), "Microsoft Basic Data");

        let unknown = uuid::Uuid::parse_str("12345678-1234-1234-1234-123456789012").unwrap();
        assert_eq!(gpt_type_name(&unknown), "Unknown");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "管理者権限が必要なローカル検証用テスト"]
    fn integration_read_system_drive_partitions() {
        // システムドライブのパーティションを読み取って表示
        let result = PhysicalDrive::open(r"\\.\PhysicalDrive0");
        let drive = match result {
            Err(PhysicalDriveError::AccessDenied { .. }) => {
                println!("管理者権限なし、スキップ");
                return;
            }
            Err(e) => panic!("open に失敗: {:?}", e),
            Ok(d) => d,
        };

        let partitions = drive.list_partitions().expect("list_partitions failed");
        println!("PhysicalDrive0 のパーティション: {} 個", partitions.len());
        for p in &partitions {
            println!(
                "  Partition {}: {} {}, offset={}, size={}",
                p.number,
                p.partition_type.display_name(),
                p.fs_type.display_name(),
                p.start_offset,
                p.size
            );
        }

        // システムドライブは最低 1 つのパーティションを持つはず
        assert!(!partitions.is_empty());
    }
}
