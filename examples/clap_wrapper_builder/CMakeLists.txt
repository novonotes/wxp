cmake_minimum_required(VERSION 3.21)
project(clap_wrapper_builder)
# enable_language(OBJCXX)

# C++23 以上を要求（clap-wrapper と AudioUnitSDK の要件に合わせる）
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_OSX_DEPLOYMENT_TARGET 10.15)

set(CLAP_SDK_ROOT "${CMAKE_CURRENT_LIST_DIR}/clap")

set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

# キャッシュ変数として静的リンクライブラリのパスを受け取る
set(CLAP_WRAPPER_BUILDER_TARGET_LIB "" CACHE FILEPATH "Path to the static library to wrap")
option(CLAP_WRAPPER_BUILDER_BUILD_STANDALONE "Build standalone app target in clap_wrapper_builder" OFF)
set(CLAP_WRAPPER_BUILDER_STANDALONE_OUTPUT_NAME "" CACHE STRING "Output name for standalone app")
set(CLAP_WRAPPER_BUILDER_STANDALONE_PLUGIN_ID "" CACHE STRING "Plugin ID used by standalone app")

# 必須パラメータの検証
if(NOT CLAP_WRAPPER_BUILDER_TARGET_LIB)
    message(FATAL_ERROR "CLAP_WRAPPER_BUILDER_TARGET_LIB must be specified. Use: -DCLAP_WRAPPER_BUILDER_TARGET_LIB=/path/to/your/plugin.a")
endif()

if(NOT EXISTS ${CLAP_WRAPPER_BUILDER_TARGET_LIB})
    message(FATAL_ERROR "Static library not found: ${CLAP_WRAPPER_BUILDER_TARGET_LIB}")
endif()

# プラグイン名の取得（静的ライブラリのファイル名から）
get_filename_component(PLUGIN_NAME ${CLAP_WRAPPER_BUILDER_TARGET_LIB} NAME_WE)
string(REGEX REPLACE "^lib" "" PLUGIN_NAME ${PLUGIN_NAME})
string(REGEX REPLACE "[^A-Za-z0-9_]" "_" PLUGIN_NAME_SAFE ${PLUGIN_NAME})

message(STATUS "CLAP Wrapper Builder Configuration:")
message(STATUS "  Target Library: ${CLAP_WRAPPER_BUILDER_TARGET_LIB}")
message(STATUS "  Plugin Name: ${PLUGIN_NAME}")
message(STATUS "  Safe Plugin Name: ${PLUGIN_NAME_SAFE}")

# clap-wrapper サブプロジェクトを追加
add_subdirectory(clap-wrapper)

# 静的ライブラリターゲットの作成
# 外部の静的ライブラリをインポートライブラリとして設定
add_library(${PLUGIN_NAME_SAFE}_impl STATIC IMPORTED)

if(MSVC)
  set_target_properties(${PLUGIN_NAME_SAFE}_impl PROPERTIES
      IMPORTED_LOCATION ${CLAP_WRAPPER_BUILDER_TARGET_LIB}
      INTERFACE_LINK_LIBRARIES "ntdll.lib;ws2_32.lib;userenv.lib"
  )
endif()

if(APPLE)
  set_target_properties(${PLUGIN_NAME_SAFE}_impl PROPERTIES
      IMPORTED_LOCATION ${CLAP_WRAPPER_BUILDER_TARGET_LIB}
      INTERFACE_LINK_LIBRARIES "-framework AppKit;-framework Foundation"
  )
endif()
# CLAP エントリーポイント用のソースファイルを生成
# ただし、 CLAP のエントリーポイントは clack が用意したものが
# CLAP_WRAPPER_BUILDER_TARGET_LIB の中に存在していて、
# ここでは CLAP_WRAPPER_BUILDER_TARGET_LIB の中の get_clap_entry() 関数を呼び出すことで
# CLAP_WRAPPER_BUILDER_TARGET_LIB の中の clap_entry のシンボルが strip されずに
# 正しく DLL 中にも残るようにしている。
set(ENTRY_SOURCE_FILE "${CMAKE_CURRENT_BINARY_DIR}/${PLUGIN_NAME_SAFE}_entry.cpp")
file(WRITE ${ENTRY_SOURCE_FILE} "
#include <clap/clap.h>
extern \"C\" {
  clap_plugin_entry_t get_clap_entry();
  const clap_plugin_entry_t internal_clap_entry = get_clap_entry();
}

// for Windows only
#if defined(_MSC_VER)
#pragma comment(linker,  \"/export:clap_entry\")
#endif
")

# ビルドするプラグインフォーマットを設定
set(PLUGIN_FORMATS "CLAP" "VST3")
if(APPLE)
    list(APPEND PLUGIN_FORMATS "AUV2")
endif()

# プラグインのバンドル情報
set(BUNDLE_ID "org.wrapper.${PLUGIN_NAME_SAFE}")
set(BUNDLE_VERSION "1.0.0")

# make_clapfirst_plugins を使用してマルチフォーマットプラグインを作成
set(CLAPFIRST_COMMON_ARGS
    TARGET_NAME ${PLUGIN_NAME_SAFE}
    IMPL_TARGET ${PLUGIN_NAME_SAFE}_impl
    OUTPUT_NAME "${PLUGIN_NAME}"
    ENTRY_SOURCE ${ENTRY_SOURCE_FILE}
    PLUGIN_FORMATS ${PLUGIN_FORMATS}
    BUNDLE_IDENTIFIER "${BUNDLE_ID}"
    BUNDLE_VERSION "${BUNDLE_VERSION}"
    COPY_AFTER_BUILD FALSE
    WINDOWS_FOLDER_VST3 FALSE
)

if(CLAP_WRAPPER_BUILDER_BUILD_STANDALONE)
    if(NOT CLAP_WRAPPER_BUILDER_STANDALONE_PLUGIN_ID)
        message(FATAL_ERROR "CLAP_WRAPPER_BUILDER_STANDALONE_PLUGIN_ID must be specified when CLAP_WRAPPER_BUILDER_BUILD_STANDALONE=ON")
    endif()

    if(NOT CLAP_WRAPPER_BUILDER_STANDALONE_OUTPUT_NAME)
        set(CLAP_WRAPPER_BUILDER_STANDALONE_OUTPUT_NAME "${PLUGIN_NAME} Standalone")
    endif()

    message(STATUS "  Standalone Enabled: ON")
    message(STATUS "  Standalone Output Name: ${CLAP_WRAPPER_BUILDER_STANDALONE_OUTPUT_NAME}")
    message(STATUS "  Standalone Plugin ID: ${CLAP_WRAPPER_BUILDER_STANDALONE_PLUGIN_ID}")

    make_clapfirst_plugins(
        ${CLAPFIRST_COMMON_ARGS}
        STANDALONE_CONFIGURATIONS standalone "${CLAP_WRAPPER_BUILDER_STANDALONE_OUTPUT_NAME}" "${CLAP_WRAPPER_BUILDER_STANDALONE_PLUGIN_ID}"
    )
else()
    make_clapfirst_plugins(${CLAPFIRST_COMMON_ARGS})
endif()

if(MSVC AND TARGET ${PLUGIN_NAME_SAFE}_standalone)
    target_link_libraries(${PLUGIN_NAME_SAFE}_standalone PRIVATE runtimeobject.lib)
endif()

target_include_directories(${PLUGIN_NAME_SAFE}_clap
  PRIVATE
  ${CLAP_SDK_ROOT}/include
)
