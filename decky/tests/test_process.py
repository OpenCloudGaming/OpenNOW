import fcntl
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from dataclasses import replace
from unittest import mock

from tests import support  # noqa: F401
from opennow_decky import process
from opennow_decky.install import Install


class LockHolderTests(unittest.TestCase):
    def setUp(self):
        self.data_dir = Path(tempfile.mkdtemp())

    def test_reads_a_real_flock_from_the_kernel(self):
        lock = self.data_dir / "core.lock"
        lock.write_text("")
        self.assertIsNone(process.lock_holder(self.data_dir))
        child = subprocess.Popen([sys.executable, "-c",
                                  "import fcntl, sys\n"
                                  "handle = open(sys.argv[1]); fcntl.flock(handle, fcntl.LOCK_EX)\n"
                                  "print('ready', flush=True); sys.stdin.readline()", str(lock)],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.addCleanup(child.stdout.close)
        child.stdout.readline()
        self.assertEqual(process.lock_holder(self.data_dir), child.pid)
        child.communicate("\n", timeout=5)
        self.assertIsNone(process.lock_holder(self.data_dir))

    def test_checking_does_not_take_the_lock(self):
        lock = self.data_dir / "core.lock"
        lock.write_text("")
        process.lock_holder(self.data_dir)
        with open(lock) as handle:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_missing_lock_file_means_not_running(self):
        self.assertIsNone(process.lock_holder(self.data_dir))

    def test_waiters_and_other_inodes_are_ignored(self):
        lock = self.data_dir / "core.lock"
        lock.write_text("")
        status = os.stat(lock)
        device = f"{os.major(status.st_dev):02x}:{os.minor(status.st_dev):02x}"
        locks = self.data_dir / "locks"
        locks.write_text(
            f"1: FLOCK  ADVISORY  WRITE 111 {device}:{status.st_ino + 1} 0 EOF\n"
            f"1: -> FLOCK  ADVISORY  WRITE 222 {device}:{status.st_ino} 0 EOF\n"
            f"2: POSIX  ADVISORY  WRITE 333 {device}:{status.st_ino} 0 EOF\n"
        )
        self.assertIsNone(process.lock_holder(self.data_dir, str(locks)))
        locks.write_text(locks.read_text() + f"3: FLOCK  ADVISORY  WRITE 444 {device}:{status.st_ino} 0 EOF\n")
        self.assertEqual(process.lock_holder(self.data_dir, str(locks)), 444)

    def test_core_lock_refuses_while_the_core_holds_it_and_blocks_the_core_while_writing(self):
        with open(self.data_dir / "core.lock", "w") as core:
            fcntl.flock(core, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(process.CoreRunning):
                with process.core_lock(self.data_dir):
                    pass
            fcntl.flock(core, fcntl.LOCK_UN)
            with process.core_lock(self.data_dir):
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(core, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(core, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def test_the_plugins_own_settings_lock_is_never_reported_or_killed(self):
        with process.core_lock(self.data_dir):
            self.assertIsNone(process.lock_holder(self.data_dir))
            self.assertTrue(process.released(self.data_dir, 0.2))

    def test_a_holder_that_is_not_opennow_is_waited_for_not_killed(self):
        lock = self.data_dir / "core.lock"
        lock.write_text("")
        child = subprocess.Popen([sys.executable, "-c",
                                  "import fcntl, sys, time\n"
                                  "handle = open(sys.argv[1]); fcntl.flock(handle, fcntl.LOCK_EX)\n"
                                  "print('ready', flush=True); time.sleep(2)", str(lock)],
                                 stdout=subprocess.PIPE, text=True)
        self.addCleanup(child.stdout.close)
        child.stdout.readline()
        self.assertFalse(process.released(self.data_dir, 0.3))
        self.assertIsNone(child.poll())
        self.assertEqual(child.wait(timeout=5), 0)


class QuitRequestTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.install = Install("native", "/usr/bin/opennow-qt", None, None,
                               "/usr/bin/opennow-qt", (), self.directory.name)

    def test_completed_request_waits_for_core_lock_release(self):
        with mock.patch.object(process.host, "run", return_value=subprocess.CompletedProcess([], 0, "opennow-quit-complete\n")) as run, \
                mock.patch.object(process, "released", return_value=True) as released:
            self.assertTrue(process.request_quit(self.install))
        run.assert_called_once_with(["/usr/bin/opennow-qt", "--request-quit"], timeout=125)
        released.assert_called_once_with(Path(self.directory.name), 5)

    def test_cancel_is_not_a_success_and_does_not_wait_or_kill(self):
        with mock.patch.object(process.host, "run", return_value=subprocess.CompletedProcess([], 2)), \
                mock.patch.object(process, "released") as released, mock.patch.object(process.os, "kill") as kill:
            self.assertFalse(process.request_quit(self.install))
        released.assert_not_called()
        kill.assert_not_called()

    def test_timeout_or_unsupported_command_never_kills(self):
        for outcome in [subprocess.TimeoutExpired([], 125), subprocess.CompletedProcess([], 1)]:
            with self.subTest(outcome=outcome), mock.patch.object(process.host, "run") as run, \
                    mock.patch.object(process.os, "kill") as kill:
                if isinstance(outcome, Exception):
                    run.side_effect = outcome
                else:
                    run.return_value = outcome
                with self.assertRaisesRegex(RuntimeError, "The running app was not terminated"):
                    process.request_quit(self.install)
                kill.assert_not_called()

    def test_completion_marker_is_insufficient_while_core_is_running(self):
        with mock.patch.object(process.host, "run", return_value=subprocess.CompletedProcess([], 0, "opennow-quit-complete\n")), \
                mock.patch.object(process, "released", return_value=False), mock.patch.object(process.os, "kill") as kill:
            with self.assertRaisesRegex(RuntimeError, "did not finish closing"):
                process.request_quit(self.install)
        kill.assert_not_called()

    def test_appimage_helpers_receive_the_install_argument_before_the_quit_flag(self):
        installed = replace(self.install, kind="appimage", exe="/usr/local/bin/droiddeck-appimage-run",
                            args=("/opt/appimages/user/Open NOW",))
        with mock.patch.object(process.host, "run", return_value=subprocess.CompletedProcess([], 0, "opennow-quit-complete\n")) as run:
            self.assertTrue(process.request_quit(installed))
        run.assert_called_once_with([
            "/usr/local/bin/droiddeck-appimage-run", "/opt/appimages/user/Open NOW", "--request-quit",
        ], timeout=125)

    def test_old_app_without_the_completion_marker_is_never_treated_as_closed(self):
        with mock.patch.object(process.host, "run", return_value=subprocess.CompletedProcess([], 0, "")), \
                mock.patch.object(process, "released", return_value=True), mock.patch.object(process.os, "kill") as kill:
            with self.assertRaisesRegex(RuntimeError, "did not finish closing"):
                process.request_quit(self.install)
        kill.assert_not_called()


if __name__ == "__main__":
    unittest.main()
