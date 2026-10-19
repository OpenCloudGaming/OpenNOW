import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

SPEC = importlib.util.spec_from_file_location("native_libraries", Path(__file__).with_name("prepare-native-libraries.py"))
libraries = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(libraries)


def response(data):
    result = io.BytesIO(data)
    result.headers = {"Content-Type": "application/octet-stream"}
    return result


class DownloadTests(unittest.TestCase):
    def test_rejects_wrong_checksum_then_uses_asset_api(self):
        payload = b"verified archive"
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "libraries.zip"
            with patch.object(libraries, "SHA256", hashlib.sha256(payload).hexdigest()), \
                 patch.object(libraries.urllib.request, "urlopen", side_effect=[response(b"HTML proxy response"), response(payload)]) as fetch:
                libraries.download_archive(archive)
            self.assertEqual(archive.read_bytes(), payload)
            self.assertEqual(fetch.call_args_list[1].args[0].full_url, libraries.ASSET_URL)
            self.assertEqual(list(Path(directory).iterdir()), [archive])

    def test_network_failure_uses_fallback(self):
        payload = b"verified archive"
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "libraries.zip"
            with patch.object(libraries, "SHA256", hashlib.sha256(payload).hexdigest()), \
                 patch.object(libraries.urllib.request, "urlopen", side_effect=[urllib.error.URLError("unavailable"), response(payload)]):
                libraries.download_archive(archive)
            self.assertEqual(archive.read_bytes(), payload)

    def test_never_installs_unverified_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "libraries.zip"
            with patch.object(libraries.urllib.request, "urlopen", side_effect=[response(b"wrong"), response(b"also wrong")]):
                with self.assertRaisesRegex(RuntimeError, "Unable to download checksum-pinned"):
                    libraries.download_archive(archive)
            self.assertEqual(list(Path(directory).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
