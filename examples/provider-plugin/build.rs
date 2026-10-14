fn main() {
    println!(
        "cargo:rustc-env=DEMO_TARGET={}",
        std::env::var("TARGET").expect("Cargo target")
    );
}
