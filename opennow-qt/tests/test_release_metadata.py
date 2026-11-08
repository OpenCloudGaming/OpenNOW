from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE = Path(__file__).resolve().parents[1] / "cmake" / "BuildMetadata.cmake"


class BuildMetadataTest(unittest.TestCase):
    def metadata(self, **variables):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "metadata.cmake"
            script.write_text(
                'cmake_minimum_required(VERSION 3.24)\n'
                + '\n'.join(f'set({key} "{value}")' for key, value in {
                    "PROJECT_VERSION": "1.0.0",
                    "CMAKE_SYSTEM_NAME": "Linux",
                    "CMAKE_SYSTEM_PROCESSOR": "x86_64",
                    **variables,
                }.items())
                + f'\ninclude("{MODULE.as_posix()}")\n'
                + 'message("RESULT=${OPENNOW_BUILD_VERSION}|${OPENNOW_NUMERIC_VERSION}|${OPENNOW_DEBIAN_VERSION}|${OPENNOW_PACKAGE_FILE_NAME}|${OPENNOW_DEBIAN_ARCH}")\n'
            )
            return subprocess.run(["cmake", "-P", str(script)], cwd=directory, capture_output=True, text=True)

    def test_stable_version_defaults_to_project(self):
        result = self.metadata()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("RESULT=1.0.0|1.0.0|1.0.0|OpenNOW-Qt-1.0.0-Linux-x64|amd64", result.stderr)

    def test_nightly_keeps_identity_and_debian_ordering(self):
        result = self.metadata(OPENNOW_BUILD_VERSION="1.0.0-nightly.23.2", CMAKE_SYSTEM_PROCESSOR="aarch64")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("1.0.0-nightly.23.2|1.0.0|1.0.0~nightly.23.2|OpenNOW-Qt-1.0.0-nightly.23.2-Linux-arm64|arm64", result.stderr)

    def test_windows_cross_compiler_overrides_host(self):
        for arch, expected in (("ARM64", "arm64"), ("x64", "x64")):
            with self.subTest(arch=arch):
                result = self.metadata(WIN32="TRUE", CMAKE_SYSTEM_NAME="Windows", CMAKE_SYSTEM_PROCESSOR="AMD64", CMAKE_CXX_COMPILER_ARCHITECTURE_ID=arch)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"OpenNOW-Qt-1.0.0-Windows-{expected}", result.stderr)

    def test_windows_generator_platform_fallback(self):
        result = self.metadata(WIN32="TRUE", CMAKE_SYSTEM_NAME="Windows", CMAKE_GENERATOR_PLATFORM="ARM64")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("OpenNOW-Qt-1.0.0-Windows-arm64", result.stderr)

    def test_macos_cross_target_overrides_host(self):
        result = self.metadata(APPLE="TRUE", CMAKE_SYSTEM_NAME="Darwin", CMAKE_OSX_ARCHITECTURES="arm64")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("OpenNOW-Qt-1.0.0-Darwin-arm64", result.stderr)

    def test_invalid_versions_and_architectures_fail_closed(self):
        for version in ("v1.0.0", "1.0.0-nightly", "1.0.0-nightly.01.1", "01.0.0", "1.0.0;bad", "1.0.0-nightly.1.0"):
            with self.subTest(version=version):
                self.assertNotEqual(self.metadata(OPENNOW_BUILD_VERSION=version).returncode, 0)
        self.assertNotEqual(self.metadata(CMAKE_SYSTEM_PROCESSOR="unknown").returncode, 0)


if __name__ == "__main__":
    unittest.main()
