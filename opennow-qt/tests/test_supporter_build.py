import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import test_release_metadata


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "opennow-qt/packaging/nightly_release.py"
sys.path.insert(0, str(SCRIPT.parent))
from nightly_release import assemble, nightly_version


class SupporterBuildTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.version = "1.0.0-supporter.123.2"
        self.commit = "a" * 40

    def test_supporter_identity_uses_project_and_run_numbers(self):
        project = self.root / "CMakeLists.txt"
        project.write_text("project(OpenNOWQt VERSION 1.0.0 LANGUAGES CXX)")
        self.assertEqual(nightly_version(project, 123, 2, "supporter"), self.version)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "version", "--cmake-file", str(project),
             "--run", "123", "--attempt", "2", "--channel", "supporter"],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.version)
        for run, attempt in ((0, 1), (1, 0), (-1, 1)):
            with self.subTest(run=run, attempt=attempt), self.assertRaises(ValueError):
                nightly_version(project, run, attempt, "supporter")
        with self.assertRaisesRegex(ValueError, "channel"):
            nightly_version(project, 123, 2, "release")

    def test_supporter_inventory_and_checksums(self):
        source = self.root / "artifacts"
        destination = self.root / "complete"
        for arch in ("x64", "arm64"):
            for platform, extension in (("Windows", "zip"), ("Linux", "AppImage"), ("Linux", "deb")):
                package = source / arch / f"OpenNOW-Qt-{self.version}-{platform}-{arch}.{extension}"
                package.parent.mkdir(parents=True, exist_ok=True)
                package.write_bytes(f"fixture {platform} {arch}".encode())
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "assemble", "--source", str(source),
             "--destination", str(destination), "--version", self.version,
             "--commit", self.commit, "--channel", "supporter"],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        metadata = json.loads((destination / "RELEASE-INFO.json").read_text())
        self.assertEqual(metadata["version"], self.version)
        self.assertEqual(metadata["sourceCommit"], self.commit)
        self.assertEqual(metadata["platformSigning"], "unsigned")
        self.assertEqual(metadata["updates"], "manual-download")
        self.assertEqual(len(metadata["assets"]), 6)
        sums = (destination / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(sums), 7)
        for line in sums:
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((destination / name).read_bytes()).hexdigest())

    def test_inventory_rejects_wrong_channel_and_malformed_versions(self):
        for version, channel in (
            (self.version, "nightly"),
            ("1.0.0-nightly.123.2", "supporter"),
            ("1.0.0-supporter.01.1", "supporter"),
            ("1.0.0-supporter.1.0", "supporter"),
            (self.version, "release"),
        ):
            with self.subTest(version=version, channel=channel), self.assertRaises(ValueError):
                assemble(self.root, self.root / "output", version, self.commit, channel)
        self.assertFalse((self.root / "output").exists())

    def test_cmake_preserves_supporter_identity_and_debian_ordering(self):
        result = test_release_metadata.BuildMetadataTest().metadata(OPENNOW_BUILD_VERSION=self.version)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            f"{self.version}|1.0.0|1.0.0~supporter.123.2|OpenNOW-Qt-{self.version}-Linux-x64|amd64",
            result.stderr,
        )
        for version in ("1.0.0-supporter", "1.0.0-supporter.01.1", "1.0.0-supporter.1.0"):
            with self.subTest(version=version):
                self.assertNotEqual(
                    test_release_metadata.BuildMetadataTest().metadata(OPENNOW_BUILD_VERSION=version).returncode, 0,
                )

    def test_supporter_workflow_is_removed_and_shared_build_has_no_publishing_path(self):
        workflows = ROOT / ".github/workflows"
        self.assertFalse((workflows / "qt-supporter-build.yml").exists())
        build = (workflows / "qt-build.yml").read_text()
        ci = (workflows / "qt-ci.yml").read_text()
        for workflow in (ci, build):
            self.assertNotIn("qt-supporter-build.yml", workflow)
        self.assertIn("permissions:\n  contents: read", build)
        for forbidden in ("contents: write", "gh release", "git tag", "publish_nightly", "secrets:", "secrets."):
            self.assertNotIn(forbidden, build)
        self.assertIn("if: inputs.upload_complete", build)
        self.assertIn("retention-days: 14", build)
        self.assertIn("--channel \"$BUILD_CHANNEL\"", build)
        self.assertIn("uses: ./.github/workflows/qt-build.yml", ci)
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.publish_nightly", ci)
        self.assertIn("needs: [contracts, checks, build]", ci)
        self.assertIn("gh release create", ci)


if __name__ == "__main__":
    unittest.main()
