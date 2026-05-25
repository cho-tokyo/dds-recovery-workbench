//! Chunk 23 結合テスト用共通ヘルパ。
//!
//! `fixtures/images/<name>.img.zst` を解凍し、NTFS ボリュームのクラスタリーダ
//! クロージャを構築する。recovery クレートの同名ヘルパと等価実装（クレートを
//! 跨いだ `tests/common` の共有はできないため）。

use std::path::PathBuf;

/// `fixtures/images/<name>.img.zst` を解凍し、生バイト列を返す。
#[allow(dead_code)]
pub fn decompress_fixture(name: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../..");
    path.push("fixtures/images");
    path.push(format!("{name}.img.zst"));
    let f = std::fs::File::open(&path)
        .unwrap_or_else(|e| panic!("fixture not found at {:?}: {}", path, e));
    let mut decoder = zstd::stream::Decoder::new(f).expect("zstd decoder");
    let mut out = Vec::new();
    std::io::copy(&mut decoder, &mut out).expect("decompress");
    out
}

/// NTFS イメージ用のクラスタ単位リーダ。
#[allow(dead_code)]
pub fn make_image_reader(
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

/// `dir` 配下の通常ファイル件数を再帰的にカウントする。
/// `walkdir` を依存に増やさず、std だけで実装。
#[allow(dead_code)]
pub fn count_files_recursive(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += count_files_recursive(&path);
        } else {
            count += 1;
        }
    }
    count
}
