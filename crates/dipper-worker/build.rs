use std::{env, path::PathBuf, process::Command};

const REVISION: &str = "0c7be696371069e0baa2515ed7aa08d5a5473aa6";

fn main() {
    println!("cargo:rerun-if-env-changed=DIPPER_WHISPER_DIR");
    println!("cargo:rerun-if-changed=native/whisper_bridge.cpp");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("native/whisper_bridge.cpp");
    if let Some(directory) = env::var_os("DIPPER_WHISPER_DIR") {
        let directory = PathBuf::from(directory);
        let revision = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&directory)
            .output()
            .expect("inspect configured whisper.cpp revision");
        assert!(
            revision.status.success()
                && String::from_utf8_lossy(&revision.stdout).trim() == REVISION,
            "DIPPER_WHISPER_DIR must contain exact pinned whisper.cpp revision {REVISION}"
        );
        let library = directory.join("build/bin");
        assert!(
            library.join("libwhisper.so").exists() && library.join("libggml-vulkan.so").exists(),
            "configured whisper.cpp needs matching shared Vulkan build/bin libraries"
        );
        build
            .define("DIPPER_HAS_WHISPER", None)
            .include(directory.join("include"))
            .include(directory.join("ggml/include"));
        // cc emits the bridge first so downstream native symbols follow it in link order.
        build.compile("dipper_whisper_bridge");
        println!("cargo:rustc-link-search=native={}", library.display());
        for name in ["whisper", "ggml", "ggml-base", "ggml-vulkan"] {
            println!("cargo:rustc-link-lib=dylib={name}");
        }
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library.display());
    } else {
        // Builds and public transport/audio tests remain possible without a native install.
        // Starting recognition reports an explicit unavailable-runtime failure.
        println!(
            "cargo:warning=DIPPER_WHISPER_DIR unset; worker reports native runtime unavailable"
        );
        build.compile("dipper_whisper_bridge");
    }
}
