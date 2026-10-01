import json
import os
from pathlib import Path
import subprocess
import tempfile


WORKSPACE = Path(__file__).resolve().parents[3]
FIXTURE = Path(__file__).resolve().parent / "fixtures/vaapi_mock.c"
PREFIX = "video::vaapi::tests::mocked_driver_h264_probe_"


def main():
    build = subprocess.run(
        ["cargo", "test", "--manifest-path", str(WORKSPACE / "Cargo.toml"),
         "-p", "opennow-streamer-platform-linux", "--no-default-features",
         "--features", "vaapi", "--no-run", "--message-format=json"],
        check=True, text=True, stdout=subprocess.PIPE,
    )
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    binaries = [artifact["executable"] for artifact in artifacts
                if artifact.get("reason") == "compiler-artifact"
                and artifact["target"]["name"] == "opennow_streamer_platform_linux"
                and artifact.get("executable")]
    if len(binaries) != 1:
        raise RuntimeError(f"expected one Linux unit-test binary, got {binaries}")
    with tempfile.TemporaryDirectory(prefix="opennow-vaapi-mock-") as directory:
        root = Path(directory)
        library = root / "mock-va.so"
        subprocess.run(["cc", "-shared", "-fPIC", str(FIXTURE), "-o", str(library)], check=True)
        dri = root / "dev/dri"
        dri.mkdir(parents=True)
        cases = [
            ("sparse render nodes", [129], {}, "accepts_a_usable_device"),
            ("first node lacks H.264", [128, 129], {"MOCK_FIRST_UNSUPPORTED": "1"},
             "accepts_a_usable_device"),
            ("first node rejects the constructor", [128, 129], {"MOCK_FIRST_TINY": "1"},
             "accepts_a_usable_device"),
            ("all nodes reject the constructor", [128, 129], {"MOCK_REJECT_TINY": "1"},
             "rejects_the_initial_context_without_panicking"),
            ("unguarded dependency still rejects the constructor", [128], {"MOCK_REJECT_TINY": "1"},
             "constructor_still_requires_the_initial_context"),
        ]
        for name, nodes, settings, test in cases:
            for node in dri.iterdir():
                node.unlink()
            for node in nodes:
                (dri / f"renderD{node}").touch()
            environment = {key: value for key, value in os.environ.items()
                           if not key.startswith("MOCK_") and key != "LD_PRELOAD"}
            environment.update(settings)
            result = subprocess.run(
                ["unshare", "-Urm", "sh", "-ec",
                 'mount --bind "$1" /dev; export LD_PRELOAD="$2"; exec "$3" "$4" --exact --ignored --nocapture',
                 "vaapi-mock", str(root / "dev"), str(library), binaries[0], PREFIX + test],
                env=environment, capture_output=True, text=True, timeout=60,
            )
            print(result.stdout + result.stderr, flush=True)
            result.check_returncode()
            if f"test {PREFIX + test} ... ok" not in result.stdout:
                raise RuntimeError(f"mock driver case did not execute its test: {name}")
            print(f"PASS: {name}", flush=True)


if __name__ == "__main__":
    main()
