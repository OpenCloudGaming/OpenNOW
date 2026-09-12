from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = ROOT / "opennow-qt/packaging"


@unittest.skipUnless(os.name == "posix" and shutil.which("dpkg-shlibdeps"), "Requires Debian packaging tools")
class BundledDebTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.appdir = self.root / "AppDir"
        files = (
            "AppRun", "AppRun.wrapped", "apprun-hooks/linuxdeploy-plugin-qt-hook.sh",
            "usr/bin/opennow-core", "usr/bin/opennow-update-helper", "usr/bin/opennow-streamer",
            "usr/bin/opennow-acceptance-verify", "usr/bin/libopennow_streamer_ffi.so", "usr/bin/qt.conf",
            "usr/lib/libSDL3.so.0", "usr/lib/libva.so.2", "usr/lib/libva-drm.so.2",
            "usr/plugins/imageformats/libqsvg.so", "usr/plugins/platforms/libqxcb.so",
            "usr/plugins/platforms/libqoffscreen.so", "usr/plugins/platforms/libqwayland-egl.so",
            "usr/plugins/platforms/libqwayland-generic.so",
            "usr/plugins/wayland-shell-integration/libxdg-shell.so",
            "usr/qml/QtQuick/qmldir", "usr/qml/QtQuick/Controls/qmldir",
            "usr/share/doc/opennow/THIRD_PARTY_NOTICES", "usr/share/doc/libva2/copyright",
        )
        for name in files:
            path = self.appdir / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("package fixture\n")
        for name, directory in (
            ("io.github.opencloudgaming.OpenNOW.desktop", "applications"),
            ("io.github.opencloudgaming.OpenNOW.metainfo.xml", "metainfo"),
            ("io.github.opencloudgaming.OpenNOW.svg", "icons/hicolor/scalable/apps"),
        ):
            destination = self.appdir / "usr/share" / directory / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(PACKAGING / name, destination)
        (self.root / "library.c").write_text("int package_fixture(void) { return 0; }\n")
        (self.root / "main.c").write_text(
            "int package_fixture(void); int main(void) { return package_fixture(); }\n"
        )
        self.run_command([
            "cc", "-shared", "-fPIC", "-Wl,-soname,libQt6Core.so.6", str(self.root / "library.c"),
            "-o", str(self.appdir / "usr/lib/libQt6Core.so.6"),
        ])
        self.run_command([
            "cc", str(self.root / "main.c"), f"-L{self.appdir}/usr/lib", "-l:libQt6Core.so.6",
            "-Wl,-rpath,$ORIGIN/../lib", "-o", str(self.appdir / "usr/bin/opennow-qt"),
        ])
        architecture = self.run_command(["dpkg", "--print-architecture"]).stdout.strip()
        (self.root / "CMakeLists.txt").write_text(f'''
cmake_minimum_required(VERSION 3.24)
project(BundledDebContract VERSION 1.2.3 LANGUAGES NONE)
install(FILES main.c DESTINATION bin)
set(CPACK_PACKAGE_NAME OpenNOW)
set(CPACK_PACKAGE_CONTACT "OpenCloudGaming <support@opennow.app>")
set(CPACK_PACKAGE_FILE_NAME bundled)
set(CPACK_DEBIAN_FILE_NAME bundled.deb)
set(CPACK_DEBIAN_PACKAGE_ARCHITECTURE "{architecture}")
set(CPACK_DEBIAN_PACKAGE_VERSION "1.2.3~nightly.4.1")
set(CPACK_DEBIAN_PACKAGE_DEPENDS "libqt6core6 (>= 6.8), libsdl3-0")
include(CPack)
''')
        self.run_command(["cmake", "-S", str(self.root), "-B", str(self.root / "build")])

    def run_command(self, command):
        result = subprocess.run(command, capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def package(self):
        return subprocess.run([
            "cpack", "--config", str(self.root / "build/CPackConfig.cmake"),
            "-D", f"CPACK_PROJECT_CONFIG_FILE={PACKAGING}/LinuxBundledDeb.cmake",
            "-D", f"OPENNOW_APPDIR={self.appdir}", "-G", "DEB", "-B", str(self.root / "packages"),
        ], capture_output=True, text=True, timeout=120)

    def test_bundles_private_libraries_without_distribution_qt_or_file_conflicts(self):
        for attempt in range(2):
            with self.subTest(attempt=attempt):
                result = self.package()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        deb = self.root / "packages/bundled.deb"
        dependencies = self.run_command(["dpkg-deb", "-f", str(deb), "Depends"]).stdout
        self.assertNotIn("qt6", dependencies.lower())
        self.assertNotIn("sdl3", dependencies.lower())
        self.assertIn("libc6", dependencies)
        self.assertIn("libvulkan1", dependencies)
        self.assertEqual(self.run_command(["dpkg-deb", "-f", str(deb), "Version"]).stdout.strip(),
                         "1.2.3~nightly.4.1")
        extracted = self.root / "extracted"
        self.run_command(["dpkg-deb", "-x", str(deb), str(extracted)])
        launcher = extracted / "usr/bin/opennow-qt"
        self.assertTrue(os.access(launcher, os.X_OK))
        self.assertEqual(launcher.read_text(), '#!/bin/sh\nexec /opt/opennow/AppRun "$@"\n')
        self.assertFalse((extracted / "usr/bin/main.c").exists())
        self.assertFalse((extracted / "usr/share/doc/libva2").exists())
        self.assertTrue((extracted / "opt/opennow/usr/share/doc/libva2/copyright").is_file())
        self.assertTrue((extracted / "usr/share/doc/opennow/THIRD_PARTY_NOTICES").is_file())
        self.assertTrue((extracted / "usr/share/applications/io.github.opencloudgaming.OpenNOW.desktop").is_file())
        self.run_command([str(extracted / "opt/opennow/usr/bin/opennow-qt")])

    def test_incomplete_runtime_fails_before_creating_a_deb(self):
        (self.appdir / "usr/plugins/imageformats/libqsvg.so").unlink()
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing usr/plugins/imageformats/libqsvg.so", result.stdout + result.stderr)
        self.assertFalse((self.root / "packages/bundled.deb").exists())


class BundledDebWorkflowTest(unittest.TestCase):
    def test_every_release_deb_uses_and_tests_the_deployed_runtime(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (ROOT / ".github/workflows" / name).read_text()
                self.assertIn('CPACK_PROJECT_CONFIG_FILE=$PWD/opennow-qt/packaging/LinuxBundledDeb.cmake', workflow)
                self.assertIn('OPENNOW_APPDIR=$PWD/build/AppDir', workflow)
                self.assertIn('bash opennow-qt/packaging/verify_bundled_deb.sh "${debs[0]}"', workflow)
                self.assertLess(workflow.index("--plugin qt"), workflow.index("LinuxBundledDeb.cmake"))


if __name__ == "__main__":
    unittest.main()
