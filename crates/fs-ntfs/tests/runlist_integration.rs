//! Runlist パーサ + `DataContent::runlist_bytes()` の結合テスト。既存フィクスチャを使い、
//! 実 NTFS イメージから runlist を取り出してデコードできることを検証する。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-REC-01（目標優先抽出）、FR-REC-04（データ整合性）。

mod common;

use dds_fs_ntfs::{
    extract_all_data_streams, extract_main_data_stream, find_best_file_name, parse_boot_sector,
    parse_mft_entry, parse_runlist, DataContent, MftEntry,
};

fn read_record(img: &[u8], idx: usize) -> Option<MftEntry> {
    let bs = parse_boot_sector(&img[..512]).ok()?;
    let off = bs.mft_byte_offset() as usize + idx * bs.mft_record_size_bytes() as usize;
    let size = bs.mft_record_size_bytes() as usize;
    if off + size > img.len() { return None; }
    if &img[off..off + 4] != b"FILE" { return None; }
    parse_mft_entry(&img[off..off + size]).ok()
}

/// 健全イメージ内の全ユーザファイル（`file_*`）が常駐 $DATA を持つ（=`runlist_bytes()` が `None`）
/// ことを確認する。小さい合成ファイルのみのため、常駐確定。
#[test]
fn all_user_files_in_healthy_image_have_resident_data() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let mut user_file_count = 0usize;
    for idx in 0..256 {
        let Some(entry) = read_record(&img, idx) else { continue; };
        let first = entry.header.first_attribute_offset as usize;
        let Some(name) = find_best_file_name(&entry.data, first) else { continue; };
        if !name.filename.starts_with("file_") { continue; }
        let Some(stream) = extract_main_data_stream(&entry.data, first) else { continue; };
        user_file_count += 1;
        // 常駐なので runlist_bytes() は None
        assert!(stream.content.runlist_bytes().is_none(),
            "expected resident for {}, got non-resident", name.filename);
        assert!(matches!(stream.content, DataContent::Resident { .. }),
            "expected DataContent::Resident for {}", name.filename);
    }
    assert!(user_file_count >= 30, "expected >=30 user files, got {}", user_file_count);
}

/// $MFT 自身（entry 0）の $DATA は書籍 Chapter 13 が明示する通り非常駐。runlist バイト列を取得し、
/// `parse_runlist` で実際にパースできることを実画像で検証する。これが Chunk 10 の本番経路。
#[test]
fn mft_entry_zero_has_non_resident_data_with_parseable_runlist() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    let entry = read_record(&img, 0).expect("MFT entry 0 must exist");
    let first = entry.header.first_attribute_offset as usize;
    // $MFT の全 $DATA ストリームを列挙、無名メインを取得
    let streams = extract_all_data_streams(&entry.data, first);
    let main = streams.iter().find(|s| s.name.is_empty()).expect("main $DATA on $MFT");
    // 書籍 Chapter 13 記載: $MFT 自身の $DATA は非常駐
    assert!(main.content.is_non_resident(), "$MFT main $DATA must be non-resident");
    let runlist_bytes = main.content.runlist_bytes().expect("runlist bytes available");
    assert!(!runlist_bytes.is_empty(), "runlist must have at least 1 byte (terminator)");
    let runs = parse_runlist(runlist_bytes).expect("$MFT runlist parses cleanly");
    assert!(!runs.is_empty(), "$MFT must occupy at least one cluster run");
    // 各ランの LCN が単調かつ非ゼロのクラスタ数を持つことを軽く検証
    for r in &runs {
        assert!(r.length_clusters > 0, "run length must be > 0");
        // $MFT は通常スパースではないので Some であることを期待
        assert!(r.lcn.is_some(), "$MFT runs should not be sparse");
    }
}

/// 削除済みファイル混在イメージでも $MFT のランリストパース経路が同じく機能することを確認。
#[test]
fn mft_entry_zero_runlist_parses_in_deletions_image() {
    let img = common::decompress_fixture("ntfs_with_5_deletions_small");
    let entry = read_record(&img, 0).expect("MFT entry 0 must exist");
    let first = entry.header.first_attribute_offset as usize;
    let main = extract_main_data_stream(&entry.data, first).expect("main $DATA");
    assert!(main.content.is_non_resident(), "$MFT main $DATA non-resident");
    let runlist_bytes = main.content.runlist_bytes().expect("runlist bytes");
    let runs = parse_runlist(runlist_bytes).expect("runlist parses");
    let total_clusters: u64 = runs.iter().map(|r| r.length_clusters).sum();
    assert!(total_clusters > 0, "$MFT must occupy >0 clusters");
    // real_size と total_clusters * cluster_size の関係を軽く検証
    let bs = parse_boot_sector(&img[..512]).expect("boot sector");
    let cluster_size = bs.cluster_size_bytes() as u64;
    let allocated_bytes = total_clusters * cluster_size;
    assert!(allocated_bytes >= main.content.size(),
        "allocated bytes ({}) must be >= real_size ({})",
        allocated_bytes, main.content.size());
}
