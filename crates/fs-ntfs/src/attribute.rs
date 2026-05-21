//! NTFS 属性共通ヘッダパーサ。MFT エントリ内の属性連を巡回するための基盤。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）, FR-LIVE-06（メタデータ表示）。
//! 仕様: <https://flatcap.github.io/linux-ntfs/ntfs/concepts/attribute_header.html>
use thiserror::Error;
const MIN_COMMON: usize = 16;
const MIN_RESIDENT: usize = 24;
const MIN_NONRESIDENT: usize = 0x40;
const END_MARKER: u32 = 0xFFFF_FFFF;
/// NTFS 属性タイプ。未知値は `Unknown(raw)` として保持（forward compatibility）。関連 FR: FR-LIVE-01。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum AttributeType {
    StandardInformation,
    AttributeList,
    FileName,
    ObjectId,
    SecurityDescriptor,
    VolumeName,
    VolumeInformation,
    Data,
    IndexRoot,
    IndexAllocation,
    Bitmap,
    ReparsePoint,
    EaInformation,
    Ea,
    LoggedUtilityStream,
    /// 未知の属性タイプ（エラーにせず保持）。
    Unknown(u32),
    /// 0xFFFFFFFF 終端マーカー。
    End,
}
macro_rules! atypes { ($($raw:expr => $var:ident),* $(,)?) => { impl AttributeType {
    /// 32bit 生値から変換。未知値は `Unknown(value)`。
    pub fn from_raw(value: u32) -> Self { match value {
        $($raw => Self::$var,)* END_MARKER => Self::End, other => Self::Unknown(other) } }
    /// 32bit 生値に変換。
    pub fn to_raw(&self) -> u32 { match self {
        $(Self::$var => $raw,)* Self::Unknown(v) => *v, Self::End => END_MARKER } }
}}; }
atypes!(0x10 => StandardInformation, 0x20 => AttributeList, 0x30 => FileName,
    0x40 => ObjectId, 0x50 => SecurityDescriptor, 0x60 => VolumeName,
    0x70 => VolumeInformation, 0x80 => Data, 0x90 => IndexRoot,
    0xA0 => IndexAllocation, 0xB0 => Bitmap, 0xC0 => ReparsePoint,
    0xD0 => EaInformation, 0xE0 => Ea, 0x100 => LoggedUtilityStream);
/// 全属性に共通する先頭 16 バイトのヘッダ。関連 FR: FR-LIVE-01。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct AttributeCommonHeader {
    pub attribute_type: AttributeType,
    pub length: u32,
    pub non_resident: bool,
    pub name_length: u8,
    pub name_offset: u16,
    pub flags: u16,
    pub attribute_id: u16,
}
/// 常駐属性固有ヘッダ（offset 0x10〜）。関連 FR: FR-LIVE-01。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ResidentInfo {
    pub content_size: u32,
    pub content_offset: u16,
    pub indexed: bool,
}
/// 非常駐属性固有ヘッダ（offset 0x10〜0x40）。関連 FR: FR-LIVE-01。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct NonResidentInfo {
    pub starting_vcn: u64,
    pub last_vcn: u64,
    pub runlist_offset: u16,
    pub compression_unit_size: u16,
    pub allocated_size: u64,
    pub real_size: u64,
    pub initialized_size: u64,
}
/// 単一属性ヘッダ（常駐/非常駐/終端）。関連 FR: FR-LIVE-01, FR-LIVE-06。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum AttributeHeader {
    Resident {
        common: AttributeCommonHeader,
        resident: ResidentInfo,
    },
    NonResident {
        common: AttributeCommonHeader,
        non_resident: NonResidentInfo,
    },
    /// 0xFFFFFFFF 終端マーカー（属性巡回はここで停止）。
    End,
}
impl AttributeHeader {
    /// 共通ヘッダへのアクセサ。`End` は `None`。
    pub fn common(&self) -> Option<&AttributeCommonHeader> {
        match self {
            Self::Resident { common, .. } | Self::NonResident { common, .. } => Some(common),
            Self::End => None,
        }
    }
    /// 属性総バイト長（次属性までのオフセット）。`End` なら 0。
    pub fn length(&self) -> u32 {
        self.common().map(|c| c.length).unwrap_or(0)
    }
    /// 属性タイプ。`End` なら `AttributeType::End`。
    pub fn attribute_type(&self) -> AttributeType {
        self.common()
            .map(|c| c.attribute_type)
            .unwrap_or(AttributeType::End)
    }
    /// 終端マーカーか。
    pub fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }
}
/// `parse_attribute_header` が返すエラー型。関連 FR: FR-LIVE-01。
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum AttributeError {
    #[error("Buffer too small for attribute header: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    #[error("Invalid attribute length: {length} (must be > 0)")]
    InvalidLength { length: u32 },
    #[error("Invalid non-resident flag: {got} (must be 0 or 1)")]
    InvalidNonResidentFlag { got: u8 },
}
/// 単一属性ヘッダを解析。type ID が 0xFFFFFFFF なら `End` を即返却。関連 FR: FR-LIVE-01, FR-LIVE-06。
pub fn parse_attribute_header(bytes: &[u8]) -> Result<AttributeHeader, AttributeError> {
    let too_small = |need| AttributeError::BufferTooSmall {
        got: bytes.len(),
        need,
    };
    if bytes.len() < 4 {
        return Err(too_small(4));
    }
    let type_id = u32::from_le_bytes(bytes[0..4].try_into().expect("len 4"));
    if type_id == END_MARKER {
        return Ok(AttributeHeader::End);
    }
    if bytes.len() < MIN_COMMON {
        return Err(too_small(MIN_COMMON));
    }
    let u16le = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    let u64le = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().expect("len 8"));
    let length = u32le(0x04);
    if length == 0 {
        return Err(AttributeError::InvalidLength { length });
    }
    let non_resident = match bytes[0x08] {
        0 => false,
        1 => true,
        other => return Err(AttributeError::InvalidNonResidentFlag { got: other }),
    };
    let common = AttributeCommonHeader {
        attribute_type: AttributeType::from_raw(type_id),
        length,
        non_resident,
        name_length: bytes[0x09],
        name_offset: u16le(0x0A),
        flags: u16le(0x0C),
        attribute_id: u16le(0x0E),
    };
    let need = if non_resident {
        MIN_NONRESIDENT
    } else {
        MIN_RESIDENT
    };
    if bytes.len() < need {
        return Err(too_small(need));
    }
    if non_resident {
        Ok(AttributeHeader::NonResident {
            common,
            non_resident: NonResidentInfo {
                starting_vcn: u64le(0x10),
                last_vcn: u64le(0x18),
                runlist_offset: u16le(0x20),
                compression_unit_size: u16le(0x22),
                allocated_size: u64le(0x28),
                real_size: u64le(0x30),
                initialized_size: u64le(0x38),
            },
        })
    } else {
        Ok(AttributeHeader::Resident {
            common,
            resident: ResidentInfo {
                content_size: u32le(0x10),
                content_offset: u16le(0x14),
                indexed: bytes[0x16] != 0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn br(type_id: u32, length: u32) -> Vec<u8> {
        let mut b = vec![0u8; length.max(24) as usize];
        b[0..4].copy_from_slice(&type_id.to_le_bytes());
        b[4..8].copy_from_slice(&length.to_le_bytes());
        b[0x0A..0x0C].copy_from_slice(&0x18u16.to_le_bytes());
        b[0x0C..0x0E].copy_from_slice(&1u16.to_le_bytes());
        b[0x0E..0x10].copy_from_slice(&7u16.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&0x48u32.to_le_bytes());
        b[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
        b[0x16] = 1;
        b
    }
    fn bnr(type_id: u32, real_size: u64) -> Vec<u8> {
        let mut b = vec![0u8; 0x50];
        b[0..4].copy_from_slice(&type_id.to_le_bytes());
        b[4..8].copy_from_slice(&0x50u32.to_le_bytes());
        b[0x08] = 1;
        b[0x18..0x20].copy_from_slice(&3u64.to_le_bytes());
        b[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        b[0x28..0x30].copy_from_slice(&16384u64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&real_size.to_le_bytes());
        b
    }
    #[test]
    fn attribute_type_from_raw_roundtrip_main_types() {
        for (raw, exp) in [
            (0x10u32, AttributeType::StandardInformation),
            (0x30, AttributeType::FileName),
            (0x80, AttributeType::Data),
            (0xB0, AttributeType::Bitmap),
            (0x100, AttributeType::LoggedUtilityStream),
        ] {
            assert_eq!(AttributeType::from_raw(raw), exp);
            assert_eq!(exp.to_raw(), raw);
        }
    }
    #[test]
    fn attribute_type_unknown_and_end() {
        assert_eq!(AttributeType::from_raw(0x42), AttributeType::Unknown(0x42));
        assert_eq!(AttributeType::Unknown(0x42).to_raw(), 0x42);
        assert_eq!(AttributeType::from_raw(END_MARKER), AttributeType::End);
        assert_eq!(AttributeType::End.to_raw(), END_MARKER);
    }
    #[test]
    fn parses_resident_header_all_fields() {
        let h = parse_attribute_header(&br(0x30, 0x60)).expect("parse");
        let c = h.common().expect("common");
        assert_eq!(
            (
                c.attribute_type,
                c.length,
                c.non_resident,
                c.flags,
                c.attribute_id
            ),
            (AttributeType::FileName, 0x60, false, 1, 7)
        );
        if let AttributeHeader::Resident { resident: r, .. } = h {
            assert_eq!(
                (r.content_size, r.content_offset, r.indexed),
                (0x48, 0x18, true)
            );
        } else {
            panic!("expected resident")
        }
    }
    #[test]
    fn parses_nonresident_header_all_fields() {
        let h = parse_attribute_header(&bnr(0x80, 12345)).expect("parse");
        let c = h.common().expect("common");
        assert!(c.non_resident && c.attribute_type == AttributeType::Data);
        if let AttributeHeader::NonResident {
            non_resident: nr, ..
        } = h
        {
            assert_eq!(
                (
                    nr.runlist_offset,
                    nr.real_size,
                    nr.allocated_size,
                    nr.last_vcn
                ),
                (0x40, 12345, 16384, 3)
            );
        } else {
            panic!("expected non-resident")
        }
    }
    #[test]
    fn end_marker_returned_immediately() {
        let h = parse_attribute_header(&[0xFFu8; 4]).expect("end");
        assert!(h.is_end());
        assert_eq!(h.length(), 0);
        assert_eq!(h.attribute_type(), AttributeType::End);
        assert!(h.common().is_none());
    }
    #[test]
    fn buffer_too_small_rejected() {
        assert_eq!(
            parse_attribute_header(&[0u8; 2]).unwrap_err(),
            AttributeError::BufferTooSmall { got: 2, need: 4 }
        );
        assert_eq!(
            parse_attribute_header(&[0x10u8, 0, 0, 0, 1, 0, 0, 0]).unwrap_err(),
            AttributeError::BufferTooSmall {
                got: 8,
                need: MIN_COMMON
            }
        );
    }
    #[test]
    fn invalid_non_resident_flag_rejected() {
        let mut buf = br(0x10, 0x20);
        buf[0x08] = 2;
        assert!(matches!(
            parse_attribute_header(&buf).unwrap_err(),
            AttributeError::InvalidNonResidentFlag { got: 2 }
        ));
    }
    #[test]
    fn zero_length_rejected_prevents_infinite_loop() {
        let mut buf = br(0x10, 0x20);
        buf[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            parse_attribute_header(&buf).unwrap_err(),
            AttributeError::InvalidLength { length: 0 }
        ));
    }
    /// 書籍 Table 13.2/13.3 例題再現: 96 バイト常駐 $STANDARD_INFORMATION。
    /// type=0x10, length=0x60, content_size=0x48, content_offset=0x18, 0x18+0x48=0x60。
    #[test]
    fn book_example_si_resident_96_byte_attribute() {
        let mut b = vec![0u8; 0x60];
        b[0..4].copy_from_slice(&0x10u32.to_le_bytes());
        b[4..8].copy_from_slice(&0x60u32.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&0x48u32.to_le_bytes());
        b[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes());
        let h = parse_attribute_header(&b).expect("parse");
        let length = h.length();
        let c = h.common().expect("common").clone();
        assert_eq!(
            (
                c.attribute_type,
                c.length,
                c.non_resident,
                c.name_length,
                c.flags,
                c.attribute_id
            ),
            (AttributeType::StandardInformation, 0x60, false, 0, 0, 0)
        );
        if let AttributeHeader::Resident { resident: r, .. } = h {
            assert_eq!((r.content_size, r.content_offset), (0x48, 0x18));
            assert_eq!(u32::from(r.content_offset) + r.content_size, length);
        } else {
            panic!("expected resident")
        }
    }
    /// 書籍 Table 13.2/13.4 例題再現: 非常駐 $DATA + runlist。
    /// type=0x80, vcn 0..0x20EF, runlist_offset=0x40, sizes=0x83C000。
    #[test]
    fn book_example_data_nonresident_with_runlist() {
        let mut b = vec![0u8; 0x60];
        b[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        b[4..8].copy_from_slice(&0x60u32.to_le_bytes());
        b[0x08] = 1;
        b[0x18..0x20].copy_from_slice(&0x20EFu64.to_le_bytes());
        b[0x20..0x22].copy_from_slice(&0x40u16.to_le_bytes());
        b[0x28..0x30].copy_from_slice(&0x83C000u64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&0x83C000u64.to_le_bytes());
        b[0x38..0x40].copy_from_slice(&0x83C000u64.to_le_bytes());
        let h = parse_attribute_header(&b).expect("parse");
        let c = h.common().expect("common");
        assert_eq!(
            (
                c.attribute_type,
                c.length,
                c.non_resident,
                c.name_length,
                c.flags
            ),
            (AttributeType::Data, 0x60, true, 0, 0)
        );
        if let AttributeHeader::NonResident {
            non_resident: nr, ..
        } = h
        {
            assert_eq!(
                (nr.starting_vcn, nr.last_vcn, nr.runlist_offset),
                (0, 0x20EF, 0x40)
            );
            assert_eq!(
                (nr.allocated_size, nr.real_size, nr.initialized_size),
                (0x83C000, 0x83C000, 0x83C000)
            );
        } else {
            panic!("expected non-resident")
        }
    }
    /// 書籍 Chapter 13 で言及される全 15 種 + Unknown 3 種 + End ラウンドトリップ網羅。
    #[test]
    fn all_attribute_types_roundtrip_including_unknown_and_end() {
        let known: &[(u32, AttributeType)] = &[
            (0x10, AttributeType::StandardInformation),
            (0x20, AttributeType::AttributeList),
            (0x30, AttributeType::FileName),
            (0x40, AttributeType::ObjectId),
            (0x50, AttributeType::SecurityDescriptor),
            (0x60, AttributeType::VolumeName),
            (0x70, AttributeType::VolumeInformation),
            (0x80, AttributeType::Data),
            (0x90, AttributeType::IndexRoot),
            (0xA0, AttributeType::IndexAllocation),
            (0xB0, AttributeType::Bitmap),
            (0xC0, AttributeType::ReparsePoint),
            (0xD0, AttributeType::EaInformation),
            (0xE0, AttributeType::Ea),
            (0x100, AttributeType::LoggedUtilityStream),
        ];
        for (raw, exp) in known {
            assert_eq!(AttributeType::from_raw(*raw), *exp);
            assert_eq!(exp.to_raw(), *raw);
        }
        for raw in [0x42u32, 0xFF, 0x200] {
            assert_eq!(AttributeType::from_raw(raw), AttributeType::Unknown(raw));
            assert_eq!(AttributeType::Unknown(raw).to_raw(), raw);
        }
        assert_eq!(AttributeType::from_raw(END_MARKER), AttributeType::End);
        assert_eq!(AttributeType::End.to_raw(), END_MARKER);
    }
    /// 書籍 Table 13.2 flags (0x0001/0x4000/0x8000) の組合せを生値保持し
    /// 呼び出し側のビット演算で個別判定可能なことを検証。
    #[test]
    fn flag_bit_combinations_preserved_as_raw_value() {
        const C: u16 = 0x0001;
        const E: u16 = 0x4000;
        const S: u16 = 0x8000;
        for combo in [C, E, S, C | E, C | E | S] {
            let mut b = br(0x80, 0x30);
            b[0x0C..0x0E].copy_from_slice(&combo.to_le_bytes());
            let c = parse_attribute_header(&b)
                .expect("parse")
                .common()
                .expect("common")
                .clone();
            assert_eq!(c.flags, combo);
            assert_eq!(
                (c.flags & C != 0, c.flags & E != 0, c.flags & S != 0),
                (combo & C != 0, combo & E != 0, combo & S != 0)
            );
        }
    }
}
