import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil


def nightly_version(cmake_file, run, attempt, channel="nightly"):
    if channel not in ("nightly", "supporter"):
        raise ValueError("Invalid unsigned build channel")
    match = re.search(r"project\(OpenNOWQt VERSION (\d+\.\d+\.\d+) LANGUAGES", cmake_file.read_text())
    if not match or run < 1 or attempt < 1:
        raise ValueError("Expected a project version and positive run/attempt numbers")
    return f"{match[1]}-{channel}.{run}.{attempt}"


def expected_packages(version, commit, channel="nightly"):
    if channel not in ("nightly", "supporter"):
        raise ValueError("Invalid unsigned build channel")
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-" + channel + r"\.[1-9][0-9]*\.[1-9][0-9]*", version):
        raise ValueError(f"Invalid {channel} version")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected an immutable source commit")
    expected = {
        f"OpenNOW-Qt-{version}-{platform}-{arch}.{extension}"
        for arch in ("x64", "arm64")
        for platform, extension in (("Windows", "msi"), ("Windows", "zip"), ("Linux", "AppImage"), ("Linux", "deb"))
    }
    expected.add(f"OpenNOW-Qt-{version}-Darwin-arm64.dmg")
    return expected


def assemble(source, destination, version, commit, channel="nightly"):
    expected = expected_packages(version, commit, channel)
    files = {}
    for path in source.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"Artifact contains a symbolic link: {path}")
        if not path.is_file():
            continue
        if path.name not in expected or path.name in files or path.stat().st_size == 0:
            raise ValueError(f"Unexpected, duplicate, or empty artifact: {path}")
        files[path.name] = path
    if files.keys() != expected:
        raise ValueError(f"Missing release artifacts: {sorted(expected - files.keys())}")
    destination.mkdir(parents=True, exist_ok=False)
    inventory = []
    for name, path in sorted(files.items()):
        target = destination / name
        shutil.copyfile(path, target)
        with target.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        inventory.append({"name": name, "size": target.stat().st_size, "sha256": digest})
    metadata = destination / "RELEASE-INFO.json"
    metadata.write_text(json.dumps({
        "version": version,
        "sourceCommit": commit,
        "platformSigning": "unsigned",
        "updates": "manual-download",
        "assets": inventory,
    }, indent=2) + "\n")
    sums = [f"{asset['sha256']}  {asset['name']}\n" for asset in inventory]
    sums.append(f"{hashlib.sha256(metadata.read_bytes()).hexdigest()}  {metadata.name}\n")
    (destination / "SHA256SUMS").write_text("".join(sums))


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    version = commands.add_parser("version")
    version.add_argument("--cmake-file", type=Path, default=Path("opennow-qt/CMakeLists.txt"))
    version.add_argument("--run", type=int, required=True)
    version.add_argument("--attempt", type=int, required=True)
    version.add_argument("--channel", choices=("nightly", "supporter"), default="nightly")
    collect = commands.add_parser("assemble")
    collect.add_argument("--source", type=Path, required=True)
    collect.add_argument("--destination", type=Path, required=True)
    collect.add_argument("--version", required=True)
    collect.add_argument("--commit", required=True)
    collect.add_argument("--channel", choices=("nightly", "supporter"), default="nightly")
    args = parser.parse_args()
    if args.command == "version":
        print(nightly_version(args.cmake_file, args.run, args.attempt, args.channel))
    else:
        assemble(args.source, args.destination, args.version, args.commit, args.channel)


if __name__ == "__main__":
    main()
