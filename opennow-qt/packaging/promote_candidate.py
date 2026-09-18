import argparse
import json
from pathlib import Path
import re
import shutil
import tempfile

from nightly_release import expected_packages
from sign_nightly_release import digest, verify_manifests


def validate_provenance(run, run_id, repository, commit):
    if (not re.fullmatch(r"[0-9a-f]{40}", commit)
            or not re.fullmatch(r"[1-9][0-9]*", run_id)
            or run.get("id") != int(run_id)
            or run.get("path") != ".github/workflows/qt-release-candidate.yml"
            or run.get("event") != "workflow_dispatch"
            or run.get("head_sha") != commit
            or run.get("status") != "completed"
            or run.get("conclusion") != "success"
            or run.get("repository", {}).get("full_name", "").lower() != repository.lower()
            or run.get("head_repository", {}).get("full_name", "").lower() != repository.lower()):
        raise ValueError("Expected a successful production candidate run at the exact release commit")


def assemble(source, destination, version, commit, public_key):
    assets = expected_packages(version, commit, "stable") | {f"OpenNOW-Qt-{version}-Darwin-arm64.zip"}
    expected = assets | {name + ".manifest.json" for name in assets}
    files = {}
    paths = {}
    if source.is_symlink() or not source.is_dir():
        raise ValueError("Expected a regular candidate directory")
    for path in source.rglob("*"):
        if path.is_symlink():
            raise ValueError("Candidate contains a symbolic link")
        if not path.is_file():
            continue
        if not path.stat().st_size:
            raise ValueError("Candidate contains an empty file")
        paths["candidate/" + path.relative_to(source).as_posix()] = path
        if path.name in ("SHA256SUMS", "RELEASE-CANDIDATE-INVENTORY.txt"):
            continue
        if path.name not in expected or path.name in files:
            raise ValueError("Unexpected or duplicate candidate asset")
        files[path.name] = path
    if files.keys() != expected:
        raise ValueError("Missing candidate asset or manifest")
    inventory = source / "RELEASE-CANDIDATE-INVENTORY.txt"
    lines = inventory.read_text().splitlines()
    if (len(lines) < 4 or lines[:3] != ["OpenNOW Qt release candidate",
                                      f"version={version}", f"sourceCommit={commit}"]
            or lines[3] not in ("windowsSigningMode=unsigned", "windowsSigningMode=authenticode")):
        raise ValueError("Candidate inventory identity does not match the release")
    checksums = {f"{digest(path)}  {name}" for name, path in paths.items() if path != inventory}
    if len(lines[4:]) != len(checksums) or set(lines[4:]) != checksums:
        raise ValueError("Candidate checksums do not match its complete inventory")
    if destination.exists() or destination.is_symlink():
        raise ValueError("Promotion destination must not exist")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".opennow-promotion-", dir=destination.parent) as directory:
        staged = Path(directory) / "release"
        staged.mkdir()
        for name, path in files.items():
            shutil.copyfile(path, staged / name)
        asset_info = [{"name": name, "size": (staged / name).stat().st_size,
                       "sha256": digest(staged / name)} for name in sorted(assets)]
        verify_manifests(staged, version, asset_info, public_key)
        metadata = {"version": version, "sourceCommit": commit,
                    "platformSigning": "macos-developer-id", "windowsSigningMode": lines[3].split("=", 1)[1],
                    "updates": "signed-manifest", "assets": asset_info}
        (staged / "RELEASE-INFO.json").write_text(json.dumps(metadata, indent=2) + "\n")
        (staged / "SHA256SUMS").write_text("".join(
            f"{digest(path)}  {path.name}\n" for path in sorted(staged.iterdir())))
        staged.rename(destination)


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    provenance = commands.add_parser("provenance")
    provenance.add_argument("--run", type=Path, required=True)
    provenance.add_argument("--run-id", required=True)
    provenance.add_argument("--repository", required=True)
    provenance.add_argument("--commit", required=True)
    promote = commands.add_parser("assemble")
    promote.add_argument("--source", type=Path, required=True)
    promote.add_argument("--destination", type=Path, required=True)
    promote.add_argument("--version", required=True)
    promote.add_argument("--commit", required=True)
    args = parser.parse_args()
    if args.command == "provenance":
        validate_provenance(json.loads(args.run.read_text()), args.run_id, args.repository, args.commit)
    else:
        public_key = Path(__file__).with_name("update-public-key.base64").read_text().strip()
        assemble(args.source, args.destination, args.version, args.commit, public_key)


if __name__ == "__main__":
    main()
