//! NTFS ブートセクタ（VBR）パーサ。先頭 512 バイトから `$MFT` 位置・クラスタサイズ等を抽出します。
//! 関連 FR: FR-LIVE-01（NTFS 読み取りの第一歩）。
//! 仕様参照: <https://flatcap.github.io/linux-ntfs/ntfs/files/boot.html>
use thiserror::Error;

const BOOT_SECTOR_SIZE: usize = 512;
const BOOT_SIGNATURE: u16 = 0xAA55;
const NTFS_OEM_ID: &[u8; 8] = b"NTFS    ";
const MIN_BYTES_PER_SECTOR: u16 = 256;
const MAX_BYTES_PER_SECTOR: u16 = 4096;
const MAX_SECTORS_PER_CLUSTER: u8 = 128;

/// NTFS ブートセクタの解析済み構造体。すべてリトルエンディアン解釈。関連 FR: FR-LIVE-01。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootSector {
    /// 1 セクタあたりのバイト数（通常 512）。
    pub bytes_per_sector: u16,
    /// 1 クラスタあたりのセクタ数（通常 8）。
    pub sectors_per_cluster: u8,
    /// メディア記述子（通常 `0xF8`）。
    pub media_descriptor: u8,
    /// パーティション内総セクタ数。
    pub total_sectors: u64,
    /// `$MFT` の論理クラスタ番号。
    pub mft_lcn: u64,
    /// `$MFTMirr` の論理クラスタ番号。
    pub mft_mirror_lcn: u64,
    /// MFT レコードあたりのクラスタ数。負値は `2^(-value)` バイトを示す（生 i8）。
    pub clusters_per_mft_record: i8,
    /// INDEX レコードあたりのクラスタ数（同上のエンコーディング）。
    pub clusters_per_index_record: i8,
    /// ボリュームシリアル番号。
    pub volume_serial: u64,
}

/// `parse_boot_sector` が返すエラー型。各バリアントの `got` は実際に読み取った値。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BootSectorError {
    /// バッファサイズが 512 バイト未満。
    #[error("Buffer too small: got {got}, need at least 512")]
    BufferTooSmall { #[allow(missing_docs)] got: usize },
    /// OEM ID が `"NTFS    "` と一致しない。
    #[error("Invalid OEM ID: expected 'NTFS    ', got {got:?}")]
    InvalidOemId { #[allow(missing_docs)] got: [u8; 8] },
    /// 末尾シグネチャが `0xAA55` ではない。
    #[error("Invalid boot signature: expected 0xAA55, got 0x{got:04X}")]
    InvalidSignature { #[allow(missing_docs)] got: u16 },
    /// `bytes_per_sector` が 2 の累乗でない、または 256〜4096 の範囲外。
    #[error("Invalid bytes per sector: {got}")]
    InvalidBytesPerSector { #[allow(missing_docs)] got: u16 },
    /// `sectors_per_cluster` が 2 の累乗でない、または 1〜128 の範囲外。
    #[error("Invalid sectors per cluster: {got}")]
    InvalidSectorsPerCluster { #[allow(missing_docs)] got: u8 },
}

#[inline]
fn is_pow2(v: u32) -> bool { v != 0 && (v & (v - 1)) == 0 }

/// 512 バイト以上のスライスから NTFS ブートセクタを解析します（先頭 512 B のみ参照）。
/// 関連 FR: FR-LIVE-01。失敗時は [`BootSectorError`] を返します。
pub fn parse_boot_sector(bytes: &[u8]) -> Result<BootSector, BootSectorError> {
    if bytes.len() < BOOT_SECTOR_SIZE {
        return Err(BootSectorError::BufferTooSmall { got: bytes.len() });
    }
    let b: &[u8; BOOT_SECTOR_SIZE] = bytes[..BOOT_SECTOR_SIZE].try_into().expect("checked");
    let oem: [u8; 8] = b[0x03..0x0B].try_into().expect("len 8");
    if &oem != NTFS_OEM_ID {
        return Err(BootSectorError::InvalidOemId { got: oem });
    }
    let signature = u16::from_le_bytes([b[0x1FE], b[0x1FF]]);
    if signature != BOOT_SIGNATURE {
        return Err(BootSectorError::InvalidSignature { got: signature });
    }
    let bytes_per_sector = u16::from_le_bytes([b[0x0B], b[0x0C]]);
    if !is_pow2(bytes_per_sector.into())
        || !(MIN_BYTES_PER_SECTOR..=MAX_BYTES_PER_SECTOR).contains(&bytes_per_sector)
    {
        return Err(BootSectorError::InvalidBytesPerSector { got: bytes_per_sector });
    }
    let sectors_per_cluster = b[0x0D];
    if !is_pow2(sectors_per_cluster.into()) || sectors_per_cluster > MAX_SECTORS_PER_CLUSTER {
        return Err(BootSectorError::InvalidSectorsPerCluster { got: sectors_per_cluster });
    }
    let u64le = |s: &[u8]| u64::from_le_bytes(s.try_into().expect("len 8"));
    Ok(BootSector {
        bytes_per_sector,
        sectors_per_cluster,
        media_descriptor: b[0x15],
        total_sectors: u64le(&b[0x28..0x30]),
        mft_lcn: u64le(&b[0x30..0x38]),
        mft_mirror_lcn: u64le(&b[0x38..0x40]),
        clusters_per_mft_record: b[0x40] as i8,
        clusters_per_index_record: b[0x44] as i8,
        volume_serial: u64le(&b[0x48..0x50]),
    })
}

/// 符号付きクラスタ数フィールド（MFT/Index record size）を実サイズ（バイト）に展開する共通ロジック。
/// 正値: `raw * cluster_size_bytes`、負値: `1 << (-raw)`（書籍 Table 13.18 のエンコーディング）。
#[inline]
fn compute_record_size_bytes(raw: i8, cluster_size: u32) -> u32 {
    if raw >= 0 {
        (raw as u32) * cluster_size
    } else {
        // i8 最小 -128 でも安全に。実NTFSでは -10 前後しか出現しない。
        1u32 << ((-(raw as i32)) as u32).min(31)
    }
}

impl BootSector {
    /// クラスタサイズ（バイト）= `bytes_per_sector * sectors_per_cluster`。
    pub fn cluster_size_bytes(&self) -> u32 {
        u32::from(self.bytes_per_sector) * u32::from(self.sectors_per_cluster)
    }
    /// MFT レコードサイズ（バイト）。負の `clusters_per_mft_record` は `1 << (-value)` を意味する。
    pub fn mft_record_size_bytes(&self) -> u32 {
        compute_record_size_bytes(self.clusters_per_mft_record, self.cluster_size_bytes())
    }
    /// INDEX レコードサイズ（バイト）。`mft_record_size_bytes` と同じ符号付きエンコーディング規則。
    /// 書籍 Table 13.18 / 381 ページの例: `clusters_per_index_record=4` × 1 KB cluster = 4096 B。
    pub fn index_record_size_bytes(&self) -> u32 {
        compute_record_size_bytes(self.clusters_per_index_record, self.cluster_size_bytes())
    }
    /// `$MFT` のバイトオフセット = `mft_lcn * cluster_size_bytes()`。
    pub fn mft_byte_offset(&self) -> u64 {
        self.mft_lcn * u64::from(self.cluster_size_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 有効な NTFS ブートセクタ（bps=512, spc=8, mft_lcn=4, cpmr=-10）を生成。
    fn make_valid_boot_sector() -> [u8; 512] {
        let mut b = [0u8; 512];
        b[0..3].copy_from_slice(&[0xEB, 0x52, 0x90]);
        b[3..11].copy_from_slice(b"NTFS    ");
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 8; b[0x15] = 0xF8; b[0x44] = 1;
        b[0x28..0x30].copy_from_slice(&40_960u64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&4u64.to_le_bytes());
        b[0x38..0x40].copy_from_slice(&20u64.to_le_bytes());
        b[0x40] = (-10i8) as u8;
        b[0x48..0x50].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
        b[0x1FE] = 0x55; b[0x1FF] = 0xAA;
        b
    }

    #[test]
    fn parses_valid_boot_sector_all_fields() {
        let bs = parse_boot_sector(&make_valid_boot_sector()).expect("parse");
        assert_eq!(bs.bytes_per_sector, 512);
        assert_eq!(bs.sectors_per_cluster, 8);
        assert_eq!(bs.media_descriptor, 0xF8);
        assert_eq!(bs.total_sectors, 40_960);
        assert_eq!(bs.mft_lcn, 4);
        assert_eq!(bs.mft_mirror_lcn, 20);
        assert_eq!(bs.clusters_per_mft_record, -10);
        assert_eq!(bs.clusters_per_index_record, 1);
        assert_eq!(bs.volume_serial, 0x0123_4567_89AB_CDEF);
        assert_eq!(bs.cluster_size_bytes(), 4096);
        assert_eq!(bs.mft_byte_offset(), 4 * 4096);
    }

    #[test]
    fn rejects_short_buffer() {
        assert_eq!(parse_boot_sector(&[0u8; 100]).unwrap_err(), BootSectorError::BufferTooSmall { got: 100 });
    }

    #[test]
    fn rejects_invalid_oem_id_and_signature() {
        let mut b = make_valid_boot_sector();
        b[3..11].copy_from_slice(b"FAT32   ");
        assert!(matches!(parse_boot_sector(&b).unwrap_err(),
            BootSectorError::InvalidOemId { .. }));
        b = make_valid_boot_sector();
        b[0x1FE] = 0;
        b[0x1FF] = 0;
        assert_eq!(parse_boot_sector(&b).unwrap_err(),
            BootSectorError::InvalidSignature { got: 0 });
    }

    #[test]
    fn rejects_zero_bps_and_zero_spc() {
        let mut b = make_valid_boot_sector();
        b[0x0B..0x0D].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(parse_boot_sector(&b).unwrap_err(),
            BootSectorError::InvalidBytesPerSector { got: 0 }));
        b = make_valid_boot_sector();
        b[0x0D] = 0;
        assert!(matches!(parse_boot_sector(&b).unwrap_err(),
            BootSectorError::InvalidSectorsPerCluster { got: 0 }));
    }

    #[test]
    fn mft_record_size_negative_and_positive_encodings() {
        // negative: 2^(-value) bytes; positive: value * cluster_size (=4096 here)
        let mut b = make_valid_boot_sector();
        for (v, exp) in [(-10i8, 1024u32), (-12, 4096), (1, 4096), (2, 8192)] {
            b[0x40] = v as u8;
            assert_eq!(parse_boot_sector(&b).unwrap().mft_record_size_bytes(), exp, "v={v}");
        }
    }

    #[test]
    fn cluster_size_various_combinations() {
        for (bps, spc, exp) in [(512u16, 1u8, 512u32), (512, 8, 4096), (4096, 1, 4096), (1024, 4, 4096)] {
            let mut b = make_valid_boot_sector();
            b[0x0B..0x0D].copy_from_slice(&bps.to_le_bytes());
            b[0x0D] = spc;
            assert_eq!(parse_boot_sector(&b).unwrap().cluster_size_bytes(), exp, "bps={bps} spc={spc}");
        }
    }

    #[test]
    fn book_example_512_byte_sector_2_spc_1kb_cluster() {
        // 書籍 Table 13.18 サンプル: bps=512, spc=2 (→cluster=1024), cpmr=1 (→1024B), cpir=4 (→4096B)。
        let mut b = make_valid_boot_sector();
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 2;
        b[0x28..0x30].copy_from_slice(&2_056_256u64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&342_709u64.to_le_bytes());
        b[0x38..0x40].copy_from_slice(&514_064u64.to_le_bytes());
        b[0x40] = 1; b[0x44] = 4;
        b[0x48..0x50].copy_from_slice(&0x0450_2284_5022_7C94u64.to_le_bytes());
        let bs = parse_boot_sector(&b).expect("parse");
        assert_eq!((bs.bytes_per_sector, bs.sectors_per_cluster), (512, 2));
        assert_eq!((bs.total_sectors, bs.mft_lcn, bs.mft_mirror_lcn), (2_056_256, 342_709, 514_064));
        assert_eq!((bs.clusters_per_mft_record, bs.clusters_per_index_record), (1, 4));
        assert_eq!(bs.volume_serial, 0x0450_2284_5022_7C94);
        assert_eq!((bs.cluster_size_bytes(), bs.mft_record_size_bytes(), bs.index_record_size_bytes()),
            (1024, 1024, 4096));
    }

    #[test]
    fn index_record_size_negative_and_positive_encodings() {
        // 同じ符号付きエンコーディング規則が index record にも適用される。bps=512, spc=2 → cluster=1024。
        let mut b = make_valid_boot_sector();
        b[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
        b[0x0D] = 2;
        for (v, exp) in [(4i8, 4096u32), (-12, 4096), (-10, 1024)] {
            b[0x44] = v as u8;
            assert_eq!(parse_boot_sector(&b).unwrap().index_record_size_bytes(), exp, "v={v}");
        }
    }

    #[test]
    fn parses_4kn_drive_with_4096_byte_sectors() {
        // Advanced Format 4Kn: bps=4096, spc=1 → cluster=4096。
        let mut b = make_valid_boot_sector();
        b[0x0B..0x0D].copy_from_slice(&4096u16.to_le_bytes());
        b[0x0D] = 1;
        let bs = parse_boot_sector(&b).expect("parse");
        assert_eq!((bs.bytes_per_sector, bs.sectors_per_cluster, bs.cluster_size_bytes()), (4096, 1, 4096));
    }

    #[test]
    fn non_power_of_two_bytes_per_sector_rejected() {
        for bad in [1000u16, 100, 8192] {
            let mut b = make_valid_boot_sector();
            b[0x0B..0x0D].copy_from_slice(&bad.to_le_bytes());
            assert!(matches!(parse_boot_sector(&b).unwrap_err(),
                BootSectorError::InvalidBytesPerSector { got } if got == bad),
                "bad bps={bad} should be rejected");
        }
    }

    #[test]
    fn non_power_of_two_sectors_per_cluster_rejected() {
        // 3 / 192 / 130: 非2累乗。256 は u8 不能のため、上限 128 超は 192/130 で検証。
        for bad in [3u8, 192, 130] {
            let mut b = make_valid_boot_sector();
            b[0x0D] = bad;
            assert!(matches!(parse_boot_sector(&b).unwrap_err(),
                BootSectorError::InvalidSectorsPerCluster { got } if got == bad),
                "bad spc={bad} should be rejected");
        }
    }
}
