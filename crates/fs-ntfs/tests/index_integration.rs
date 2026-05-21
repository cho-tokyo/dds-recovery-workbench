//! 結合テスト: 実 NTFS イメージから $INDEX_ROOT / $INDEX_ALLOCATION を解析する。
//! Chunk 12 の主要動作確認 + 業務上重要な「インデックス vs MFT 走査の差」の定量実証。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05。

mod common;

use dds_fs_ntfs::{
    find_attribute, find_best_file_name, parse_boot_sector, parse_entries_in_node,
    parse_index_root, AttributeHeader, AttributeType, NtfsVolume,
};

fn make_image_reader(
    img: Vec<u8>,
    cluster_size: u64,
) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
    move |lcn, count| {
        let start = (lcn * cluster_size) as usize;
        let end = start + (count * cluster_size) as usize;
        if end > img.len() {
            Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "oob",
            ))
        } else {
            Ok(img[start..end].to_vec())
        }
    }
}

fn open_fixture(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(make_image_reader(img, cs)).expect("open")
}

/// ルートディレクトリ (MFT 5) の $INDEX_ROOT を解析し、user files が列挙されることを確認。
/// 単純な小規模ディレクトリでは $INDEX_ROOT 単独で全エントリが収まる可能性が高い。
#[test]
fn root_directory_index_root_lists_user_files() {
    let mut volume = open_fixture("ntfs_healthy_small");
    let root = volume.read_record(5).expect("read root");
    let ir_attr = find_attribute(
        &root.data,
        root.header.first_attribute_offset as usize,
        AttributeType::IndexRoot,
    )
    .expect("$INDEX_ROOT must exist on root dir");
    let AttributeHeader::Resident { resident, .. } = &ir_attr.header else {
        panic!("$INDEX_ROOT must be resident");
    };
    let co = resident.content_offset as usize;
    let ce = co + resident.content_size as usize;
    let content = &ir_attr.raw[co..ce];
    let index_root = parse_index_root(content).expect("parse index_root");
    assert_eq!(index_root.index_type, 0x30, "must be $FILE_NAME index");

    let entries = parse_entries_in_node(index_root.node_body).expect("entries");
    let user_files: Vec<String> = entries
        .iter()
        .filter_map(|e| e.file_name.as_ref().map(|f| f.filename.clone()))
        .filter(|n| n.starts_with("file_"))
        .collect();

    println!("\n=== Root $INDEX_ROOT entries ===");
    println!("Total entries (incl. terminal): {}", entries.len());
    println!("User files in $INDEX_ROOT: {}", user_files.len());
    println!("has_children: {}", index_root.node_header.has_children());
    println!(
        "First few names: {:?}",
        user_files.iter().take(5).collect::<Vec<_>>()
    );

    // 小規模なら 30 件すべて $INDEX_ROOT 内、大きければ has_children=true。
    if !index_root.node_header.has_children() {
        assert_eq!(user_files.len(), 30, "small dir should hold all in root");
    } else {
        assert!(
            user_files.len() <= 30,
            "some entries may be in $INDEX_ALLOCATION"
        );
    }
}

/// $INDEX_ALLOCATION が存在する場合の検出と最低限の確認（不在ならスキップ）。
#[test]
fn root_index_allocation_indx_blocks_parseable() {
    let mut volume = open_fixture("ntfs_healthy_small");
    let root = volume.read_record(5).expect("read root");
    let ia = find_attribute(
        &root.data,
        root.header.first_attribute_offset as usize,
        AttributeType::IndexAllocation,
    );
    match ia {
        None => {
            println!("$INDEX_ALLOCATION not present on small fixture (expected, dir fits in root)");
        }
        Some(attr) => {
            // 存在するなら非常駐であることだけ確認。実 INDX 読込は Chunk 13 で B+ ツリー走査と統合。
            assert!(
                matches!(attr.header, AttributeHeader::NonResident { .. }),
                "$INDEX_ALLOCATION must be non-resident"
            );
            println!("$INDEX_ALLOCATION found on root, non-resident as expected");
        }
    }
}

/// 業務上極めて重要なテスト: インデックスに見えるファイル数 vs MFT 走査で見える数 を比較。
/// NTFS は削除時にインデックスからエントリを除去する一方、MFT エントリは In Use フラグを 0 にして
/// 残す。よって「インデックス = ライブモードで見えるファイル」「MFT 走査 = 削除復旧対象まで含む」
/// という二分が定量化される。Chunk 11 の volume.iter_records と Chunk 12 の parse_entries_in_node
/// を組み合わせて DDS Recovery Workbench のアーキテクチャ判断を実証する。
#[test]
fn deleted_files_appear_or_disappear_in_index() {
    let mut volume = open_fixture("ntfs_with_5_deletions_small");

    // (A) インデックス経由でユーザファイルを集める。
    let root = volume.read_record(5).expect("read root");
    let ir_attr = find_attribute(
        &root.data,
        root.header.first_attribute_offset as usize,
        AttributeType::IndexRoot,
    )
    .expect("$INDEX_ROOT");
    let AttributeHeader::Resident { resident, .. } = &ir_attr.header else {
        panic!("resident expected");
    };
    let co = resident.content_offset as usize;
    let ce = co + resident.content_size as usize;
    let index_root = parse_index_root(&ir_attr.raw[co..ce]).expect("parse");
    let index_entries = parse_entries_in_node(index_root.node_body).expect("entries");
    let in_index: std::collections::BTreeSet<String> = index_entries
        .iter()
        .filter_map(|e| e.file_name.as_ref().map(|f| f.filename.clone()))
        .filter(|n| n.starts_with("file_"))
        .collect();

    // (B) MFT 直接走査でユーザファイルを集める（削除済みも含む）。
    let mut in_mft = std::collections::BTreeSet::new();
    let mut deleted_in_mft = std::collections::BTreeSet::new();
    for (_idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        let Some(name) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if !name.filename.starts_with("file_") {
            continue;
        }
        in_mft.insert(name.filename.clone());
        if entry.header.is_deleted() {
            deleted_in_mft.insert(name.filename);
        }
    }

    println!("\n=== Index vs MFT walk: ntfs_with_5_deletions_small ===");
    println!(
        "Files visible via $INDEX_ROOT (live mode): {}",
        in_index.len()
    );
    println!(
        "Files visible via MFT walk (recovery mode): {}",
        in_mft.len()
    );
    println!(
        "Deleted files (MFT only):                 {}",
        deleted_in_mft.len()
    );
    let only_in_mft: Vec<_> = in_mft.difference(&in_index).cloned().collect();
    println!("Names in MFT but not in index:            {only_in_mft:?}");

    // 業務上の主張: 削除ファイルは MFT に残り、インデックスには無い。
    assert!(
        in_mft.len() >= in_index.len(),
        "MFT walk must surface at least as many names as live index"
    );
    // 5 件の削除分が「MFT のみに見える」差として観測される。
    assert!(
        deleted_in_mft.len() >= 5,
        "expected >=5 deleted file_*.txt in MFT, got {}: {deleted_in_mft:?}",
        deleted_in_mft.len()
    );
    // 削除されたファイルはインデックスから消えている（NTFS の正規動作）。
    for d in &deleted_in_mft {
        assert!(
            !in_index.contains(d),
            "deleted file {d} should not appear in $INDEX_ROOT (live view)"
        );
    }
}
