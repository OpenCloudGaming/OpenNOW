import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("native_checks_runner", ROOT / ".github/scripts/run-native-checks.py")
RUNNER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RUNNER
SPEC.loader.exec_module(RUNNER)


class NativeChecksRunnerTest(unittest.TestCase):
    def test_budgets_are_bounded_and_keep_every_lane(self):
        self.assertEqual(RUNNER.budgets(8, 3), [3, 3, 2])
        self.assertEqual(RUNNER.budgets(3, 3), [1, 1, 1])
        with self.assertRaises(ValueError):
            RUNNER.budgets(2, 3)

    def test_builds_use_separate_workspaces_and_keep_all_tests(self):
        lanes = RUNNER.make_lanes("build", 8, {"RUNNER_OS": "Linux", "RUNNER_TEMP": "/runner/temp"})
        self.assertEqual([lane.name for lane in lanes], ["core", "streamer", "qt"])
        self.assertEqual(sum(int(lane.environment["CARGO_BUILD_JOBS"]) for lane in lanes), 8)
        self.assertIn("--no-run", lanes[0].commands[0])
        self.assertIn("--workspace", lanes[1].commands[0])
        self.assertIn("opennow-ci-unit-tests", lanes[2].commands[1])
        self.assertIn("--timings", lanes[0].commands[0])

    def test_qt_configuration_keeps_platform_toolchains(self):
        windows = RUNNER.make_lanes("build", 8, {"RUNNER_OS": "Windows", "RUNNER_TEMP": "C:/temp"})[-1]
        self.assertIn("-DCMAKE_CXX_COMPILER=cl", windows.commands[0])
        mac = RUNNER.make_lanes("build", 3, {
            "RUNNER_OS": "macOS", "RUNNER_TEMP": "/temp", "TARGET_ARCH": "arm64",
            "MACOSX_DEPLOYMENT_TARGET": "13.0",
        })[-1]
        self.assertIn("-DCMAKE_OSX_ARCHITECTURES=arm64", mac.commands[0])
        self.assertIn("-DCMAKE_OSX_DEPLOYMENT_TARGET=13.0", mac.commands[0])

    def test_core_tests_finish_before_signed_helper_cleans_their_target(self):
        lanes = RUNNER.make_lanes("test", 6, {"OPENNOW_CHECKOUT": "/checkout"})
        core = lanes[0]
        self.assertEqual(core.commands[0][:3], ["cargo", "test", "--locked"])
        self.assertIn("run_update_helper_integration.py", core.commands[1][-1])
        self.assertEqual(core.environment["OPENNOW_UPDATE_TEST_TARGET_DIR"], "/checkout/native/opennow-core/target")
        self.assertEqual(lanes[1].commands[0][-1], "--workspace")
        qt = lanes[2].commands[0]
        self.assertIn("ci-unit", qt)
        self.assertIn("--no-tests=error", qt)
        self.assertIn("qt-unit-tests.xml", qt)
        self.assertNotIn("--ignored", lanes[0].commands[0])

    def test_test_execution_shares_existing_cores_without_oversubscription(self):
        for total, expected in ((16, [4, 4, 8]), (8, [2, 2, 4]), (6, [2, 2, 2]), (4, [1, 1, 2])):
            with self.subTest(cores=total):
                lanes = RUNNER.make_lanes("test", total, {"OPENNOW_CHECKOUT": "/checkout"})
                self.assertEqual([int(lane.environment["RUST_TEST_THREADS"]) for lane in lanes], expected)
                self.assertEqual(sum(expected), total)

    @patch("builtins.print")
    def test_independent_lanes_overlap(self, _print):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            commands = []
            for own, other in (("left", "right"), ("right", "left")):
                program = (
                    "from pathlib import Path; import time; "
                    f"Path({str(directory / own)!r}).touch(); "
                    "deadline=time.monotonic()+5\n"
                    f"while not Path({str(directory / other)!r}).exists():\n"
                    " if time.monotonic()>deadline: raise RuntimeError('lanes ran serially')\n"
                    " time.sleep(0.01)\n"
                )
                commands.append(RUNNER.Lane(own, [[sys.executable, "-c", program]], dict(os.environ)))
            self.assertEqual(RUNNER.run_lanes(commands, directory / "logs"), 0)

    @patch("builtins.print")
    def test_failure_is_reported_without_skipping_other_lanes(self, _print):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            marker = directory / "completed"
            forbidden = directory / "must-not-run"
            lanes = [
                RUNNER.Lane("fails", [[sys.executable, "-c", "raise SystemExit(7)"],
                    [sys.executable, "-c", f"open({str(forbidden)!r}, 'w').close()"]], dict(os.environ)),
                RUNNER.Lane("passes", [[sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"]], dict(os.environ)),
            ]
            self.assertEqual(RUNNER.run_lanes(lanes, directory / "logs"), 1)
            self.assertTrue(marker.exists())
            self.assertFalse(forbidden.exists())
            self.assertTrue((directory / "logs/fails.log").is_file())

    @patch("builtins.print")
    def test_spawn_failure_is_not_a_pass(self, _print):
        with tempfile.TemporaryDirectory() as temporary:
            lane = RUNNER.Lane("missing", [[str(Path(temporary) / "missing-executable")]], dict(os.environ))
            self.assertEqual(RUNNER.run_lanes([lane], Path(temporary) / "logs"), 1)

    @unittest.skipIf(os.name == "nt", "POSIX process-group interruption")
    def test_interrupt_stops_the_active_process_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            marker = directory / "pid"
            child = (
                "import os,time; from pathlib import Path; "
                f"marker=Path({str(marker)!r}); pending=marker.with_suffix('.pending'); "
                "pending.write_text(str(os.getpid())); pending.replace(marker); time.sleep(30)"
            )
            program = (
                "import importlib.util,sys;from pathlib import Path; "
                f"spec=importlib.util.spec_from_file_location('runner', {str(ROOT / '.github/scripts/run-native-checks.py')!r}); "
                "r=importlib.util.module_from_spec(spec);sys.modules[spec.name]=r;spec.loader.exec_module(r); "
                f"r.run_lanes([r.Lane('sleep', [[sys.executable, '-c', {child!r}]], dict(__import__('os').environ))], Path({str(directory / 'logs')!r}))"
            )
            process = subprocess.Popen([sys.executable, "-c", program], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                deadline = time.monotonic() + 5
                while not marker.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(marker.exists())
                pid = int(marker.read_text())
                process.send_signal(signal.SIGINT)
                self.assertNotEqual(process.wait(timeout=5), 0)
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    unittest.main()
