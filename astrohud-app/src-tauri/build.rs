fn main() {
    println!("cargo:rustc-check-cfg=cfg(mobile)");
    if cfg!(feature = "app") {
        tauri_build::build();
    }
}
