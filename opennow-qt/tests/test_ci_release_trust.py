import base64
import os
from pathlib import Path
import subprocess
import textwrap
import unittest


WORKFLOWS = Path(__file__).resolve().parents[2] / ".github/workflows"


class CIReleaseTrustTest(unittest.TestCase):
    def validate(self, source_commit, workflow_sha):
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

    def test_release_checkouts_do_not_use_input_refs(self):
        for name in ("qt-release-candidate.yml", "qt-ci.yml"):
            with self.subTest(workflow=name):
                refs = [line.strip() for line in (WORKFLOWS / name).read_text().splitlines()
                        if line.strip().startswith("ref:")]
                self.assertTrue(refs)
                self.assertEqual(set(refs), {"ref: ${{ github.sha }}"})


if __name__ == "__main__":
    unittest.main()
