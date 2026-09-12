import argparse
import base64
import binascii
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from nightly_release import expected_packages


def decode_key(value):
    try:
        key = base64.b64decode(value, validate=True)
    except (ValueError, binascii.Error):
        raise ValueError("Expected a canonical base64-encoded 32-byte Ed25519 key") from None
    if len(key) != 32 or base64.b64encode(key).decode() != value:
        raise ValueError("Expected a canonical base64-encoded 32-byte Ed25519 key")
    return key


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def payload(manifest):
    return ("OpenNOW update manifest v1\n"
            f"version={manifest['version']}\nasset={manifest['asset']}\n"
            f"size={manifest['size']}\nsha256={manifest['sha256']}\n").encode()


def openssl(*args):
    subprocess.run(["openssl", *map(str, args)], check=True, capture_output=True)


def validate_inventory(source, version, commit, signed=False):
    packages = expected_packages(version, commit)
    expected = packages | {"RELEASE-INFO.json", "SHA256SUMS"}
    if signed:
        expected |= {name + ".manifest.json" for name in packages}
    if source.is_symlink() or not source.is_dir():
        raise ValueError("Expected a regular inventory directory")
    entries = list(source.iterdir())
    if (any(path.is_symlink() or not path.is_file() or path.stat().st_size == 0 for path in entries)
            or {path.name for path in entries} != expected):
        raise ValueError("Unexpected, missing, empty, or linked inventory entry")
    metadata = json.loads((source / "RELEASE-INFO.json").read_text())
    inventory = [{"name": name, "size": (source / name).stat().st_size,
                  "sha256": digest(source / name)} for name in sorted(packages)]
    if metadata != {
        "version": version, "sourceCommit": commit, "platformSigning": "unsigned",
        "updates": "signed-manifest" if signed else "manual-download", "assets": inventory,
    }:
        raise ValueError("Release metadata does not match immutable package inventory")
    lines = (source / "SHA256SUMS").read_text().splitlines()
    expected_sums = {f"{digest(source / name)}  {name}" for name in expected - {"SHA256SUMS"}}
    if len(lines) != len(expected_sums) or set(lines) != expected_sums:
        raise ValueError("SHA256SUMS does not match complete inventory")
    return metadata


def verify(source, version, commit, public_key):
    key = decode_key(public_key)
    metadata = validate_inventory(source, version, commit, signed=True)
    with tempfile.TemporaryDirectory(prefix="opennow-update-verify-") as directory:
        root = Path(directory)
        public = root / "public.der"
        public.write_bytes(bytes.fromhex("302a300506032b6570032100") + key)
        for asset in metadata["assets"]:
            manifest = json.loads((source / (asset["name"] + ".manifest.json")).read_text())
            unsigned = {"schemaVersion": 1, "version": version, "asset": asset["name"],
                        "size": asset["size"], "sha256": asset["sha256"]}
            if set(manifest) != set(unsigned) | {"signature"} or any(
                    type(manifest[name]) is not type(value) or manifest[name] != value
                    for name, value in unsigned.items()):
                raise ValueError("Manifest does not match its exact package")
            signature = base64.b64decode(manifest["signature"], validate=True)
            if len(signature) != 64:
                raise ValueError("Expected a 64-byte Ed25519 signature")
            (root / "payload").write_bytes(payload(manifest))
            (root / "signature").write_bytes(signature)
            openssl("pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                    "-inkey", public, "-in", root / "payload", "-sigfile", root / "signature")


def sign(source, destination, version, commit, public_key, private_seed):
    key = decode_key(public_key)
    seed = decode_key(private_seed)
    metadata = validate_inventory(source, version, commit)
    if destination.exists() or destination.is_symlink():
        raise ValueError("Signing destination must not exist")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="opennow-update-signing-") as directory, \
            tempfile.TemporaryDirectory(prefix=".opennow-signed-", dir=destination.parent) as output:
        root = Path(directory)
        staged = Path(output) / "release"
        staged.mkdir()
        private = root / "private.der"
        private.write_bytes(bytes.fromhex("302e020100300506032b657004220420") + seed)
        private.chmod(0o600)
        openssl("pkey", "-inform", "DER", "-in", private, "-pubout", "-outform", "DER",
                "-out", root / "public.der")
        if (root / "public.der").read_bytes() != bytes.fromhex("302a300506032b6570032100") + key:
            raise ValueError("Signing seed does not match the pinned public key")
        for asset in metadata["assets"]:
            target = staged / asset["name"]
            shutil.copyfile(source / asset["name"], target)
            if target.stat().st_size != asset["size"] or digest(target) != asset["sha256"]:
                raise ValueError("Package changed after inventory validation")
            manifest = {"schemaVersion": 1, "version": version, "asset": asset["name"],
                        "size": asset["size"], "sha256": asset["sha256"]}
            (root / "payload").write_bytes(payload(manifest))
            openssl("pkeyutl", "-sign", "-rawin", "-keyform", "DER", "-inkey", private,
                    "-in", root / "payload", "-out", root / "signature")
            manifest["signature"] = base64.b64encode((root / "signature").read_bytes()).decode()
            (staged / (asset["name"] + ".manifest.json")).write_text(json.dumps(manifest, indent=2) + "\n")
        metadata["updates"] = "signed-manifest"
        (staged / "RELEASE-INFO.json").write_text(json.dumps(metadata, indent=2) + "\n")
        (staged / "SHA256SUMS").write_text("".join(
            f"{digest(path)}  {path.name}\n" for path in sorted(staged.iterdir())))
        verify(staged, version, commit, public_key)
        staged.rename(destination)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("preflight", "sign", "verify"))
    parser.add_argument("--source", type=Path)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--version")
    parser.add_argument("--commit")
    args = parser.parse_args()
    public_key = os.environ.get("OPENNOW_UPDATE_PUBLIC_KEY", "")
    if args.command == "preflight":
        if public_key or os.environ.get("PUBLISH_NIGHTLY") == "true":
            decode_key(public_key)
        return
    if not args.source or not args.version or not args.commit:
        parser.error("sign and verify require --source, --version, and --commit")
    if args.command == "sign":
        if not args.destination:
            parser.error("sign requires --destination")
        sign(args.source, args.destination, args.version, args.commit, public_key,
             os.environ.pop("OPENNOW_UPDATE_ED25519_PRIVATE_KEY", ""))
    else:
        verify(args.source, args.version, args.commit, public_key)


if __name__ == "__main__":
    main()
