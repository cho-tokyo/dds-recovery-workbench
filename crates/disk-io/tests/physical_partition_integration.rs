//! 物理パーティション経由で NTFS ボリュームを open する統合テスト (Chunk 24d-3)。
//!
//! 実機の物理ドライブが必要なため、CI では `#[ignore]` を付けてスキップする。
//! ローカルで管理者権限の Powershell から `cargo test -p dds-disk-io
//! --test physical_partition_integration -- --ignored` で実行する。

#![cfg(windows)]

use dds_disk_io::{enumerate_physical_drives, FsType, PhysicalDrive, PhysicalPartitionReader};

/// `\\.\PhysicalDrive0` (システムドライブ) の最初の NTFS パーティションから
/// ブートセクタを `PhysicalPartitionReader` 経由で読み出せることを確認する。
///
/// 期待:
/// - 先頭 8 バイトに NTFS の OEM ID `"NTFS    "` が含まれる
/// - 511 バイト目に 0x55、512 バイト目相当に 0xAA (ブートシグネチャ)
#[test]
#[ignore = "管理者権限 + 実機 PhysicalDrive0 が必要なローカル検証用テスト"]
fn integration_open_ntfs_via_physical_partition() {
    let drives = enumerate_physical_drives();
    if drives.is_empty() {
        println!("物理ドライブが検出されませんでした (管理者権限なし?)、スキップ");
        return;
    }

    // 最初の NTFS パーティションを探す。
    let mut chosen: Option<(std::path::PathBuf, u64, u64)> = None;
    for info in &drives {
        let Ok(drive) = PhysicalDrive::open(&info.path) else {
            continue;
        };
        let Ok(partitions) = drive.list_partitions() else {
            continue;
        };
        if let Some(part) = partitions.iter().find(|p| p.fs_type == FsType::Ntfs) {
            chosen = Some((info.path.clone(), part.start_offset, part.size));
            break;
        }
    }

    let (path, start, size) = match chosen {
        Some(t) => t,
        None => {
            println!("NTFS パーティションが検出されませんでした、スキップ");
            return;
        }
    };

    let drive = PhysicalDrive::open(&path).expect("PhysicalDrive を再 open");
    let reader = PhysicalPartitionReader::new(drive, start, size);
    let mut read_bytes = reader.into_closure();

    let boot = read_bytes(0, 512).expect("ブートセクタ読み取り");
    assert_eq!(boot.len(), 512);
    // NTFS の OEM ID は offset 3-10 に "NTFS    " (8 バイト)
    assert_eq!(&boot[3..11], b"NTFS    ", "NTFS OEM ID 不一致");
    // ブートシグネチャ
    assert_eq!(boot[510], 0x55);
    assert_eq!(boot[511], 0xAA);
}

/// `PhysicalPartitionReader::into_closure()` のパーティション境界クリップ動作を
/// 実機で確認する。サイズ末尾を超える要求が安全に切り詰められることを保証。
#[test]
#[ignore = "管理者権限 + 実機 PhysicalDrive が必要なローカル検証用テスト"]
fn integration_partition_boundary_clips_on_real_drive() {
    let drives = enumerate_physical_drives();
    if drives.is_empty() {
        println!("物理ドライブなし、スキップ");
        return;
    }

    let mut chosen: Option<(std::path::PathBuf, u64, u64)> = None;
    for info in &drives {
        let Ok(drive) = PhysicalDrive::open(&info.path) else {
            continue;
        };
        let Ok(partitions) = drive.list_partitions() else {
            continue;
        };
        if let Some(part) = partitions.iter().find(|p| p.fs_type == FsType::Ntfs) {
            chosen = Some((info.path.clone(), part.start_offset, part.size));
            break;
        }
    }

    let (path, start, size) = match chosen {
        Some(t) => t,
        None => {
            println!("NTFS パーティションなし、スキップ");
            return;
        }
    };

    let drive = PhysicalDrive::open(&path).expect("PhysicalDrive を再 open");
    let reader = PhysicalPartitionReader::new(drive, start, size);
    let mut read_bytes = reader.into_closure();

    // パーティション末尾からはみ出す要求は切り詰められて返るはず。
    let near_end_offset = size.saturating_sub(256);
    let result = read_bytes(near_end_offset, 4096).expect("末尾近傍読み取り");
    assert!(
        result.len() <= 256,
        "境界クリップ動作 (got {} bytes)",
        result.len()
    );

    // 完全に境界超過 → UnexpectedEof
    let err = read_bytes(size + 1024, 512).expect_err("境界超過は EOF");
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
}
