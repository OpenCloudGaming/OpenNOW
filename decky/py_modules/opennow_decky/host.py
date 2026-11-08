import os
import subprocess


def host_environment() -> dict[str, str]:
    environment = dict(os.environ)
    original = environment.pop("LD_LIBRARY_PATH_ORIG", None)
    if original:
        environment["LD_LIBRARY_PATH"] = original
    else:
        environment.pop("LD_LIBRARY_PATH", None)
    return environment


def run(command: list[str], timeout: float) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, timeout=timeout, check=False, env=host_environment())
