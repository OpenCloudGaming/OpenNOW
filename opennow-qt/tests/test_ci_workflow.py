from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"


def jobs(workflow):
    sections = re.split(r"^  ([a-z][a-z-]*):\n", workflow.split("jobs:\n", 1)[1], flags=re.MULTILINE)
    return dict(zip(sections[1::2], sections[2::2]))


class CIWorkflowTest(unittest.TestCase):
    def test_automatic_events_run_checks_without_packages(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        entries = jobs(ci)
        self.assertEqual(set(entries), {"checks", "build", "publish-nightly"})
        self.assertIn("uses: ./.github/workflows/qt-checks.yml", entries["checks"])
        self.assertNotIn("    if:", entries["checks"])
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["build"])
        self.assertIn("    needs: checks\n", entries["build"])
        self.assertIn("uses: ./.github/workflows/qt-build.yml", entries["build"])
        self.assertIn("  pull_request:\n", ci)
        self.assertIn("  push:\n", ci)
        self.assertEqual(ci.count('      - ".github/workflows/qt-checks.yml"'), 2)

    def test_manual_packages_cannot_bypass_dispatch_gate(self):
        entries = jobs((WORKFLOWS / "qt-build.yml").read_text())
        self.assertEqual(set(entries), {"metadata", "packages", "macos-package", "artifact-inventory"})
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["metadata"])
        for name in ("packages", "macos-package"):
            self.assertIn("    needs: metadata\n", entries[name])
            self.assertNotIn("always()", entries[name])
        self.assertIn("    needs: [metadata, packages]\n", entries["artifact-inventory"])
        self.assertIn("        if: inputs.upload_complete\n", entries["artifact-inventory"])

    def test_windows_and_linux_share_one_packaging_matrix(self):
        entries = jobs((WORKFLOWS / "qt-build.yml").read_text())
        labels = re.findall(r"^          - label: (.+)$", entries["packages"], re.MULTILINE)
        self.assertCountEqual(labels, ["linux-x64", "linux-arm64", "windows-x64", "windows-arm64"])
        self.assertIn("name: Package ${{ matrix.label }}", entries["packages"])
        self.assertIn("Create Linux AppImage", entries["packages"])
        self.assertIn("Test relocated bundle without development libraries", entries["macos-package"])

    def test_checks_compile_tests_without_packaging_or_runtime_targets(self):
        checks = (WORKFLOWS / "qt-checks.yml").read_text()
        entries = jobs(checks)
        self.assertEqual(set(entries), {"contracts", "unit-tests"})
        self.assertIn("runs-on: ubuntu-24.04", entries["contracts"])
        self.assertIn("runs-on: blacksmith-4vcpu-ubuntu-2404", entries["unit-tests"])
        for forbidden in ("matrix:", "cpack ", "linuxdeploy", "upload-artifact", "--release",
                          "uses: ./.github/workflows/qt-build.yml"):
            self.assertNotIn(forbidden, checks)
        self.assertIn("--target opennow-ci-unit-tests --parallel 4", checks)
        self.assertIn("--no-tests=error -L ci-unit", checks)
        self.assertIn("cargo clippy --locked", checks)
        self.assertIn("cargo test --locked", checks)
        self.assertIn("--workspace --all-targets -- -D warnings", checks)
        self.assertIn('"$QT_ROOT_DIR/bin/qmlformat"', checks)
        cmake = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        targets = re.search(r"set\(OPENNOW_CI_UNIT_TEST_TARGETS\s+(.*?)\)", cmake, re.DOTALL)[1].split()
        self.assertEqual(len(targets), 17)
        self.assertEqual(len(set(targets)), 17)
        for forbidden in ("opennow-qt", "opennow-streamvideo-tests", "opennow-nativestreamruntime-tests",
                          "opennow-nativeframegeneration-tests", "opennow-linuxvulkangraphics-tests"):
            self.assertNotIn(forbidden, targets)
        self.assertIn("add_custom_target(opennow-ci-unit-tests DEPENDS ${OPENNOW_CI_UNIT_TEST_TARGETS})", cmake)
        self.assertIn('set_tests_properties(${OPENNOW_CI_UNIT_TEST_TARGETS} PROPERTIES LABELS "ci-unit")', cmake)

    def test_publishing_remains_explicitly_opt_in_after_build(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        self.assertIn("        type: boolean\n        default: false", ci)
        publish = jobs(ci)["publish-nightly"]
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.publish_nightly", publish)
        self.assertIn("    needs: build\n", publish)


if __name__ == "__main__":
    unittest.main()
