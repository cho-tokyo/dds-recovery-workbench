//! NTFS ボリュームを論理ドライブパス (`\\.\E:`) から open するヘルパー。
//!
//! Windows ファイル API は `\\.\<letter>:` 形式でパーティション本体に
//! read-only アクセス可能 (管理者権限要)。
//!
//! `NtfsVolume::open` は `read_clusters(lcn, count)` クロージャを受け取り
//! `count * cluster_size` バイトを返す必要があるため、先にブートセクタの
//! 先頭 512 バイトを読んでクラスタサイズを確定する。

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

use anyhow::{Context, Result};
use dds_disk_io::PhysicalPartitionReader;
use dds_fs_ntfs::{parse_boot_sector, NtfsVolume};

/// `read_clusters` クロージャの戻り型 (ボックス化、Send は不要)。
type ClusterReader = Box<dyn FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>>;

/// 論理ドライブパス (例: `"\\\\.\\E:"`) から `NtfsVolume` を open する。
///
/// 手順:
/// 1. `OpenOptions::new().read(true).open(access_path)` で File 取得
/// 2. 先頭 512 バイトを読んで `parse_boot_sector` でクラスタサイズ確定
/// 3. `Mutex<File>` を共有する `read_clusters` クロージャを構築
/// 4. `NtfsVolume::open` でボリュームを bootstrap
///
/// # エラー
///
/// open / read / boot sector parse / volume open のいずれかで失敗した場合は
/// `anyhow::Error` でラップして返す。
pub fn open_ntfs_volume(access_path: &str) -> Result<NtfsVolume<ClusterReader>> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(access_path)
        .with_context(|| {
            format!(
                "ドライブを開けません: {}\n\
             管理者として実行する必要があるかもしれません。",
                access_path
            )
        })?;

    // ブートセクタ (先頭 512B) を読み、クラスタサイズを確定する。
    let mut boot = vec![0u8; 512];
    file.seek(SeekFrom::Start(0))
        .context("ブートセクタへの seek に失敗しました")?;
    file.read_exact(&mut boot)
        .context("ブートセクタの読み取りに失敗しました")?;
    let bs = parse_boot_sector(&boot).context("NTFS ブートセクタの解析に失敗しました")?;
    let cluster_size = u64::from(bs.cluster_size_bytes());

    let reader: ClusterReader = make_cluster_reader(file, cluster_size);
    NtfsVolume::open(reader).context("NTFS ボリュームの open に失敗しました")
}

/// 物理パーティション ([`PhysicalPartitionReader`]) から `NtfsVolume` を open する
/// (Chunk 24d-3)。
///
/// 手順:
/// 1. `PhysicalPartitionReader::into_closure()` でパーティション内バイトリーダ取得
/// 2. 先頭 512 バイトを読んで `parse_boot_sector` でクラスタサイズ確定
/// 3. バイトリーダを `(lcn, count) -> Vec<u8>` にラップ
/// 4. `NtfsVolume::open` でボリュームを bootstrap
///
/// # エラー
///
/// 読み取り / boot sector parse / volume open のいずれかで失敗した場合は
/// `anyhow::Error` でラップして返す。NTFS 管理領域 ($MFT) の破損などにより
/// open に失敗するケースは呼び出し側で業務的なメッセージへ変換すること。
pub fn open_ntfs_volume_from_partition(
    reader: PhysicalPartitionReader,
) -> Result<NtfsVolume<ClusterReader>> {
    let mut read_bytes = reader.into_closure();

    // パーティション先頭 512 バイト = NTFS ブートセクタ。
    let boot = read_bytes(0, 512).context("ブートセクタの読み取りに失敗しました")?;
    if boot.len() < 512 {
        anyhow::bail!(
            "ブートセクタが 512 バイトに満たない (got={})、パーティションサイズを確認してください",
            boot.len()
        );
    }
    let bs = parse_boot_sector(&boot[..512]).context("NTFS ブートセクタの解析に失敗しました")?;
    let cluster_size = u64::from(bs.cluster_size_bytes());

    let cluster_reader = make_partition_cluster_reader(read_bytes, cluster_size);
    NtfsVolume::open(cluster_reader).context("NTFS ボリュームの open に失敗しました")
}

/// パーティション内バイトリーダを `(lcn, count)` 形式のクラスタリーダにラップする。
///
/// `lcn * cluster_size` / `count * cluster_size` 計算のオーバーフローを検出する。
fn make_partition_cluster_reader<R>(mut read_bytes: R, cluster_size: u64) -> ClusterReader
where
    R: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error> + 'static,
{
    Box::new(
        move |lcn: u64, count: u64| -> Result<Vec<u8>, std::io::Error> {
            let offset = lcn.checked_mul(cluster_size).ok_or_else(|| {
                std::io::Error::other(format!(
                    "lcn overflow: lcn={} cluster_size={}",
                    lcn, cluster_size
                ))
            })?;
            let length = count.checked_mul(cluster_size).ok_or_else(|| {
                std::io::Error::other(format!(
                    "count overflow: count={} cluster_size={}",
                    count, cluster_size
                ))
            })?;
            read_bytes(offset, length)
        },
    )
}

/// `File` を `Mutex` に包んで `(lcn, count) -> Vec<u8>` クロージャを構築する。
///
/// クロージャ内では `lcn * cluster_size` へ seek し `count * cluster_size`
/// バイトを `read_exact` で読み取る。
fn make_cluster_reader(file: File, cluster_size: u64) -> ClusterReader {
    let file = Mutex::new(file);
    Box::new(
        move |lcn: u64, count: u64| -> Result<Vec<u8>, std::io::Error> {
            let mut f = file
                .lock()
                .map_err(|_| std::io::Error::other("File Mutex poisoned"))?;
            let offset = lcn.checked_mul(cluster_size).ok_or_else(|| {
                std::io::Error::other(format!(
                    "lcn overflow: lcn={} cluster_size={}",
                    lcn, cluster_size
                ))
            })?;
            let length = count.checked_mul(cluster_size).ok_or_else(|| {
                std::io::Error::other(format!(
                    "count overflow: count={} cluster_size={}",
                    count, cluster_size
                ))
            })?;
            f.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; length as usize];
            f.read_exact(&mut buf)?;
            Ok(buf)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn open_ntfs_volume_fails_on_non_existent_path() {
        // 存在しないパスを開こうとすると、Context 付きエラーが返ること。
        // `NtfsVolume` は `Debug` 未実装のため `unwrap_err` ではなく `match` で取り出す。
        let result = open_ntfs_volume("\\\\.\\Z99_does_not_exist:");
        let err = match result {
            Ok(_) => panic!("存在しないパスで成功するはずがない"),
            Err(e) => e,
        };
        let msg = format!("{:#}", err);
        assert!(msg.contains("ドライブを開けません"));
    }

    #[test]
    fn make_cluster_reader_returns_requested_bytes() {
        // 一時ファイルに 8 KB 書き、cluster_size=512 で (lcn=2, count=3)
        // = offset 1024, length 1536 を読み出せること。
        let mut tmp = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..8192u16).map(|n| (n & 0xFF) as u8).collect();
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();

        let file = OpenOptions::new().read(true).open(tmp.path()).unwrap();
        let mut reader = make_cluster_reader(file, 512);
        let got = reader(2, 3).unwrap();
        assert_eq!(got.len(), 1536);
        assert_eq!(got, data[1024..1024 + 1536]);
    }

    #[test]
    fn make_partition_cluster_reader_translates_lcn_to_bytes() {
        // バイトリーダのモック: offset/length をエコーバックする vec を返す。
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(u64, u64)>::new()));
        let calls_clone = calls.clone();
        let mock_byte_reader = move |off: u64, len: u64| -> Result<Vec<u8>, std::io::Error> {
            calls_clone.lock().unwrap().push((off, len));
            Ok(vec![0u8; len as usize])
        };
        let cluster_size = 4096u64;
        let mut cluster_reader = make_partition_cluster_reader(mock_byte_reader, cluster_size);
        let result = cluster_reader(3, 2).expect("read");
        assert_eq!(result.len(), (2 * cluster_size) as usize);
        let recorded = calls.lock().unwrap().clone();
        assert_eq!(recorded, vec![(3 * cluster_size, 2 * cluster_size)]);
    }

    #[test]
    fn make_cluster_reader_returns_eof_when_past_end() {
        // ファイル末尾を越える読み取りは EOF エラー。
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&[0u8; 256]).unwrap();
        tmp.flush().unwrap();
        let file = OpenOptions::new().read(true).open(tmp.path()).unwrap();
        let mut reader = make_cluster_reader(file, 512);
        let err = reader(0, 1).unwrap_err();
        // 512 バイト要求に対しファイルは 256 バイトしかないので失敗するはず。
        assert!(
            err.kind() == std::io::ErrorKind::UnexpectedEof
                || err.to_string().contains("EOF")
                || err.to_string().contains("read")
        );
    }

    #[test]
    fn make_partition_cluster_reader_detects_lcn_overflow() {
        // lcn が `u64::MAX / cluster_size` を超えるとオーバーフローエラーになる。
        let dummy = |_off: u64, _len: u64| -> Result<Vec<u8>, std::io::Error> {
            unreachable!("オーバーフロー検出で呼ばれないはず")
        };
        let mut reader = make_partition_cluster_reader(dummy, 4096);
        let err = reader(u64::MAX, 1).expect_err("overflow expected");
        assert!(err.to_string().contains("overflow"));
    }
}
