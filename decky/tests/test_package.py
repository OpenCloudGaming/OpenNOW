import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from tests import support
from opennow_decky import installer
import package


class PackageTests(unittest.TestCase):
    def test_source_and_packaged_key_disagreement_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module = root / "decky/py_modules/opennow_decky/installer.py"
            module.parent.mkdir(parents=True)
            source = root / "opennow-qt/packaging/update-public-key.base64"
            source.parent.mkdir(parents=True)
            source.write_bytes(package.PUBLIC_KEY.read_bytes())
            module.with_name("update-public-key.base64").write_text("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
            with mock.patch.object(installer, "__file__", str(module)), \
                    self.assertRaisesRegex(installer.InstallError, "differ"):
                installer.pinned_public_key()

    def test_packaging_uses_the_authoritative_key_and_identical_verifier_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "decky"
            root.mkdir()
            for name in package.FILES:
                shutil.copyfile(support.ROOT / name, root / name)
            shutil.copytree(support.ROOT / "py_modules", root / "py_modules", ignore=shutil.ignore_patterns("__pycache__"))
            (root / "dist").mkdir()
            (root / "dist/index.js").write_text("export {};")
            with mock.patch.object(package, "ROOT", root):
                output = package.package()
            with zipfile.ZipFile(output) as archive:
                key = archive.read("OpenNOW/py_modules/opennow_decky/update-public-key.base64")
                self.assertEqual(key, package.PUBLIC_KEY.read_bytes())
                for source in (support.ROOT / "py_modules").rglob("*.py"):
                    self.assertEqual(archive.read("OpenNOW/" + str(source.relative_to(support.ROOT))), source.read_bytes())
                archive.extractall(Path(directory) / "installed")
            environment = dict(os.environ, PYTHONPATH=str(Path(directory) / "installed/OpenNOW/py_modules"))
            result = subprocess.run([sys.executable, "-c", "from opennow_decky.installer import pinned_public_key; "
                                     "import base64; print(base64.b64encode(pinned_public_key()).decode())"],
                                    cwd=directory, capture_output=True, text=True, timeout=15, check=True, env=environment)
            self.assertEqual(result.stdout.strip(), key.decode().strip())
