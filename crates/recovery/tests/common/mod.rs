//! 結合テスト用共通ヘルパ。
//!
//! fixtures/images/ 配下の zstd 圧縮 NTFS イメージ解凍と ground truth JSON 読み込み。
//! fs-ntfs クレートの同名ヘルパと同等機能だが、recovery クレートからも独立して
//! 使えるよう独自に複製している（クレート間で test/common は共有できないため）。

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

/// ground truth JSON を読む。
#[allow(dead_code)]
pub fn load_ground_truth(name: &str) -> serde_json::Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../..");
    path.push("fixtures/images");
    path.push(format!("{name}.json"));
    let s = std::fs::read_to_string(&path).expect("read json");
    serde_json::from_str(&s).expect("parse json")
}

/// クラスタ単位イメージリーダ（fs-ntfs 結合テスト同型）。
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
