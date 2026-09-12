import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def verify_capabilities(message):
    if message.get("type") != "ready":
        raise ValueError("The packaged probe did not return ready")
    backends = message["capabilities"]["videoBackends"]
    vaapi = next(backend for backend in backends if backend["backend"] == "vaapi")
    codecs = {codec["codec"]: codec for codec in vaapi["codecs"]}
    if "without the vaapi feature" in json.dumps(vaapi):
        raise ValueError("The packaged probe was built without native VAAPI")
    if set(codecs) != {"h264", "h265", "av1"}:
        raise ValueError("Unexpected native VAAPI codec contract")
    if codecs["h265"]["available"] or codecs["av1"]["available"]:
        raise ValueError("Native VAAPI must not advertise HEVC or AV1 support")
    software = next(backend for backend in backends if backend["backend"] == "ffmpeg")
    fallback_codecs = {codec["codec"]: codec["available"] for codec in software["codecs"]}
    if fallback_codecs != {"h264": True, "h265": True, "av1": True}:
        raise ValueError("The packaged FFmpeg software fallback is unavailable")


def verify_package(bin_dir):
    helper = bin_dir / "opennow-update-helper"
    if not helper.is_file() or not os.access(helper, os.X_OK):
        raise ValueError("The package is missing its executable update helper")
    for name in ("opennow-streamer", "libopennow_streamer_ffi.so"):
        binary = bin_dir / name
        dependencies = subprocess.check_output(["readelf", "-d", binary], text=True)
        for library in ("libva.so.2", "libva-drm.so.2"):
            if f"[{library}]" not in dependencies:
                raise ValueError(f"{name} is missing its {library} dependency")
        resolved = subprocess.check_output(["ldd", binary], text=True)
        if "not found" in resolved:
            raise ValueError(f"{name} has unresolved runtime dependencies:\n{resolved}")
    commands = [
        {"id": "package-probe", "type": "hello", "protocolVersion": 7},
        {"id": "package-shutdown", "type": "shutdown"},
    ]
    probe = subprocess.run(
        [bin_dir / "opennow-streamer"],
        input="".join(json.dumps(command) + "\n" for command in commands),
        text=True,
        capture_output=True,
        check=True,
        timeout=30,
        env={**os.environ, "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy"},
    )
    messages = [json.loads(line) for line in probe.stdout.splitlines()]
    verify_capabilities(next(message for message in messages if message.get("id") == "package-probe"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("bin_dir", type=Path)
    parser.add_argument("--deb", type=Path)
    args = parser.parse_args()
    verify_package(args.bin_dir.resolve())
    if args.deb:
        dependencies = subprocess.check_output(["dpkg-deb", "-f", args.deb, "Depends"], text=True)
        for dependency in ("libva2", "libva-drm2", "qt6-svg-plugins"):
            if not any(item.strip().split()[0] == dependency for item in dependencies.split(",")):
                raise ValueError(f"The DEB does not require {dependency}")
        with tempfile.TemporaryDirectory(prefix="opennow-deb-check-") as directory:
            subprocess.run(["dpkg-deb", "-x", args.deb, directory], check=True)
            verify_package(Path(directory) / "usr/bin")
    print("Linux package VAAPI H.264 and FFmpeg capability checks passed")
