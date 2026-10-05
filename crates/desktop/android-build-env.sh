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

# dx must not download wasm-bindgen, wasm-opt or esbuild (F-Droid requires every build tool to
# come from source or the distro): take them from PATH instead, i.e. cargo-installed
# wasm-bindgen-cli and Debian trixie's binaryen and esbuild, in the F-Droid recipe and in
# docker/Dockerfile.android alike. The wasm-opt version changes the output, so both must agree.
export NO_DOWNLOADS=1
export DX_TELEMETRY_ENABLED=false
# NO_DOWNLOADS alone is not enough: dx still prefers a tool it downloaded earlier into its data
# dir (~/.local/share/.dx/tools) over PATH. The self-hosted runner keeps $HOME between jobs and
# had wasm-opt 129 and esbuild 0.27.3 there from older builds, so 5.4.1's CI APKs differed from
# F-Droid's clean build. A dx data dir of our own, with no tools in it, rules that out.
export DX_HOME="${TMPDIR:-/tmp}/rsplayer-android-dx"
rm -rf "$DX_HOME/tools"
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
