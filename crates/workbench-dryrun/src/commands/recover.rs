//! `recover` サブコマンド: 既存の案件に対して復旧を実行する。

use anyhow::{anyhow, Context, Result};

use dds_case_manager::{execute_business_recovery, CaseStorage};
use dds_core::format::format_bytes;
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number, prompt_string};
use crate::volume::open_ntfs_volume;

/// `recover` サブコマンドのエントリーポイント。
///
/// 手順:
/// 1. 案件番号で `Case` を load
/// 2. 未診断なら警告 + 続行確認
/// 3. ソース HDD / 納品先 HDD を選択 (同一ドライブを禁止)
/// 4. Wishlist を対話形式または JSON ファイルから取得
/// 5. 確認後 `execute_business_recovery` で復旧 + レポート生成
/// 6. 結果表示と `case.json` 保存
pub fn run() -> Result<()> {
    println!("復旧モード");
    println!("---------------------------------------------");
    println!();

    // Step 1: 案件番号入力 + 案件読み込み
    let case_id = prompt_case_id()?;
    let storage = CaseStorage::default_location();
    let mut case = storage
        .load(&case_id)
        .context("案件が見つかりません。先に diagnose で案件を作成してください")?;

    println!();
    if case.diagnostic_input.diagnosed_at.is_none() {
        println!("この案件はまだ診断されていません。");
        if !confirm("診断なしで復旧を進めますか? (推奨されません)")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }

    // Step 2: 対象 HDD の選択
    println!("接続中の NTFS ドライブ:");
    let drives: Vec<_> = list_drives()
        .into_iter()
        .filter(|d| d.is_ntfs() && !d.is_system_drive())
        .collect();

    if drives.len() < 2 {
        return Err(anyhow!(
            "NTFS ドライブが 2 つ以上必要です (ソース + 納品先)。\n\
             現在検出: {} 件",
            drives.len()
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

    let src_sel = prompt_number("ソース HDD (お客様の HDD) を選択", 1, drives.len())?;
    let source_drive = drives[src_sel - 1].clone();

    let dst_sel = prompt_number("納品先 HDD (G:\\ など) を選択", 1, drives.len())?;
    let delivery_drive = drives[dst_sel - 1].clone();

    if source_drive.drive_letter.eq_ignore_ascii_case(&delivery_drive.drive_letter) {
        return Err(anyhow!(
            "ソースと納品先が同じドライブです。別のドライブを選択してください。"
        ));
    }

    // Step 3: Wishlist 入力。Chunk 23.7 以降は「お客様優先データ」のラベリング用
    //         （空でも全件復旧は実行される）。
    let wishlist = prompt_wishlist()?;
    if wishlist.is_empty() {
        println!();
        println!("[注意] Wishlist が空ですが、全 user file を復旧します（R-STUDIO 風）。");
        println!("       お客様優先データの強調表示は行われません。");
        if !confirm("Wishlist 空のまま続行しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }

    // Chunk 23.7: 除外パターンは業務標準のデフォルトを使用。
    let exclusions = ExclusionList::default_system_exclusions();

    // Step 4: 確認
    println!();
    println!("確認:");
    println!("  案件番号:       {}", case_id);
    println!(
        "  ソース:         {} ({})",
        source_drive.drive_letter, source_drive.label
    );
    println!(
        "  納品先:         {} ({})",
        delivery_drive.drive_letter, delivery_drive.label
    );
    println!("  お客様優先データ: {} 件", wishlist.len());
    for (i, wish) in wishlist.wishes.iter().enumerate() {
        println!("    {}: 「{}」", i + 1, wish.label);
    }
    println!();
    println!("  除外パターン (業務標準):");
    println!("    - Windows / Program Files フォルダ");
    println!("    - $Recycle.Bin, System Volume Information");
    println!("    - $ で始まるシステムファイル");
    println!();
    println!("出力先: {}\\{}\\", delivery_drive.drive_letter, case_id);
    println!();

    if !confirm("復旧を開始しますか?")? {
        return Err(anyhow!("ユーザーキャンセル"));
    }

    // Step 5: 復旧実行
    println!();
    println!("[復旧中... 完了まで時間がかかる場合があります]");
    let start = std::time::Instant::now();

    let mut volume = open_ntfs_volume(&source_drive.access_path)?;
    let result = execute_business_recovery(
        &mut case,
        delivery_drive.mount_point.clone(),
        &mut volume,
        &wishlist,
        &exclusions,
    )
    .context("復旧の実行に失敗しました")?;

    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();

    // Step 6: 結果表示。Chunk 23.7 で「全体 + 優先データ」二重表示。
    println!("結果 (全体):");
    println!("  該当ファイル:   {} 件", result.report.total_matched);
    println!(
        "  復旧成功:       {} 件 ({:.1}%)",
        result.report.recovered.len(),
        result.report.recovery_success_rate()
    );
    println!(
        "  品質保証率:     {:.1}%",
        result.report.quality_assurance_rate()
    );
    println!(
        "  復旧データ量:   {}",
        format_bytes(result.report.total_bytes_written())
    );
    println!();
    if result.report.priority_count() > 0 {
        println!("結果 (お客様優先データ):");
        println!("  該当ファイル:   {} 件", result.report.priority_count());
        println!(
            "  品質保証率:     {:.1}%",
            result.report.priority_quality_assurance_rate()
        );
        println!(
            "  復旧データ量:   {}",
            format_bytes(result.report.priority_total_bytes())
        );
        println!();
    }

    println!("生成ファイル:");
    println!("  {}", result.case_output.root().display());
    println!("    └─ 復旧データ/");
    println!("        ├─ 通常ファイル/");
    println!("        └─ 削除ファイル/");
    println!("    └─ レポート/");
    println!("        ├─ 復旧レポート.docx");
    println!("        ├─ 要確認ファイル一覧.txt");
    println!("        ├─ 業務管理レポート.html");
    println!("        └─ report.csv");
    println!();

    // Step 7: case.json 永続化
    storage.save(&case)?;
    println!(
        "案件情報を保存しました: {}",
        storage.case_file_path(&case_id).display()
    );

    Ok(())
}

/// Wishlist の入力方法を選択し、`Wishlist` を返す。
fn prompt_wishlist() -> Result<Wishlist> {
    println!();
    println!("Wishlist の作成:");
    println!("  1. 対話形式で入力 (拡張子ベース、シンプル)");
    println!("  2. JSON ファイルから読み込み");
    let method = prompt_number("作成方法を選択", 1, 2)?;

    match method {
        1 => prompt_interactive_wishlist(),
        2 => load_wishlist_from_json(),
        _ => unreachable!("prompt_number でガード済み"),
    }
}

/// 対話形式で `Wishlist` を組み立てる。空ラベルで終了。
///
/// 拡張子ベース (`WishItem::Extension`) のみサポート。フル機能は Phase 2.1 UI で。
fn prompt_interactive_wishlist() -> Result<Wishlist> {
    println!();
    println!(
        "希望データを 1 つずつ入力します。完了するには「ラベル」で空 Enter を押してください。"
    );
    println!();

    let mut wishlist = Wishlist::new();
    let mut count = 1;

    loop {
        println!("希望 {}:", count);
        let label = prompt_string("  ラベル (例: Word ファイル、空 Enter で終了)")?;
        if label.is_empty() {
            break;
        }

        let ext = prompt_string("  拡張子 (例: docx、ピリオドなし)")?;
        let priority_input =
            prompt_string("  優先度 (critical/high/normal/low、デフォルト high)")?;
        let priority = parse_priority(&priority_input);

        wishlist = wishlist.add(
            Wish::new(WishItem::Extension(ext.to_lowercase()), &label).with_priority(priority),
        );
        count += 1;
        println!();
    }

    Ok(wishlist)
}

/// 優先度文字列を `Priority` に解釈する。未知の値はデフォルト `High`。
fn parse_priority(input: &str) -> Priority {
    match input.to_lowercase().as_str() {
        "critical" | "c" => Priority::Critical,
        "normal" | "n" => Priority::Normal,
        "low" | "l" => Priority::Low,
        _ => Priority::High,
    }
}

/// JSON ファイルパスを受け取り、`Wishlist` をデシリアライズして返す。
fn load_wishlist_from_json() -> Result<Wishlist> {
    let path = prompt_string("Wishlist JSON ファイルのパス")?;
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("ファイルを読めません: {}", path))?;
    let wishlist: Wishlist =
        serde_json::from_str(&json).context("Wishlist JSON のパースに失敗しました")?;
    Ok(wishlist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_priority_recognizes_named_levels() {
        assert_eq!(parse_priority("critical"), Priority::Critical);
        assert_eq!(parse_priority("C"), Priority::Critical);
        assert_eq!(parse_priority("normal"), Priority::Normal);
        assert_eq!(parse_priority("n"), Priority::Normal);
        assert_eq!(parse_priority("low"), Priority::Low);
        assert_eq!(parse_priority("L"), Priority::Low);
    }

    #[test]
    fn parse_priority_defaults_to_high_for_unknown() {
        assert_eq!(parse_priority(""), Priority::High);
        assert_eq!(parse_priority("high"), Priority::High);
        assert_eq!(parse_priority("medium"), Priority::High);
        assert_eq!(parse_priority("urgent"), Priority::High);
    }

    #[test]
    fn parse_priority_is_case_insensitive() {
        assert_eq!(parse_priority("CRITICAL"), Priority::Critical);
        assert_eq!(parse_priority("Normal"), Priority::Normal);
        assert_eq!(parse_priority("LOW"), Priority::Low);
    }
}
