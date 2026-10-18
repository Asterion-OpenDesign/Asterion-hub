fn main() {
    // the exe's embedded icon comes from icons/icon.ico: rebuild the Windows resource when the icons change
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
