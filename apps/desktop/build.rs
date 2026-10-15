fn main() {
    println!("cargo:rerun-if-env-changed=DIPPER_BUILD_REVISION");
    let revision = std::env::var("DIPPER_BUILD_REVISION").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=DIPPER_BUILD_REVISION={revision}");
    tauri_build::build();
}
