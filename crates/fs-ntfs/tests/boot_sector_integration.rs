//! 結合テスト: 実 NTFS フィクスチャイメージのブートセクタを解析する。
//! 関連 FR: FR-LIVE-01。
mod common;
use dds_fs_ntfs::parse_boot_sector;

#[test]
fn parses_healthy_small_fixture_boot_sector() {
    let img = common::decompress_fixture("ntfs_healthy_small");
    assert!(img.len() >= 512, "image too small: {}", img.len());
    let bs = parse_boot_sector(&img[..512]).expect("parse");
    assert!(bs.bytes_per_sector >= 512, "bps={}", bs.bytes_per_sector);
    let cs = bs.cluster_size_bytes();
    assert!(
        (512..=65536).contains(&cs),
        "cluster size {cs} out of typical range"
    );
    assert!(bs.mft_lcn > 0, "mft_lcn must be > 0");
}

#[test]
fn cluster_size_within_typical_range_for_fixtures() {
    for name in ["ntfs_healthy_small", "ntfs_with_5_deletions_small"] {
        let img = common::decompress_fixture(name);
        let bs = parse_boot_sector(&img[..512]).expect("parse");
        let cs = bs.cluster_size_bytes();
        assert!(
            (512..=65536).contains(&cs),
            "fixture {name}: cluster size {cs} out of typical range"
        );
        assert!(
            bs.mft_record_size_bytes() >= 512,
            "fixture {name}: mft record too small"
        );
    }
}
