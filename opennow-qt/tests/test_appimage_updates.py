import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "opennow-qt/packaging"))
from appimage_updates import update_information, verify, verify_zsync


class AppImageUpdatesTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.appimage = Path(directory.name) / "OpenNOW-Qt-1.2.3-Linux-x64.AppImage"
        self.appimage.write_bytes(b"AppImage fixture")
        self.sidecar = self.appimage.with_name(self.appimage.name + ".zsync")
        self.headers = (f"zsync: 0.6.2\nFilename: {self.appimage.name}\n"
                        f"URL: {self.appimage.name}\nLength: {self.appimage.stat().st_size}\n"
                        f"SHA-1: {hashlib.sha1(self.appimage.read_bytes()).hexdigest()}\n\n").encode()
        self.sidecar.write_bytes(self.headers + b"block checksums")

    def test_channel_and_architecture_are_explicit(self):
        for arch in ("x64", "arm64"):
            stable = update_information("stable", arch)
            nightly = update_information("nightly", arch)
            self.assertEqual(stable, "gh-releases-zsync|OpenCloudGaming|OpenNOW|latest|"
                             f"OpenNOW-Qt-*-Linux-{arch}.AppImage.zsync")
            self.assertEqual(nightly, "gh-releases-zsync|OpenCloudGaming|OpenNOW|latest-pre|"
                             f"OpenNOW-Qt-*-nightly.*-Linux-{arch}.AppImage.zsync")
        for channel, arch in (("beta", "x64"), ("stable", "x86_64")):
            with self.assertRaises(ValueError):
                update_information(channel, arch)

    def test_checks_embedded_information_and_matching_sidecar(self):
        with patch("appimage_updates.subprocess.run") as run:
            run.return_value = subprocess.CompletedProcess([], 0, update_information("stable", "x64") + "\n")
            verify(self.appimage, "stable", "x64")
            self.assertEqual(run.call_args.args[0][-1], "--appimage-updateinformation")
            self.assertNotIn("APPIMAGE_EXTRACT_AND_RUN", run.call_args.kwargs["env"])
            for incorrect in ("", update_information("nightly", "x64"), update_information("stable", "arm64")):
                run.return_value.stdout = incorrect
                with self.assertRaisesRegex(ValueError, "embedded update information"):
                    verify(self.appimage, "stable", "x64")

    def test_rejects_missing_truncated_stale_or_misnamed_sidecars(self):
        verify_zsync(self.appimage)
        for content in (b"", self.headers, self.headers[:-1],
                        self.headers.replace(b"1.2.3", b"1.2.4") + b"checksums",
                        self.headers.replace(b"Length: 16", b"Length: 17") + b"checksums",
                        self.headers.replace(b"URL: ", b"URL: https://evil.example/") + b"checksums"):
            self.sidecar.write_bytes(content)
            with self.assertRaises(ValueError):
                verify_zsync(self.appimage)
        self.sidecar.write_bytes(self.headers + b"checksums")
        self.appimage.write_bytes(b"Changed after zsync generation")
        with self.assertRaises(ValueError):
            verify_zsync(self.appimage)
        self.sidecar.unlink()
        with self.assertRaises(ValueError):
            verify_zsync(self.appimage)

    def test_every_packaging_path_embeds_verifies_and_uploads_sidecars(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            workflow = (ROOT / ".github/workflows" / name).read_text()
            self.assertIn("LDAI_UPDATE_INFORMATION=$(python3 opennow-qt/packaging/appimage_updates.py information", workflow)
            self.assertIn("appimage_updates.py verify", workflow)
            self.assertIn("OpenNOW-Qt-*.AppImage.zsync", workflow)
            self.assertIn("/*.AppImage.zsync", workflow)

    def test_publication_workflows_use_the_existing_production_key(self):
        key = (ROOT / "opennow-qt/packaging/update-public-key.base64").read_text().strip()
        self.assertEqual(key, "HoyKVmfuH+KDioPCJJNxOB2e/bXJoOALSjo1QeMsDlk=")
        for name in ("qt-ci.yml", "qt-release-candidate.yml"):
            workflow = (ROOT / ".github/workflows" / name).read_text()
            self.assertIn(key, workflow)
            self.assertIn("sign_nightly_release.py preflight", workflow)
        promotion = (ROOT / "opennow-qt/packaging/promote_candidate.py").read_text()
        self.assertIn('with_name("update-public-key.base64")', promotion)


if __name__ == "__main__":
    unittest.main()
