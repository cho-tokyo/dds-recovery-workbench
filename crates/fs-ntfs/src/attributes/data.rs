//! `$DATA` 属性（タイプ 0x80）パーサ。常駐版で実バイト列を、非常駐版で runlist 情報のみを
//! 取り出す。非常駐の実データ取得は Chunk 10 で対応する。関連 FR: FR-LIVE-01（NTFS 読み取り）、
//! FR-LIVE-04（ファイルツリー構築の前提）、FR-REC-01（目標優先抽出）、FR-REC-04（データ整合性）。
//! 仕様: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/data.html>
use crate::attribute::{AttributeCommonHeader, AttributeHeader, AttributeType};
use crate::attributes::AttributeIterator;
use thiserror::Error;
const FLAG_COMPRESSED: u16 = 0x0001;
const FLAG_ENCRYPTED: u16 = 0x4000;
const FLAG_SPARSE: u16 = 0x8000;
/// `$DATA` 属性のコンテンツ。常駐は実バイト参照、非常駐は runlist 情報のみ。関連 FR: FR-LIVE-01, FR-REC-04。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum DataContent<'a> {
    /// 常駐: コンテンツバイト列がそのままファイル内容。
    Resident { bytes: &'a [u8], size: u32 },
    /// 非常駐: 実バイトはクラスタに散在。Chunk 10 の runlist デコードで取得する。
    NonResident {
        real_size: u64, allocated_size: u64, starting_vcn: u64, last_vcn: u64,
        runlist_offset_in_attr: usize, attribute_raw: &'a [u8],
    },
}
impl<'a> DataContent<'a> {
    /// 常駐なら true。
    pub fn is_resident(&self) -> bool { matches!(self, DataContent::Resident { .. }) }
    /// 非常駐なら true。
    pub fn is_non_resident(&self) -> bool { !self.is_resident() }
    /// 論理サイズ。常駐 = `content_size`、非常駐 = `real_size`。
    pub fn size(&self) -> u64 { match self {
        DataContent::Resident { size, .. } => *size as u64,
        DataContent::NonResident { real_size, .. } => *real_size } }
}
/// 1 つの `$DATA` ストリーム（無名メイン or ADS）。`name` 空文字列がメイン無名ストリーム。
/// `is_compressed`/`is_encrypted`/`is_sparse` は属性 flags から派生。関連 FR: FR-LIVE-01, FR-REC-01。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct DataStream<'a> {
    pub name: String, pub content: DataContent<'a>,
    pub is_compressed: bool, pub is_encrypted: bool, pub is_sparse: bool,
}
/// `parse_data_stream` / 抽出関数のエラー型。関連 FR: FR-LIVE-01。
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DataError {
    #[error("Buffer too small for resident data attribute")]
    ResidentBufferTooSmall,
    #[error("Invalid resident content offset: {offset}")]
    InvalidContentOffset { offset: u16 },
    #[error("Invalid stream name (UTF-16 decoding or bounds)")]
    InvalidStreamName,
}
fn extract_attribute_name(raw: &[u8], h: &AttributeCommonHeader) -> Result<String, DataError> {
    if h.name_length == 0 { return Ok(String::new()); }
    let off = h.name_offset as usize;
    let end = off.checked_add((h.name_length as usize) * 2).ok_or(DataError::InvalidStreamName)?;
    if end > raw.len() { return Err(DataError::InvalidStreamName); }
    let u: Vec<u16> = raw[off..end].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16(&u).map_err(|_| DataError::InvalidStreamName)
}
/// 1 つの `$DATA` 属性（ヘッダ + コンテンツ）から `DataStream` を構築する。
/// 関連 FR: FR-LIVE-01, FR-REC-01, FR-REC-04。
pub fn parse_data_stream<'a>(
    attr_raw: &'a [u8], header: &AttributeHeader,
) -> Result<DataStream<'a>, DataError> {
    let c = header.common().ok_or(DataError::ResidentBufferTooSmall)?;
    if c.attribute_type != AttributeType::Data { return Err(DataError::ResidentBufferTooSmall); }
    let name = extract_attribute_name(attr_raw, c)?;
    let content = match header {
        AttributeHeader::Resident { resident: r, .. } => {
            let s = r.content_offset as usize;
            if s > attr_raw.len() {
                return Err(DataError::InvalidContentOffset { offset: r.content_offset }); }
            let e = s.checked_add(r.content_size as usize)
                .ok_or(DataError::ResidentBufferTooSmall)?;
            if e > attr_raw.len() { return Err(DataError::ResidentBufferTooSmall); }
            DataContent::Resident { bytes: &attr_raw[s..e], size: r.content_size }
        }
        AttributeHeader::NonResident { non_resident: n, .. } => DataContent::NonResident {
            real_size: n.real_size, allocated_size: n.allocated_size,
            starting_vcn: n.starting_vcn, last_vcn: n.last_vcn,
            runlist_offset_in_attr: n.runlist_offset as usize, attribute_raw: attr_raw },
        AttributeHeader::End => return Err(DataError::ResidentBufferTooSmall),
    };
    Ok(DataStream { name, content,
        is_compressed: c.flags & FLAG_COMPRESSED != 0,
        is_encrypted: c.flags & FLAG_ENCRYPTED != 0,
        is_sparse: c.flags & FLAG_SPARSE != 0 })
}
/// MFT エントリ内の全 `$DATA` ストリームを抽出。関連 FR: FR-LIVE-01, FR-REC-01。
pub fn extract_all_data_streams<'a>(
    entry_data: &'a [u8], first_attribute_offset: usize,
) -> Vec<DataStream<'a>> {
    AttributeIterator::new(entry_data, first_attribute_offset).filter_map(Result::ok)
        .filter(|a| a.header.attribute_type() == AttributeType::Data)
        .filter_map(|a| parse_data_stream(a.raw, &a.header).ok()).collect()
}
/// 無名（メイン）`$DATA` ストリームを取り出す。関連 FR: FR-LIVE-01, FR-REC-01, FR-REC-04。
pub fn extract_main_data_stream<'a>(
    entry_data: &'a [u8], first_attribute_offset: usize,
) -> Option<DataStream<'a>> {
    extract_all_data_streams(entry_data, first_attribute_offset).into_iter().find(|s| s.name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::{NonResidentInfo, ResidentInfo};

    fn ch(nl: u8, no: u16, fl: u16, ln: u32, nr: bool) -> AttributeCommonHeader {
        AttributeCommonHeader { attribute_type: AttributeType::Data, length: ln, non_resident: nr,
            name_length: nl, name_offset: no, flags: fl, attribute_id: 0 } }
    fn put_name(r: &mut [u8], off: usize, u: &[u16]) {
        for (i, v) in u.iter().enumerate() {
            r[off + i * 2..off + i * 2 + 2].copy_from_slice(&v.to_le_bytes()); }
    }
    // 常駐 $DATA: name_offset=0x18 → UTF-16LE 名前 → コンテンツ、で配置する。
    fn br(name: &str, content: &[u8], flags: u16) -> (Vec<u8>, AttributeHeader) {
        let u: Vec<u16> = name.encode_utf16().collect();
        let (no, co) = (0x18u16, 0x18u16 + (u.len() * 2) as u16);
        let total = co as usize + content.len(); let mut r = vec![0u8; total];
        r[0..4].copy_from_slice(&AttributeType::Data.to_raw().to_le_bytes());
        r[4..8].copy_from_slice(&(total as u32).to_le_bytes());
        r[0x09] = u.len() as u8; r[0x0A..0x0C].copy_from_slice(&no.to_le_bytes());
        r[0x0C..0x0E].copy_from_slice(&flags.to_le_bytes());
        r[0x10..0x14].copy_from_slice(&(content.len() as u32).to_le_bytes());
        r[0x14..0x16].copy_from_slice(&co.to_le_bytes());
        put_name(&mut r, no as usize, &u); r[co as usize..].copy_from_slice(content);
        (r, AttributeHeader::Resident { common: ch(u.len() as u8, no, flags, total as u32, false),
            resident: ResidentInfo { content_size: content.len() as u32, content_offset: co,
                indexed: false } })
    }
    fn bnr(name: &str, real_size: u64) -> (Vec<u8>, AttributeHeader) {
        let u: Vec<u16> = name.encode_utf16().collect();
        let ro = 0x40u16 + (u.len() * 2) as u16;
        let total = ro as usize + 8; let mut r = vec![0u8; total];
        r[0..4].copy_from_slice(&AttributeType::Data.to_raw().to_le_bytes());
        r[4..8].copy_from_slice(&(total as u32).to_le_bytes());
        r[0x08] = 1; r[0x09] = u.len() as u8;
        r[0x0A..0x0C].copy_from_slice(&0x40u16.to_le_bytes()); put_name(&mut r, 0x40, &u);
        (r, AttributeHeader::NonResident { common: ch(u.len() as u8, 0x40, 0, total as u32, true),
            non_resident: NonResidentInfo { starting_vcn: 0, last_vcn: 7, runlist_offset: ro,
                compression_unit_size: 0, allocated_size: 4096, real_size,
                initialized_size: real_size } })
    }
    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        let mut e: Vec<u8> = parts.iter().flat_map(|p| p.iter().copied()).collect();
        e.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); e.resize(e.len() + 8, 0); e
    }

    #[test] fn resident_data_content_extraction() {
        let (r, h) = br("", b"Hello, World!", 0);
        let s = parse_data_stream(&r, &h).unwrap();
        if let DataContent::Resident { bytes, size } = s.content {
            assert_eq!((bytes, size), (b"Hello, World!".as_slice(), 13));
        } else { panic!("expected resident") }
        assert!(s.name.is_empty());
    }
    #[test] fn empty_unnamed_and_named_decoded() {
        let (r0, h0) = br("", b"", 0); let (r6, h6) = br("", b"abcdef", 0);
        let (rs, hs) = br("secret", b"xyz", 0);
        assert_eq!(parse_data_stream(&r0, &h0).unwrap().content.size(), 0);
        let s6 = parse_data_stream(&r6, &h6).unwrap();
        assert!(s6.name.is_empty()); assert_eq!(s6.content.size(), 6);
        assert_eq!(parse_data_stream(&rs, &hs).unwrap().name, "secret");
    }
    #[test] fn japanese_named_stream_decoded() {
        let (r, h) = br("秘匿データ", b"hidden", 0);
        assert_eq!(parse_data_stream(&r, &h).unwrap().name, "秘匿データ");
    }
    #[test] fn data_content_is_resident_check() {
        let (r1, h1) = br("", b"x", 0); let (r2, h2) = bnr("", 4096);
        assert!(parse_data_stream(&r1, &h1).unwrap().content.is_resident());
        assert!(parse_data_stream(&r2, &h2).unwrap().content.is_non_resident()); }
    #[test] fn non_resident_data_info_extraction() {
        let (r, h) = bnr("", 1_000_000);
        if let DataContent::NonResident { real_size, last_vcn,
            runlist_offset_in_attr, .. } = parse_data_stream(&r, &h).unwrap().content {
            assert_eq!((real_size, last_vcn, runlist_offset_in_attr), (1_000_000, 7, 0x40));
        } else { panic!("expected non-resident") }
    }
    #[test] fn extract_all_and_main_data_streams() {
        let (a, _) = br("", b"main", 0); let (b, _) = br("ads1", b"x", 0);
        let (c, _) = br("ads2", b"yz", 0); let e = cat(&[&a, &b, &c]);
        let s = extract_all_data_streams(&e, 0); assert_eq!(s.len(), 3);
        assert_eq!(s.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            vec!["", "ads1", "ads2"]);
        let m = extract_main_data_stream(&e, 0).unwrap();
        assert!(m.name.is_empty());
        if let DataContent::Resident { bytes, .. } = m.content {
            assert_eq!(bytes, b"main"); } else { panic!() }
    }
    #[test] fn flags_compressed_encrypted_sparse_decoded() {
        let (r, h) = br("", b"x", FLAG_COMPRESSED | FLAG_ENCRYPTED | FLAG_SPARSE);
        let s = parse_data_stream(&r, &h).unwrap();
        assert!(s.is_compressed && s.is_encrypted && s.is_sparse); }
    #[test] fn non_data_attribute_type_rejected() {
        let mut c = ch(0, 0x18, 0, 0x20, false); c.attribute_type = AttributeType::FileName;
        let h = AttributeHeader::Resident { common: c, resident:
            ResidentInfo { content_size: 0, content_offset: 0x18, indexed: false } };
        assert!(parse_data_stream(&[0u8; 0x20], &h).is_err()); }
    // 書籍 318 ページが触れる現実例: Windows がインターネット由来ファイルに付与する
    // "Zone.Identifier" ADS。無名 $DATA + 名前付き ADS の典型ペアを再現する。
    #[test] fn zone_identifier_ads_name_decoded() {
        let (a, _) = br("", b"content", 0);
        let (z, _) = br("Zone.Identifier", b"[ZoneTransfer]\r\nZoneId=3", 0);
        let e = cat(&[&a, &z]); let all = extract_all_data_streams(&e, 0);
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["", "Zone.Identifier"]);
        let m = extract_main_data_stream(&e, 0).unwrap(); assert!(m.name.is_empty());
        if let DataContent::Resident { bytes, .. } = m.content {
            assert_eq!(bytes, b"content"); } else { panic!("expected resident") }
        let ads = all.iter().find(|s| s.name == "Zone.Identifier").unwrap();
        if let DataContent::Resident { bytes, .. } = ads.content {
            assert_eq!(bytes, b"[ZoneTransfer]\r\nZoneId=3");
        } else { panic!("expected resident") } }
    // 書籍 319 ページ Figure 12.4 簡略再現: 無名 + ADS "ADS" 両方暗号化フラグあり。
    #[test] fn book_figure_12_4_dual_encrypted_data_streams() {
        let (u, _) = br("", b"plain-bytes", FLAG_ENCRYPTED);
        let (n, _) = br("ADS", b"ads-bytes", FLAG_ENCRYPTED);
        let e = cat(&[&u, &n]); let all = extract_all_data_streams(&e, 0);
        assert_eq!(all.len(), 2); assert!(all.iter().all(|s| s.is_encrypted));
        assert!(all.iter().any(|s| s.name.is_empty()));
        assert!(all.iter().any(|s| s.name == "ADS"));
        let m = extract_main_data_stream(&e, 0).unwrap();
        assert!(m.name.is_empty() && m.is_encrypted); } }
