#!/bin/bash
# build_and_install.sh - example_gain_plugin のビルド＆インストール（一括）
#
# build.sh と install.sh をまとめて実行するショートカットスクリプト。
# 通常の開発サイクルではこのスクリプトを使えばよい。
#
# 使い方:
#   ./script/build_and_install.sh [Debug|Release]
#
# 引数:
#   Debug|Release - ビルド構成（省略時は Debug）

set -e  # エラー発生時にスクリプトを停止

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# ターミナル出力の色付け用エスケープコード
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'  # No Color（色のリセット）

# 第 1 引数を BUILD_CONFIG に設定。省略時は "Debug"。
BUILD_CONFIG="${1:-Debug}"

echo -e "${BLUE}example_gain_plugin (CLAP) をインストール中...${NC}"
echo "ビルド構成: $BUILD_CONFIG"
echo ""

echo "1. CLAPプラグインをビルドしています..."
"$SCRIPT_DIR/build.sh" "$BUILD_CONFIG"

echo ""
echo "2. CLAPプラグインをインストールしています..."
"$SCRIPT_DIR/install.sh"

echo ""
echo -e "${GREEN}example_gain_plugin のインストールが完了しました！${NC}"
echo "インストールされたプラグイン:"
echo "  - WXP Example Gain.clap (CLAP形式)"
