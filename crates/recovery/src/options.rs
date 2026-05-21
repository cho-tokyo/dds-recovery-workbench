//! Chunk 17: 復旧時の動作オプション。
//!
//! 衝突戦略 / 削除マーカー有効化 / 生存削除分離 / SHA256 計算 / サイズ上限を
//! まとめて指定する。デフォルトは「業務的に安全側」（リネーム + 削除マーカー +
//! サブディレクトリ分離 + SHA256 計算 + サイズ無制限）。

/// `RecoveryEngine` の動作オプション。
///
/// 業務側で挙動を切り替える必要がある項目を網羅。お客様要件で
/// 「上書きしてでも復旧したい」「サイズ上限を設けたい」等の要望に応える。
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    /// 同名ファイル衝突時の戦略。
    pub conflict_strategy: ConflictStrategy,

    /// 削除ファイルのファイル名に識別子（例: `(deleted-#67)`）を埋め込むか。
    pub mark_deleted_in_filename: bool,

    /// 生存 / 削除を `live/` と `deleted/` サブディレクトリで分離するか。
    pub separate_live_and_deleted: bool,

    /// 復旧した各ファイルの SHA256 を計算してレポートに含めるか。
    pub compute_sha256: bool,

    /// このサイズ（バイト）を超えるファイルはスキップ。`None` で無制限。
    pub max_file_size_bytes: Option<u64>,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Rename,
            mark_deleted_in_filename: true,
            separate_live_and_deleted: true,
            compute_sha256: true,
            max_file_size_bytes: None,
        }
    }
}

/// 同名ファイル衝突時の処理方針。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// 連番付与でリネーム (`foo.txt` → `foo (1).txt`)、デフォルト。
    Rename,
    /// 既存ファイルを上書き（要注意、お客様明示要求時のみ）。
    Overwrite,
    /// スキップしてレポートの `skipped` に記録。
    Skip,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_are_business_safe() {
        // デフォルトは業務的に安全側を選択していることを保証する回帰テスト。
        let opt = RecoveryOptions::default();
        assert_eq!(opt.conflict_strategy, ConflictStrategy::Rename);
        assert!(opt.mark_deleted_in_filename);
        assert!(opt.separate_live_and_deleted);
        assert!(opt.compute_sha256);
        assert!(opt.max_file_size_bytes.is_none());
    }

    #[test]
    fn conflict_strategy_is_copy() {
        // Copy 制約が壊れていないことの保証（業務コードで値渡し多用）。
        let s = ConflictStrategy::Overwrite;
        let s2 = s;
        assert_eq!(s, s2);
    }

    #[test]
    fn options_can_be_customized() {
        let opt = RecoveryOptions {
            conflict_strategy: ConflictStrategy::Skip,
            mark_deleted_in_filename: false,
            separate_live_and_deleted: false,
            compute_sha256: false,
            max_file_size_bytes: Some(100 * 1024 * 1024),
        };
        assert_eq!(opt.conflict_strategy, ConflictStrategy::Skip);
        assert_eq!(opt.max_file_size_bytes, Some(100 * 1024 * 1024));
    }
}
