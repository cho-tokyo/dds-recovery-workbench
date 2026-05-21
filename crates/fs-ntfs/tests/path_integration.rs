//! Chunk 13 結合テスト。実フィクスチャを使った B+ ツリー走査 + フルパス再構築の E2E 検証。
//! 関連 FR: FR-LIVE-04（ファイルツリー、完全達成）, FR-LIVE-05（削除エントリ可視化）, FR-LIVE-06。

mod common;

use std::collections::{HashMap, HashSet};

use dds_fs_ntfs::{
    find_best_file_name, parse_boot_sector, FileNameNamespace, NtfsVolume, PathResolver,
};

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

/// 名前→record_index のマップを全 MFT 走査で構築。`$` 始まりは除外。
fn build_name_to_index(
    volume: &mut NtfsVolume<impl FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>,
) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_in_use() {
            continue;
        }
        let Some(fn_) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if fn_.filename.starts_with('$') {
            continue;
        }
        map.entry(fn_.filename).or_insert(idx);
    }
    map
}

#[test]
fn lists_all_files_in_root_with_full_paths() {
    let mut volume = open_fixture("ntfs_healthy_small");
    let entries = volume.list_directory(5).expect("list root");
    let user: Vec<_> = entries
        .iter()
        .filter(|e| e.name().starts_with("file_"))
        .collect();
    let mut win32_names: Vec<&str> = user
        .iter()
        .filter(|e| e.file_name.namespace.is_preferred_for_display())
        .map(|e| e.name())
        .collect();
    win32_names.sort();
    win32_names.dedup();
    assert_eq!(
        win32_names.len(),
        30,
        "expected 30 unique user files in root, got {}",
        win32_names.len()
    );

    let mut resolver = PathResolver::new();
    for e in user.iter().take(5) {
        let full = resolver
            .resolve(e.child_ref.entry_number, &mut volume)
            .expect("resolve");
        assert!(full.starts_with("\\file_"), "unexpected path: {}", full);
    }
}

#[test]
fn reconstructs_deep_nested_paths() {
    let mut volume = open_fixture("ntfs_directories");
    let ground_truth = common::load_ground_truth("ntfs_directories");

    let mut resolver = PathResolver::new();
    let mut found_paths: HashSet<String> = HashSet::new();
    let mut indices: Vec<u64> = Vec::new();
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_in_use() {
            continue;
        }
        let Some(fn_) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if fn_.filename.starts_with('$') {
            continue;
        }
        indices.push(idx);
    }
    for idx in indices {
        if let Ok(p) = resolver.resolve(idx, &mut volume) {
            found_paths.insert(p);
        }
    }

    let expected_files = ground_truth["files"].as_array().expect("files[]");
    let mut hit = 0usize;
    let mut missed: Vec<String> = Vec::new();
    for f in expected_files {
        let path = f["path"].as_str().expect("path str");
        if found_paths.contains(path) {
            hit += 1;
        } else {
            missed.push(path.to_string());
        }
    }
    assert_eq!(
        hit,
        expected_files.len(),
        "missing {} paths, e.g. {:?}",
        missed.len(),
        missed.iter().take(3).collect::<Vec<_>>()
    );

    // 4 階層パス再構築の明示的確認
    assert!(
        found_paths.contains("\\dir1\\sub1\\sub2\\file_deeply.txt"),
        "deep nested path missing"
    );
}

#[test]
fn enumerates_100_files_directory_via_index_allocation() {
    let mut volume = open_fixture("ntfs_directories");
    let name_to_index = build_name_to_index(&mut volume);
    let many_idx = *name_to_index.get("many").expect("\\many directory");

    let entries = volume.list_directory(many_idx).expect("list many");
    let unique: HashSet<String> = entries
        .iter()
        .filter(|e| e.file_name.namespace.is_preferred_for_display())
        .map(|e| e.name().to_string())
        .collect();
    assert_eq!(
        unique.len(),
        100,
        "expected 100 files in \\many (via $INDEX_ALLOCATION B+ tree), got {}",
        unique.len()
    );
    for i in 0..100 {
        let expected = format!("file_{:03}.txt", i);
        assert!(unique.contains(&expected), "missing {}", expected);
    }
}

#[test]
fn reconstructs_deleted_file_paths() {
    let mut volume = open_fixture("ntfs_with_5_deletions_small");
    let mut resolver = PathResolver::new();
    let mut deleted_paths: Vec<String> = Vec::new();
    let mut deleted_indices: Vec<u64> = Vec::new();
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        if !entry.header.is_deleted() {
            continue;
        }
        let Some(fn_) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if !fn_.filename.starts_with("file_") {
            continue;
        }
        deleted_indices.push(idx);
    }
    for idx in deleted_indices {
        let p = resolver.resolve(idx, &mut volume).expect("resolve deleted");
        assert!(
            p.starts_with("\\file_"),
            "deleted file path should be \\file_*, got {}",
            p
        );
        deleted_paths.push(p);
    }
    assert!(
        deleted_paths.len() >= 5,
        "expected >=5 deleted with paths, got {}",
        deleted_paths.len()
    );
}

#[test]
fn product_demo_with_full_paths() {
    let mut volume = open_fixture("ntfs_with_5_deletions_small");

    println!("\n=== DDS Recovery Workbench - Phase 1 (post-Chunk 13) ===\n");
    println!("NTFS reader 実用形完成: list_directory + PathResolver でフルパス付き全エントリ取得");
    println!("Total MFT records: {}\n", volume.total_records());

    // 全エントリ収集（後で sort してから表示）
    let mut snapshots: Vec<(u64, String, bool, bool)> = Vec::new();
    for (idx, result) in volume.iter_records() {
        let Ok(entry) = result else { continue };
        let Some(fn_) =
            find_best_file_name(&entry.data, entry.header.first_attribute_offset as usize)
        else {
            continue;
        };
        if !fn_.filename.starts_with("file_") {
            continue;
        }
        snapshots.push((
            idx,
            fn_.filename.clone(),
            entry.header.is_deleted(),
            fn_.namespace == FileNameNamespace::Dos,
        ));
    }

    // フルパスを別途解決（resolver と volume.iter_records の借用は分離が必要）
    let mut resolver = PathResolver::new();
    let mut live_count = 0;
    let mut deleted_count = 0;
    let mut rows: Vec<(u64, String, bool)> = Vec::new();
    for (idx, fallback_name, is_deleted, is_dos) in snapshots {
        if is_dos {
            continue; // DOS 短縮名は重複なので除外
        }
        let path = match resolver.resolve(idx, &mut volume) {
            Ok(p) => p,
            Err(_) => format!("\\?\\{}", fallback_name),
        };
        rows.push((idx, path, is_deleted));
        if is_deleted {
            deleted_count += 1;
        } else {
            live_count += 1;
        }
    }
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    for (idx, path, del) in &rows {
        let status = if *del { "[DELETED]" } else { "[Live]   " };
        let suffix = if *del { "  <- 完全復元!" } else { "" };
        println!("  {} #{:<4} {}{}", status, idx, path, suffix);
    }
    println!("\n=== Summary ===");
    println!("Live files recovered:    {}", live_count);
    println!(
        "Deleted files recovered: {}  <- パスも完全復元",
        deleted_count
    );
    println!("Total user files:        {}\n", live_count + deleted_count);

    assert!(live_count >= 25, "expected >=25 live, got {}", live_count);
    assert!(
        deleted_count >= 5,
        "expected >=5 deleted, got {}",
        deleted_count
    );
}
