//! `show` サブコマンド: 案件情報を表示する。

use anyhow::Result;
use dds_case_manager::CaseStorage;
use dds_core::format::format_bytes;

use crate::prompts::prompt_case_id;

/// `show` サブコマンドのエントリーポイント。
///
/// 案件 JSON を読み込み、3 セクション (診断 / Wishlist / 復旧) を整形表示する。
/// 未実施のセクションは「未実施」「未作成」と表示する。
pub fn run() -> Result<()> {
    println!("案件情報の表示");
    println!("---------------------------------------------");
    println!();

    let case_id = prompt_case_id()?;
    let storage = CaseStorage::default_location();
    let case = storage.load(&case_id)?;

    println!();
    println!("案件番号: {}", case.case_id);
    println!("作成日時: {}", case.created_at.format("%Y-%m-%d %H:%M"));
    println!("最終更新: {}", case.updated_at.format("%Y-%m-%d %H:%M"));
    println!();

    // 診断結果
    if case.diagnostic_input.diagnosed_at.is_some() {
        println!("【診断結果】");
        if let Some(at) = case.diagnostic_input.diagnosed_at {
            println!("  実施日時:     {}", at.format("%Y-%m-%d %H:%M"));
        }
        if let Some(secs) = case.diagnostic_input.duration_secs {
            println!("  診断時間:     {} 秒", secs);
        }
        if let Some(fs) = &case.diagnostic_input.filesystem_type {
            println!("  FS:           {}", fs);
        }
        println!(
            "  全ファイル:   {} 件",
            case.diagnostic_input.total_files
        );
        println!(
            "  削除ファイル: {} 件",
            case.diagnostic_input.deleted_files
        );
        if let Some(findings) = &case.diagnostic_input.filesystem_findings {
            if findings.has_any_issue() {
                println!("  破損検出:     あり");
                if !findings.signature_valid {
                    println!("    ・ NTFS シグネチャ異常");
                }
                if findings.mft_corrupted_count > 0 {
                    println!("    ・ MFT 破損 {} 件", findings.mft_corrupted_count);
                }
                if findings.invalid_runlist_count > 0 {
                    println!(
                        "    ・ 不正 run-list {} 件",
                        findings.invalid_runlist_count
                    );
                }
            } else {
                println!("  破損検出:     なし");
            }
        }
        println!();
    } else {
        println!("【診断結果】 未実施");
        println!();
    }

    // Wishlist
    if let Some(wishlist) = &case.wishlist {
        println!("【Wishlist】 {} 件", wishlist.len());
        for (i, wish) in wishlist.wishes.iter().enumerate() {
            println!("  {}: 「{}」 ({:?})", i + 1, wish.label, wish.priority);
        }
        println!();
    } else {
        println!("【Wishlist】 未作成");
        println!();
    }

    // 復旧結果
    if let Some(summary) = &case.recovery_report_summary {
        println!("【復旧結果】");
        println!("  該当ファイル: {} 件", summary.total_matched);
        println!(
            "  復旧成功:     {} 件 ({:.1}%)",
            summary.recovered_count, summary.recovery_success_rate
        );
        println!("  品質保証率:   {:.1}%", summary.quality_assurance_rate);
        println!(
            "  復旧データ量: {}",
            format_bytes(summary.total_bytes_written)
        );
        if let Some(output) = &case.output_dir {
            println!("  出力先:       {}", output.display());
        }
        println!();
    } else {
        println!("【復旧結果】 未実施");
        println!();
    }

    Ok(())
}
