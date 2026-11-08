import json
import os
from pathlib import Path
import subprocess
import tempfile


WORKSPACE = Path(__file__).resolve().parents[3]
FIXTURE = Path(__file__).resolve().parent / "fixtures/cuda_mock.c"
TEST = "video::cuda_probe::tests::mocked_driver_checks_caps_on_the_selected_context"


def main():
    build = subprocess.run(
        ["cargo", "test", "--release", "--manifest-path", str(WORKSPACE / "Cargo.toml"),
         "-p", "opennow-streamer-platform-linux", "--features", "ffmpeg-bundled",
         "--no-run", "--message-format=json"],
        check=True, text=True, stdout=subprocess.PIPE,
    )
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    binaries = [artifact["executable"] for artifact in artifacts
                if artifact.get("reason") == "compiler-artifact"
                and artifact["target"]["name"] == "opennow_streamer_platform_linux"
                and artifact.get("executable")]
    if len(binaries) != 1:
        raise RuntimeError(f"expected one Linux unit-test binary, got {binaries}")
    with tempfile.TemporaryDirectory(prefix="opennow-cuda-mock-") as directory:
        root = Path(directory)
        library = root / "libcuda.so.1"
        subprocess.run(
            ["cc", "-shared", "-fPIC", "-I",
             str(WORKSPACE / "vendor/ffmpeg-sys-next/bundled-headers/include"),
             str(FIXTURE), "-o", str(library)], check=True,
        )
        (root / "libnvcuvid.so.1").symlink_to(library.name)
        environment = dict(os.environ)
        environment["LD_LIBRARY_PATH"] = str(root) + os.pathsep + environment.get("LD_LIBRARY_PATH", "")
        environment.pop("LD_PRELOAD", None)
        result = subprocess.run(
            [binaries[0], TEST, "--exact", "--ignored", "--nocapture"],
            env=environment, capture_output=True, text=True, timeout=60,
        )
        print(result.stdout + result.stderr, flush=True)
        result.check_returncode()
        if f"test {TEST} ... ok" not in result.stdout:
            raise RuntimeError("CUDA mock test did not execute")


if __name__ == "__main__":
    main()
