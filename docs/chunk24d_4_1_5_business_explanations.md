# Chunk 24d-4-1.5 指示: 業務的説明文の追加

Phase 1.5 拡張の **第 4 段階 (中間)**。各診断項目に「なぜそうなっているか」「お客様への説明例」を追加し、営業がお客様に説明できるレベルの業務適用品質を実現する。

> 🎯 完了時点で「営業が診断結果を見れば、お客様にそのまま説明できる」状態に到達。Chouさんの要望「Dirty Bit は何なのか、なぜ立っているかお客様に説明したい」を実現。

---

## 全体像 (Chunk 24d シリーズ)

```
✅ Chunk 24d-1: 物理ディスクアクセス層
✅ Chunk 24d-2: パーティションテーブル解析
✅ Chunk 24d-3: NtfsVolume との統合
✅ Chunk 24d-4-1: 業務的診断項目の拡充
🚧 Chunk 24d-4-1.5: 業務的説明文の追加 ← 本指示書
⏳ Chunk 24d-4-2: 営業向け診断書 (DOCX)
⏳ Chunk 24d-4-3: 実機ドライランとフィードバック反映
```

## 背景: Chouさんの観察

```
[Chunk 24d-4-1 完了後の Chouさんの声]
診断結果:
  Dirty Bit: 立っている (Windows がマウント拒否する原因)
  
「Dirty Bit は何なのか、なぜフラグが立っているのか、
 お客様に説明したい」

[業務的な含意]
営業がお客様に「専門用語抜きで」説明できる必要がある:
- これは何か (何が起きているか)
- なぜそうなっているか (考えられる原因)
- Windows がどう反応しているか
- 業務的にどういう意味か (復旧可能性)
- お客様にどう伝えるか (説明テンプレート)
```

## 本チャンクのスコープ

### 含むもの

| Part | 内容 |
|---|---|
| **A** | `BusinessExplanation` 構造体 (5 セクションの説明文) |
| **B** | 各診断項目の説明文定義 (定数として静的に保持) |
| **C** | CLI `--verbose` オプション (詳細説明を含む表示) |
| **D** | CRM 貼り付けテキストに業務説明セクション追加 |
| **E** | 免責注釈の付与 (法的責任を負わない参考情報) |

### 含まないもの

```
✗ 営業向け診断書 (DOCX) → Chunk 24d-4-2
✗ 動的な説明文 (案件情報を埋め込み) → Phase 2
✗ 多言語対応 (日本語のみ) → Phase 2 以降
```

## 対象クレート

- **新規ファイル**: `crates/diagnostic/src/explanation.rs`
- **修正**: 既存の各診断モジュール (説明文への紐付け)
- **修正**: `crates/diagnostic/src/lib.rs`
- **修正**: `crates/report/src/crm_text.rs` (業務説明セクション)
- **修正**: `crates/workbench-dryrun/src/commands/diagnose.rs` (`--verbose` オプション)

## 重要な設計原則

### 説明文の 5 セクション構造 (Q2 で確定)

各診断項目 (異常状態のみ) に以下を提供:

```
1. 【何が起きているか】 (what_happened)
   - 技術的事実を平易な日本語で説明
   
2. 【考えられる原因】 (causes)
   - 業務的に「お客様の HDD で何があったか」のシナリオ
   - 複数列挙
   
3. 【Windows の挙動】 (windows_behavior)
   - Windows がなぜマウントできないか
   - これが「Windows で開けない」理由
   
4. 【業務的な意味】 (business_meaning)
   - データ復旧可能性
   - 営業の見積根拠
   
5. 【お客様への説明例】 (customer_explanation)
   - 専門用語ゼロ
   - 営業がそのままお客様に伝えられる文言
   - 免責注釈付き
```

### 免責注釈 (Q3: a+c)

```
[各お客様向け説明文の末尾に追加]
※ この説明文は参考情報として提供しています。
   個別案件の状況により異なる場合があります。
   法的責任を負うものではありません。
```

Q3 = a+c の組み合わせ: 法的責任なし + 免責注釈。

### 説明文の定義場所

```rust
// crates/diagnostic/src/explanation.rs
pub mod explanations {
    pub static DIRTY_BIT_SET: BusinessExplanation = BusinessExplanation { ... };
    pub static DIRTY_BIT_CLEAN: BusinessExplanation = BusinessExplanation { ... };
    pub static LOGFILE_INCONSISTENT: BusinessExplanation = BusinessExplanation { ... };
    // ...
}
```

全ての説明文を 1 箇所 (`explanations` モジュール) に集約。
Chouさんが業務的に文言を確認・修正しやすくする。

### CLI 表示モード

```
[通常モード] (デフォルト、既存維持)
- 簡潔な要約のみ
- Chunk 24d-4-1 の出力

[--verbose モード] (Chunk 24d-4-1.5 で追加)
- 通常モード + 業務的説明文 (5 セクション)
- 営業がじっくり読む用
```

## 仕様参照

### ビジネス要件

- **FR-DIAG-08** (業務的説明文の提供) ← 新規達成
- **FR-DIAG-09** (お客様への説明テンプレート) ← 新規達成
- **FR-DIAG-10** (免責注釈) ← 新規達成

## 実装内容

### Part A: `BusinessExplanation` 構造体 (`crates/diagnostic/src/explanation.rs` 新規)

```rust
//! 各診断項目の業務的説明文.
//!
//! 営業がお客様に説明できるよう、各診断結果に対して
//! 5 セクション (何が起きているか、原因、Windows の挙動、
//! 業務的な意味、お客様への説明例) の説明文を提供する。
//!
//! ## Chouさんの業務観点
//!
//! 「Dirty Bit は何なのか、なぜ立っているか、お客様に説明したい」
//! → ツールが業務的な説明文を提供する必要がある
//!
//! ## 免責注釈
//!
//! 各お客様向け説明文には免責注釈が付与される。
//! ツールは「参考情報」を提供し、法的責任は負わない。

use serde::{Deserialize, Serialize};

/// 業務的説明文 (5 セクション構造)
///
/// 異常状態の診断項目に紐付けられ、営業がお客様に説明する際の
/// 参考情報として使用される。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessExplanation {
    /// 一行要約 (CLI の簡易表示で使用)
    pub summary: &'static str,
    
    /// 【何が起きているか】 技術的事実を平易な日本語で
    pub what_happened: &'static str,
    
    /// 【考えられる原因】 業務的なシナリオ
    pub causes: &'static [&'static str],
    
    /// 【Windows の挙動】 マウント拒否等の理由
    pub windows_behavior: &'static str,
    
    /// 【業務的な意味】 データ復旧可能性、見積根拠
    pub business_meaning: &'static str,
    
    /// 【お客様への説明例】 専門用語ゼロ、営業がそのまま使える
    pub customer_explanation: &'static str,
}

/// 免責注釈 (お客様向け説明文に付与)
pub const CUSTOMER_DISCLAIMER: &str = "※ この説明文は参考情報として提供しています。\n\
※ 個別案件の状況により異なる場合があります。\n\
※ 法的責任を負うものではありません。";

impl BusinessExplanation {
    /// CLI の --verbose 表示用フォーマット
    pub fn format_for_cli(&self, indent: &str) -> String {
        let mut s = String::new();
        
        s.push_str(&format!("{}【何が起きているか】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.what_happened));
        s.push_str("\n");
        
        s.push_str(&format!("{}【考えられる原因】\n", indent));
        for cause in self.causes {
            s.push_str(&format!("{}  - {}\n", indent, cause));
        }
        s.push_str("\n");
        
        s.push_str(&format!("{}【Windows の挙動】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.windows_behavior));
        s.push_str("\n");
        
        s.push_str(&format!("{}【業務的な意味】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.business_meaning));
        s.push_str("\n");
        
        s.push_str(&format!("{}【お客様への説明例】\n", indent));
        s.push_str(&format!("{}  「{}」\n", indent, self.customer_explanation));
        s.push_str("\n");
        
        for line in CUSTOMER_DISCLAIMER.lines() {
            s.push_str(&format!("{}  {}\n", indent, line));
        }
        
        s
    }
    
    /// CRM 貼り付けテキスト用 (簡潔バージョン、お客様説明のみ)
    pub fn format_for_crm(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.customer_explanation);
        s.push_str("\n\n");
        s.push_str(CUSTOMER_DISCLAIMER);
        s
    }
}

// ============================================================
// 説明文の定義 (Dirty Bit)
// ============================================================

pub static DIRTY_BIT_SET: BusinessExplanation = BusinessExplanation {
    summary: "立っている (Windows がマウント拒否する原因)",
    
    what_happened: "HDD への書き込み処理中に、何らかの理由で処理が中断された記録が NTFS ファイルシステムに残っています。",
    
    causes: &[
        "PC の電源が突然切れた (停電、シャットダウン強制終了)",
        "USB HDD を Windows の「安全な取り外し」をせずに抜いた",
        "システムクラッシュ、ブルースクリーン",
        "アプリケーションが書き込み中に強制終了",
    ],
    
    windows_behavior: "未完了の書き込みによりデータが不整合な可能性があるため、Windows は安全のためアクセスを拒否し、chkdsk による修復を要求します。",
    
    business_meaning: "NTFS の構造自体は健全な場合が多く、データ復旧は十分可能です。当社の専門ツールはこの状態でも問題なくデータを読み出せます。",
    
    customer_explanation: "お客様の HDD は、書き込み中に何らかの理由 (電源断、不正な取り外し等) で処理が中断された状態です。Windows はそのままでは安全に開けないと判断していますが、データ自体は失われていない可能性が高く、当社の専門ツールで復旧可能です。",
};

// ============================================================
// 説明文の定義 ($LogFile)
// ============================================================

pub static LOGFILE_INCONSISTENT: BusinessExplanation = BusinessExplanation {
    summary: "不整合あり (未完了トランザクション)",
    
    what_happened: "NTFS のトランザクションログ ($LogFile) に、書き込み処理の途中で記録された未完了の項目が残っています。",
    
    causes: &[
        "書き込み処理中の予期しない中断",
        "電源断によるトランザクションの未完了",
        "USB HDD の不正な切断",
        "システムの予期しないシャットダウン",
    ],
    
    windows_behavior: "Windows はマウント前にこの未完了トランザクションを再生して整合性を取ろうとしますが、再生が失敗する場合はマウントを拒否します。",
    
    business_meaning: "メタデータ (ファイル管理情報) の一部が未確定ですが、データファイル自体は読み出し可能です。当社のツールは直接 NTFS 構造を解析するため、$LogFile の状態に影響されません。",
    
    customer_explanation: "お客様の HDD には、書き込み処理の途中で中断された記録が残っています。Windows はこの状態を「整合性に問題あり」と判断していますが、データファイル自体は無事である可能性が高く、当社で復旧可能です。",
};

// ============================================================
// 説明文の定義 (BitLocker)
// ============================================================

pub static BITLOCKER_ENCRYPTED: BusinessExplanation = BusinessExplanation {
    summary: "BitLocker 暗号化を検出 (回復キーが必要)",
    
    what_happened: "HDD は Windows の暗号化機能 (BitLocker) で保護されています。データはすべて暗号化された状態で保存されています。",
    
    causes: &[
        "Windows 10/11 Pro 以上で BitLocker が有効化されている",
        "Microsoft アカウントの設定で自動的に有効化された",
        "企業のセキュリティポリシーで有効化された",
        "Surface 等の端末でデフォルトで有効化されている",
    ],
    
    windows_behavior: "BitLocker は暗号化と認証を行うため、回復キー (48 桁の数字) またはパスワードがないとアクセスできません。Windows は別の PC からこのドライブを開く際もキーを要求します。",
    
    business_meaning: "BitLocker 暗号化されたデータは、回復キーがあれば復旧可能です。キーが不明な場合は、お客様の Microsoft アカウントから取得できる場合があります。当社ではキーなしの BitLocker 復旧は Phase 1.5 では対応していません。",
    
    customer_explanation: "お客様の HDD は Windows の暗号化機能 (BitLocker) で保護されています。データを取り出すには、48 桁の回復キーが必要です。お客様の Microsoft アカウントから取得できる可能性がありますので、お調べください。",
};

// ============================================================
// 説明文の定義 (MFT エントリ破損)
// ============================================================

pub static MFT_CORRUPTION_LIGHT: BusinessExplanation = BusinessExplanation {
    summary: "軽度 (1-10 件)",
    
    what_happened: "ファイル管理情報の中で、少数のエントリ (1-10 件) が破損しています。",
    
    causes: &[
        "ディスク表面の小規模な物理障害",
        "書き込み中の電源断",
        "古い HDD の経年劣化",
    ],
    
    windows_behavior: "Windows は破損エントリを認識すると、対応するファイルをアクセス不能にすることがあります。chkdsk で修復可能な場合もあります。",
    
    business_meaning: "全体ファイル数に対して影響は限定的です。破損したエントリ以外のファイルは正常に復旧可能です。",
    
    customer_explanation: "お客様の HDD のファイル管理情報の一部 (少数のファイル分) が破損しています。影響は限定的で、ほとんどのファイルは復旧可能です。",
};

pub static MFT_CORRUPTION_MODERATE: BusinessExplanation = BusinessExplanation {
    summary: "中度 (11-100 件)",
    
    what_happened: "ファイル管理情報の中で、複数のエントリ (11-100 件) が破損しています。",
    
    causes: &[
        "中規模な物理障害 (不良セクタ)",
        "繰り返された不適切な取り扱い",
        "経年劣化が進んでいる HDD",
    ],
    
    windows_behavior: "Windows はこのレベルの破損があるとマウント拒否、または部分的なファイルアクセス不能を引き起こします。",
    
    business_meaning: "復旧可能なファイル数が制限される可能性があります。当社のツールは健全なエントリから順次復旧します。",
    
    customer_explanation: "お客様の HDD のファイル管理情報に複数の破損が見つかりました。影響を受けるファイルがあるかもしれませんが、大部分のファイルは復旧可能です。",
};

pub static MFT_CORRUPTION_SEVERE: BusinessExplanation = BusinessExplanation {
    summary: "重度 (101 件以上)",
    
    what_happened: "ファイル管理情報の中で、多数のエントリ (101 件以上) が破損しています。",
    
    causes: &[
        "深刻な物理障害",
        "ディスク表面の広範囲な不良セクタ",
        "故障が進行している HDD",
    ],
    
    windows_behavior: "Windows はこのレベルの破損があるとマウント拒否し、chkdsk でも修復困難なことが多いです。",
    
    business_meaning: "復旧可能ファイル数に制限がありますが、ファイル単位の復旧 (カービング技術) で重要データを取り出せる可能性があります。難易度は高いですが、業務上対応可能です。",
    
    customer_explanation: "お客様の HDD のファイル管理情報に大規模な破損があります。すべてのファイルを復旧することは困難ですが、当社の専門技術で重要なデータを取り出せる可能性があります。",
};

// ============================================================
// 説明文の定義 (Boot sector 異常)
// ============================================================

pub static BOOT_SECTOR_DAMAGED: BusinessExplanation = BusinessExplanation {
    summary: "異常 (NTFS の起動情報が破損)",
    
    what_happened: "HDD の先頭にある NTFS の起動情報 (ブートセクタ) が破損しています。これは NTFS ファイルシステムの基本情報が失われている状態です。",
    
    causes: &[
        "ディスクの先頭部分への物理障害",
        "誤ったパーティション操作",
        "ウイルス・マルウェアによる破壊",
        "ファイルシステムを変換しようとして失敗",
    ],
    
    windows_behavior: "Windows はブートセクタを読めないため、フォーマットされていないと判断します。「フォーマットしますか?」のダイアログが表示されます。",
    
    business_meaning: "ブートセクタ自体は復元可能 (NTFS にはバックアップが存在する) で、その後通常通り復旧できる場合が多いです。",
    
    customer_explanation: "お客様の HDD の起動情報が破損しています。Windows は「フォーマットされていない」と判断していますが、当社の専門技術で起動情報を再構築し、データを復旧することが可能です。",
};

// ============================================================
// 説明文の定義 (復旧難易度)
// ============================================================

pub static DIFFICULTY_EASY: BusinessExplanation = BusinessExplanation {
    summary: "易 (標準的な業務ケース)",
    
    what_happened: "NTFS の構造は健全で、特に重大な障害は検出されていません。",
    
    causes: &[
        "通常のファイル削除によるデータ消失",
        "軽微な論理障害",
        "正常な HDD でのアクセス問題",
    ],
    
    windows_behavior: "Windows がマウントできる、または軽微な問題のみです。",
    
    business_meaning: "標準的な復旧プロセスで対応可能です。復旧成功の見込みが高く、業務的に容易な案件です。",
    
    customer_explanation: "お客様の HDD の状態は良好で、データ復旧は標準的なプロセスで対応可能です。高い成功率が見込まれます。",
};

pub static DIFFICULTY_MEDIUM: BusinessExplanation = BusinessExplanation {
    summary: "中 (部分的な障害あり、業務的に標準範囲)",
    
    what_happened: "Dirty Bit や $LogFile の不整合、または小規模な MFT 破損など、部分的な障害が検出されています。",
    
    causes: &[
        "電源断や不正な取り外しによる中断",
        "削除ファイルが多数",
        "軽度の物理障害",
    ],
    
    windows_behavior: "Windows がマウント拒否、または部分的にアクセス不能になっている可能性があります。",
    
    business_meaning: "やや慎重な復旧プロセスが必要ですが、業務的には標準範囲で対応可能です。復旧成功率は良好です。",
    
    customer_explanation: "お客様の HDD には部分的な障害がありますが、データ復旧は十分可能です。当社の専門ツールで丁寧に処理いたします。",
};

pub static DIFFICULTY_HARD: BusinessExplanation = BusinessExplanation {
    summary: "難 (大規模な障害、ファイル単位の復旧が必要)",
    
    what_happened: "大規模な MFT 破損、ブートセクタ破損、BitLocker 暗号化、または完全な FS 構造破壊など、深刻な障害が検出されています。",
    
    causes: &[
        "深刻な物理障害",
        "誤った操作 (フォーマット、削除等)",
        "BitLocker 暗号化",
        "経年劣化の進行",
    ],
    
    windows_behavior: "Windows はマウントを拒否、または「フォーマットしますか?」のダイアログを表示します。",
    
    business_meaning: "ファイル管理情報からの通常の復旧は困難ですが、ファイル単位の復旧 (カービング技術) により重要データを取り出せる可能性があります。難易度は高く、復旧時間も標準より長くなります。",
    
    customer_explanation: "お客様の HDD には深刻な障害があり、難易度の高い案件となります。すべてのファイルを復旧することは保証できませんが、当社の専門技術で重要なデータを取り出せる可能性があります。費用と期間が標準より高くなる可能性があります。",
};

pub static DIFFICULTY_CAUTION: BusinessExplanation = BusinessExplanation {
    summary: "注意 (物理障害の兆候、業務的に慎重判断が必要)",
    
    what_happened: "MFT のほとんどが読めない等、HDD の物理障害の兆候が検出されています。これ以上のアクセスで状態が悪化する可能性があります。",
    
    causes: &[
        "ヘッドクラッシュ等の機械的故障",
        "回路基板の故障",
        "重度の経年劣化",
    ],
    
    windows_behavior: "Windows は HDD を認識できない、または極端に遅い動作をします。",
    
    business_meaning: "通常の論理障害対応では復旧困難です。物理障害対応 (クリーンルームでの作業等) が必要な可能性があり、受注可否は業務担当者が慎重に判断する必要があります。当社では物理障害対応も実施しています。",
    
    customer_explanation: "お客様の HDD には物理的な障害の兆候があります。通常のソフトウェアでの復旧は難しい状態です。当社の物理障害対応の専門チームでも対応可能ですが、復旧難易度と費用については別途お見積もりをご案内します。",
};

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn business_explanation_has_all_fields() {
        let exp = &DIRTY_BIT_SET;
        assert!(!exp.summary.is_empty());
        assert!(!exp.what_happened.is_empty());
        assert!(!exp.causes.is_empty());
        assert!(!exp.windows_behavior.is_empty());
        assert!(!exp.business_meaning.is_empty());
        assert!(!exp.customer_explanation.is_empty());
    }
    
    #[test]
    fn dirty_bit_explanation_mentions_business_meaning() {
        assert!(DIRTY_BIT_SET.business_meaning.contains("復旧"));
    }
    
    #[test]
    fn customer_explanation_avoids_technical_jargon() {
        // 技術用語が含まれていないことを確認 (主要な専門用語のみチェック)
        let customer_text = DIRTY_BIT_SET.customer_explanation;
        assert!(!customer_text.contains("MFT"));
        assert!(!customer_text.contains("$Volume"));
        assert!(!customer_text.contains("VOLUME_INFORMATION"));
    }
    
    #[test]
    fn format_for_cli_includes_disclaimer() {
        let output = DIRTY_BIT_SET.format_for_cli("  ");
        assert!(output.contains("【何が起きているか】"));
        assert!(output.contains("【考えられる原因】"));
        assert!(output.contains("【Windows の挙動】"));
        assert!(output.contains("【業務的な意味】"));
        assert!(output.contains("【お客様への説明例】"));
        assert!(output.contains("法的責任を負うもの"));
    }
    
    #[test]
    fn format_for_crm_includes_customer_explanation_and_disclaimer() {
        let output = DIRTY_BIT_SET.format_for_crm();
        assert!(output.contains("お客様の HDD"));
        assert!(output.contains("法的責任を負うもの"));
    }
    
    #[test]
    fn difficulty_caution_emphasizes_human_judgment() {
        // 「注意」レベルは人間判断を強調する文言が必要
        assert!(DIFFICULTY_CAUTION.business_meaning.contains("業務担当者") 
            || DIFFICULTY_CAUTION.business_meaning.contains("慎重"));
    }
    
    #[test]
    fn bitlocker_explanation_mentions_recovery_key() {
        assert!(BITLOCKER_ENCRYPTED.summary.contains("回復キー"));
        assert!(BITLOCKER_ENCRYPTED.customer_explanation.contains("回復キー"));
    }
    
    #[test]
    fn disclaimer_format() {
        assert!(CUSTOMER_DISCLAIMER.contains("参考情報"));
        assert!(CUSTOMER_DISCLAIMER.contains("法的責任"));
        assert!(CUSTOMER_DISCLAIMER.contains("個別案件"));
    }
}
```

### Part B: 各診断項目への紐付け

#### `crates/diagnostic/src/dirty_bit.rs` の修正

```rust
use super::explanation::{BusinessExplanation, DIRTY_BIT_SET};

impl DirtyBitStatus {
    /// 業務的説明文を取得 (異常時のみ Some を返す)
    pub fn explanation(&self) -> Option<&'static BusinessExplanation> {
        match self {
            Self::Dirty => Some(&DIRTY_BIT_SET),
            Self::Clean => None,    // 正常は説明文不要
            Self::Unknown => None,  // 不明は説明文なし
        }
    }
}
```

同様のパターンで `log_file.rs`, `bitlocker.rs`, `difficulty.rs` にも `explanation()` メソッドを追加。

#### MFT 破損カウントの説明文

```rust
// crates/diagnostic/src/engine.rs か別ファイルで:
pub fn mft_corruption_explanation(count: u64) -> Option<&'static BusinessExplanation> {
    match count {
        0 => None,
        1..=10 => Some(&MFT_CORRUPTION_LIGHT),
        11..=100 => Some(&MFT_CORRUPTION_MODERATE),
        _ => Some(&MFT_CORRUPTION_SEVERE),
    }
}
```

#### Boot sector 異常の説明文

```rust
// 既存の Boot sector 判定ロジックに紐付け
pub fn boot_sector_explanation(is_damaged: bool) -> Option<&'static BusinessExplanation> {
    if is_damaged {
        Some(&BOOT_SECTOR_DAMAGED)
    } else {
        None
    }
}
```

### Part C: CLI `--verbose` オプション

`crates/workbench-dryrun/src/commands/diagnose.rs`:

```rust
#[derive(Args, Debug)]
pub struct DiagnoseArgs {
    /// 物理ドライブ番号
    #[arg(long)]
    pub physical: Option<u32>,
    
    /// パーティション番号
    #[arg(long)]
    pub partition: Option<u32>,
    
    /// 業務的説明文を含む詳細表示
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

fn show_diagnostic_result(case: &Case, verbose: bool) -> Result<()> {
    let diag = case.diagnostic_input.as_ref()
        .ok_or_else(|| anyhow!("診断結果がありません"))?;
    
    // 既存の表示 (Chunk 24d-4-1 で実装済み)
    show_basic_diagnostic_summary(diag);
    
    // ★ --verbose 時に業務説明を追加
    if verbose {
        println!();
        println!("===========================================");
        println!("  業務説明 (営業の業務的判断のための参考情報)");
        println!("===========================================");
        println!();
        
        // Dirty Bit の説明
        if let Some(status) = &diag.dirty_bit {
            if let Some(exp) = status.explanation() {
                println!("[Dirty Bit について]");
                print!("{}", exp.format_for_cli("  "));
                println!();
            }
        }
        
        // $LogFile の説明
        if let Some(status) = &diag.log_file {
            if let Some(exp) = status.explanation() {
                println!("[$LogFile 整合性について]");
                print!("{}", exp.format_for_cli("  "));
                println!();
            }
        }
        
        // BitLocker の説明
        if let Some(status) = &diag.bitlocker {
            if let Some(exp) = status.explanation() {
                println!("[BitLocker 暗号化について]");
                print!("{}", exp.format_for_cli("  "));
                println!();
            }
        }
        
        // MFT 破損の説明
        if let Some(exp) = mft_corruption_explanation(diag.mft_corruption_count()) {
            println!("[MFT エントリ破損について]");
            print!("{}", exp.format_for_cli("  "));
            println!();
        }
        
        // Boot sector の説明
        if let Some(exp) = boot_sector_explanation(!diag.is_boot_sector_ok()) {
            println!("[Boot sector について]");
            print!("{}", exp.format_for_cli("  "));
            println!();
        }
        
        // 復旧難易度の説明
        if let Some(diff) = &diag.recovery_difficulty {
            if let Some(exp) = diff.explanation() {
                println!("[復旧難易度について]");
                print!("{}", exp.format_for_cli("  "));
                println!();
            }
        }
        
        println!("===========================================");
    } else {
        // 通常モード: ヒントを表示
        println!();
        println!("💡 業務的な詳細説明 (お客様への説明テンプレート含む) を表示するには:");
        println!("   workbench-dryrun diagnose [既存のオプション] --verbose");
        println!();
    }
    
    Ok(())
}
```

### Part D: CRM 貼り付けテキストへの統合

`crates/report/src/crm_text.rs`:

```rust
pub fn render_crm_paste_text(case: &Case) -> String {
    let mut s = String::new();
    
    // 既存セクション (Chunk 24d-4-1 で実装)
    s.push_str(&format!("【案件番号】 {}\n", case.case_id));
    // ...
    
    // ★ 新規追加: お客様への説明 (業務的説明文)
    if let Some(diag) = &case.diagnostic_input {
        s.push_str("\n");
        s.push_str("【お客様への説明 (参考)】\n");
        s.push_str("\n");
        
        let mut has_explanation = false;
        
        // Dirty Bit
        if let Some(status) = &diag.dirty_bit {
            if let Some(exp) = status.explanation() {
                s.push_str("■ HDD の状態 (Dirty Bit):\n");
                s.push_str(exp.customer_explanation);
                s.push_str("\n\n");
                has_explanation = true;
            }
        }
        
        // $LogFile
        if let Some(status) = &diag.log_file {
            if let Some(exp) = status.explanation() {
                s.push_str("■ HDD の状態 ($LogFile):\n");
                s.push_str(exp.customer_explanation);
                s.push_str("\n\n");
                has_explanation = true;
            }
        }
        
        // BitLocker
        if let Some(status) = &diag.bitlocker {
            if let Some(exp) = status.explanation() {
                s.push_str("■ HDD の状態 (BitLocker):\n");
                s.push_str(exp.customer_explanation);
                s.push_str("\n\n");
                has_explanation = true;
            }
        }
        
        // MFT 破損
        if let Some(exp) = mft_corruption_explanation(diag.mft_corruption_count()) {
            s.push_str("■ HDD の状態 (ファイル管理情報):\n");
            s.push_str(exp.customer_explanation);
            s.push_str("\n\n");
            has_explanation = true;
        }
        
        // Boot sector
        if let Some(exp) = boot_sector_explanation(!diag.is_boot_sector_ok()) {
            s.push_str("■ HDD の状態 (起動情報):\n");
            s.push_str(exp.customer_explanation);
            s.push_str("\n\n");
            has_explanation = true;
        }
        
        // 復旧難易度
        if let Some(diff) = &diag.recovery_difficulty {
            if let Some(exp) = diff.explanation() {
                s.push_str("■ 復旧難易度について:\n");
                s.push_str(exp.customer_explanation);
                s.push_str("\n\n");
                has_explanation = true;
            }
        }
        
        if has_explanation {
            s.push_str(super::super::diagnostic::explanation::CUSTOMER_DISCLAIMER);
            s.push_str("\n");
        }
    }
    
    s
}
```

### Part E: `lib.rs` の更新

`crates/diagnostic/src/lib.rs`:

```rust
// 新規追加:
pub mod explanation;

pub use explanation::{
    BusinessExplanation,
    CUSTOMER_DISCLAIMER,
    DIRTY_BIT_SET,
    LOGFILE_INCONSISTENT,
    BITLOCKER_ENCRYPTED,
    MFT_CORRUPTION_LIGHT,
    MFT_CORRUPTION_MODERATE,
    MFT_CORRUPTION_SEVERE,
    BOOT_SECTOR_DAMAGED,
    DIFFICULTY_EASY,
    DIFFICULTY_MEDIUM,
    DIFFICULTY_HARD,
    DIFFICULTY_CAUTION,
};
```

## 単体テスト要件 (最低 12 件)

### `explanation.rs` (最低 8 件)

1. `business_explanation_has_all_fields`
2. `dirty_bit_explanation_mentions_business_meaning`
3. `customer_explanation_avoids_technical_jargon`
4. `format_for_cli_includes_disclaimer`
5. `format_for_crm_includes_customer_explanation_and_disclaimer`
6. `difficulty_caution_emphasizes_human_judgment`
7. `bitlocker_explanation_mentions_recovery_key`
8. `disclaimer_format`

### 各診断モジュール (最低 4 件)

9. `dirty_bit_clean_returns_no_explanation`
10. `dirty_bit_set_returns_some_explanation`
11. `mft_corruption_classification` (0/light/moderate/severe)
12. `difficulty_returns_explanation`

## 結合テスト要件 (最低 2 件)

```rust
#[test]
fn crm_text_includes_customer_explanations_when_anomalies() {
    let mut case = make_case_with_dirty_bit_and_corruption();
    let crm_text = render_crm_paste_text(&case);
    
    assert!(crm_text.contains("【お客様への説明"));
    assert!(crm_text.contains("Dirty Bit"));
    assert!(crm_text.contains("参考情報"));
    assert!(crm_text.contains("法的責任"));
}

#[test]
fn crm_text_omits_explanations_when_healthy() {
    let mut case = make_healthy_case();
    let crm_text = render_crm_paste_text(&case);
    
    // 健全な場合、お客様向け説明セクションは表示されない or 空
    // (具体的な実装次第)
}
```

## 制約

- **行数目安**:
  - `diagnostic/src/explanation.rs` (新規): 約 400 行 (説明文データ大)
  - 各 diagnostic モジュールへの `explanation()` メソッド追加: 約 60 行 (合計)
  - `workbench-dryrun/src/commands/diagnose.rs` 修正: +80 行 (`--verbose` 対応)
  - `report/src/crm_text.rs` 修正: +60 行 (業務説明セクション)
  - `diagnostic/src/engine.rs` 修正: +30 行 (MFT/Boot sector の説明文関数)
  - `diagnostic/src/lib.rs` 修正: +10 行 (export)
  - テスト: 約 100 行
  - 合計: 約 740 行追加・修正
- **単体テスト新規**: 最低 12 件
- **結合テスト新規**: 最低 2 件
- **`unsafe` 追加行数**: 0
- **既存テスト**: 全パス維持

## 完了条件チェックリスト

- [ ] `cargo check --workspace` がエラーなし
- [ ] `cargo test --workspace` 全体で全パス
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`: warning 0
- [ ] 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)
- [ ] `workbench-dryrun diagnose --verbose` が動作
- [ ] 業務的説明文に 5 セクションすべてが含まれる
- [ ] お客様向け説明文に技術用語 (MFT、$Volume 等) が含まれない
- [ ] 免責注釈が全説明文に含まれる
- [ ] CRM 貼り付けテキストに「お客様への説明」セクションが追加される
- [ ] 異常がない場合、説明文は表示されない (CLI と CRM 両方)
- [ ] 「注意」レベルの説明文に「人間が判断」が含まれる

## 関連 FR 要件

- **FR-DIAG-08** (業務的説明文の提供) ← 達成
- **FR-DIAG-09** (お客様への説明テンプレート) ← 達成
- **FR-DIAG-10** (免責注釈) ← 達成

## 完了後

1. tester エージェントへ引き継ぎ
2. テスト合格後、progress-tracker へ
3. **次のチャンク: Chunk 24d-4-2 (営業向け診断書 DOCX)**

---

## 注意事項

### 説明文の業務的レビュー

```
[業務的に重要なポイント]
- お客様への説明例: 専門用語ゼロ
- 業務的な意味: 復旧可能性が分かる
- Windows の挙動: なぜ Windows で開けないか

[実装後の確認]
Chouさんが実機テスト時に各説明文を確認し、業務的に違和感あれば
explanation.rs の static 定数を修正するだけで対応可能 (再ビルドのみ)
```

### 説明文の更新容易性

```
[設計の利点]
全ての説明文を 1 つのファイル (explanation.rs) に集約
業務的な文言調整は Chouさんが業務的に判断して修正可能
コード変更不要、文字列リテラルのみの修正
```

### 免責注釈の位置付け

```
[業務的な役割]
- 法的責任を限定 (Q3-a)
- 「参考情報」として位置付け (Q3-c)
- 個別案件の特性を尊重

[実装での重要性]
全てのお客様向け説明文の最後に必ず付与
営業が CRM にコピペする際も自動的に含まれる
```

### CLI `--verbose` の使い分け

```
[通常モード]
業務的なルーチン作業向け
簡潔、見やすい

[--verbose モード]
- 営業がじっくり業務的に確認したい時
- お客様への説明テンプレートを確認したい時
- CRM 貼り付け前に内容を確認したい時
```

### Phase 2.1 UI への引き継ぎ

```
[UI での想定]
- 各診断項目をタップ/クリック → 詳細パネル表示
- 詳細パネルに 5 セクションを表示
- 「お客様用説明をコピー」ボタン

[Chunk 24d-4-1.5 で公開する API]
BusinessExplanation 構造体 → JSON シリアライズ可能
→ UI から取得して表示可能
```

---

## 質問が必要なケース

- 説明文の文言が業務的に違和感がある場合 → Chouさんと相談して定数を修正
- 既存の DiagnosticInput の構造が想定外の場合
- CRM 貼り付けテキストの既存実装と統合が困難な場合

---

## 完了報告例

```markdown
## Chunk 24d-4-1.5 完了報告

### 新規ファイル
- crates/diagnostic/src/explanation.rs (約 400 行 + テスト 70 行)

### 修正ファイル
- crates/diagnostic/src/dirty_bit.rs (+15 行 explanation() メソッド)
- crates/diagnostic/src/log_file.rs (+15 行)
- crates/diagnostic/src/bitlocker.rs (+15 行)
- crates/diagnostic/src/difficulty.rs (+20 行)
- crates/diagnostic/src/engine.rs (+30 行 MFT/Boot sector の説明関数)
- crates/diagnostic/src/lib.rs (+10 行 export)
- crates/workbench-dryrun/src/commands/diagnose.rs (+80 行 --verbose)
- crates/report/src/crm_text.rs (+60 行 業務説明セクション)

### 新規 API
- BusinessExplanation 構造体
- CUSTOMER_DISCLAIMER 定数
- 静的説明文定数: DIRTY_BIT_SET, LOGFILE_INCONSISTENT, BITLOCKER_ENCRYPTED,
                MFT_CORRUPTION_LIGHT/MODERATE/SEVERE, BOOT_SECTOR_DAMAGED,
                DIFFICULTY_EASY/MEDIUM/HARD/CAUTION
- 各診断 enum に explanation() メソッド

### unsafe 統計
- 全 workspace の unsafe 行数: 約 35-40 行 (変化なし)

### テスト統計
- 単体: 既存 + 新規 12 件
- 結合: 既存 + 新規 2 件
- 全 workspace: 全パス

### 動作確認サンプル
[通常モード]
```
> workbench-dryrun diagnose --physical 1 --partition 1
...
[Windows のマウント状態]
  Dirty Bit: 立っている (Windows がマウント拒否する原因)
  ...

💡 業務的な詳細説明を表示するには --verbose を付けて再実行してください。
```

[--verbose モード]
```
> workbench-dryrun diagnose --physical 1 --partition 1 --verbose
...
[Windows のマウント状態]
  Dirty Bit: 立っている (Windows がマウント拒否する原因)
  ...

===========================================
  業務説明 (営業の業務的判断のための参考情報)
===========================================

[Dirty Bit について]
  【何が起きているか】
    HDD への書き込み処理中に、何らかの理由で処理が中断された記録が
    NTFS ファイルシステムに残っています。
  
  【考えられる原因】
    - PC の電源が突然切れた (停電、シャットダウン強制終了)
    - USB HDD を Windows の「安全な取り外し」をせずに抜いた
    - システムクラッシュ、ブルースクリーン
    - アプリケーションが書き込み中に強制終了
  
  【Windows の挙動】
    未完了の書き込みによりデータが不整合な可能性があるため、
    Windows は安全のためアクセスを拒否し、chkdsk による修復を要求します。
  
  【業務的な意味】
    NTFS の構造自体は健全な場合が多く、データ復旧は十分可能です。
  
  【お客様への説明例】
    「お客様の HDD は、書き込み中に何らかの理由 (電源断、不正な取り外し等)
    で処理が中断された状態です。Windows はそのままでは安全に開けないと
    判断していますが、データ自体は失われていない可能性が高く、
    当社の専門ツールで復旧可能です。」
  
  ※ この説明文は参考情報として提供しています。
  ※ 個別案件の状況により異なる場合があります。
  ※ 法的責任を負うものではありません。
```

[CRM 貼り付けテキスト]
```
【案件番号】 260603-01
【診断結果 - 業務サマリ】
  推定ファイル数: 約 1,200 件
  復旧難易度: 中
  推定復旧成功率: 95%

【診断結果 - 技術詳細】
  Dirty Bit: 立っている
  ...

【お客様への説明 (参考)】

■ HDD の状態 (Dirty Bit):
お客様の HDD は、書き込み中に何らかの理由で処理が中断された状態です。
Windows はそのままでは安全に開けないと判断していますが、
データ自体は失われていない可能性が高く、当社の専門ツールで復旧可能です。

※ この説明文は参考情報として提供しています。
※ 個別案件の状況により異なる場合があります。
※ 法的責任を負うものではありません。
```

### 🎯 達成事項
- 営業がお客様に説明できる業務適用品質に到達
- Chouさんの観察 (Dirty Bit の理由をお客様に説明したい) を実現
- 全 9 種類の説明文を 1 ファイルに集約、業務的な調整が容易
- 免責注釈で法的リスクに配慮
- CLI と CRM テキストの両方で業務説明が利用可能

### 次のステップ
Chunk 24d-4-2 で:
- 営業向け診断書 (DOCX) を生成
- 業務管理用セクション + お客様用セクション
- 各 BusinessExplanation を DOCX に整形

→ tester エージェントへ引き継ぎ
→ tester 合格後、Chouさんが --verbose 付きで実機テスト
→ 業務的な文言の確認 (営業的に違和感ないか)
→ Chunk 24d-4-2 の指示書を私に依頼
```
