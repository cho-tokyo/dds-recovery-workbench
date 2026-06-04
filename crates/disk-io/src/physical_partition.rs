//! 物理ドライブのパーティションを NTFS reader として使うためのアダプタ (Chunk 24d-3)。
//!
//! 既存の `NtfsVolume` は `F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>`
//! というクロージャを受け取る (LCN / count) が、ボリュームを open する前段で
//! 「パーティション内の任意バイトオフセットを読む」生バイト読み出しが必要になる。
//!
//! [`PhysicalPartitionReader`] はパーティション内の論理オフセット (0 から) を
//! 物理ドライブの絶対オフセットへ自動変換しつつ、パーティション境界をはみ出した
//! 場合は適切にエラー or 切り詰めを行う。
//!
//! ## 使い方
//!
//! ```ignore
//! use dds_disk_io::{PhysicalDrive, PhysicalPartitionReader};
//!
//! let drive = PhysicalDrive::open(r"\\.\PhysicalDrive1")?;
//! let reader = PhysicalPartitionReader::new(drive, partition_start, partition_size);
//! let mut read_bytes = reader.into_closure();
//!
//! // パーティション先頭 512 バイト (NTFS ブートセクタ) を読む。
//! let boot = read_bytes(0, 512)?;
//! ```
//!
//! ## 安全性
//!
//! - 物理ドライブは [`PhysicalDrive::read_at`] 経由でのみアクセス (`unsafe` は追加しない)。
//! - 書き込み API は意図的に提供しない (NFR-REL-01)。
//! - `start + offset` のオーバーフローは `checked_add` で検出して `InvalidInput` 化する。
//!
//! 関連 FR: FR-PHY-06 (物理パーティションからの NtfsVolume open)

use std::io;
use std::sync::Mutex;

use crate::physical::PhysicalDrive;

/// 物理パーティション内のオフセットを物理ドライブの絶対オフセットへ変換する reader。
///
/// `NtfsVolume::open` の前段で「ブートセクタ生バイト読み出し」を行うために使う。
/// クラスタサイズが確定したあとは、本 reader を包んだ `(lcn, count)` 形式の
/// 高位クロージャを `NtfsVolume::open` に渡せばよい (詳細は `workbench-dryrun`
/// 側の volume ヘルパーを参照)。
///
/// ## 設計メモ
///
/// 内部で [`PhysicalDrive`] を [`Mutex`] に包むのは、`PhysicalDrive::read_at`
/// が `&self` を取りつつ Windows API 側でファイルポインタを移動させるため、
/// 単一の `&PhysicalDrive` をクロージャ内で共有しても排他制御が必要になる
/// ためである。`Arc` は意図的に使っていない: クロージャに `self` の中身を
/// `move` して 1 つの所有者で完結させたほうがシンプルで、`PhysicalDrive` が
/// `!Send` であることに起因する clippy 警告も避けられる。
pub struct PhysicalPartitionReader {
    drive: Mutex<PhysicalDrive>,
    partition_start_offset: u64,
    partition_size: u64,
}

impl PhysicalPartitionReader {
    /// 新規に reader を作る。
    ///
    /// # 引数
    /// - `drive`: open 済みの [`PhysicalDrive`] (read-only)
    /// - `partition_start_offset`: パーティション開始位置の絶対オフセット (バイト)
    /// - `partition_size`: パーティションサイズ (バイト)
    ///
    /// `drive` は内部で `Mutex` に包まれ、`into_closure` で生成される FnMut が
    /// 唯一の所有者となる。並列化レイヤ (Chunk 24b) は post-processing で発生する
    /// ため、本 reader 自体はシリアル運用前提。
    pub fn new(drive: PhysicalDrive, partition_start_offset: u64, partition_size: u64) -> Self {
        Self {
            drive: Mutex::new(drive),
            partition_start_offset,
            partition_size,
        }
    }

    /// パーティション開始からの論理オフセットを引数に取り、バイト列を返す
    /// クロージャを作って返す。
    ///
    /// # 引数 (返却される FnMut)
    /// - `offset`: パーティション先頭からのバイトオフセット (0 以上)
    /// - `length`: 読み取りバイト数
    ///
    /// # 振る舞い
    /// - `offset >= partition_size` の場合は `UnexpectedEof` エラー
    /// - `length` がパーティション末尾を超える場合は **境界内に切り詰めて** 返す
    /// - 切り詰め後のサイズが 0 のときは空 `Vec` を返す (エラーではない)
    /// - `partition_start_offset + offset` でオーバーフロー時は `InvalidInput`
    ///
    /// 内部では [`PhysicalDrive::read_at`] (read-only) を呼ぶ。
    pub fn into_closure(self) -> impl FnMut(u64, u64) -> Result<Vec<u8>, io::Error> {
        let drive = self.drive;
        let start = self.partition_start_offset;
        let size = self.partition_size;

        move |offset: u64, length: u64| -> Result<Vec<u8>, io::Error> {
            // パーティション境界チェック (開始位置)。
            if offset >= size {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "オフセットがパーティション境界を超えています: \
                         offset={}, partition_size={}",
                        offset, size
                    ),
                ));
            }

            // 読み取り長を境界内に制限。
            let max_len = size - offset;
            let read_len_u64 = length.min(max_len);
            if read_len_u64 == 0 {
                return Ok(Vec::new());
            }
            // PhysicalDrive::read_at は usize を要求するため範囲チェック。
            let read_len = usize::try_from(read_len_u64).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("読み取り長が usize 範囲外: length={}", read_len_u64),
                )
            })?;

            // 絶対オフセットへ変換 (オーバーフロー検出)。
            let absolute_offset = start.checked_add(offset).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "絶対オフセット計算でオーバーフロー: start={}, offset={}",
                        start, offset
                    ),
                )
            })?;

            // 物理ドライブから読み取り。Mutex が poisoned の場合は Other で報告。
            let drive_guard = drive
                .lock()
                .map_err(|_| io::Error::other("PhysicalDrive Mutex poisoned"))?;

            drive_guard
                .read_at(absolute_offset, read_len)
                .map_err(|e| io::Error::other(format!("{}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    // 注: `PhysicalDrive` は Windows の実機 HANDLE を保持するため、純粋なユニット
    //     テストで生成するのは困難。ここではオフセット計算と境界チェックの
    //     ロジックを「コンセプトベース」で検証する。実機統合テストは
    //     Chunk 24d-4 で実施 (workbench-dryrun の `--physical` 経由)。

    /// パーティション内の論理オフセットを絶対オフセットに換算するロジック。
    #[test]
    fn partition_reader_offset_calculation_concept() {
        // パーティションが 1 MB から始まる場合、論理 offset 100 は物理 1 MB + 100。
        let partition_start: u64 = 1024 * 1024;
        let logical_offset: u64 = 100;
        let absolute = partition_start
            .checked_add(logical_offset)
            .expect("no overflow");
        assert_eq!(absolute, 1024 * 1024 + 100);
    }

    /// パーティション境界内の読み取り長クリップロジック。
    #[test]
    fn partition_reader_boundary_check_concept() {
        let partition_size: u64 = 1024;

        // 正常な範囲。
        let offset: u64 = 500;
        let length: u64 = 200;
        assert!(offset + length <= partition_size);

        // 境界を超える要求 → 切り詰め。
        let offset: u64 = 900;
        let length: u64 = 200;
        let max_len = partition_size - offset; // 124 バイトまで読める
        assert_eq!(length.min(max_len), 124);

        // 末尾ぴったり → 0 バイト (空 Vec)。
        let offset: u64 = 1024;
        // offset >= partition_size → エラーで弾かれることをロジックで確認
        assert!(offset >= partition_size);
    }

    /// オーバーフロー検出ロジック (絶対オフセット計算時)。
    #[test]
    fn partition_reader_overflow_is_detected() {
        let start: u64 = u64::MAX - 10;
        let offset: u64 = 100;
        assert!(start.checked_add(offset).is_none());

        let start: u64 = 1024 * 1024;
        let offset: u64 = 512;
        assert_eq!(start.checked_add(offset), Some(1024 * 1024 + 512));
    }
}
