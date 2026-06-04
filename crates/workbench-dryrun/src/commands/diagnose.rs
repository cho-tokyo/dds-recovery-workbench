//! `diagnose` サブコマンド: 案件を作成/更新し、対象 HDD を診断する。
//!
//! Chunk 24d-3 拡張: `--physical N --partition M` で物理ドライブのパーティションを
//! 直接診断可能 (壊れた FS / マウント不能 HDD 対応)。引数なしの呼び出しは従来通り
//! 論理ドライブモード。

use anyhow::{anyhow, Context, Result};
use clap::Args;

use dds_case_manager::{Case, CaseId, CaseStorage};
use dds_core::format::format_bytes;
use dds_diagnostic::{DiagnosticEngine, DiagnosticReport};
use dds_disk_io::{enumerate_physical_drives, FsType, PhysicalDrive, PhysicalPartitionReader};

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number};
use crate::volume::{open_ntfs_volume, open_ntfs_volume_from_partition};

/// `diagnose` サブコマンドの引数 (Chunk 24d-3 で追加)。
///
/// 引数なし: 既存の論理ドライブモード (`E:` / `F:` などマウント済み NTFS)。
/// `--physical N --partition M`: 物理ドライブのパーティションを直接診断。
#[derive(Args, Debug, Default)]
pub struct DiagnoseArgs {
    /// 物理ドライブ番号 (例: `1` → `\\.\PhysicalDrive1`)
    #[arg(long)]
    pub physical: Option<u32>,

    /// パーティション番号 (1 ベース、`--physical` と共に指定)
    #[arg(long)]
    pub partition: Option<u32>,
}

/// `diagnose` サブコマンドのエントリーポイント。
///
/// `args.physical` / `args.partition` の組合せでモードを切替える。
/// 両方指定で物理モード、両方未指定で論理モード、片方だけはエラー。
pub fn run(args: &DiagnoseArgs) -> Result<()> {
    println!("診断モード");
    println!("---------------------------------------------");
    println!();

    match (args.physical, args.partition) {
        (Some(drive_num), Some(part_num)) => run_physical(drive_num, part_num),
        (Some(_), None) => Err(anyhow!(
            "--physical を指定する場合は --partition も必要です"
        )),
        (None, Some(_)) => Err(anyhow!(
            "--partition を指定する場合は --physical も必要です"
        )),
        (None, None) => run_logical(),
    }
}

/// 論理ドライブモード (既存の挙動を完全に維持)。
fn run_logical() -> Result<()> {
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

    finalize_diagnose(&storage, &mut case, &case_id, report)
}

/// 物理ドライブモード (Chunk 24d-3 新規)。
///
/// `\\.\PhysicalDriveN` を open → パーティション一覧 → 指定パーティション選択 →
/// 生バイトリーダ経由で `NtfsVolume::open` → 診断という流れ。マウント不能 / 壊れた
/// FS の HDD でもパーティションが見えていれば診断可能。
fn run_physical(drive_num: u32, part_num: u32) -> Result<()> {
    println!("物理ドライブモード:");
    println!("  物理ドライブ: \\\\.\\PhysicalDrive{}", drive_num);
    println!("  パーティション: {}", part_num);
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

    // Step 3: 物理ドライブを列挙して該当ドライブを探す
    let drives = enumerate_physical_drives();
    let drive_info = drives
        .iter()
        .find(|d| d.drive_number == drive_num)
        .ok_or_else(|| {
            anyhow!(
                "物理ドライブ {} が見つかりません。\n\
                 `list-drives --physical` で接続中の物理ドライブを確認してください。",
                drive_num
            )
        })?
        .clone();

    println!();
    println!("ドライブ情報:");
    println!("  パス:      {}", drive_info.path.display());
    println!("  サイズ:    {}", format_bytes(drive_info.total_bytes));
    if let Some(vendor) = &drive_info.vendor_id {
        println!("  Vendor:    {}", vendor);
    }
    if let Some(product) = &drive_info.product_id {
        println!("  Product:   {}", product);
    }
    println!();

    // Step 4: パーティション一覧
    let drive = PhysicalDrive::open(&drive_info.path)
        .with_context(|| format!("物理ドライブ {} を open できません", drive_num))?;
    let partitions = drive
        .list_partitions()
        .context("パーティション情報を取得できません")?;

    if partitions.is_empty() {
        return Err(anyhow!(
            "パーティションが検出されませんでした。\n\
             有効なパーティションテーブル (MBR/GPT) が存在しない可能性があります。"
        ));
    }

    let partition = partitions
        .iter()
        .find(|p| p.number == part_num)
        .ok_or_else(|| {
            let available = partitions
                .iter()
                .map(|p| p.number.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!(
                "パーティション {} が見つかりません。利用可能: {}",
                part_num,
                available
            )
        })?
        .clone();

    println!("選択されたパーティション:");
    println!(
        "  パーティション {}: {}, {}, {}",
        partition.number,
        partition.partition_type.display_name(),
        format_bytes(partition.size),
        partition.fs_type.display_name(),
    );
    println!();

    if partition.fs_type != FsType::Ntfs {
        return Err(anyhow!(
            "選択されたパーティションは {} です。\n\
             Phase 1.5 では NTFS のみ復旧/診断可能です。",
            partition.fs_type.display_name()
        ));
    }

    // Step 5: 確認
    println!("確認:");
    println!("  案件番号:       {}", case_id);
    println!(
        "  物理パーティション: \\\\.\\PhysicalDrive{} Partition {}",
        drive_num, part_num
    );
    println!();

    if !confirm("診断を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }

    // Step 6: 物理ドライブを再 open してパーティション reader を作る
    //         (`drive` は list_partitions のために move 済み)
    let drive_for_reader =
        PhysicalDrive::open(&drive_info.path).context("物理ドライブの再 open に失敗しました")?;
    let partition_reader =
        PhysicalPartitionReader::new(drive_for_reader, partition.start_offset, partition.size);

    println!();
    println!("[NTFS ボリュームを open しています...]");
    let mut volume = open_ntfs_volume_from_partition(partition_reader).map_err(|e| {
        anyhow!(
            "NTFS ボリュームを open できませんでした。\n\
             原因: {:#}\n\
             \n\
             考えられる状況:\n\
             \u{3000}• NTFS の管理領域 ($MFT) が破損している\n\
             \u{3000}• パーティションテーブルは残っているが、FS が深刻に壊れている\n\
             \u{3000}• 別ツール (R-STUDIO 等) での復旧をご検討ください",
            e
        )
    })?;
    println!("NTFS ボリューム open 成功");
    println!();

    // Step 7: 診断実行
    println!("[診断中... MFT 読み取り中、少々お待ちください]");
    let start = std::time::Instant::now();
    let report = DiagnosticEngine::diagnose(&mut volume, case_id.clone())
        .context("診断の実行に失敗しました")?;
    let elapsed = start.elapsed();
    println!("[診断完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();

    finalize_diagnose(&storage, &mut case, &case_id, report)
}

/// 診断結果を表示し、CRM テキストを保存して `case.json` を更新する共通処理。
///
/// 論理 / 物理どちらのモードからも呼ばれる (Chunk 24d-3)。
/// Chunk 24d-4-1: 業務サマリ + 技術詳細 + 業務的評価セクションを追加。
fn finalize_diagnose(
    storage: &CaseStorage,
    case: &mut Case,
    case_id: &CaseId,
    report: DiagnosticReport,
) -> Result<()> {
    // 結果サマリ表示
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

    // Chunk 24d-4-1: 業務管理用の追加セクション表示
    show_business_diagnostic_summary(&report);

    // CRM 貼り付けテキスト生成と保存
    let crm_text = report.to_crm_text();
    let case_dir = storage.case_dir(case_id);
    std::fs::create_dir_all(&case_dir)?;
    let crm_text_path = case_dir.join("診断結果_CRM貼り付け用.txt");
    std::fs::write(&crm_text_path, &crm_text)
        .with_context(|| format!("CRM テキストの書き出しに失敗: {}", crm_text_path.display()))?;

    println!("CRM 貼り付けテキスト:");
    println!("---------------------------------------------");
    println!("{}", crm_text);
    println!("---------------------------------------------");
    println!();

    // case.json 更新
    case.diagnostic_input = report.to_diagnostic_input();
    storage.save(case)?;

    println!("保存先:");
    println!(
        "  案件 JSON:      {}",
        storage.case_file_path(case_id).display()
    );
    println!("  CRM 貼り付け用: {}", crm_text_path.display());
    println!();
    println!("診断完了。CRM 貼り付けテキストをコピーして CRM に貼り付けてください。");

    Ok(())
}

/// Chunk 24d-4-1: 業務管理用の診断サマリを CLI に表示する。
///
/// 業務情報がいずれか 1 つでも存在する場合のみ各セクションを出力する。
/// 既存の論理ドライブ表示パスを破壊しないよう、`finalize_diagnose` から呼ばれる。
fn show_business_diagnostic_summary(report: &DiagnosticReport) {
    let has_mount_info = report.dirty_bit.is_some()
        || report.log_file_status.is_some()
        || report.bitlocker.is_some();
    let has_business = report.file_estimation.is_some()
        || report.recovery_difficulty.is_some()
        || report.success_rate.is_some();
    if !has_mount_info && !has_business {
        return;
    }

    if has_mount_info {
        println!("【Windows のマウント状態】");
        if let Some(dirty) = &report.dirty_bit {
            println!("  Dirty Bit:            {}", dirty.business_message());
        }
        if let Some(log) = &report.log_file_status {
            println!("  $LogFile 整合性:     {}", log.business_message());
        }
        if let Some(bl) = &report.bitlocker {
            println!("  BitLocker 暗号化:    {}", bl.business_message());
        }
        println!();
    }

    if has_business {
        println!("【業務的な評価】");
        if let Some(est) = &report.file_estimation {
            println!("  {}", est.business_summary());
        }
        if let Some(diff) = &report.recovery_difficulty {
            println!(
                "  復旧難易度:           {} ({})",
                diff.display_name(),
                diff.business_explanation()
            );
        }
        if let Some(rate) = &report.success_rate {
            println!("  {}", rate.business_summary());
            if !rate.reasoning.is_empty() {
                println!("    計算根拠:");
                for r in &rate.reasoning {
                    println!("      - {}", r);
                }
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnose_args_physical_requires_partition() {
        let args = DiagnoseArgs {
            physical: Some(1),
            partition: None,
        };
        let err = run(&args).expect_err("--physical のみはエラーになるべき");
        let msg = format!("{:#}", err);
        assert!(msg.contains("--partition"), "msg={}", msg);
    }

    #[test]
    fn diagnose_args_partition_requires_physical() {
        let args = DiagnoseArgs {
            physical: None,
            partition: Some(1),
        };
        let err = run(&args).expect_err("--partition のみはエラーになるべき");
        let msg = format!("{:#}", err);
        assert!(msg.contains("--physical"), "msg={}", msg);
    }

    #[test]
    fn diagnose_args_default_is_logical_mode() {
        let args = DiagnoseArgs::default();
        assert!(args.physical.is_none());
        assert!(args.partition.is_none());
    }
}
