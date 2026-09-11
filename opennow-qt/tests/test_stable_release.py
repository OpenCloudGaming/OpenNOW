import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "opennow-qt/packaging"))
from nightly_release import assemble, expected_packages, nightly_version


class StableReleaseTest(unittest.TestCase):
    def test_stable_version_uses_project_version_without_nightly_suffix(self):
        self.assertEqual(nightly_version(ROOT / "opennow-qt/CMakeLists.txt", 1, 1, "stable"), "1.0.0")

    def test_channels_cannot_relabel_each_others_versions(self):
        for version, channel in (("1.0.0-nightly.1.1", "stable"), ("1.0.0", "nightly"),
                                 ("1.0.0-supporter.1.1", "stable"), ("01.0.0", "stable"),
                                 ("1.0.0\n", "stable")):
            with self.subTest(version=version, channel=channel):
                with self.assertRaises(ValueError):
                    expected_packages(version, "a" * 40, channel)

    def test_complete_stable_inventory_is_unsigned_with_manual_updates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            names = expected_packages("1.0.0", "a" * 40, "stable")
            self.assertEqual(len(names), 9)
            self.assertIn("OpenNOW-Qt-1.0.0-Darwin-arm64.dmg", names)
            for name in names:
                (source / name).write_bytes(name.encode())
            destination = root / "release"
            assemble(source, destination, "1.0.0", "a" * 40, "stable")
            metadata = json.loads((destination / "RELEASE-INFO.json").read_text())
            self.assertEqual(metadata["version"], "1.0.0")
            self.assertEqual(metadata["sourceCommit"], "a" * 40)
            self.assertEqual(metadata["platformSigning"], "unsigned")
            self.assertEqual(metadata["updates"], "manual-download")
            for line in (destination / "SHA256SUMS").read_text().splitlines():
                digest, name = line.split("  ")
                self.assertEqual(hashlib.sha256((destination / name).read_bytes()).hexdigest(), digest)
            (source / next(iter(names))).unlink()
            with self.assertRaisesRegex(ValueError, "Missing release artifacts"):
                assemble(source, root / "incomplete", "1.0.0", "a" * 40, "stable")

    def test_publication_requires_main_checks_and_complete_packages(self):
        workflow = (ROOT / ".github/workflows/qt-stable-release.yml").read_text()
        for required in ('[[ "$GITHUB_REF" == refs/heads/main ]]',
                         '[[ "$SOURCE_COMMIT" == "$GITHUB_SHA" ]]',
                         "needs: [preflight, contracts, checks, build]",
                         "channel: stable", 'update_public_key: ""',
                         "sha256sum --check --strict SHA256SUMS",
                         "cmp downloaded/RELEASE-INFO.json release/RELEASE-INFO.json",
                         "cmp downloaded/SHA256SUMS release/SHA256SUMS",
                         "--draft --latest=false", "--draft=false --prerelease=false --latest"):
            self.assertIn(required, workflow)
        self.assertNotIn("continue-on-error", workflow)
        self.assertNotIn("secrets.", workflow)
        self.assertNotIn("always()", workflow)


if __name__ == "__main__":
    unittest.main()
