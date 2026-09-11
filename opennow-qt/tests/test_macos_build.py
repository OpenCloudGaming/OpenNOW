from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]


class MacOSBuildContractTest(unittest.TestCase):
    def test_public_dmg_is_copied_before_relocated_smoke(self):
        workflow = (QT_SOURCE.parent / ".github/workflows/qt-build.yml").read_text()
        self.assertIn("-G 'DragNDrop;ZIP'", workflow)
        commands = [
            'hdiutil verify "${dmgs[0]}"',
            'hdiutil attach "${dmgs[0]}" -readonly -nobrowse -mountpoint "$mount"',
            'ditto "$mount/OpenNOW.app" "$RUNNER_TEMP/dmg-relocated/OpenNOW.app"',
            'hdiutil detach "$mount"\n',
            'for app in "$zip_app" "$RUNNER_TEMP/dmg-relocated/OpenNOW.app"; do',
            '/usr/bin/codesign --verify --deep --strict "$app"',
            "grep -Fx 'Identifier=io.github.opencloudgaming.OpenNOW'",
            'mv "$QT_ROOT_DIR" "$QT_ROOT_DIR.unavailable"',
            '[bin_dir / "OpenNOW", "--smoke-test"',
        ]
        positions = [workflow.index(command) for command in commands]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("name: opennow-qt-macos-arm64-unsigned", workflow)
        self.assertIn("name: opennow-macos-arm64-validation", workflow)

    def test_architecture_validation_places_input_before_architecture_list(self):
        workflow = QT_SOURCE.parent / ".github/workflows/qt-build.yml"
        commands = [shlex.split(line.strip()) for line in workflow.read_text().splitlines()
                    if line.strip().startswith("lipo ")]
        self.assertEqual(len(commands), 2)
        self.assertEqual(
            commands,
            [["lipo", "$bin/$executable", "-verify_arch", "arm64"],
             ["lipo", "$bin/libopennow_streamer_ffi.dylib", "-verify_arch", "arm64"]],
        )

    def configure(self, arch="arm64", rust_target="", adhoc=False):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        source = Path(directory.name)
        build = source / "build"
        (source / "main.cpp").write_text("int main() { return 0; }\n")
        (source / "CMakeLists.txt").write_text(
            f'''cmake_minimum_required(VERSION 3.24)
project(MacOSBuildContract VERSION 1.0.0 LANGUAGES CXX)
include(GNUInstallDirs)
add_executable(opennow-qt main.cpp)
set_target_properties(opennow-qt PROPERTIES MACOSX_BUNDLE_GUI_IDENTIFIER "io.github.opencloudgaming.OpenNOW")
set(APPLE TRUE)
set(WIN32 FALSE)
set(CMAKE_SYSTEM_NAME Darwin)
set(CMAKE_OSX_ARCHITECTURES "{arch}")
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
set(OPENNOW_RUST_TARGET "{rust_target}")
set(CARGO_EXECUTABLE cargo)
set(OPENNOW_MACOS_ADHOC_SIGN {"ON" if adhoc else "OFF"})
include("{QT_SOURCE.as_posix()}/cmake/BuildMetadata.cmake")
include("{QT_SOURCE.as_posix()}/cmake/NativeRuntime.cmake")
set(OPENNOW_EXECUTABLE_NAME OpenNOW)
set(OPENNOW_SDL3_RUNTIME_TARGET SDL3-runtime)
add_library(SDL3-runtime SHARED IMPORTED)
set_target_properties(SDL3-runtime PROPERTIES
    IMPORTED_LOCATION "${{CMAKE_BINARY_DIR}}/libSDL3.dylib")
function(qt_generate_deploy_qml_app_script)
    file(WRITE "${{CMAKE_BINARY_DIR}}/deploy.cmake" "")
    file(WRITE "${{CMAKE_BINARY_DIR}}/deploy-args.txt" "${{ARGV}}")
    set(opennow_deploy_script "${{CMAKE_BINARY_DIR}}/deploy.cmake" PARENT_SCOPE)
endfunction()
include("{QT_SOURCE.as_posix()}/cmake/Packaging.cmake")
get_target_property(build_rpath opennow-qt BUILD_RPATH)
get_target_property(install_rpath opennow-qt INSTALL_RPATH)
file(GENERATE OUTPUT "${{CMAKE_BINARY_DIR}}/contract.txt" CONTENT
"${{OPENNOW_RUST_TARGET}}\n${{OPENNOW_CORE_ARTIFACT_ROOT}}\n${{OPENNOW_STREAMER_FFI_RUNTIME}}\n${{build_rpath}}\n${{install_rpath}}\n")
'''
        )
        result = subprocess.run(
            ["cmake", "-S", str(source), "-B", str(build),
             "-G", "Unix Makefiles", "-DCMAKE_BUILD_TYPE=Release"],
            capture_output=True, text=True,
        )
        return result, build

    def test_rust_target_follows_qt_architecture(self):
        for arch, target in (("arm64", "aarch64-apple-darwin"),
                             ("x86_64", "x86_64-apple-darwin")):
            with self.subTest(arch=arch):
                result, build = self.configure(arch)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                contract = (build / "contract.txt").read_text().splitlines()
                self.assertEqual(contract[0], target)
                self.assertTrue(contract[1].endswith(f"rust-target/{target}"))
                self.assertTrue(contract[2].endswith(
                    f"streamer-rust-target/{target}/release/libopennow_streamer_ffi.dylib"))
                self.assertIn("@loader_path", contract[3])
                self.assertIn("@loader_path", contract[4])

    def test_matching_explicit_rust_target_is_accepted(self):
        result, _ = self.configure(rust_target="aarch64-apple-darwin")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_mismatched_rust_target_is_rejected(self):
        result, _ = self.configure(rust_target="x86_64-apple-darwin")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must match the macOS Qt architecture", result.stderr)

    def test_ffi_requests_relocatable_install_name(self):
        result, build = self.configure()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        rule = (build / "CMakeFiles/opennow-streamer-ffi-build.dir/build.make").read_text()
        self.assertIn("cargo rustc", rule)
        self.assertIn("-- -C rpath=yes", rule)
        self.assertIn("--target aarch64-apple-darwin", rule)

    def test_bundle_explicitly_installs_native_helpers(self):
        result, build = self.configure()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        rule = (build / "CMakeFiles/opennow-update-helper-build.dir/build.make").read_text()
        self.assertIn("--bin opennow-update-helper", rule)
        self.assertIn("rust-target/aarch64-apple-darwin/release/opennow-update-helper", rule)
        cpack = (build / "CPackConfig.cmake").read_text()
        self.assertIn('set(CPACK_GENERATOR "DragNDrop;ZIP")', cpack)
        self.assertIn('set(CPACK_PACKAGE_FILE_NAME "OpenNOW-Qt-1.0.0-Darwin-arm64")', cpack)
        install = (build / "cmake_install.cmake").read_text()
        self.assertIn("OpenNOW.app/Contents/MacOS", install)
        for helper in ("opennow-core", "opennow-acceptance-verify", "opennow-update-helper"):
            self.assertIn(f"rust-target/aarch64-apple-darwin/release/{helper}", install)
        for runtime in ("opennow-streamer", "libopennow_streamer_ffi.dylib"):
            self.assertIn(f"streamer-rust-target/aarch64-apple-darwin/release/{runtime}", install)

    def test_deployment_scans_helper_dependencies(self):
        result, build = self.configure()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        arguments = (build / "deploy-args.txt").read_text().split(";")
        for helper in ("opennow-core", "opennow-acceptance-verify", "opennow-update-helper", "opennow-streamer"):
            self.assertIn(f"-executable=OpenNOW.app/Contents/MacOS/{helper}", arguments)

    def test_nightly_deployment_explicitly_seals_bundle_with_adhoc_identity(self):
        for enabled in (False, True):
            with self.subTest(enabled=enabled):
                result, build = self.configure(adhoc=enabled)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                arguments = (build / "deploy-args.txt").read_text().split(";")
                self.assertEqual("-codesign=-" in arguments, enabled)
                install = (build / "cmake_install.cmake").read_text()
                self.assertEqual("macos-adhoc-seal.cmake" in install, enabled)
                if enabled:
                    self.assertLess(install.index("/deploy.cmake"), install.index("/macos-adhoc-seal.cmake"))
                    seal = (build / "macos-adhoc-seal.cmake").read_text()
                    self.assertIn('--identifier "io.github.opencloudgaming.OpenNOW"', seal)
                    self.assertIn("--verify --deep --strict", seal)
                    self.assertIn('$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}/OpenNOW.app', seal)
        workflow = (QT_SOURCE.parent / ".github/workflows/qt-build.yml").read_text()
        self.assertIn("-DOPENNOW_MACOS_ADHOC_SIGN=ON", workflow)
        self.assertIn("grep -Fx 'Signature=adhoc'", workflow)
        self.assertIn("grep -Fx 'TeamIdentifier=not set'", workflow)
        self.assertIn("Print :CFBundleIdentifier", workflow)
        main = (QT_SOURCE / "CMakeLists.txt").read_text()
        self.assertIn('MACOSX_BUNDLE_GUI_IDENTIFIER "io.github.opencloudgaming.OpenNOW"', main)
        self.assertIn("${MACOSX_BUNDLE_GUI_IDENTIFIER}", (QT_SOURCE / "packaging/Info.plist.in").read_text())
        candidate = (QT_SOURCE.parent / ".github/workflows/qt-release-candidate.yml").read_text()
        self.assertNotIn("OPENNOW_MACOS_ADHOC_SIGN", candidate)


if __name__ == "__main__":
    unittest.main()
