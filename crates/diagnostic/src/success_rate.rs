//! Chunk 24d-4-1: 復旧成功率の予測ロジック。
//!
//! 全体的な復旧成功率を 100 から各リスク要因の減点で算出する。
//! Wishlist 指定時 (お客様希望リスト) は優先データの成功率も計算する。
//!
//! ## 業務原則
//!
//! 算出された数値は営業の説明用 (見積根拠) であり、絶対的な保証ではない。
//! 計算根拠 ([`SuccessRatePrediction::reasoning`]) を明示することで、
//! 営業はお客様に「なぜこの数字なのか」を業務的に説明できる。
//!
//! 関連 FR: FR-DIAG-07 (復旧成功率予測)。

use dds_case_manager::{BitLockerStatus, DirtyBitStatus, LogFileStatus, SuccessRatePrediction};

/// 起点の成功率 (満点)。
const BASE_SUCCESS_RATE: i32 = 100;
/// BitLocker 暗号化検出時の減点 (回復キーが必要なため大幅減点)。
const DEDUCTION_BITLOCKER: i32 = 90;
/// MFT 破損率からの最大減点。
const DEDUCTION_MFT_MAX: i32 = 50;
/// Dirty Bit 検出時の減点 (技術的にはほぼ問題なく復旧可能)。
const DEDUCTION_DIRTY_BIT: i32 = 2;
/// `$LogFile` 不整合検出時の減点。
const DEDUCTION_LOGFILE: i32 = 5;
/// 優先データ (Wishlist) に与えるボーナス (重要度の高いデータは個別チェックされやすい)。
const PRIORITY_BONUS: u16 = 5;

/// 復旧成功率を計算する。
///
/// 各リスク要因に応じて 100 から減点していき、0 を下限とする u8 で返す。
/// `reasoning` には減点の理由を業務的な日本語で残し、営業の説明用とする。
///
/// 関連 FR: FR-DIAG-07。
pub fn predict_success_rate(
    mft_corruption_count: u64,
    total_mft_entries: u64,
    dirty_bit: DirtyBitStatus,
    log_file: LogFileStatus,
    bitlocker: BitLockerStatus,
    has_wishlist: bool,
) -> SuccessRatePrediction {
    let mut overall: i32 = BASE_SUCCESS_RATE;
    let mut reasoning: Vec<String> = Vec::new();

    // BitLocker 暗号化 (大幅減点)
    if matches!(bitlocker, BitLockerStatus::Encrypted) {
        overall -= DEDUCTION_BITLOCKER;
        reasoning.push(format!(
            "BitLocker 暗号化を検出 (-{}%、回復キー必須)",
            DEDUCTION_BITLOCKER
        ));
    }

    // MFT 破損 (破損率から最大 DEDUCTION_MFT_MAX %)
    if mft_corruption_count > 0 && total_mft_entries > 0 {
        let corruption_rate =
            (mft_corruption_count as f64 / total_mft_entries as f64 * 100.0) as i32;
        let deduction = corruption_rate.min(DEDUCTION_MFT_MAX);
        if deduction > 0 {
            overall -= deduction;
            reasoning.push(format!("MFT エントリ破損 (-{}%)", deduction));
        }
    }

    // Dirty Bit (軽微)
    if matches!(dirty_bit, DirtyBitStatus::Dirty) {
        overall -= DEDUCTION_DIRTY_BIT;
        reasoning.push(format!("Dirty Bit あり (-{}%)", DEDUCTION_DIRTY_BIT));
    }

    // $LogFile 不整合 (軽微)
    if matches!(log_file, LogFileStatus::Inconsistent) {
        overall -= DEDUCTION_LOGFILE;
        reasoning.push(format!("$LogFile 不整合 (-{}%)", DEDUCTION_LOGFILE));
    }

    // 0 でフロアリング
    let overall = overall.max(0) as u8;

    // 優先データ (Wishlist 指定時): 全体 + ボーナス、上限 100。
    let priority_rate = if has_wishlist {
        Some((overall as u16 + PRIORITY_BONUS).min(100) as u8)
    } else {
        None
    };

    SuccessRatePrediction {
        overall_rate: overall,
        priority_rate,
        reasoning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_drive_high_success_rate() {
        let pred = predict_success_rate(
            0,
            10_000,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            false,
        );
        assert_eq!(pred.overall_rate, 100);
        assert!(pred.priority_rate.is_none());
        assert!(pred.reasoning.is_empty());
    }

    #[test]
    fn bitlocker_severe_deduction() {
        let pred = predict_success_rate(
            0,
            10_000,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::Encrypted,
            false,
        );
        assert_eq!(pred.overall_rate, 10); // 100 - 90
        assert!(pred.reasoning.iter().any(|r| r.contains("BitLocker")));
    }

    #[test]
    fn dirty_bit_small_deduction() {
        let pred = predict_success_rate(
            0,
            10_000,
            DirtyBitStatus::Dirty,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            false,
        );
        assert_eq!(pred.overall_rate, 98); // 100 - 2
    }

    #[test]
    fn wishlist_provides_priority_rate() {
        let pred = predict_success_rate(
            0,
            10_000,
            DirtyBitStatus::Clean,
            LogFileStatus::Consistent,
            BitLockerStatus::NotEncrypted,
            true,
        );
        assert!(pred.priority_rate.is_some());
        // 100 + 5 だが上限 100 でクランプされる
        assert_eq!(pred.priority_rate.unwrap(), 100);
    }

    #[test]
    fn reasoning_includes_deduction_reasons() {
        let pred = predict_success_rate(
            5,
            1000,
            DirtyBitStatus::Dirty,
            LogFileStatus::Inconsistent,
            BitLockerStatus::NotEncrypted,
            false,
        );
        assert!(!pred.reasoning.is_empty());
        assert!(pred.reasoning.iter().any(|r| r.contains("Dirty Bit")));
        assert!(pred.reasoning.iter().any(|r| r.contains("$LogFile")));
    }

    #[test]
    fn extreme_deductions_floor_at_zero() {
        // BitLocker (-90) + MFT 破損率 100% (capped at -50) + Dirty (-2) + LogFile (-5)
        // = -147 → floor 0
        let pred = predict_success_rate(
            1000,
            1000,
            DirtyBitStatus::Dirty,
            LogFileStatus::Inconsistent,
            BitLockerStatus::Encrypted,
            false,
        );
        assert_eq!(pred.overall_rate, 0, "should floor at 0");
    }
}
