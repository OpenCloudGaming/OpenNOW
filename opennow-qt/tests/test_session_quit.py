import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def wait_for(log, text, primary):
    deadline = time.monotonic() + 10
    while text not in log.read_text():
        assert primary.poll() is None, f"primary exited before {text}"
        assert time.monotonic() < deadline, f"fixture did not reach {text}"
        time.sleep(0.05)


def main():
    binary, surface, mode = sys.argv[1:]
    for stop_failure in [False, True]:
        with tempfile.TemporaryDirectory() as directory:
            environment = dict(os.environ, XDG_DATA_HOME=directory, XDG_CONFIG_HOME=directory)
            request_environment = dict(environment)
            for key in ["DISPLAY", "WAYLAND_DISPLAY", "QT_QPA_PLATFORM"]:
                request_environment.pop(key, None)
            log = Path(directory) / "primary.log"
            with log.open("w+") as output:
                primary = subprocess.Popen([
                    binary, "--smoke-test", "--smoke-stream-exit", "--smoke-exit-requested-quit",
                    "--" + surface, "--route", "stream", "--quit-after-session", "--reduced-motion",
                    *(["--fullscreen"] if mode == "fullscreen" else []),
                    *(["--smoke-exit-stop-failure"] if stop_failure else []),
                ], env=environment, stdout=output, stderr=subprocess.STDOUT)
                pending = None
                try:
                    wait_for(log, "SESSION_QUIT_READY", primary)
                    cancelled = subprocess.run([binary, "--request-quit"], env=request_environment, timeout=15)
                    assert cancelled.returncode == 2, f"cancel returned {cancelled.returncode}"
                    assert primary.poll() is None, "cancel closed the primary"
                    if stop_failure:
                        pending = subprocess.Popen([binary, "--request-quit"], env=request_environment,
                                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                        wait_for(log, "SESSION_QUIT_STOP_FAILED", primary)
                        time.sleep(0.3)
                        assert pending.poll() is None, "failed session.stop completed the quit request"
                        assert primary.poll() is None, "failed session.stop closed the primary"
                        pending.terminate()
                        stdout, _ = pending.communicate(timeout=5)
                        assert "opennow-quit-complete" not in stdout, "failed session.stop acknowledged completion"
                        wait_for(log, "SESSION_QUIT_FAILURE_RETAINED", primary)
                    else:
                        completed = subprocess.run([binary, "--request-quit"], env=request_environment, timeout=15,
                                                   capture_output=True, text=True)
                        assert completed.returncode == 0, f"completion returned {completed.returncode}"
                        assert completed.stdout.strip() == "opennow-quit-complete", "completion acknowledgement is missing"
                        assert "SESSION_QUIT_FINALIZED" in log.read_text(), "shutdown skipped recording completion"
                    assert primary.wait(timeout=15) == 0, "primary shutdown failed"
                finally:
                    if pending and pending.poll() is None:
                        pending.kill()
                        pending.communicate()
                    if primary.poll() is None:
                        primary.kill()
                        primary.wait()
                    print(log.read_text())


if __name__ == "__main__":
    main()
