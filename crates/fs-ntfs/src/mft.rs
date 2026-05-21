//! NTFS MFT エントリ（FILE レコード）ヘッダパーサ。フィクサップ（Update Sequence）も適用する。
//! Chunk 12 で `crate::fixup` モジュールにフィクサップロジックを共有化（INDX ブロックと共用）。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-05（削除エントリ可視化）。
//! 仕様: <https://flatcap.github.io/linux-ntfs/ntfs/concepts/file_record.html>
use crate::fixup::{apply_fixup, FixupError};
use thiserror::Error;

const MIN_HEADER_SIZE: usize = 48;
const MAGIC_FILE: &[u8; 4] = b"FILE";
const MAGIC_BAAD: &[u8; 4] = b"BAAD";
const DEFAULT_SECTOR_SIZE: u16 = 512;
const FLAG_IN_USE: u16 = 0x0001;
const FLAG_DIR: u16 = 0x0002;

/// MFT エントリヘッダ（先頭約 48 バイト）。全フィールドリトルエンディアン、MFT 仕様と 1:1 対応。関連 FR: FR-LIVE-01, FR-LIVE-05。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct MftEntryHeader {
    pub usa_offset: u16,
    /// USA のワード数（USN 1 ワード + 各セクタ用 fixup）。`(allocated_size + sector_size - 1) / sector_size + 1` と一致するのが正常値。
    pub usa_size: u16,
    pub lsn: u64,
    /// 同一 MFT レコード番号の世代カウンタ。エントリが割当または解放されるたびに +1 され、過去のファイル参照が現在の別ファイルを指していないか検出するのに使う。関連 FR: FR-LIVE-05。
    pub sequence_number: u16,
    /// このエントリを参照するディレクトリエントリ数（ハードリンク含む）。別名リンクが作成されるたびに +1 される。値 0 は通常削除済み（未参照）。
    pub hard_link_count: u16,
    pub first_attribute_offset: u16, pub flags: u16,
    pub used_size: u32, pub allocated_size: u32,
    pub base_record_reference: u64, pub next_attribute_id: u16,
    /// 自身の MFT レコード番号（XP+）。offset 0x2C が 0 なら `None`。
    pub mft_record_number: Option<u32>,
}

/// パース済み MFT エントリ。`data` はフィクサップ適用済みの全データ。
#[derive(Debug, Clone)]
pub struct MftEntry {
    /// ヘッダ部分の解析結果。
    pub header: MftEntryHeader,
    /// フィクサップ適用済みのエントリ全体バイト列。属性パースはここから行う。
    pub data: Vec<u8>,
}

/// `parse_mft_entry` が返すエラー型。フィクサップ関連エラーは `Fixup(FixupError)` バリアントに
/// 集約（Chunk 12 リファクタ）。`MftError::InvalidUsaSize` は MFT 固有の事前検証で出るもののみ。
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum MftError {
    #[error("Buffer too small for MFT entry: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    #[error("Invalid MFT entry magic: expected 'FILE', got {got:?}")]
    InvalidMagic { got: [u8; 4] },
    /// `"BAAD"` シグネチャ検出（NTFS が破損を明示マーキング）。
    #[error("BAAD MFT entry: data corruption detected")]
    BadEntry,
    /// MFT 固有の事前検証エラー: usa_offset がヘッダ最小サイズ（48）未満。
    #[error("Invalid USA offset: {offset}")]
    InvalidUsaOffset { offset: u16 },
    /// MFT 固有の事前検証エラー: usa_size が allocated_size 整合ルールから外れる。
    #[error("Invalid USA size: {size}")]
    InvalidUsaSize { size: u16 },
    #[error("Fixup error: {0}")]
    Fixup(#[from] FixupError),
    #[error("used_size ({used}) exceeds allocated_size ({allocated})")]
    UsedExceedsAllocated { used: u32, allocated: u32 },
}

/// MFT エントリ（FILE レコード）1 件をパースし、フィクサップ（sector_size=512）を適用して返す。関連 FR: FR-LIVE-01, FR-LIVE-05。
pub fn parse_mft_entry(bytes: &[u8]) -> Result<MftEntry, MftError> {
    if bytes.len() < MIN_HEADER_SIZE {
        return Err(MftError::BufferTooSmall { got: bytes.len(), need: MIN_HEADER_SIZE });
    }
    let magic: [u8; 4] = bytes[0..4].try_into().expect("len 4");
    if &magic == MAGIC_BAAD { return Err(MftError::BadEntry); }
    if &magic != MAGIC_FILE { return Err(MftError::InvalidMagic { got: magic }); }
    let u16le = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    let u64le = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().expect("len 8"));
    let (usa_offset, usa_size) = (u16le(0x04), u16le(0x06));
    let (used_size, allocated_size) = (u32le(0x18), u32le(0x1C));
    if used_size > allocated_size {
        return Err(MftError::UsedExceedsAllocated { used: used_size, allocated: allocated_size });
    }
    // Carrier 著書記載の整合性ルール: USA size = ceil(allocated_size / sector_size) + 1。
    // 不一致は破損疑い。allocated_size=0 の場合は他チェックに委ねる。
    if allocated_size > 0 {
        let expected = allocated_size.div_ceil(DEFAULT_SECTOR_SIZE as u32) + 1;
        if expected != usa_size as u32 {
            return Err(MftError::InvalidUsaSize { size: usa_size });
        }
    }
    let rec_no_raw = u32le(0x2C);
    let header = MftEntryHeader {
        usa_offset, usa_size, lsn: u64le(0x08),
        sequence_number: u16le(0x10), hard_link_count: u16le(0x12),
        first_attribute_offset: u16le(0x14), flags: u16le(0x16),
        used_size, allocated_size,
        base_record_reference: u64le(0x20), next_attribute_id: u16le(0x28),
        mft_record_number: if rec_no_raw == 0 { None } else { Some(rec_no_raw) },
    };
    // MFT 固有の事前検証: usa_offset はヘッダ最小サイズ (48) 以上である必要がある。
    // 共有 fixup モジュールは汎用なのでこのチェックは呼び出し側責務。
    if (usa_offset as usize) < MIN_HEADER_SIZE {
        return Err(MftError::InvalidUsaOffset { offset: usa_offset });
    }
    let mut data = bytes.to_vec();
    apply_fixup(&mut data, usa_offset, usa_size, DEFAULT_SECTOR_SIZE)?;
    Ok(MftEntry { header, data })
}

impl MftEntryHeader {
    /// 使用中なら true（`flags & 0x0001 != 0`）。関連 FR: FR-LIVE-05。
    pub fn is_in_use(&self) -> bool { self.flags & FLAG_IN_USE != 0 }
    /// 削除済みなら true。関連 FR: FR-LIVE-05。
    pub fn is_deleted(&self) -> bool { !self.is_in_use() }
    /// ディレクトリなら true。関連 FR: FR-LIVE-01。
    pub fn is_directory(&self) -> bool { self.flags & FLAG_DIR != 0 }
    /// ベースレコードなら true（拡張レコードでない）。関連 FR: FR-LIVE-01。
    pub fn is_base_record(&self) -> bool { self.base_record_reference == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_valid_mft_entry(flags: u16, usn: u16, fx0: u16, fx1: u16) -> Vec<u8> {
        let mut b = vec![0u8; 1024];
        let put16 = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let put32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        b[0..4].copy_from_slice(b"FILE");
        put16(&mut b, 0x04, 0x30); put16(&mut b, 0x06, 3);
        b[0x08..0x10].copy_from_slice(&0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes());
        put16(&mut b, 0x10, 5); put16(&mut b, 0x12, 1);
        put16(&mut b, 0x14, 0x38); put16(&mut b, 0x16, flags);
        put32(&mut b, 0x18, 512); put32(&mut b, 0x1C, 1024);
        put16(&mut b, 0x28, 7); put32(&mut b, 0x2C, 42);
        put16(&mut b, 0x30, usn); put16(&mut b, 0x32, fx0); put16(&mut b, 0x34, fx1);
        put16(&mut b, 0x1FE, usn); put16(&mut b, 0x3FE, usn);
        b
    }

    #[test]
    fn parses_valid_header_fields() {
        let e = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0xAABB, 0xCCDD)).unwrap();
        assert_eq!((e.header.usa_offset, e.header.usa_size), (0x30, 3));
        assert_eq!(e.header.lsn, 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!((e.header.sequence_number, e.header.hard_link_count), (5, 1));
        assert_eq!(e.header.first_attribute_offset, 0x38);
        assert_eq!((e.header.used_size, e.header.allocated_size), (512, 1024));
        assert_eq!(e.header.next_attribute_id, 7);
        assert_eq!(e.header.mft_record_number, Some(42));
        assert!(e.header.is_base_record());
    }

    #[test]
    fn baad_signature_is_bad_entry() {
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0, 0);
        buf[0..4].copy_from_slice(b"BAAD");
        assert_eq!(parse_mft_entry(&buf).unwrap_err(), MftError::BadEntry);
    }

    #[test]
    fn invalid_magic_rejected() {
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0, 0);
        buf[0..4].copy_from_slice(b"XXXX");
        assert!(matches!(parse_mft_entry(&buf).unwrap_err(), MftError::InvalidMagic { .. }));
    }
    #[test]
    fn flags_in_use_deleted_directory() {
        let iu = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0, 0)).unwrap();
        assert!(iu.header.is_in_use() && !iu.header.is_deleted() && !iu.header.is_directory());
        let d = parse_mft_entry(&build_valid_mft_entry(0, 0x1234, 0, 0)).unwrap();
        assert!(d.header.is_deleted() && !d.header.is_in_use());
        let dir = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE | FLAG_DIR, 0x1234, 0, 0)).unwrap();
        assert!(dir.header.is_in_use() && dir.header.is_directory());
    }
    #[test]
    fn fixup_applied_restores_sector_tails() {
        let e = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0xAABB, 0xCCDD)).unwrap();
        assert_eq!(&e.data[0x1FE..0x200], &0xAABBu16.to_le_bytes());
        assert_eq!(&e.data[0x3FE..0x400], &0xCCDDu16.to_le_bytes());
    }
    #[test]
    fn fixup_mismatch_detected() {
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0xAABB, 0xCCDD);
        buf[0x3FE..0x400].copy_from_slice(&0x9999u16.to_le_bytes());
        // Chunk 12 リファクタ後: `MftError::Fixup(FixupError::FixupMismatch { .. })` で伝播。
        assert!(matches!(
            parse_mft_entry(&buf).unwrap_err(),
            MftError::Fixup(FixupError::FixupMismatch { sector: 1, expected: 0x1234, got: 0x9999 })
        ));
    }
    #[test]
    fn used_exceeds_allocated_rejected() {
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0xAABB, 0xCCDD);
        buf[0x18..0x1C].copy_from_slice(&2048u32.to_le_bytes());
        assert!(matches!(parse_mft_entry(&buf).unwrap_err(),
            MftError::UsedExceedsAllocated { used: 2048, allocated: 1024 }));
    }
    #[test]
    fn buffer_too_small_rejected() {
        assert!(matches!(parse_mft_entry(&[0u8; 10]).unwrap_err(),
            MftError::BufferTooSmall { got: 10, need: 48 }));
    }

    #[test]
    fn book_example_signature_0x0058_applies_fixup() {
        // Carrier Ch.13 例: USN=0x0058, USA size=3, record=1024, sector=512, fixup=0x0000 x2。
        let e = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE, 0x0058, 0x0000, 0x0000)).unwrap();
        assert_eq!(&e.data[0x1FE..0x200], &[0x00, 0x00]);
        assert_eq!(&e.data[0x3FE..0x400], &[0x00, 0x00]);
        assert_eq!(e.header.usa_size, 3);
    }
    #[test]
    fn usa_size_mismatch_with_record_size_rejected() {
        // allocated_size=1024 なら正しい usa_size は 3。10 は破損疑い → InvalidUsaSize。
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0, 0);
        buf[0x06..0x08].copy_from_slice(&10u16.to_le_bytes());
        assert!(matches!(parse_mft_entry(&buf).unwrap_err(), MftError::InvalidUsaSize { size: 10 }));
    }
    #[test]
    fn parses_2kb_entry_with_four_fixups() {
        // 2KB エントリ: allocated_size=2048, usa_size=5（USN + 4 fixup）。
        let (usn, fx): (u16, [u16; 4]) = (0xBEEF, [0x1111, 0x2222, 0x3333, 0x4444]);
        let mut b = vec![0u8; 2048];
        b[0..4].copy_from_slice(b"FILE");
        b[0x04..0x08].copy_from_slice(&[0x30, 0, 5, 0]); // usa_offset=0x30, usa_size=5
        b[0x14..0x18].copy_from_slice(&[0x40, 0, FLAG_IN_USE as u8, 0]); // first_attr=0x40, flags=in_use
        b[0x18..0x1C].copy_from_slice(&1024u32.to_le_bytes());
        b[0x1C..0x20].copy_from_slice(&2048u32.to_le_bytes());
        b[0x30..0x32].copy_from_slice(&usn.to_le_bytes());
        for (i, v) in fx.iter().enumerate() {
            b[0x32 + i * 2..0x34 + i * 2].copy_from_slice(&v.to_le_bytes());
            let pos = 512 * (i + 1) - 2;
            b[pos..pos + 2].copy_from_slice(&usn.to_le_bytes());
        }
        let e = parse_mft_entry(&b).unwrap();
        for (i, v) in fx.iter().enumerate() {
            let pos = 512 * (i + 1) - 2;
            assert_eq!(&e.data[pos..pos + 2], &v.to_le_bytes(), "sector {i}");
        }
        assert_eq!(e.header.allocated_size, 2048);
    }
    #[test]
    fn usn_zero_is_accepted() {
        // USN=0 は未割り当てエントリで普通に起こる。全セクタ末尾も 0 なら正常に fixup される。
        let e = parse_mft_entry(&build_valid_mft_entry(FLAG_IN_USE, 0x0000, 0xAABB, 0xCCDD)).unwrap();
        assert_eq!(&e.data[0x1FE..0x200], &0xAABBu16.to_le_bytes());
        assert_eq!(&e.data[0x3FE..0x400], &0xCCDDu16.to_le_bytes());
    }
    #[test]
    fn partial_corruption_detected_at_second_sector() {
        // sector 0 は USN 一致、sector 1 のみ別値 → sector=1 で FixupMismatch（Fixup 経由）。
        let mut buf = build_valid_mft_entry(FLAG_IN_USE, 0x1234, 0xAABB, 0xCCDD);
        assert_eq!(&buf[0x1FE..0x200], &0x1234u16.to_le_bytes());
        buf[0x3FE..0x400].copy_from_slice(&0xDEADu16.to_le_bytes());
        assert!(matches!(
            parse_mft_entry(&buf).unwrap_err(),
            MftError::Fixup(FixupError::FixupMismatch { sector: 1, expected: 0x1234, got: 0xDEAD })
        ));
    }
}
