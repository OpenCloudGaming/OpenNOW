import fcntl
import os
import subprocess
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

from . import host
from .install import Install

LOCK_NAME = "core.lock"


def process_name(pid: int, proc: str = "/proc") -> str:
    try:
        with open(f"{proc}/{pid}/comm", encoding="utf-8") as handle:
            return handle.read().strip()
    except OSError:
        return ""


def lock_holder(data_dir: Path, locks_path: str = "/proc/locks", proc: str = "/proc") -> int | None:
    try:
        status = os.stat(data_dir / LOCK_NAME)
        with open(locks_path, encoding="ascii") as handle:
            lines = handle.read().splitlines()
    except OSError:
        return None
    device = f"{os.major(status.st_dev):02x}:{os.minor(status.st_dev):02x}"
    for line in lines:
        fields = line.split()
        if len(fields) < 6 or "->" in fields or fields[1] != "FLOCK" or not fields[4].isdigit():
            continue
        lock_device, _, inode = fields[5].rpartition(":")
        if inode != str(status.st_ino):
            continue
        pid = int(fields[4])
        if pid == os.getpid():
            continue
        if lock_device.lower() == device or process_name(pid, proc).startswith("opennow"):
            return pid
    return None


class CoreRunning(RuntimeError):
    pass


@contextmanager
def core_lock(data_dir: Path) -> Iterator[None]:
    data_dir.mkdir(parents=True, exist_ok=True)
    with open(data_dir / LOCK_NAME, "a+") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise CoreRunning("Close OpenNOW to change stream settings") from None
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def released(data_dir: Path, timeout_seconds: float, poll_seconds: float = 0.1) -> bool:
    deadline = time.monotonic() + timeout_seconds
    while lock_holder(data_dir) is not None:
        if time.monotonic() >= deadline:
            return False
        time.sleep(poll_seconds)
    return True


def request_quit(install: Install) -> bool:
    try:
        result = host.run([install.exe, *install.args, "--request-quit"], timeout=125)
    except (OSError, subprocess.SubprocessError) as error:
        raise RuntimeError("OpenNOW did not finish closing. The running app was not terminated. Check OpenNOW before trying again.") from error
    if result.returncode == 2:
        return False
    if (result.returncode != 0 or "opennow-quit-complete" not in result.stdout.splitlines()
            or not released(Path(install.data_dir), 5)):
        raise RuntimeError("OpenNOW did not finish closing. Update OpenNOW if it does not support graceful shutdown. The running app was not terminated.")
    return True
