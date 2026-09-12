import base64
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import textwrap
import traceback
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("macos_release", ROOT / "opennow-qt/packaging/macos_release.py")
RELEASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RELEASE)
WORKFLOW = ROOT / ".github/workflows/qt-release-candidate.yml"


class MacOSReleaseTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.output = self.root / "output"
        self.calls = []
        self.copies = {}
        self.fail_at = None
        self.notary_status = "Accepted"
        self.entitlements = {
            "com.apple.security.cs.allow-jit": True,
            "com.apple.security.device.audio-input": True,
        }
        values = {name: "fixture" for name in RELEASE.SECRET_NAMES}
        values.update({
            "RUNNER_TEMP": str(self.root),
            "OPENNOW_MACOS_SIGN_IDENTITY": "Developer ID Application: Fixture (ABCDEFGHIJ)",
            "OPENNOW_MACOS_DEVELOPER_ID_P12_BASE64": base64.b64encode(b"test certificate").decode(),
            "OPENNOW_APPLE_API_KEY_BASE64": base64.b64encode(b"test key").decode(),
        })
        self.addCleanup(patch.stopall)
        output = contextlib.redirect_stdout(io.StringIO())
        output.__enter__()
        self.addCleanup(output.__exit__, None, None, None)
        patch.dict(os.environ, values).start()
        patch.object(RELEASE, "run", side_effect=self.command).start()

    def command(self, *args):
        args = [str(arg) for arg in args]
        self.calls.append(args)
        if self.fail_at == len(self.calls):
            raise RuntimeError("injected command failure")
        if args[:4] == ["security", "list-keychains", "-d", "user"] and "-s" not in args:
            return '"/Users/fixture/Library/Keychains/login.keychain-db"\n'
        if args[:2] == ["security", "create-keychain"]:
            Path(args[-1]).touch()
        elif args[:2] == ["security", "delete-keychain"]:
            Path(args[-1]).unlink(missing_ok=True)
        elif args[:2] == ["cmake", "--install"]:
            app = Path(args[-1]) / "OpenNOW.app"
            (app / "Contents/MacOS").mkdir(parents=True)
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({"CFBundleIdentifier": RELEASE.BUNDLE_ID}))
            for relative in ("MacOS/OpenNOW", "MacOS/opennow-core", "MacOS/opennow-update-helper",
                             "MacOS/libopennow_streamer_ffi.dylib", "PlugIns/platforms/libqcocoa.dylib",
                             "Frameworks/QtCore.framework/Versions/A/QtCore",
                             "Helpers/Nested.app/Contents/MacOS/Nested"):
                path = app / "Contents" / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(bytes.fromhex("cffaedfe") + b"fixture")
            framework = app / "Contents/Frameworks/QtCore.framework"
            (framework / "QtCore").symlink_to("Versions/A/QtCore")
        elif args[:3] == ["ditto", "-c", "-k"]:
            Path(args[-1]).write_bytes(b"zip fixture")
            copied = self.root / f"copy-{len(self.copies)}"
            shutil.copytree(args[-2], copied, symlinks=True)
            self.copies[args[-1]] = copied
        elif args[:3] == ["ditto", "-x", "-k"]:
            shutil.copytree(self.copies[args[-2]], Path(args[-1]) / "OpenNOW.app", symlinks=True)
        elif args[0] == "ditto":
            shutil.copytree(args[-2], args[-1], symlinks=True)
        elif args[:2] == ["hdiutil", "create"]:
            Path(args[-1]).write_bytes(b"dmg fixture")
            self.copies[args[-1]] = Path(args[args.index("-srcfolder") + 1]) / "OpenNOW.app"
        elif args[:2] == ["hdiutil", "attach"]:
            shutil.copytree(self.copies[args[2]], Path(args[-1]) / "OpenNOW.app", symlinks=True)
        elif args[:3] == ["xcrun", "notarytool", "submit"]:
            return json.dumps({"status": self.notary_status})
        elif args[:3] == ["xcrun", "stapler", "staple"] and args[-1].endswith(".app"):
            (Path(args[-1]) / "ticket").touch()
        elif args[:3] == ["xcrun", "stapler", "validate"] and args[-1].endswith(".app"):
            self.assertTrue((Path(args[-1]) / "ticket").is_file(), "ZIP/DMG must retain app ticket")
        elif args[:3] == ["codesign", "--display", "--entitlements"]:
            return plistlib.dumps(self.entitlements).decode()
        return ""

    def package(self):
        RELEASE.package(self.root / "build", self.output, "1.2.3")

    def test_inside_out_signing_precedes_notarization_and_final_packaging(self):
        self.package()
        signed = [call for call in self.calls if call[:2] == ["codesign", "--force"]]
        self.assertTrue(signed)
        for call in signed:
            self.assertNotIn("--deep", call)
            self.assertIn("--timestamp", call)
            if not call[-1].endswith(".dmg"):
                self.assertIn("runtime", call)
        paths = [Path(call[-1]) for call in signed[:-1]]
        for index, path in enumerate(paths):
            for descendant in paths[index + 1:]:
                self.assertNotIn(path, descendant.parents)
        self.assertEqual(paths[-1].name, "OpenNOW.app")
        self.assertNotIn("/Frameworks/QtCore.framework/QtCore", "\n".join(str(path) for path in paths))
        install = next(i for i, call in enumerate(self.calls) if call[0] == "cmake")
        self.assertIn("--strip", self.calls[install])
        self.assertLess(install, self.calls.index(signed[0]))
        app_staple = next(i for i, call in enumerate(self.calls)
                          if call[:3] == ["xcrun", "stapler", "staple"] and call[-1].endswith(".app"))
        final_zip = next(i for i, call in enumerate(self.calls)
                         if call[:3] == ["ditto", "-c", "-k"] and call[-1].endswith("Darwin-arm64.zip"))
        self.assertLess(app_staple, final_zip)
        notaries = [call for call in self.calls if call[:3] == ["xcrun", "notarytool", "submit"]]
        self.assertEqual(len(notaries), 2)
        self.assertTrue(notaries[0][3].endswith("notarization.zip"))
        self.assertTrue(notaries[1][3].endswith("Darwin-arm64.dmg"))
        self.assertEqual(len([call for call in self.calls if call[0] == "spctl"]), 4)
        self.assertEqual({path.name for path in self.output.iterdir()},
                         {"OpenNOW-Qt-1.2.3-Darwin-arm64.zip", "OpenNOW-Qt-1.2.3-Darwin-arm64.dmg"})
        self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))

    def test_only_root_app_receives_exact_jit_and_microphone_entitlements(self):
        with RELEASE.ENTITLEMENTS.open("rb") as source:
            self.assertEqual(plistlib.load(source), self.entitlements)
        self.package()
        signed = [call for call in self.calls if call[:2] == ["codesign", "--force"]]
        for call in signed:
            with self.subTest(target=call[-1]):
                if Path(call[-1]).name == "OpenNOW.app":
                    self.assertEqual(call[call.index("--entitlements") + 1], str(RELEASE.ENTITLEMENTS))
                else:
                    self.assertNotIn("--entitlements", call)
        inspected = [call[-1] for call in self.calls if call[:3] == ["codesign", "--display", "--entitlements"]]
        self.assertEqual(len(inspected), 3)
        for location in ("stage", "zip-extracted", "dmg-extracted"):
            self.assertTrue(any(f"/{location}/OpenNOW.app/Contents/MacOS/OpenNOW" in path for path in inspected))

    def test_missing_or_broadened_signed_entitlements_prevent_promotion(self):
        expected = self.entitlements.copy()
        for changed in ({}, {**expected, "com.apple.security.get-task-allow": True},
                        {**expected, "com.apple.security.cs.disable-library-validation": True}):
            with self.subTest(entitlements=changed):
                self.entitlements = changed
                with self.assertRaisesRegex(ValueError, "entitlements"):
                    self.package()
                self.assertFalse(list(self.output.iterdir()))
                self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))

    def test_every_external_failure_prevents_candidate_promotion_and_cleans_secrets(self):
        self.package()
        total = len(self.calls)
        shutil.rmtree(self.output)
        for failure in range(1, total + 1):
            with self.subTest(command=failure):
                self.calls.clear()
                self.fail_at = failure
                with self.assertRaises(RuntimeError):
                    self.package()
                self.assertFalse(list(self.output.iterdir()))
                self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))
                if failure > 1:
                    self.assertTrue(any(call[:2] == ["security", "delete-keychain"] for call in self.calls))
                    self.assertEqual(self.calls[-1][:5], ["security", "list-keychains", "-d", "user", "-s"])

    def test_notary_rejection_is_fatal_even_with_zero_process_exit(self):
        self.notary_status = "Invalid"
        with self.assertRaisesRegex(RuntimeError, "Accepted"):
            self.package()
        self.assertFalse(list(self.output.iterdir()))
        self.assertFalse(any(call[:3] == ["xcrun", "stapler", "staple"] for call in self.calls))
        self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))

    def test_signal_unwinds_cleanup_before_publishing(self):
        original = self.command

        def interrupt(*args):
            if args[:2] == ("codesign", "--force"):
                RELEASE.interrupted(15, None)
            return original(*args)

        RELEASE.run.side_effect = interrupt
        with self.assertRaises(SystemExit):
            self.package()
        self.assertFalse(list(self.output.iterdir()))
        self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))
        self.assertTrue(any(call[:2] == ["security", "delete-keychain"] for call in self.calls))

    def test_invalid_credentials_fail_before_apple_commands(self):
        for name, value in (("OPENNOW_MACOS_SIGN_IDENTITY", "-"),
                            ("OPENNOW_APPLE_API_KEY_BASE64", "!invalid")):
            with self.subTest(secret=name), patch.dict(os.environ, {name: value}):
                with self.assertRaises(ValueError):
                    self.package()
                self.assertEqual(self.calls, [])
                self.assertFalse(list(self.root.glob("opennow-macos-signing-*")))


class MacOSSigningCommandTest(unittest.TestCase):
    def test_timeout_does_not_expose_command_arguments_or_captured_secrets(self):
        password = "fixture-private-password"
        args = ["security", "import", "certificate.p12", "-P", password]
        timeout = subprocess.TimeoutExpired(args, 2400, output=password, stderr=password)
        with patch.object(RELEASE.subprocess, "run", side_effect=timeout):
            try:
                RELEASE.run(*args)
            except RuntimeError:
                error = traceback.format_exc()
            else:
                self.fail("Timeout must fail closed")
        self.assertIn("security import timed out", error)
        self.assertNotIn(password, error)

    def test_failed_commands_do_not_log_secrets_or_inherit_signing_environment(self):
        password = "fixture-private-password"
        result = subprocess.CompletedProcess([], 1, password, password)
        with patch.dict(os.environ, {name: password for name in RELEASE.SECRET_NAMES}), \
                patch.object(RELEASE.subprocess, "run", return_value=result) as command:
            with self.assertRaisesRegex(RuntimeError, "security import failed") as error:
                RELEASE.run("security", "import", "certificate.p12", "-P", password)
        self.assertNotIn(password, str(error.exception))
        self.assertFalse(set(RELEASE.SECRET_NAMES) & command.call_args.kwargs["env"].keys())


class MacOSCandidateWorkflowTest(unittest.TestCase):
    def inventory(self, names):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, name in enumerate(names):
                path = root / "candidate" / str(index) / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"fixture")
            step = WORKFLOW.read_text().split("      - name: Verify complete platform artifact set and produce inventory\n", 1)[1]
            script = textwrap.dedent(step.split("        run: |\n", 1)[1].split("          signing_dir=", 1)[0])
            return subprocess.run(["bash", "-euo", "pipefail", "-c", script], cwd=root,
                                  env={**os.environ, "RELEASE_VERSION": "1.2.3"}, capture_output=True, text=True)

    def names(self):
        return [f"OpenNOW-Qt-1.2.3-{platform}-{arch}.{extension}"
                for platform, arches, extensions in (
                    ("Linux", ("x64", "arm64"), ("AppImage", "deb")),
                    ("Windows", ("x64", "arm64"), ("msi", "zip")),
                    ("Darwin", ("arm64",), ("dmg", "zip")))
                for arch in arches for extension in extensions]

    @unittest.skipUnless(sys.platform.startswith("linux"), "Inventory uses GNU find on the Linux signing runner")
    def test_exact_ten_platform_artifacts_are_required(self):
        names = self.names()
        result = self.inventory(names)
        self.assertEqual(result.returncode, 0, result.stderr)
        for index in range(len(names)):
            with self.subTest(missing=names[index]):
                self.assertNotEqual(self.inventory(names[:index] + names[index + 1:]).returncode, 0)
        for changed in (names[:-1] + [names[0]],
                        [name.replace("Darwin-arm64", "Darwin-x64") for name in names],
                        [name.replace("1.2.3", "1.2.4") for name in names],
                        names + ["unexpected.txt"], names + [names[0] + ".manifest.json"]):
            with self.subTest(names=changed):
                self.assertNotEqual(self.inventory(changed).returncode, 0)

    def test_production_candidate_is_native_protected_and_never_published(self):
        workflow = WORKFLOW.read_text()
        macos = workflow.split("  macos:\n", 1)[1].split("  inventory:\n", 1)[0]
        self.assertIn("needs: [preflight, linux, windows, macos]", workflow)
        for required in ("blacksmith-6vcpu-macos-15", "environment: qt-production-release",
                         "CMAKE_OSX_ARCHITECTURES=arm64",
                         "OPENNOW_UPDATE_ED25519_PUBLIC_KEY=", "--locked", "ctest --test-dir",
                         'mv "$QT_ROOT_DIR" "$QT_ROOT_DIR.unavailable"',
                         '"--smoke-test"', "macos_release.py"):
            self.assertIn(required, macos)
        self.assertNotIn("cpack ", macos)
        self.assertNotIn("OPENNOW_MACOS_ADHOC_SIGN", macos)
        self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", macos)
        self.assertNotIn("contents: write", workflow)
        self.assertNotIn("gh release", workflow)
        self.assertNotIn("softprops/action-gh-release", workflow)
        for name in RELEASE.SECRET_NAMES:
            self.assertIn("secrets." + name, macos)


if __name__ == "__main__":
    unittest.main()
