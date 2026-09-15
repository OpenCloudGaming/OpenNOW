SUMMARY = "OpenNOW Qt cloud gaming client"

require opennow-source.inc

inherit qt6-cmake features_check

COMPATIBLE_HOST = "(x86_64|aarch64).*-linux$"
REQUIRED_DISTRO_FEATURES = "opengl vulkan wayland"
DEPENDS += "libsdl3 opennow-runtime qtdeclarative qtdeclarative-native \
    qtmultimedia qtshadertools qtshadertools-native qtsvg qtwayland \
    vulkan-headers vulkan-loader wayland wayland-native wayland-protocols"

OECMAKE_SOURCEPATH = "${S}/opennow-qt"
EXTRA_OECMAKE += "-DBUILD_TESTING=OFF \
    -DOPENNOW_BUILD_VERSION=${OPENNOW_VERSION} \
    -DOPENNOW_PREBUILT_NATIVE_DIR=${STAGING_LIBDIR}/opennow-native"

RDEPENDS:${PN} += "ca-certificates qtbase-plugins qtdeclarative-qmlplugins \
    qtmultimedia-plugins qtmultimedia-qmlplugins qtsvg-plugins qtwayland-plugins"
FILES:${PN} += "${bindir}/libopennow_streamer_ffi.so ${datadir}/metainfo"
