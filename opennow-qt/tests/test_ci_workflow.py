from pathlib import Path
import os
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"


def jobs(workflow):
    sections = re.split(r"^  ([a-z][a-z0-9-]*):\n", workflow.split("jobs:\n", 1)[1], flags=re.MULTILINE)
    return dict(zip(sections[1::2], sections[2::2]))


class CIWorkflowTest(unittest.TestCase):
    def test_arm_hosted_intel_can_publish_but_linux_measurements_cannot(self):
        entries = jobs((WORKFLOWS / "qt-ci.yml").read_text())
        for name in ("sign-nightly", "publish-nightly"):
            with self.subTest(job=name):
                condition = re.search(r"^    if: (.+)$", entries[name], re.MULTILINE).group(1)
                self.assertEqual(
                    condition,
                    "github.event_name == 'workflow_dispatch' && inputs.publish_nightly "
                    "&& inputs.linux_arm64_host == 'native'",
                )

    def test_stack_base_branches_run_pr_checks_without_new_push_or_publication_triggers(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        pull = ci.split("  pull_request:\n", 1)[1].split("  push:\n", 1)[0]
        push = ci.split("  push:\n", 1)[1].split("\nconcurrency:", 1)[0]
        for branch in ("capy/explain-cloud-session-setup", "capy/gfn-correctness/**"):
            self.assertIn(f"      - {branch}\n", pull)
            self.assertNotIn(branch, push)
        self.assertNotIn("pull_request_target:", ci)
        self.assertIn("github.event_name == 'workflow_dispatch'", jobs(ci)["publish-nightly"])

    def test_linux_appimages_deploy_wayland_platform_and_shell_plugins(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (WORKFLOWS / name).read_text()
                if name == "qt-build.yml":
                    self.assertIn("uses: ./.github/actions/linux-package", workflow)
                    workflow += (ROOT / ".github/actions/linux-package/action.yml").read_text()
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
        self.assertEqual(set(entries), {"metadata", "linux-arm64-cross", "packages", "flatpak", "artifact-inventory"})
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n", entries["metadata"])
        self.assertIn("    needs: metadata\n", entries["linux-arm64-cross"])
        self.assertIn("    if: inputs.linux_arm64_host == 'x64'\n", entries["linux-arm64-cross"])
        self.assertIn("    needs: metadata\n", entries["packages"])
        self.assertNotIn("always()", entries["packages"])
        self.assertIn("    needs: metadata\n", entries["flatpak"])
        self.assertIn("    needs: [metadata, packages, flatpak]\n", entries["artifact-inventory"])
        self.assertIn("        if: inputs.upload_complete\n", entries["artifact-inventory"])

    def test_all_platforms_share_one_packaging_matrix(self):
        entries = jobs((WORKFLOWS / "qt-build.yml").read_text())
        labels = re.findall(r"^          - label: (.+)$", entries["packages"], re.MULTILINE)
        self.assertCountEqual(labels, ["linux-x64", "linux-arm64", "windows-x64", "windows-arm64", "macos-arm64", "macos-x64"])
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
        self.assertIn("os: blacksmith-16vcpu-windows-2025", checks)
        self.assertIn("os: blacksmith-6vcpu-macos-15", checks)
        self.assertNotIn("continue-on-error", checks)

    def test_linux_and_windows_checks_use_sixteen_core_runners(self):
        checks = jobs((WORKFLOWS / "qt-ci.yml").read_text())["checks"]
        for label, runner in (("linux-x64", "blacksmith-16vcpu-ubuntu-2404"),
                              ("windows-x64", "blacksmith-16vcpu-windows-2025")):
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
        self.assertIn("    needs: contracts\n    if: ${{ !cancelled() && needs.contracts.result != 'cancelled' }}\n", checks)
        self.assertIn("CONTRACTS_RESULT: ${{ needs.contracts.result }}", checks)
        script = re.search(r"        run: (test .+)\n", checks)[1]
        for result in ("success", "failure", "skipped", ""):
            with self.subTest(result=result):
                process = subprocess.run(["bash", "-c", script], env={**os.environ, "CONTRACTS_RESULT": result})
                self.assertEqual(process.returncode == 0, result == "success")

    def test_checks_compile_tests_without_packaging_or_runtime_targets(self):
        contracts = (WORKFLOWS / "qt-checks.yml").read_text()
        checks = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        entries = jobs(contracts)
        self.assertEqual(set(entries), {"contracts"})
        self.assertIn("runs-on: blacksmith-2vcpu-ubuntu-2404", entries["contracts"])
        runner = (ROOT / ".github/scripts/run-native-checks.py").read_text()
        for forbidden in ("matrix:", "cpack ", "linuxdeploy", "--release",
                          "uses: ./.github/workflows/qt-build.yml"):
            self.assertNotIn(forbidden, checks)
        self.assertIn('"--target", "opennow-ci-unit-tests"', runner)
        self.assertIn('"--no-tests=error"', runner)
        self.assertIn('"-L", "ci-unit", "--output-junit", "qt-unit-tests.xml"', runner)
        self.assertIn('"LastTestsFailed.log", "LastTest.log"', runner)
        self.assertIn('["cargo", "clippy", *core, "--all-targets", "--", "-D", "warnings"]', runner)
        self.assertIn('["cargo", "clippy", *streamer, "--workspace", "--all-targets", "--", "-D", "warnings"]', runner)
        self.assertIn('["cargo", "test", *core]', runner)
        self.assertIn('["cargo", "test", *streamer, "--workspace"]', runner)
        self.assertIn('"--locked", "--manifest-path", "native/opennow-core/Cargo.toml"', runner)
        self.assertIn('"--locked", "--manifest-path", "native/opennow-streamer/Cargo.toml"', runner)
        self.assertIn('tasks[-1].extend(["--parallel", str(jobs)])', runner)
        self.assertIn('name: native-check-diagnostics-${{ inputs.label }}', checks)
        self.assertNotIn("build/qt-packages", checks)
        for phase in ("lint", "build", "test"):
            self.assertIn(f'run-native-checks.py {phase} --parallel "$CARGO_BUILD_JOBS"', checks)
        for setup in ("name: Install Qt", "name: Build and cache SDL3 test dependency", "name: Cache complete Rust build state"):
            self.assertLess(checks.index(setup), checks.index("    - name: Lint Rust"))
        self.assertIn('"$QT_ROOT_DIR/bin/qmlformat"', checks)
        self.assertNotIn("ensure-windows-test-desktop.ps1", checks)
        self.assertIn("ensure-windows-media-foundation.ps1", checks)
        cmake = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        targets = re.search(r"set\(OPENNOW_CI_UNIT_TEST_TARGETS\s+(.*?)\)", cmake, re.DOTALL)[1].split()
        self.assertEqual(len(targets), 37)
        self.assertEqual(len(set(targets)), 37)
        self.assertIn("opennow-windowsmoveloop-tests", targets)
        self.assertIn("opennow-generichidinput-tests", targets)
        self.assertIn("opennow-windowtheme-tests", targets)
        self.assertIn("opennow-updatefailure-tests", targets)
        self.assertIn('add_test(NAME opennow-updatefailure-tests COMMAND opennow-updatefailure-tests', cmake)
        self.assertIn('-input "${CMAKE_CURRENT_SOURCE_DIR}/tests/qml-updater"', cmake)
        self.assertIn("opennow-applicationicons-tests", targets)
        self.assertIn("opennow-streampresenttimings-tests", targets)
        self.assertIn("opennow-fsrupscaler-tests", targets)
        self.assertRegex(cmake, r'if\(WIN32 OR CMAKE_SYSTEM_NAME STREQUAL "Linux"\)\s+'
                         r'set_tests_properties\(opennow-frameinterpolator-tests opennow-fsrupscaler-tests\s+'
                         r'PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen"\)')
        self.assertIn("opennow-tenbitwarning-tests", targets)
        self.assertIn("opennow-consoleactions-tests", targets)
        self.assertIn("opennow-controllernavigation-tests", targets)
        self.assertIn("opennow-graphicsdevices-tests", targets)
        self.assertIn("opennow-consolelayout-tests", targets)
        self.assertIn("opennow-consolecontrols-tests", targets)
        self.assertIn("opennow-consolelaunch-tests", targets)
        self.assertIn("opennow-consolelaunchinput-tests", targets)
        self.assertIn("opennow-macawdl-tests", targets)
        self.assertIn("opennow-controllericons-tests", targets)
        self.assertIn("opennow-streamtoasts-tests", targets)
        self.assertIn("opennow-waylandhdroutput-tests", targets)
        self.assertIn("opennow-queueselector-tests", targets)
        for forbidden in ("opennow-qt", "opennow-streamvideo-tests", "opennow-nativestreamruntime-tests",
                          "opennow-nativeframegeneration-tests", "opennow-linuxvulkangraphics-tests"):
            self.assertNotIn(forbidden, targets)
        self.assertIn("add_custom_target(opennow-ci-unit-tests DEPENDS ${OPENNOW_CI_UNIT_TEST_TARGETS})", cmake)
        self.assertIn('set_tests_properties(${OPENNOW_CI_UNIT_TEST_TARGETS} PROPERTIES LABELS "ci-unit")', cmake)
        self.assertIn('ENVIRONMENT "QT_QPA_PLATFORM=cocoa" RUN_SERIAL TRUE TIMEOUT 30 LABELS "interactive-desktop"', cmake)

    def test_windows_move_loop_regression_runs_in_the_unit_lane_without_a_desktop(self):
        cmake = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        targets = re.search(r"set\(OPENNOW_CI_UNIT_TEST_TARGETS\s+(.*?)\)", cmake, re.DOTALL)[1].split()
        self.assertIn("opennow-windowsmoveloop-tests", targets)
        registration = cmake.split("qt_add_executable(opennow-windowsmoveloop-tests", 1)[1]
        registration = registration.split("qt_add_executable(opennow-waylandpointer-tests", 1)[0]
        self.assertIn("tests/tst_windowswindowmoveloop.cpp", registration)
        self.assertIn("opennow-platform-input Qt6::Test", registration)
        self.assertIn("add_test(NAME opennow-windowsmoveloop-tests COMMAND opennow-windowsmoveloop-tests", registration)
        self.assertIn('Testing/Temporary/windows-move-loop-results.xml,junitxml', registration)
        self.assertRegex(registration, r'if\(WIN32\)\s+'
                         r'add_test\(NAME opennow-windowsmoveloop-native-tests\s+'
                         r'COMMAND opennow-streamvideo-tests nativeWindowMoveLoopOwnsTheCursorUntilItExits -o -,txt\s+'
                         r'-o "\$\{CMAKE_CURRENT_BINARY_DIR\}/Testing/Temporary/native-stream-item-move-loop-results.xml,junitxml"\)')
        self.assertIn('set_tests_properties(opennow-windowsmoveloop-native-tests PROPERTIES\n'
                      '            ENVIRONMENT "QT_QPA_PLATFORM=windows;QT_QUICK_BACKEND=software"\n'
                      '            RUN_SERIAL TRUE TIMEOUT 30 LABELS "ci-unit")', registration)
        self.assertRegex(cmake, r'if\(WIN32\)\s+'
                         r'add_dependencies\(opennow-ci-unit-tests opennow-streamvideo-tests\)')
        self.assertRegex(registration, r'set_tests_properties\(opennow-windowsmoveloop-tests PROPERTIES\s+'
                         r'ENVIRONMENT "QT_QPA_PLATFORM=windows" RUN_SERIAL TRUE TIMEOUT 30\)')
        native = (ROOT / "opennow-qt/tests/tst_windowswindowmoveloop.cpp").read_text()
        self.assertIn("void sentSystemCommandsSuspendBeforeQtDispatch()", native)
        self.assertIn('QCOMPARE(QGuiApplication::platformName(), QStringLiteral("windows"))', native)
        self.assertIn("SendMessageW(hwnd, WM_SYSCOMMAND, command, position)", native)
        self.assertIn("QVERIFY(!IsWindowVisible(hwnd))", native)
        self.assertNotIn("QSKIP", native)

    def test_signed_helper_integration_runs_on_every_platform_check_not_signers(self):
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        marker = "    - name: Test Rust\n"
        before, after = action.split(marker, 1)
        step = after.split("    - name:", 1)[0]
        for setup in ("uses: ilammy/msvc-dev-cmd@v1", "uses: dtolnay/rust-toolchain@stable",
                      "name: Install Linux test dependencies", "name: Build Rust and Qt tests concurrently"):
            self.assertIn(setup, before)
        self.assertNotIn("      if:", step)
        self.assertNotIn("continue-on-error", step)
        self.assertIn("working-directory: ${{ env.OPENNOW_CHECKOUT }}", step)
        self.assertIn("CARGO_BUILD_JOBS: ${{ inputs.parallel }}", step)
        self.assertIn('run-native-checks.py test --parallel "$CARGO_BUILD_JOBS"', step)
        runner = (ROOT / ".github/scripts/run-native-checks.py").read_text()
        self.assertIn('["cargo", "test", *core], [sys.executable, "opennow-qt/tests/run_update_helper_integration.py"]', runner)
        self.assertIn('env["OPENNOW_UPDATE_TEST_TARGET_DIR"] = str(Path(environment["OPENNOW_CHECKOUT"]) / "native/opennow-core/target")', runner)
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
        self.assertIn("key: windows-nasm-3.2.0", action)
        self.assertIn("choco install nasm --version=3.2.0 -y --no-progress", action)
        self.assertIn("NASM_CACHE_HIT: ${{ steps.windows-nasm.outputs.cache-hit }}", action)
        self.assertIn("NASM installation failed with exit code", action)
        self.assertIn("Expected NASM 3.02 from Chocolatey package 3.2.0", action)
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
                    if runner == "macos-15-intel":
                        self.assertIn(workflow.name, ("qt-build.yml", "qt-release-candidate.yml"))
                        self.assertIn("            macos-arch: x86_64\n", workflow.read_text())
                        continue
                    self.assertTrue(
                        runner.startswith(("blacksmith-", "${{")),
                    )

    def test_rust_caches_survive_job_renames_and_later_test_failures(self):
        for path in (ROOT / ".github/actions/qt-unit-tests/action.yml", WORKFLOWS / "qt-build.yml"):
            with self.subTest(path=path):
                cache = re.split(r"\n\s+- (?:name:|uses:)", path.read_text().split("uses: ./.github/actions/rust-build-cache", 1)[1], 1)[0]
                self.assertIn("shared-key:", cache)
                self.assertIn("native/opennow-core -> target", cache)
                self.assertIn("native/opennow-streamer -> target", cache)
        action = (ROOT / ".github/actions/rust-build-cache/action.yml").read_text()
        self.assertIn("uses: useblacksmith/stickydisk@", action)
        self.assertIn("commit: ${{ steps.identity.outputs.save }}", action)
        self.assertIn("uses: actions/cache/restore@", action)
        self.assertIn("restore-keys: ${{ steps.identity.outputs.restore-keys }}", action)
        self.assertNotIn("Swatinem/rust-cache", action)

    def test_qt_compile_cache_restores_previous_timestamped_entries(self):
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        cache = action.split("uses: hendrikmuhs/ccache-action@", 1)[1].split("\n    - name:", 1)[0]
        self.assertIn("key: qt-checks-${{ inputs.label }}", cache)
        self.assertIn("restore-keys: qt-checks-${{ inputs.label }}", cache)
        self.assertIn("verbose: 1", cache)
        cmake = (ROOT / "opennow-qt/CMakeLists.txt").read_text()
        self.assertIn("cmake_policy(SET CMP0141 NEW)", cmake)
        self.assertIn('set(CMAKE_MSVC_DEBUG_INFORMATION_FORMAT "$<$<CONFIG:Debug,RelWithDebInfo>:Embedded>")', cmake)
        tests = (ROOT / "opennow-qt/cmake/Tests.cmake").read_text()
        self.assertIn("set_property(TARGET opennow-hdrcolor-tests PROPERTY MSVC_DEBUG_INFORMATION_FORMAT Embedded)", tests)
        self.assertIn("target_compile_options(opennow-hdrcolor-tests PRIVATE /Zi)", tests)

    def test_package_compile_caches_restore_their_own_timestamped_entries(self):
        for workflow_name, job_names in (
            ("qt-build.yml", ("packages",)),
            ("qt-release-candidate.yml", ("linux", "windows", "macos")),
        ):
            workflow_jobs = jobs((WORKFLOWS / workflow_name).read_text())
            for job_name in job_names:
                with self.subTest(workflow=workflow_name, job=job_name):
                    cache = workflow_jobs[job_name].split("uses: hendrikmuhs/ccache-action@", 1)[1].split("\n      -", 1)[0]
                    key = re.search(r"^\s+key: (.+)$", cache, re.MULTILINE).group(1)
                    self.assertIn(f"restore-keys: {key}\n", cache)
                    self.assertIn("verbose: 1", cache)
                    self.assertNotIn("append-timestamp: false", cache)

    def test_release_candidates_reuse_the_nightly_package_caches(self):
        def block(job, action):
            return job.split(f"uses: {action}", 1)[1].split("\n      -", 1)[0]

        def key_expression(cache, name):
            return re.search(rf"^\s+{name}: (.+)$", cache, re.MULTILINE).group(1)

        nightly = jobs((WORKFLOWS / "qt-build.yml").read_text())["packages"]
        candidates = jobs((WORKFLOWS / "qt-release-candidate.yml").read_text())

        # Resolve the nightly package job's own key expression per matrix entry,
        # so a rename in either workflow has to move both sides together.
        nightly_rust = block(nightly, "./.github/actions/rust-build-cache")
        self.assertEqual(key_expression(nightly_rust, "shared-key"),
                         key_expression(block(nightly, "hendrikmuhs/ccache-action"), "key"))
        expression = key_expression(nightly_rust, "shared-key")
        macos = re.search(r"runner\.os == 'macOS' && format\('(\S+)', matrix\.label\)", expression)
        template = re.search(r"format\('(\S+)', matrix\.os, matrix\.label\)", expression)
        self.assertIsNotNone(macos, expression)
        self.assertIsNotNone(template, expression)
        platforms = {
            label: macos[1].replace("{0}", label) if label.startswith("macos-")
            else template[1].replace("{0}", image).replace("{1}", label)
            for label, image in re.findall(r"- label: (\S+)\n\s+os: (\S+)\n", nightly)
        }
        self.assertEqual(set(platforms), {
            "linux-x64", "linux-arm64", "windows-x64", "windows-arm64", "macos-arm64", "macos-x64",
        })
        # A candidate restores the nightly cache only when its own key pattern
        # resolves to that platform's key.
        linux = key_expression(block(candidates["linux"], "./.github/actions/rust-build-cache"), "shared-key")
        windows = key_expression(block(candidates["windows"], "./.github/actions/rust-build-cache"), "shared-key")
        self.assertEqual({
            **{
                f"linux-{arch}": linux.replace("${{ matrix.runner }}", runner)
                                      .replace("${{ matrix.arch }}", arch)
                for arch, runner in re.findall(r"- arch: (\S+)\n\s+runner: (\S+)\n", candidates["linux"])
            },
            **{
                f"windows-{arch}": windows.replace("${{ matrix.arch }}", arch)
                for arch in re.findall(r"- arch: (\S+)\n", candidates["windows"])
            },
            **{
                f"macos-{arch}": key_expression(block(candidates["macos"], "./.github/actions/rust-build-cache"), "shared-key")
                                .replace("${{ matrix.arch }}", arch)
                for arch in re.findall(r"- arch: (\S+)\n", candidates["macos"])
            },
        }, platforms)

        nightly_workspaces = nightly_rust.split("workspaces: |", 1)[1].strip()
        self.assertIn("native/opennow-core -> ../../build/opennow-qt-release/rust-target", nightly_workspaces)
        self.assertIn("native/opennow-streamer -> ../../build/opennow-qt-release/peer-probe-rust-target", nightly_workspaces)
        for name in ("linux", "windows", "macos"):
            with self.subTest(candidate=name):
                rust = block(candidates[name], "./.github/actions/rust-build-cache")
                ccache = block(candidates[name], "hendrikmuhs/ccache-action")
                self.assertEqual(key_expression(rust, "shared-key"), key_expression(ccache, "key"))
                self.assertEqual(rust.split("workspaces: |", 1)[1].strip(), nightly_workspaces)
                self.assertEqual(key_expression(ccache, "restore-keys"), key_expression(ccache, "key"))
                self.assertIn("max-size: 1G", ccache)

        self.assertEqual(re.search(r"cache-key-prefix: (qt-target-.+)$", nightly, re.MULTILINE).group(1),
                         "qt-target-${{ matrix.label }}")
        for name, prefix in (("linux", "qt-target-linux-${{ matrix.arch }}"),
                             ("windows", "qt-target-windows-${{ matrix.arch }}"),
                             ("macos", "qt-target-macos-${{ matrix.arch }}")):
            with self.subTest(candidate=name):
                self.assertIn(f"cache-key-prefix: {prefix}\n", candidates[name])

    def test_relocated_core_probe_matches_shell_protocol(self):
        header = (ROOT / "opennow-qt/src/core/CoreClient.h").read_text()
        version = int(re.search(r"CurrentProtocolVersion = (\d+);", header).group(1))
        core = (ROOT / "native/opennow-core/src/main.rs").read_text()
        self.assertEqual(int(re.search(r"const PROTOCOL_VERSION: i64 = (\d+);", core).group(1)), version)
        client = (ROOT / "opennow-qt/src/core/CoreClient.cpp").read_text()
        required = re.search(r"for \(const auto &capability : \{(.*?)\}\)", client, re.DOTALL)[1]
        capabilities = re.findall(r'u"([^"]+)"_s', required)
        self.assertTrue(capabilities)
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (WORKFLOWS / name).read_text()
                probe = workflow.split('"id":"package-core"', 1)[1].split("core.stdin.flush()", 1)[0]
                self.assertEqual(int(re.search(r'"protocolVersion":(\d+)', probe).group(1)), version)
                assertion = next(line for line in workflow.splitlines() if '.issubset(response["result"]["capabilities"])' in line)
                for capability in capabilities:
                    self.assertIn(f'"{capability}"', assertion)

    def test_native_streamer_probes_match_shell_protocol(self):
        protocol = (ROOT / "native/opennow-streamer/crates/opennow-streamer-protocol/src/lib.rs").read_text()
        version = int(re.search(r"pub const PROTOCOL_VERSION: u64 = (\d+);", protocol).group(1))
        shell = (ROOT / "opennow-qt/qml/state/ShellStore.qml").read_text()
        self.assertEqual(int(re.search(r"nativeProtocolVersion: (\d+)", shell).group(1)), version)
        core = (ROOT / "native/opennow-core/src/streamer.rs").read_text()
        self.assertEqual(int(re.search(r"const STREAMER_PROTOCOL_VERSION: u64 = (\d+);", core).group(1)), version)
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (WORKFLOWS / name).read_text()
                probe = workflow.split('"id":"package-probe"', 1)[1].split("communicate(", 1)[0]
                self.assertEqual(int(re.search(r'"protocolVersion":\s*(\d+)', probe).group(1)), version)
        for name in ("verify_linux_package.py", "verify_flatpak.py"):
            with self.subTest(package=name):
                probe = (ROOT / "opennow-qt/packaging" / name).read_text()
                self.assertEqual(int(re.search(r'"protocolVersion":\s*(\d+)', probe).group(1)), version)

    def test_publishing_remains_explicitly_opt_in_after_build(self):
        ci = (WORKFLOWS / "qt-ci.yml").read_text()
        self.assertIn("        type: boolean\n        default: false", ci)
        publish = jobs(ci)["publish-nightly"]
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.publish_nightly", publish)
        self.assertIn("    needs: [contracts, checks, build, sign-nightly]\n", publish)


if __name__ == "__main__":
    unittest.main()
