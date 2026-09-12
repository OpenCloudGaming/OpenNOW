from pathlib import Path
import os
import re
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"


def jobs(workflow):
    sections = re.split(r"^  ([a-z][a-z-]*):\n", workflow.split("jobs:\n", 1)[1], flags=re.MULTILINE)
    return dict(zip(sections[1::2], sections[2::2]))


class CIWorkflowTest(unittest.TestCase):
    def test_linux_appimages_deploy_wayland_platform_and_shell_plugins(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (WORKFLOWS / name).read_text()
                self.assertIn("qtwaylandcompositor", workflow)
                self.assertIn("EXTRA_QT_MODULES: waylandcompositor", workflow)
                self.assertIn(
                    "EXTRA_PLATFORM_PLUGINS: libqoffscreen.so;libqwayland-egl.so;libqwayland-generic.so",
                    workflow,
                )
                for plugin in ("platforms/libqwayland-egl.so", "platforms/libqwayland-generic.so",
                               "wayland-shell-integration/libxdg-shell.so"):
                    self.assertIn(f"test -f build/AppDir/usr/plugins/{plugin}", workflow)

    def test_automatic_events_run_checks_without_packages(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        entries = jobs(ci)
        self.assertEqual(set(entries), {"preflight", "contracts", "checks", "build", "sign-nightly", "publish-nightly"})
        self.assertIn("uses: ./.github/workflows/qt-checks.yml", entries["contracts"])
        self.assertNotIn("    if:", entries["contracts"])
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["build"])
        self.assertIn("    needs: [contracts, preflight]\n", entries["build"])
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

    def test_windows_packaging_failure_prints_bounded_logs_and_preserves_status(self):
        workflow = (WORKFLOWS / "qt-build.yml").read_text()
        step = workflow.split("      - name: Create unsigned package\n", 1)[1].split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index in range(6):
                log = root / f"build/qt-packages/log folder {index}/wix.log"
                log.parent.mkdir(parents=True)
                log.write_text("first-line-must-be-truncated\n" + ("x" * 1024 + "\n") * 300 + "WiX failure detail\n")
            prelude = 'cd() { return 0; }\ncpack() { return "$CPACK_STATUS"; }\n'
            for platform, status in (("Windows", 37), ("Windows", 0), ("Linux", 37)):
                with self.subTest(platform=platform, status=status):
                    result = subprocess.run(["bash", "-euo", "pipefail", "-c", prelude + script],
                                            cwd=root, env={**os.environ, "RUNNER_OS": platform,
                                                           "CPACK_STATUS": str(status), "PACKAGE_GENERATOR": "WIX;ZIP"},
                                            capture_output=True, text=True)
                    self.assertEqual(result.returncode, status, result.stderr)
                    if platform == "Windows" and status:
                        self.assertEqual(result.stdout.count("--- WiX packaging log:"), 4)
                        self.assertEqual(result.stdout.count("WiX failure detail"), 4)
                        self.assertNotIn("first-line-must-be-truncated", result.stdout)
                        self.assertLess(len(result.stdout), 4 * 65536 + 4096)
                    else:
                        self.assertEqual(result.stdout, "")
            result = subprocess.run(["bash", "-euo", "pipefail", "-c",
                                     prelude + 'tail() { return 11; }\n' + script],
                                    cwd=root, env={**os.environ, "RUNNER_OS": "Windows",
                                                   "CPACK_STATUS": "37", "PACKAGE_GENERATOR": "WIX;ZIP"},
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 37, result.stderr)
            for log in root.rglob("wix.log"):
                log.unlink()
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", prelude + script],
                                    cwd=root, env={**os.environ, "RUNNER_OS": "Windows",
                                                   "CPACK_STATUS": "37", "PACKAGE_GENERATOR": "WIX;ZIP"},
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertIn("No generated WiX logs found.", result.stdout)

    def test_all_native_platform_checks_keep_required_status_names(self):
        checks = jobs((WORKFLOWS / "qt-ci.yml").read_text())["checks"]
        labels = re.findall(r"^          - label: (.+)$", checks, re.MULTILINE)
        self.assertCountEqual(labels, ["linux-x64", "windows-x64", "macos-arm64"])
        self.assertIn("    name: ${{ matrix.label }}\n", checks)
        self.assertIn("    runs-on: ${{ matrix.os }}\n", checks)
        self.assertIn("      fail-fast: false\n", checks)
        self.assertIn("uses: ./.github/actions/qt-unit-tests", checks)
        self.assertIn("os: blacksmith-8vcpu-windows-2025", checks)
        self.assertIn("os: blacksmith-6vcpu-macos-15", checks)
        self.assertNotIn("continue-on-error", checks)

    def test_linux_and_windows_checks_use_eight_core_build_parallelism(self):
        checks = jobs((WORKFLOWS / "qt-ci.yml").read_text())["checks"]
        for label, runner in (("linux-x64", "blacksmith-8vcpu-ubuntu-2404"),
                              ("windows-x64", "blacksmith-8vcpu-windows-2025")):
            with self.subTest(label=label):
                entry = checks.split(f"          - label: {label}\n", 1)[1].split("          - label:", 1)[0]
                self.assertIn(f"            os: {runner}\n", entry)
                self.assertIn('            parallel: "8"\n', entry)
        self.assertNotIn("CARGO_BUILD_JOBS:", checks)
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        self.assertNotIn("CARGO_BUILD_JOBS:", action.split("    - name: Lint Rust", 1)[0])
        for step in ("Lint Rust", "Test Rust"):
            entry = action.split(f"    - name: {step}\n", 1)[1].split("    - name:", 1)[0]
            self.assertIn("CARGO_BUILD_JOBS: ${{ inputs.parallel }}", entry)
        self.assertIn("parallel: ${{ matrix.parallel }}", checks)

    def test_linux_and_windows_packages_use_eight_core_build_parallelism(self):
        packages = jobs((WORKFLOWS / "qt-build.yml").read_text())["packages"]
        for label in ("linux-x64", "linux-arm64", "windows-x64", "windows-arm64"):
            with self.subTest(label=label):
                entry = packages.split(f"          - label: {label}\n", 1)[1].split("          - label:", 1)[0]
                self.assertIn("            os: blacksmith-8vcpu-", entry)
                self.assertIn("            build-parallel: 8\n", entry)

    def test_manual_packages_overlap_checks_without_bypassing_publication_gates(self):
        entries = jobs((WORKFLOWS / "qt-ci.yml").read_text())
        self.assertIn("    needs: [contracts, preflight]\n", entries["build"])
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["build"])
        self.assertIn("    needs: [preflight, contracts, checks, build]\n", entries["sign-nightly"])
        self.assertIn("    needs: [contracts, checks, build, sign-nightly]\n", entries["publish-nightly"])
        self.assertNotIn("always()", entries["build"])
        self.assertNotIn("always()", entries["publish-nightly"])
        self.assertNotIn("continue-on-error", entries["checks"])
        self.assertNotIn("continue-on-error", entries["build"])

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
        self.assertEqual(len(targets), 26)
        self.assertEqual(len(set(targets)), 26)
        self.assertIn("opennow-tenbitwarning-tests", targets)
        self.assertIn("opennow-consoleactions-tests", targets)
        self.assertIn("opennow-controllernavigation-tests", targets)
        self.assertIn("opennow-graphicsdevices-tests", targets)
        self.assertIn("opennow-consolelayout-tests", targets)
        self.assertIn("opennow-macawdl-tests", targets)
        self.assertIn("opennow-controllericons-tests", targets)
        self.assertIn("opennow-streamtoasts-tests", targets)
        self.assertIn("opennow-waylandhdroutput-tests", targets)
        for forbidden in ("opennow-qt", "opennow-streamvideo-tests", "opennow-nativestreamruntime-tests",
                          "opennow-nativeframegeneration-tests", "opennow-linuxvulkangraphics-tests"):
            self.assertNotIn(forbidden, targets)
        self.assertIn("add_custom_target(opennow-ci-unit-tests DEPENDS ${OPENNOW_CI_UNIT_TEST_TARGETS})", cmake)
        self.assertIn('set_tests_properties(${OPENNOW_CI_UNIT_TEST_TARGETS} PROPERTIES LABELS "ci-unit")', cmake)
        self.assertIn('ENVIRONMENT "QT_QPA_PLATFORM=cocoa" RUN_SERIAL TRUE TIMEOUT 30 LABELS "interactive-desktop"', cmake)

    def test_signed_helper_integration_runs_on_every_platform_check_not_signers(self):
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        marker = "    - name: Test signed update helper integration\n"
        before, after = action.split(marker, 1)
        step = after.split("    - name:", 1)[0]
        for setup in ("uses: ilammy/msvc-dev-cmd@v1", "uses: dtolnay/rust-toolchain@stable",
                      "name: Install Linux test dependencies", "name: Test Rust"):
            self.assertIn(setup, before)
        self.assertNotIn("      if:", step)
        self.assertNotIn("continue-on-error", step)
        self.assertIn("working-directory: ${{ env.OPENNOW_CHECKOUT }}", step)
        self.assertIn("CARGO_BUILD_JOBS: ${{ inputs.parallel }}", step)
        self.assertIn("python opennow-qt/tests/run_update_helper_integration.py", step)
        self.assertIn("python3 opennow-qt/tests/run_update_helper_integration.py", step)
        self.assertNotIn("secrets.", step)
        ci = jobs((WORKFLOWS / "qt-ci.yml").read_text())
        for label in ("linux-x64", "windows-x64", "macos-arm64"):
            self.assertIn(f"label: {label}", ci["checks"])
        self.assertIn("uses: ./.github/actions/qt-unit-tests", ci["checks"])
        self.assertNotIn("run_update_helper_integration.py", ci["sign-nightly"])
        candidate = jobs((WORKFLOWS / "qt-release-candidate.yml").read_text())
        self.assertNotIn("run_update_helper_integration.py", candidate["inventory"])

    def test_windows_checks_use_verified_llvm_fallback(self):
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        llvm = (ROOT / ".github/scripts/ensure-windows-llvm.ps1").read_text()
        self.assertIn(".github/scripts/ensure-windows-llvm.ps1", action)
        self.assertNotIn("choco install llvm", action)
        self.assertIn("choco install nasm -y --no-progress", action)
        self.assertIn("NASM installation is missing nasm.exe", action)
        self.assertIn("LLVM-22.1.8-win64.exe", llvm)
        self.assertIn("16e5709785fef73c854646241c4a92c5cd574318d1b33c63330dd7721903e55c", llvm)
        self.assertIn("LLVM-22.1.8-woa64.exe", llvm)
        self.assertIn("76f44ef1ba6eeb5a65904e9500f042f588fade49952778ce48f0374daa934396", llvm)
        self.assertIn("Get-FileHash -Algorithm SHA256", llvm)
        self.assertIn("libclang.dll", llvm)
        self.assertIn("LIBCLANG_PATH=$llvmBin", llvm)
        self.assertIn("Unsupported Windows LLVM runner architecture", llvm)

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
        self.assertIn("    needs: [contracts, checks, build, sign-nightly]\n", publish)


if __name__ == "__main__":
    unittest.main()
