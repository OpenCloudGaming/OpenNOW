import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import stat
import subprocess
import time
import uuid


ABI_VARIABLES = re.compile(
    r"^(RUSTFLAGS|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_ENCODED_RUSTFLAGS|"
    r"CARGO_BUILD_TARGET|CARGO_BUILD_RUSTFLAGS|CARGO_BUILD_RUSTC|CARGO_BUILD_RUSTC_WRAPPER|"
    r"CARGO_TARGET_.*_(LINKER|RUSTFLAGS)|CARGO_PROFILE_.*|"
    r"(CC|CXX|AR|LD|CFLAGS|CXXFLAGS|CPPFLAGS|ARFLAGS|LDFLAGS)(_.+)?|"
    r"HOST_(CC|CXX|AR|CFLAGS|CXXFLAGS)|TARGET_(CC|CXX|AR|CFLAGS|CXXFLAGS)|"
    r"SDKROOT|MACOSX_DEPLOYMENT_TARGET|IPHONEOS_DEPLOYMENT_TARGET|"
    r"LIBCLANG_PATH|BINDGEN_EXTRA_CLANG_ARGS.*|PKG_CONFIG.*|"
    r"VCToolsVersion|VCToolsInstallDir|WindowsSDKVersion|WindowsSdkDir|"
    r"VSCMD_ARG_TGT_ARCH|INCLUDE|LIB|LIBPATH|OPENNOW_MACOS_ARCH|SDL_VERSION)$"
)
TOOL_VARIABLES = re.compile(r"^((CC|CXX|AR|LD)(_.+)?|HOST_(CC|CXX|AR)|TARGET_(CC|CXX|AR)|CARGO_TARGET_.*_LINKER)$")


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()[:32]


def file_digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def command_output(command, required=False):
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, errors="replace", timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        if required:
            raise RuntimeError(f"Required tool unavailable: {command[0]}") from None
        return "unavailable"
    if required and result.returncode:
        raise RuntimeError(f"Required tool failed: {command[0]}")
    return result.stdout.strip()


def workspace_targets(root, workspaces):
    targets = set()
    roots = set()
    for line in workspaces.splitlines():
        if not line.strip():
            continue
        parts = line.split("->")
        if len(parts) != 2 or not all(part.strip() for part in parts):
            raise ValueError("Expected workspace -> relative target path")
        workspace, target = (Path(part.strip()) for part in parts)
        if workspace.is_absolute() or target.is_absolute():
            raise ValueError("Workspace and target mappings must be relative")
        workspace = Path(os.path.abspath(root / workspace))
        target = Path(os.path.abspath(workspace / target))
        if not workspace.is_relative_to(root) or not target.is_relative_to(root) or target == root:
            raise ValueError("Cache mappings must stay inside the checkout")
        if not (workspace / "Cargo.toml").is_file():
            raise ValueError(f"Workspace manifest not found: {workspace}")
        roots.add(workspace)
        targets.add(target)
    if not targets:
        raise ValueError("At least one workspace target is required")
    for target in targets:
        if any(workspace.is_relative_to(target) for workspace in roots):
            raise ValueError("A target must not contain a source workspace")
        if any(other != target and other.is_relative_to(target) for other in targets):
            raise ValueError("Cache target directories must not overlap")
        if target.parent.resolve() != target.parent:
            raise ValueError("Cache target parents must not contain symlinks")
    return sorted(roots), sorted(targets)


def tool_identity(env):
    tools = {
        "cargo": ["--version"], "cc": ["--version"], "c++": ["--version"],
        "clang": ["--version"], "clang++": ["--version"], "ar": ["--version"],
        "ld": ["--version"], "cmake": ["--version"], "ninja": ["--version"],
        "make": ["--version"], "nasm": ["-v"], "pkg-config": ["--version"],
    }
    if env["RUNNER_OS"] == "Windows":
        tools.update({"cl.exe": [], "link.exe": ["/?"], "lib.exe": ["/?"]})
    for key, value in env.items():
        if TOOL_VARIABLES.fullmatch(key) and value:
            command = value if Path(value).is_file() else shlex.split(value)[0]
            tools[command] = [] if Path(command).name.lower() in {"cl", "cl.exe"} else ["--version"]
    result = {"rustc": command_output([env.get("RUSTC", "rustc"), "-vV"], required=True)}
    for tool, args in sorted(tools.items()):
        executable = shutil.which(tool)
        result[tool] = None if executable is None else {
            "path": executable,
            "binary": file_digest(Path(executable)),
            "version": command_output([executable, *args]),
        }
    if env["RUNNER_OS"] == "Linux":
        result["native-packages"] = command_output([
            "dpkg-query", "-W", "-f=${binary:Package}=${Version}\n",
            "libclang*", "libc6*", "libstdc++*", "*-dev",
        ])
    if env["RUNNER_OS"] == "macOS":
        result["sdk"] = command_output(["xcrun", "--show-sdk-version"], required=True)
        result["xcode"] = command_output(["xcodebuild", "-version"], required=True)
    if env.get("LIBCLANG_PATH"):
        path = Path(env["LIBCLANG_PATH"])
        result["libclang"] = {
            item.name: file_digest(item)
            for item in sorted(path.glob("*clang*.dll")) if item.is_file()
        }
    return result


def trusted_writer(env, commit):
    return (
        env.get("RUST_CACHE_SAVE_IF", "true").lower() == "true"
        and env.get("GITHUB_EVENT_NAME") in {"push", "workflow_dispatch"}
        and env.get("GITHUB_REF") in {"refs/heads/dev", "refs/heads/main"}
        and commit == env.get("GITHUB_SHA")
    )


def cache_keys(env, identity, lockfiles, commit):
    namespace = digest({"repository": env["GITHUB_REPOSITORY"],
                        "shared-key": env["RUST_CACHE_SHARED_KEY"]})
    prefix = f"opennow-rust-v1-{env['RUNNER_OS']}-{env['RUNNER_ARCH']}-{namespace}-{digest(identity)}-"
    locked_prefix = f"{prefix}{digest(lockfiles)}-"
    return {
        "disk-key": prefix.rstrip("-"),
        "archive-key": f"{locked_prefix}{commit}",
        "restore-keys": f"{locked_prefix}\n{prefix}",
    }


def write_outputs(values, destination):
    with destination.open("a", encoding="utf-8") as output:
        for key, value in values.items():
            delimiter = f"CACHE_{uuid.uuid4().hex}"
            output.write(f"{key}<<{delimiter}\n{value}\n{delimiter}\n")


def prepare(env):
    started = time.time()
    root = Path(env["GITHUB_WORKSPACE"]).resolve()
    workspaces, targets = workspace_targets(root, env["RUST_CACHE_WORKSPACES"])
    cargo_home = Path(env.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    paths = [cargo_home / "registry", cargo_home / "git", *targets]
    identity = {
        "tools": tool_identity(env),
        "abi": {key: value for key, value in env.items() if ABI_VARIABLES.fullmatch(key) and value},
        "targets": [str(path.relative_to(root)) for path in targets],
        "recipe": file_digest(Path(__file__)),
        "vendored-inputs": command_output([
            "git", "-C", str(root), "ls-tree", "HEAD", "--", "native/opennow-streamer/vendor",
        ], required=True),
        "cargo-config": {
            str(path): file_digest(path)
            for directory in [cargo_home, root / ".cargo", *(workspace / ".cargo" for workspace in workspaces)]
            for path in [directory / "config", directory / "config.toml"] if path.is_file()
        },
    }
    lockfiles = {str(path.relative_to(root)): file_digest(path)
                 for workspace in workspaces for path in [workspace / "Cargo.lock"] if path.is_file()}
    commit = command_output(["git", "-C", str(root), "rev-parse", "HEAD"], required=True)
    keys = cache_keys(env, identity, lockfiles, commit)
    values = {**keys, "save": str(trusted_writer(env, commit)).lower(),
              "paths": "\n".join(str(path) for path in paths)}
    write_outputs(values, Path(env["GITHUB_OUTPUT"]))
    disk = Path(env["RUNNER_TEMP"]) / "opennow-rust-cache"
    links = [{"source": str(path), "destination": str(disk / "cargo" / path.name)}
             for path in paths[:2]]
    links.extend({"source": str(path),
                  "destination": str(disk / "targets" / digest(str(path.relative_to(root))))}
                 for path in targets)
    plan = {"started": started, "links": links, "save": values["save"], "disk-key": keys["disk-key"]}
    (Path(env["RUNNER_TEMP"]) / "opennow-rust-cache-plan.json").write_text(json.dumps(plan))
    print(f"Rust cache identity prepared in {time.time() - started:.2f}s; writer={values['save']}")


def link_directory(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError("Persistent cache directory must not be a symlink")
    destination.mkdir(exist_ok=True)
    source.parent.mkdir(parents=True, exist_ok=True)
    if source.is_symlink():
        if source.resolve() != destination.resolve():
            raise ValueError("Refusing to replace an unrelated cache symlink")
        return
    if source.exists():
        if not source.is_dir():
            raise ValueError("Cache source must be a directory")
        shutil.copytree(source, destination, dirs_exist_ok=True, symlinks=True)
        shutil.rmtree(source)
    source.symlink_to(destination, target_is_directory=True)


def restore_source_mtimes(root, manifest, targets):
    started = time.monotonic()
    hydrated = time.time_ns()
    try:
        previous = json.loads(manifest.read_text()) if not manifest.is_symlink() else {}
    except (OSError, ValueError):
        previous = {}
    if not isinstance(previous, dict):
        previous = {}
    previous = {
        name: record for name, record in previous.items()
        if isinstance(name, str) and isinstance(record, dict)
        and isinstance(record.get("sha256"), str) and re.fullmatch(r"[a-f0-9]{64}", record["sha256"])
        and type(record.get("mode")) is int
        and type(record.get("mtime_ns")) is int and 0 <= record["mtime_ns"] < 2**63
    }
    tracked = subprocess.check_output([
        "git", "-C", str(root), "ls-files", "-z", "--",
        "native/opennow-core", "native/opennow-streamer",
    ])
    excluded = [root / "native/opennow-core/target", root / "native/opennow-streamer/target", *targets]
    current = {}
    restored = 0
    for name in tracked.split(b"\0"):
        if not name:
            continue
        relative = os.fsdecode(name)
        path = root / relative
        if any(path.is_relative_to(target) for target in excluded):
            continue
        if any(parent.is_symlink() for parent in path.parents if parent.is_relative_to(root)):
            continue
        try:
            before = path.lstat()
        except OSError:
            continue
        if not stat.S_ISREG(before.st_mode):
            continue
        checksum = file_digest(path)
        cached = previous.get(relative)
        if (isinstance(cached, dict) and cached.get("sha256") == checksum
                and cached.get("mode") == stat.S_IMODE(before.st_mode)
                and type(cached.get("mtime_ns")) is int and 0 <= cached["mtime_ns"] < 2**63):
            os.utime(path, ns=(before.st_atime_ns, cached["mtime_ns"]), follow_symlinks=False)
            restored += 1
        else:
            os.utime(path, ns=(before.st_atime_ns, max(before.st_mtime_ns, hydrated)), follow_symlinks=False)
        current[relative] = {"sha256": checksum, "mode": stat.S_IMODE(before.st_mode),
                             "mtime_ns": path.lstat().st_mtime_ns}
    for relative in previous.keys() - current.keys():
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts or not path.parts[:2] in {
            ("native", "opennow-core"), ("native", "opennow-streamer"),
        }:
            continue
        for parent in (root / path).parents:
            if parent == root:
                break
            if not (parent / "Cargo.toml").is_file():
                continue
            build_script = parent / "build.rs"
            name = str(build_script.relative_to(root))
            if name in current:
                before = build_script.lstat()
                fresh = max(before.st_mtime_ns, hydrated)
                os.utime(build_script, ns=(before.st_atime_ns, fresh), follow_symlinks=False)
                current[name]["mtime_ns"] = fresh
            break
    manifest.parent.mkdir(parents=True, exist_ok=True)
    temporary = manifest.with_name(f".source-mtimes-{uuid.uuid4().hex}.tmp")
    with temporary.open("x", encoding="utf-8") as output:
        json.dump(current, output, sort_keys=True)
    temporary.replace(manifest)
    print(f"Rust source timestamps: reused {restored}/{len(current)} regular tracked files "
          f"in {time.monotonic() - started:.2f}s")


def main():
    parser = argparse.ArgumentParser(description="Prepare persistent full-target Cargo caches")
    parser.add_argument("command", choices=("prepare", "link", "diagnostics"))
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(os.environ)
        return
    plan = json.loads((Path(os.environ["RUNNER_TEMP"]) / "opennow-rust-cache-plan.json").read_text())
    if args.command == "link":
        for entry in plan["links"]:
            link_directory(Path(entry["source"]), Path(entry["destination"]))
        restore_source_mtimes(
            Path(os.environ["GITHUB_WORKSPACE"]).resolve(),
            Path(os.environ["RUNNER_TEMP"]) / "opennow-rust-cache/source-mtimes.json",
            [Path(entry["source"]) for entry in plan["links"]],
        )
    else:
        print(f"Rust cache setup: {time.time() - plan['started']:.2f}s, "
              f"{len(plan['links'])} directories, writer={plan['save']}, key={plan['disk-key']}")


if __name__ == "__main__":
    main()
