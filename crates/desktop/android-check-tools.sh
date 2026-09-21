#!/bin/sh
# Fails unless the toolchain matches the versions pinned in android-build-env.sh. Run by the
# release job before building, so a runner with a newer rustc can't quietly publish an APK
# that F-Droid's build (which uses the pinned versions) cannot reproduce.
set -u
cd "$(dirname "$0")" || exit 1
. ./android-build-env.sh

status=0
check() {
  # $1 = tool, $2 = wanted version, $3 = what the tool reports
  if printf '%s' "$3" | grep -qF -- "$2"; then
    echo "ok      $1 $2"
  else
    echo "::error::$1 must be $2 for reproducible Android builds, found: $3"
    status=1
  fi
}

check rustc "$ANDROID_RUST_VERSION" "$(rustc --version 2>&1)"
check dioxus-cli "$ANDROID_DX_VERSION" "$(dx --version 2>&1 | head -n 1)"
check tauri-cli "$ANDROID_TAURI_CLI_VERSION" "$(cargo tauri --version 2>&1 | head -n 1)"

for target in aarch64-linux-android armv7-linux-androideabi wasm32-unknown-unknown; do
  if rustup target list --installed | grep -qx "$target"; then
    echo "ok      rust target $target"
  else
    echo "::error::Rust target $target is not installed for the active toolchain"
    status=1
  fi
done
exit $status
