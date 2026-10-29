import errno
import json
import os
import stat
from pathlib import Path
from typing import Any


def _open_regular(path: Path, flags: int) -> int:
    descriptor = os.open(path, flags | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise OSError(errno.EINVAL, "JSON destination must be a regular file without other links", path)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def read_json(path: Path) -> Any | None:
    try:
        with os.fdopen(_open_regular(path, os.O_RDONLY), encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return None


def _write_private(path: Path, data: bytes) -> None:
    with os.fdopen(_open_regular(path, os.O_WRONLY | os.O_CREAT), "wb") as handle:
        os.fchmod(handle.fileno(), 0o600)
        handle.truncate(0)
        handle.write(data)
        handle.flush()
        os.fsync(handle.fileno())


def write_json_atomic(path: Path, value: Any, keep_backup: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    backup = path.with_name(path.name + ".bak")
    for destination in (path, temporary, backup) if keep_backup else (path, temporary):
        try:
            descriptor = _open_regular(destination, os.O_RDONLY)
        except FileNotFoundError:
            continue
        with os.fdopen(descriptor, "rb") as handle:
            os.fchmod(handle.fileno(), 0o600)
    data = json.dumps(value, indent=2, ensure_ascii=False).encode("utf-8")
    _write_private(temporary, data)
    if keep_backup:
        try:
            with os.fdopen(_open_regular(path, os.O_RDONLY), "rb") as handle:
                previous = handle.read()
            json.loads(previous)
        except (FileNotFoundError, ValueError):
            pass
        else:
            _write_private(backup, previous)
    os.replace(temporary, path)
