import importlib.util
from pathlib import Path
import plistlib
import subprocess
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]


def load_test_module(name):
    spec = importlib.util.spec_from_file_location(name, QT_SOURCE / "tests" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUILD_TESTS = load_test_module("test_macos_build")
RELEASE_TESTS = load_test_module("test_macos_release")


class MacOSAwdlPackagingTest(unittest.TestCase):
    def test_both_workflows_check_the_packaged_helper(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (QT_SOURCE.parent / ".github/workflows" / name).read_text()
                self.assertIn("python3 -m unittest discover -s opennow-qt/packaging -p test_macos_awdl.py",
                              workflow)
                self.assertIn('test -x "$app/Contents/Helpers/opennow-awdl-helper"', workflow)
                self.assertIn('lipo "$app/Contents/Helpers/opennow-awdl-helper" -verify_arch "$OPENNOW_MACOS_ARCH"', workflow)
                self.assertIn('/usr/bin/codesign --verify --strict "$app/Contents/Helpers/opennow-awdl-helper"', workflow)
                self.assertIn('plutil -lint "$app/Contents/Resources/opennow-awdl.plist"', workflow)
                self.assertIn('if name == "../Helpers/opennow-awdl-helper":', workflow)
                self.assertIn("-p opennow-awdl-helper", workflow)

    def test_launch_daemon_has_fixed_root_only_contract(self):
        with (QT_SOURCE / "packaging/opennow-awdl.plist").open("rb") as source:
            plist = plistlib.load(source)
        self.assertEqual(plist["Label"], "io.github.opencloudgaming.OpenNOW.awdl")
        self.assertEqual(plist["ProgramArguments"], [
            "/Library/PrivilegedHelperTools/io.github.opencloudgaming.OpenNOW.awdl-helper",
        ])
        self.assertEqual(plist["UserName"], "root")
        self.assertEqual(plist["GroupName"], "wheel")
        self.assertEqual(plist["Umask"], 0o077)
        self.assertTrue(plist["RunAtLoad"])
        self.assertTrue(plist["KeepAlive"])
        for key in ("Sockets", "EnvironmentVariables", "WorkingDirectory", "Program"):
            self.assertNotIn(key, plist)

    def test_helper_build_and_install_follow_architecture_and_profile(self):
        for arch, target in (("arm64", "aarch64-apple-darwin"),
                             ("x86_64", "x86_64-apple-darwin")):
            with self.subTest(arch=arch):
                result, build = BUILD_TESTS.MacOSBuildContractTest.configure(self, arch=arch)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                rule = (build / "CMakeFiles/opennow-awdl-helper-build.dir/build.make").read_text()
                self.assertIn("--package opennow-awdl-helper --bin opennow-awdl-helper", rule)
                self.assertIn(f"--target {target} --release", rule)
                self.assertIn(f"streamer-rust-target/{target}/release/opennow-awdl-helper", rule)
                self.assertIn("../Helpers/opennow-awdl-helper", rule)
                self.assertIn("../Resources/opennow-awdl.plist", rule)
                self.assertNotIn("--features", rule)
                install = (build / "cmake_install.cmake").read_text()
                self.assertIn("OpenNOW.app/Contents/Helpers", install)
                self.assertIn("opennow-awdl-helper", install)
                self.assertIn("opennow-awdl.plist", install)
                self.assertNotIn("/Library/LaunchDaemons", install)
                arguments = (build / "deploy-args.txt").read_text().split(";")
                self.assertIn("-executable=OpenNOW.app/Contents/Helpers/opennow-awdl-helper", arguments)

    def test_debug_helper_uses_debug_profile(self):
        result, build = BUILD_TESTS.MacOSBuildContractTest.configure(self)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        result = subprocess.run(["cmake", "-S", str(build.parent), "-B", str(build),
                                 "-DCMAKE_BUILD_TYPE=Debug"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        rule = (build / "CMakeFiles/opennow-awdl-helper-build.dir/build.make").read_text()
        self.assertIn("streamer-rust-target/aarch64-apple-darwin/debug/opennow-awdl-helper", rule)
        self.assertNotIn("--release", rule)
        self.assertIn("streamer-rust-target/aarch64-apple-darwin/debug/opennow-awdl-helper",
                      (build / "cmake_install.cmake").read_text())

    def test_adhoc_signing_seals_helper_before_app(self):
        result, build = BUILD_TESTS.MacOSBuildContractTest.configure(self, adhoc=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        seal = (build / "macos-adhoc-seal.cmake").read_text()
        self.assertIn('--identifier "io.github.opencloudgaming.OpenNOW.awdl-helper"', seal)
        self.assertLess(seal.index("Contents/Helpers/opennow-awdl-helper"),
                        seal.index('--identifier "io.github.opencloudgaming.OpenNOW"'))
        self.assertIn("--verify --deep --strict", seal)

    def test_developer_id_signing_includes_helper_without_app_entitlements(self):
        fixture = RELEASE_TESTS.MacOSReleaseTest()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        command = fixture.command

        def install_helper(*args):
            result = command(*args)
            if args[:2] == ("cmake", "--install"):
                helper = Path(args[-1]) / "OpenNOW.app/Contents/Helpers/opennow-awdl-helper"
                helper.write_bytes(bytes.fromhex("cffaedfe") + b"fixture")
            return result

        RELEASE_TESTS.RELEASE.run.side_effect = install_helper
        fixture.package()
        calls = [call for call in fixture.calls
                 if call[:2] == ["codesign", "--force"]
                 and call[-1].endswith("/Contents/Helpers/opennow-awdl-helper")]
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][calls[0].index("--identifier") + 1],
                         "io.github.opencloudgaming.OpenNOW.awdl-helper")
        self.assertIn("runtime", calls[0])
        self.assertNotIn("--entitlements", calls[0])
        self.assertTrue(any(call[0] == "lipo" and call[1] == calls[0][-1]
                            for call in fixture.calls))


if __name__ == "__main__":
    unittest.main()
