# Source this before building the Android app (release APKs, F-Droid recipe):
#
#   . crates/desktop/android-build-env.sh

# Tool versions the published APKs are built with. The F-Droid recipe (metadata/de.rsplayer.app.yml
# in fdroiddata) installs exactly these, so keep both in sync: a different rustc, dx or tauri-cli
# can change the output and the F-Droid build would no longer match our signed APK.
# android-check-tools.sh fails the release job when the runner differs.
ANDROID_RUST_VERSION=1.98.1
ANDROID_DX_VERSION=0.7.10
ANDROID_TAURI_CLI_VERSION=2.11.4
#
# It makes the output independent of where the build runs, so that the APK F-Droid builds
# from source is byte-identical to the one we sign and publish (reproducible builds).
# rustc embeds absolute source paths (panic locations); map them to fixed names.
#
# RUSTFLAGS replaces `build.rustflags` from .cargo/config.toml, so `--cfg tokio_unstable`
# has to be repeated here. Keep both in sync.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
sysroot="$(rustc --print sysroot)"
rustc_commit="$(rustc -vV | sed -n 's/^commit-hash: //p')"
# The most specific mapping goes last: rust-src (only present with the rust-src component)
# is reported as /rustc/<commit>/... by toolchains without it, such as Debian's.
export RUSTFLAGS="--cfg tokio_unstable \
--remap-path-prefix=$cargo_home=/cargo \
--remap-path-prefix=$rustup_home=/rustup \
--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$rustc_commit"
unset cargo_home rustup_home sysroot rustc_commit
