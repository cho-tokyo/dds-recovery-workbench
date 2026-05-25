//! サブコマンド実装群。各モジュールに 1 つの `pub fn run() -> anyhow::Result<()>` が定義される。

pub mod diagnose;
pub mod list_drives;
pub mod recover;
pub mod show;
