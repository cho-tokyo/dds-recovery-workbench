//! 属性巡回モジュール: MFT エントリ内の属性を順次取り出すイテレータと検索ヘルパ。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-05（削除エントリ可視化）、FR-LIVE-06（メタデータ表示）。
pub mod file_name;
pub mod standard_information;
pub use file_name::{
    find_best_file_name, parse_file_name, FileName, FileNameError, FileNameNamespace,
    MftReference,
};
pub use standard_information::{
    parse_standard_information, FileAttributes, FileTime, SiError, StandardInformation,
};
use crate::attribute::{parse_attribute_header, AttributeError, AttributeHeader, AttributeType};

/// MFT エントリ内の単一属性への参照。`raw` はヘッダ含む全バイトで、常駐属性のコンテンツは
/// `ResidentInfo::content_offset` から `content_size` バイトを取り出す。関連 FR: FR-LIVE-01, FR-LIVE-06。
#[allow(missing_docs)] pub struct AttributeRef<'a> {
    pub header: AttributeHeader, pub raw: &'a [u8], pub offset_in_entry: usize,
}
/// MFT エントリの属性イテレータ。`AttributeHeader::End` で停止し、パースエラー発生時は
/// そのエラーを yield して以後 `None` を返す。関連 FR: FR-LIVE-01。
pub struct AttributeIterator<'a> { entry_data: &'a [u8], cursor: usize, done: bool }
impl<'a> AttributeIterator<'a> {
    /// MFT エントリのデータと最初の属性オフセットからイテレータを構築する。
    pub fn new(entry_data: &'a [u8], first_attribute_offset: usize) -> Self {
        Self { entry_data, cursor: first_attribute_offset, done: false } } }
impl<'a> Iterator for AttributeIterator<'a> {
    type Item = Result<AttributeRef<'a>, AttributeError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.cursor >= self.entry_data.len() { return None; }
        match parse_attribute_header(&self.entry_data[self.cursor..]) {
            Ok(AttributeHeader::End) => { self.done = true; None }
            Ok(header) => {
                let length = header.length() as usize;
                if length == 0 || self.cursor + length > self.entry_data.len() {
                    self.done = true;
                    return Some(Err(AttributeError::InvalidLength { length: length as u32 }));
                }
                let raw = &self.entry_data[self.cursor..self.cursor + length];
                let r = AttributeRef { header, raw, offset_in_entry: self.cursor };
                self.cursor += length; Some(Ok(r))
            }
            Err(e) => { self.done = true; Some(Err(e)) } } } }
/// 指定タイプの属性を最初に見つけて返す線形探索ヘルパ。関連 FR: FR-LIVE-01, FR-LIVE-06。
pub fn find_attribute<'a>(
    entry_data: &'a [u8], first_attribute_offset: usize, target_type: AttributeType,
) -> Option<AttributeRef<'a>> {
    AttributeIterator::new(entry_data, first_attribute_offset)
        .filter_map(Result::ok).find(|a| a.header.attribute_type() == target_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    // 常駐属性 1 件 (length バイト)、type_id/length をリトルエンディアンで埋める。
    fn resident(type_id: u32, length: u32) -> Vec<u8> {
        let mut b = vec![0u8; length as usize];
        b[0..4].copy_from_slice(&type_id.to_le_bytes());
        b[4..8].copy_from_slice(&length.to_le_bytes());
        b[0x0A..0x0C].copy_from_slice(&0x18u16.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&(length - 0x18).to_le_bytes());
        b[0x14..0x16].copy_from_slice(&0x18u16.to_le_bytes()); b
    }
    fn entry(attrs: &[Vec<u8>]) -> Vec<u8> { // 属性連結 + End マーカー + パディング
        let mut d: Vec<u8> = attrs.iter().flatten().copied().collect();
        d.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        d.resize(d.len() + 16, 0); d
    }
    #[test] fn iterator_empty_on_end_marker() {
        let d = entry(&[]); let mut it = AttributeIterator::new(&d, 0);
        assert!(it.next().is_none()); assert!(it.next().is_none());
    }
    #[test] fn iterator_yields_single_attribute_then_end() {
        let d = entry(&[resident(0x10, 0x60)]); let mut it = AttributeIterator::new(&d, 0);
        let a = it.next().expect("first").expect("ok");
        assert_eq!(a.header.attribute_type(), AttributeType::StandardInformation);
        assert_eq!((a.offset_in_entry, a.raw.len()), (0, 0x60));
        assert!(it.next().is_none());
    }
    #[test] fn iterator_yields_multiple_attributes() {
        let d = entry(&[resident(0x10, 0x60), resident(0x30, 0x70)]);
        let types: Vec<_> = AttributeIterator::new(&d, 0)
            .filter_map(Result::ok).map(|a| a.header.attribute_type()).collect();
        assert_eq!(types, vec![AttributeType::StandardInformation, AttributeType::FileName]);
    }
    #[test] fn find_attribute_finds_existing_type() {
        let d = entry(&[resident(0x10, 0x60), resident(0x30, 0x70)]);
        let f = find_attribute(&d, 0, AttributeType::FileName).expect("found");
        assert_eq!((f.header.attribute_type(), f.offset_in_entry),
            (AttributeType::FileName, 0x60));
    }
    #[test] fn find_attribute_returns_none_for_missing_type() {
        let d = entry(&[resident(0x10, 0x60)]);
        assert!(find_attribute(&d, 0, AttributeType::Data).is_none());
    }
}
