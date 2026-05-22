#!/usr/bin/env bash
set -e

# Resolve SDK version and toolchain suffix written during image build.
OSXCROSS_SDK_VERSION=$(cat /opt/osxcross/SDK_VERSION)
TOOL_VER=$(cat /opt/osxcross/TOOL_VERSION)
SDK_PATH=$(cat /opt/osxcross/SDK_PATH)

export OSXCROSS_SDK_VERSION
export CROSS_TARGET=x86_64-apple-darwin
export CROSS_SYSROOT="$SDK_PATH"

# Minimum macOS version — cpal uses AudioHardwareDestroyProcessTap (macOS 14+)
export MACOSX_DEPLOYMENT_TARGET=14.0

tools_prefix="x86_64-apple-darwin${TOOL_VER}"

# Use the versioned tool names (e.g. x86_64-apple-darwin22.4-clang)
# — these are the real osxcross wrappers that know how to find the SDK.
export CC_x86_64_apple_darwin="${tools_prefix}-clang"
export CXX_x86_64_apple_darwin="${tools_prefix}-clang++"
export AR_x86_64_apple_darwin="${tools_prefix}-ar"
export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="${tools_prefix}-clang"

# C/C++ flags — force libc++ and osxcross linker
export CFLAGS_x86_64_apple_darwin="-stdlib=libc++ -fuse-ld=${tools_prefix}-ld"
export CXXFLAGS_x86_64_apple_darwin="-stdlib=libc++ -fuse-ld=${tools_prefix}-ld"

# bindgen
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_apple_darwin="--sysroot=$SDK_PATH -idirafter/usr/include"

exec "$@"