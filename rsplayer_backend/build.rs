use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let makefile = fs::read_to_string("../Makefile.toml").expect("Failed to read Makefile.toml");
    let version = makefile
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("RELEASE_VERSION")
                .map(|rest| rest.trim().trim_start_matches('=').trim().trim_matches('"'))
        })
        .expect("RELEASE_VERSION not found in Makefile.toml")
        .to_string();

    println!("cargo:rustc-env=APP_VERSION={version}");

    let out_dir = env::var("OUT_DIR").unwrap();

    let dx_index = "../rsplayer_web_ui/target/dx/rsplayer_web_ui/release/web/public/index.html";
    let index_html = if env::var("PROFILE").as_deref() == Ok("release") {
        fs::read_to_string(dx_index)
            .expect("index.html not found — run `dx build --release --platform web` in rsplayer_web_ui first")
    } else {
        fs::read_to_string(dx_index).unwrap_or_else(|_| String::from("<!-- dev build: UI not embedded -->"))
    };

    let dest = Path::new(&out_dir).join("index.html");
    fs::write(&dest, index_html).expect("Failed to write index.html");

    println!("cargo:rerun-if-changed={dx_index}");
    println!("cargo:rerun-if-changed=../rsplayer_web_ui/target/dx/rsplayer_web_ui/release/web/public/tw.css");
    println!("cargo:rerun-if-changed=../Makefile.toml");
}
