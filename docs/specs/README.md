# FS仕様書

このディレクトリには各ファイルシステムの仕様書を配置します。

Claude Code（特に builder エージェント）は実装前に該当ディレクトリの仕様書を必ず参照してください。

---

## ディレクトリ構成（推奨）

```
docs/specs/
├── ntfs-references/
│   ├── carrier-fsfa-chapter11-12.pdf    ← Brian Carrier著書（要購入）
│   ├── linux-ntfs-docs.pdf              ← Linux NTFS Documentation Project
│   ├── ntfs-3g-readonly-source/         ← 参考ソース（GPL、参照のみ）
│   └── notes.md                         ← 実装メモ
├── exfat/
│   ├── exfat-specification-microsoft-2019.pdf
│   └── notes.md
├── fat32/
│   ├── fat32-whitepaper-microsoft.pdf
│   └── notes.md
├── apfs/                                ← Phase 3で使用
├── ext4/                                ← Phase 3で使用
└── hfsplus/                             ← Phase 3で使用
```

---

## 仕様書の入手方法

### NTFS（最重要、最難関）

⚠️ Microsoftは完全仕様を公開していません。以下を組み合わせます:

**必須**:
- 📕 Brian Carrier『File System Forensic Analysis』(約8,000円、購入必須)
  - 特に第11-12章（NTFS）
  - https://www.amazon.co.jp/dp/0321268172

**無料リソース**:
- Linux NTFS Documentation Project: https://flatcap.github.io/linux-ntfs/ntfs/
- Russon の NTFS Documentation: https://github.com/libyal/libfsntfs/blob/main/documentation/

**参考ソース**（コードコピー不可、仕様理解のみ）:
- ntfs-3g: https://github.com/tuxera/ntfs-3g
- The Sleuth Kit (libtsk_ntfs): https://github.com/sleuthkit/sleuthkit

### exFAT

✅ Microsoftが公式仕様を公開（2019年〜）

- Microsoft exFAT File System Specification
- https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification

### FAT32

✅ Microsoftが公式仕様を公開

- "Microsoft FAT32 File System Specification" Hardware White Paper
- https://learn.microsoft.com/en-us/previous-versions/windows/embedded/ms905177(v=msdn.10)

---

## 参照ルール（CLAUDE.md より抜粋）

```
- 実装時は必ず docs/specs/<該当FS>/ 配下を参照すること
- Web検索で見つけた非公式情報を一次ソースとして使わない
- 仕様に曖昧さがある場合は OSS実装ソースを参照（コピー不可、理解のみ）
- 不明点は推測せず、必ず確認質問を出すこと
```

---

## 仕様書取得後の追加手順

1. PDFを所定ディレクトリに配置
2. `notes.md` に実装で重要なセクション番号と要点をメモ
3. CLAUDE.md の「ファイルシステム仕様の参照ルール」を確認
4. builder エージェント起動時、該当 specs ディレクトリを明示的に指示する

---

## 現状

- [ ] NTFS仕様書 配置済み
- [ ] exFAT仕様書 配置済み
- [ ] FAT32仕様書 配置済み
- [ ] Brian Carrier書籍 入手済み

**仕様書が揃うまで、Chunk 4以降（NTFS実装）は着手しないこと。**  
Chunk 1〜3（core, fs-common, disk-io の基盤実装）は仕様書なしで進められます。
