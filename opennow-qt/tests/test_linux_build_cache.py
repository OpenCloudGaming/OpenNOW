from pathlib import Path
import os
import re
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
ACTION = ROOT / ".github/actions/linux-build-cache/action.yml"


class LinuxBuildCacheTest(unittest.TestCase):
    def test_disks_are_stable_and_only_default_branch_jobs_commit(self):
        action = ACTION.read_text()
        self.assertEqual(action.count("uses: useblacksmith/stickydisk@25e27b93b68733b532d9af6b201df28ffaf7dbfc"), 5)
        self.assertEqual(action.count(
            "commit: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) "
            "&& (github.event_name == 'push' || github.event_name == 'workflow_dispatch') }}"
        ), 5)
        for suffix in ("registry", "git", "core", "streamer", "ccache"):
            self.assertIn(f"key: opennow-native-v1-${{{{ inputs.key }}}}-{suffix}\n", action)
        for invalidator in ("github.sha", "github.run_id", "hashFiles", "matrix.os", "CARGO_BUILD_JOBS"):
            self.assertNotIn(invalidator, action)
        paths = [line.strip().removeprefix("path: ") for line in action.splitlines()
                 if line.strip().startswith("path: ")]
        self.assertEqual(paths, [
            "${{ env.CARGO_HOME }}/registry",
            "${{ env.CARGO_HOME }}/git",
            "${{ github.workspace }}/${{ inputs.core-target }}",
            "${{ github.workspace }}/${{ inputs.streamer-target }}",
            "${{ github.workspace }}/.ccache",
        ])

    def test_only_linux_mounts_disks_with_separate_check_and_release_targets(self):
        for path, key, core_target, streamer_target in (
            (ROOT / ".github/actions/qt-unit-tests/action.yml", "ubuntu-2404-checks-${{ inputs.label }}",
             "native/opennow-core/target", "native/opennow-streamer/target"),
            (ROOT / ".github/workflows/qt-build.yml", "ubuntu-2404-release-${{ matrix.label }}",
             "build/opennow-qt-release/rust-target", "build/opennow-qt-release/streamer-rust-target"),
        ):
            with self.subTest(path=path):
                workflow = path.read_text()
                step = re.split(r"\n\s+- ", workflow.split("- name: Mount persistent Linux build cache\n", 1)[1], 1)[0] + "\n"
                self.assertIn("if: runner.os == 'Linux'", step)
                self.assertIn("uses: ./.github/actions/linux-build-cache", step)
                self.assertIn(f"key: {key}\n", step)
                self.assertIn(f"core-target: {core_target}\n", step)
                self.assertIn(f"streamer-target: {streamer_target}\n", step)
                self.assertLess(workflow.index("dtolnay/rust-toolchain@"), workflow.index("id: sticky-cache"))

    def test_archive_fallback_remains_available_until_disks_are_populated(self):
        for path in (ROOT / ".github/actions/qt-unit-tests/action.yml", ROOT / ".github/workflows/qt-build.yml"):
            with self.subTest(path=path):
                workflow = path.read_text()
                self.assertIn("if: runner.os != 'Linux' || steps.sticky-cache.outputs.rust-warm != 'true'", workflow)
                self.assertIn("cache-on-failure: true", workflow)
                for operation in ("restore", "save"):
                    self.assertIn(
                        f"{operation}: ${{{{ runner.os != 'Linux' || steps.sticky-cache.outputs.ccache-warm != 'true' }}}}",
                        workflow,
                    )

    def test_artifact_detection_distinguishes_empty_partial_and_populated_disks(self):
        script = textwrap.dedent(ACTION.read_text().split("      run: |\n", 1)[1])
        for core, streamer, ccache in ((False, False, False), (True, False, False),
                                        (False, True, True), (True, True, False), (True, True, True)):
            with self.subTest(core=core, streamer=streamer, ccache=ccache), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for name in ("core", "streamer", "ccache"):
                    (root / name).mkdir()
                (root / "ccache/ccache.conf").write_text("max_size = 1G\n")
                for present, artifact in ((core, "core/libdependency.rlib"),
                                          (streamer, "streamer/libdependency.rlib"),
                                          (ccache, "ccache/compiledR")):
                    if present:
                        (root / artifact).write_bytes(b"fixture")
                result = subprocess.run(["bash", "-euo", "pipefail", "-c", script], env={
                    **os.environ,
                    "CORE_TARGET": str(root / "core"),
                    "STREAMER_TARGET": str(root / "streamer"),
                    "CXX_CACHE": str(root / "ccache"),
                    "GITHUB_OUTPUT": str(root / "output"),
                    "GITHUB_STEP_SUMMARY": str(root / "summary"),
                }, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((root / "output").read_text(),
                                 f"rust-warm={str(core and streamer).lower()}\nccache-warm={str(ccache).lower()}\n")
                self.assertIn("Only default-branch", (root / "summary").read_text())


if __name__ == "__main__":
    unittest.main()
