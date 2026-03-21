use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    let out_dir = env::var("OUT_DIR").unwrap();

    let template = fs::read_to_string("../rsplayer_web_ui/public/index.html")
        .expect("Failed to read index.html template");

    let processed = template.replace("__VERSION__", version);

    let dest = Path::new(&out_dir).join("index.html");
    fs::write(&dest, processed).expect("Failed to write processed index.html");

    println!("cargo:rerun-if-changed=../rsplayer_web_ui/public/index.html");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
