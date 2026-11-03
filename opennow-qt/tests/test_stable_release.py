import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "opennow-qt/packaging"))
from nightly_release import assemble, expected_packages, nightly_version


class StableReleaseTest(unittest.TestCase):
    def test_generated_release_notes_describe_both_signed_macos_dmgs(self):
        workflow = (ROOT / ".github/workflows/qt-stable-release.yml").read_text()
        step = workflow.split("      - name: Prepare public release notes\n", 1)[1].split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", script], cwd=directory,
                env={**os.environ, "RELEASE_VERSION": "1.2.3", "SOURCE_SHA": "a" * 40,
                     "CANDIDATE_RUN_ID": "123"}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            notes = (Path(directory) / "release-notes.md").read_text()
        for required in ("macOS 13+", "Developer ID-signed and notarized", "Apple Silicon (arm64)",
                         "Intel (x64)", "OpenNOW-Qt-1.2.3-Darwin-arm64.dmg",
                         "OpenNOW-Qt-1.2.3-Darwin-x64.dmg"):
            self.assertIn(required, notes)
        self.assertNotIn("Intel macOS is not included", notes)

    def test_stable_version_uses_project_version_without_nightly_suffix(self):
        cmake = ROOT / "opennow-qt/CMakeLists.txt"
        self.assertEqual(nightly_version(cmake, 1, 1, "stable"), "1.0.4")
        self.assertEqual(nightly_version(cmake, 620, 1), "1.0.4-nightly.620.1")

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
            self.assertEqual(len(names), 14)
            self.assertIn("OpenNOW-Qt-1.0.0-Linux-x64.flatpak", names)
            self.assertIn("OpenNOW-Qt-1.0.0-Linux-arm64.flatpak", names)
            self.assertIn("OpenNOW-Qt-1.0.0-Darwin-arm64.dmg", names)
            self.assertIn("OpenNOW-Qt-1.0.0-Darwin-x64.dmg", names)
            self.assertIn("OpenNOW-Qt-1.0.0-Windows-x64-setup.exe", names)
            self.assertIn("OpenNOW-Qt-1.0.0-Windows-arm64-setup.exe", names)
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
                         "needs: [preflight, contracts, checks]",
                         "--channel stable", "candidate_run_id:",
                         "sha256sum --check --strict SHA256SUMS",
                         "promote_candidate.py provenance", "promote_candidate.py assemble",
                         "--draft --latest=false", "--draft=false --prerelease=false --latest"):
            self.assertIn(required, workflow)
        self.assertNotIn("continue-on-error", workflow)
        self.assertIn("uses: ./.github/actions/qt-unit-tests", workflow)
        for label in ("linux-x64", "windows-x64", "macos-arm64"):
            self.assertIn(f"label: {label}", workflow)
        publisher = workflow.split("  publish:\n", 1)[1]
        self.assertIn("environment: qt-production-release", publisher)
        self.assertIn("persist-credentials: false", publisher)
        self.assertNotIn("secrets.", workflow)
        self.assertNotIn("qt-build.yml", workflow)
        self.assertNotIn("complete-unsigned", publisher)
        self.assertIn("complete-candidate", publisher)
        self.assertIn("run-id: ${{ inputs.candidate_run_id }}", publisher)
        self.assertLess(publisher.index("promote_candidate.py provenance"), publisher.index("actions/download-artifact"))
        self.assertLess(publisher.index("promote_candidate.py assemble"), publisher.index("gh release create"))
        self.assertNotIn("Updates require a manual download", publisher)


if __name__ == "__main__":
    unittest.main()
