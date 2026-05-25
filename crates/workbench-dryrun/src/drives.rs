//! 論理ドライブ列挙ヘルパー。
//!
//! Windows 環境では USB HDD は自動マウントされ `E:` / `F:` 等のドライブレターが
//! 割り当てられる。`\\.\E:` 形式の論理デバイスパスへ open することで、unsafe
//! Windows API を使わずにパーティション本体へアクセスできる。
//!
//! 物理ドライブ (`\\.\PhysicalDriveN`) への直接アクセスは Phase 2.1 で対応予定。

use std::path::PathBuf;
use sysinfo::Disks;

/// 接続中の論理ドライブの情報スナップショット。
#[derive(Debug, Clone)]
pub struct DriveInfo {
    /// ドライブパス (例: `"E:"`)
    pub drive_letter: String,
    /// マウントポイント (例: `"E:\\"`)
    pub mount_point: PathBuf,
    /// ボリュームラベル (例: `"USB_HDD"`)
    pub label: String,
    /// 容量 (バイト)
    pub total_bytes: u64,
    /// 空き容量 (バイト)
    pub available_bytes: u64,
    /// ファイルシステム (例: `"NTFS"`, `"FAT32"`)
    pub file_system: String,
    /// アクセス用デバイスパス (例: `"\\\\.\\E:"`)
    pub access_path: String,
}

impl DriveInfo {
    /// パーティションが NTFS かどうか (大小区別なし比較)。
    pub fn is_ntfs(&self) -> bool {
        self.file_system.eq_ignore_ascii_case("NTFS")
    }

    /// システムドライブ (`C:`) かどうか。
    pub fn is_system_drive(&self) -> bool {
        self.drive_letter.eq_ignore_ascii_case("C:")
    }
}

/// マウントポイント文字列から `"<letter>:"` 形式を抽出する。
///
/// `"E:\\"` → `"E:"`、`"D:"` → `"D:"`、それ以外はそのまま返す。
fn extract_drive_letter(mount: &str) -> String {
    let bytes = mount.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        mount[..2].to_string()
    } else {
        mount.to_string()
    }
}

/// 接続中の論理ドライブ一覧を返す。
///
/// `sysinfo::Disks::new_with_refreshed_list` 経由でシステム情報を取得し、
/// `DriveInfo` の `Vec` に変換する。空でも panic しない (検証ビルドサーバ等)。
pub fn list_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();

    disks
        .list()
        .iter()
        .map(|disk| {
            let mount = disk.mount_point().to_path_buf();
            let mount_str = mount.to_string_lossy().to_string();
            let drive_letter = extract_drive_letter(&mount_str);
            let access_path = format!("\\\\.\\{}", drive_letter);

            DriveInfo {
                drive_letter,
                mount_point: mount,
                label: disk.name().to_string_lossy().to_string(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                file_system: disk.file_system().to_string_lossy().to_string(),
                access_path,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(letter: &str, fs: &str) -> DriveInfo {
        DriveInfo {
            drive_letter: letter.to_string(),
            mount_point: format!("{}\\", letter).into(),
            label: "test".to_string(),
            total_bytes: 1024,
            available_bytes: 512,
            file_system: fs.to_string(),
            access_path: format!("\\\\.\\{}", letter),
        }
    }

    #[test]
    fn list_drives_does_not_panic() {
        // 検証ビルドサーバ (sysinfo がディスクを認識できない環境) でも
        // panic せず空 Vec を返すこと。
        let drives = list_drives();
        // Vec の長さは環境依存なので、length 0 以上のみ保証。
        let _ = drives.len();
    }

    #[test]
    fn drive_info_correctly_identifies_ntfs() {
        let d = sample("E:", "NTFS");
        assert!(d.is_ntfs());
        assert!(!d.is_system_drive());
    }

    #[test]
    fn drive_info_identifies_system_drive_case_insensitively() {
        assert!(sample("C:", "NTFS").is_system_drive());
        assert!(sample("c:", "NTFS").is_system_drive());
        assert!(!sample("D:", "NTFS").is_system_drive());
    }

    #[test]
    fn drive_info_is_ntfs_case_insensitive() {
        assert!(sample("E:", "ntfs").is_ntfs());
        assert!(sample("E:", "NTFS").is_ntfs());
        assert!(!sample("E:", "FAT32").is_ntfs());
        assert!(!sample("E:", "exFAT").is_ntfs());
    }

    #[test]
    fn extract_drive_letter_handles_common_shapes() {
        assert_eq!(extract_drive_letter("E:\\"), "E:");
        assert_eq!(extract_drive_letter("D:"), "D:");
        assert_eq!(extract_drive_letter("/"), "/");
        assert_eq!(extract_drive_letter(""), "");
    }
}
