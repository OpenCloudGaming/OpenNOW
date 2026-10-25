import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile


ROOT = Path(__file__).resolve().parents[2]
BUILD = Path("build/opennow-qt-release")
RUST_TARGETS = {"rust-target", "streamer-rust-target", "peer-probe-rust-target", "update-helper-rust-target"}


def inventory():
    result = subprocess.check_output([
        "ctest", "--test-dir", str(ROOT / BUILD), "--show-only=json-v1", "-LE", "interactive-desktop",
    ], text=True)
    return [test["name"] for test in json.loads(result)["tests"]]


def fingerprint(path):
    with path.open("rb") as source:
        header = source.read(20)
        if header.startswith(b"\x7fELF"):
            if header[4:6] != b"\x02\x01" or int.from_bytes(header[18:20], "little") != 183:
                raise ValueError(f"Not an ARM64 ELF: {path}")
        source.seek(0)
        return hashlib.file_digest(source, "sha256").hexdigest()


def pack(archive, version):
    files = []
    for directory, directories, names in os.walk(ROOT / BUILD):
        directories[:] = [name for name in directories if name not in RUST_TARGETS | {"CMakeFiles", "Testing"}]
        files.extend(Path(directory) / name for name in names if not name.endswith((".o", ".a")))
    rust = ROOT / BUILD / "streamer-rust-target/aarch64-unknown-linux-gnu/release"
    files.extend([rust / "libopennow_streamer_ffi.so", rust / "opennow-streamer"])
    sdl = Path(os.environ["RUNNER_TEMP"]) / "SDL-install"
    files.extend(path for path in sdl.rglob("*") if path.is_file())
    entries = {}
    with tarfile.open(archive, "w:gz", compresslevel=1, dereference=True) as target:
        for path in sorted(set(files)):
            name = str(Path("runtime/SDL-install") / path.relative_to(sdl)) if path.is_relative_to(sdl) else str(path.relative_to(ROOT))
            entries[name] = {"sha256": fingerprint(path), "mode": path.stat().st_mode & 0o777}
            target.add(path, arcname=name, recursive=False)
        manifest = {
            "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "version": version, "workspace": str(ROOT), "runner_temp": os.environ["RUNNER_TEMP"],
            "qt_root": os.environ["QT_ROOT_DIR"], "sdk": os.environ["OPENNOW_ARM64_SDK_ID"],
            "tests": inventory(), "files": entries,
        }
        content = json.dumps(manifest, indent=2).encode()
        info = tarfile.TarInfo("handoff.json")
        info.size = len(content)
        target.addfile(info, io.BytesIO(content))
    print(f"Packed {len(entries)} files and {len(manifest['tests'])} native ARM tests: {archive.stat().st_size} bytes")


def restore(archive, version):
    with tarfile.open(archive, "r:gz") as source:
        manifest = json.load(source.extractfile("handoff.json"))
        expected = {
            "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "version": version, "workspace": str(ROOT), "runner_temp": os.environ["RUNNER_TEMP"],
            "qt_root": os.environ["QT_ROOT_DIR"],
        }
        for key, value in expected.items():
            if manifest[key] != value:
                raise ValueError(f"Cross-build {key} mismatch: {manifest[key]!r} != {value!r}")
        members = source.getmembers()
        if len(members) != len(manifest["files"]) + 1 or len({member.name for member in members}) != len(members):
            raise ValueError("Unexpected or duplicate handoff archive entries")
        for member in members:
            if member.name == "handoff.json":
                continue
            path = Path(member.name)
            if (not member.isfile() or path.is_absolute() or ".." in path.parts
                    or member.name not in manifest["files"]
                    or not (path.is_relative_to(BUILD) or path.is_relative_to("runtime/SDL-install"))):
                raise ValueError(f"Unsafe handoff entry: {member.name}")
        for destination in (ROOT / BUILD, Path(os.environ["RUNNER_TEMP"]) / "SDL-install"):
            if destination.exists() or destination.is_symlink():
                raise ValueError(f"Restore requires a fresh destination: {destination}")
            if destination.parent.resolve() != destination.parent:
                raise ValueError(f"Restore refuses linked destination ancestors: {destination}")
        for member in members:
            if member.name == "handoff.json":
                continue
            path = Path(member.name)
            destination = (Path(os.environ["RUNNER_TEMP"]) / path.relative_to("runtime")) if path.is_relative_to("runtime") else ROOT / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as input, destination.open("wb") as output:
                shutil.copyfileobj(input, output)
            destination.chmod(manifest["files"][member.name]["mode"])
            if fingerprint(destination) != manifest["files"][member.name]["sha256"]:
                raise ValueError(f"Cross-build file checksum mismatch: {member.name}")
        if inventory() != manifest["tests"] or not manifest["tests"]:
            raise ValueError("Native ARM test inventory differs from the cross-build")
        print(f"Verified {len(manifest['files'])} files and {len(manifest['tests'])} native ARM tests")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("pack", "restore"))
    parser.add_argument("archive", type=Path)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    {"pack": pack, "restore": restore}[args.command](args.archive, args.version)
