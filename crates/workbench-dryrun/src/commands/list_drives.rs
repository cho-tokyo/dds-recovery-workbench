//! `list-drives` サブコマンド: 接続中の論理ドライブを一覧表示する。

use anyhow::Result;
use dds_core::format::format_bytes;

use crate::drives::list_drives;

/// `list-drives` サブコマンドのエントリーポイント。
///
/// `sysinfo` 経由でドライブ一覧を取得し、整形して標準出力に書き出す。
/// ドライブが 1 件も見つからない場合は警告メッセージのみ表示して
/// `Ok(())` を返す (環境依存のため非エラー扱い)。
pub fn run() -> Result<()> {
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
    Ok(())
}
