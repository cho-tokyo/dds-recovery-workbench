//! Chunk 14 結合テスト。実フィクスチャ（zstd 圧縮 NTFS イメージ）を使った高レベル `NtfsFile`
//! API の E2E 検証。`volume.iter_files()` 列挙 + `read_file_content` で ground truth と
//! SHA256 突合し、Phase 1 NTFS リーダー実装完成形を実証する。
//! 関連 FR: FR-LIVE-01, FR-LIVE-04, FR-LIVE-05, FR-LIVE-06, FR-REC-01, FR-REC-04。

mod common;

use dds_fs_ntfs::{parse_boot_sector, NtfsFile, NtfsVolume};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// クラスタ単位イメージリーダ（既存 `volume_integration.rs` と同型）。
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

fn open_fixture(name: &str) -> NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>> {
    let img = common::decompress_fixture(name);
    let cs = u64::from(parse_boot_sector(&img[..512]).unwrap().cluster_size_bytes());
    NtfsVolume::open(make_image_reader(img, cs)).expect("open")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

#[test]
fn iter_files_enumerates_all_three_fixtures() {
    // 仕様書では 4 フィクスチャ列挙が指定されているが、`ntfs_large_files` は実在しないので除外。
    // 各フィクスチャで「ユーザファイル数 >= expected_user_files」を検証する。
    for (fixture_name, expected_user_files) in [
        ("ntfs_healthy_small", 30usize),
        ("ntfs_with_5_deletions_small", 25),
        ("ntfs_directories", 109),
    ] {
        let mut volume = open_fixture(fixture_name);
        let user_files: Vec<NtfsFile> = volume
            .iter_files()
            .filter_map(Result::ok)
            .filter(|f| f.is_user_file() && !f.name.starts_with('$'))
            .collect();
        let unique_user_files: HashSet<u64> = user_files.iter().map(|f| f.record_index).collect();
        assert!(
            unique_user_files.len() >= expected_user_files,
            "Fixture {} expected >= {} user files, got {}",
            fixture_name,
            expected_user_files,
            unique_user_files.len()
        );
    }
}

#[test]
fn read_file_content_matches_ground_truth_sha256() {
    let mut volume = open_fixture("ntfs_directories");
    let ground_truth = common::load_ground_truth("ntfs_directories");
    let expected: HashMap<String, String> = ground_truth["files"]
        .as_array()
        .expect("files[]")
        .iter()
        .map(|f| {
            (
                f["path"].as_str().expect("path").to_string(),
                f["content_hash_sha256"].as_str().expect("hash").to_string(),
            )
        })
        .collect();

    // path -> NtfsFile マップを作成（path 重複時は最初のものを保持）
    let files: Vec<NtfsFile> = volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && !f.name.starts_with('$'))
        .collect();

    let mut matched = 0usize;
    let mut mismatched: Vec<(String, String, String)> = Vec::new();
    for file in &files {
        if let Some(expected_hash) = expected.get(&file.path) {
            let content = volume
                .read_file_content(file)
                .unwrap_or_else(|e| panic!("read failed for {}: {:?}", file.path, e));
            let actual_hash = sha256_hex(&content);
            if &actual_hash == expected_hash {
                matched += 1;
            } else {
                mismatched.push((file.path.clone(), expected_hash.clone(), actual_hash));
            }
        }
    }
    assert!(
        mismatched.is_empty(),
        "{} hash mismatches; first few: {:?}",
        mismatched.len(),
        mismatched.iter().take(3).collect::<Vec<_>>()
    );
    assert_eq!(
        matched,
        expected.len(),
        "expected all {} ground-truth files matched, got {}",
        expected.len(),
        matched
    );
}

#[test]
fn product_demo_with_ntfs_file_api() {
    let mut volume = open_fixture("ntfs_with_5_deletions_small");

    println!("\n=== DDS Recovery Workbench - Phase 1 NTFS Final Demo (Chunk 14) ===\n");
    println!("API completion: volume.iter_files() で全ファイルを 1 つの owned 型に統合");
    println!("Total MFT records: {}\n", volume.total_records());

    // iter_files で集めて user file + "file_" prefix にフィルタ
    let files: Vec<NtfsFile> = volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| f.is_user_file() && f.name.starts_with("file_"))
        .collect();

    let mut live: Vec<&NtfsFile> = files.iter().filter(|f| !f.is_deleted).collect();
    let mut deleted: Vec<&NtfsFile> = files.iter().filter(|f| f.is_deleted).collect();
    live.sort_by_key(|f| f.record_index);
    deleted.sort_by_key(|f| f.record_index);

    println!("Recoverable (Deleted) files:");
    let mut deleted_clones: Vec<NtfsFile> = deleted.iter().map(|f| (*f).clone()).collect();
    deleted_clones.sort_by_key(|f| f.record_index);
    for f in &deleted_clones {
        let content = volume.read_file_content(f).expect("read deleted");
        let hash = sha256_hex(&content);
        println!(
            "  [DELETED] #{:<4} {} ({} bytes, sha256: {}...)",
            f.record_index,
            f.path,
            f.size,
            &hash[..16]
        );
    }

    println!("\nLive files (showing all):");
    for f in &live {
        println!(
            "  [Live]    #{:<4} {} ({} bytes)",
            f.record_index, f.path, f.size
        );
    }

    println!("\n=== Summary ===");
    println!("Live files:    {}", live.len());
    println!("Deleted files: {}  <- 全件 SHA256 取得成功", deleted.len());
    println!("API code reduction: iter_records + 4 manual parsers -> iter_files (1 line)\n");

    assert_eq!(live.len(), 25, "expected 25 live, got {}", live.len());
    assert_eq!(
        deleted.len(),
        5,
        "expected 5 deleted, got {}",
        deleted.len()
    );

    // 削除ファイル全件で SHA256 取得成功（read_file_content が空でない）
    for f in &deleted_clones {
        let content = volume.read_file_content(f).expect("read deleted");
        assert!(
            !content.is_empty(),
            "Deleted file {} content should not be empty",
            f.path
        );
    }
}

#[test]
fn iter_files_supports_path_and_extension_filtering() {
    let mut volume = open_fixture("ntfs_directories");

    let txt_files: Vec<NtfsFile> = volume
        .iter_files()
        .filter_map(Result::ok)
        .filter(|f| {
            f.is_user_file() && !f.name.starts_with('$') && f.extension().as_deref() == Some("txt")
        })
        .collect();

    let unique: HashSet<u64> = txt_files.iter().map(|f| f.record_index).collect();
    assert_eq!(
        unique.len(),
        109,
        "expected 109 unique .txt files, got {}",
        unique.len()
    );

    // 多階層パス確認
    let deeply = txt_files
        .iter()
        .find(|f| f.path == "\\dir1\\sub1\\sub2\\file_deeply.txt");
    assert!(
        deeply.is_some(),
        "Expected to find deeply nested file \\dir1\\sub1\\sub2\\file_deeply.txt"
    );

    // \many\ 配下の 100 件確認
    let many_files: Vec<&NtfsFile> = txt_files
        .iter()
        .filter(|f| f.path.starts_with("\\many\\"))
        .collect();
    assert_eq!(
        many_files.len(),
        100,
        "expected 100 files under \\many\\, got {}",
        many_files.len()
    );
}
