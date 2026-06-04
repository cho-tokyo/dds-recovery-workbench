//! Chunk 24d-4-1: 復旧難易度の評価ロジック。
//!
//! 各種診断結果 (FS 構造健全性 / MFT 破損件数 / Dirty Bit / $LogFile / BitLocker /
//! ファイル数推定) から、4 段階 (易/中/難/注意) の [`RecoveryDifficulty`] を導出する。
//!
//! ## 業務原則
//!
//! - 「受注不可」のような決めつけ表現は使わない (case-manager 側 enum で保証)
//! - 完全な FS 構造破壊もカービングで復旧可能 → 「難」扱い (決して「不可」ではない)
//! - 物理障害の兆候は「注意」 → 受注可否は人間が判断
//!
//! 関連 FR: FR-DIAG-06 (難易度評価)。

use dds_case_manager::{
    BitLockerStatus, DirtyBitStatus, FileEstimation, LogFileStatus, RecoveryDifficulty,
};

/// 「注意」レベル判定の閾値: 推定ファイル数が 0 件のみ。
///
/// Phase 1.5 では S.M.A.R.T. 等の物理検出がないため、業務的に
/// 「MFT を走査しても 1 件もファイルが見つからない」状態を物理障害の兆候とみなす。
const CAUTION_NO_FILES_THRESHOLD: u64 = 0;

/// 「難」レベル判定の閾値: MFT エントリ破損件数。
const HARD_MFT_CORRUPTION_THRESHOLD: u64 = 100;

/// 「中」レベル判定の閾値: 削除ファイル件数。
const MEDIUM_DELETED_FILES_THRESHOLD: u64 = 100;

/// 各種診断結果から復旧難易度を判定する。
///
/// 判定優先度: 注意 → 難 → 中 → 易。最初に該当した条件で確定する。
///
/// - **注意**: 物理障害の兆候 (推定総ファイル数 = 0)
/// - **難**: BitLocker 暗号化、または FS 構造非健全、または MFT 破損 100 件超
/// - **中**: Dirty Bit、$LogFile 不整合、削除 100 件超、または MFT 破損あり
/// - **易**: 上記いずれにも該当しない (標準業務ケース)
///
/// 関連 FR: FR-DIAG-06。
pub fn evaluate_difficulty(
    fs_structure_ok: bool,
    mft_corruption_count: u64,
    dirty_bit: DirtyBitStatus,
    log_file: LogFileStatus,
    bitlocker: BitLockerStatus,
    file_estimation: &FileEstimation,
) -> RecoveryDifficulty {
    // 注意レベル (物理障害の兆候、人間が判断)
    if file_estimation.estimated_total_files == CAUTION_NO_FILES_THRESHOLD {
        return RecoveryDifficulty::Caution;
    }

    // 難レベル (BitLocker / FS 構造破壊 / 大規模 MFT 破損)
    if matches!(bitlocker, BitLockerStatus::Encrypted)
        || !fs_structure_ok
        || mft_corruption_count > HARD_MFT_CORRUPTION_THRESHOLD
    {
        return RecoveryDifficulty::Hard;
    }

    // 中レベル (部分障害、業務的に標準範囲)
    if matches!(dirty_bit, DirtyBitStatus::Dirty)
        || matches!(log_file, LogFileStatus::Inconsistent)
        || file_estimation.estimated_deleted_files > MEDIUM_DELETED_FILES_THRESHOLD
        || mft_corruption_count > 0
    {
        return RecoveryDifficulty::Medium;
    }

    // 易レベル
    RecoveryDifficulty::Easy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_estimation(total: u64, deleted: u64) -> FileEstimation {
        FileEstimation {
            estimated_total_files: total,
            estimated_deleted_files: deleted,
            estimated_live_files: total.saturating_sub(deleted),
        }
    }

    #[test]
    fn evaluate_easy() {
        let d = evaluate_difficulty(
            true,
            0,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Easy);
    }

    #[test]
    fn evaluate_medium_dirty_bit() {
        let d = evaluate_difficulty(
            true,
            0,
            DirtyBitStatus::Dirty,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Medium);
    }

    #[test]
    fn evaluate_hard_bitlocker() {
        let d = evaluate_difficulty(
            true,
            0,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::Encrypted,
            &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Hard);
    }

    #[test]
    fn evaluate_hard_structure_broken() {
        // 業務原則: FS 構造破壊でもファイル単位の復旧は可能 → Hard、決して Caution / 不可ではない。
        let d = evaluate_difficulty(
            false,
            0,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            &make_estimation(1000, 5),
        );
        assert_eq!(d, RecoveryDifficulty::Hard);
    }

    #[test]
    fn evaluate_caution_no_files() {
        let d = evaluate_difficulty(
            true,
            0,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            &make_estimation(0, 0),
        );
        assert_eq!(d, RecoveryDifficulty::Caution);
    }

    #[test]
    fn business_explanation_includes_human_judgment() {
        // 業務原則テスト: Caution 説明には必ず「人間が判断」を含む。
        let caution = RecoveryDifficulty::Caution;
        assert!(
            caution.business_explanation().contains("人間が判断"),
            "explanation: {}",
            caution.business_explanation()
        );
    }
}
