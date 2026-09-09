import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SCRIPT = Path(__file__).resolve().parents[2] / ".github/scripts/windows-test-bundle.py"
SPEC = importlib.util.spec_from_file_location("windows_test_bundle", SCRIPT)
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


class WindowsTestBundleTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.build = self.root / "original"
        self.destination = self.root / "restored"
        self.archive = self.root / "bundle.zip"
        self.revision = "0123456789abcdef"
        self.tests = ["embedded-orchestration", "stream-input"]
        self.files = {
            "Release/OpenNOW.exe": b"application\x00",
            "Release/tst_streaminput.exe": b"test executable",
            "Release/Qt6Core.dll": b"runtime library",
            "Release/OpenNOW.pdb": b"debug symbols",
            "Release/platforms/qwindows.dll": b"platform plugin",
            "Release/qml/OpenNOW/qmldir": b"module OpenNOW",
            "Release/resources/shader.bin": b"\x00\xff\x01",
            "CTestTestfile.cmake": (
                b'add_test(stream-input "O:/build/opennow-qt-release/Release/tst_streaminput.exe")\n'
                b'subdirs("nested")\n'
            ),
            "nested/CTestTestfile.cmake": b"# generated nested test configuration\n",
        }
        for name, data in self.files.items():
            self.write(name, data)
        for name in (
            "CMakeCache.txt", "build.ninja", "object.obj", "Debug/OpenNOW.exe",
            "rust-target/CTestTestfile.cmake",
            "streamer-rust-target/nested/CTestTestfile.cmake",
            "nested/target/CTestTestfile.cmake",
        ):
            self.write(name, b"not bundled")
        self.run = patch.object(bundle.subprocess, "run").start()
        self.addCleanup(patch.stopall)
        self.discovery(self.tests)

    def write(self, name, data):
        path = self.build / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def discovery(self, names):
        self.run.return_value = subprocess.CompletedProcess(
            [], 0, stdout=json.dumps({"tests": [{"name": name} for name in names]})
        )

    def pack(self):
        bundle.pack(self.build, self.archive, self.revision)

    def unpack(self):
        bundle.unpack(self.archive, self.destination, self.revision)

    def test_round_trip_preserves_all_runtime_files_and_generated_commands(self):
        self.pack()
        with zipfile.ZipFile(self.archive) as archive:
            self.assertEqual(set(archive.namelist()), set(self.files) | {bundle.MANIFEST})
            manifest = json.loads(archive.read(bundle.MANIFEST))
            self.assertEqual(manifest, {"revision": self.revision, "tests": self.tests})
        self.discovery(list(reversed(self.tests)))
        self.unpack()
        for name, data in self.files.items():
            with self.subTest(name=name):
                self.assertEqual((self.destination / name).read_bytes(), data)
        self.assertEqual(self.run.call_count, 2)
        for call, build in zip(self.run.call_args_list, [self.build, self.destination]):
            self.assertEqual(call.args[0], [
                "ctest", "--test-dir", str(build), "-C", "Release", "--show-only=json-v1",
            ])
            self.assertEqual(call.kwargs, {"check": True, "capture_output": True, "text": True})

    def test_source_mismatch_is_rejected_before_extraction_or_discovery(self):
        self.pack()
        self.run.reset_mock()
        with self.assertRaisesRegex(ValueError, "source revision"):
            bundle.unpack(self.archive, self.destination, "different-revision")
        self.assertFalse(self.destination.exists())
        self.run.assert_not_called()

    def test_empty_discovery_cannot_be_packed(self):
        self.discovery([])
        with self.assertRaisesRegex(ValueError, "no tests"):
            self.pack()
        self.assertFalse(self.archive.exists())

    def test_empty_discovery_after_unpack_is_rejected(self):
        self.pack()
        self.discovery([])
        with self.assertRaisesRegex(ValueError, "no tests"):
            self.unpack()

    def test_empty_manifest_inventory_is_rejected_before_extraction(self):
        with zipfile.ZipFile(self.archive, "w") as archive:
            archive.writestr(bundle.MANIFEST, json.dumps({"revision": self.revision, "tests": []}))
        self.run.reset_mock()
        with self.assertRaisesRegex(ValueError, "inventory is empty"):
            self.unpack()
        self.assertFalse(self.destination.exists())
        self.run.assert_not_called()

    def test_inventory_mismatch_is_rejected(self):
        self.pack()
        for names in (self.tests[:1], self.tests + ["extra"], ["renamed", self.tests[1]]):
            with self.subTest(names=names):
                self.discovery(names)
                with self.assertRaisesRegex(ValueError, "inventory mismatch"):
                    self.unpack()

    def test_unsafe_archive_paths_are_rejected_before_any_extraction(self):
        for name in (
            "../escaped.txt", "Release/../../escaped.txt", "/absolute.txt",
            "C:/absolute.txt", "C:relative.txt", "\\\\server\\share\\file",
            "Release\\..\\escaped.txt", "Release/.. /escaped.txt",
        ):
            with self.subTest(name=name):
                self.pack()
                with zipfile.ZipFile(self.archive, "a") as archive:
                    archive.writestr(name, "unsafe")
                self.run.reset_mock()
                with self.assertRaisesRegex(ValueError, "Unsafe archive path"):
                    self.unpack()
                self.assertFalse(self.destination.exists())
                self.assertFalse((self.root / "escaped.txt").exists())
                self.run.assert_not_called()

    def test_ctest_failure_is_not_ignored(self):
        self.run.side_effect = subprocess.CalledProcessError(1, "ctest")
        with self.assertRaises(subprocess.CalledProcessError):
            self.pack()
        self.assertFalse(self.archive.exists())

    def test_missing_root_ctest_file_is_rejected(self):
        (self.build / "CTestTestfile.cmake").unlink()
        with self.assertRaisesRegex(ValueError, "root CTestTestfile"):
            self.pack()

    def test_missing_runtime_directory_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Release runtime"):
            bundle.pack(self.root / "missing", self.archive, self.revision)


if __name__ == "__main__":
    unittest.main()
