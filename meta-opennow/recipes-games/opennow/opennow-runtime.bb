SUMMARY = "Cross-compiled native runtime artifacts for the OpenNOW Qt client"

require opennow-source.inc
require opennow-crates.inc
require opennow-sdl.inc

inherit cmake cargo pkgconfig features_check nopackages

COMPATIBLE_HOST = "(x86_64|aarch64).*-linux$"
REQUIRED_DISTRO_FEATURES = "opengl vulkan wayland"
DEPENDS += "alsa-lib clang-native dbus ffmpeg libdrm libva nasm-native \
    opennow-license-report-native udev vulkan-headers vulkan-loader \
    wayland wayland-native wayland-protocols"

CARGO_SRC_DIR = "native/opennow-streamer"
BUILD_MODE = "--release"
BUILD_DIR = "release"
export CARGO_NET_OFFLINE = "true"
export CARGO_PROFILE_RELEASE_STRIP = "false"
export CMAKE_TOOLCHAIN_FILE = "${WORKDIR}/toolchain.cmake"
export LIBCLANG_PATH = "${STAGING_LIBDIR_NATIVE}"
export BINDGEN_EXTRA_CLANG_ARGS = "--sysroot=${RECIPE_SYSROOT} --target=${HOST_SYS} -I${STAGING_INCDIR}"
export OPENNOW_BUILD_VERSION = "${OPENNOW_VERSION}"

do_configure() {
    cargo_common_do_configure
}

do_configure[postfuncs] += "opennow_vendor_sdl opennow_check_sources"

python __anonymous() {
    functions = (d.getVarFlag("do_configure", "postfuncs") or "").split()
    d.setVarFlag("do_configure", "postfuncs", " ".join(
        function for function in functions if function != "cargo_common_do_patch_paths"))
}

python opennow_vendor_sdl() {
    import json
    import shutil
    import tomllib

    source = d.getVar("UNPACKDIR") + "/sdl2"
    vendor = d.getVar("CARGO_VENDORING_DIRECTORY")
    for relative in ("", "/sdl2-sys"):
        with open(source + relative + "/Cargo.toml", "rb") as manifest:
            package = tomllib.load(manifest)["package"]
        destination = vendor + "/" + package["name"] + "-" + package["version"]
        shutil.copytree(source + relative, destination, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns(".git"))
        with open(destination + "/.cargo-checksum.json", "w") as checksum:
            json.dump({"files": {}, "package": None}, checksum)
    with open(d.getVar("CARGO_HOME") + "/config.toml", "a") as config:
        config.write('\n[source.opennow-sdl]\n'
                     'git = "https://github.com/zortos293/rust-sdl2.git"\n'
                     'rev = "' + d.getVar("SRCREV_sdl2") + '"\n'
                     'replace-with = "bitbake"\n')
}

opennow_check_sources() {
    ${CARGO} metadata --frozen --format-version 1 \
        --manifest-path ${S}/native/opennow-core/Cargo.toml > /dev/null
    ${CARGO} metadata --frozen --format-version 1 \
        --manifest-path ${S}/native/opennow-streamer/Cargo.toml > /dev/null
}

do_compile() {
    oe_cargo_build --package opennow-streamer-ffi --lib --features linux-ffmpeg,linux-vaapi
    oe_cargo_build --package opennow-streamer --bin opennow-streamer --features linux-ffmpeg,linux-vaapi
    ${CARGO} build --frozen --release --target ${RUST_HOST_SYS} \
        --manifest-path ${S}/native/opennow-core/Cargo.toml \
        --bin opennow-core --bin opennow-update-helper --bin opennow-acceptance-verify
    opennow-license-report ${S}/THIRD_PARTY_NOTICES ${B}/THIRD_PARTY_NOTICES.generated \
        ${S}/native/opennow-core/Cargo.toml ${S}/native/opennow-streamer/Cargo.toml
}

do_install() {
    install -d ${D}${libdir}/opennow-native
    for artifact in opennow-core opennow-update-helper opennow-acceptance-verify \
        opennow-streamer libopennow_streamer_ffi.so; do
        install -m 0755 ${B}/target/${CARGO_TARGET_SUBDIR}/$artifact ${D}${libdir}/opennow-native/
    done
    install -m 0644 ${B}/THIRD_PARTY_NOTICES.generated ${D}${libdir}/opennow-native/
}

SYSROOT_DIRS += "${libdir}/opennow-native"
INHIBIT_SYSROOT_STRIP = "1"
