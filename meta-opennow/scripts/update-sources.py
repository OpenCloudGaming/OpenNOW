#!/usr/bin/env python3

import argparse
import difflib
import hashlib
from pathlib import Path
import re
import subprocess
import tomllib


def render_crates(root):
    crates = {}
    git_packages = {}
    for workspace in ("opennow-core", "opennow-streamer"):
        lock = root / "native" / workspace / "Cargo.lock"
        for package in tomllib.loads(lock.read_text())["package"]:
            source = package.get("source", "")
            if source == "registry+https://github.com/rust-lang/crates.io-index":
                key = (package["name"], package["version"])
                checksum = package["checksum"]
                if key in crates and crates[key] != checksum:
                    raise ValueError(f"Conflicting checksums for {key}")
                crates[key] = checksum
            elif source.startswith("git+"):
                git_packages[package["name"]] = source
            elif source:
                raise ValueError(f"Unsupported Cargo source: {source}")
    if set(git_packages) != {"sdl2", "sdl2-sys"}:
        raise ValueError(f"Unexpected Git packages: {git_packages}")
    if git_packages["sdl2"] != git_packages["sdl2-sys"]:
        raise ValueError("SDL2 packages must share one source revision")
    match = re.fullmatch(
        r"git\+https://github.com/zortos293/rust-sdl2.git\?rev=([0-9a-f]{40})#\1",
        git_packages["sdl2"],
    )
    if not match:
        raise ValueError("Unexpected SDL2 source; update the fetch integration")
    lines = ['SRC_URI += " \\']
    for name, version in sorted(crates):
        lines.append(f"    crate://crates.io/{name}/{version} \\")
    lines.extend(['"', ""])
    for (name, version), checksum in sorted(crates.items()):
        lines.append(f'SRC_URI[{name}-{version}.sha256sum] = "{checksum}"')
    sdl = (
        'SRC_URI += "gitsm://github.com/zortos293/rust-sdl2.git;protocol=https;'
        'nobranch=1;name=sdl2;destsuffix=sdl2"\n'
        f'SRCREV_sdl2 = "{match[1]}"\n'
        'SRCREV_FORMAT = "default_sdl2"\n'
    )
    return {"opennow-crates.inc": "\n".join(lines) + "\n", "opennow-sdl.inc": sdl}


def main():
    parser = argparse.ArgumentParser(description="Regenerate pinned Yocto Cargo fetch metadata")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--revision", help="Pin an OpenNOW commit containing the current build inputs")
    args = parser.parse_args()
    if args.check and args.revision:
        parser.error("--check and --revision cannot be combined")
    root = Path(__file__).resolve().parents[2]
    recipes = root / "meta-opennow/recipes-games/opennow"
    outputs = render_crates(root)
    source_path = recipes / "opennow-source.inc"
    source = source_path.read_text()
    checksum = hashlib.md5((root / "LICENSE").read_bytes()).hexdigest()
    source = re.sub(r"file://LICENSE;md5=[^\"]+", f"file://LICENSE;md5={checksum}", source)
    if args.revision:
        revision = subprocess.check_output(
            ["git", "rev-parse", "--verify", f"{args.revision}^{{commit}}"], cwd=root, text=True
        ).strip()
        for path in ("native/opennow-core/Cargo.lock", "native/opennow-streamer/Cargo.lock"):
            committed = subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=root)
            if committed != (root / path).read_bytes():
                raise ValueError(f"{path} differs from revision {revision}")
        source = re.sub(r'^SRCREV = ".*"$', f'SRCREV = "{revision}"', source, flags=re.MULTILINE)
    outputs["opennow-source.inc"] = source
    stale = False
    for name, content in outputs.items():
        path = recipes / name
        current = path.read_text() if path.exists() else ""
        if current == content:
            continue
        if args.check:
            print("".join(difflib.unified_diff(current.splitlines(True), content.splitlines(True),
                                              fromfile=name, tofile="generated/" + name)), end="")
            stale = True
        else:
            path.write_text(content)
    if stale:
        raise SystemExit("Yocto metadata is stale; run meta-opennow/scripts/update-sources.py")


if __name__ == "__main__":
    main()
