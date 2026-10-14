fn main() {
    println!("cargo:rerun-if-changed=native/capture.c");
    println!("cargo:rerun-if-env-changed=DIPPER_PIPEWIRE_PREFIX");
    let prefix = std::env::var("DIPPER_PIPEWIRE_PREFIX").unwrap_or_else(|_| "/usr".into());
    let include = if std::path::Path::new(&prefix)
        .join("usr/include/pipewire-0.3")
        .exists()
    {
        format!("{prefix}/usr/include")
    } else {
        format!("{prefix}/include")
    };
    cc::Build::new()
        .file("native/capture.c")
        .include(format!("{include}/pipewire-0.3"))
        .include(format!("{include}/spa-0.2"))
        .flag_if_supported("-std=c11")
        .compile("dipper_capture");
    // Link the installed runtime soname; extracted headers need no system installation.
    println!("cargo:rustc-link-lib=dylib:+verbatim=libpipewire-0.3.so.0");
}
