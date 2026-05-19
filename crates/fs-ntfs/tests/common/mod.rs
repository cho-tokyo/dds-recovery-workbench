//! 結合テスト用共通ヘルパ。fixtures の zstd 圧縮イメージ解凍と ground truth JSON 読み込み。
use std::path::PathBuf;

/// fixtures/images/<name>.img.zst を解凍し、生バイト列を返す。
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
