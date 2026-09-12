import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "packaging"))
from nightly_release import assemble, expected_packages
from sign_nightly_release import decode_key, digest, sign, verify


ROOT = Path(__file__).resolve().parents[2]


class SignedNightlyReleaseTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.version = "1.0.0-nightly.123.2"
        self.commit = "a" * 40
        self.packages = expected_packages(self.version, self.commit)
        raw = self.root / "raw"
        raw.mkdir()
        for name in self.packages:
            (raw / name).write_bytes(f"test package {name}".encode())
        self.source = self.root / "unsigned"
        self.destination = self.root / "signed"
        assemble(raw, self.source, self.version, self.commit)
        self.seed = base64.b64encode(os.urandom(32)).decode()
        key = self.root / "test-private.der"
        key.write_bytes(bytes.fromhex("302e020100300506032b657004220420") + base64.b64decode(self.seed))
        public = subprocess.check_output([
            "openssl", "pkey", "-inform", "DER", "-in", str(key), "-pubout", "-outform", "DER"])
        self.public = base64.b64encode(public[-32:]).decode()
        key.unlink()

    def sign(self):
        sign(self.source, self.destination, self.version, self.commit, self.public, self.seed)

    def verify(self):
        verify(self.destination, self.version, self.commit, self.public)

    def rehash(self, directory):
        (directory / "SHA256SUMS").write_text("".join(
            f"{digest(path)}  {path.name}\n" for path in sorted(directory.iterdir())
            if path.name != "SHA256SUMS"))

    def test_signs_and_verifies_every_exact_package_without_changing_bytes(self):
        self.sign()
        self.verify()
        self.assertEqual(len(list(self.destination.iterdir())), 20)
        self.assertEqual(len((self.destination / "SHA256SUMS").read_text().splitlines()), 19)
        info = json.loads((self.destination / "RELEASE-INFO.json").read_text())
        self.assertEqual(info["updates"], "signed-manifest")
        self.assertEqual(info["platformSigning"], "unsigned")
        for name in self.packages:
            self.assertEqual((self.source / name).read_bytes(), (self.destination / name).read_bytes())
            manifest = json.loads((self.destination / (name + ".manifest.json")).read_text())
            self.assertEqual(manifest["asset"], name)
            self.assertEqual(manifest["version"], self.version)

    def test_mismatched_seed_never_exposes_a_final_inventory(self):
        self.seed = base64.b64encode(os.urandom(32)).decode()
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.sign()
        self.assertFalse(self.destination.exists())

    def test_each_package_is_required_before_signing(self):
        for name in self.packages:
            with self.subTest(name=name):
                package = self.source / name
                content = package.read_bytes()
                package.unlink()
                with self.assertRaisesRegex(ValueError, "inventory entry"):
                    self.sign()
                self.assertFalse(self.destination.exists())
                package.write_bytes(content)

    def test_rejects_extra_package_validation_zip_and_symbolic_link(self):
        for name in ("unexpected.zip", f"OpenNOW-Qt-{self.version}-Darwin-arm64.zip"):
            extra = self.source / name
            extra.write_bytes(b"not public")
            with self.assertRaises(ValueError):
                self.sign()
            extra.unlink()
        package = self.source / next(iter(self.packages))
        package.unlink()
        package.symlink_to(self.source / "RELEASE-INFO.json")
        with self.assertRaises(ValueError):
            self.sign()

    def test_source_commit_and_package_hash_are_immutable(self):
        self.commit = "b" * 40
        with self.assertRaisesRegex(ValueError, "immutable package inventory"):
            self.sign()
        self.commit = "a" * 40
        (self.source / next(iter(self.packages))).write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "immutable package inventory"):
            self.sign()

    def test_rejects_incomplete_or_duplicate_checksums(self):
        sums = self.source / "SHA256SUMS"
        lines = sums.read_text().splitlines(keepends=True)
        for changed in (lines[:-1], lines + lines[:1]):
            sums.write_text("".join(changed))
            with self.assertRaisesRegex(ValueError, "SHA256SUMS"):
                self.sign()

    def test_rejects_rehashed_manifest_tampering_and_wrong_public_key(self):
        self.sign()
        public = self.public
        self.public = base64.b64encode(os.urandom(32)).decode()
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify()
        self.public = public
        manifest_path = next(self.destination.glob("*.manifest.json"))
        manifest = json.loads(manifest_path.read_text())
        manifest["signature"] = base64.b64encode(bytes(64)).decode()
        manifest_path.write_text(json.dumps(manifest))
        self.rehash(self.destination)
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify()

    def test_rejects_wrong_manifest_identity_even_with_matching_checksums(self):
        self.sign()
        manifest_path = next(self.destination.glob("*.manifest.json"))
        manifest = json.loads(manifest_path.read_text())
        manifest["asset"] = "different.zip"
        manifest_path.write_text(json.dumps(manifest))
        self.rehash(self.destination)
        with self.assertRaisesRegex(ValueError, "exact package"):
            self.verify()

    def test_preflight_requires_canonical_key_only_for_publication(self):
        script = ROOT / "opennow-qt/packaging/sign_nightly_release.py"
        for key, publish, success in (("", "false", True), ("", "true", False),
                                      (self.public, "true", True), ("invalid", "false", False),
                                      (self.public + "\n", "true", False),
                                      (base64.b64encode(bytes(31)).decode(), "true", False)):
            with self.subTest(key_length=len(key), publish=publish):
                result = subprocess.run([sys.executable, str(script), "preflight"],
                                        env={**os.environ, "OPENNOW_UPDATE_PUBLIC_KEY": key,
                                             "PUBLISH_NIGHTLY": publish}, capture_output=True)
                self.assertEqual(result.returncode == 0, success)
        with self.assertRaises(ValueError):
            decode_key(self.public[:-1])

    def test_private_seed_is_not_inherited_by_tools_or_written_to_logs(self):
        tools = self.root / "tools"
        tools.mkdir()
        wrapper = tools / "openssl"
        real_openssl = shutil.which("openssl")
        self.assertIsNotNone(real_openssl)
        wrapper.write_text(
            f"#!{sys.executable}\nimport os, sys\n"
            "assert 'OPENNOW_UPDATE_ED25519_PRIVATE_KEY' not in os.environ\n"
            f"os.execv({real_openssl!r}, [{real_openssl!r}, *sys.argv[1:]])\n")
        wrapper.chmod(0o700)
        result = subprocess.run([
            sys.executable, str(ROOT / "opennow-qt/packaging/sign_nightly_release.py"), "sign",
            "--source", str(self.source), "--destination", str(self.destination),
            "--version", self.version, "--commit", self.commit,
        ], env={**os.environ, "PATH": str(tools) + os.pathsep + os.environ["PATH"],
                "OPENNOW_UPDATE_PUBLIC_KEY": self.public,
                "OPENNOW_UPDATE_ED25519_PRIVATE_KEY": self.seed}, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn(self.seed, result.stdout + result.stderr)
        self.verify()

    def test_workflow_secret_is_isolated_and_publication_uses_only_signed_set(self):
        workflow = (ROOT / ".github/workflows/qt-ci.yml").read_text()
        signer = workflow.split("  sign-nightly:\n", 1)[1].split("  publish-nightly:\n", 1)[0]
        publisher = workflow.split("  publish-nightly:\n", 1)[1]
        self.assertIn("runs-on: [self-hosted, opennow-release-signer]", signer)
        self.assertIn("environment: qt-update-signing", signer)
        self.assertIn("needs: [preflight, contracts, checks, build]", signer)
        self.assertIn("ref: ${{ github.sha }}", signer)
        self.assertIn("persist-credentials: false", signer)
        self.assertEqual(workflow.count("secrets.OPENNOW_UPDATE_ED25519_PRIVATE_KEY"), 1)
        self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", publisher)
        self.assertNotIn("complete-unsigned", publisher)
        self.assertIn("complete-update-signed", publisher)
        self.assertIn("sign_nightly_release.py verify", publisher)
        for forbidden in ("cargo ", "cmake ", "unsigned-release/", "signed-release/"):
            self.assertNotIn("run: " + forbidden, signer)
        self.assertIn("update_public_key: ${{ inputs.public_key }}", workflow)
        self.assertIn('--title "OpenNOW v$RELEASE_VERSION" --generate-notes', publisher)
        self.assertNotIn("--notes-file", publisher)
        self.assertNotIn("nightly-notes.md", publisher)
        preflight = workflow.split("  preflight:\n", 1)[1].split("  contracts:\n", 1)[0]
        self.assertIn("if: github.event_name == 'workflow_dispatch'", preflight)
        outside_signer = workflow.replace(signer, "")
        self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", outside_signer)
        for path in (ROOT / ".github/workflows/qt-build.yml", ROOT / ".github/actions/qt-unit-tests/action.yml"):
            self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", path.read_text())


if __name__ == "__main__":
    unittest.main()
