# Chunk 24d-2 指示: パーティションテーブル解析

Phase 1.5 拡張の **第 2 段階**。物理ドライブのパーティションテーブル (MBR / GPT) を解析し、各パーティションの位置・サイズ・FS タイプを取得する。

> 🎯 完了時点で「`workbench-dryrun list-drives --physical` で物理ドライブとパーティション情報が表示される」状態に到達。続く Chunk 24d-3 で NtfsVolume との統合を行う基盤。

---

## 全体像 (Chunk 24d シリーズ)

```
✅ Chunk 24d-1: 物理ディスクアクセス層 (完了)
🚧 Chunk 24d-2: パーティションテーブル解析 ← 本指示書
⏳ Chunk 24d-3: NtfsVolume との統合 (パーティション → 復旧)
⏳ Chunk 24d-4: 実機ドライランとフィードバック反映
```

## 本チャンクのスコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | MBR (Master Boot Record) パーサー |
| **B** | GPT (GUID Partition Table) パーサー |
| **C** | パーティションの FS タイプ判定 (NTFS / FAT32 / exFAT / Unknown) |
| **D** | `PhysicalDrive` から「パーティションの一覧」を取得する API |
| **E** | `workbench-dryrun list-drives --physical` でパーティション情報を表示 |

### 含まないもの

```
✗ NtfsVolume を物理パーティションで open する (Chunk 24d-3)
✗ diagnose --physical / recover --physical の実装 (Chunk 24d-3)
✗ ext4 / HFS+ 等の Unix 系 FS 判定 (Phase 2)
✗ 暗号化 FS (BitLocker) の対応 (Phase 2)
✗ 動的ディスク / Storage Spaces (Phase 2)
```

## 対象クレート

- **新規ファイル**: `crates/disk-io/src/partition.rs`, `crates/disk-io/src/fs_detection.rs`
- **修正**: `crates/disk-io/src/physical.rs`, `crates/disk-io/src/lib.rs`
- **修正**: `crates/workbench-dryrun/src/commands/list_drives.rs`

## 重要な設計原則

### unsafe の追加なし (基本)

```
[Chunk 24d-1 後の現状]
crates/recovery/src/timestamps.rs: 5-10 行
crates/disk-io/src/physical.rs: 約 30 行

[Chunk 24d-2 後]
追加 unsafe: 0 行 (バイト列パースは全て safe Rust)
合計: 約 35-40 行 (変化なし)
```

パーティション解析はバイト列の解釈なので、unsafe 不要。

### パーティション番号は 1 ベース (Windows 風)

```
[Windows 慣習]
diskpart コマンドでも "Partition 1", "Partition 2"
ユーザの直感に合う

[業界標準]
R-STUDIO、TestDisk、AOMEI 等もほぼ 1 ベース
```

### Phase 1.5 のスコープに合わせた精度

```
[Chunk 24d-2 で判定する]
NTFS / FAT32 / exFAT / Unknown
シグネチャベースの簡易判定

[Chunk 24d-3 で深掘りする]
壊れた NTFS の判定 (実際に NtfsVolume で open して判断)
$MFT が読めるかの確認
```

Chunk 24d-2 では「FS タイプを特定する」だけ。健全性判定は次のチャンク。

## 仕様参照

### ビジネス要件

- **FR-PHY-04** (パーティションテーブル解析) ← 新規達成
- **FR-PHY-05** (FS タイプ判定) ← 新規達成

### 技術仕様

- **MBR**: Microsoft 公式仕様 + 一般的な実装慣習
- **GPT**: UEFI 仕様 v2.10
- **NTFS ブートセクタ**: Microsoft NTFS 仕様
- **FAT32/exFAT ブートセクタ**: Microsoft FAT 仕様

## 実装内容

### Part A: `crates/disk-io/src/partition.rs` (新規ファイル)

```rust
//! パーティションテーブル (MBR / GPT) の解析.
//!
//! 物理ドライブの先頭セクタを読み、パーティションテーブルを解析する。
//! MBR と GPT の両方をサポート。
//!
//! ## 解析の流れ
//!
//! 1. 先頭 512 バイト (LBA 0) を読む
//! 2. MBR シグネチャ (0x55AA) を確認
//! 3. MBR パーティションテーブルから最大 4 つのパーティションを取得
//! 4. パーティションタイプが 0xEE (Protective MBR) ならば GPT を解析:
//!    - LBA 1 から GPT ヘッダを読む
//!    - パーティションエントリ配列を読む
//!    - 各エントリからパーティション情報を抽出

use std::fmt;
use thiserror::Error;

use crate::physical::{PhysicalDrive, PhysicalDriveError};

/// パーティション解析エラー
#[derive(Debug, Error)]
pub enum PartitionError {
    #[error("物理ドライブの読み取りに失敗: {0}")]
    Read(#[from] PhysicalDriveError),
    
    #[error("MBR シグネチャが無効: 期待 0x55AA、実際 0x{0:04X}")]
    InvalidMbrSignature(u16),
    
    #[error("GPT ヘッダのシグネチャが無効")]
    InvalidGptSignature,
    
    #[error("GPT ヘッダの CRC が無効")]
    InvalidGptCrc,
    
    #[error("パーティションテーブルが破損している可能性: {0}")]
    Corrupted(String),
    
    #[error("バイト列の長さが不足: 必要 {required}、実際 {actual}")]
    InsufficientData { required: usize, actual: usize },
}

/// パーティション情報
#[derive(Debug, Clone)]
pub struct Partition {
    /// パーティション番号 (1 ベース、Windows 風)
    pub number: u32,
    
    /// パーティション開始位置 (バイト)
    pub start_offset: u64,
    
    /// パーティションサイズ (バイト)
    pub size: u64,
    
    /// パーティションタイプ (MBR の場合) または UUID (GPT の場合)
    pub partition_type: PartitionType,
    
    /// 検出された FS タイプ (Chunk 24d-2 で実装)
    pub fs_type: super::fs_detection::FsType,
}

/// パーティションタイプ
#[derive(Debug, Clone)]
pub enum PartitionType {
    /// MBR パーティションタイプ ID
    /// (例: 0x07 = NTFS/HPFS, 0x0B/0x0C = FAT32, 0x83 = Linux)
    MbrType(u8),
    
    /// GPT パーティションタイプ UUID
    /// (例: EBD0A0A2-B9E5-4433-87C0-68B6B72699C7 = Basic Data, NTFS含む)
    GptType(uuid::Uuid),
}

impl PartitionType {
    /// 業務的に表示用の文字列
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

/// GPT パーティションタイプ UUID から名前を取得
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
pub fn read_partitions(drive: &PhysicalDrive) -> Result<Vec<Partition>, PartitionError> {
    // 先頭セクタ (MBR) を読む
    let mbr_bytes = drive.read_at(0, 512)?;
    
    if mbr_bytes.len() < 512 {
        return Err(PartitionError::InsufficientData {
            required: 512,
            actual: mbr_bytes.len(),
        });
    }
    
    // MBR シグネチャ (0x55AA at offset 510)
    let signature = u16::from_le_bytes([mbr_bytes[510], mbr_bytes[511]]);
    if signature != 0xAA55 {
        return Err(PartitionError::InvalidMbrSignature(signature));
    }
    
    // MBR パーティションテーブル (offset 446 から 16 バイト × 4 エントリ)
    let mbr_partitions = parse_mbr_partitions(&mbr_bytes)?;
    
    // GPT 判定 (最初のパーティションタイプが 0xEE)
    let is_gpt = mbr_partitions.iter()
        .any(|p| matches!(p.partition_type, PartitionType::MbrType(0xEE)));
    
    if is_gpt {
        // GPT を解析
        parse_gpt(drive)
    } else {
        // MBR のまま、各パーティションの FS を判定
        let mut result = Vec::new();
        for (i, mut partition) in mbr_partitions.into_iter().enumerate() {
            if partition.size > 0 {
                partition.number = (i + 1) as u32;
                partition.fs_type = super::fs_detection::detect_fs_type(drive, partition.start_offset)
                    .unwrap_or(super::fs_detection::FsType::Unknown);
                result.push(partition);
            }
        }
        Ok(result)
    }
}

/// MBR パーティションテーブルを解析
fn parse_mbr_partitions(mbr: &[u8]) -> Result<Vec<Partition>, PartitionError> {
    if mbr.len() < 512 {
        return Err(PartitionError::InsufficientData { required: 512, actual: mbr.len() });
    }
    
    let mut partitions = Vec::new();
    
    // MBR パーティションエントリ: offset 446 から 16 バイト × 4
    for i in 0..4 {
        let entry_offset = 446 + i * 16;
        let entry = &mbr[entry_offset..entry_offset + 16];
        
        let partition_type_byte = entry[4];
        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
        let num_sectors = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);
        
        // 空のエントリはスキップ
        if partition_type_byte == 0x00 || num_sectors == 0 {
            continue;
        }
        
        partitions.push(Partition {
            number: (i + 1) as u32,  // 1 ベース
            start_offset: (start_lba as u64) * 512,
            size: (num_sectors as u64) * 512,
            partition_type: PartitionType::MbrType(partition_type_byte),
            fs_type: super::fs_detection::FsType::Unknown,  // 後で判定
        });
    }
    
    Ok(partitions)
}

/// GPT パーティションテーブルを解析
fn parse_gpt(drive: &PhysicalDrive) -> Result<Vec<Partition>, PartitionError> {
    // GPT ヘッダは LBA 1 (オフセット 512 バイト)
    let gpt_header_bytes = drive.read_at(512, 512)?;
    
    if gpt_header_bytes.len() < 92 {
        return Err(PartitionError::InsufficientData { required: 92, actual: gpt_header_bytes.len() });
    }
    
    // シグネチャ確認 ("EFI PART" = 0x5452415020494645)
    let signature = &gpt_header_bytes[0..8];
    if signature != b"EFI PART" {
        return Err(PartitionError::InvalidGptSignature);
    }
    
    // パーティションエントリの位置とサイズ
    let partition_entries_lba = u64::from_le_bytes([
        gpt_header_bytes[72], gpt_header_bytes[73], gpt_header_bytes[74], gpt_header_bytes[75],
        gpt_header_bytes[76], gpt_header_bytes[77], gpt_header_bytes[78], gpt_header_bytes[79],
    ]);
    let num_entries = u32::from_le_bytes([
        gpt_header_bytes[80], gpt_header_bytes[81], gpt_header_bytes[82], gpt_header_bytes[83],
    ]);
    let entry_size = u32::from_le_bytes([
        gpt_header_bytes[84], gpt_header_bytes[85], gpt_header_bytes[86], gpt_header_bytes[87],
    ]);
    
    // 各エントリを読む
    let total_size = (num_entries as u64) * (entry_size as u64);
    let entries_offset = partition_entries_lba * 512;
    
    // 妥当性チェック
    if entry_size < 128 || entry_size > 4096 {
        return Err(PartitionError::Corrupted(format!(
            "GPT エントリサイズが異常: {}", entry_size
        )));
    }
    
    if num_entries > 128 {
        return Err(PartitionError::Corrupted(format!(
            "GPT エントリ数が異常: {}", num_entries
        )));
    }
    
    let entries_bytes = drive.read_at(entries_offset, total_size as usize)?;
    
    let mut partitions = Vec::new();
    let mut partition_number: u32 = 1;  // 1 ベース
    
    for i in 0..(num_entries as usize) {
        let entry_offset = i * (entry_size as usize);
        if entry_offset + 128 > entries_bytes.len() {
            break;
        }
        
        let entry = &entries_bytes[entry_offset..entry_offset + 128];
        
        // パーティションタイプ UUID (offset 0-15)
        let type_uuid_bytes: [u8; 16] = entry[0..16].try_into().unwrap();
        let type_uuid = uuid_from_le_bytes(&type_uuid_bytes);
        
        // 未使用エントリ (UUID が all-zero) はスキップ
        if type_uuid_bytes.iter().all(|&b| b == 0) {
            continue;
        }
        
        // 開始 LBA、終了 LBA
        let start_lba = u64::from_le_bytes([
            entry[32], entry[33], entry[34], entry[35],
            entry[36], entry[37], entry[38], entry[39],
        ]);
        let end_lba = u64::from_le_bytes([
            entry[40], entry[41], entry[42], entry[43],
            entry[44], entry[45], entry[46], entry[47],
        ]);
        
        if end_lba < start_lba {
            continue;
        }
        
        let start_offset = start_lba * 512;
        let size = (end_lba - start_lba + 1) * 512;
        
        // FS タイプ判定
        let fs_type = super::fs_detection::detect_fs_type(drive, start_offset)
            .unwrap_or(super::fs_detection::FsType::Unknown);
        
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

/// GPT の UUID バイト列を `uuid::Uuid` に変換する。
///
/// GPT の UUID は最初の 3 フィールド (4 + 2 + 2 バイト) が
/// リトルエンディアン、残りはビッグエンディアン。
fn uuid_from_le_bytes(bytes: &[u8; 16]) -> uuid::Uuid {
    // GPT 形式: 最初の 3 フィールドが LE、残りが BE
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
        // 最小限の MBR (空のパーティションテーブル + シグネチャ)
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        
        // パーティション 1: タイプ 0x07 (NTFS)、LBA 2048、サイズ 100MB
        let entry_offset = 446;
        mbr[entry_offset + 4] = 0x07;  // タイプ
        mbr[entry_offset + 8..entry_offset + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[entry_offset + 12..entry_offset + 16].copy_from_slice(&((100 * 1024 * 1024 / 512) as u32).to_le_bytes());
        
        let partitions = parse_mbr_partitions(&mbr).unwrap();
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].number, 1);
        assert_eq!(partitions[0].start_offset, 2048 * 512);
        assert_eq!(partitions[0].size, 100 * 1024 * 1024);
        assert!(matches!(partitions[0].partition_type, PartitionType::MbrType(0x07)));
    }
    
    #[test]
    fn mbr_partition_parse_empty_entries_skipped() {
        let mut mbr = vec![0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        
        // 全エントリ空
        let partitions = parse_mbr_partitions(&mbr).unwrap();
        assert!(partitions.is_empty());
    }
    
    #[test]
    fn partition_type_display_names() {
        assert_eq!(PartitionType::MbrType(0x07).display_name(), "NTFS/exFAT/HPFS");
        assert_eq!(PartitionType::MbrType(0x0C).display_name(), "FAT32 (LBA)");
        assert_eq!(PartitionType::MbrType(0xEE).display_name(), "GPT Protective");
    }
    
    #[test]
    fn gpt_uuid_from_le_bytes_correct() {
        // EFI System の UUID: C12A7328-F81F-11D2-BA4B-00A0C93EC93B
        // GPT 形式のバイト列 (最初の 3 フィールドが LE)
        let bytes: [u8; 16] = [
            0x28, 0x73, 0x2A, 0xC1,  // C12A7328 (LE)
            0x1F, 0xF8,              // F81F (LE)
            0xD2, 0x11,              // 11D2 (LE)
            0xBA, 0x4B,              // BA4B (BE のまま)
            0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,  // 00A0C93EC93B
        ];
        let uuid = uuid_from_le_bytes(&bytes);
        assert_eq!(uuid.to_string().to_uppercase(), "C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
    }
}
```

### Part B: `crates/disk-io/src/fs_detection.rs` (新規ファイル)

```rust
//! ファイルシステムタイプの検出.
//!
//! パーティションの先頭セクタ (ブートセクタ) を読み、
//! シグネチャから FS タイプを判定する。
//!
//! ## サポート FS
//! - NTFS
//! - FAT32
//! - exFAT
//! - その他は Unknown
//!
//! ## 注意
//!
//! シグネチャベースの簡易判定。FS の健全性は Chunk 24d-3 で
//! 実際に NtfsVolume を open することで判定する。

use crate::physical::{PhysicalDrive, PhysicalDriveError};

/// ファイルシステムタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsType {
    Ntfs,
    Fat32,
    ExFat,
    Unknown,
}

impl FsType {
    /// 業務的に表示用の文字列
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ntfs => "NTFS",
            Self::Fat32 => "FAT32",
            Self::ExFat => "exFAT",
            Self::Unknown => "Unknown",
        }
    }
    
    /// この FS が Workbench で復旧可能か
    /// Phase 1.5 では NTFS のみ対応
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Ntfs)
    }
}

/// 指定オフセットのブートセクタから FS タイプを判定
pub fn detect_fs_type(
    drive: &PhysicalDrive,
    partition_offset: u64,
) -> Result<FsType, PhysicalDriveError> {
    // ブートセクタ (512 バイト) を読む
    let boot_sector = drive.read_at(partition_offset, 512)?;
    
    if boot_sector.len() < 512 {
        return Ok(FsType::Unknown);
    }
    
    Ok(detect_from_boot_sector(&boot_sector))
}

/// ブートセクタのバイト列から FS タイプを判定 (純粋関数)
pub fn detect_from_boot_sector(boot_sector: &[u8]) -> FsType {
    if boot_sector.len() < 90 {
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
    if boot_sector.len() >= 90 && &boot_sector[82..90] == b"FAT32   " {
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
        assert!(!FsType::Fat32.is_recoverable());  // Phase 1.5 では NTFS のみ
        assert!(!FsType::ExFat.is_recoverable());
        assert!(!FsType::Unknown.is_recoverable());
    }
}
```

### Part C: `crates/disk-io/src/physical.rs` への追加

`PhysicalDrive` に「パーティション一覧を取得する」メソッドを追加:

```rust
impl PhysicalDrive {
    /// このドライブのパーティション一覧を取得する。
    ///
    /// MBR / GPT を自動判定して解析する。
    pub fn list_partitions(&self) -> Result<Vec<crate::partition::Partition>, crate::partition::PartitionError> {
        crate::partition::read_partitions(self)
    }
}
```

### Part D: `crates/disk-io/src/lib.rs` への追加

```rust
// 既存:
pub mod logical;
pub mod physical;

// 新規追加:
pub mod partition;
pub mod fs_detection;

// 公開 API:
pub use physical::{
    enumerate_physical_drives, PhysicalDrive, PhysicalDriveError, PhysicalDriveInfo, BusType,
};
pub use partition::{Partition, PartitionType, PartitionError};
pub use fs_detection::{FsType, detect_from_boot_sector};
```

### Part E: `crates/disk-io/Cargo.toml` の依存追加

```toml
[dependencies]
# 既存:
windows-sys = { version = "0.52", features = [...] }

# 新規追加:
uuid = { version = "1.10", features = ["v4"] }
```

### Part F: workbench-dryrun の表示更新

`crates/workbench-dryrun/src/commands/list_drives.rs` の `run_physical()` を更新:

```rust
fn run_physical() -> Result<()> {
    println!("物理ドライブ:");
    println!("---------------------------------------------");
    
    let infos = enumerate_physical_drives();
    
    if infos.is_empty() {
        println!("物理ドライブが検出されませんでした。");
        println!();
        println!("考えられる原因:");
        println!("  1. 管理者権限がない");
        println!("  2. 物理的に接続されているドライブがない");
        return Ok(());
    }
    
    println!("検出された物理ドライブ: {} 個", infos.len());
    println!();
    
    for info in &infos {
        // ドライブの基本情報
        let bus_label = if info.bus_type.is_removable() {
            format!("{} (リムーバブル)", info.bus_type.display_name())
        } else {
            info.bus_type.display_name().to_string()
        };
        
        println!("[{}] {} - {} {}", 
            info.drive_number,
            info.path.display(),
            dds_core::format::format_bytes(info.total_bytes),
            bus_label,
        );
        
        if let Some(vendor) = &info.vendor_id {
            print!("    Vendor: {}", vendor);
            if let Some(product) = &info.product_id {
                print!(" | Product: {}", product);
            }
            println!();
        }
        
        if let Some(serial) = &info.serial_number {
            println!("    Serial: {}", serial);
        }
        
        // ★ パーティション情報を取得して表示 (Chunk 24d-2 で追加)
        match PhysicalDrive::open(&info.path) {
            Ok(drive) => {
                match drive.list_partitions() {
                    Ok(partitions) if partitions.is_empty() => {
                        println!("    └─ パーティション情報を取得できませんでした");
                    }
                    Ok(partitions) => {
                        for partition in &partitions {
                            let recoverable_mark = if partition.fs_type.is_recoverable() {
                                " ★ 復旧対象"
                            } else {
                                ""
                            };
                            
                            println!("    └─ Partition {}: {}, {}, {}{}", 
                                partition.number,
                                partition.partition_type.display_name(),
                                dds_core::format::format_bytes(partition.size),
                                partition.fs_type.display_name(),
                                recoverable_mark,
                            );
                        }
                    }
                    Err(e) => {
                        println!("    └─ パーティション解析エラー: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("    └─ ドライブを再 open できませんでした: {}", e);
            }
        }
        println!();
    }
    
    println!("---------------------------------------------");
    println!("注: diagnose --physical / recover --physical は Chunk 24d-3 で追加予定");
    
    Ok(())
}
```

## 単体テスト要件 (最低 10 件)

### `partition.rs` (最低 4 件)

1. `mbr_partition_parse_valid`: 正常な MBR を解析
2. `mbr_partition_parse_empty_entries_skipped`: 空エントリはスキップ
3. `partition_type_display_names`: 表示名が正しい
4. `gpt_uuid_from_le_bytes_correct`: GPT UUID 変換が正しい

### `fs_detection.rs` (最低 6 件)

5. `detect_ntfs_signature`
6. `detect_fat32_signature`
7. `detect_exfat_signature`
8. `detect_unknown_for_random_bytes`
9. `detect_unknown_for_short_buffer`
10. `fs_type_display_names` + `fs_type_recoverable`

### 統合テスト (任意、Windows + 管理者権限のみ)

```rust
#[cfg(windows)]
#[test]
#[ignore]  // ローカル検証用
fn integration_read_system_drive_partitions() {
    let drive = PhysicalDrive::open(r"\\.\PhysicalDrive0").unwrap();
    let partitions = drive.list_partitions().unwrap();
    
    println!("PhysicalDrive0 のパーティション: {} 個", partitions.len());
    for p in &partitions {
        println!("  Partition {}: {:?} {}, offset={}, size={}",
            p.number, p.partition_type, p.fs_type.display_name(),
            p.start_offset, p.size);
    }
    
    // システムドライブは最低 1 つのパーティションを持つはず
    assert!(!partitions.is_empty());
}
```

## 制約

- **行数目安**:
  - `crates/disk-io/src/partition.rs` (新規): 約 300 行 + テスト 50 行
  - `crates/disk-io/src/fs_detection.rs` (新規): 約 100 行 + テスト 60 行
  - `crates/disk-io/src/physical.rs` 修正: +10 行 (list_partitions メソッド)
  - `crates/disk-io/src/lib.rs` 修正: +5 行
  - `crates/disk-io/Cargo.toml` 修正: +1 行 (uuid 依存)
  - `crates/workbench-dryrun/src/commands/list_drives.rs` 修正: +30 行 (パーティション表示)
  - 合計: 約 555 行追加・修正
- **単体テスト新規**: 最低 10 件
- **統合テスト**: 1 件 (`#[ignore]` 付き)
- **`unsafe` 追加行数**: 0 (バイト列パースのみ)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `cargo doc --workspace --no-deps`: 全公開 API に rustdoc
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (Chunk 24d-1 から変化なし)
- [ ] MBR パーティション解析が動作
- [ ] GPT パーティション解析が動作
- [ ] NTFS / FAT32 / exFAT の検出が動作
- [ ] `workbench-dryrun list-drives --physical` でパーティション情報が表示される
- [ ] NTFS パーティションに「★ 復旧対象」マークが付く

## 関連 FR 要件

- **FR-PHY-04** (パーティションテーブル解析) ← 達成
- **FR-PHY-05** (FS タイプ判定) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **次のチャンク: Chunk 24d-3 (NtfsVolume との統合 + --physical 対応)**

---

## 注意事項

### MBR と GPT の判定ロジック

```
[判定の流れ]
1. LBA 0 (先頭 512 バイト) を読む
2. シグネチャ 0x55AA を確認 (offset 510)
3. 最初のパーティションタイプを見る:
   - 0xEE → "Protective MBR" → GPT を読む
   - その他 → 純粋な MBR

[Protective MBR]
GPT ディスクは MBR 領域も持つ。
最初のパーティションタイプを 0xEE にすることで
「これは GPT だよ」と示している。
古い OS が誤って書き込むのを防ぐ仕組み。
```

### GPT の UUID 形式の注意

```
[GPT の UUID]
最初の 3 フィールド (4 + 2 + 2 バイト) が**リトルエンディアン**
残りの 8 バイトは**ビッグエンディアン**

例: EFI System の UUID
表記: C12A7328-F81F-11D2-BA4B-00A0C93EC93B

バイト列 (GPT 形式):
  28 73 2A C1  ← C12A7328 (LE)
  1F F8        ← F81F (LE)
  D2 11        ← 11D2 (LE)
  BA 4B        ← BA4B (BE のまま)
  00 A0 C9 3E C9 3B  ← 00A0C93EC93B (BE のまま)
```

これは間違えやすいので、`uuid_from_le_bytes` のテストを必ず通す。

### パーティション番号は 1 ベース

```rust
// 業界慣習に合わせて 1 から始まる
Partition 1, Partition 2, ...

// Rust の配列インデックス (0 ベース) との混同に注意
for (i, p) in partitions.iter().enumerate() {
    // i は 0 ベース、p.number は 1 ベース
}
```

### FS 判定はシグネチャベースの簡易判定

```
[Chunk 24d-2 のスコープ]
- ブートセクタのマジックバイトを確認するだけ
- 例: NTFS なら "NTFS    " at offset 3
- これで「FS タイプを特定」できる

[Chunk 24d-2 のスコープ外]
- $MFT が読めるかの確認 → Chunk 24d-3
- 「壊れている」「健全」の判定 → Chunk 24d-3
- パーティション全体の構造的検証 → Phase 2 以降
```

シンプルにすることで、誤検出のリスクを抑える。

### 「★ 復旧対象」マークの業務的意義

```
[業務的な狙い]
CS が物理ドライブを見たとき:
  「どのパーティションを復旧すべきか」が一目で分かる

[例]
PhysicalDrive1: 1.8 TB USB
  └─ Partition 1: EFI System, 200 MB, FAT32
  └─ Partition 2: MSR, 16 MB, Unknown
  └─ Partition 3: NTFS/exFAT/HPFS, 1.8 TB, NTFS ★ 復旧対象

→ CS: 「Partition 3 を選んで復旧」
```

業務的なオペレーションのガイドになる。

### 拡張子なしの uuid 依存

```toml
uuid = { version = "1.10", features = ["v4"] }
```

`features = ["v4"]` は UUIDv4 生成用。本チャンクではパース機能だけ必要なので、本来は不要。
ただし、将来 (Chunk 24d-3 等) で「案件 ID に UUID を使う」可能性に備えて、最初から含めておく。

不要なら `features = []` で OK。

### Phase 2.1 UI への引き継ぎ

```
[Tauri UI で表示する情報]
物理ドライブのツリー構造:
  PhysicalDrive0
    ├─ Partition 1 (EFI)
    ├─ Partition 2 (MSR)
    └─ Partition 3 (NTFS) ← 復旧対象、選択可能

[Chunk 24d-2 で公開する API]
PhysicalDrive::list_partitions()
Partition (start_offset, size, partition_type, fs_type)
→ UI で表示しやすい構造
```

---

## 質問が必要なケース

- 既存の workbench-dryrun の list_drives モジュールが想定外の構造になっている
- uuid クレートのバージョン互換性問題
- 既存の disk-io クレートに同名の `partition` モジュールが既にある

---

## 完了報告例

```markdown
## Chunk 24d-2 完了報告

### 新規ファイル
- crates/disk-io/src/partition.rs (約 300 行 + テスト 50 行)
- crates/disk-io/src/fs_detection.rs (約 100 行 + テスト 60 行)

### 修正ファイル
- crates/disk-io/src/physical.rs (+10 行 list_partitions メソッド)
- crates/disk-io/src/lib.rs (+5 行 公開 API)
- crates/disk-io/Cargo.toml (+1 行 uuid 依存)
- crates/workbench-dryrun/src/commands/list_drives.rs (+30 行 パーティション表示)

### 新規 API
- read_partitions(&PhysicalDrive) -> Result<Vec<Partition>, PartitionError>
- detect_fs_type(&PhysicalDrive, partition_offset) -> Result<FsType, PhysicalDriveError>
- detect_from_boot_sector(&[u8]) -> FsType
- PhysicalDrive::list_partitions() -> Result<Vec<Partition>, PartitionError>

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行 (Chunk 24d-1 から変化なし)

### テスト統計
- 単体: 既存 + 新規 10 件
- 統合: 1 件 (#[ignore]、ローカル検証用)
- 全 workspace: 全パス

### 動作確認サンプル (管理者として実行)
```
> workbench-dryrun list-drives --physical
物理ドライブ:
---------------------------------------------
検出された物理ドライブ: 2 個

[0] \\.\PhysicalDrive0 - 931.5 GB NVMe
    Vendor: SAMSUNG | Product: MZVL2512HCJQ-00B07
    Serial: S64ANJ0RA00012
    └─ Partition 1: EFI System, 100.0 MB, FAT32
    └─ Partition 2: Microsoft Reserved, 16.0 MB, Unknown
    └─ Partition 3: Microsoft Basic Data, 930.5 GB, NTFS ★ 復旧対象
    └─ Partition 4: Windows Recovery, 880.0 MB, NTFS ★ 復旧対象

[1] \\.\PhysicalDrive1 - 1.8 TB USB (リムーバブル)
    Vendor: Seagate | Product: ST2000DM006-2DM164
    Serial: Z7E1234
    └─ Partition 1: NTFS/exFAT/HPFS, 1.8 TB, NTFS ★ 復旧対象

---------------------------------------------
注: diagnose --physical / recover --physical は Chunk 24d-3 で追加予定
```

### 🎯 達成事項
- MBR / GPT 両方のパーティションテーブルを解析可能
- NTFS / FAT32 / exFAT の検出が動作
- 「★ 復旧対象」マークで業務的なガイドを提供
- 壊れた FS の HDD でも、パーティション構造は見える (シグネチャがあれば)

### 次のステップ
Chunk 24d-3 で:
- NtfsVolume を物理パーティションで open
- diagnose --physical / recover --physical の実装
- パーティション指定オプション (--partition N)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんが Chunk 24d-3 の指示書を私に依頼
```
