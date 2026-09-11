import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = ROOT / "opennow-qt/packaging"


class UpdateHelperPackagingTest(unittest.TestCase):
    def test_windows_static_crt_is_scoped_to_separate_helper_build(self):
        for target in ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                source = Path(directory)
                build = source / "build"
                cargo = source / "cargo-fixture"
                cargo.write_text(f'''#!{sys.executable}
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
target_dir = Path(args[args.index("--target-dir") + 1])
target = args[args.index("--target") + 1]
artifact = target_dir / target / "release/opennow-update-helper.exe"
artifact.parent.mkdir(parents=True, exist_ok=True)
artifact.write_bytes(b"helper deployment fixture")
(target_dir / "environment.json").write_text(json.dumps({{
    "RUSTFLAGS": os.environ.get("RUSTFLAGS"),
    "CARGO_ENCODED_RUSTFLAGS": os.environ.get("CARGO_ENCODED_RUSTFLAGS"),
}}))
''')
                cargo.chmod(0o700)
                (source / "main.cpp").write_text("int main() { return 0; }\n")
                (source / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(UpdateHelperContract LANGUAGES CXX)
add_executable(opennow-qt main.cpp)
set(WIN32 TRUE)
set(APPLE FALSE)
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_CURRENT_SOURCE_DIR "{(ROOT / 'opennow-qt').as_posix()}")
set(OPENNOW_RUST_TARGET "{target}")
set(CARGO_EXECUTABLE "{cargo.as_posix()}")
include("{(ROOT / 'opennow-qt/cmake/NativeRuntime.cmake').as_posix()}")
''')
                result = subprocess.run(["cmake", "-S", str(source), "-B", str(build),
                                         "-G", "Unix Makefiles", "-DCMAKE_BUILD_TYPE=Release"],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                helper = (build / "CMakeFiles/opennow-update-helper-build.dir/build.make").read_text()
                self.assertIn("--unset=CARGO_ENCODED_RUSTFLAGS", helper)
                self.assertIn("target-feature=+crt-static", helper)
                self.assertIn(f"--target {target}", helper)
                self.assertIn(f"update-helper-rust-target/{target}/release/opennow-update-helper.exe", helper)
                self.assertIn("--bin opennow-update-helper", helper)
                for name in ("opennow-core", "opennow-streamer-ffi-build", "opennow-streamer-bin-build"):
                    rule = (build / f"CMakeFiles/{name}.dir/build.make").read_text()
                    self.assertNotIn("crt-static", rule)
                    self.assertNotIn("update-helper-rust-target", rule)
                result = subprocess.run(["cmake", "--build", str(build), "--target", "opennow-update-helper-build"],
                                        env={**os.environ, "RUSTFLAGS": "parent-flags",
                                             "CARGO_ENCODED_RUSTFLAGS": "encoded-parent-flags"},
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(json.loads((build / "update-helper-rust-target/environment.json").read_text()), {
                    "RUSTFLAGS": "-C target-feature=+crt-static", "CARGO_ENCODED_RUSTFLAGS": None,
                })
                self.assertEqual((build / "opennow-update-helper.exe").read_bytes(), b"helper deployment fixture")

    def test_windows_authoritative_list_drives_installation_and_both_package_checks(self):
        names = (PACKAGING / "windows-release-binaries.txt").read_text().splitlines()
        self.assertEqual(names.count("opennow-update-helper.exe"), 1)
        install = (PACKAGING / "WindowsReleaseBinaries.cmake").read_text()
        self.assertIn('file(STRINGS "${CMAKE_CURRENT_LIST_DIR}/windows-release-binaries.txt"', install)
        self.assertIn('"$<TARGET_FILE_DIR:opennow-qt>/${OPENNOW_WINDOWS_RELEASE_BINARY}"', install)
        validation = (PACKAGING / "windows-release.ps1").read_text()
        self.assertIn('Get-Content (Join-Path $PSScriptRoot "windows-release-binaries.txt")', validation)
        self.assertIn('Assert-OpenNowStandaloneUpdateHelper -Path $matches[0].FullName', validation)
        self.assertIn('dumpbin /dependents $Path', validation)
        for function in ("Assert-OpenNowSignedPackage", "Assert-OpenNowPackagePayload"):
            body = validation.split(f"function {function} {{", 1)[1].split("\nfunction ", 1)[0]
            self.assertIn("Get-OpenNowReleaseBinaries -Root $Root", body)
            self.assertIn("Get-FileHash $file.FullName -Algorithm SHA256", body)
        workflow = (ROOT / ".github/workflows/qt-build.yml").read_text()
        for root in ("$msiExpanded", "$zipExpanded"):
            self.assertIn(f"Assert-OpenNowPackagePayload -Root {root}", workflow)
        candidate = (ROOT / ".github/workflows/qt-release-candidate.yml").read_text()
        self.assertEqual(candidate.count("Assert-OpenNowSignedPackage -Root"), 2)

    def test_final_macos_dmg_validates_helper_architecture_and_dependencies(self):
        workflow = (ROOT / ".github/workflows/qt-build.yml").read_text()
        validation = workflow.split("      - name: Test relocated bundle without development libraries", 1)[1]
        self.assertIn('for app in "$zip_app" "$RUNNER_TEMP/dmg-relocated/OpenNOW.app"; do', validation)
        self.assertIn("opennow-acceptance-verify opennow-update-helper opennow-streamer; do", validation)
        self.assertIn('"opennow-update-helper", "opennow-streamer", "libopennow_streamer_ffi.dylib"', validation)


if __name__ == "__main__":
    unittest.main()
