#!/usr/bin/env bash
#
# DDS Recovery Workbench - 初期セットアップスクリプト
#
# 実行: bash scripts/setup.sh
#

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "================================================"
echo "DDS Recovery Workbench - Setup"
echo "================================================"
echo ""

# 1. Rust環境チェック
echo "[1/4] Checking Rust toolchain..."
if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found. Install Rust from https://rustup.rs/"
    exit 1
fi

RUST_VERSION=$(rustc --version | awk '{print $2}')
echo "  ✓ Rust: $RUST_VERSION"

# 1.75以上チェック（簡易）
REQUIRED="1.75"
if ! printf '%s\n%s\n' "$REQUIRED" "$RUST_VERSION" | sort -V -C; then
    echo "  ⚠ Warning: Rust 1.75+ recommended"
fi

# 2. 推奨ツールチェック
echo ""
echo "[2/4] Checking recommended tools..."
for tool in cargo-tarpaulin cargo-audit; do
    if cargo install --list | grep -q "^${tool} "; then
        echo "  ✓ $tool installed"
    else
        echo "  ⚠ $tool not installed (recommended: cargo install $tool)"
    fi
done

# 3. ワークスペース初期ビルド
echo ""
echo "[3/4] Initial build check..."
echo "  Running: cargo check --workspace"
if cargo check --workspace 2>&1 | tail -5; then
    echo "  ✓ Workspace compiles"
else
    echo "  ⚠ Build errors present (expected before Chunk 1 implementation)"
fi

# 4. ガイダンス
echo ""
echo "[4/4] Next steps:"
echo ""
echo "  1. Start Claude Code:"
echo "       claude"
echo ""
echo "  2. Initial prompt:"
echo "       > CLAUDE.md を読んだ後、docs/first_chunk.md の"
echo "       > 指示に従って Chunk 1 を実装してください。"
echo ""
echo "  3. (Optional) Place FS specs:"
echo "       - NTFS specs in docs/specs/ntfs-references/"
echo "       - exFAT spec in docs/specs/exfat/"
echo "       - FAT32 whitepaper in docs/specs/fat32/"
echo ""
echo "  4. (Linux only) Generate test fixtures:"
echo "       sudo python3 fixtures/scripts/gen_ntfs_basic.py"
echo ""
echo "================================================"
echo "Setup complete. Ready to start development."
echo "================================================"
