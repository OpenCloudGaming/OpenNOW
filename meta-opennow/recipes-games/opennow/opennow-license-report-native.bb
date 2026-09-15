SUMMARY = "Build-host license notice generator for OpenNOW"

require opennow-source.inc
require opennow-crates.inc

inherit cargo pkgconfig native

DEPENDS += "dbus"
CARGO_SRC_DIR = "native/opennow-core"
CARGO_BUILD_FLAGS += "--bin opennow-license-report"
export CARGO_PROFILE_RELEASE_STRIP = "false"

do_install() {
    install -d ${D}${bindir}
    install -m 0755 ${B}/target/${CARGO_TARGET_SUBDIR}/opennow-license-report ${D}${bindir}/
}
