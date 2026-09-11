import hashlib
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
                + 'if(WIN32)\n'
                + 'set(CPACK_PACKAGE_NAME "OpenNOW")\n'
                + f'include("{(MODULE.parent / "WindowsInstaller.cmake").as_posix()}")\n'
                + 'message("MSI=${CPACK_PACKAGE_VERSION}|${CPACK_PACKAGE_NAME}|${CPACK_PACKAGE_INSTALL_DIRECTORY}|${CPACK_WIX_UPGRADE_GUID}")\n'
                + 'message("LAUNCHER=${CPACK_PACKAGE_EXECUTABLES}")\n'
                + 'if(CPACK_RESOURCE_FILE_LICENSE)\n'
                + 'file(SHA256 "${CPACK_RESOURCE_FILE_LICENSE}" license_hash)\n'
                + 'message("LICENSE=${license_hash}")\n'
                + 'endif()\n'
                + 'endif()\n'
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

    def windows_installer(self, version):
        result = self.metadata(WIN32="TRUE", OPENNOW_BUILD_VERSION=version)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"RESULT={version}|1.0.0|", result.stderr)
        return next(line.removeprefix("MSI=").split("|")
                    for line in result.stderr.splitlines() if line.startswith("MSI="))

    def test_nightly_msi_orders_runs_and_retries(self):
        versions = ["1.0.0-nightly.255.65535", "1.0.0-nightly.256.1",
                    "1.0.0-nightly.256.2", "1.0.0-nightly.65535.65535"]
        actual = [self.windows_installer(version)[0] for version in versions]
        self.assertEqual(actual, ["0.255.65535", "1.0.1", "1.0.2", "255.255.65535"])
        self.assertEqual(actual, sorted(actual, key=lambda value: tuple(map(int, value.split(".")))))

    def test_nightly_msi_is_isolated_from_stable_and_supporter(self):
        stable = self.windows_installer("1.0.0")
        nightly = self.windows_installer("1.0.0-nightly.256.1")
        supporter = self.windows_installer("1.0.0-supporter.256.1")
        self.assertEqual(stable[0], "1.0.0")
        self.assertEqual(stable[1:3], ["OpenNOW", ""])
        self.assertEqual(stable[3], "6E81F7AE-B19D-4E87-A94A-2B2F01EBF762")
        self.assertEqual(nightly[1:3], ["OpenNOW Nightly", "OpenNOW Nightly"])
        self.assertEqual(supporter[1:3], ["OpenNOW Supporter", "OpenNOW Supporter"])
        self.assertEqual(len({stable[3], nightly[3], supporter[3]}), 3)

    def test_msi_version_limits_fail_closed(self):
        for version in ("1.0.0-nightly.65536.1", "1.0.0-nightly.1.65536",
                        "1.0.0-nightly.999999999999999999999.1", "256.0.0", "1.256.0", "1.0.65536"):
            with self.subTest(version=version):
                result = self.metadata(WIN32="TRUE", OPENNOW_BUILD_VERSION=version)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("MSI", result.stderr)

    def test_msi_launcher_label_and_license_follow_the_package(self):
        license_digest = hashlib.sha256((MODULE.parents[2] / "LICENSE").read_bytes()).hexdigest()
        for version, label in (("1.0.0", "OpenNOW"),
                               ("1.0.0-nightly.256.1", "OpenNOW Nightly"),
                               ("1.0.0-supporter.256.1", "OpenNOW Supporter")):
            with self.subTest(version=version):
                result = self.metadata(WIN32="TRUE", OPENNOW_BUILD_VERSION=version)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"LAUNCHER=OpenNOW;{label}", result.stderr)
                self.assertIn(f"LICENSE={license_digest}", result.stderr)

    def test_invalid_versions_and_architectures_fail_closed(self):
        for version in ("v1.0.0", "1.0.0-nightly", "1.0.0-nightly.01.1", "01.0.0", "1.0.0;bad", "1.0.0-nightly.1.0"):
            with self.subTest(version=version):
                self.assertNotEqual(self.metadata(OPENNOW_BUILD_VERSION=version).returncode, 0)
        self.assertNotEqual(self.metadata(CMAKE_SYSTEM_PROCESSOR="unknown").returncode, 0)


if __name__ == "__main__":
    unittest.main()
