from pathlib import Path
import os
import re
import subprocess
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
        self.assertEqual(set(entries), {"contracts", "checks", "build", "publish-nightly"})
        self.assertIn("uses: ./.github/workflows/qt-checks.yml", entries["contracts"])
        self.assertNotIn("    if:", entries["contracts"])
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["build"])
        self.assertIn("    needs: [contracts, checks]\n", entries["build"])
        self.assertIn("uses: ./.github/workflows/qt-build.yml", entries["build"])
        self.assertIn("  pull_request:\n", ci)
        self.assertIn("  push:\n", ci)
        self.assertEqual(ci.count('      - ".github/workflows/qt-checks.yml"'), 2)

    def test_manual_packages_cannot_bypass_dispatch_gate(self):
        entries = jobs((WORKFLOWS / "qt-build.yml").read_text())
        self.assertEqual(set(entries), {"metadata", "packages", "artifact-inventory"})
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["metadata"])
        self.assertIn("    needs: metadata\n", entries["packages"])
        self.assertNotIn("always()", entries["packages"])
        self.assertIn("    needs: [metadata, packages]\n", entries["artifact-inventory"])
        self.assertIn("        if: inputs.upload_complete\n", entries["artifact-inventory"])

    def test_all_platforms_share_one_packaging_matrix(self):
        entries = jobs((WORKFLOWS / "qt-build.yml").read_text())
        labels = re.findall(r"^          - label: (.+)$", entries["packages"], re.MULTILINE)
        self.assertCountEqual(labels, ["linux-x64", "linux-arm64", "windows-x64", "windows-arm64", "macos-arm64"])
        self.assertIn("name: Package ${{ matrix.label }}", entries["packages"])
        self.assertIn("Create Linux AppImage", entries["packages"])
        self.assertIn("Test relocated bundle without development libraries", entries["packages"])

    def test_all_native_platform_checks_keep_required_status_names(self):
        checks = jobs((WORKFLOWS / "qt-ci.yml").read_text())["checks"]
        labels = re.findall(r"^          - label: (.+)$", checks, re.MULTILINE)
        self.assertCountEqual(labels, ["linux-x64", "windows-x64", "macos-arm64"])
        self.assertIn("    name: ${{ matrix.label }}\n", checks)
        self.assertIn("    runs-on: ${{ matrix.os }}\n", checks)
        self.assertIn("      fail-fast: false\n", checks)
        self.assertIn("uses: ./.github/actions/qt-unit-tests", checks)
        self.assertIn("os: blacksmith-4vcpu-windows-2025", checks)
        self.assertIn("os: blacksmith-6vcpu-macos-15", checks)
        self.assertNotIn("continue-on-error", checks)

    def test_required_platform_checks_fail_when_shared_checks_do_not_succeed(self):
        checks = jobs((WORKFLOWS / "qt-ci.yml").read_text())["checks"]
        self.assertIn("    needs: contracts\n    if: always()\n", checks)
        self.assertIn("CONTRACTS_RESULT: ${{ needs.contracts.result }}", checks)
        script = re.search(r"        run: (test .+)\n", checks)[1]
        for result in ("success", "failure", "cancelled", "skipped", ""):
            with self.subTest(result=result):
                process = subprocess.run(["bash", "-c", script], env={**os.environ, "CONTRACTS_RESULT": result})
                self.assertEqual(process.returncode == 0, result == "success")

    def test_checks_compile_tests_without_packaging_or_runtime_targets(self):
        contracts = (WORKFLOWS / "qt-checks.yml").read_text()
        checks = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        entries = jobs(contracts)
        self.assertEqual(set(entries), {"contracts"})
        self.assertIn("runs-on: blacksmith-2vcpu-ubuntu-2404", entries["contracts"])
        for forbidden in ("matrix:", "cpack ", "linuxdeploy", "upload-artifact", "--release",
                          "uses: ./.github/workflows/qt-build.yml"):
            self.assertNotIn(forbidden, checks)
        self.assertIn('--target opennow-ci-unit-tests --parallel "$BUILD_PARALLEL"', checks)
        self.assertIn("--no-tests=error -L ci-unit", checks)
        self.assertIn("cargo clippy --locked", checks)
        self.assertIn("cargo test --locked", checks)
        self.assertIn("--workspace --all-targets -- -D warnings", checks)
        self.assertIn('"$QT_ROOT_DIR/bin/qmlformat"', checks)
        self.assertNotIn("ensure-windows-test-desktop.ps1", checks)
        self.assertIn("ensure-windows-media-foundation.ps1", checks)
        cmake = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        targets = re.search(r"set\(OPENNOW_CI_UNIT_TEST_TARGETS\s+(.*?)\)", cmake, re.DOTALL)[1].split()
        self.assertEqual(len(targets), 18)
        self.assertEqual(len(set(targets)), 18)
        self.assertIn("opennow-waylandhdroutput-tests", targets)
        for forbidden in ("opennow-qt", "opennow-streamvideo-tests", "opennow-nativestreamruntime-tests",
                          "opennow-nativeframegeneration-tests", "opennow-linuxvulkangraphics-tests"):
            self.assertNotIn(forbidden, targets)
        self.assertIn("add_custom_target(opennow-ci-unit-tests DEPENDS ${OPENNOW_CI_UNIT_TEST_TARGETS})", cmake)
        self.assertIn('set_tests_properties(${OPENNOW_CI_UNIT_TEST_TARGETS} PROPERTIES LABELS "ci-unit")', cmake)
        self.assertIn('ENVIRONMENT "QT_QPA_PLATFORM=cocoa" RUN_SERIAL TRUE TIMEOUT 30 LABELS "interactive-desktop"', cmake)

    def test_interactive_tests_remain_registered_outside_headless_ci(self):
        cmake = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        self.assertIn("list(REMOVE_ITEM OPENNOW_CI_UNIT_TEST_TARGETS opennow-hdrcolor-tests)", cmake)
        self.assertIn('set_tests_properties(opennow-hdrcolor-tests PROPERTIES LABELS "interactive-desktop")', cmake)
        self.assertIn("add_custom_target(opennow-interactive-tests DEPENDS opennow-hdrcolor-tests)", cmake)
        self.assertIn("add_custom_target(opennow-interactive-tests DEPENDS opennow-macpointer-tests)", cmake)
        self.assertIn("add_test(NAME opennow-hdrcolor-tests", cmake)
        self.assertIn("add_test(NAME opennow-macpointer-native-tests", cmake)
        runtime_consumers = cmake.split("foreach(test_target IN ITEMS", 1)[1]
        self.assertIn("opennow-hdrcolor-tests", runtime_consumers)
        packages = (WORKFLOWS / "qt-build.yml").read_text()
        self.assertEqual(packages.count("--no-tests=error -LE interactive-desktop"), 2)

    def test_general_purpose_runners_are_blacksmith(self):
        for workflow in WORKFLOWS.glob("*.yml"):
            runners = re.findall(r"^\s+(?:runs-on|os|runner): (.+)$", workflow.read_text(), re.MULTILINE)
            for runner in runners:
                with self.subTest(workflow=workflow.name, runner=runner):
                    self.assertTrue(
                        runner.startswith(("blacksmith-", "${{"))
                        or runner == "[self-hosted, opennow-release-signer]",
                    )

    def test_rust_caches_survive_job_renames_and_later_test_failures(self):
        for path in (ROOT / ".github/actions/qt-unit-tests/action.yml", WORKFLOWS / "qt-build.yml"):
            with self.subTest(path=path):
                cache = path.read_text().split("uses: Swatinem/rust-cache@", 1)[1].split("\n      -", 1)[0]
                self.assertIn("shared-key:", cache)
                self.assertIn("cache-on-failure: true", cache)
                self.assertIn("native/opennow-core -> target", cache)
                self.assertIn("native/opennow-streamer -> target", cache)

    def test_publishing_remains_explicitly_opt_in_after_build(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        self.assertIn("        type: boolean\n        default: false", ci)
        publish = jobs(ci)["publish-nightly"]
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.publish_nightly", publish)
        self.assertIn("    needs: build\n", publish)


if __name__ == "__main__":
    unittest.main()
