fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/DELAYLOAD:mfplat.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && std::env::var_os("CARGO_FEATURE_LINUX_FFMPEG_BUNDLED").is_some()
    {
        // OpenH264 is compiled from C++ sources. The production Linux binary
        // carries that runtime and GCC's unwinder just like its other native
        // media libraries.
        println!("cargo:rustc-link-lib=static=stdc++");
        println!("cargo:rustc-link-lib=static=gcc");
        println!("cargo:rustc-link-lib=static=gcc_eh");
    }
}
