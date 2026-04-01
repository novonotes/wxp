#!/bin/bash
# build_wrapper.sh - gain_plugin の VST3 / AU ラッパーをビルド
#
# 使い方:
#   ./script/build_wrapper.sh [Debug|Release]
#
# 環境変数:
#   SKIP_CLAP_BUILD=1 を指定すると、事前の CLAP ビルドをスキップする。

set -e
set -u

case "$(uname -s)" in
    Darwin*)
        OS="macos"
        ;;
    Linux*)
        OS="linux"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        OS="windows"
        ;;
    *)
        echo "エラー: 未対応のOS $(uname -s)"
        exit 1
        ;;
esac

BUILD_CONFIG="${1:-Debug}"

case "$BUILD_CONFIG" in
    Debug|debug|DEBUG)
        BUILD_CONFIG="Debug"
        ;;
    Release|release|RELEASE)
        BUILD_CONFIG="Release"
        ;;
    *)
        echo "エラー: 無効なビルド構成: $BUILD_CONFIG"
        exit 1
        ;;
esac

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PLUGIN_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
REPO_ROOT="$( cd "$PLUGIN_ROOT/../.." && pwd )"
WRAPPER_DIR="$REPO_ROOT/examples/clap_wrapper_builder"

if [[ "${SKIP_CLAP_BUILD:-0}" != "1" ]]; then
    echo "CLAPプラグインを先にビルドします..."
    "$SCRIPT_DIR/build.sh" "$BUILD_CONFIG"
fi

if [[ "$OS" == "linux" ]]; then
    echo "Linux では VST3 / AU ラッパーのビルドをスキップします"
    exit 0
fi

echo "VST3 / AU ラッパーをビルドしています..."
(
    cd "$WRAPPER_DIR"
    ./build_wrapper_plugin.sh "$REPO_ROOT/target/bundled/WXP Example Gain.clap" "WXP Example Gain" "$BUILD_CONFIG"
)

echo "VST3 / AU ラッパーのビルドが完了しました"
