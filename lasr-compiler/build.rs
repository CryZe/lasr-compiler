use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../lasr-runtime/Cargo.toml");
    for file in runtime_source_files() {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir
        .parent()
        .expect("failed to find workspace root");
    let target_dir = workspace_dir.join("target").join("lasr-buildrs");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .current_dir(workspace_dir)
        .env("CARGO_TARGET_DIR", &target_dir)
        .args([
            "build",
            "-p",
            "lasr-runtime",
            "--release",
            "--target",
            "wasm32-wasip1",
        ])
        .status()
        .expect("failed to run cargo build for lasr-runtime");

    assert!(status.success(), "failed to build lasr-runtime wasm");

    let built_wasm = target_dir
        .join("wasm32-wasip1")
        .join("release")
        .join("lasr_runtime.wasm");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing OUT_DIR"));
    let staged_wasm = out_dir.join("lasr_runtime.wasm");

    fs::copy(&built_wasm, &staged_wasm).expect("failed to stage lasr_runtime.wasm into OUT_DIR");
}

fn runtime_source_files() -> Vec<PathBuf> {
    let runtime_src = PathBuf::from("../lasr-runtime/src");
    let mut files = Vec::new();
    collect_files(&runtime_src, &mut files);
    files
}

fn collect_files(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("failed to read runtime source directory");
    for entry in entries {
        let path = entry.expect("failed to read runtime source entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}
