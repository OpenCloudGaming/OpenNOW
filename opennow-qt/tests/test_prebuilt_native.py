import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]
ARTIFACTS = (
    "opennow-core",
    "opennow-update-helper",
    "opennow-acceptance-verify",
    "opennow-streamer",
    "libopennow_streamer_ffi.so",
    "THIRD_PARTY_NOTICES.generated",
)


class PrebuiltNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="opennow-prebuilt-")
        cls.root = Path(cls.temporary.name)
        cls.source = cls.root / "fixture"
        cls.source.mkdir()
        (cls.source / "packaging").symlink_to(QT_SOURCE / "packaging", target_is_directory=True)
        cls.tools = cls.root / "tools"
        cls.tools.mkdir()
        cls.cargo_called = cls.root / "cargo-called"
        (cls.tools / "cargo").write_text(
            f"#!/bin/sh\ntouch '{cls.cargo_called}'\nexit 99\n"
        )
        (cls.tools / "cargo").chmod(0o755)
        (cls.source / "CMakeLists.txt").write_text(
            """cmake_minimum_required(VERSION 3.24)
project(PrebuiltNativeContract VERSION 1.0.0 LANGUAGES C CXX)
if(PRODUCE_ARTIFACTS)
    add_library(opennow_streamer_ffi SHARED library.c)
    set_target_properties(opennow_streamer_ffi PROPERTIES NO_SONAME TRUE)
    foreach(name opennow-core opennow-update-helper opennow-acceptance-verify opennow-streamer)
        add_executable(${name} tool.c)
    endforeach()
    file(WRITE "${CMAKE_BINARY_DIR}/THIRD_PARTY_NOTICES.generated" "CMake contract fixture notices\\n")
    return()
endif()
include("${QT_SOURCE}/cmake/NativeArtifacts.cmake")
if(VALIDATE_ONLY)
    return()
endif()
include(GNUInstallDirs)
include("${QT_SOURCE}/cmake/BuildMetadata.cmake")
add_executable(opennow-qt consumer.cpp)
set_target_properties(opennow-qt PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/app")
set(CMAKE_CURRENT_SOURCE_DIR "${QT_SOURCE}")
set(OPENNOW_EXECUTABLE_NAME opennow-qt)
add_library(contract-host-sdl SHARED IMPORTED)
set_target_properties(contract-host-sdl PROPERTIES IMPORTED_LOCATION "/host-only/libSDL3.so")
set(OPENNOW_SDL3_RUNTIME_TARGET contract-host-sdl)
include("${QT_SOURCE}/cmake/NativeRuntime.cmake")
include("${QT_SOURCE}/cmake/Packaging.cmake")
"""
        )
        (cls.source / "library.c").write_text(
            "int cmake_contract_fixture_value(void) { return 42; }\n"
        )
        (cls.source / "tool.c").write_text(
            '#include <stdio.h>\nint main(void) { puts("CMake contract fixture"); return 0; }\n'
        )
        (cls.source / "consumer.cpp").write_text(
            '#include "opennow_streamer_ffi.h"\n'
            'extern "C" int cmake_contract_fixture_value(void);\n'
            "int main() { return cmake_contract_fixture_value() == 42 ? 0 : 1; }\n"
        )
        cls.producer = cls.root / "producer"
        cls.run_command("cmake", "-S", cls.source, "-B", cls.producer, "-DPRODUCE_ARTIFACTS=ON")
        cls.run_command("cmake", "--build", cls.producer, "--parallel", "2")
        cls.artifacts = cls.root / "native artifacts"
        cls.artifacts.mkdir()
        for name in ARTIFACTS:
            shutil.copy2(cls.producer / name, cls.artifacts / name)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    @classmethod
    def run_command(cls, *args, expected_success=True, env=None):
        environment = dict(os.environ if env is None else env)
        environment["PATH"] = f"{cls.tools}{os.pathsep}{environment['PATH']}"
        result = subprocess.run(
            [str(arg) for arg in args], capture_output=True, text=True, env=environment
        )
        if (result.returncode == 0) != expected_success:
            raise AssertionError(f"Command: {args}\n{result.stdout}\n{result.stderr}")
        return result.stdout + result.stderr

    def configure(self, build, *args, artifacts=None, expected_success=True):
        return self.run_command(
            "cmake", "-S", self.source, "-B", build,
            f"-DQT_SOURCE={QT_SOURCE}",
            f"-DOPENNOW_PREBUILT_NATIVE_DIR={artifacts or self.artifacts}",
            *args, expected_success=expected_success,
        )

    def test_build_install_and_refresh_without_cargo(self):
        build = self.root / "consumer"
        self.configure(build, "-DCMAKE_BUILD_TYPE=Release")
        cache = (build / "CMakeCache.txt").read_text()
        self.assertNotIn("CARGO_EXECUTABLE:", cache)
        self.assertNotIn("OPENNOW_RUSTC_EXECUTABLE:", cache)
        self.assertFalse((build / "rust-target").exists())
        self.run_command("cmake", "--build", build, "--target", "opennow-qt", "--parallel", "2")
        app = build / "app"
        for name in ARTIFACTS[:-1]:
            self.assertEqual((app / name).read_bytes(), (self.artifacts / name).read_bytes())
        self.run_command(app / "opennow-qt")
        for name in ARTIFACTS[:4]:
            self.assertIn("CMake contract fixture", self.run_command(app / name))
        (app / "opennow-core").unlink()
        (app / "opennow-streamer").unlink()
        self.run_command("cmake", "--build", build, "--target", "opennow-qt")
        self.assertTrue((app / "opennow-core").is_file())
        self.assertTrue((app / "opennow-streamer").is_file())
        (app / "libopennow_streamer_ffi.so").unlink()
        self.run_command("cmake", "--build", build)
        self.run_command(app / "opennow-qt")
        for name in ARTIFACTS:
            with (self.artifacts / name).open("ab") as artifact:
                artifact.write(b"\nCMake contract fixture refresh\n")
        self.run_command("cmake", "--build", build, "--target", "opennow-qt")
        for name in ARTIFACTS[:-1]:
            self.assertEqual((app / name).read_bytes(), (self.artifacts / name).read_bytes())
        self.run_command(app / "opennow-qt")
        stage = self.root / "stage"
        self.run_command("cmake", "--install", build, "--prefix", "/usr",
                         env={**os.environ, "DESTDIR": str(stage)})
        installed = stage / "usr/bin"
        self.assertEqual({path.name for path in installed.iterdir()},
                         {"opennow-qt", *ARTIFACTS[:-1]})
        self.run_command(installed / "opennow-qt")
        for name in ARTIFACTS[:4]:
            self.assertTrue(os.access(installed / name, os.X_OK))
        notices = stage / "usr/share/doc/opennow/THIRD_PARTY_NOTICES"
        self.assertEqual(notices.read_bytes(), (self.artifacts / ARTIFACTS[-1]).read_bytes())
        self.assertFalse(list(stage.rglob("libSDL3*")))
        dynamic = self.run_command("readelf", "-d", installed / "opennow-qt")
        self.assertIn("$ORIGIN", dynamic)
        self.assertNotIn(str(self.artifacts), dynamic)
        hidden = self.artifacts.with_name("hidden artifacts")
        self.artifacts.rename(hidden)
        try:
            self.run_command(installed / "opennow-qt")
        finally:
            hidden.rename(self.artifacts)
        self.assertFalse(self.cargo_called.exists())

    def test_each_artifact_is_required_and_must_be_a_file(self):
        for name in ARTIFACTS:
            for directory in (False, True):
                with self.subTest(name=name, directory=directory):
                    root = self.root / f"invalid-{name}-{directory}"
                    artifacts = root / "artifacts"
                    shutil.copytree(self.artifacts, artifacts)
                    (artifacts / name).unlink()
                    if directory:
                        (artifacts / name).mkdir()
                    output = self.configure(root / "build", "-DVALIDATE_ONLY=ON",
                                            artifacts=artifacts, expected_success=False)
                    self.assertIn("OPENNOW_PREBUILT_NATIVE_DIR requires a file:", output)
                    self.assertIn(name, output)

    def test_prebuilt_mode_rejects_non_linux_targets(self):
        output = self.configure(self.root / "non-linux", "-DCMAKE_SYSTEM_NAME=Generic",
                                "-DVALIDATE_ONLY=ON", expected_success=False)
        self.assertIn("supported only for Linux targets", output)

    def test_default_mode_still_discovers_cargo(self):
        build = self.root / "default"
        self.run_command("cmake", "-S", self.source, "-B", build,
                         f"-DQT_SOURCE={QT_SOURCE}", "-DVALIDATE_ONLY=ON")
        self.assertIn("CARGO_EXECUTABLE:FILEPATH=", (build / "CMakeCache.txt").read_text())

    @unittest.skipUnless(shutil.which("wayland-scanner") and shutil.which("pkg-config"),
                         "Wayland scanner and development pkg-config files are required")
    def test_wayland_protocols_use_the_sysroot_exactly_once(self):
        protocols = Path(self.run_command(
            "pkg-config", "--variable=pkgdatadir", "wayland-protocols"
        ).strip())
        source = self.root / "wayland-fixture"
        source.mkdir()
        (source / "CMakeLists.txt").write_text(
            """cmake_minimum_required(VERSION 3.24)
project(WaylandProtocolContract LANGUAGES C CXX)
set(CMAKE_SYSROOT "${TEST_SYSROOT}")
add_library(Qt6::Gui INTERFACE IMPORTED)
add_library(Qt6::GuiPrivate INTERFACE IMPORTED)
include("${QT_SOURCE}/cmake/PlatformInput.cmake")
add_custom_target(contract-protocols ALL DEPENDS
    "${OPENNOW_INPUT_PROTOCOL_DIR}/relative-pointer-unstable-v1-client-protocol.h"
    "${OPENNOW_INPUT_PROTOCOL_DIR}/relative-pointer-unstable-v1-protocol.c"
    "${OPENNOW_INPUT_PROTOCOL_DIR}/pointer-constraints-unstable-v1-client-protocol.h"
    "${OPENNOW_INPUT_PROTOCOL_DIR}/pointer-constraints-unstable-v1-protocol.c")
file(WRITE "${CMAKE_BINARY_DIR}/resolved-protocols" "${WAYLAND_PROTOCOLS_DIR}")
"""
        )
        for mode in ("target-path", "sysroot-path", "native"):
            with self.subTest(mode=mode):
                root = self.root / f"wayland-{mode}"
                sysroot = root / "sysroot"
                target_data = "/usr/share/opennow-contract-protocols"
                data = sysroot / target_data.lstrip("/")
                for protocol in ("relative-pointer", "pointer-constraints"):
                    relative = Path("unstable") / protocol / f"{protocol}-unstable-v1.xml"
                    (data / relative).parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(protocols / relative, data / relative)
                pc = root / "pkgconfig"
                pc.mkdir()
                pkgdatadir = target_data if mode == "target-path" else str(data)
                (pc / "wayland-protocols.pc").write_text(
                    f"pkgdatadir={pkgdatadir}\nName: wayland-protocols\n"
                    "Description: Real Wayland XML in a contract-test sysroot\nVersion: 1.45\n"
                )
                build = root / "build"
                environment = {**os.environ, "PKG_CONFIG_PATH": str(pc)}
                self.run_command(
                    "cmake", "-S", source, "-B", build, f"-DQT_SOURCE={QT_SOURCE}",
                    *([] if mode == "native" else ["-DCMAKE_SYSTEM_NAME=Linux"]),
                    f"-DTEST_SYSROOT={sysroot}", env=environment,
                )
                self.assertEqual((build / "resolved-protocols").read_text(), str(data))
                self.run_command("cmake", "--build", build, "--target", "contract-protocols",
                                 env=environment)
                for protocol in ("relative-pointer", "pointer-constraints"):
                    header = build / "input-protocols" / f"{protocol}-unstable-v1-client-protocol.h"
                    self.assertIn("Generated by wayland-scanner", header.read_text())


if __name__ == "__main__":
    unittest.main()
