//! NTFS `$INDEX_ROOT` (0x90) と `$INDEX_ALLOCATION` (0xA0) 配下の INDX ブロック解析。
//! ディレクトリは B+ ツリー構造でファイル名を保持し、各ノードはエントリ列（子 MFT 参照 +
//! `$FILE_NAME` 情報）を持つ。本モジュールは「単一ノード内のエントリ列挙」までを担当し、
//! B+ ツリー走査と `NtfsVolume` 統合は Chunk 13 で実装する。
//! 書籍『File System Forensic Analysis』Ch.12「INDEXES」/ Ch.13 Table 13.13-13.17 準拠。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-LIVE-04（ファイルツリー）。
use crate::attributes::file_name::{parse_file_name, FileName, FileNameError, MftReference};
use crate::fixup::{apply_fixup, FixupError};
use thiserror::Error;

const STD_HDR: usize = 16;
const NODE_HDR: usize = 16;
const INDX_HDR: usize = 0x18;
const INDX_PREFIX: usize = INDX_HDR + NODE_HDR;
const ENTRY_HDR: usize = 16;
const FN_TYPE: u32 = 0x30;
const F_CHILD: u32 = 0x01;
const F_LAST: u32 = 0x02;
const MAGIC: &[u8; 4] = b"INDX";

/// インデックス解析エラー。`FileNameError` / `FixupError` を `#[from]` で集約。関連 FR: FR-LIVE-04。
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum IndexError {
    #[error("Buffer too small for index structure: got {got}, need at least {need}")]
    BufferTooSmall { got: usize, need: usize },
    #[error("Invalid INDX magic: expected 'INDX', got {got:?}")]
    InvalidIndxMagic { got: [u8; 4] },
    #[error("Invalid index entry type: expected 0x30 ($FILE_NAME), got 0x{got:08X}")]
    UnsupportedIndexType { got: u32 },
    #[error("Entry length zero (infinite loop protection)")]
    EntryLengthZero,
    #[error("Entry length exceeds node body: length={length}, remaining={remaining}")]
    EntryLengthExceedsBuffer { length: u16, remaining: usize },
    #[error("File name parse error: {0}")]
    FileName(#[from] FileNameError),
    #[error("Fixup error in INDX block: {0}")]
    Fixup(#[from] FixupError),
}

/// Index Node Header（$INDEX_ROOT / INDX 共通 16 バイト）。関連 FR: FR-LIVE-04。
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct IndexNodeHeader {
    pub first_entry_offset: u32, pub end_of_entries_offset: u32,
    pub end_of_buffer_offset: u32,
    /// bit 0: 子ノード（`$INDEX_ALLOCATION` 内 INDX）を持つ内部ノードフラグ。
    pub flags: u8,
}
impl IndexNodeHeader {
    /// 子ノード（INDX ブロック）を持つ内部ノードなら true。関連 FR: FR-LIVE-04。
    pub fn has_children(&self) -> bool { self.flags & 0x01 != 0 }
}

/// `$INDEX_ROOT` のコンテンツ解析結果。`node_body` はエントリ列先頭への参照。関連 FR: FR-LIVE-04。
#[derive(Debug)]
#[allow(missing_docs)]
pub struct IndexRoot<'a> {
    pub index_type: u32, pub collation_rule: u32,
    pub bytes_per_index_record: u32, pub clusters_per_index_record: i8,
    pub node_header: IndexNodeHeader, pub node_body: &'a [u8],
}

/// `$INDEX_ALLOCATION` 内の 1 INDX ブロック（フィクサップ適用済み）。関連 FR: FR-LIVE-04。
#[derive(Debug)]
#[allow(missing_docs)]
pub struct IndxBlock {
    pub vcn: u64, pub node_header: IndexNodeHeader,
    /// フィクサップ適用済みのブロック全データ。
    pub data: Vec<u8>,
    /// `data` 内での Index Node Header 開始オフセット（固定 0x18）。
    pub node_header_offset: usize,
}
impl IndxBlock {
    /// Index Node Header 直後のエントリ列。`parse_entries_in_node` に渡せる形。
    pub fn node_body(&self) -> &[u8] { &self.data[self.node_header_offset + NODE_HDR..] }
}

/// 単一インデックスエントリ。終端 (is_last) は `file_name == None`。関連 FR: FR-LIVE-04。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct IndexEntry {
    pub child_ref: MftReference, pub entry_length: u16, pub flags: u32,
    /// 通常エントリの `$FILE_NAME`。終端では `None`。
    pub file_name: Option<FileName>,
    /// 子 INDX ブロックの VCN。`has_child_node()` 時のみ `Some`。
    pub child_vcn: Option<u64>,
}
impl IndexEntry {
    /// 終端エントリ（ファイル名なし、B+ ツリーのナビゲーション用）。
    pub fn is_last(&self) -> bool { self.flags & F_LAST != 0 }
    /// 子ノード（INDX ブロック）への参照を持つか。
    pub fn has_child_node(&self) -> bool { self.flags & F_CHILD != 0 }
}

// 内部ヘルパ: Index Node Header（16 バイト）をパース。
fn parse_index_node_header(bytes: &[u8]) -> Result<IndexNodeHeader, IndexError> {
    if bytes.len() < NODE_HDR {
        return Err(IndexError::BufferTooSmall { got: bytes.len(), need: NODE_HDR });
    }
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    Ok(IndexNodeHeader {
        first_entry_offset: u32le(0), end_of_entries_offset: u32le(4),
        end_of_buffer_offset: u32le(8), flags: bytes[12],
    })
}

/// `$INDEX_ROOT` 属性のコンテンツ部を解析。書籍 Ch.13 Table 13.13/13.14 準拠。
/// `index_type` が 0x30（`$FILE_NAME` インデックス）以外は `UnsupportedIndexType`。
/// 関連 FR: FR-LIVE-04。
pub fn parse_index_root(bytes: &[u8]) -> Result<IndexRoot<'_>, IndexError> {
    let need = STD_HDR + NODE_HDR;
    if bytes.len() < need { return Err(IndexError::BufferTooSmall { got: bytes.len(), need }); }
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    let index_type = u32le(0);
    if index_type != FN_TYPE { return Err(IndexError::UnsupportedIndexType { got: index_type }); }
    let node_header = parse_index_node_header(&bytes[STD_HDR..need])?;
    Ok(IndexRoot {
        index_type, collation_rule: u32le(4),
        bytes_per_index_record: u32le(8), clusters_per_index_record: bytes[12] as i8,
        node_header, node_body: &bytes[need..],
    })
}

/// INDX ブロック全体を解析（フィクサップ適用込み）。書籍 Ch.13 Table 13.15 準拠。
/// 入力は `$INDEX_ALLOCATION` の runlist から読んだ 1 ブロック分（典型 4096 B）。
/// 関連 FR: FR-LIVE-04。
pub fn parse_indx_block(bytes: &[u8], sector_size: u16) -> Result<IndxBlock, IndexError> {
    if bytes.len() < INDX_PREFIX {
        return Err(IndexError::BufferTooSmall { got: bytes.len(), need: INDX_PREFIX });
    }
    let magic: [u8; 4] = bytes[0..4].try_into().expect("len 4");
    if &magic != MAGIC { return Err(IndexError::InvalidIndxMagic { got: magic }); }
    let usa_offset = u16::from_le_bytes([bytes[4], bytes[5]]);
    let usa_size = u16::from_le_bytes([bytes[6], bytes[7]]);
    let vcn = u64::from_le_bytes(bytes[0x10..0x18].try_into().expect("len 8"));
    let mut data = bytes.to_vec();
    apply_fixup(&mut data, usa_offset, usa_size, sector_size)?;
    let node_header = parse_index_node_header(&data[INDX_HDR..INDX_PREFIX])?;
    Ok(IndxBlock { vcn, node_header, data, node_header_offset: INDX_HDR })
}

/// 単一ノード内のエントリ列を順次解析。終端エントリ (`flags & 0x02`) を含めて Vec に push し、
/// その時点で停止。終端以降のバイトは無視。B+ ツリー走査は Chunk 13 で統合。関連 FR: FR-LIVE-04。
pub fn parse_entries_in_node(node_body: &[u8]) -> Result<Vec<IndexEntry>, IndexError> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    loop {
        if cursor + ENTRY_HDR > node_body.len() {
            return Err(IndexError::BufferTooSmall {
                got: node_body.len() - cursor, need: ENTRY_HDR });
        }
        let child_raw = u64::from_le_bytes(node_body[cursor..cursor + 8].try_into().expect("len 8"));
        let elen_u16 = u16::from_le_bytes([node_body[cursor + 8], node_body[cursor + 9]]);
        let fn_len = u16::from_le_bytes([node_body[cursor + 10], node_body[cursor + 11]]);
        let flags = u32::from_le_bytes(node_body[cursor + 12..cursor + 16].try_into().expect("len 4"));
        if elen_u16 == 0 { return Err(IndexError::EntryLengthZero); }
        let elen = elen_u16 as usize;
        if cursor + elen > node_body.len() {
            return Err(IndexError::EntryLengthExceedsBuffer {
                length: elen_u16, remaining: node_body.len() - cursor });
        }
        let is_last = flags & F_LAST != 0;
        let has_child = flags & F_CHILD != 0;
        let file_name = if !is_last && fn_len > 0 {
            let (s, e) = (cursor + ENTRY_HDR, cursor + ENTRY_HDR + fn_len as usize);
            if e > cursor + elen {
                return Err(IndexError::EntryLengthExceedsBuffer {
                    length: fn_len, remaining: elen - ENTRY_HDR });
            }
            Some(parse_file_name(&node_body[s..e])?)
        } else { None };
        let child_vcn = if has_child && elen >= 8 {
            let vo = cursor + elen - 8;
            Some(u64::from_le_bytes(node_body[vo..vo + 8].try_into().expect("len 8")))
        } else { None };
        entries.push(IndexEntry {
            child_ref: MftReference::from_raw(child_raw),
            entry_length: elen_u16, flags, file_name, child_vcn,
        });
        cursor += elen;
        if is_last { break; }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put16(b: &mut [u8], o: usize, v: u16) { b[o..o + 2].copy_from_slice(&v.to_le_bytes()); }
    fn put32(b: &mut [u8], o: usize, v: u32) { b[o..o + 4].copy_from_slice(&v.to_le_bytes()); }
    fn fn_content(name: &str) -> Vec<u8> {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut b = vec![0u8; 0x42 + utf16.len() * 2];
        b[0..8].copy_from_slice(&(5u64 | (1u64 << 48)).to_le_bytes()); // parent=5
        b[0x40] = utf16.len() as u8; b[0x41] = 1; // Win32
        for (i, u) in utf16.iter().enumerate() {
            b[0x42 + i * 2..0x44 + i * 2].copy_from_slice(&u.to_le_bytes());
        } b
    }
    fn build_entries(items: &[(u64, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        for &(no, name) in items {
            let fnc = fn_content(name);
            let elen = ((ENTRY_HDR + fnc.len() + 7) & !7) as u16;
            let mut e = vec![0u8; elen as usize];
            e[0..8].copy_from_slice(&(no | (1u64 << 48)).to_le_bytes());
            put16(&mut e, 8, elen); put16(&mut e, 10, fnc.len() as u16);
            e[ENTRY_HDR..ENTRY_HDR + fnc.len()].copy_from_slice(&fnc);
            buf.extend_from_slice(&e);
        }
        let mut t = vec![0u8; 16]; put16(&mut t, 8, 16);
        put32(&mut t, 12, F_LAST); buf.extend_from_slice(&t); buf
    }
    fn build_index_root(items: &[(u64, &str)]) -> Vec<u8> {
        let entries = build_entries(items);
        let mut buf = vec![0u8; STD_HDR + NODE_HDR + entries.len()];
        put32(&mut buf, 0, FN_TYPE); put32(&mut buf, 8, 4096); buf[12] = 1;
        let eo = (NODE_HDR + entries.len()) as u32;
        put32(&mut buf, STD_HDR, 16);
        put32(&mut buf, STD_HDR + 4, eo); put32(&mut buf, STD_HDR + 8, eo);
        buf[STD_HDR + NODE_HDR..].copy_from_slice(&entries); buf
    }
    // 4096 B INDX ブロック合成。USA をエントリ列の後ろに配置（8 セクタ）。
    fn build_indx(vcn: u64, items: &[(u64, &str)]) -> Vec<u8> {
        const USN: u16 = 0x55AA;
        let entries = build_entries(items);
        let mut buf = vec![0u8; 4096];
        buf[0..4].copy_from_slice(MAGIC);
        put16(&mut buf, 6, 9); // usa_size = USN + 8 fixup
        buf[0x10..0x18].copy_from_slice(&vcn.to_le_bytes());
        let eo = (NODE_HDR + entries.len()) as u32;
        put32(&mut buf, INDX_HDR, 16);
        put32(&mut buf, INDX_HDR + 4, eo); put32(&mut buf, INDX_HDR + 8, eo);
        buf[INDX_PREFIX..INDX_PREFIX + entries.len()].copy_from_slice(&entries);
        let usa_off = (INDX_PREFIX + entries.len()).next_multiple_of(8);
        put16(&mut buf, 4, usa_off as u16);
        put16(&mut buf, usa_off, USN);
        for i in 0..8 {
            put16(&mut buf, usa_off + 2 + i * 2, 0xBB00 | (i as u16));
            put16(&mut buf, 512 * (i + 1) - 2, USN);
        } buf
    }

    #[test] fn parse_index_root_minimal_valid_directory() {
        let buf = build_index_root(&[(64, "a.txt"), (65, "b.txt")]);
        let ir = parse_index_root(&buf).expect("ok");
        assert_eq!((ir.index_type, ir.bytes_per_index_record), (0x30, 4096));
        assert!(!ir.node_header.has_children());
        let entries = parse_entries_in_node(ir.node_body).expect("ok");
        assert_eq!(entries.len(), 3);
        assert!(entries.last().unwrap().is_last() && entries.last().unwrap().file_name.is_none());
    }
    #[test] fn parse_index_root_rejects_non_filename_type() {
        let mut buf = build_index_root(&[(64, "x")]);
        put32(&mut buf, 0, 0x90);
        assert!(matches!(parse_index_root(&buf).unwrap_err(),
            IndexError::UnsupportedIndexType { got: 0x90 }));
    }
    #[test] fn parse_index_root_buffer_too_small() {
        assert!(matches!(parse_index_root(&[0u8; 31]).unwrap_err(),
            IndexError::BufferTooSmall { got: 31, need: 32 }));
    }
    #[test] fn parse_indx_block_with_valid_magic_and_fixup() {
        let blk = build_indx(2, &[(64, "p.txt"), (65, "q.txt")]);
        let ib = parse_indx_block(&blk, 512).expect("ok");
        assert_eq!(ib.vcn, 2);
        let last = ib.data.len() - 2;
        assert_eq!(&ib.data[last..], &0xBB07u16.to_le_bytes());
        let entries = parse_entries_in_node(ib.node_body()).expect("ok");
        assert!(entries.last().unwrap().is_last());
    }
    #[test] fn parse_indx_block_rejects_invalid_magic() {
        let mut blk = build_indx(0, &[(64, "x")]);
        blk[0..4].copy_from_slice(b"XXXX");
        assert!(matches!(parse_indx_block(&blk, 512).unwrap_err(),
            IndexError::InvalidIndxMagic { got: b } if &b == b"XXXX"));
    }
    #[test] fn parse_indx_block_fixup_mismatch_propagates() {
        let mut blk = build_indx(0, &[(64, "x")]);
        let last = blk.len() - 2;
        blk[last..].copy_from_slice(&0xDEADu16.to_le_bytes());
        assert!(matches!(parse_indx_block(&blk, 512).unwrap_err(),
            IndexError::Fixup(FixupError::FixupMismatch { sector: 7, .. })));
    }
    #[test] fn parse_entries_single_terminal_entry() {
        let mut t = vec![0u8; 16]; put16(&mut t, 8, 16); put32(&mut t, 12, F_LAST);
        let entries = parse_entries_in_node(&t).expect("ok");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_last() && entries[0].file_name.is_none());
    }
    #[test] fn parse_entries_multiple_with_filenames() {
        let buf = build_entries(&[(10, "a.txt"), (11, "b.txt"), (12, "c.txt")]);
        let entries = parse_entries_in_node(&buf).expect("ok");
        assert_eq!(entries.len(), 4);
        let names: Vec<_> = entries.iter()
            .filter_map(|e| e.file_name.as_ref().map(|f| f.filename.as_str())).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
        assert_eq!(entries[0].child_ref.entry_number, 10);
    }
    #[test] fn parse_entries_zero_length_returns_error() {
        assert!(matches!(parse_entries_in_node(&[0u8; 16]).unwrap_err(),
            IndexError::EntryLengthZero));
    }
    #[test] fn parse_entries_length_exceeds_buffer_returns_error() {
        let mut buf = vec![0u8; 16]; put16(&mut buf, 8, 64);
        assert!(matches!(parse_entries_in_node(&buf).unwrap_err(),
            IndexError::EntryLengthExceedsBuffer { length: 64, .. }));
    }
    #[test] fn parse_entries_with_child_node_vcn_extracted() {
        let fnc = fn_content("x");
        let elen = (ENTRY_HDR + fnc.len() + 8).next_multiple_of(8) as u16;
        let mut e = vec![0u8; elen as usize];
        e[0..8].copy_from_slice(&(100u64 | (1u64 << 48)).to_le_bytes());
        put16(&mut e, 8, elen); put16(&mut e, 10, fnc.len() as u16);
        put32(&mut e, 12, F_CHILD);
        e[ENTRY_HDR..ENTRY_HDR + fnc.len()].copy_from_slice(&fnc);
        let voff = elen as usize - 8;
        e[voff..voff + 8].copy_from_slice(&0x42u64.to_le_bytes());
        let mut t = vec![0u8; 16]; put16(&mut t, 8, 16); put32(&mut t, 12, F_LAST);
        e.extend_from_slice(&t);
        let entries = parse_entries_in_node(&e).expect("ok");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].has_child_node() && entries[0].child_vcn == Some(0x42));
    }
}
