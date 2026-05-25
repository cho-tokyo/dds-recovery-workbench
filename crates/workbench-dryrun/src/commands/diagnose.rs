//! `diagnose` サブコマンド: 案件を作成/更新し、対象 HDD を診断する。

use anyhow::{anyhow, Context, Result};

use dds_case_manager::CaseStorage;
use dds_core::format::format_bytes;
use dds_diagnostic::DiagnosticEngine;

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number};
use crate::volume::open_ntfs_volume;

/// `diagnose` サブコマンドのエントリーポイント。
///
/// 手順:
/// 1. 案件番号の入力
/// 2. 既存案件があれば更新可否を確認
/// 3. NTFS かつ非システムの論理ドライブから対象を選択
/// 4. 確認後 `DiagnosticEngine::diagnose` を実行
/// 5. CRM 貼り付け用テキストをファイル保存
/// 6. `case.diagnostic_input` を更新し `CaseStorage::save`
pub fn run() -> Result<()> {
    println!("診断モード");
    println!("---------------------------------------------");
    println!();

    // Step 1: 案件番号入力
    let case_id = prompt_case_id()?;
    println!();

    // Step 2: 案件作成 or 既存案件チェック
    let storage = CaseStorage::default_location();
    let case_exists = storage.case_file_path(&case_id).exists();
    let mut case = if case_exists {
        println!("この案件はすでに存在しています: {}", case_id);
        if !confirm("既存の案件を更新しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
        storage.load(&case_id)?
    } else {
        storage.create_new(case_id.clone())?
    };

    // Step 3: 対象ドライブの選択
    println!();
    println!("接続中の NTFS ドライブ:");
    let drives: Vec<_> = list_drives()
        .into_iter()
        .filter(|d| d.is_ntfs() && !d.is_system_drive())
        .collect();

    if drives.is_empty() {
        return Err(anyhow!(
            "操作可能な NTFS ドライブが見つかりませんでした。\n\
             システムドライブ (C:) は対象外です。"
        ));
    }

    for (i, drive) in drives.iter().enumerate() {
        println!(
            "  [{}] {} ({}, {})",
            i + 1,
            drive.drive_letter,
            drive.label,
            format_bytes(drive.total_bytes)
        );
    }
    println!();

    let selection = prompt_number("診断対象を選択", 1, drives.len())?;
    let selected_drive = &drives[selection - 1];

    // Step 4: 確認
    println!();
    println!("確認:");
    println!("  案件番号:     {}", case_id);
    println!(
        "  対象ドライブ: {} ({})",
        selected_drive.drive_letter, selected_drive.label
    );
    println!("  アクセスパス: {}", selected_drive.access_path);
    println!();

    if !confirm("診断を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }

    // Step 5: 診断実行
    println!();
    println!("[診断中... MFT 読み取り中、少々お待ちください]");
    let start = std::time::Instant::now();

    let mut volume = open_ntfs_volume(&selected_drive.access_path)?;
    let report = DiagnosticEngine::diagnose(&mut volume, case_id.clone())
        .context("診断の実行に失敗しました")?;

    let elapsed = start.elapsed();
    println!("[診断完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();

    // Step 6: 結果サマリ表示
    println!("結果サマリ:");
    println!("  全ファイル:    {} 件", report.file_stats.total_files);
    println!("  通常 (生存):   {} 件", report.file_stats.live_files);
    println!("  削除済み:      {} 件", report.file_stats.deleted_files);
    if !report.filesystem_findings.signature_valid {
        println!("  ファイルシステム署名異常を検出");
    }
    if report.filesystem_findings.mft_corrupted_count > 0 {
        println!(
            "  MFT エントリ破損: {} 件",
            report.filesystem_findings.mft_corrupted_count
        );
    }
    println!();

    // Step 7: CRM 貼り付けテキスト生成と保存
    let crm_text = report.to_crm_text();
    let case_dir = storage.case_dir(&case_id);
    std::fs::create_dir_all(&case_dir)?;
    let crm_text_path = case_dir.join("診断結果_CRM貼り付け用.txt");
    std::fs::write(&crm_text_path, &crm_text)
        .with_context(|| format!("CRM テキストの書き出しに失敗: {}", crm_text_path.display()))?;

    println!("CRM 貼り付けテキスト:");
    println!("---------------------------------------------");
    println!("{}", crm_text);
    println!("---------------------------------------------");
    println!();

    // Step 8: case.json 更新
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(&case)?;

    println!("保存先:");
    println!(
        "  案件 JSON:      {}",
        storage.case_file_path(&case_id).display()
    );
    println!("  CRM 貼り付け用: {}", crm_text_path.display());
    println!();
    println!("診断完了。CRM 貼り付けテキストをコピーして CRM に貼り付けてください。");

    Ok(())
}
