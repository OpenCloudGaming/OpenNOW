import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "packaging"))
from verify_linux_package import verify_capabilities, verify_core, verify_core_capabilities, verify_package


class LinuxPackageDependenciesTest(unittest.TestCase):
    def test_debian_packages_require_svg_image_plugin(self):
        qt_source = Path(__file__).resolve().parents[1]
        for processor, architecture in (("x86_64", "amd64"), ("aarch64", "arm64")):
            with self.subTest(architecture=architecture), tempfile.TemporaryDirectory() as directory:
                source = Path(directory)
                build = source / "build"
                (source / "main.cpp").write_text("int main() { return 0; }\n")
                (source / "CMakeLists.txt").write_text(
                    f'''cmake_minimum_required(VERSION 3.24)
project(LinuxPackageContract VERSION 1.0.0 LANGUAGES CXX)
include(GNUInstallDirs)
add_executable(opennow-qt main.cpp)
set(APPLE FALSE)
set(WIN32 FALSE)
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR "{processor}")
set(CMAKE_CURRENT_SOURCE_DIR "{qt_source.as_posix()}")
include("{qt_source.as_posix()}/cmake/BuildMetadata.cmake")
set(OPENNOW_SDL3_RUNTIME_TARGET SDL3-runtime)
add_library(SDL3-runtime SHARED IMPORTED)
set_target_properties(SDL3-runtime PROPERTIES
    IMPORTED_LOCATION "${{CMAKE_BINARY_DIR}}/libSDL3.so")
set(OPENNOW_STREAMER_FFI_RUNTIME "${{CMAKE_BINARY_DIR}}/libopennow_streamer_ffi.so")
set(OPENNOW_STREAMER_BIN_ARTIFACT "${{CMAKE_BINARY_DIR}}/opennow-streamer")
set(OPENNOW_GENERATED_NOTICES "${{CMAKE_BINARY_DIR}}/THIRD_PARTY_NOTICES")
include("{qt_source.as_posix()}/cmake/Packaging.cmake")
'''
                )
                result = subprocess.run(
                    ["cmake", "-S", str(source), "-B", str(build), "-G", "Unix Makefiles"],
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                contract = source / "check.cmake"
                contract.write_text(
                    f'''include("{build.as_posix()}/CPackConfig.cmake")
file(WRITE "{source.as_posix()}/dependencies.txt" "${{CPACK_DEBIAN_PACKAGE_DEPENDS}}")
file(WRITE "{source.as_posix()}/architecture.txt" "${{CPACK_DEBIAN_PACKAGE_ARCHITECTURE}}")
'''
                )
                subprocess.run(["cmake", "-P", str(contract)], check=True, capture_output=True)
                dependencies = (source / "dependencies.txt").read_text().split(",")
                self.assertIn("qt6-svg-plugins (>= 6.8)", [item.strip() for item in dependencies])
                self.assertIn("pkexec", [item.strip() for item in dependencies])
                self.assertEqual((source / "architecture.txt").read_text(), architecture)
                self.assertIn("opennow-update-helper", (build / "cmake_install.cmake").read_text())

    def test_missing_or_nonexecutable_helper_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            bin_dir = Path(directory)
            with self.assertRaisesRegex(ValueError, "executable update helper"):
                verify_package(bin_dir)
            helper = bin_dir / "opennow-update-helper"
            helper.write_bytes(b"test helper")
            helper.chmod(0o600)
            with self.assertRaisesRegex(ValueError, "executable update helper"):
                verify_package(bin_dir)


class CorePackageCapabilitiesTest(unittest.TestCase):
    def setUp(self):
        self.message = {"id": "package-core", "ok": True,
                        "result": {"protocolVersion": 6, "capabilities": ["catalog.storeLocal.v2", "account.mutations.scoped.v1", "session.allocationSettlement.v1"]}}

    def test_store_replacement_capability_is_required(self):
        verify_core_capabilities(self.message)
        self.message["result"]["capabilities"] = ["catalog.storeLocal.v1", "account.mutations.scoped.v1", "session.allocationSettlement.v1"]
        with self.assertRaisesRegex(ValueError, "Store replacement support"):
            verify_core_capabilities(self.message)

    def test_scoped_account_mutation_capability_is_required(self):
        self.message["result"]["capabilities"] = ["catalog.storeLocal.v2", "session.allocationSettlement.v1"]
        with self.assertRaisesRegex(ValueError, "scoped account mutations"):
            verify_core_capabilities(self.message)

    def test_allocation_settlement_capability_is_required(self):
        self.message["result"]["capabilities"] = ["catalog.storeLocal.v2", "account.mutations.scoped.v1"]
        with self.assertRaisesRegex(ValueError, "allocation settlement"):
            verify_core_capabilities(self.message)

    def test_previous_protocol_is_rejected(self):
        self.message["result"]["protocolVersion"] = 5
        with self.assertRaisesRegex(ValueError, "Store replacement support"):
            verify_core_capabilities(self.message)

    def test_core_failure_is_not_a_valid_handshake(self):
        self.message["ok"] = False
        with self.assertRaisesRegex(ValueError, "Store replacement support"):
            verify_core_capabilities(self.message)

    def test_installed_core_probe_checks_the_real_response(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "opennow-core"
            for capability in ("catalog.storeLocal.v2", "catalog.storeLocal.v1"):
                self.message["result"]["capabilities"] = [capability, "account.mutations.scoped.v1", "session.allocationSettlement.v1"]
                binary.write_text(
                    "#!/usr/bin/env python3\nimport json, sys\n"
                    "request = json.loads(sys.stdin.readline())\n"
                    "assert request['method'] == 'core.hello' and request['params']['protocolVersion'] == 6\n"
                    f"print({json.dumps(self.message)!r}, flush=True)\n"
                )
                binary.chmod(0o700)
                if capability.endswith("v2"):
                    verify_core(Path(directory))
                else:
                    with self.assertRaisesRegex(ValueError, "Store replacement support"):
                        verify_core(Path(directory))


class LinuxPackageCapabilitiesTest(unittest.TestCase):
    def setUp(self):
        self.message = {
            "type": "ready",
            "capabilities": {
                "videoBackends": [
                    {
                        "backend": "vaapi",
                        "available": False,
                        "codecs": [
                            {"codec": "h264", "available": False, "reason": "no render node"},
                            {"codec": "h265", "available": False},
                            {"codec": "av1", "available": False},
                        ],
                    },
                    {
                        "backend": "ffmpeg",
                        "codecs": [
                            {"codec": codec, "available": True}
                            for codec in ("h264", "h265", "av1")
                        ],
                    },
                ]
            },
        }

    def test_driverless_runner_can_validate_compiled_vaapi(self):
        verify_capabilities(self.message)

    def test_h264_hardware_can_be_available(self):
        backend = self.message["capabilities"]["videoBackends"][0]
        backend["available"] = True
        backend["codecs"][0] = {"codec": "h264", "available": True}
        verify_capabilities(self.message)

    def test_missing_vaapi_feature_is_rejected(self):
        self.message["capabilities"]["videoBackends"][0]["codecs"][0]["reason"] = (
            "crate was built without the vaapi feature"
        )
        with self.assertRaisesRegex(ValueError, "without native VAAPI"):
            verify_capabilities(self.message)

    def test_hevc_and_av1_native_vaapi_are_rejected(self):
        for index in (1, 2):
            with self.subTest(index=index):
                message = copy.deepcopy(self.message)
                message["capabilities"]["videoBackends"][0]["codecs"][index]["available"] = True
                with self.assertRaisesRegex(ValueError, "HEVC or AV1"):
                    verify_capabilities(message)

    def test_missing_ffmpeg_fallback_is_rejected(self):
        self.message["capabilities"]["videoBackends"][1]["codecs"][0]["available"] = False
        with self.assertRaisesRegex(ValueError, "FFmpeg software fallback"):
            verify_capabilities(self.message)

    def test_software_av1_can_be_unavailable_without_a_cpu_decoder(self):
        self.message["capabilities"]["videoBackends"][1]["codecs"][2]["available"] = False
        verify_capabilities(self.message)

    def test_missing_hevc_software_decoder_is_rejected(self):
        self.message["capabilities"]["videoBackends"][1]["codecs"][1]["available"] = False
        with self.assertRaisesRegex(ValueError, "FFmpeg software fallback"):
            verify_capabilities(self.message)

    def test_software_codec_contract_must_be_complete(self):
        for index in range(3):
            with self.subTest(index=index):
                message = copy.deepcopy(self.message)
                message["capabilities"]["videoBackends"][1]["codecs"].pop(index)
                with self.assertRaisesRegex(ValueError, "FFmpeg software fallback"):
                    verify_capabilities(message)

    def test_software_codec_availability_must_be_boolean(self):
        for index in range(3):
            for available in (None, 1, "false"):
                with self.subTest(index=index, available=available):
                    message = copy.deepcopy(self.message)
                    message["capabilities"]["videoBackends"][1]["codecs"][index]["available"] = available
                    with self.assertRaisesRegex(ValueError, "FFmpeg software fallback"):
                        verify_capabilities(message)

    def test_non_ready_response_is_rejected(self):
        self.message["type"] = "error"
        with self.assertRaisesRegex(ValueError, "did not return ready"):
            verify_capabilities(self.message)


if __name__ == "__main__":
    unittest.main()
