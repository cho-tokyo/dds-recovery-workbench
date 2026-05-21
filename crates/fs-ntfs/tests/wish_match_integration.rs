//! Chunk 15 結合テスト: fs-ntfs と wish-match の橋渡し検証。
//!
//! 実フィクスチャ NTFS イメージから [`NtfsFile`] を列挙し、`FileInfo` に変換、
//! [`Wishlist`] と突合する一連の流れを E2E で検証する。お客様視点の業務シナリオを
//! テスト名で物語る形で命名（NTFS 技術実装層の技術命名と対比）。
//! 関連 FR: FR-WISH-01, FR-WISH-02, FR-REC-01。

mod common;

use dds_fs_ntfs::{parse_boot_sector, NtfsFile, NtfsVolume};
use dds_wish_match::{
    match_files, FileInfo, Priority, Wish, WishItem, Wishlist,
};
use std::collections::HashSet;

fn make_image_reader(
    img: Vec<u8>,
    cluster_size: u64,
) -> impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> {
    move |lcn, count| {
        let start = (lcn * cluster_size) as usize;
        let end = start + (count * cluster_size) as usize;
        if end > img.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "oob",
            ));
        }
        Ok(img[start..end].to_vec())
    }
}

fn open_fixture(
    name: &str,
) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(make_image_reader(img, cs)).expect("open volume")
}

/// `iter_files` の結果をフィルタして `FileInfo` の Vec に変換。
fn collect_user_file_infos<F>(volume: &mut NtfsVolume<F>) -> Vec<FileInfo>
where
    F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && !f.has_system_name_prefix())
        .map(|f| FileInfo::from(&f))
        .collect()
}

#[test]
fn matches_all_txt_files_in_directories_fixture() {
    // 業務シナリオ: お客様が「全部の .txt ファイルが欲しい」と希望。
    let mut volume = open_fixture("ntfs_directories");
    let wishlist = Wishlist::new().add(
        Wish::new(WishItem::Extension("txt".into()), "テキストファイル全部")
            .with_priority(Priority::High),
    );
    let file_infos = collect_user_file_infos(&mut volume);
    let matches = match_files(&file_infos, &wishlist);

    // ntfs_directories の .txt は 109 件（ntfs_file_integration::iter_files_supports_path_and_extension_filtering と整合）。
    let unique: HashSet<&str> = matches.iter().map(|m| m.source_id.as_str()).collect();
    assert_eq!(
        unique.len(),
        109,
        "Expected 109 unique .txt matches, got {}",
        unique.len()
    );
}

#[test]
fn matches_files_in_dir1_subdirectory_only() {
    // 業務シナリオ: お客様が「dir1 配下のファイルが欲しい」と希望。
    // 期待: file_001.txt + sub1/file_002.txt + sub1/sub2/file_deeply.txt = 3 ファイル。
    // 境界条件: 部分名前一致 (dir1other 等) はマッチしてはいけない。
    let mut volume = open_fixture("ntfs_directories");
    let wishlist = Wishlist::new().add(
        Wish::new(WishItem::PathPrefix("\\dir1".into()), "dir1 配下")
            .with_priority(Priority::Critical),
    );
    let file_infos = collect_user_file_infos(&mut volume);
    let matches = match_files(&file_infos, &wishlist);

    assert_eq!(
        matches.len(),
        3,
        "Expected 3 files under \\dir1, got {} ({:?})",
        matches.len(),
        matches.iter().map(|m| &m.source_id).collect::<Vec<_>>()
    );
    // 全件 Critical (=100) スコアのはず。
    for m in &matches {
        assert_eq!(m.priority_score, 100);
    }
}

#[test]
fn matches_deleted_files_with_txt_extension() {
    // 業務シナリオ: お客様が「削除された .txt を全部復旧したい」と希望。
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let wishlist = Wishlist::new()
        .add(Wish::new(WishItem::Extension("txt".into()), "復旧したい .txt"));

    let deleted_infos: Vec<FileInfo> = volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && f.is_deleted && !f.has_system_name_prefix())
        .map(|f| FileInfo::from(&f))
        .collect();
    let matches = match_files(&deleted_infos, &wishlist);

    assert_eq!(
        matches.len(),
        5,
        "Expected 5 deleted .txt files, got {}",
        matches.len()
    );
}

#[test]
fn product_demo_wish_match_with_priority() {
    // プロダクトデモ: お客様の希望（架空のシナリオ）から優先抽出順を導出する。
    //
    //   Critical: \dir1\sub1\sub2 配下 (最深部の重要書類)        +100
    //   High:     file_root を含むファイル名 (ルート直下の対象)  +75
    //   Low:      .txt 全般                                       +25
    //
    // 期待: \dir1\sub1\sub2\file_deeply.txt が Critical(100) + Low(25) = 125 で最高スコア。
    let mut volume = open_fixture("ntfs_directories");
    let wishlist = Wishlist::new()
        .add(
            Wish::new(
                WishItem::PathPrefix("\\dir1\\sub1\\sub2".into()),
                "最深部の重要書類",
            )
            .with_priority(Priority::Critical),
        )
        .add(
            Wish::new(
                WishItem::FilenameContains("file_root".into()),
                "ルート直下の root_ プレフィックスファイル",
            )
            .with_priority(Priority::High),
        )
        .add(
            Wish::new(WishItem::Extension("txt".into()), "テキスト全般")
                .with_priority(Priority::Low),
        );

    // source_id → path 逆引きを作る（println 用）。
    let files: Vec<NtfsFile> = volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && !f.has_system_name_prefix())
        .collect();
    let path_by_source: std::collections::HashMap<String, String> = files
        .iter()
        .map(|f| (format!("NTFS#{}", f.record_index), f.path.clone()))
        .collect();
    let file_infos: Vec<FileInfo> = files.iter().map(FileInfo::from).collect();

    let matches = match_files(&file_infos, &wishlist);

    println!("\n=== Wishlist Match Results (Priority-Sorted) ===");
    println!("Wishlist:");
    println!("  Critical(100): PathPrefix \\dir1\\sub1\\sub2 - 最深部の重要書類");
    println!("  High(75):      FilenameContains \"file_root\" - ルート直下の root_ プレフィックスファイル");
    println!("  Low(25):       Extension \"txt\" - テキスト全般");
    println!();
    println!("Top 15 matches (score-sorted, source -> path):");
    for (i, m) in matches.iter().enumerate().take(15) {
        let path = path_by_source
            .get(&m.source_id)
            .map(String::as_str)
            .unwrap_or("?");
        let labels: Vec<&str> = m
            .matched_wishes
            .iter()
            .map(|w| w.label.as_str())
            .collect();
        println!(
            "  {:2}. [{:3}] {} -> {}  (matched: {})",
            i + 1,
            m.priority_score,
            m.source_id,
            path,
            labels.join(" + ")
        );
    }
    println!("\nTotal matches: {}", matches.len());

    // 1 位は file_deeply.txt（Critical 100 + Low 25 = 125）。
    assert_eq!(
        matches[0].priority_score, 125,
        "Top score must be 125 (Critical+Low). Got {}",
        matches[0].priority_score
    );
    let top_path = path_by_source.get(&matches[0].source_id).expect("path");
    assert_eq!(top_path, "\\dir1\\sub1\\sub2\\file_deeply.txt");
    assert!(matches[0]
        .matched_wishes
        .iter()
        .any(|w| w.label.contains("最深部")));
}
