//! フィクサップ（Update Sequence）共有モジュール。MFT エントリ（`mft.rs`）と INDX ブロック
//! （`attributes/index.rs`）両方で使う汎用 USA 検証・復元。Chunk 12 で共有化。
//! 書籍『File System Forensic Analysis』Ch.12「Fixup Values」/ Ch.13 Table 13.15 準拠。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04。
use thiserror::Error;

/// `apply_fixup` のエラー型。MFT/INDX 両方で `#[from]` 経由で再ラップ。
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FixupError {
    #[error("Buffer too small for fixup: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    #[error("Invalid USA offset: {offset}")]
    InvalidUsaOffset { offset: u16 },
    #[error("Invalid USA size: {size}")]
    InvalidUsaSize { size: u16 },
    #[error("Fixup mismatch at sector {sector}: expected USN 0x{expected:04X}, got 0x{got:04X}")]
    FixupMismatch {
        sector: usize,
        expected: u16,
        got: u16,
    },
}

/// USA を読み取り、各セクタ末尾 2 バイトに対しフィクサップを適用（in-place）。
/// 呼び出し側固有の事前検証（MFT: `usa_offset >= 48` 等）は本関数では扱わない。汎用維持のため。
/// 関連 FR: FR-LIVE-01, FR-LIVE-04。
pub fn apply_fixup(
    bytes: &mut [u8],
    usa_offset: u16,
    usa_size: u16,
    sector_size: u16,
) -> Result<(), FixupError> {
    if usa_size == 0 {
        return Err(FixupError::InvalidUsaSize { size: usa_size });
    }
    let (usa_off, usa_bytes) = (usa_offset as usize, (usa_size as usize) * 2);
    let need = usa_off
        .checked_add(usa_bytes)
        .ok_or(FixupError::InvalidUsaOffset { offset: usa_offset })?;
    if need > bytes.len() {
        return Err(FixupError::BufferTooSmall {
            got: bytes.len(),
            need,
        });
    }
    let usn = u16::from_le_bytes([bytes[usa_off], bytes[usa_off + 1]]);
    let (sectors, ss) = ((usa_size as usize) - 1, sector_size as usize);
    if ss < 2 {
        return Err(FixupError::InvalidUsaSize { size: usa_size });
    }
    let span = sectors
        .checked_mul(ss)
        .ok_or(FixupError::InvalidUsaSize { size: usa_size })?;
    if span > bytes.len() {
        return Err(FixupError::BufferTooSmall {
            got: bytes.len(),
            need: span,
        });
    }
    for i in 0..sectors {
        let pos = ss * (i + 1) - 2;
        let got = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        if got != usn {
            return Err(FixupError::FixupMismatch {
                sector: i,
                expected: usn,
                got,
            });
        }
        let fx_off = usa_off + 2 + i * 2;
        bytes[pos] = bytes[fx_off];
        bytes[pos + 1] = bytes[fx_off + 1];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn build(usn: u16, fx0: u16, fx1: u16, tail1: u16) -> Vec<u8> {
        let mut b = vec![0u8; 1024];
        b[0x30..0x32].copy_from_slice(&usn.to_le_bytes());
        b[0x32..0x34].copy_from_slice(&fx0.to_le_bytes());
        b[0x34..0x36].copy_from_slice(&fx1.to_le_bytes());
        b[0x1FE..0x200].copy_from_slice(&usn.to_le_bytes());
        b[0x3FE..0x400].copy_from_slice(&tail1.to_le_bytes());
        b
    }
    /// 2 セクタ分のレコードでフィクサップを通すと末尾 2 バイトが復元される。
    #[test]
    fn apply_fixup_basic_two_sector_record() {
        let mut buf = build(0x1234, 0xAABB, 0xCCDD, 0x1234);
        apply_fixup(&mut buf, 0x30, 3, 512).expect("ok");
        assert_eq!(&buf[0x1FE..0x200], &0xAABBu16.to_le_bytes());
        assert_eq!(&buf[0x3FE..0x400], &0xCCDDu16.to_le_bytes());
    }
    /// USN 不一致セクタを `FixupMismatch` で検出（sector 番号報告）。
    #[test]
    fn apply_fixup_propagates_mismatch_error() {
        let mut buf = build(0x1234, 0xAABB, 0xCCDD, 0x9999);
        assert_eq!(
            apply_fixup(&mut buf, 0x30, 3, 512).unwrap_err(),
            FixupError::FixupMismatch {
                sector: 1,
                expected: 0x1234,
                got: 0x9999
            }
        );
    }
    /// `usa_size == 0` は無効値として弾く（無限ループ・除算エラー防止）。
    #[test]
    fn apply_fixup_rejects_zero_usa_size() {
        let mut buf = vec![0u8; 1024];
        assert_eq!(
            apply_fixup(&mut buf, 0x30, 0, 512).unwrap_err(),
            FixupError::InvalidUsaSize { size: 0 }
        );
    }
}
