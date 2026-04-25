use std::env;


fn main() {
    // 1. Get version from Cargo.toml
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    println!("cargo:rustc-env=MAAZDB_VERSION={}", version);

    // 2. Get current build time (Unix timestamp)
    let build_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=MAAZDB_BUILD_TIME={}", build_time);

    // 3. Windows-specific: Embed Icon
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "windows" {
        // This requires 'icon.rc' to exist in your root folder
        // and 'embed-resource' in [build-dependencies]
        let _ = embed_resource::compile("icon.rc", embed_resource::NONE);
    }

    // Re-run if these files change
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=icon.rc");
    println!("cargo:rerun-if-changed=images/maazdb.ico");
}