extern crate bindgen;
extern crate cc;
extern crate num_cpus;
extern crate pkg_config;

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;

use bindgen::callbacks::{
    EnumVariantCustomBehavior, EnumVariantValue, IntKind, MacroParsingBehavior, ParseCallbacks,
};

#[derive(Debug)]
struct Library {
    name: &'static str,
    is_feature: bool,
}

impl Library {
    fn feature_name(&self) -> Option<String> {
        if self.is_feature {
            Some("CARGO_FEATURE_".to_string() + &self.name.to_uppercase())
        } else {
            None
        }
    }
}

static LIBRARIES: &[Library] = &[
    Library {
        name: "avcodec",
        is_feature: true,
    },
    Library {
        name: "avdevice",
        is_feature: true,
    },
    Library {
        name: "avfilter",
        is_feature: true,
    },
    Library {
        name: "avformat",
        is_feature: true,
    },
    Library {
        name: "avresample",
        is_feature: true,
    },
    Library {
        name: "avutil",
        is_feature: false,
    },
    Library {
        name: "postproc",
        is_feature: true,
    },
    Library {
        name: "swresample",
        is_feature: true,
    },
    Library {
        name: "swscale",
        is_feature: true,
    },
];

#[derive(Debug)]
struct Callbacks;

impl ParseCallbacks for Callbacks {
    fn int_macro(&self, _name: &str, value: i64) -> Option<IntKind> {
        let ch_layout_prefix = "AV_CH_";
        let codec_cap_prefix = "AV_CODEC_CAP_";
        let codec_flag_prefix = "AV_CODEC_FLAG_";
        let error_max_size = "AV_ERROR_MAX_STRING_SIZE";

        if _name.starts_with(ch_layout_prefix) {
            Some(IntKind::ULongLong)
        } else if value >= i32::MIN as i64
            && value <= i32::MAX as i64
            && (_name.starts_with(codec_cap_prefix) || _name.starts_with(codec_flag_prefix))
        {
            Some(IntKind::UInt)
        } else if _name == error_max_size {
            Some(IntKind::Custom {
                name: "usize",
                is_signed: false,
            })
        } else if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            Some(IntKind::Int)
        } else {
            None
        }
    }

    fn enum_variant_behavior(
        &self,
        _enum_name: Option<&str>,
        original_variant_name: &str,
        _variant_value: EnumVariantValue,
    ) -> Option<EnumVariantCustomBehavior> {
        let dummy_codec_id_prefix = "AV_CODEC_ID_FIRST_";
        if original_variant_name.starts_with(dummy_codec_id_prefix) {
            Some(EnumVariantCustomBehavior::Constify)
        } else {
            None
        }
    }

    // https://github.com/rust-lang/rust-bindgen/issues/687#issuecomment-388277405
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        use MacroParsingBehavior::*;

        match name {
            "FP_INFINITE" => Ignore,
            "FP_NAN" => Ignore,
            "FP_NORMAL" => Ignore,
            "FP_SUBNORMAL" => Ignore,
            "FP_ZERO" => Ignore,
            _ => Default,
        }
    }
}

fn version() -> String {
    let major: u8 = env::var("CARGO_PKG_VERSION_MAJOR")
        .unwrap()
        .parse()
        .unwrap();
    let minor: u8 = env::var("CARGO_PKG_VERSION_MINOR")
        .unwrap()
        .parse()
        .unwrap();

    format!("{major}.{minor}")
}

fn output() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

fn source() -> PathBuf {
    output().join(format!("ffmpeg-{}", version()))
}

fn search() -> PathBuf {
    let mut absolute = env::current_dir().unwrap();
    absolute.push(output());
    absolute.push("dist");

    absolute
}

fn fetch() -> io::Result<()> {
    let output_base_path = output();
    let clone_dest_dir = format!("ffmpeg-{}", version());
    let _ = std::fs::remove_dir_all(output_base_path.join(&clone_dest_dir));
    let status = Command::new("git")
        .current_dir(&output_base_path)
        .args(if cfg!(target_os = "windows") {
            vec!["-c", "core.autocrlf=false"]
        } else {
            vec![]
        })
        .arg("clone")
        .arg("--depth=1")
        .arg("-b")
        .arg(format!("release/{}", version()))
        .arg("https://github.com/FFmpeg/FFmpeg")
        .arg(&clone_dest_dir)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("fetch failed"))
    }
}

fn switch(configure: &mut Command, feature: &str, name: &str) {
    let arg = if env::var("CARGO_FEATURE_".to_string() + feature).is_ok() {
        "--enable-"
    } else {
        "--disable-"
    };
    configure.arg(arg.to_string() + name);
}

fn is_apple_simulator() -> bool {
    env::var("CARGO_CFG_TARGET_VENDOR").unwrap_or_default() == "apple"
        && (env::var("CARGO_CFG_TARGET_ABI").unwrap_or_default() == "sim"
            || env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "x86_64")
}

fn apple_sdk_name(target_os: &str, is_sim: bool) -> Option<&'static str> {
    match (target_os, is_sim) {
        ("macos", _) => Some("macosx"),
        ("ios", true) => Some("iphonesimulator"),
        ("ios", false) => Some("iphoneos"),
        ("tvos", true) => Some("appletvsimulator"),
        ("tvos", false) => Some("appletvos"),
        _ => None,
    }
}

/// Returns the `-m*-version-min=` cflags for cross-compiling to Apple platforms.
fn apple_version_min_cflag(target_os: &str, is_sim: bool) -> Option<&'static str> {
    match (target_os, is_sim) {
        ("ios", true) => Some("-mios-simulator-version-min=11.0"),
        ("ios", false) => Some("-mios-version-min=11.0"),
        ("macos", _) => Some("-mmacosx-version-min=10.11"),
        ("tvos", true) => Some("-mappletvsimulator-version-min=13.0"),
        ("tvos", false) => Some("-mappletvos-version-min=13.0"),
        _ => None,
    }
}

fn get_ffmpeg_target_os() -> String {
    let cargo_target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    match cargo_target_os.as_str() {
        "ios" | "tvos" => "darwin".to_string(),
        _ => cargo_target_os,
    }
}

/// Find the sysroot required for a cross compilation by ffmpeg **and** bindgen
/// @see https://github.com/rust-lang/rust-bindgen/issues/1229
fn find_sysroot() -> Option<String> {
    if env::var("CARGO_FEATURE_BUILD").is_err() || env::var("HOST") == env::var("TARGET") {
        return None;
    }

    if let Ok(sysroot) = env::var("SYSROOT") {
        return Some(sysroot.to_string());
    }

    if matches!(
        env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("ios") | Ok("tvos")
    ) {
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
        let sdk = apple_sdk_name(&target_os, is_apple_simulator()).unwrap();
        let xcode_output = Command::new("xcrun")
            .args(["--sdk", sdk, "--show-sdk-path"])
            .output()
            .expect("failed to run xcrun");

        if !xcode_output.status.success() {
            panic!(
                "Failed to run xcrun to get the {} sysroot, please install xcode tools or provide sysroot using $SYSROOT env. Error: {}",
                sdk,
                String::from_utf8_lossy(&xcode_output.stderr)
            );
        }

        let string = String::from_utf8(xcode_output.stdout)
            .expect("Failed to parse xcrun output")
            .replace("\n", "");

        if !Path::new(&string).exists() {
            panic!("xcrun returned invalid sysroot path: {}", string);
        }

        return Some(string);
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        let sysroot_path = env::var("CARGO_NDK_SYSROOT_PATH").expect("Missing android sysroot path. For android cross compilation please use cargo-ndk which exposes all the required NDK paths throught env variables.");

        if !Path::new(&sysroot_path).exists() {
            panic!("Android sysroot path does not exists: {}", sysroot_path);
        }

        return Some(sysroot_path);
    }

    println!("cargo:warning=Detected cross compilation but sysroot not provided");
    None
}

/// Validate a CPU flag value (e.g. from FFMPEG_MARCH/FFMPEG_MTUNE) before passing
/// it to the compiler, to prevent arbitrary string injection.
fn validated_cpu_flag(var: &str, value: String) -> String {
    if value.is_empty() {
        return value; // omit the flag
    }
    let valid = !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '+' | '_'));
    if !valid {
        panic!(
            "Invalid {} value {:?}: only ASCII letters, digits and '-._+' are allowed \
             (e.g. native, x86-64-v3, armv8.2-a+crc)",
            var, value
        );
    }
    value
}

fn build(sysroot: Option<&str>) -> io::Result<()> {
    let source_dir = source();
    if cfg!(target_os = "windows") {
        let path = env::var("PATH").unwrap_or_default();
        let mut paths = env::split_paths(&path).collect::<Vec<_>>();
        paths.push(source_dir.clone());
        let new_path = env::join_paths(paths).unwrap();

        let include = env::var("INCLUDE").unwrap_or_default();
        let mut includes = env::split_paths(&include).collect::<Vec<_>>();
        includes.push(source_dir.clone());
        let new_include = env::join_paths(includes).unwrap();

        unsafe {
            env::set_var("PATH", &new_path);
            env::set_var("INCLUDE", &new_include);
        }
    }

    // Command's path is not relative to command's current_dir
    let configure_path = source_dir.join("configure");
    assert!(configure_path.exists());
    let mut configure = if cfg!(target_os = "windows") {
        if Command::new("sh")
            .arg("-c")
            .arg("echo ok")
            .output()
            .is_err()
        {
            return Err(io::Error::other(
                "Failed to find 'sh.exe', which is required for building FFmpeg",
            ));
        }

        let mut configure = Command::new("sh");
        configure.arg(configure_path);
        if cfg!(target_env = "msvc") {
            configure.arg("--toolchain=msvc");
        }

        configure
    } else {
        Command::new(&configure_path)
    };

    configure.current_dir(&source_dir);
    if env::var("CARGO_FEATURE_BUILD_VULKAN").is_ok()
        || env::var("CARGO_FEATURE_BUILD_NVDEC").is_ok()
    {
        let bundled_include = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("bundled-headers")
            .join("include");
        let inherited = env::var("CFLAGS").unwrap_or_default();
        let bundled_pkgconfig = bundled_include
            .parent()
            .unwrap()
            .join("lib")
            .join("pkgconfig");
        let inherited_pkgconfig = env::var("PKG_CONFIG_PATH").unwrap_or_default();
        configure.env(
            "CFLAGS",
            format!("-I{} {inherited}", bundled_include.to_string_lossy()),
        );
        configure.env(
            "PKG_CONFIG_PATH",
            format!(
                "{}:{inherited_pkgconfig}",
                bundled_pkgconfig.to_string_lossy()
            ),
        );
        println!(
            "cargo:rerun-if-changed={}",
            bundled_include.to_string_lossy()
        );
    }
    configure.arg(format!("--prefix={}", search().to_string_lossy()));

    let target = env::var("TARGET").unwrap();
    let host = env::var("HOST").unwrap();
    if target != host {
        configure.arg("--enable-cross-compile");

        // Rust targets are subtly different than naming scheme for compiler prefixes.
        // The cc crate has the messy logic of guessing a working prefix,
        // and this is a messy way of reusing that logic.
        let cc = cc::Build::new();

        // Apple-clang needs this, -arch is not enough.
        let target_flag = format!("--target={target}");
        if cc.is_flag_supported(&target_flag).unwrap_or(false) {
            configure.arg(format!("--extra-cflags={target_flag}"));
            configure.arg(format!("--extra-ldflags={target_flag}"));
        }

        configure.arg(format!(
            "--arch={}",
            env::var("CARGO_CFG_TARGET_ARCH").unwrap()
        ));
        configure.arg(format!("--target-os={}", get_ffmpeg_target_os()));

        // cross-prefix won't work for android because they use different compiler for every
        // platform version, so we provide direct compiler paths manually instead
        if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
            let compiler = cc.get_compiler();
            if let Some(compiler_name) = compiler.path().file_stem().and_then(|s| s.to_str()) {
                if let Some(suffix_pos) = compiler_name.rfind('-') {
                    let prefix = compiler_name[0..suffix_pos].trim_end_matches("-wr"); // "wr-c++" compiler

                    configure.arg(format!("--cross-prefix={prefix}-"));
                }
            } else {
                eprintln!(
                    "warning: Could not determine cross-compiler prefix from path: {:?}",
                    compiler.path()
                );
            }
        }
    } else {
        // Determine -march/-mtune flags for the compiler.
        // Priority: env vars > build-portable feature > default (native)
        println!("cargo:rerun-if-env-changed=FFMPEG_MARCH");
        println!("cargo:rerun-if-env-changed=FFMPEG_MTUNE");

        let march_env = env::var("FFMPEG_MARCH").ok();
        let mtune_env = env::var("FFMPEG_MTUNE").ok();

        let (march, mtune) = if march_env.is_some() || mtune_env.is_some() {
            // Env vars take highest priority. Empty string means omit the flag.
            // Validate the values to prevent arbitrary string injection.
            (
                validated_cpu_flag(
                    "FFMPEG_MARCH",
                    march_env.unwrap_or_else(|| "native".to_string()),
                ),
                validated_cpu_flag(
                    "FFMPEG_MTUNE",
                    mtune_env.unwrap_or_else(|| "native".to_string()),
                ),
            )
        } else if cfg!(feature = "build-portable") {
            // Omit both flags so the compiler uses its baseline target.
            (String::new(), String::new())
        } else {
            // Default: tune for the host architecture.
            ("native".to_string(), "native".to_string())
        };

        let mut parts = Vec::new();
        if !march.is_empty() {
            parts.push(format!("-march={march}"));
        }
        if !mtune.is_empty() {
            parts.push(format!("-mtune={mtune}"));
        }
        if !parts.is_empty() {
            configure.arg(format!("--extra-cflags={}", parts.join(" ")));
        }
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // essential librareis on windowsw
        println!("cargo:rustc-link-lib=dylib=ole32");
        println!("cargo:rustc-link-lib=dylib=oleaut32");
        println!("cargo:rustc-link-lib=dylib=gdi32");
        println!("cargo:rustc-link-lib=dylib=user32");
        println!("cargo:rustc-link-lib=dylib=vfw32");
        println!("cargo:rustc-link-lib=dylib=strmiids");
        println!("cargo:rustc-link-lib=dylib=bcrypt");
        println!("cargo:rustc-link-lib=dylib=shlwapi");
        println!("cargo:rustc-link-lib=dylib=shell32");
    }

    // for ios/tvos it is required to provide sysroot for both configure and bindgen
    // for macos the easiest way is to run xcrun, for other platform we support $SYSROOT var
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let is_sim = is_apple_simulator();

    if matches!(target_os.as_str(), "ios" | "tvos") {
        let sdk = apple_sdk_name(&target_os, is_sim).unwrap();
        let sysroot = sysroot.unwrap_or_else(|| panic!("The sysroot is required for {} cross compilation, make sure to have available xcode or provide the $SYSROOT env var", target_os));
        configure.arg(format!("--sysroot={sysroot}"));

        let cc = Command::new("xcrun")
            .args(["--sdk", sdk, "-f", "clang"])
            .output()
            .expect("failed to run xcrun")
            .stdout;

        configure.arg(format!(
            "--cc={}",
            str::from_utf8(&cc)
                .expect("Failed to parse xcrun output")
                .trim()
        ));
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // cargo ndk auto populates rust env variables for android cross compilation
        // so we can just leverage the same compiler path and cflags for ffmpeg build
        let android_cc_raw_path = env::var(format!("CC_{target}")).expect("Missing CC path for android. Make sure to use cargo-ndk for adnrdoic cross compilation");
        let android_cc_path = Path::new(&android_cc_raw_path);
        if !android_cc_path.exists() {
            panic!("Android CC path does not exists: {}", android_cc_raw_path);
        }
        configure.arg(format!("--cc={android_cc_raw_path}"));

        for tool in ["nm", "strip"] {
            configure.arg(format!(
                "--{tool}={}",
                android_cc_path
                    .join("..")
                    .join(format!("llvm-{tool}"))
                    .canonicalize()
                    .unwrap_or_else(|_| panic!("failed to resolve a path to android {}", tool))
                    .display()
            ));
        }

        if let Ok(android_target_flags) = env::var(format!("CFLAGS_{target}")).as_deref() {
            configure.arg(format!("--extra-cflags={android_target_flags}"));
            configure.arg(format!("--extra-ldflags={android_target_flags}"));
        }

        if matches!(
            env::var("CARGO_CFG_TARGET_ARCH").as_deref(),
            Ok("x86_64") | Ok("x86")
        ) {
            // x86 asm contains position dependent code (relocations)
            configure.arg("--disable-asm");
        }

        // https://github.com/Javernaut/ffmpeg-android-maker/blob/master/scripts/ffmpeg/build.sh#L30
        // configure.arg(--extra-ldflags=-WL,-z,max-page-size=16384");
        // required for android
        configure.arg("--extra-cflags=-fPIC");
    }

    // control debug build
    if env::var("DEBUG").is_ok() {
        configure.arg("--enable-debug");
        configure.arg("--disable-stripping");
    } else {
        configure.arg("--disable-debug");
        configure.arg("--enable-stripping");
        configure.arg("--extra-cflags=-03 -ffast-math -funroll-loops");
        #[cfg(not(target_os = "windows"))]
        configure.arg("--extra-ldflags=-flto");
    }

    // make it static
    configure.arg("--enable-static");
    configure.arg("--disable-shared");
    // windows includes threading in the standard library
    #[cfg(not(target_env = "msvc"))]
    {
        configure.arg("--enable-pthreads");
    }

    // position independent code
    configure.arg("--enable-pic");

    // stop autodetected libraries enabling themselves, causing linking errors
    configure.arg("--disable-autodetect");

    // do not build programs since we don't need them
    configure.arg("--disable-programs");

    // do not generate documentation
    configure.arg("--disable-doc");

    macro_rules! enable {
        ($conf:expr, $feat:expr, $name:expr) => {
            if env::var(concat!("CARGO_FEATURE_", $feat)).is_ok() {
                $conf.arg(concat!("--enable-", $name));
            }
        };
    }

    // macro_rules! disable {
    //     ($conf:expr, $feat:expr, $name:expr) => (
    //         if env::var(concat!("CARGO_FEATURE_", $feat)).is_err() {
    //             $conf.arg(concat!("--disable-", $name));
    //         }
    //     )
    // }

    // the binary using ffmpeg-sys must comply with GPL
    switch(&mut configure, "BUILD_LICENSE_GPL", "gpl");

    // the binary using ffmpeg-sys must comply with (L)GPLv3
    switch(&mut configure, "BUILD_LICENSE_VERSION3", "version3");

    // the binary using ffmpeg-sys cannot be redistributed
    switch(&mut configure, "BUILD_LICENSE_NONFREE", "nonfree");

    let ffmpeg_major_version: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();

    // configure building libraries based on features
    for lib in LIBRARIES
        .iter()
        .filter(|lib| lib.is_feature)
        .filter(|lib| !(lib.name == "avresample" && ffmpeg_major_version >= 5))
        .filter(|lib| !(lib.name == "postproc" && ffmpeg_major_version >= 8))
    {
        switch(&mut configure, &lib.name.to_uppercase(), lib.name);
    }

    // configure external SSL libraries
    enable!(configure, "BUILD_LIB_GNUTLS", "gnutls");
    enable!(configure, "BUILD_LIB_OPENSSL", "openssl");

    // configure external filters
    enable!(configure, "BUILD_LIB_FONTCONFIG", "fontconfig");
    enable!(configure, "BUILD_LIB_FREI0R", "frei0r");
    enable!(configure, "BUILD_LIB_LADSPA", "ladspa");
    enable!(configure, "BUILD_LIB_ASS", "libass");
    enable!(configure, "BUILD_LIB_FREETYPE", "libfreetype");
    enable!(configure, "BUILD_LIB_FRIBIDI", "libfribidi");
    enable!(configure, "BUILD_LIB_OPENCV", "libopencv");
    enable!(configure, "BUILD_LIB_VMAF", "libvmaf");

    // configure external encoders/decoders
    enable!(configure, "BUILD_LIB_AACPLUS", "libaacplus");
    enable!(configure, "BUILD_LIB_CELT", "libcelt");
    enable!(configure, "BUILD_LIB_DCADEC", "libdcadec");
    enable!(configure, "BUILD_LIB_DAV1D", "libdav1d");
    enable!(configure, "BUILD_LIB_FAAC", "libfaac");
    enable!(configure, "BUILD_LIB_FDK_AAC", "libfdk-aac");
    enable!(configure, "BUILD_LIB_GSM", "libgsm");
    enable!(configure, "BUILD_LIB_ILBC", "libilbc");
    enable!(configure, "BUILD_LIB_VAZAAR", "libvazaar");
    enable!(configure, "BUILD_LIB_MP3LAME", "libmp3lame");
    enable!(configure, "BUILD_LIB_OPENCORE_AMRNB", "libopencore-amrnb");
    enable!(configure, "BUILD_LIB_OPENCORE_AMRWB", "libopencore-amrwb");
    enable!(configure, "BUILD_LIB_OPENH264", "libopenh264");
    enable!(configure, "BUILD_LIB_OPENH265", "libopenh265");
    enable!(configure, "BUILD_LIB_OPENJPEG", "libopenjpeg");
    enable!(configure, "BUILD_LIB_OPUS", "libopus");
    enable!(configure, "BUILD_LIB_SCHROEDINGER", "libschroedinger");
    enable!(configure, "BUILD_LIB_SHINE", "libshine");
    enable!(configure, "BUILD_LIB_SNAPPY", "libsnappy");
    enable!(configure, "BUILD_LIB_SPEEX", "libspeex");
    enable!(
        configure,
        "BUILD_LIB_STAGEFRIGHT_H264",
        "libstagefright-h264"
    );
    enable!(configure, "BUILD_LIB_THEORA", "libtheora");
    enable!(configure, "BUILD_LIB_TWOLAME", "libtwolame");
    enable!(configure, "BUILD_LIB_UTVIDEO", "libutvideo");
    enable!(configure, "BUILD_LIB_VO_AACENC", "libvo-aacenc");
    enable!(configure, "BUILD_LIB_VO_AMRWBENC", "libvo-amrwbenc");
    enable!(configure, "BUILD_LIB_VORBIS", "libvorbis");
    enable!(configure, "BUILD_LIB_VPX", "libvpx");
    enable!(configure, "BUILD_LIB_WAVPACK", "libwavpack");
    enable!(configure, "BUILD_LIB_WEBP", "libwebp");
    enable!(configure, "BUILD_LIB_X264", "libx264");
    enable!(configure, "BUILD_LIB_X265", "libx265");
    enable!(configure, "BUILD_LIB_XAVS", "libxavs");
    enable!(configure, "BUILD_LIB_XVID", "libxvid");

    // make sure to only enable related hw acceleration features for a correct
    // target os. This allows to leave allows cargo features enable and control
    // ffmpeg compilation using target only

    // Apple VideoToolbox (iOS, macOS, and tvOS)
    if env::var("CARGO_FEATURE_BUILD_VIDEOTOOLBOX").is_ok()
        && matches!(target_os.as_str(), "ios" | "macos" | "tvos")
    {
        configure.arg("--enable-videotoolbox");

        if target != host
            && let Some(flag) = apple_version_min_cflag(&target_os, is_sim)
        {
            configure.arg(format!("--extra-cflags={flag}"));
        }
    }

    // Apple audio hw acceleration API (iOS, macOS, and tvOS)
    if env::var("CARGO_FEATURE_BUILD_AUDIOTOOLBOX").is_ok()
        && matches!(target_os.as_str(), "ios" | "macos" | "tvos")
    {
        configure.arg("--enable-audiotoolbox");

        if target != host
            && let Some(flag) = apple_version_min_cflag(&target_os, is_sim)
        {
            configure.arg(format!("--extra-cflags={flag}"));
        }
    }

    // Linux video acceleration API (VAAPI)
    if env::var("CARGO_FEATURE_BUILD_VAAPI").is_ok() && matches!(target_os.as_str(), "linux") {
        configure.arg("--enable-vaapi");
    }

    if env::var("CARGO_FEATURE_BUILD_LIB_D3D11VA").is_ok()
        && matches!(target_os.as_str(), "windows")
    {
        configure.arg("--enable-d3d11va");
    }

    // DirectX Video Acceleration 2 (Windows only)
    if env::var("CARGO_FEATURE_BUILD_LIB_DXVA2").is_ok() && matches!(target_os.as_str(), "windows")
    {
        configure.arg("--enable-dxva2");
    }

    // NVIDIA NVDEC acceleration without a CUDA toolkit dependency. FFmpeg
    // loads CUDA and nvcuvid from the installed driver at runtime.
    if env::var("CARGO_FEATURE_BUILD_NVDEC").is_ok()
        && matches!(target_os.as_str(), "linux" | "windows")
    {
        configure.arg("--enable-ffnvcodec");
        configure.arg("--enable-cuda");
        configure.arg("--enable-cuvid");
        configure.arg("--enable-nvdec");
    }

    // NVIDIA NVENC/NVDEC acceleration (Linux and Windows)
    if env::var("CARGO_FEATURE_BUILD_NVIDIA").is_ok()
        && matches!(target_os.as_str(), "linux" | "windows")
    {
        configure.arg("--enable-libnpp");
        configure.arg("--enable-cuda-nvcc");
        configure.arg("--enable-cuvid");
        configure.arg("--enable-nvenc");
        configure.arg("--enable-cuda-llvm");

        let cuda_path = env::var("CUDA_PATH").unwrap_or(if target_os == "linux" {
            "/usr/local/cuda".to_string()
        } else {
            "C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA".to_string()
        });
        println!("cargo:rustc-link-arg=-L{cuda_path}/lib64");
        println!("cargo:rustc-link-arg=-I{cuda_path}/include");

        // Additional configuration may be needed for CUDA toolkit path
        // This could be provided as an environment variable
        if let Ok(cuda_path) = env::var("CUDA_PATH") {
            configure.arg(format!("--cuda-path={cuda_path}"));
        }
    }

    // Intel Quick Sync Video (Linux and Windows)
    if env::var("CARGO_FEATURE_BUILD_LIB_LIBMFX").is_ok()
        && matches!(target_os.as_str(), "linux" | "windows")
    {
        configure.arg("--enable-libmfx");
    }

    // Android MediaCodec (Android only)
    if env::var("CARGO_FEATURE_BUILD_MEDIACODEC").is_ok() && target_os == "android" {
        configure.arg("--enable-mediacodec");
        configure.arg("--enable-jni");

        // Add common MediaCodec decoders
        configure.arg("--enable-decoder=h264_mediacodec");
        configure.arg("--enable-decoder=hevc_mediacodec");
        configure.arg("--enable-decoder=vp8_mediacodec");
        configure.arg("--enable-decoder=vp9_mediacodec");
        configure.arg("--enable-decoder=av1_mediacodec");
    }

    // AMD Advanced Media Framework (Linux and Windows)
    if env::var("CARGO_FEATURE_BUILD_AMF").is_ok()
        && matches!(target_os.as_str(), "linux" | "windows")
        && matches!(
            env::var("CARGO_FEATURE_TARGET_ARCH").as_deref(),
            Ok("x86_64")
        )
    {
        configure.arg("--enable-amf");
    }

    enable!(configure, "BUILD_VULKAN", "vulkan");

    // other external libraries
    enable!(configure, "BUILD_LIB_DRM", "libdrm");
    enable!(configure, "BUILD_NVENC", "nvenc");

    // configure external protocols
    enable!(configure, "BUILD_LIB_SMBCLIENT", "libsmbclient");
    enable!(configure, "BUILD_LIB_SSH", "libssh");

    // configure misc build options
    enable!(configure, "BUILD_PIC", "pic");

    // skip all the warnings from output as they can significantly slow down the build
    // time on platforms like mac which spawns thousands of nullabilty complieance warnings
    configure.arg("--extra-cflags=-w");

    // run ./configure
    let output = configure
        .output()
        .unwrap_or_else(|_| panic!("{:?} failed", configure));
    if !output.status.success() {
        println!(
            "configure stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        println!(
            "configure stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        return Err(io::Error::other(format!(
            "configure failed {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // run make
    if !Command::new("make")
        .arg("-j")
        .arg(num_cpus::get().to_string())
        .current_dir(source())
        .status()?
        .success()
    {
        return Err(io::Error::other("make failed"));
    }

    // run make install
    if !Command::new("make")
        .current_dir(source())
        .arg("install")
        .status()?
        .success()
    {
        return Err(io::Error::other("make install failed"));
    }

    Ok(())
}

#[cfg(not(target_env = "msvc"))]
fn try_vcpkg(_statik: bool) -> Option<Vec<PathBuf>> {
    None
}

#[cfg(target_env = "msvc")]
fn try_vcpkg(statik: bool) -> Option<Vec<PathBuf>> {
    if !statik {
        unsafe {
            env::set_var("VCPKGRS_DYNAMIC", "1");
        }
    }

    vcpkg::find_package("ffmpeg")
        .map_err(|e| {
            println!("Could not find ffmpeg with vcpkg: {}", e);
        })
        .map(|library| library.include_paths)
        .ok()
}

fn check_features(
    include_paths: Vec<PathBuf>,
    infos: &[(&'static str, Option<&'static str>, &'static str)],
) {
    let mut includes_code = String::new();
    let mut main_code = String::new();

    for &(header, feature, var) in infos {
        if let Some(feature) = feature
            && env::var(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_err()
        {
            continue;
        }

        let include = format!("#include <{header}>");
        if !includes_code.contains(&include) {
            includes_code.push_str(&include);
            includes_code.push('\n');
        }
        let _ = write!(
            includes_code,
            r#"
            #ifndef {var}_is_defined
            #ifndef {var}
            #define {var} 0
            #define {var}_is_defined 0
            #else
            #define {var}_is_defined 1
            #endif
            #endif
        "#
        );

        let _ = write!(
            main_code,
            r#"printf("[{var}]%d%d\n", {var}, {var}_is_defined);
            "#
        );
    }

    let version_check_info = [("avcodec", 56, 64, 0, 108)];
    for &(lib, begin_version_major, end_version_major, begin_version_minor, end_version_minor) in
        version_check_info.iter()
    {
        for version_major in begin_version_major..end_version_major {
            for version_minor in begin_version_minor..end_version_minor {
                let _ = write!(
                    main_code,
                    r#"printf("[{lib}_version_greater_than_{version_major}_{version_minor}]%d\n", LIB{lib_uppercase}_VERSION_MAJOR > {version_major} || (LIB{lib_uppercase}_VERSION_MAJOR == {version_major} && LIB{lib_uppercase}_VERSION_MINOR > {version_minor}));
                    "#,
                    lib = lib,
                    lib_uppercase = lib.to_uppercase(),
                    version_major = version_major,
                    version_minor = version_minor
                );
            }
        }
    }

    let out_dir = output();

    write!(
        File::create(out_dir.join("check.c")).expect("Failed to create file"),
        r#"
            #include <stdio.h>
            {includes_code}

            int main()
            {{
                {main_code}
                return 0;
            }}
           "#
    )
    .expect("Write failed");

    let executable = out_dir.join(if cfg!(windows) { "check.exe" } else { "check" });
    let mut compiler = cc::Build::new()
        .target(&env::var("HOST").unwrap()) // don't cross-compile this
        .get_compiler()
        .to_command();

    for dir in include_paths {
        compiler.arg("-I");
        compiler.arg(dir.to_string_lossy().into_owned());
    }
    if !compiler
        .current_dir(&out_dir)
        .arg("-o")
        .arg(&executable)
        .arg("check.c")
        .status()
        .expect("Command failed")
        .success()
    {
        panic!("Compile failed");
    }

    let check_output = Command::new(out_dir.join(&executable))
        .current_dir(&out_dir)
        .output()
        .expect("Check failed");
    if !check_output.status.success() {
        panic!(
            "{} failed: {}\n{}",
            executable.display(),
            String::from_utf8_lossy(&check_output.stdout),
            String::from_utf8_lossy(&check_output.stderr)
        );
    }

    let stdout = str::from_utf8(&check_output.stdout).unwrap();

    println!("stdout of {}={}", executable.display(), stdout);

    for &(_, feature, var) in infos {
        if let Some(feature) = feature
            && env::var(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_err()
        {
            continue;
        }
        // Here so the features are listed for rust-ffmpeg at build time. Does
        // NOT represent activated features, just features that exist (hence the
        // lack of "=true" at the end)
        println!(r#"cargo:{var}="#);

        let var_str = format!("[{var}]");
        let pos = var_str.len()
            + stdout
                .find(&var_str)
                .unwrap_or_else(|| panic!("Variable '{}' not found in stdout output", var_str));
        if &stdout[pos..pos + 1] == "1" {
            println!(r#"cargo:rustc-cfg=feature="{}""#, var.to_lowercase());
            println!(r#"cargo:{}=true"#, var.to_lowercase());
        }

        // Also find out if defined or not (useful for cases where only the definition of a macro
        // can be used as distinction)
        if &stdout[pos + 1..pos + 2] == "1" {
            println!(
                r#"cargo:rustc-cfg=feature="{}_is_defined""#,
                var.to_lowercase()
            );
            println!(r#"cargo:{}_is_defined=true"#, var.to_lowercase());
        }
    }

    for &(lib, begin_version_major, end_version_major, begin_version_minor, end_version_minor) in
        version_check_info.iter()
    {
        for version_major in begin_version_major..end_version_major {
            for version_minor in begin_version_minor..end_version_minor {
                let search_str =
                    format!("[{lib}_version_greater_than_{version_major}_{version_minor}]");
                let pos = stdout
                    .find(&search_str)
                    .expect("Variable not found in output")
                    + search_str.len();

                if &stdout[pos..pos + 1] == "1" {
                    println!(
                        r#"cargo:rustc-cfg=feature="{}""#,
                        &search_str[1..(search_str.len() - 1)]
                    );
                    println!(r#"cargo:{}=true"#, &search_str[1..(search_str.len() - 1)]);
                }
            }
        }
    }

    let ffmpeg_lavc_versions = [
        ("ffmpeg_3_0", 57, 24),
        ("ffmpeg_3_1", 57, 48),
        ("ffmpeg_3_2", 57, 64),
        ("ffmpeg_3_3", 57, 89),
        ("ffmpeg_3_4", 57, 107),
        ("ffmpeg_4_0", 58, 18),
        ("ffmpeg_4_1", 58, 35),
        ("ffmpeg_4_2", 58, 54),
        ("ffmpeg_4_3", 58, 91),
        ("ffmpeg_4_4", 58, 100),
        ("ffmpeg_5_0", 59, 18),
        ("ffmpeg_5_1", 59, 37),
        ("ffmpeg_6_0", 60, 3),
        ("ffmpeg_6_1", 60, 31),
        ("ffmpeg_7_0", 61, 3),
        ("ffmpeg_7_1", 61, 19),
        ("ffmpeg_8_0", 62, 8),
        ("ffmpeg_8_1", 62, 28),
        ("ffmpeg_9_0", 63, 1),
    ];
    for &(ffmpeg_version_flag, lavc_version_major, lavc_version_minor) in
        ffmpeg_lavc_versions.iter()
    {
        let search_str = format!(
            "[avcodec_version_greater_than_{lavc_version_major}_{lavc_version_minor}]",
            lavc_version_major = lavc_version_major,
            lavc_version_minor = lavc_version_minor - 1
        );
        let pos = stdout
            .find(&search_str)
            .expect("Variable not found in output")
            + search_str.len();
        println!(
            r#"cargo:rustc-check-cfg=cfg(feature, values("{}"))"#,
            ffmpeg_version_flag
        );
        if &stdout[pos..pos + 1] == "1" {
            println!(r#"cargo:rustc-cfg=feature="{ffmpeg_version_flag}""#);
            println!(r#"cargo:{ffmpeg_version_flag}=true"#);
        } else {
            println!(r#"cargo:{ffmpeg_version_flag}="#);
        }
    }
}

fn search_include(include_paths: &[PathBuf], header: &str) -> String {
    for dir in include_paths {
        let include = dir.join(header);
        if fs::metadata(&include).is_ok() {
            if let Some(path_str) = include.as_path().to_str() {
                return path_str.to_string();
            }
            // Fall back to lossy conversion if path contains non-UTF8 characters
            return include.to_string_lossy().to_string();
        }
    }
    format!("/usr/include/{header}")
}

fn maybe_search_include(include_paths: &[PathBuf], header: &str) -> Option<String> {
    let path = search_include(include_paths, header);
    if fs::metadata(&path).is_ok() {
        Some(path)
    } else {
        None
    }
}

fn link_to_libraries(statik: bool, target_os: &str) {
    let ffmpeg_ty = if statik { "static" } else { "dylib" };
    for lib in LIBRARIES {
        let feat_is_enabled = lib.feature_name().and_then(|f| env::var(f).ok()).is_some();
        if !lib.is_feature || feat_is_enabled {
            println!("cargo:rustc-link-lib={}={}", ffmpeg_ty, lib.name);
        }
    }
    // Keep GNU ld from dropping libraries whose symbols it thinks are unused.
    // Apple's ld and MSVC's link.exe reject the option and don't drop them
    // anyway, so skip it there.
    if !matches!(
        target_os,
        "macos" | "ios" | "tvos" | "watchos" | "visionos" | "windows"
    ) {
        println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
    }
    if env::var("CARGO_FEATURE_BUILD_ZLIB").is_ok() && target_os == "linux" {
        println!("cargo:rustc-link-lib=z");
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");

    let statik = env::var("CARGO_FEATURE_STATIC").is_ok();
    let ffmpeg_major_version: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let sysroot = find_sysroot();
    let include_paths: Vec<PathBuf> = if env::var("CARGO_FEATURE_BUILD").is_ok() {
        println!(
            "cargo:rustc-link-search=native={}",
            search().join("lib").to_string_lossy()
        );
        link_to_libraries(statik, &target_os);
        if fs::metadata(search().join("lib").join("libavutil.a")).is_err() {
            fs::create_dir_all(output()).expect("failed to create build directory");
            fetch().unwrap();
            build(sysroot.as_deref()).unwrap();
        }

        // Check additional required libraries.
        {
            let config_mak = source().join("ffbuild/config.mak");
            let file = File::open(config_mak).unwrap();
            let reader = BufReader::new(file);
            let extra_linker_args = reader
                .lines()
                .filter_map(|line| {
                    let line = line.as_ref().ok()?;

                    if line.starts_with("EXTRALIBS") {
                        Some(
                            line.split('=')
                                .next_back()
                                .unwrap()
                                .split(' ')
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    }
                })
                .flatten()
                .collect::<Vec<_>>();

            extra_linker_args
                .iter()
                .filter(|flag| flag.starts_with("-l"))
                .map(|lib| &lib[2..])
                .for_each(|lib| println!("cargo:rustc-link-lib={lib}"));

            extra_linker_args
                .iter()
                .filter(|v| v.starts_with("-L"))
                .map(|flag| {
                    let path = &flag[2..];
                    if path.starts_with('/') {
                        PathBuf::from(path)
                    } else {
                        source().join(path)
                    }
                })
                .for_each(|lib_search_path| {
                    println!(
                        "cargo:rustc-link-search=native={}",
                        lib_search_path.to_string_lossy()
                    );
                })
        }

        vec![search().join("include")]
    }
    // Use prebuilt library
    else if let Ok(ffmpeg_dir) = env::var("FFMPEG_DIR") {
        let ffmpeg_dir = PathBuf::from(ffmpeg_dir);
        if ffmpeg_dir.join("lib/amd64").exists()
            && env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64")
        {
            println!(
                "cargo:rustc-link-search=native={}",
                ffmpeg_dir.join("lib/amd64").to_string_lossy()
            );
        } else if ffmpeg_dir.join("lib/armhf").exists()
            && env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("arm")
        {
            println!(
                "cargo:rustc-link-search=native={}",
                ffmpeg_dir.join("lib/armhf").to_string_lossy()
            );
        } else if ffmpeg_dir.join("lib/arm64").exists()
            && env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64")
        {
            println!(
                "cargo:rustc-link-search=native={}",
                ffmpeg_dir.join("lib/arm64").to_string_lossy()
            );
        } else {
            println!(
                "cargo:rustc-link-search=native={}",
                ffmpeg_dir.join("lib").to_string_lossy()
            );
        }
        link_to_libraries(statik, &target_os);
        vec![ffmpeg_dir.join("include")]
    } else if let Some(paths) = try_vcpkg(statik) {
        // vcpkg doesn't detect the "system" dependencies
        if statik {
            if cfg!(feature = "avcodec") || cfg!(feature = "avdevice") {
                println!("cargo:rustc-link-lib=ole32");
            }

            if cfg!(feature = "avformat") {
                println!("cargo:rustc-link-lib=secur32");
                println!("cargo:rustc-link-lib=ws2_32");
            }

            // avutil dependencies
            println!("cargo:rustc-link-lib=bcrypt");
            println!("cargo:rustc-link-lib=user32");
        }

        paths
    }
    // Fallback to pkg-config
    else {
        let avutil_result = pkg_config::Config::new().statik(statik).probe("libavutil");

        if let Err(e) = avutil_result {
            eprintln!("error: FFmpeg not found.");
            eprintln!();
            eprintln!("FFmpeg libraries could not be found using any of the following methods:");
            eprintln!("  1. FFMPEG_DIR environment variable (not set)");
            eprintln!("  2. vcpkg package manager (ffmpeg package not found)");
            eprintln!("  3. pkg-config (libavutil not found: {})", e);
            eprintln!();
            eprintln!("To fix this, please do one of the following:");
            eprintln!("  - Set FFMPEG_DIR to point to your FFmpeg installation directory");
            eprintln!("  - Install FFmpeg via vcpkg: vcpkg install ffmpeg");
            eprintln!("  - Install FFmpeg and ensure pkg-config can find it");
            eprintln!("  - Enable the 'build' feature to compile FFmpeg from source");
            std::process::exit(1);
        }

        let mut libs = vec![
            ("libavformat", "AVFORMAT"),
            ("libavfilter", "AVFILTER"),
            ("libavdevice", "AVDEVICE"),
            ("libswscale", "SWSCALE"),
            ("libswresample", "SWRESAMPLE"),
        ];
        if ffmpeg_major_version < 5 {
            libs.push(("libavresample", "AVRESAMPLE"));
        }

        for (lib_name, env_variable_name) in libs.iter() {
            if env::var(format!("CARGO_FEATURE_{env_variable_name}")).is_ok()
                && let Err(e) = pkg_config::Config::new().statik(statik).probe(lib_name)
            {
                eprintln!("error: FFmpeg library {} not found: {}", lib_name, e);
                eprintln!();
                eprintln!(
                    "The {} library is required but could not be found.",
                    lib_name
                );
                eprintln!("Please ensure FFmpeg is installed with all required components.");
                std::process::exit(1);
            }
        }

        match pkg_config::Config::new().statik(statik).probe("libavcodec") {
            Ok(lib) => lib.include_paths,
            Err(e) => {
                eprintln!("error: FFmpeg library libavcodec not found: {}", e);
                eprintln!();
                eprintln!("The libavcodec library is required but could not be found.");
                eprintln!("Please ensure FFmpeg is installed with all required components.");
                std::process::exit(1);
            }
        }
    };

    if statik
        && matches!(
            env::var("CARGO_CFG_TARGET_OS").as_deref(),
            Ok("macos") | Ok("ios") | Ok("tvos")
        )
    {
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

        // Frameworks available on all Apple platforms (macOS, iOS, tvOS)
        let frameworks = vec![
            "AudioToolbox",
            "AVFoundation",
            "CoreFoundation",
            "CoreGraphics",
            "CoreMedia",
            "CoreServices",
            "CoreVideo",
            "Foundation",
            "QuartzCore",
            "Security",
            "VideoToolbox",
        ];
        for f in &frameworks {
            println!("cargo:rustc-link-lib=framework={f}");
        }

        // Frameworks only available on macOS
        if target_os == "macos" {
            let macos_frameworks = vec![
                "AppKit",
                "OpenCL",
                "OpenGL",
                "QTKit",
                "VideoDecodeAcceleration",
            ];
            for f in &macos_frameworks {
                println!("cargo:rustc-link-lib=framework={f}");
            }
        }
    }

    check_features(
        include_paths.clone(),
        &[
            ("libavutil/avutil.h", None, "FF_API_OLD_AVOPTIONS"),
            ("libavutil/avutil.h", None, "FF_API_PIX_FMT"),
            ("libavutil/avutil.h", None, "FF_API_CONTEXT_SIZE"),
            ("libavutil/avutil.h", None, "FF_API_PIX_FMT_DESC"),
            ("libavutil/avutil.h", None, "FF_API_AV_REVERSE"),
            ("libavutil/avutil.h", None, "FF_API_AUDIOCONVERT"),
            ("libavutil/avutil.h", None, "FF_API_CPU_FLAG_MMX2"),
            ("libavutil/avutil.h", None, "FF_API_LLS_PRIVATE"),
            ("libavutil/avutil.h", None, "FF_API_AVFRAME_LAVC"),
            ("libavutil/avutil.h", None, "FF_API_VDPAU"),
            (
                "libavutil/avutil.h",
                None,
                "FF_API_GET_CHANNEL_LAYOUT_COMPAT",
            ),
            ("libavutil/avutil.h", None, "FF_API_XVMC"),
            ("libavutil/avutil.h", None, "FF_API_OPT_TYPE_METADATA"),
            ("libavutil/avutil.h", None, "FF_API_DLOG"),
            ("libavutil/avutil.h", None, "FF_API_HMAC"),
            ("libavutil/avutil.h", None, "FF_API_VAAPI"),
            ("libavutil/avutil.h", None, "FF_API_PKT_PTS"),
            ("libavutil/avutil.h", None, "FF_API_ERROR_FRAME"),
            ("libavutil/avutil.h", None, "FF_API_FRAME_QP"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_VIMA_DECODER",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_REQUEST_CHANNELS",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_DECODE_AUDIO",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_ENCODE_AUDIO",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_ENCODE_VIDEO",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_ID"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AUDIO_CONVERT",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AVCODEC_RESAMPLE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DEINTERLACE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DESTRUCT_PACKET",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_GET_BUFFER"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_MISSING_SAMPLE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_LOWRES"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CAP_VDPAU"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_BUFS_VDPAU"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VOXWARE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_SET_DIMENSIONS",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_DEBUG_MV"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AC_VLC"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_MSMPEG4",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_ASPECT_EXTENDED",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_THREAD_OPAQUE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_PKT"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_ALPHA"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ERROR_RATE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_QSCALE_TYPE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MB_TYPE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_MAX_BFRAMES",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_NEG_LINESIZES",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_EMU_EDGE"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_SH4"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_SPARC"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_UNUSED_MEMBERS",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_IDCT_XVIDMMX",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_INPUT_PRESERVED",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_NORMALIZE_AQP",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_GMC"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MV0"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_NAME"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AFD"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VISMV"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DV_FRAME_PROFILE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AUDIOENC_DELAY",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_VAAPI_CONTEXT",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AVCTX_TIMEBASE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MPV_OPT"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_STREAM_CODEC_TAG",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_QUANT_BIAS"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_RC_STRATEGY",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_CODED_FRAME",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MOTION_EST"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_WITHOUT_PREFIX",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_CONVERGENCE_DURATION",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_PRIVATE_OPT",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODER_TYPE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_RTP_CALLBACK",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_STAT_BITS"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VBV_DELAY"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_SIDEDATA_ONLY_PKT",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AVPICTURE"),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_BITEXACT",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_FRAC",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_URL_FEOF",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_PROBESIZE_32",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_AVCTX",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_OLD_OPEN_CALLBACKS",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_AVFILTERPAD_PUBLIC",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_FOO_COUNT",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_OPTS",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_OPTS_ERROR",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_AVFILTER_OPEN",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_REGISTER",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_GRAPH_PARSE",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_NOCONST_GET_NAME",
            ),
            (
                "libavresample/avresample.h",
                Some("avresample"),
                "FF_API_RESAMPLE_CLOSE_OPEN",
            ),
            (
                "libswscale/swscale.h",
                Some("swscale"),
                "FF_API_SWS_CPU_CAPS",
            ),
            ("libswscale/swscale.h", Some("swscale"), "FF_API_ARCH_BFIN"),
        ],
    );

    let clang_includes = include_paths
        .iter()
        .map(|include| format!("-I{}", include.to_string_lossy()));

    println!("cargo:rerun-if-changed=hwcontext_wrapper.h");
    println!("cargo:rerun-if-changed=hwcontext_stubs/vulkan/vulkan.h");
    println!("cargo:rerun-if-changed=hwcontext_stubs/VideoToolbox/VideoToolbox.h");
    println!("cargo:rerun-if-changed=hwcontext_stubs/va/va.h");
    println!("cargo:rerun-if-changed=hwcontext_stubs/mfxvideo.h");
    println!("cargo:rerun-if-changed=hwcontext_stubs/mfx/mfxvideo.h");

    // SDK shims (vulkan/, VideoToolbox/, va/, mfxvideo.h) used by hwcontext_wrapper.h.
    let hwcontext_stub_dir = format!(
        "-I{}/hwcontext_stubs",
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")
    );

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let mut builder = bindgen::Builder::default()
        // Shim dir first, so our stubs win over the real SDK headers on the path (by design, to avoid generating bindings for full SDKs).
        .clang_arg(&hwcontext_stub_dir)
        .clang_args(clang_includes)
        .ctypes_prefix("libc")
        // https://github.com/rust-lang/rust-bindgen/issues/550
        .blocklist_type("max_align_t")
        .blocklist_function("_.*")
        // Blocklist functions with u128 in signature.
        // https://github.com/zmwangx/rust-ffmpeg-sys/issues/1
        // https://github.com/rust-lang/rust-bindgen/issues/1549
        .blocklist_function("acoshl")
        .blocklist_function("acosl")
        .blocklist_function("asinhl")
        .blocklist_function("asinl")
        .blocklist_function("atan2l")
        .blocklist_function("atanhl")
        .blocklist_function("atanl")
        .blocklist_function("cbrtl")
        .blocklist_function("ceill")
        .blocklist_function("copysignl")
        .blocklist_function("coshl")
        .blocklist_function("cosl")
        .blocklist_function("dreml")
        .blocklist_function("ecvt_r")
        .blocklist_function("erfcl")
        .blocklist_function("erfl")
        .blocklist_function("exp2l")
        .blocklist_function("expl")
        .blocklist_function("expm1l")
        .blocklist_function("fabsl")
        .blocklist_function("fcvt_r")
        .blocklist_function("fdiml")
        .blocklist_function("finitel")
        .blocklist_function("floorl")
        .blocklist_function("fmal")
        .blocklist_function("fmaxl")
        .blocklist_function("fminl")
        .blocklist_function("fmodl")
        .blocklist_function("frexpl")
        .blocklist_function("gammal")
        .blocklist_function("hypotl")
        .blocklist_function("ilogbl")
        .blocklist_function("isinfl")
        .blocklist_function("isnanl")
        .blocklist_function("j0l")
        .blocklist_function("j1l")
        .blocklist_function("jnl")
        .blocklist_function("ldexpl")
        .blocklist_function("lgammal")
        .blocklist_function("lgammal_r")
        .blocklist_function("llrintl")
        .blocklist_function("llroundl")
        .blocklist_function("log10l")
        .blocklist_function("log1pl")
        .blocklist_function("log2l")
        .blocklist_function("logbl")
        .blocklist_function("logl")
        .blocklist_function("lrintl")
        .blocklist_function("lroundl")
        .blocklist_function("modfl")
        .blocklist_function("nanl")
        .blocklist_function("nearbyintl")
        .blocklist_function("nextafterl")
        .blocklist_function("nexttoward")
        .blocklist_function("nexttowardf")
        .blocklist_function("nexttowardl")
        .blocklist_function("powl")
        .blocklist_function("qecvt")
        .blocklist_function("qecvt_r")
        .blocklist_function("qfcvt")
        .blocklist_function("qfcvt_r")
        .blocklist_function("qgcvt")
        .blocklist_function("remainderl")
        .blocklist_function("remquol")
        .blocklist_function("rintl")
        .blocklist_function("roundl")
        .blocklist_function("scalbl")
        .blocklist_function("scalblnl")
        .blocklist_function("scalbnl")
        .blocklist_function("significandl")
        .blocklist_function("sinhl")
        .blocklist_function("sinl")
        .blocklist_function("sqrtl")
        .blocklist_function("strtold")
        .blocklist_function("tanhl")
        .blocklist_function("tanl")
        .blocklist_function("tgammal")
        .blocklist_function("truncl")
        .blocklist_function("y0l")
        .blocklist_function("y1l")
        .blocklist_function("ynl")
        .opaque_type("__mingw_ldbl_type_t")
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: env::var("CARGO_FEATURE_NON_EXHAUSTIVE_ENUMS").is_ok(),
        })
        .prepend_enum_name(false)
        .wrap_unsafe_ops(true)
        .derive_eq(true)
        .size_t_is_usize(true)
        .parse_callbacks(Box::new(Callbacks));

    if let Some(sysroot) = sysroot.as_deref() {
        builder = builder.clang_arg(format!("--sysroot={sysroot}"));
    }

    // The input headers we would like to generate
    // bindings for.
    if env::var("CARGO_FEATURE_AVCODEC").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavcodec/avcodec.h"))
            .header(search_include(&include_paths, "libavcodec/dv_profile.h"))
            .header(search_include(&include_paths, "libavcodec/vorbis_parser.h"));

        let bsf_path = search_include(&include_paths, "libavcodec/bsf.h");
        if std::path::Path::new(&bsf_path).exists() {
            builder = builder.header(bsf_path);
        }

        if ffmpeg_major_version < 5 {
            builder = builder.header(search_include(&include_paths, "libavcodec/vaapi.h"));
        }
        if ffmpeg_major_version < 8
            && let Some(avfft_path) = maybe_search_include(&include_paths, "libavcodec/avfft.h")
        {
            builder = builder.header(avfft_path);
        }
    }

    if env::var("CARGO_FEATURE_AVDEVICE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libavdevice/avdevice.h"));
    }

    if env::var("CARGO_FEATURE_AVFILTER").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavfilter/buffersink.h"))
            .header(search_include(&include_paths, "libavfilter/buffersrc.h"))
            .header(search_include(&include_paths, "libavfilter/avfilter.h"));
    }

    if env::var("CARGO_FEATURE_AVFORMAT").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavformat/avformat.h"))
            .header(search_include(&include_paths, "libavformat/avio.h"));
    }

    if env::var("CARGO_FEATURE_AVRESAMPLE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libavresample/avresample.h"));
    }

    builder = builder
        .header(search_include(&include_paths, "libavutil/adler32.h"))
        .header(search_include(&include_paths, "libavutil/aes.h"))
        .header(search_include(&include_paths, "libavutil/audio_fifo.h"))
        .header(search_include(&include_paths, "libavutil/base64.h"))
        .header(search_include(&include_paths, "libavutil/blowfish.h"))
        .header(search_include(&include_paths, "libavutil/bprint.h"))
        .header(search_include(&include_paths, "libavutil/buffer.h"))
        .header(search_include(&include_paths, "libavutil/camellia.h"))
        .header(search_include(&include_paths, "libavutil/cast5.h"))
        .header(search_include(&include_paths, "libavutil/channel_layout.h"))
        // Here until https://github.com/rust-lang/rust-bindgen/issues/2192 /
        // https://github.com/rust-lang/rust-bindgen/issues/258 is fixed.
        .header("channel_layout_fixed.h")
        .header(search_include(&include_paths, "libavutil/cpu.h"))
        .header(search_include(&include_paths, "libavutil/crc.h"))
        .header(search_include(&include_paths, "libavutil/dict.h"))
        .header(search_include(&include_paths, "libavutil/display.h"))
        .header(search_include(&include_paths, "libavutil/downmix_info.h"))
        .header(search_include(&include_paths, "libavutil/error.h"))
        .header(search_include(&include_paths, "libavutil/eval.h"))
        .header(search_include(&include_paths, "libavutil/fifo.h"))
        .header(search_include(&include_paths, "libavutil/file.h"))
        .header(search_include(&include_paths, "libavutil/frame.h"))
        .header(search_include(&include_paths, "libavutil/hash.h"))
        .header(search_include(&include_paths, "libavutil/hmac.h"))
        .header(search_include(&include_paths, "libavutil/hwcontext.h"))
        .header(search_include(&include_paths, "libavutil/imgutils.h"))
        .header(search_include(&include_paths, "libavutil/lfg.h"))
        .header(search_include(&include_paths, "libavutil/log.h"))
        .header(search_include(&include_paths, "libavutil/lzo.h"))
        .header(search_include(&include_paths, "libavutil/macros.h"))
        .header(search_include(&include_paths, "libavutil/mathematics.h"))
        .header(search_include(&include_paths, "libavutil/md5.h"))
        .header(search_include(&include_paths, "libavutil/mem.h"))
        .header(search_include(&include_paths, "libavutil/motion_vector.h"))
        .header(search_include(&include_paths, "libavutil/murmur3.h"))
        .header(search_include(&include_paths, "libavutil/opt.h"))
        .header(search_include(&include_paths, "libavutil/parseutils.h"))
        .header(search_include(&include_paths, "libavutil/pixdesc.h"))
        .header(search_include(&include_paths, "libavutil/pixfmt.h"))
        .header(search_include(&include_paths, "libavutil/random_seed.h"))
        .header(search_include(&include_paths, "libavutil/rational.h"))
        .header(search_include(&include_paths, "libavutil/replaygain.h"))
        .header(search_include(&include_paths, "libavutil/ripemd.h"))
        .header(search_include(&include_paths, "libavutil/samplefmt.h"))
        .header(search_include(&include_paths, "libavutil/sha.h"))
        .header(search_include(&include_paths, "libavutil/sha512.h"))
        .header(search_include(&include_paths, "libavutil/stereo3d.h"))
        .header(search_include(&include_paths, "libavutil/avstring.h"))
        .header(search_include(&include_paths, "libavutil/threadmessage.h"))
        .header(search_include(&include_paths, "libavutil/time.h"))
        .header(search_include(&include_paths, "libavutil/timecode.h"))
        .header(search_include(&include_paths, "libavutil/twofish.h"))
        .header(search_include(&include_paths, "libavutil/avutil.h"))
        .header(search_include(&include_paths, "libavutil/xtea.h"));

    let raw_color_params_path = search_include(&include_paths, "libavutil/raw_color_params.h");
    if std::path::Path::new(&raw_color_params_path).exists() {
        builder = builder.header(raw_color_params_path);
    }

    if env::var("CARGO_FEATURE_POSTPROC").is_ok() {
        let postproc_path = search_include(&include_paths, "libpostproc/postprocess.h");
        if std::path::Path::new(&postproc_path).exists() {
            builder = builder.header(postproc_path);
        }
    }

    if env::var("CARGO_FEATURE_SWRESAMPLE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libswresample/swresample.h"));
    }

    if env::var("CARGO_FEATURE_SWSCALE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libswscale/swscale.h"));
    }

    if let Some(hwcontext_drm_header) =
        maybe_search_include(&include_paths, "libavutil/hwcontext_drm.h")
    {
        builder = builder.header(hwcontext_drm_header);
    }

    // Per-API hwcontext structs (CUDA, D3D11/12, VideoToolbox, MediaCodec,
    // Vulkan, VAAPI, QSV) all bind through one wrapper.
    builder = builder.header("hwcontext_wrapper.h");

    // Finish the builder and generate the bindings.
    let bindings = builder
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(output().join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
