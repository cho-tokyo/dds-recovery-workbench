//! Chunk 24d-4-1.5: 各診断項目の業務的説明文。
//!
//! 営業がお客様に説明できるよう、各診断結果に対して
//! 5 セクション (何が起きているか、原因、Windows の挙動、業務的な意味、
//! お客様への説明例) の説明文を提供する。
//!
//! ## Chouさんの業務観点
//!
//! 「Dirty Bit は何なのか、なぜ立っているか、お客様に説明したい」
//! → ツールが業務的な説明文を提供する必要がある。
//!
//! ## 配置理由 (case-manager 側に置く理由)
//!
//! 説明文 (`BusinessExplanation`) と各種診断 enum
//! ([`super::diagnostic::DirtyBitStatus`] 等) の `impl explanation()`
//! を同一クレートに置くと、Rust の孤児ルール (orphan rule) 違反を避けつつ
//! `enum.explanation()` の自然な API が実現できる。`dds-diagnostic` 側からは
//! `dds_case_manager::BusinessExplanation` 等を経由して参照する。
//!
//! ## 免責注釈
//!
//! 各お客様向け説明文には [`CUSTOMER_DISCLAIMER`] が付与される。
//! ツールは「参考情報」を提供し、法的責任は負わない。
//!
//! 関連 FR: FR-DIAG-08 (業務的説明文の提供), FR-DIAG-09 (お客様への説明テンプレート),
//!         FR-DIAG-10 (免責注釈)。

use serde::Serialize;

use super::diagnostic::{BitLockerStatus, DirtyBitStatus, LogFileStatus, RecoveryDifficulty};

/// 業務的説明文 (5 セクション構造)。
///
/// 異常状態の診断項目に紐付けられ、営業がお客様に説明する際の
/// 参考情報として使用される。`&'static str` を保持するため
/// `Deserialize` は実装しない (static データを後から構築する用途なし)。
#[derive(Debug, Clone, Serialize)]
pub struct BusinessExplanation {
    /// 一行要約 (CLI の簡易表示で使用)。
    pub summary: &'static str,
    /// 【何が起きているか】 技術的事実を平易な日本語で。
    pub what_happened: &'static str,
    /// 【考えられる原因】 業務的なシナリオ。
    pub causes: &'static [&'static str],
    /// 【Windows の挙動】 マウント拒否等の理由。
    pub windows_behavior: &'static str,
    /// 【業務的な意味】 データ復旧可能性、見積根拠。
    pub business_meaning: &'static str,
    /// 【お客様への説明例】 専門用語ゼロ、営業がそのまま使える。
    pub customer_explanation: &'static str,
}

/// 免責注釈 (お客様向け説明文に付与)。
///
/// 業務原則:
/// - 法的責任を限定 (「参考情報」として位置付け)
/// - 個別案件の特性を尊重 (画一的な決めつけを避ける)
pub const CUSTOMER_DISCLAIMER: &str = "※ この説明文は参考情報として提供しています。\n\
※ 個別案件の状況により異なる場合があります。\n\
※ 法的責任を負うものではありません。";

impl BusinessExplanation {
    /// CLI の `--verbose` 表示用フォーマット (5 セクション全て展開 + 免責注釈)。
    pub fn format_for_cli(&self, indent: &str) -> String {
        let mut s = String::new();

        s.push_str(&format!("{}【何が起きているか】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.what_happened));
        s.push('\n');

        s.push_str(&format!("{}【考えられる原因】\n", indent));
        for cause in self.causes {
            s.push_str(&format!("{}  - {}\n", indent, cause));
        }
        s.push('\n');

        s.push_str(&format!("{}【Windows の挙動】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.windows_behavior));
        s.push('\n');

        s.push_str(&format!("{}【業務的な意味】\n", indent));
        s.push_str(&format!("{}  {}\n", indent, self.business_meaning));
        s.push('\n');

        s.push_str(&format!("{}【お客様への説明例】\n", indent));
        s.push_str(&format!("{}  「{}」\n", indent, self.customer_explanation));
        s.push('\n');

        for line in CUSTOMER_DISCLAIMER.lines() {
            s.push_str(&format!("{}  {}\n", indent, line));
        }

        s
    }

    /// CRM 貼り付けテキスト用 (簡潔、お客様説明のみ + 免責注釈)。
    pub fn format_for_crm(&self) -> String {
        let mut s = String::new();
        s.push_str(self.customer_explanation);
        s.push_str("\n\n");
        s.push_str(CUSTOMER_DISCLAIMER);
        s
    }
}

// ============================================================
// 説明文の定義 (Dirty Bit)
// ============================================================

/// Dirty Bit が立っている状態の説明文。
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

/// `$LogFile` 不整合の説明文。
pub static LOGFILE_INCONSISTENT: BusinessExplanation = BusinessExplanation {
    summary: "不整合あり (未完了トランザクション)",
    what_happened: "NTFS のトランザクションログ (書き込み処理の記録領域) に、書き込み処理の途中で記録された未完了の項目が残っています。",
    causes: &[
        "書き込み処理中の予期しない中断",
        "電源断によるトランザクションの未完了",
        "USB HDD の不正な切断",
        "システムの予期しないシャットダウン",
    ],
    windows_behavior: "Windows はマウント前にこの未完了トランザクションを再生して整合性を取ろうとしますが、再生が失敗する場合はマウントを拒否します。",
    business_meaning: "ファイル管理情報の一部が未確定ですが、データファイル自体は読み出し可能です。当社のツールは直接 NTFS 構造を解析するため、トランザクションログの状態に影響されません。",
    customer_explanation: "お客様の HDD には、書き込み処理の途中で中断された記録が残っています。Windows はこの状態を「整合性に問題あり」と判断していますが、データファイル自体は無事である可能性が高く、当社で復旧可能です。",
};

/// BitLocker 暗号化の説明文。
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

/// ファイル管理情報の軽度破損 (1-10 件) の説明文。
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

/// ファイル管理情報の中度破損 (11-100 件) の説明文。
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

/// ファイル管理情報の重度破損 (101 件以上) の説明文。
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

/// Boot sector 破損の説明文。
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

/// 復旧難易度: 易の説明文。
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

/// 復旧難易度: 中の説明文。
pub static DIFFICULTY_MEDIUM: BusinessExplanation = BusinessExplanation {
    summary: "中 (部分的な障害あり、業務的に標準範囲)",
    what_happened: "Dirty Bit やトランザクションログの不整合、または小規模なファイル管理情報の破損など、部分的な障害が検出されています。",
    causes: &[
        "電源断や不正な取り外しによる中断",
        "削除ファイルが多数",
        "軽度の物理障害",
    ],
    windows_behavior: "Windows がマウント拒否、または部分的にアクセス不能になっている可能性があります。",
    business_meaning: "やや慎重な復旧プロセスが必要ですが、業務的には標準範囲で対応可能です。復旧成功率は良好です。",
    customer_explanation: "お客様の HDD には部分的な障害がありますが、データ復旧は十分可能です。当社の専門ツールで丁寧に処理いたします。",
};

/// 復旧難易度: 難の説明文。
pub static DIFFICULTY_HARD: BusinessExplanation = BusinessExplanation {
    summary: "難 (大規模な障害、ファイル単位の復旧が必要)",
    what_happened: "大規模なファイル管理情報の破損、ブートセクタ破損、BitLocker 暗号化、または完全な FS 構造破壊など、深刻な障害が検出されています。",
    causes: &[
        "深刻な物理障害",
        "誤った操作 (フォーマット、削除等)",
        "BitLocker 暗号化",
        "経年劣化の進行",
    ],
    windows_behavior: "Windows はマウントを拒否、または「フォーマットしますか?」のダイアログを表示します。",
    business_meaning: "ファイル管理情報からの通常の復旧は難度が高いですが、ファイル単位の復旧 (カービング技術) により重要データを取り出せる可能性があります。難易度は高く、復旧時間も標準より長くなります。",
    customer_explanation: "お客様の HDD には深刻な障害があり、難易度の高い案件となります。すべてのファイルの復旧をお約束はできませんが、当社の専門技術で重要なデータを取り出せる可能性があります。費用と期間が標準より高くなる可能性があります。",
};

/// 復旧難易度: 注意の説明文 (物理障害の兆候、業務担当者が慎重判断)。
pub static DIFFICULTY_CAUTION: BusinessExplanation = BusinessExplanation {
    summary: "注意 (物理障害の兆候、業務的に慎重判断が必要)",
    what_happened: "ファイル管理情報のほとんどが読めない等、HDD の物理障害の兆候が検出されています。これ以上のアクセスで状態が悪化する可能性があります。",
    causes: &[
        "ヘッドクラッシュ等の機械的故障",
        "回路基板の故障",
        "重度の経年劣化",
    ],
    windows_behavior: "Windows は HDD を認識できない、または極端に遅い動作をします。",
    business_meaning: "通常の論理障害対応では復旧難度が高い状態です。物理障害対応 (クリーンルームでの作業等) が必要な可能性があり、受注可否は業務担当者が慎重に判断する必要があります。当社では物理障害対応も実施しています。",
    customer_explanation: "お客様の HDD には物理的な障害の兆候があります。通常のソフトウェアでの復旧は難しい状態です。当社の物理障害対応の専門チームでも対応可能ですが、復旧難易度と費用については別途お見積もりをご案内します。",
};

// ============================================================
// Part B: 各診断 enum の `explanation()` メソッド
// ============================================================

impl DirtyBitStatus {
    /// 業務的説明文を取得 (異常時のみ Some)。
    pub fn explanation(&self) -> Option<&'static BusinessExplanation> {
        match self {
            Self::Dirty => Some(&DIRTY_BIT_SET),
            Self::Clean | Self::Unknown => None,
        }
    }
}

impl LogFileStatus {
    /// 業務的説明文を取得 (異常時のみ Some)。
    pub fn explanation(&self) -> Option<&'static BusinessExplanation> {
        match self {
            Self::Inconsistent => Some(&LOGFILE_INCONSISTENT),
            Self::Consistent | Self::Unknown => None,
        }
    }
}

impl BitLockerStatus {
    /// 業務的説明文を取得 (暗号化時のみ Some)。
    pub fn explanation(&self) -> Option<&'static BusinessExplanation> {
        match self {
            Self::Encrypted => Some(&BITLOCKER_ENCRYPTED),
            Self::NotEncrypted | Self::Unknown => None,
        }
    }
}

impl RecoveryDifficulty {
    /// 業務的説明文を取得 (4 段階それぞれに対応する説明文を返す)。
    pub fn explanation(&self) -> Option<&'static BusinessExplanation> {
        Some(match self {
            Self::Easy => &DIFFICULTY_EASY,
            Self::Medium => &DIFFICULTY_MEDIUM,
            Self::Hard => &DIFFICULTY_HARD,
            Self::Caution => &DIFFICULTY_CAUTION,
        })
    }
}

// ============================================================
// Part C: ヘルパーフリー関数 (件数 / フラグから説明文を選択)
// ============================================================

/// MFT 破損件数から該当する説明文を返す (0 件なら `None`)。
///
/// `u32` 型は [`FilesystemFindings::mft_corrupted_count`](super::diagnostic::FilesystemFindings::mft_corrupted_count)
/// (`usize`) を `as u32` で渡せるよう設計。
pub fn mft_corruption_explanation(count: u32) -> Option<&'static BusinessExplanation> {
    match count {
        0 => None,
        1..=10 => Some(&MFT_CORRUPTION_LIGHT),
        11..=100 => Some(&MFT_CORRUPTION_MODERATE),
        _ => Some(&MFT_CORRUPTION_SEVERE),
    }
}

/// Boot sector の異常フラグから説明文を返す (正常なら `None`)。
pub fn boot_sector_explanation(is_damaged: bool) -> Option<&'static BusinessExplanation> {
    if is_damaged {
        Some(&BOOT_SECTOR_DAMAGED)
    } else {
        None
    }
}

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
        // 業務原則: お客様向け説明文には技術用語を含めない。
        // すべての customer_explanation を確認 (MFT, $Volume, VOLUME_INFORMATION)。
        let all = [
            &DIRTY_BIT_SET,
            &LOGFILE_INCONSISTENT,
            &BITLOCKER_ENCRYPTED,
            &MFT_CORRUPTION_LIGHT,
            &MFT_CORRUPTION_MODERATE,
            &MFT_CORRUPTION_SEVERE,
            &BOOT_SECTOR_DAMAGED,
            &DIFFICULTY_EASY,
            &DIFFICULTY_MEDIUM,
            &DIFFICULTY_HARD,
            &DIFFICULTY_CAUTION,
        ];
        for exp in all {
            let t = exp.customer_explanation;
            assert!(!t.contains("MFT"), "MFT found in: {}", t);
            assert!(!t.contains("$Volume"), "$Volume found in: {}", t);
            assert!(
                !t.contains("VOLUME_INFORMATION"),
                "VOLUME_INFORMATION found in: {}",
                t
            );
        }
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
        assert!(output.contains("参考情報"));
    }

    #[test]
    fn difficulty_caution_emphasizes_human_judgment() {
        // 業務 CRITICAL: 「注意」レベルは人間判断を強調する文言が必要。
        assert!(
            DIFFICULTY_CAUTION.business_meaning.contains("業務担当者")
                || DIFFICULTY_CAUTION.business_meaning.contains("慎重"),
            "Caution must emphasize human judgment, got: {}",
            DIFFICULTY_CAUTION.business_meaning
        );
    }

    #[test]
    fn bitlocker_explanation_mentions_recovery_key() {
        assert!(BITLOCKER_ENCRYPTED.summary.contains("回復キー"));
        assert!(BITLOCKER_ENCRYPTED
            .customer_explanation
            .contains("回復キー"));
    }

    #[test]
    fn disclaimer_format() {
        assert!(CUSTOMER_DISCLAIMER.contains("参考情報"));
        assert!(CUSTOMER_DISCLAIMER.contains("法的責任"));
        assert!(CUSTOMER_DISCLAIMER.contains("個別案件"));
    }

    // ---- Part B / C のテスト ----

    #[test]
    fn dirty_bit_clean_returns_no_explanation() {
        assert!(DirtyBitStatus::Clean.explanation().is_none());
        assert!(DirtyBitStatus::Unknown.explanation().is_none());
    }

    #[test]
    fn dirty_bit_set_returns_some_explanation() {
        let exp = DirtyBitStatus::Dirty.explanation().expect("dirty has exp");
        assert!(exp.summary.contains("Windows"));
    }

    #[test]
    fn mft_corruption_classification() {
        // 0 件は None、1-10 は Light、11-100 は Moderate、101+ は Severe。
        assert!(mft_corruption_explanation(0).is_none());
        assert!(std::ptr::eq(
            mft_corruption_explanation(5).unwrap(),
            &MFT_CORRUPTION_LIGHT,
        ));
        assert!(std::ptr::eq(
            mft_corruption_explanation(50).unwrap(),
            &MFT_CORRUPTION_MODERATE,
        ));
        assert!(std::ptr::eq(
            mft_corruption_explanation(200).unwrap(),
            &MFT_CORRUPTION_SEVERE,
        ));
    }

    #[test]
    fn difficulty_returns_explanation() {
        // 4 段階すべてが説明文を持つこと。
        assert!(RecoveryDifficulty::Easy.explanation().is_some());
        assert!(RecoveryDifficulty::Medium.explanation().is_some());
        assert!(RecoveryDifficulty::Hard.explanation().is_some());
        assert!(RecoveryDifficulty::Caution.explanation().is_some());
    }

    #[test]
    fn log_file_inconsistent_returns_explanation() {
        assert!(LogFileStatus::Inconsistent.explanation().is_some());
        assert!(LogFileStatus::Consistent.explanation().is_none());
        assert!(LogFileStatus::Unknown.explanation().is_none());
    }

    #[test]
    fn bitlocker_encrypted_returns_explanation() {
        assert!(BitLockerStatus::Encrypted.explanation().is_some());
        assert!(BitLockerStatus::NotEncrypted.explanation().is_none());
        assert!(BitLockerStatus::Unknown.explanation().is_none());
    }

    #[test]
    fn boot_sector_explanation_flag_routing() {
        assert!(boot_sector_explanation(true).is_some());
        assert!(boot_sector_explanation(false).is_none());
    }
}
