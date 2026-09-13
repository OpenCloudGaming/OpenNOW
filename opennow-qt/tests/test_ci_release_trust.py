import base64
import os
from pathlib import Path
import subprocess
import textwrap
import unittest


WORKFLOWS = Path(__file__).resolve().parents[2] / ".github/workflows"


class CIReleaseTrustTest(unittest.TestCase):
    def validate(self, source_commit, workflow_sha, windows_signing_mode="unsigned"):
        workflow = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        step = workflow.split("      - name: Validate release inputs\n", 1)[1]
        step = step.split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        return subprocess.run(
            ["bash", "-euo", "pipefail", "-c", script],
            env={
                **os.environ,
                "RELEASE_VERSION": "1.0.0",
                "SOURCE_COMMIT": source_commit,
                "GITHUB_SHA": workflow_sha,
                "UPDATE_PUBLIC_KEY": base64.b64encode(bytes(32)).decode(),
                "WINDOWS_SIGNING_MODE": windows_signing_mode,
            },
            capture_output=True,
            text=True,
        )

    def test_dispatched_revision_is_accepted(self):
        result = self.validate("a" * 40, "a" * 40)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_independent_revision_is_rejected(self):
        result = self.validate("a" * 40, "b" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("branch or tag pointing to source_commit", result.stderr)

    def test_non_sha_revision_is_rejected(self):
        result = self.validate("dev", "a" * 40)
        self.assertNotEqual(result.returncode, 0)

    def test_windows_signing_mode_is_explicit_and_validated(self):
        self.assertEqual(self.validate("a" * 40, "a" * 40, "authenticode").returncode, 0)
        for mode in ("", "signed", "auto", "UNSIGNED"):
            with self.subTest(mode=mode):
                result = self.validate("a" * 40, "a" * 40, mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Invalid Windows signing mode", result.stderr)

    def test_release_checkouts_do_not_use_input_refs(self):
        for name in ("qt-release-candidate.yml", "qt-build.yml"):
            with self.subTest(workflow=name):
                refs = [line.strip() for line in (WORKFLOWS / name).read_text().splitlines()
                        if line.strip().startswith("ref:")]
                self.assertTrue(refs)
                self.assertEqual(set(refs), {"ref: ${{ github.sha }}"})

    def test_windows_unsigned_default_keeps_signing_explicit_and_separate(self):
        workflow = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        choice = workflow.split("      windows_signing_mode:\n", 1)[1].split("\nconcurrency:", 1)[0]
        for required in ("type: choice", "default: unsigned", "- unsigned", "- authenticode"):
            self.assertIn(required, choice)
        windows = workflow.split("  windows:\n", 1)[1].split("  macos:\n", 1)[0]
        self.assertIn("environment: qt-production-release", windows)
        self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", windows)
        for step in windows.split("      - name: ")[1:]:
            if "secrets.OPENNOW_WINDOWS_" in step:
                self.assertIn("if: inputs.windows_signing_mode == 'authenticode'", step)
        for name in ("Authenticode-sign application binaries", "Authenticode-sign MSI"):
            step = windows.split(f"      - name: {name}\n", 1)[1].split("      - name:", 1)[0]
            self.assertIn("if: inputs.windows_signing_mode == 'authenticode'", step)
            self.assertIn("Invoke-OpenNowSignTool sign", step)
            self.assertIn("Invoke-OpenNowSignTool verify /pa /all", step)
            self.assertNotIn("continue-on-error", step)
        verify = windows.split("      - name: Verify MSI and portable ZIP payloads\n", 1)[1].split("      - name:", 1)[0]
        self.assertNotIn("        if:", verify)
        self.assertEqual(verify.count("Assert-OpenNowPackagePayload -Root"), 2)
        self.assertEqual(verify.count("Assert-OpenNowSignedPackage -Root"), 2)
        self.assertIn("always() && inputs.windows_signing_mode == 'authenticode'", windows)

    def test_windows_artifact_names_and_inventory_record_platform_signing_mode(self):
        workflow = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        self.assertIn("name: opennow-qt-${{ inputs.version }}-windows-${{ matrix.arch }}-${{ inputs.windows_signing_mode }}", workflow)
        self.assertNotIn("windows-${{ matrix.arch }}-signed", workflow)
        inventory = workflow.split("  inventory:\n", 1)[1]
        self.assertIn("pattern: opennow-qt-${{ inputs.version }}-*", inventory)
        self.assertIn("WINDOWS_SIGNING_MODE: ${{ inputs.windows_signing_mode }}", inventory)
        self.assertIn('echo "windowsSigningMode=$WINDOWS_SIGNING_MODE"', inventory)
        self.assertIn("environment: qt-update-signing", inventory)
        self.assertIn("secrets.OPENNOW_UPDATE_ED25519_PRIVATE_KEY", inventory)

    def test_candidate_update_key_is_isolated_in_a_blacksmith_signing_job(self):
        workflow = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        builds, signer = workflow.split("  inventory:\n", 1)
        self.assertIn("runs-on: blacksmith-2vcpu-ubuntu-2404", signer)
        self.assertIn("environment: qt-update-signing", signer)
        self.assertIn("needs: [preflight, linux, windows, macos]", signer)
        self.assertIn("timeout-minutes: 30", signer)
        self.assertIn("name: Verify signing tools", signer)
        self.assertEqual(signer.count("secrets.OPENNOW_UPDATE_ED25519_PRIVATE_KEY"), 1)
        self.assertNotIn("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", builds)
        for forbidden in ("actions/cache", "actions/checkout", "cargo ", "cmake ", "cpack "):
            self.assertNotIn(forbidden, signer)


if __name__ == "__main__":
    unittest.main()
