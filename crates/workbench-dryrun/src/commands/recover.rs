//! `recover` サブコマンド: 案件に対して復旧を実行する。
//!
//! Chunk 23.6 改訂版: 復旧 PC では「いきなり recover」が標準フロー。
//! 案件 JSON が無ければ新規作成、既存出力フォルダがあれば確認プロンプトを出す。

use anyhow::{anyhow, Context, Result};

use dds_case_manager::{execute_business_recovery, CaseStorage};
use dds_core::format::format_bytes;
use dds_wish_match::{ExclusionList, Priority, Wish, WishItem, Wishlist};

use crate::drives::list_drives;
use crate::prompts::{confirm, prompt_case_id, prompt_number, prompt_string};
use crate::volume::open_ntfs_volume;

/// `recover` サブコマンドのエントリーポイント。
///
/// 手順 (Chunk 23.6 改訂版):
/// 1. 案件番号で `Case` を load。**案件が無ければ新規作成** (復旧 PC 標準フロー)
/// 2. 既に復旧済みなら 2 回目納品の可能性として警告
/// 3. ソース HDD / 納品先 HDD を選択 (同一ドライブを禁止)
/// 4. **納品先に既に案件フォルダがあれば上書き確認**
/// 5. Wishlist を対話形式または JSON ファイルから取得 (空でも可)
/// 6. 確認後 `execute_business_recovery` で復旧 + レポート生成
/// 7. 結果を「全体 / お客様優先データ」二重表示 + `case.json` 保存
pub fn run() -> Result<()> {
    println!("復旧モード");
    println!("---------------------------------------------");
    println!();

    // Step 1: 案件番号入力 + 案件 load または新規作成 (Chunk 23.6 改訂版)。
    //
    // 業務フロー上、診断 PC と復旧 PC は別物理 PC で、復旧 PC では診断 PC の
    // case.json は届かない。「いきなり recover」が標準なので、ここで自動作成する。
    let case_id = prompt_case_id()?;
    let storage = CaseStorage::default_location();
    let case_file = storage.case_file_path(&case_id);

    let mut case = if case_file.exists() {
        println!();
        println!("既存の案件を読み込みます: {}", case_id);
        let loaded = storage
            .load(&case_id)
            .context("既存 case.json の読み込みに失敗しました")?;
        if loaded.diagnostic_input.diagnosed_at.is_some() {
            println!("  - 診断済み (この PC で診断 → 復旧のフロー)");
        }
        if loaded.recovery_report_summary.is_some() {
            println!("  [注意] この案件は既に 1 回以上復旧されています。");
            println!("         2 回目以降の納品 (優先納品など) の場合は続行してください。");
        }
        loaded
    } else {
        println!();
        println!(
            "案件が見つかりません。新規作成して復旧を進めます: {}",
            case_id
        );
        println!("  (復旧 PC では「いきなり復旧」が標準フローです)");
        storage
            .create_new(case_id.clone())
            .context("新規案件の作成に失敗しました")?
    };

    println!();
    if case.diagnostic_input.diagnosed_at.is_none() {
        println!("[情報] この案件はまだ診断されていません。");
        println!("       復旧 PC では診断結果は CRM 経由で参照する運用が標準です。");
        if !confirm("このまま復旧を進めますか?")? {
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

    if source_drive
        .drive_letter
        .eq_ignore_ascii_case(&delivery_drive.drive_letter)
    {
        return Err(anyhow!(
            "ソースと納品先が同じドライブです。別のドライブを選択してください。"
        ));
    }

    // Step 3: 既存出力フォルダの検出 (Chunk 23.6 改訂版)。
    //
    // 同じ案件番号で 2 回目以降の recover や、誤った案件番号入力、前回失敗の残骸を
    // 検出するためのチェック。技術的な防御は限定的だが、業務的に「気付き」を促す。
    let case_output_root = delivery_drive.mount_point.join(case_id.as_str());
    if case_output_root.exists() {
        println!();
        println!("[注意] 納品先に既にこの案件のフォルダが存在します:");
        println!("    {}", case_output_root.display());
        println!();
        println!("考えられるケース:");
        println!("  1. 2 回目以降の納品 (優先納品など)");
        println!("  2. 別の案件で同じ番号を使ってしまった");
        println!("  3. 前回の復旧が失敗した残骸");
        println!();
        println!("続行すると既存ファイルが上書きされる可能性があります。");
        if !confirm("続行しますか?")? {
            return Err(anyhow!("ユーザーキャンセル"));
        }
    }

    // Step 4: Wishlist 入力。Chunk 23.7 以降は「お客様優先データ」のラベリング用
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
    println!("出力先: {}\\", case_output_root.display());
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
        &storage,
    )
    .context("復旧の実行に失敗しました")?;

    let elapsed = start.elapsed();
    println!("[復旧完了 - {:.2} 秒]", elapsed.as_secs_f64());
    println!();

    // Step 6: 結果表示。Chunk 24a で「品質保証率」表示削除 (お客様向け簡素化方針)。
    println!("結果 (全体):");
    println!("  該当ファイル:   {} 件", result.report.total_matched);
    println!(
        "  復旧成功:       {} 件 ({:.1}%)",
        result.report.recovered.len(),
        result.report.recovery_success_rate()
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
            "  復旧データ量:   {}",
            format_bytes(result.report.priority_total_bytes())
        );
        println!();
    }

    // Chunk 24a: 生成物を「納品 HDD」「社内保存」の 2 系統に分けて表示。
    println!("生成ファイル:");
    println!("  納品 HDD ({}):", result.case_output.root().display());
    println!("    └─ 復旧データ/");
    println!("        ├─ 通常ファイル/");
    println!("        └─ 削除ファイル/");
    println!("    └─ レポート/");
    println!(
        "        └─ 復旧レポート.docx ({})",
        result.report_paths.customer_docx.display()
    );
    println!();
    println!("  社内保存 ({}):", storage.base_dir().display());
    println!("    └─ {}/業務管理レポート.html", case_id.as_str());
    println!("    └─ {}/復旧詳細.csv (UTF-8 BOM 付き)", case_id.as_str());
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
        let priority_input = prompt_string("  優先度 (critical/high/normal/low、デフォルト high)")?;
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
    use dds_case_manager::{CaseId, CaseStorage};
    use tempfile::TempDir;

    /// Chunk 23.6 改訂版: 案件 JSON 不在時に新規作成できる挙動の単体保証。
    /// 復旧 PC では「いきなり recover」が標準なので、`run()` の `create_new` パスが
    /// 動くことを `CaseStorage` 側から確認する (実 prompt はモックしない)。
    #[test]
    fn recover_creates_new_case_when_not_exists() {
        let temp = TempDir::new().unwrap();
        let storage = CaseStorage::with_base_dir(temp.path());
        let case_id = CaseId::parse("260522-04").unwrap();

        assert!(!storage.case_file_path(&case_id).exists());

        let case = storage.create_new(case_id.clone()).unwrap();
        assert_eq!(case.case_id, case_id);
        assert!(case.diagnostic_input.diagnosed_at.is_none());
        assert!(storage.case_file_path(&case_id).exists());
    }

    /// 既存案件は `load` で読み込めることの確認。`run()` の `load` パス用。
    #[test]
    fn recover_loads_existing_case_when_present() {
        let temp = TempDir::new().unwrap();
        let storage = CaseStorage::with_base_dir(temp.path());
        let case_id = CaseId::parse("260522-04").unwrap();

        let case = storage.create_new(case_id.clone()).unwrap();
        storage.save(&case).unwrap();

        let loaded = storage.load(&case_id).unwrap();
        assert_eq!(loaded.case_id, case_id);
    }

    /// 既存出力フォルダ検出ロジックの単体保証 (実 prompt はモックしない)。
    /// `delivery_drive.mount_point.join(case_id.as_str()).exists()` で判定可能なこと。
    #[test]
    fn existing_output_directory_detection_logic() {
        let temp = TempDir::new().unwrap();
        let case_id = CaseId::parse("260522-04").unwrap();
        let case_output_root = temp.path().join(case_id.as_str());

        // 不在状態
        assert!(!case_output_root.exists());

        // 既存ディレクトリ作成 → 検出される
        std::fs::create_dir_all(&case_output_root).unwrap();
        assert!(case_output_root.exists());
        assert!(case_output_root.is_dir());
    }

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
