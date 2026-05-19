//! 結合テスト: 実 NTFS フィクスチャの MFT エントリ 0 から属性連を巡回する。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）, FR-LIVE-06（メタデータ表示）。
mod common;
use dds_fs_ntfs::{parse_attribute_header, parse_boot_sector, parse_mft_entry, AttributeType};

fn collect_attribute_types_for_record(img: &[u8], record_index: usize) -> Vec<AttributeType> {
    let bs = parse_boot_sector(&img[..512]).expect("boot");
    let mft_off = bs.mft_byte_offset() as usize;
    let rec_size = bs.mft_record_size_bytes() as usize;
    let entry_bytes = &img[mft_off + record_index * rec_size..mft_off + (record_index + 1) * rec_size];
    let entry = parse_mft_entry(entry_bytes).expect("mft");
    let mut types = Vec::new();
    let mut cursor = entry.header.first_attribute_offset as usize;
    loop {
        if cursor >= entry.data.len() {
            break;
        }
        let hdr = parse_attribute_header(&entry.data[cursor..]).expect("attr");
        if hdr.is_end() {
            types.push(AttributeType::End);
            break;
        }
        types.push(hdr.attribute_type());
        let len = hdr.length() as usize;
        if len == 0 {
            break;
        }
        cursor += len;
    }
    types
}

#[test]
fn iterates_attributes_of_mft_record_zero() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let types = collect_attribute_types_for_record(&img, 0);
    assert!(
        types.contains(&AttributeType::StandardInformation),
        "missing $STANDARD_INFORMATION, got: {:?}",
        types
    );
    assert!(
        types.contains(&AttributeType::FileName),
        "missing $FILE_NAME, got: {:?}",
        types
    );
    assert!(
        types.contains(&AttributeType::Data),
        "missing $DATA, got: {:?}",
        types
    );
    assert!(
        types.last() == Some(&AttributeType::End),
        "list should end with End marker, got: {:?}",
        types
    );
}

#[test]
fn attributes_are_in_ascending_type_id_order() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let types = collect_attribute_types_for_record(&img, 0);
    let raws: Vec<u32> = types
        .iter()
        .filter(|t| !matches!(t, AttributeType::End))
        .map(|t| t.to_raw())
        .collect();
    for w in raws.windows(2) {
        assert!(w[0] < w[1], "attribute order not ascending: {:?}", raws);
    }
}
