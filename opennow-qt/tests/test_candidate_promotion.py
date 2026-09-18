import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "opennow-qt/packaging"))
from nightly_release import expected_packages
from promote_candidate import assemble, validate_provenance
from sign_nightly_release import digest, verify_manifests


class CandidateProvenanceTest(unittest.TestCase):
    def test_requires_exact_successful_production_workflow_revision_and_repository(self):
        run = {"id": 123, "path": ".github/workflows/qt-release-candidate.yml",
               "event": "workflow_dispatch", "head_sha": "a" * 40, "status": "completed",
               "conclusion": "success", "repository": {"full_name": "OpenCloudGaming/OpenNOW"},
               "head_repository": {"full_name": "OpenCloudGaming/OpenNOW"}}
        validate_provenance(run, "123", "opencloudgaming/opennow", "a" * 40)
        for field, value in (("id", 124), ("path", ".github/workflows/qt-build.yml"),
                             ("event", "pull_request"), ("head_sha", "b" * 40),
                             ("status", "in_progress"), ("conclusion", "failure"),
                             ("repository", {"full_name": "other/repository"}),
                             ("head_repository", {"full_name": "fork/OpenNOW"})):
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_provenance({**run, field: value}, "123", "OpenCloudGaming/OpenNOW", "a" * 40)


@unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("jq"), "Requires Linux candidate signing tools")
class CandidatePromotionTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.source = self.root / "candidate"
        self.destination = self.root / "release"
        packages = self.source / "packages"
        packages.mkdir(parents=True)
        self.version, self.commit = "1.2.3", "a" * 40
        names = expected_packages(self.version, self.commit, "stable") | {"OpenNOW-Qt-1.2.3-Darwin-arm64.zip"}
        for name in names:
            (packages / name).write_bytes(name.encode())
        (packages / "SHA256SUMS").write_text("".join(
            f"{digest(path)}  {path.name}\n" for path in sorted(packages.iterdir())))
        seed = os.urandom(32)
        private = self.root / "private.der"
        private.write_bytes(bytes.fromhex("302e020100300506032b657004220420") + seed)
        public = subprocess.check_output(["openssl", "pkey", "-inform", "DER", "-in", str(private),
                                          "-pubout", "-outform", "DER"])
        self.public = base64.b64encode(public[-32:]).decode()
        private.unlink()
        workflow = (ROOT / ".github/workflows/qt-release-candidate.yml").read_text()
        step = workflow.split("      - name: Verify complete platform artifact set and produce inventory\n", 1)[1]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1].split("      - uses:", 1)[0])
        result = subprocess.run(["bash", "-euo", "pipefail", "-c", script], cwd=self.root,
                                env={**os.environ, "RELEASE_VERSION": self.version, "SOURCE_SHA": self.commit,
                                     "WINDOWS_SIGNING_MODE": "unsigned", "RUNNER_TEMP": str(self.root),
                                     "UPDATE_PUBLIC_KEY": self.public,
                                     "UPDATE_PRIVATE_KEY": base64.b64encode(seed).decode()}, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def promote(self):
        assemble(self.source, self.destination, self.version, self.commit, self.public)

    def rehash(self):
        inventory = self.source / "RELEASE-CANDIDATE-INVENTORY.txt"
        header = inventory.read_text().splitlines()[:4]
        sums = [f"{digest(path)}  candidate/{path.relative_to(self.source).as_posix()}"
                for path in sorted(self.source.rglob("*")) if path.is_file() and path != inventory]
        inventory.write_text("\n".join(header + sums) + "\n")

    def test_promotes_exact_signed_candidate_bytes_and_preserves_platform_identity(self):
        self.promote()
        info = json.loads((self.destination / "RELEASE-INFO.json").read_text())
        self.assertEqual(info["platformSigning"], "macos-developer-id")
        self.assertEqual(info["windowsSigningMode"], "unsigned")
        self.assertEqual(info["updates"], "signed-manifest")
        self.assertEqual(len(info["assets"]), 12)
        self.assertEqual(len(list(self.destination.iterdir())), 26)
        verify_manifests(self.destination, self.version, info["assets"], self.public)
        for asset in info["assets"]:
            self.assertEqual((self.source / "packages" / asset["name"]).read_bytes(),
                             (self.destination / asset["name"]).read_bytes())
        sums = (self.destination / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(sums), 25)
        for line in sums:
            expected, name = line.split("  ")
            self.assertEqual(digest(self.destination / name), expected)

    def test_rejects_missing_sidecar_manifest_wrong_source_and_changed_checksums(self):
        sidecar = next(self.source.rglob("*.zsync.manifest.json"))
        data = sidecar.read_bytes()
        sidecar.unlink()
        with self.assertRaisesRegex(ValueError, "Missing"):
            self.promote()
        sidecar.write_bytes(data)
        self.commit = "b" * 40
        with self.assertRaisesRegex(ValueError, "identity"):
            self.promote()
        self.commit = "a" * 40
        sidecar.write_bytes(data + b" ")
        with self.assertRaisesRegex(ValueError, "checksums"):
            self.promote()
        self.assertFalse(self.destination.exists())

    def test_rejects_tampered_manifest_even_when_inventory_is_rehashed(self):
        path = next(self.source.rglob("*.manifest.json"))
        manifest = json.loads(path.read_text())
        manifest["signature"] = base64.b64encode(bytes(64)).decode()
        path.write_text(json.dumps(manifest))
        self.rehash()
        with self.assertRaises(subprocess.CalledProcessError):
            self.promote()
        self.assertFalse(self.destination.exists())

    def test_rejects_duplicate_and_linked_candidates(self):
        asset = next(self.source.rglob("*.AppImage"))
        duplicate = self.source / asset.name
        duplicate.write_bytes(asset.read_bytes())
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.promote()
        duplicate.unlink()
        duplicate.symlink_to(asset)
        with self.assertRaisesRegex(ValueError, "symbolic link"):
            self.promote()


if __name__ == "__main__":
    unittest.main()
