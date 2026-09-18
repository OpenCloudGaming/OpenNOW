import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess


def update_information(channel, arch):
    if channel not in ("stable", "nightly", "supporter") or arch not in ("x64", "arm64"):
        raise ValueError("Unsupported AppImage update channel or architecture")
    tag = "latest" if channel == "stable" else "latest-pre"
    version = "*" if channel == "stable" else f"*-{channel}.*"
    return (f"gh-releases-zsync|OpenCloudGaming|OpenNOW|{tag}|"
            f"OpenNOW-Qt-{version}-Linux-{arch}.AppImage.zsync")


def verify_zsync(appimage):
    sidecar = appimage.with_name(appimage.name + ".zsync")
    if sidecar.is_symlink() or not sidecar.is_file():
        raise ValueError("Missing regular AppImage zsync sidecar")
    with sidecar.open("rb") as stream:
        header = bytearray()
        while not header.endswith(b"\n\n") and len(header) < 65536:
            byte = stream.read(1)
            if not byte:
                break
            header.extend(byte)
        if not header.endswith(b"\n\n") or not stream.read(1):
            raise ValueError("Invalid zsync header or missing block checksums")
    fields = {}
    for line in header.decode("ascii").splitlines():
        if not line:
            continue
        name, separator, value = line.partition(": ")
        if not separator or name in fields:
            raise ValueError("Invalid or duplicate zsync field")
        fields[name] = value
    with appimage.open("rb") as stream:
        sha1 = hashlib.file_digest(stream, "sha1").hexdigest()
    expected = {"Filename": appimage.name, "URL": appimage.name,
                "Length": str(appimage.stat().st_size), "SHA-1": sha1}
    if not fields.get("zsync") or any(fields.get(key) != value for key, value in expected.items()):
        raise ValueError("zsync metadata does not match the versioned AppImage")


def verify(appimage, channel, arch):
    if not re.fullmatch(r"OpenNOW-Qt-[0-9A-Za-z.+-]+-Linux-" + arch + r"\.AppImage", appimage.name):
        raise ValueError("Expected a versioned AppImage filename")
    result = subprocess.run([str(appimage.resolve()), "--appimage-updateinformation"],
                            check=True, capture_output=True, text=True, timeout=30,
                            env={name: value for name, value in os.environ.items()
                                 if name != "APPIMAGE_EXTRACT_AND_RUN"})
    if result.stdout.strip() != update_information(channel, arch):
        raise ValueError("AppImage embedded update information does not match its channel")
    verify_zsync(appimage)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("information", "verify"))
    parser.add_argument("--channel", required=True, choices=("stable", "nightly", "supporter"))
    parser.add_argument("--arch", required=True, choices=("x64", "arm64"))
    parser.add_argument("--appimage", type=Path)
    args = parser.parse_args()
    if args.command == "information":
        print(update_information(args.channel, args.arch))
    elif args.appimage is None:
        parser.error("verify requires --appimage")
    else:
        verify(args.appimage, args.channel, args.arch)


if __name__ == "__main__":
    main()
