//! Cache key for `public/tw.css`. The server caches static files for days
//! and `asset!()` would embed absolute paths (breaking reproducible builds),
//! so the stylesheet URL carries a hash of the file's content instead: any
//! CSS change gets a new URL, identical sources give identical builds.

use std::fs;

const CSS: &str = "public/tw.css";

fn main() {
    println!("cargo:rerun-if-changed={CSS}");
    // Absent on a fresh checkout until `cargo make build_css` has run.
    let hash = fs::read(CSS).map_or(0, |bytes| fnv1a(&bytes));
    println!("cargo:rustc-env=TW_CSS_HASH={hash:016x}");
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
