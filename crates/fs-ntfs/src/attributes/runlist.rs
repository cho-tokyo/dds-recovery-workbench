//! NTFS 非常駐 `$DATA` 属性の runlist（データラン）デコーダ。書籍 Chapter 11 Figure 11.6
//! （VCN → LCN 概念）と Chapter 13 Figure 13.3（物理エンコーディング）に準拠。
//! 関連 FR: FR-LIVE-01（NTFS 読み取り）、FR-REC-01、FR-REC-04。
use thiserror::Error;

/// 単一データラン（連続クラスタ群）。書籍 Chapter 11 Figure 11.6 の VCN→LCN マッピング 1 要素。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// このランのクラスタ数。
    pub length_clusters: u64,
    /// 開始 LCN。`None` = スパース（クラスタ未割当、論理ゼロ）。
    pub lcn: Option<u64>,
}
impl Run {
    /// スパースランなら true。
    pub fn is_sparse(&self) -> bool { self.lcn.is_none() }
    /// ラン長をバイト数に換算（オーバーフロー時 saturate）。
    pub fn byte_length(&self, cluster_size: u64) -> u64 {
        self.length_clusters.saturating_mul(cluster_size)
    }
}

/// `parse_runlist` / `read_runs_with` のエラー型。`DiskRead` variant ゆえ `PartialEq` 不実装。
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum RunlistError {
    #[error("Buffer too small for runlist header: got {got}, need at least 1")]
    BufferTooSmall { got: usize },
    #[error("Invalid runlist header nibble: length_bytes={length_bytes}, offset_bytes={offset_bytes} (length 1..=8, offset 0..=8)")]
    InvalidHeaderNibble { length_bytes: u8, offset_bytes: u8 },
    #[error("Length field truncated: need {need} bytes, got {got}")]
    LengthFieldTruncated { need: usize, got: usize },
    #[error("Offset field truncated: need {need} bytes, got {got}")]
    OffsetFieldTruncated { need: usize, got: usize },
    #[error("LCN overflow during accumulation: previous={previous}, delta={delta}")]
    LcnOverflow { previous: i64, delta: i64 },
    #[error("Resolved LCN is negative: got {got}")]
    NegativeLcn { got: i64 },
    #[error("Invalid cluster size: {got} (must be > 0)")]
    InvalidClusterSize { got: u64 },
    #[error("Real size mismatch: computed={computed}, declared={declared}")]
    RealSizeMismatch { computed: u64, declared: u64 },
    #[error("Disk read error: {0}")]
    DiskRead(#[from] std::io::Error),
}

/// バイト列から runlist をデコードする（純粋関数、I/O なし）。書籍 Chapter 13 Figure 13.3
/// 準拠: ラン先頭バイト下位 4bit = length バイト数、上位 4bit = offset バイト数、続いて
/// length（符号なし LE）、offset（符号付き LE、前ランからの差分）。終端は `0x00`。
/// `offset_bytes == 0` はスパースラン（書籍 Figure 11.6）。
pub fn parse_runlist(bytes: &[u8]) -> Result<Vec<Run>, RunlistError> {
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    let mut current_lcn: i64 = 0;
    loop {
        if cursor >= bytes.len() { return Err(RunlistError::BufferTooSmall { got: cursor }); }
        let header = bytes[cursor]; cursor += 1;
        if header == 0x00 { break; }
        let length_bytes = header & 0x0F;
        let offset_bytes = (header >> 4) & 0x0F;
        if length_bytes == 0 || length_bytes > 8 || offset_bytes > 8 {
            return Err(RunlistError::InvalidHeaderNibble { length_bytes, offset_bytes });
        }
        let length = read_unsigned_le(bytes, cursor, length_bytes as usize)
            .map_err(|got| RunlistError::LengthFieldTruncated { need: length_bytes as usize, got })?;
        cursor += length_bytes as usize;
        let lcn = if offset_bytes == 0 { None } else {
            let delta = read_signed_le(bytes, cursor, offset_bytes as usize)
                .map_err(|got| RunlistError::OffsetFieldTruncated { need: offset_bytes as usize, got })?;
            cursor += offset_bytes as usize;
            current_lcn = current_lcn.checked_add(delta)
                .ok_or(RunlistError::LcnOverflow { previous: current_lcn, delta })?;
            if current_lcn < 0 { return Err(RunlistError::NegativeLcn { got: current_lcn }); }
            Some(current_lcn as u64)
        };
        runs.push(Run { length_clusters: length, lcn });
    }
    Ok(runs)
}

/// runlist に従ってデータを読む。`read_clusters(lcn, count)` でクラスタ単位 I/O を抽象化
/// （テスト容易性優先）。スパースは `0x00` で埋め、最終的に `real_size` バイトでトリミング。
pub fn read_runs_with<F>(
    runs: &[Run], cluster_size: u64, real_size: u64, mut read_clusters: F,
) -> Result<Vec<u8>, RunlistError>
where F: FnMut(u64, u64) -> Result<Vec<u8>, std::io::Error>,
{
    if cluster_size == 0 { return Err(RunlistError::InvalidClusterSize { got: 0 }); }
    let total: u64 = runs.iter().map(|r| r.byte_length(cluster_size)).sum();
    if total < real_size {
        return Err(RunlistError::RealSizeMismatch { computed: total, declared: real_size });
    }
    let mut buf = Vec::with_capacity(real_size as usize);
    for run in runs {
        match run.lcn {
            Some(lcn) => buf.extend_from_slice(&read_clusters(lcn, run.length_clusters)?),
            None => buf.resize(buf.len() + run.byte_length(cluster_size) as usize, 0),
        }
    }
    buf.truncate(real_size as usize);
    Ok(buf)
}

/// 指定バイト数（1..=8）を符号なし LE で読む。失敗時は読めた長さを返す。
fn read_unsigned_le(bytes: &[u8], offset: usize, count: usize) -> Result<u64, usize> {
    if offset.checked_add(count).map_or(true, |e| e > bytes.len()) {
        return Err(bytes.len().saturating_sub(offset));
    }
    let mut buf = [0u8; 8];
    buf[..count].copy_from_slice(&bytes[offset..offset + count]);
    Ok(u64::from_le_bytes(buf))
}

/// 指定バイト数（1..=8）を符号付き LE で読む（**符号拡張あり**）。書籍 Chapter 13 が示す
/// 「offset は前ランとの差分、符号拡張必須」を実装。
fn read_signed_le(bytes: &[u8], offset: usize, count: usize) -> Result<i64, usize> {
    if offset.checked_add(count).map_or(true, |e| e > bytes.len()) {
        return Err(bytes.len().saturating_sub(offset));
    }
    let mut buf = [0u8; 8];
    buf[..count].copy_from_slice(&bytes[offset..offset + count]);
    if count > 0 && bytes[offset + count - 1] & 0x80 != 0 {
        for b in buf.iter_mut().skip(count) { *b = 0xFF; }
    }
    Ok(i64::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    // 書籍 Chapter 13 p.358-359 サンプル: `32 c0 1e b5 3a 05 21 70 1b 1f 00`
    // 第1run: 0x32 → L=2,O=3 / len=0x1ec0=7872, offset=0x053ab5=342709
    // 第2run: 0x21 → L=1,O=2 / len=0x70=112, delta=0x1f1b=+7963 → LCN=350672
    #[test] fn book_chapter13_runlist_example_two_runs() {
        let b = [0x32, 0xc0, 0x1e, 0xb5, 0x3a, 0x05, 0x21, 0x70, 0x1b, 0x1f, 0x00];
        let r = parse_runlist(&b).unwrap();
        assert_eq!(r, vec![
            Run { length_clusters: 7872, lcn: Some(342_709) },
            Run { length_clusters: 112, lcn: Some(350_672) }]);
    }
    #[test] fn single_run_then_end_marker() {
        assert_eq!(parse_runlist(&[0x11, 0x05, 0x0A, 0x00]).unwrap(),
            vec![Run { length_clusters: 5, lcn: Some(10) }]);
    }
    #[test] fn empty_runlist_immediate_end() {
        assert_eq!(parse_runlist(&[0x00]).unwrap(), Vec::<Run>::new());
    }
    #[test] fn unterminated_runlist_returns_buffer_too_small() {
        assert!(matches!(parse_runlist(&[0x11, 0x05, 0x0A]),
            Err(RunlistError::BufferTooSmall { .. })));
    }
    #[test] fn sparse_run_offset_bytes_zero() {
        let r = parse_runlist(&[0x01, 0x05, 0x00]).unwrap();
        assert_eq!(r, vec![Run { length_clusters: 5, lcn: None }]);
        assert!(r[0].is_sparse());
    }
    #[test] fn sparse_mixed_with_normal_runs() {
        // 通常(len=3,lcn=10) → スパース(len=2) → 通常(len=4,delta=+5,lcn=15)
        let r = parse_runlist(&[0x11, 0x03, 0x0A, 0x01, 0x02, 0x11, 0x04, 0x05, 0x00]).unwrap();
        assert_eq!(r, vec![
            Run { length_clusters: 3, lcn: Some(10) },
            Run { length_clusters: 2, lcn: None },
            Run { length_clusters: 4, lcn: Some(15) }]);
    }
    #[test] fn sign_extension_negative_one_byte_offset() {
        // L=1,O=1,len=5,offset=0xFF → -1 → NegativeLcn
        assert!(matches!(parse_runlist(&[0x11, 0x05, 0xFF, 0x00]),
            Err(RunlistError::NegativeLcn { got: -1 })));
    }
    #[test] fn sign_extension_three_byte_offset_high_bit_set() {
        // 第1run: len=5,offset=100 → LCN=100
        // 第2run: L=1,O=3,len=2,offset=0xFFFFFF (=-1) → LCN=99（3 バイト符号拡張）
        let r = parse_runlist(&[0x11, 0x05, 0x64, 0x31, 0x02, 0xFF, 0xFF, 0xFF, 0x00]).unwrap();
        assert_eq!(r[1], Run { length_clusters: 2, lcn: Some(99) });
    }
    #[test] fn invalid_header_nibble_length_zero_with_data() {
        assert!(matches!(parse_runlist(&[0xF0, 0xAA]),
            Err(RunlistError::InvalidHeaderNibble { length_bytes: 0, offset_bytes: 15 })));
    }
    #[test] fn invalid_header_nibble_offset_over_eight() {
        assert!(matches!(parse_runlist(&[0x91, 0x05, 0xAA]),
            Err(RunlistError::InvalidHeaderNibble { length_bytes: 1, offset_bytes: 9 })));
    }
    #[test] fn length_field_truncated_returns_specific_error() {
        assert!(matches!(parse_runlist(&[0x14, 0xAA, 0xBB]),
            Err(RunlistError::LengthFieldTruncated { need: 4, got: 2 })));
    }
    #[test] fn negative_lcn_after_subtraction_returns_negative_lcn_error() {
        // 第1run: LCN=100、第2run: delta=-200 (LE: 0x38 0xFF) → LCN=-100
        assert!(matches!(parse_runlist(&[0x11, 0x05, 0x64, 0x21, 0x02, 0x38, 0xFF, 0x00]),
            Err(RunlistError::NegativeLcn { got: -100 })));
    }
    #[test] fn read_runs_with_mock_reader_assembles_continuous_data() {
        let r = vec![Run { length_clusters: 2, lcn: Some(10) },
                     Run { length_clusters: 2, lcn: Some(20) }];
        let out = read_runs_with(&r, 4, 16, |lcn, c| {
            Ok(vec![if lcn == 10 { 0xAA } else { 0xBB }; (c * 4) as usize])
        }).unwrap();
        assert_eq!(&out[..8], &[0xAA; 8]); assert_eq!(&out[8..], &[0xBB; 8]);
    }
    #[test] fn read_runs_with_sparse_run_fills_zeros() {
        let r = vec![Run { length_clusters: 1, lcn: Some(10) },
                     Run { length_clusters: 2, lcn: None },
                     Run { length_clusters: 1, lcn: Some(20) }];
        let out = read_runs_with(&r, 2, 8, |_, c| Ok(vec![0xCC; (c * 2) as usize])).unwrap();
        assert_eq!(out, vec![0xCC, 0xCC, 0, 0, 0, 0, 0xCC, 0xCC]);
    }
    #[test] fn read_runs_with_truncates_to_real_size() {
        let r = vec![Run { length_clusters: 2, lcn: Some(0) }];
        let out = read_runs_with(&r, 4, 5, |_, c| Ok(vec![0x77; (c * 4) as usize])).unwrap();
        assert_eq!(out, vec![0x77; 5]);
    }
    #[test] fn read_runs_with_cluster_size_zero_returns_invalid_cluster_size() {
        let r = vec![Run { length_clusters: 1, lcn: Some(0) }];
        assert!(matches!(read_runs_with(&r, 0, 4, |_, _| Ok(vec![])),
            Err(RunlistError::InvalidClusterSize { got: 0 })));
    }
}
