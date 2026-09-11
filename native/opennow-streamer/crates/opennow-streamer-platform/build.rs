use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let output = cc::Build::new()
        .get_compiler()
        .to_command()
        .arg("--print-resource-dir")
        .output()
        .expect("query the macOS C compiler's runtime directory");
    assert!(
        output.status.success(),
        "could not locate the macOS Clang runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resource_dir = String::from_utf8(output.stdout).expect("UTF-8 Clang resource directory");
    let runtime_dir = PathBuf::from(resource_dir.trim()).join("lib/darwin");
    assert!(
        runtime_dir.join("libclang_rt.osx.a").is_file(),
        "macOS Clang runtime is missing from {}; install the Xcode command-line tools",
        runtime_dir.display()
    );
    println!("cargo:rustc-link-search=native={}", runtime_dir.display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
