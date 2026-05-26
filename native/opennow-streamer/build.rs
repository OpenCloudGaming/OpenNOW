fn main() {
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=wayland-client");
        println!("cargo:rustc-link-lib=gstwayland-1.0");
    }
}
