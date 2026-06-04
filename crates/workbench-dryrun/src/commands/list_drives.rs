//! `list-drives` サブコマンド: 接続中のドライブを一覧表示する。
//!
//! 既定では論理ドライブ (sysinfo 経由、マウント済み) を表示する。
//! `--physical` を指定すると物理ドライブ (`\\.\PhysicalDriveN`) を列挙する。
//! 後者は Chunk 24d-1 で追加された壊れた FS の HDD 対応用ルート。

use anyhow::Result;
use clap::Args;
use dds_core::format::format_bytes;
use dds_disk_io::{enumerate_physical_drives, FsType, PhysicalDrive};

use crate::drives::list_drives;

/// `list-drives` サブコマンドの引数。
#[derive(Args, Debug, Default)]
pub struct ListDrivesArgs {
    /// 物理ドライブ (`\\.\PhysicalDriveN`) を列挙する。
    ///
    /// マウントできない壊れた FS の HDD も検出可能 (要管理者権限)。
    #[arg(long)]
    pub physical: bool,
}

/// `list-drives` サブコマンドのエントリーポイント。
///
/// `args.physical` の真偽で物理ドライブ列挙 / 論理ドライブ列挙を切り替える。
pub fn run(args: &ListDrivesArgs) -> Result<()> {
    if args.physical {
        run_physical()
    } else {
        run_logical()
    }
}

/// 論理ドライブ (sysinfo 経由) を整形して表示する。
fn run_logical() -> Result<()> {
    println!("接続中の論理ドライブ:");
    println!();

    let drives = list_drives();
    if drives.is_empty() {
        println!("  ドライブが見つかりませんでした。");
        println!("  (sysinfo が情報を取得できない環境か、ドライブが未マウントの可能性)");
        return Ok(());
    }

    for (i, drive) in drives.iter().enumerate() {
        let system_marker = if drive.is_system_drive() {
            " [システム]"
        } else {
            ""
        };
        let ntfs_marker = if drive.is_ntfs() { "  NTFS" } else { "" };

        println!(
            "  [{}] {} {}{}{}",
            i + 1,
            drive.drive_letter,
            drive.label,
            system_marker,
            ntfs_marker
        );
        println!("       容量:       {}", format_bytes(drive.total_bytes));
        println!("       空き容量:   {}", format_bytes(drive.available_bytes));
        println!("       FS:         {}", drive.file_system);
        println!("       アクセス:   {}", drive.access_path);
        println!();
    }

    println!("対象 HDD を Workbench で読み込むには、上記の「アクセス」パスを使用します。");
    println!("壊れた FS の HDD は `list-drives --physical` で物理ドライブ側を確認してください。");
    Ok(())
}

/// 物理ドライブ (`\\.\PhysicalDriveN`) を列挙して表示する。
fn run_physical() -> Result<()> {
    println!("物理ドライブ:");
    println!("---------------------------------------------");

    let drives = enumerate_physical_drives();

    if drives.is_empty() {
        println!("物理ドライブが検出されませんでした。");
        println!();
        println!("考えられる原因:");
        println!("  1. 管理者権限がない (「管理者として実行」で起動してください)");
        println!("  2. 物理的に接続されているドライブがない");
        println!("  3. ドライバが正しくインストールされていない");
        return Ok(());
    }

    println!("検出された物理ドライブ: {} 個", drives.len());
    println!();

    for drive in &drives {
        println!("[{}] {}", drive.drive_number, drive.path.display());
        println!("    サイズ:    {}", format_bytes(drive.total_bytes));
        if let Some(vendor) = &drive.vendor_id {
            println!("    Vendor:    {}", vendor);
        }
        if let Some(product) = &drive.product_id {
            println!("    Product:   {}", product);
        }
        if let Some(serial) = &drive.serial_number {
            println!("    Serial:    {}", serial);
        }
        let removable_marker = if drive.bus_type.is_removable() {
            " (リムーバブル)"
        } else {
            ""
        };
        println!(
            "    Bus Type:  {}{}",
            drive.bus_type.display_name(),
            removable_marker
        );

        // パーティション情報を取得して表示 (Chunk 24d-2)
        match PhysicalDrive::open(&drive.path) {
            Ok(opened) => match opened.list_partitions() {
                Ok(partitions) if partitions.is_empty() => {
                    println!("    └─ パーティション情報を取得できませんでした");
                }
                Ok(partitions) => {
                    for partition in &partitions {
                        let recoverable_mark = if partition.fs_type.is_recoverable() {
                            " ★ 復旧対象"
                        } else {
                            ""
                        };
                        println!(
                            "    └─ Partition {}: {}, {}, {}{}",
                            partition.number,
                            partition.partition_type.display_name(),
                            format_bytes(partition.size),
                            partition.fs_type.display_name(),
                            recoverable_mark,
                        );
                    }
                }
                Err(e) => {
                    println!("    └─ パーティション解析エラー: {}", e);
                }
            },
            Err(e) => {
                println!("    └─ ドライブを再 open できませんでした: {}", e);
            }
        }
        println!();
    }

    println!("---------------------------------------------");
    println!();
    println!("使い方:");
    println!("  診断: workbench-dryrun diagnose --physical N --partition M");
    println!("  復旧: workbench-dryrun recover --physical N --partition M");
    println!();
    println!("例:");
    let example = find_ntfs_partition_example(&drives);
    match example {
        Some((drive_num, part_num)) => {
            println!(
                "  workbench-dryrun diagnose --physical {} --partition {}",
                drive_num, part_num
            );
            println!(
                "  workbench-dryrun recover  --physical {} --partition {}",
                drive_num, part_num
            );
        }
        None => {
            println!("  workbench-dryrun diagnose --physical 1 --partition 1");
            println!("  workbench-dryrun recover  --physical 1 --partition 1");
        }
    }
    println!();

    Ok(())
}

/// 接続中の物理ドライブから「最初に見つかった NTFS パーティション」を例として返す
/// (Chunk 24d-3 の業務ヒント用)。
///
/// パーティション再読み込みのため [`PhysicalDrive::open`] を再実行するが、失敗時は
/// 単純にスキップしてデフォルト値にフォールバックする。
fn find_ntfs_partition_example(drives: &[dds_disk_io::PhysicalDriveInfo]) -> Option<(u32, u32)> {
    for drive_info in drives {
        let opened = match PhysicalDrive::open(&drive_info.path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let partitions = match opened.list_partitions() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if let Some(part) = partitions.iter().find(|p| p.fs_type == FsType::Ntfs) {
            return Some((drive_info.drive_number, part.number));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_default_logical_mode() {
        let args = ListDrivesArgs::default();
        assert!(!args.physical);
    }

    #[test]
    fn run_logical_does_not_panic() {
        // sysinfo がドライブを列挙できない環境でも Ok を返すことだけ確認。
        let args = ListDrivesArgs { physical: false };
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn run_physical_does_not_panic() {
        // 管理者権限なしでも空 Vec が返って Ok になることを確認。
        let args = ListDrivesArgs { physical: true };
        let result = run(&args);
        assert!(result.is_ok());
    }
}
