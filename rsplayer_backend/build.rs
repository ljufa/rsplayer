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

    let template = fs::read_to_string("../rsplayer_web_ui/public/index.html")
        .expect("Failed to read index.html template");

    let processed = template.replace("__VERSION__", &version);

    let dest = Path::new(&out_dir).join("index.html");
    fs::write(&dest, processed).expect("Failed to write processed index.html");

    println!("cargo:rerun-if-changed=../rsplayer_web_ui/public/index.html");
    println!("cargo:rerun-if-changed=../Makefile.toml");
}
