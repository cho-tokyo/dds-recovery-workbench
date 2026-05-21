//! `$FILE_NAME` 属性（タイプ 0x30）のコンテンツパーサ。UTF-16LE ファイル名、親ディレクトリ
//! MFT 参照、名前空間、タイムスタンプ、Reparse Value を取得する。関連 FR: FR-LIVE-01,
//! FR-LIVE-05, FR-LIVE-06。仕様: <https://flatcap.github.io/linux-ntfs/ntfs/attributes/file_name.html>
//! 書籍『File System Forensic Analysis』Ch.13 Table 13.7/13.8 準拠（`docs/specs/ntfs-references/notes.md` §9）。
use crate::attribute::{AttributeHeader, AttributeType};
use crate::attributes::standard_information::{FileAttributes, FileTime};
use crate::attributes::AttributeIterator;
use thiserror::Error;
const MIN_SIZE: usize = 66;
const NAME_OFFSET: usize = 0x42;
const ROOT_ENTRY_NUMBER: u64 = 5;

/// `$FILE_NAME` の名前空間。Win32 / Win32AndDos がロング名で表示推奨。関連 FR: FR-LIVE-06。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FileNameNamespace {
    Posix,
    Win32,
    Dos,
    Win32AndDos,
}
impl FileNameNamespace {
    /// 1 バイト生値から変換。未知値は `None`。関連 FR: FR-LIVE-06。
    pub fn from_raw(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Posix),
            1 => Some(Self::Win32),
            2 => Some(Self::Dos),
            3 => Some(Self::Win32AndDos),
            _ => None,
        }
    }
    /// 表示用に適した名前空間か（DOS 以外は true）。関連 FR: FR-LIVE-06。
    pub fn is_preferred_for_display(&self) -> bool {
        !matches!(self, Self::Dos)
    }
}

/// MFT 参照（エントリ番号 48bit + シーケンス番号 16bit）。関連 FR: FR-LIVE-01, FR-LIVE-06。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct MftReference {
    pub entry_number: u64,
    pub sequence_number: u16,
}
impl MftReference {
    /// 8 バイト生値（リトル）から分解。下位 48bit=エントリ番号、上位 16bit=シーケンス。
    pub fn from_raw(raw: u64) -> Self {
        Self {
            entry_number: raw & 0x0000_FFFF_FFFF_FFFF,
            sequence_number: ((raw >> 48) & 0xFFFF) as u16,
        }
    }
    /// ルートディレクトリ（エントリ番号 5）か。関連 FR: FR-LIVE-01。
    pub fn is_root_directory(&self) -> bool {
        self.entry_number == ROOT_ENTRY_NUMBER
    }
}

/// `$FILE_NAME` のコンテンツ。`allocated_size` / `real_size` は作成時スナップショットなので
/// 実サイズは `$DATA` を参照すべき。`reparse_value` は Reparse Point の場合のみ意味を持つ
/// 32bit タグ（書籍 Table 13.7 offset 60-63）。関連 FR: FR-LIVE-01, FR-LIVE-05, FR-LIVE-06。
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct FileName {
    pub parent_directory: MftReference,
    pub created: FileTime,
    pub modified: FileTime,
    pub mft_modified: FileTime,
    pub accessed: FileTime,
    pub allocated_size: u64,
    pub real_size: u64,
    pub file_attributes: FileAttributes,
    pub reparse_value: u32,
    pub namespace: FileNameNamespace,
    pub filename: String,
}

/// `parse_file_name` が返すエラー型。関連 FR: FR-LIVE-01。
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum FileNameError {
    #[error("Buffer too small for $FILE_NAME: got {got}, need at least 66")]
    BufferTooSmall { got: usize },
    #[error(
        "Filename buffer too small: declared {declared} u16 units, have {got} bytes after header"
    )]
    FilenameBufferTooSmall { declared: u8, got: usize },
    #[error("Invalid filename namespace: {got}")]
    InvalidNamespace { got: u8 },
    #[error("Invalid UTF-16 sequence in filename")]
    InvalidUtf16,
}

/// `$FILE_NAME` 属性のコンテンツ部（ヘッダ除外）をパース。UTF-16LE → Rust `String` 変換は
/// サロゲートペア（絵文字等）も自動処理。Reparse Value は offset 0x3C-0x3F から読む。
/// 関連 FR: FR-LIVE-01, FR-LIVE-05, FR-LIVE-06。
pub fn parse_file_name(bytes: &[u8]) -> Result<FileName, FileNameError> {
    if bytes.len() < MIN_SIZE {
        return Err(FileNameError::BufferTooSmall { got: bytes.len() });
    }
    let u32le = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("len 4"));
    let u64le = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().expect("len 8"));
    let filename_len_units = bytes[0x40];
    let namespace_raw = bytes[0x41];
    let namespace = FileNameNamespace::from_raw(namespace_raw)
        .ok_or(FileNameError::InvalidNamespace { got: namespace_raw })?;
    let filename_byte_length = (filename_len_units as usize) * 2;
    if bytes.len() < NAME_OFFSET + filename_byte_length {
        return Err(FileNameError::FilenameBufferTooSmall {
            declared: filename_len_units,
            got: bytes.len() - NAME_OFFSET,
        });
    }
    let filename_bytes = &bytes[NAME_OFFSET..NAME_OFFSET + filename_byte_length];
    let utf16: Vec<u16> = filename_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let filename = String::from_utf16(&utf16).map_err(|_| FileNameError::InvalidUtf16)?;
    Ok(FileName {
        parent_directory: MftReference::from_raw(u64le(0x00)),
        created: FileTime(u64le(0x08)),
        modified: FileTime(u64le(0x10)),
        mft_modified: FileTime(u64le(0x18)),
        accessed: FileTime(u64le(0x20)),
        allocated_size: u64le(0x28),
        real_size: u64le(0x30),
        file_attributes: FileAttributes(u32le(0x38)),
        reparse_value: u32le(0x3C),
        namespace,
        filename,
    })
}

/// MFT エントリ内の全 `$FILE_NAME` 常駐属性をパースして列挙。ハードリンクを持つファイルは
/// 1 エントリに複数の `$FILE_NAME` が並ぶ（書籍 Ch.12 / `notes.md` §9）。パース失敗はスキップ。
/// 関連 FR: FR-LIVE-01, FR-LIVE-06。
pub fn find_all_file_names(entry_data: &[u8], first_attribute_offset: usize) -> Vec<FileName> {
    AttributeIterator::new(entry_data, first_attribute_offset)
        .filter_map(Result::ok)
        .filter(|a| a.header.attribute_type() == AttributeType::FileName)
        .filter_map(|a| {
            if let AttributeHeader::Resident { resident, .. } = &a.header {
                let co = resident.content_offset as usize;
                let ce = co.checked_add(resident.content_size as usize)?;
                if ce <= a.raw.len() {
                    return parse_file_name(&a.raw[co..ce]).ok();
                }
            }
            None
        })
        .collect()
}

/// MFT エントリ内の全 `$FILE_NAME` から表示に最適なものを選ぶ。優先順位:
/// Win32 / Win32AndDos → Posix → Dos。常駐属性のみ対象。関連 FR: FR-LIVE-05, FR-LIVE-06。
pub fn find_best_file_name(entry_data: &[u8], first_attribute_offset: usize) -> Option<FileName> {
    let candidates = find_all_file_names(entry_data, first_attribute_offset);
    candidates
        .iter()
        .find(|f| {
            matches!(
                f.namespace,
                FileNameNamespace::Win32 | FileNameNamespace::Win32AndDos
            )
        })
        .or_else(|| {
            candidates
                .iter()
                .find(|f| f.namespace == FileNameNamespace::Posix)
        })
        .or_else(|| {
            candidates
                .iter()
                .find(|f| f.namespace == FileNameNamespace::Dos)
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn build_file_name_bytes(name: &str, namespace: u8, parent_ref_raw: u64) -> Vec<u8> {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut b = vec![0u8; NAME_OFFSET + utf16.len() * 2];
        b[0x00..0x08].copy_from_slice(&parent_ref_raw.to_le_bytes());
        b[0x40] = utf16.len() as u8;
        b[0x41] = namespace;
        for (i, u) in utf16.iter().enumerate() {
            b[NAME_OFFSET + i * 2..NAME_OFFSET + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b
    }
    /// 常駐 $FILE_NAME 属性 1 件（ヘッダ + コンテンツ、8 バイト整列）。`length=hs+cs` を切上げ。
    fn build_resident_fn_attr(name: &str, namespace: u8, parent: u64) -> Vec<u8> {
        let content = build_file_name_bytes(name, namespace, parent);
        let (cs, hs) = (content.len() as u32, 0x18u32);
        let length = (hs + cs).div_ceil(8) * 8;
        let mut b = vec![0u8; length as usize];
        b[0..4].copy_from_slice(&0x30u32.to_le_bytes());
        b[4..8].copy_from_slice(&length.to_le_bytes());
        b[0x0A..0x0C].copy_from_slice(&0x18u16.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&cs.to_le_bytes());
        b[0x14..0x16].copy_from_slice(&(hs as u16).to_le_bytes());
        b[hs as usize..hs as usize + content.len()].copy_from_slice(&content);
        b
    }
    fn build_entry(attrs: &[Vec<u8>]) -> Vec<u8> {
        let mut d: Vec<u8> = attrs.iter().flatten().copied().collect();
        d.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        d.resize(d.len() + 16, 0);
        d
    }
    #[test]
    fn parses_ascii_filename() {
        let f = parse_file_name(&build_file_name_bytes(
            "hello.txt",
            1,
            0x0001_0000_0000_0005,
        ))
        .unwrap();
        assert_eq!(
            (f.filename.as_str(), f.namespace),
            ("hello.txt", FileNameNamespace::Win32)
        );
        assert_eq!(
            (
                f.parent_directory.entry_number,
                f.parent_directory.sequence_number
            ),
            (5, 1)
        );
        assert!(f.parent_directory.is_root_directory());
    }
    #[test]
    fn parses_japanese_filename() {
        let name = "報告書_山田.docx";
        assert_eq!(
            parse_file_name(&build_file_name_bytes(name, 1, 0))
                .unwrap()
                .filename,
            name
        );
    }
    #[test]
    fn parses_emoji_filename() {
        let name = "📁メモ.txt";
        assert_eq!(
            parse_file_name(&build_file_name_bytes(name, 1, 0))
                .unwrap()
                .filename,
            name
        );
        assert!(
            name.encode_utf16().count() > name.chars().count(),
            "surrogate pair present"
        );
    }
    #[test]
    fn namespace_win32_dos_win32dos_posix() {
        let parse_ns = |ns: u8| {
            parse_file_name(&build_file_name_bytes("a", ns, 0))
                .unwrap()
                .namespace
        };
        assert_eq!(parse_ns(1), FileNameNamespace::Win32);
        assert_eq!(parse_ns(2), FileNameNamespace::Dos);
        assert_eq!(parse_ns(3), FileNameNamespace::Win32AndDos);
        assert_eq!(parse_ns(0), FileNameNamespace::Posix);
        assert!(!FileNameNamespace::Dos.is_preferred_for_display());
    }
    #[test]
    fn invalid_namespace_rejected() {
        let mut b = build_file_name_bytes("x", 0, 0);
        b[0x41] = 4;
        assert!(matches!(
            parse_file_name(&b).unwrap_err(),
            FileNameError::InvalidNamespace { got: 4 }
        ));
    }
    #[test]
    fn buffer_too_small_rejected() {
        assert!(matches!(
            parse_file_name(&[0u8; 65]).unwrap_err(),
            FileNameError::BufferTooSmall { got: 65 }
        ));
    }
    #[test]
    fn filename_buffer_too_small_rejected() {
        let mut b = build_file_name_bytes("abcd", 1, 0);
        b[0x40] = 10;
        assert!(matches!(
            parse_file_name(&b).unwrap_err(),
            FileNameError::FilenameBufferTooSmall { declared: 10, .. }
        ));
    }
    #[test]
    fn mft_reference_bit_decomposition() {
        let r = MftReference::from_raw(0x000A_0000_0000_002A);
        assert_eq!((r.entry_number, r.sequence_number), (42, 10));
        assert!(!r.is_root_directory());
        assert!(MftReference::from_raw(5).is_root_directory());
    }
    #[test]
    fn is_preferred_for_display_truth_table() {
        use FileNameNamespace::*;
        assert!(Posix.is_preferred_for_display() && Win32.is_preferred_for_display());
        assert!(Win32AndDos.is_preferred_for_display() && !Dos.is_preferred_for_display());
    }
    /// 書籍 Chapter 13 例題: $MFT 自身の $FILE_NAME 属性を再現。
    /// parent=entry 5/seq 5、allocated=real=0x4000、namespace=Win32&DOS、name="$MFT"。
    #[test]
    fn book_example_mft_self_file_name() {
        let mut b = build_file_name_bytes("$MFT", 3, 0x0005_0000_0000_0005);
        b[0x28..0x30].copy_from_slice(&0x4000u64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&0x4000u64.to_le_bytes());
        let f = parse_file_name(&b).unwrap();
        assert!(f.parent_directory.is_root_directory());
        assert_eq!(
            (
                f.parent_directory.sequence_number,
                f.filename.as_str(),
                f.namespace
            ),
            (5, "$MFT", FileNameNamespace::Win32AndDos)
        );
        assert_eq!((f.allocated_size, f.real_size), (0x4000, 0x4000));
    }
    /// 書籍 Chapter 13 例題: Win32 と DOS の二重登録（短縮名併記）パターン。
    /// `find_all_file_names` が 2 件返し、`find_best_file_name` が Win32 を選ぶことを検証。
    #[test]
    fn book_example_dual_filename_win32_and_dos() {
        let entry = build_entry(&[
            build_resident_fn_attr("57398408d01", 1, 0x0001_0000_0000_0005),
            build_resident_fn_attr("573984~1", 2, 0x0001_0000_0000_0005),
        ]);
        let all = find_all_file_names(&entry, 0);
        assert_eq!(all.len(), 2);
        let names: Vec<&str> = all.iter().map(|f| f.filename.as_str()).collect();
        assert!(names.contains(&"57398408d01") && names.contains(&"573984~1"));
        let best = find_best_file_name(&entry, 0).expect("best");
        assert_eq!(
            (best.filename.as_str(), best.namespace),
            ("57398408d01", FileNameNamespace::Win32)
        );
    }
    /// ハードリンクで同一ベースに複数 $FILE_NAME を持つエントリを模擬し、3 件取得を確認。
    #[test]
    fn find_all_file_names_returns_multiple_hardlinks() {
        let entry = build_entry(&[
            build_resident_fn_attr("primary.txt", 1, 0x0001_0000_0000_0005),
            build_resident_fn_attr("alias_one.txt", 1, 0x0001_0000_0000_0005),
            build_resident_fn_attr("alias_two.txt", 1, 0x0001_0000_0000_0006),
        ]);
        let all = find_all_file_names(&entry, 0);
        let names: Vec<&str> = all.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(all.len(), 3);
        for n in ["primary.txt", "alias_one.txt", "alias_two.txt"] {
            assert!(names.contains(&n), "missing: {n}");
        }
    }
    /// 書籍 Table 13.7 offset 60-63（Reparse Value）。Mount Point タグ 0xA0000003 と
    /// 通常ファイルの 0 をそれぞれ確認。
    #[test]
    fn reparse_value_field_is_parsed() {
        let mut b = build_file_name_bytes("mount_point", 1, 0);
        b[0x3C..0x40].copy_from_slice(&0xA000_0003u32.to_le_bytes());
        assert_eq!(parse_file_name(&b).unwrap().reparse_value, 0xA000_0003);
        assert_eq!(
            parse_file_name(&build_file_name_bytes("p.txt", 1, 0))
                .unwrap()
                .reparse_value,
            0
        );
    }
}
