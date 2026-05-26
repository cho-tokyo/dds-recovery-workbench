# Chunk 24c 指示: タイムスタンプ書き込みの高速化

実機ドライランで判明したパフォーマンス問題の根本原因 (タイムスタンプ書き込みのオーバーヘッド) を解消する小規模なチャンク。

> 🎯 完了時点で「ファイル毎の open/close 回数を半減」し、復旧速度を大幅改善する。

---

## 背景: 実機ドライランで判明したボトルネック

実機テスト (2 回目、Chunk 24a + 24b 後) の結果:

```
[全体]
1859 ファイル / 4.52 GB / 50:35 = 1.9 MB/s (悪化、Chunk 24a 前の 4 MB/s からさらに遅い)

[1GB の単一ファイル (rtrF63B.tmp)]
3 分で復旧 = 約 5.5 MB/s

[残り 1858 ファイル / 3.52 GB / 47 分]
= 約 1.3 MB/s、1 ファイル平均 1.5 秒

→ ボトルネックは「ファイル毎の処理オーバーヘッド」
→ データ転送速度ではなく、ファイル毎の open/close 回数
```

### 根本原因: タイムスタンプ書き込みでの再 open

Chunk 24a で追加した `apply_timestamps()` は、ファイル書き込み完了後に**再度ファイルを開いて**SetFileTime を呼ぶ:

```rust
// 現状 (遅い):
1. fs::File::create(path)        // ← 1 回目 open
2. file.write_all(content)
3. file.drop()                    // ← 1 回目 close

4. OpenOptions::new()...open(path) // ← 2 回目 open (再度!)
5. SetFileTime(handle, ...)
6. drop                            // ← 2 回目 close

→ 1858 ファイル × 2 回 open/close = 大幅なオーバーヘッド
```

### 修正方針

SetFileTime は **既に開いているファイルハンドル** に対して呼べる。再度 open する必要なし:

```rust
// 修正後 (速い):
1. fs::File::create(path)         // ← open (1 回のみ)
2. file.write_all(content)
3. SetFileTime(file.handle, ...)  // ← 同じハンドルで設定
4. file.drop()                     // ← close (1 回のみ)

→ ファイル毎の open/close が半減
```

### 期待効果

```
[現状] 1.9 MB/s (1.5 秒/ファイル)
[Chunk 24c 後] 推定 30-50 MB/s

理由:
- ファイル毎の Windows API 呼び出しが半減
- ハンドル再 open のオーバーヘッド消失
- 1GB ファイル (5.5 MB/s) と同等のペースに近づく
```

## 目的

1 つの集中した変更:

| Part | 内容 |
|---|---|
| **A** | `apply_timestamps_to_handle()` の新規追加 (open file handle 版) |
| **B** | `engine.rs` の write 処理を統合 (open → write → SetFileTime → close) |
| **C** | 既存の `apply_timestamps(path)` は維持 (後方互換性) |

## 対象クレート

- **修正**: `crates/recovery/`
- **影響テスト**: 既存テスト約 5 件 (タイムスタンプ関連)

## 重要な設計原則

### ファイル open 回数の最小化

```
✗ Chunk 24a の設計: file write 後、別途 open で SetFileTime
○ Chunk 24c の設計: file open 中に SetFileTime を実行
```

### 後方互換性の維持

既存の `apply_timestamps(path)` メソッドは保持。理由:
- 復旧フローの中で「書き込みエラー後にタイムスタンプだけ修正したい」等のリカバリで使う可能性
- テストコードが既存 API を呼んでいる
- 内部で `apply_timestamps_to_handle` を呼ぶ実装にする

### unsafe の追加なし

```
[現状の unsafe]
crates/recovery/src/timestamps.rs に 5-10 行 (Chunk 24a で追加)

[Chunk 24c 後]
同じ位置に同じ量の unsafe (関数の引数が変わるだけ)
追加の unsafe コードはなし
```

## 仕様参照

### ビジネス要件

- **FR-REC-09** (ファイル open 回数の最小化、性能改善) ← 新規達成
- **FR-REC-08** (復旧速度、目標 100 MB/s) ← 部分達成、まず 30-50 MB/s 到達を目指す

## 実装内容

### Part A: timestamps.rs の修正

`crates/recovery/src/timestamps.rs` を以下に修正:

```rust
//! NTFS タイムスタンプの保持 (Creation / Modified / Accessed).
//!
//! Windows の `SetFileTime` API を使用して、復旧したファイルに
//! 元のタイムスタンプを設定する。R-STUDIO 等の業界標準に準拠。
//!
//! ## API バリエーション
//!
//! 2 つの関数を提供:
//! - [`apply_timestamps_to_handle`]: 既に開いているファイルハンドルに設定 (高速、推奨)
//! - [`apply_timestamps`]: パスからファイルを開いて設定 (互換性、エラーリカバリ用)
//!
//! ## 安全性
//!
//! `unsafe` ブロックは `apply_timestamps_to_handle()` 内に限定。
//! 引数検証と RAII (`File` 構造体) で安全性を確保。

use std::fs::{File, OpenOptions};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg(windows)]
use windows_sys::Win32::Foundation::FILETIME;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    SetFileTime, FILE_WRITE_ATTRIBUTES,
};

/// タイムスタンプ書き込みのエラー
#[derive(Debug, Error)]
pub enum TimestampError {
    #[error("ファイルを開けません: {0}")]
    Open(#[from] std::io::Error),
    
    #[error("Windows API SetFileTime が失敗しました (エラーコード: {0})")]
    Win32Error(u32),
    
    #[error("時刻の変換に失敗しました: {0}")]
    TimeConversion(String),
    
    #[cfg(not(windows))]
    #[error("タイムスタンプ書き込みは Windows のみサポートしています")]
    Unsupported,
}

/// NTFS タイムスタンプ (3 種類)
#[derive(Debug, Clone, Copy)]
pub struct NtfsTimestamps {
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub accessed: DateTime<Utc>,
}

/// 既に開いているファイルハンドルにタイムスタンプを設定する (高速版).
///
/// 復旧処理中に「write 直後、close 前」に呼ぶことで、
/// ファイルの再 open を避けられる。Chunk 24c で導入。
///
/// ## 注意
///
/// `file` は `GENERIC_WRITE` または `FILE_WRITE_ATTRIBUTES` アクセスで
/// 開かれている必要がある。通常の `File::create()` で開いたファイルは
/// この条件を満たす。
///
/// Windows 以外では `TimestampError::Unsupported` を返す。
#[cfg(windows)]
pub fn apply_timestamps_to_handle(
    file: &File,
    timestamps: &NtfsTimestamps,
) -> Result<(), TimestampError> {
    // chrono::DateTime → FILETIME 変換
    let creation_ft = datetime_to_filetime(timestamps.created)?;
    let modified_ft = datetime_to_filetime(timestamps.modified)?;
    let accessed_ft = datetime_to_filetime(timestamps.accessed)?;
    
    let handle = file.as_raw_handle();
    
    // SAFETY:
    // - handle は `file: &File` から取得した有効なハンドル
    // - file の lifetime はこの関数のスコープ内で生存
    // - FILETIME 構造体は値型で、参照は有効
    // - SetFileTime は Windows API の標準的な使用方法
    let result = unsafe {
        SetFileTime(
            handle as *mut std::ffi::c_void,
            &creation_ft as *const FILETIME,
            &accessed_ft as *const FILETIME,
            &modified_ft as *const FILETIME,
        )
    };
    
    if result == 0 {
        // SAFETY: GetLastError は副作用のない Windows API
        let error_code = unsafe { 
            windows_sys::Win32::Foundation::GetLastError() 
        };
        return Err(TimestampError::Win32Error(error_code));
    }
    
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_timestamps_to_handle(
    _file: &File,
    _timestamps: &NtfsTimestamps,
) -> Result<(), TimestampError> {
    Err(TimestampError::Unsupported)
}

/// パスからファイルを開いてタイムスタンプを設定する (互換版).
///
/// ## いつ使うか
///
/// - エラーリカバリ: 書き込み完了後、別途タイムスタンプだけ修正したい場合
/// - 外部からの呼び出し: ファイルハンドルがない場合
///
/// ## 性能
///
/// この関数はファイルを再度 open するためオーバーヘッドが大きい。
/// 復旧処理中は [`apply_timestamps_to_handle`] を使うこと。
///
/// 内部で `apply_timestamps_to_handle` を呼ぶ。
#[cfg(windows)]
pub fn apply_timestamps(path: &Path, timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    let file = OpenOptions::new()
        .write(true)
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .open(path)?;
    
    apply_timestamps_to_handle(&file, timestamps)
}

#[cfg(not(windows))]
pub fn apply_timestamps(_path: &Path, _timestamps: &NtfsTimestamps) -> Result<(), TimestampError> {
    Err(TimestampError::Unsupported)
}

/// chrono::DateTime<Utc> を Windows FILETIME に変換する。
///
/// FILETIME は 1601-01-01 UTC からの 100 ナノ秒単位。
#[cfg(windows)]
fn datetime_to_filetime(dt: DateTime<Utc>) -> Result<FILETIME, TimestampError> {
    // UNIX epoch (1970-01-01) から Windows epoch (1601-01-01) までの差: 11644473600 秒
    const EPOCH_DIFFERENCE_SECONDS: i64 = 11_644_473_600;
    
    let unix_secs = dt.timestamp();
    let unix_nanos = dt.timestamp_subsec_nanos();
    
    let windows_seconds = unix_secs.checked_add(EPOCH_DIFFERENCE_SECONDS)
        .ok_or_else(|| TimestampError::TimeConversion(
            "時刻オーバーフロー".into()
        ))?;
    
    let filetime_100ns = windows_seconds.checked_mul(10_000_000)
        .and_then(|v| v.checked_add((unix_nanos / 100) as i64))
        .ok_or_else(|| TimestampError::TimeConversion(
            "FILETIME 換算オーバーフロー".into()
        ))?;
    
    let filetime_u64 = filetime_100ns as u64;
    
    Ok(FILETIME {
        dwLowDateTime: (filetime_u64 & 0xFFFFFFFF) as u32,
        dwHighDateTime: ((filetime_u64 >> 32) & 0xFFFFFFFF) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn ntfs_timestamps_struct_holds_three_dates() {
        let now = Utc::now();
        let ts = NtfsTimestamps {
            created: now,
            modified: now,
            accessed: now,
        };
        assert_eq!(ts.created, ts.modified);
    }
    
    #[cfg(windows)]
    #[test]
    fn datetime_to_filetime_roundtrip() {
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ft = datetime_to_filetime(dt).unwrap();
        
        let filetime_u64 = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        let windows_seconds = (filetime_u64 / 10_000_000) as i64;
        let unix_secs = windows_seconds - 11_644_473_600;
        
        assert_eq!(unix_secs, dt.timestamp());
    }
    
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_via_path_works() {
        use std::fs::write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        write(temp.path(), b"test").unwrap();
        
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };
        
        apply_timestamps(temp.path(), &ts).unwrap();
        
        let metadata = std::fs::metadata(temp.path()).unwrap();
        let modified_time = metadata.modified().unwrap();
        let modified_dt: DateTime<Utc> = modified_time.into();
        
        assert_eq!(modified_dt.timestamp(), dt.timestamp());
    }
    
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_to_open_handle_works() {
        // ★ 新規テスト: open file handle に対して直接設定
        use std::io::Write;
        let temp = tempfile::NamedTempFile::new().unwrap();
        
        // ファイルを書き込みモードで open したまま SetFileTime
        let mut file = OpenOptions::new()
            .write(true)
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .open(temp.path())
            .unwrap();
        file.write_all(b"test").unwrap();
        
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps {
            created: dt,
            modified: dt,
            accessed: dt,
        };
        
        // open 状態で SetFileTime
        apply_timestamps_to_handle(&file, &ts).unwrap();
        
        // close
        drop(file);
        
        // 確認
        let metadata = std::fs::metadata(temp.path()).unwrap();
        let modified_time = metadata.modified().unwrap();
        let modified_dt: DateTime<Utc> = modified_time.into();
        
        assert_eq!(modified_dt.timestamp(), dt.timestamp());
    }
    
    #[cfg(windows)]
    #[test]
    fn apply_timestamps_via_path_calls_handle_version() {
        // 既存の apply_timestamps が apply_timestamps_to_handle と
        // 同じ結果を出すことを確認
        use std::fs::write;
        let temp1 = tempfile::NamedTempFile::new().unwrap();
        let temp2 = tempfile::NamedTempFile::new().unwrap();
        write(temp1.path(), b"test1").unwrap();
        write(temp2.path(), b"test2").unwrap();
        
        let dt = DateTime::parse_from_rfc3339("2024-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ts = NtfsTimestamps { created: dt, modified: dt, accessed: dt };
        
        // path 版
        apply_timestamps(temp1.path(), &ts).unwrap();
        
        // handle 版
        let file = OpenOptions::new()
            .write(true)
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .open(temp2.path())
            .unwrap();
        apply_timestamps_to_handle(&file, &ts).unwrap();
        drop(file);
        
        // 両方とも同じ結果
        let m1 = std::fs::metadata(temp1.path()).unwrap().modified().unwrap();
        let m2 = std::fs::metadata(temp2.path()).unwrap().modified().unwrap();
        let dt1: DateTime<Utc> = m1.into();
        let dt2: DateTime<Utc> = m2.into();
        
        assert_eq!(dt1.timestamp(), dt2.timestamp());
        assert_eq!(dt1.timestamp(), dt.timestamp());
    }
}
```

### Part B: engine.rs の修正

`crates/recovery/src/engine.rs` の `process_recovery_task()` (Chunk 24b で追加された並列ワーカー関数) を修正:

#### 修正前 (Chunk 24a + 24b):

```rust
fn process_recovery_task(
    task: &RecoveryTask,
    config: &RecoveryConfig,
    wishlist: &Wishlist,
) -> ProcessedEntry {
    let output_path = compute_output_path(config, &task.file_meta);
    
    // 書き込み (open → write → close)
    let bytes_written = match write_with_large_buffer(&output_path, &task.content) {
        Ok(n) => n,
        Err(e) => return ProcessedEntry::Failed(...),
    };
    
    // SHA256, validation
    let sha256 = compute_sha256(&task.content);
    let validation = run_validators(&task.content, ...);
    
    // ★ ここで再度 open: タイムスタンプ書き込み
    let timestamps = NtfsTimestamps { ... };
    if let Err(e) = crate::timestamps::apply_timestamps(&output_path, &timestamps) {
        log::warn!("タイムスタンプ書き込み失敗: ...");
    }
    
    // ProcessedEntry::Success { ... }
}

fn write_with_large_buffer(path: &Path, content: &[u8]) -> std::io::Result<u64> {
    use std::io::{BufWriter, Write};
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let file = std::fs::File::create(path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    writer.write_all(content)?;
    writer.flush()?;
    
    Ok(content.len() as u64)
}
```

#### 修正後 (Chunk 24c):

```rust
fn process_recovery_task(
    task: &RecoveryTask,
    config: &RecoveryConfig,
    wishlist: &Wishlist,
) -> ProcessedEntry {
    let output_path = compute_output_path(config, &task.file_meta);
    
    let timestamps = NtfsTimestamps {
        created: task.file_meta.creation_time,
        modified: task.file_meta.modified_time,
        accessed: task.file_meta.accessed_time,
    };
    
    // ★ 書き込みとタイムスタンプを統合 (1 回の open のみ)
    let bytes_written = match write_with_timestamps(&output_path, &task.content, &timestamps) {
        Ok(n) => n,
        Err(e) => return ProcessedEntry::Failed(...),
    };
    
    // SHA256, validation (タイムスタンプとは独立、変更なし)
    let sha256 = compute_sha256(&task.content);
    let validation = run_validators(&task.content, ...);
    
    // ★ 削除: 別途 apply_timestamps(&output_path, ...) の呼び出し
    
    // ProcessedEntry::Success { ... }
}

/// ファイル書き込み + タイムスタンプ設定 を 1 回の open で実施.
///
/// Chunk 24c で導入。ファイル open 回数を半減し、復旧速度を改善する。
/// タイムスタンプ書き込みが失敗しても、ファイル書き込み自体は成功扱い (警告ログのみ).
fn write_with_timestamps(
    path: &Path,
    content: &[u8],
    timestamps: &NtfsTimestamps,
) -> std::io::Result<u64> {
    use std::io::{BufWriter, Write};
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // ★ open は 1 回のみ
    let file = std::fs::File::create(path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    writer.write_all(content)?;
    writer.flush()?;
    
    // ★ BufWriter から File ハンドルを取り出す
    let file = writer.into_inner()
        .map_err(|e| e.into_error())?;
    
    // ★ 同じ File ハンドルでタイムスタンプ設定 (再 open なし)
    if let Err(e) = crate::timestamps::apply_timestamps_to_handle(&file, timestamps) {
        log::warn!("タイムスタンプ書き込み失敗: {:?} ({})", path, e);
        // 書き込み自体は成功なので、エラーにはしない
    }
    
    // file は drop されて close される (1 回のみ)
    Ok(content.len() as u64)
}

// ★ 元の write_with_large_buffer は削除 or deprecated (移行が完了したら削除)
```

### Part C: lib.rs の公開 API 追加

`crates/recovery/src/lib.rs`:

```rust
pub use timestamps::{
    apply_timestamps,
    apply_timestamps_to_handle,  // ★ 新規エクスポート
    NtfsTimestamps,
    TimestampError,
};
```

## 単体テスト要件 (最低 3 件、新規)

`timestamps.rs` に追加:

1. `apply_timestamps_to_open_handle_works`: open file に対して直接設定できる
2. `apply_timestamps_via_path_calls_handle_version`: path 版と handle 版で同じ結果
3. `non_windows_returns_unsupported_error` (`#[cfg(not(windows))]`)

## 結合テスト要件

既存テストの確認のみ (新規追加は不要):

- `recovered_files_preserve_original_timestamps` (Chunk 24a で追加) → そのまま pass するはず

## 制約

- **行数目安**:
  - `recovery/src/timestamps.rs`: +50 行 (新関数 `apply_timestamps_to_handle`)、+30 行 (テスト)
  - `recovery/src/engine.rs`: +20 行 (`write_with_timestamps`)、-15 行 (旧コード削除)
  - `recovery/src/lib.rs`: +1 行 (export 追加)
  - 合計: 約 85 行追加・修正
- **単体テスト新規**: 最低 3 件
- **`unsafe` 追加行数**: 0 (関数間で移動しただけ)
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] `apply_timestamps_to_handle()` 関数が公開されている
- [ ] `engine.rs` の `process_recovery_task` で `apply_timestamps(path)` 呼び出しが削除されている
- [ ] `write_with_timestamps()` で 1 回の open でタイムスタンプ設定されている
- [ ] 既存の `apply_timestamps(path)` 関数は残っている (後方互換性)
- [ ] `unsafe` 行数が Chunk 24b から増えていない (5-10 行のまま)

## 関連 FR 要件

- **FR-REC-09** (ファイル open 回数の最小化、性能改善) ← 達成
- **FR-REC-08** (復旧速度、目標 100 MB/s) ← 部分達成見込み (30-50 MB/s 期待)

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **Chouさんが 3 回目の実機ドライランで速度を計測**
   - 期待値: 1.9 → 30-50 MB/s
   - 100 MB/s 未達なら Chunk 24d (追加最適化) を検討
   - 達成なら Phase 1.5 完成

---

## 注意事項

### `into_inner()` のエラーハンドリング

```rust
let file = writer.into_inner()
    .map_err(|e| e.into_error())?;
```

`BufWriter::into_inner()` は `Result<File, IntoInnerError<File>>` を返す。
`IntoInnerError` は flush 中のエラーを内包しており、`into_error()` で `std::io::Error` に変換可能。

flush は既に上の `writer.flush()` で実行しているので、ここで失敗する可能性は低いが、念のため対応。

### 並列化との関係

Chunk 24b の並列化 (crossbeam-channel + ワーカープール) は維持。
Chunk 24c でファイル毎のオーバーヘッドが減れば、並列化の効果がより見えるはず。

```
[現状 (24a + 24b)]
1.9 MB/s
ファイル毎のオーバーヘッドが支配的、並列化の恩恵が薄い

[Chunk 24c 後]
30-50 MB/s 期待
ファイル毎のオーバーヘッドが減り、並列化の恩恵が出る (SHA256/validator の並列実行)
```

### 並列化の維持 vs 廃止の判断

Chunk 24c 完了後の実機テストで判断:

```
[結果 1: 30-50 MB/s に改善]
→ Chunk 24c の修正で十分、並列化も機能している
→ Phase 1.5 完成判定 (業務的に許容範囲)

[結果 2: 10 MB/s 程度の改善]
→ ファイル毎オーバーヘッドはまだ残る
→ Chunk 24d で追加最適化 (例: SHA256/validation の遅延実行)

[結果 3: 改善なし or 悪化]
→ 別の原因 (NTFS read など)
→ プロファイリングで深堀調査
```

### `rtrF63B.tmp` の謎について

Chouさんから報告された「1 回目復旧時に検出されなかったが、2 回目で検出された 1GB の `.tmp` ファイル」:

```
[考えられる原因 (推測)]
- Windows のバックグラウンド処理 (Defender、Search Indexer 等) が一時ファイル作成
- 何らかのソフトウェアが HDD にアクセス
- NTFS の遅延書き込み

[業務的な影響]
- Phase 1.5 完成のクリティカルパスではない
- 本ファイルは Chunk 24c のスコープ外

[Chunk 24c 完了後の検証]
- 3 回目のドライランで同じ .tmp ファイルが検出されるか確認
- 検出されない場合、何かのプロセスが残骸を削除した可能性
```

これは現時点では追跡しない。

---

## 完了報告例

```markdown
## Chunk 24c 完了報告

### 修正ファイル
- crates/recovery/src/timestamps.rs (+50 行新関数、+30 行テスト)
- crates/recovery/src/engine.rs (write_with_timestamps 統合 +20 行、旧コード削除 -15 行)
- crates/recovery/src/lib.rs (+1 行 export)

### 新規 API
- `apply_timestamps_to_handle(&File, &NtfsTimestamps)` (高速版)
- `apply_timestamps(&Path, &NtfsTimestamps)` (既存、内部で handle 版を呼ぶ)

### unsafe 統計
- 全 workspace の unsafe 行数: 5-10 行 (Chunk 24a/24b から増加なし)

### テスト統計
- 単体: 既存 + 新規 3 件
- 結合: 既存維持
- 全 workspace: 全パス

### 期待されるパフォーマンス改善
- 現状 (Chunk 24a + 24b): 1.9 MB/s
- 期待値 (Chunk 24c): 30-50 MB/s
- 改善要因: ファイル毎の open/close 回数を半減
- 実機計測は Chouさんの 3 回目ドライランで確認

### 🎉 マイルストーン
- ボトルネック (タイムスタンプ書き込み再 open) を解消
- ファイル open 回数を 2 回 → 1 回に削減
- R-STUDIO 並みのファイル open 効率に近づいた

- **関連 FR**: FR-REC-09 (達成)、FR-REC-08 (部分達成)

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんによる 3 回目実機ドライラン
   - 同じ HDD (1858 ファイル / 4.52 GB) で復旧時間を計測
   - 期待: 30-50 MB/s
```
