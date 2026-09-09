import argparse
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import subprocess
import zipfile


MANIFEST = "windows-test-bundle.json"
RUST_TARGET_DIRS = {"target", "rust-target", "streamer-rust-target"}


def discover_tests(build):
    result = subprocess.run(
        ["ctest", "--test-dir", str(build), "-C", "Release", "--show-only=json-v1"],
        check=True,
        capture_output=True,
        text=True,
    )
    names = sorted(test["name"] for test in json.loads(result.stdout)["tests"])
    if not names:
        raise ValueError("CTest discovered no tests")
    return names


def pack(build, output, revision):
    build = Path(build)
    runtime = build / "Release"
    if not runtime.is_dir():
        raise ValueError("Build is missing the Release runtime directory")
    if not (build / "CTestTestfile.cmake").is_file():
        raise ValueError("Build is missing the root CTestTestfile.cmake")
    files = {path for path in runtime.rglob("*") if path.is_file()}
    for directory, directories, filenames in os.walk(build):
        directories[:] = [name for name in directories if name not in RUST_TARGET_DIRS]
        if "CTestTestfile.cmake" in filenames:
            files.add(Path(directory) / "CTestTestfile.cmake")
    manifest = {"revision": revision, "tests": discover_tests(build)}
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        archive.writestr(MANIFEST, json.dumps(manifest, indent=2) + "\n")
        for path in sorted(files):
            archive.write(path, path.relative_to(build).as_posix())


def unpack(archive_path, build, revision):
    build = Path(build)
    with zipfile.ZipFile(archive_path) as archive:
        manifest = json.loads(archive.read(MANIFEST))
        if manifest["revision"] != revision:
            raise ValueError("Bundle source revision does not match the checkout revision")
        if not manifest["tests"]:
            raise ValueError("Bundle test inventory is empty")
        for entry in archive.infolist():
            name = entry.filename
            path = PurePosixPath(name)
            windows_path = PureWindowsPath(name)
            if (path.is_absolute() or windows_path.drive or windows_path.root
                    or "\\" in name or ":" in name
                    or any(part in {"", ".", ".."} or part.endswith((".", " "))
                           for part in name.rstrip("/").split("/"))
                    or not (build / path).resolve().is_relative_to(build.resolve())):
                raise ValueError(f"Unsafe archive path: {name}")
        archive.extractall(build)
    actual = discover_tests(build)
    expected = sorted(manifest["tests"])
    if actual != expected:
        raise ValueError(f"CTest test inventory mismatch: expected {expected!r}, got {actual!r}")


def main():
    parser = argparse.ArgumentParser(description="Transfer a prebuilt Windows Release CTest tree")
    commands = parser.add_subparsers(dest="command", required=True)
    pack_parser = commands.add_parser("pack")
    pack_parser.add_argument("--build", required=True, type=Path)
    pack_parser.add_argument("--output", required=True, type=Path)
    pack_parser.add_argument("--revision", required=True)
    unpack_parser = commands.add_parser("unpack")
    unpack_parser.add_argument("--archive", required=True, type=Path)
    unpack_parser.add_argument("--build", required=True, type=Path)
    unpack_parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    if args.command == "pack":
        pack(args.build, args.output, args.revision)
    else:
        unpack(args.archive, args.build, args.revision)


if __name__ == "__main__":
    main()
