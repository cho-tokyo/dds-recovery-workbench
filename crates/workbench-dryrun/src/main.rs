//! DDS Recovery Workbench - 実機ドライラン用暫定 CLI (Chunk 23.5)
//!
//! Phase 1.5 完成機能を検証 PC で実機 HDD に対して試すためのツール。
//! Phase 2.1 (Tauri UI) 完成後は予備品として残ります。
//!
//! 関連 FR: FR-CLI-01〜04。

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

#[cfg(not(windows))]
compile_error!("workbench-dryrun は Windows のみサポートしています");

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod drives;
mod prompts;
mod volume;

/// 実機ドライラン用 CLI のトップレベル引数。
#[derive(Parser, Debug)]
#[command(name = "workbench-dryrun")]
#[command(about = "DDS Recovery Workbench - 実機ドライラン用 CLI (Phase 1.5 暫定)")]
#[command(long_about = "
DDS Recovery Workbench の Phase 1.5 機能を実機 HDD で試すための暫定 CLI です。

使用例:
  workbench-dryrun list-drives        # 接続中ドライブの一覧
  workbench-dryrun diagnose           # 対話形式で診断
  workbench-dryrun recover            # 対話形式で復旧
  workbench-dryrun show               # 案件情報の表示

注意:
  ・物理ドライブへのアクセスには管理者権限が必要です
  ・「管理者として実行」で開いたコマンドプロンプトから実行してください
")]
struct Cli {
    /// 実行するサブコマンド。
    #[command(subcommand)]
    command: Commands,
}

/// 提供サブコマンド。
#[derive(Subcommand, Debug)]
enum Commands {
    /// 接続中のドライブを一覧表示する (既定: 論理 / `--physical` で物理)。
    ListDrives(commands::list_drives::ListDrivesArgs),

    /// 案件を作成/更新し、対象 HDD を診断する。
    Diagnose,

    /// 既存の案件に対して復旧を実行する。
    Recover,

    /// 案件情報を表示する。
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!();
    println!("DDS Recovery Workbench (Phase 1.5)");
    println!("============================================");
    println!();

    match cli.command {
        Commands::ListDrives(args) => commands::list_drives::run(&args),
        Commands::Diagnose => commands::diagnose::run(),
        Commands::Recover => commands::recover::run(),
        Commands::Show => commands::show::run(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_parses_list_drives_command() {
        let cli = Cli::try_parse_from(["workbench-dryrun", "list-drives"]).unwrap();
        match cli.command {
            Commands::ListDrives(args) => assert!(!args.physical),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_list_drives_physical_flag() {
        let cli = Cli::try_parse_from(["workbench-dryrun", "list-drives", "--physical"]).unwrap();
        match cli.command {
            Commands::ListDrives(args) => assert!(args.physical),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_diagnose_command() {
        let cli = Cli::try_parse_from(["workbench-dryrun", "diagnose"]).unwrap();
        assert!(matches!(cli.command, Commands::Diagnose));
    }

    #[test]
    fn cli_parses_recover_command() {
        let cli = Cli::try_parse_from(["workbench-dryrun", "recover"]).unwrap();
        assert!(matches!(cli.command, Commands::Recover));
    }

    #[test]
    fn cli_parses_show_command() {
        let cli = Cli::try_parse_from(["workbench-dryrun", "show"]).unwrap();
        assert!(matches!(cli.command, Commands::Show));
    }

    #[test]
    fn cli_rejects_unknown_subcommand() {
        let result = Cli::try_parse_from(["workbench-dryrun", "bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_rejects_missing_subcommand() {
        let result = Cli::try_parse_from(["workbench-dryrun"]);
        assert!(result.is_err());
    }
}
