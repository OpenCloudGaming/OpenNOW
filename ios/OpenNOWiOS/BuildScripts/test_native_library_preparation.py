import importlib.util
from pathlib import Path
import plistlib
import shutil
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("native_libraries", Path(__file__).with_name("prepare-native-libraries.py"))
libraries = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(libraries)
FRAMEWORKS = Path("ios/OpenNOWiOS/Frameworks")
ARCHIVE = Path(__file__).resolve().parents[1] / "Frameworks" / libraries.ARCHIVE


class PreparationTests(unittest.TestCase):
    def bundle(self, root):
        archive = root / FRAMEWORKS / libraries.ARCHIVE
        archive.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ARCHIVE, archive)
        return archive

    def assert_frameworks(self, root):
        for name in ["OpenSSL", "usrsctp"]:
            framework = root / FRAMEWORKS / f"{name}.xcframework"
            manifest = plistlib.loads((framework / "Info.plist").read_bytes())
            slices = manifest["AvailableLibraries"]
            self.assertEqual({item["LibraryIdentifier"] for item in slices},
                             {"ios-arm64", "ios-arm64-simulator"})
            for item in slices:
                binary = framework / item["LibraryIdentifier"] / item["LibraryPath"] / name
                self.assertGreater(binary.stat().st_size, 0)

    def test_clean_checkout_prepares_device_and_simulator_without_network(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.bundle(root)
            with patch("socket.socket", side_effect=AssertionError("Preparation must stay offline")):
                libraries.prepare(root)
            self.assert_frameworks(root)
            self.assertEqual(list((root / "Build/native-library-cache").iterdir()), [])

    def test_old_download_cache_is_not_used(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.bundle(root)
            cache = root / "Build/native-library-cache" / libraries.ARCHIVE
            cache.parent.mkdir(parents=True)
            cache.write_bytes(b"truncated old download")
            libraries.prepare(root)
            self.assert_frameworks(root)
            self.assertEqual(cache.read_bytes(), b"truncated old download")

    def test_corrupt_bundle_is_rejected_before_replacing_frameworks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.bundle(root).write_bytes(b"corrupted repository archive")
            marker = root / FRAMEWORKS / "OpenSSL.xcframework" / "existing-library"
            marker.parent.mkdir()
            marker.write_bytes(b"keep")
            with self.assertRaisesRegex(RuntimeError, "Bundled native library archive checksum mismatch"):
                libraries.prepare(root)
            self.assertEqual(marker.read_bytes(), b"keep")

    def test_missing_bundle_reports_checkout_error(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "Missing bundled native library archive"):
                libraries.prepare(Path(directory))


if __name__ == "__main__":
    unittest.main()
