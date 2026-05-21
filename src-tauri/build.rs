use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    sync_codex_icon();
    tauri_build::build();
}

fn sync_codex_icon() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let codex_app_path =
        env::var("CODEX_APP_PATH").unwrap_or_else(|_| "/Applications/Codex.app".to_string());
    let codex_icon_source = PathBuf::from(codex_app_path)
        .join("Contents")
        .join("Resources")
        .join("icon.icns");

    if !codex_icon_source.is_file() {
        panic!("Codex icon not found: {}", codex_icon_source.display());
    }

    let icons_dir = manifest_dir.join("icons");
    fs::create_dir_all(&icons_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", icons_dir.display()));

    let tauri_icon_png = icons_dir.join("icon.png");
    let status = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(&codex_icon_source)
        .arg("--out")
        .arg(&tauri_icon_png)
        .status()
        .unwrap_or_else(|error| panic!("failed to run sips for Codex icon conversion: {error}"));

    if !status.success() {
        panic!(
            "failed to convert Codex icon {} to {}",
            codex_icon_source.display(),
            tauri_icon_png.display()
        );
    }

    println!("cargo:rerun-if-env-changed=CODEX_APP_PATH");
    println!("cargo:rerun-if-changed={}", codex_icon_source.display());
}
