import base64
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import tempfile
from pathlib import Path
from threading import Event, Timer
from typing import Callable

from . import host
from .install import FLATPAK_ID
from .net import open_url, require_host
from .releases import MAX_BUNDLE_BYTES, MAX_MANIFEST_BYTES, VERSION, release_asset_url

DOWNLOAD_HOSTS = ("github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com")
FLATHUB = "https://dl.flathub.org/repo/flathub.flatpakrepo"
DROIDDECK_FLATPAK = "/usr/local/bin/droiddeck-flatpak"
INSTALL_TIMEOUT = 3600
CHUNK = 256 * 1024

Progress = Callable[[str, float], None]


class InstallError(RuntimeError):
    pass


class Cancelled(InstallError):
    pass


def pinned_public_key() -> bytes:
    packaged = Path(__file__).with_name("update-public-key.base64")
    source = Path(__file__).resolve().parents[3] / "opennow-qt/packaging/update-public-key.base64"
    paths = [path for path in (source, packaged) if path.is_file()]
    if not paths:
        raise InstallError("The plugin has no pinned update signing key; reinstall the plugin")
    encoded = [path.read_text().strip() for path in paths]
    if len(set(encoded)) != 1:
        raise InstallError("The source and packaged update signing keys differ")
    try:
        key = base64.b64decode(encoded[0], validate=True)
    except ValueError:
        raise InstallError("The pinned update signing key is invalid") from None
    if len(key) != 32:
        raise InstallError("The pinned update signing key has the wrong length")
    return key


def verify_manifest(data: bytes, selected: dict) -> dict:
    try:
        manifest = json.loads(data)
        if (set(manifest) != {"schemaVersion", "version", "asset", "size", "sha256", "signature"}
                or type(manifest["schemaVersion"]) is not int or manifest["schemaVersion"] != 1
                or not isinstance(manifest["version"], str)
                or manifest["version"].removeprefix("v") != selected["version"]
                or manifest["asset"] != selected["name"]
                or type(manifest["size"]) is not int or manifest["size"] != selected["size"]
                or not isinstance(manifest["sha256"], str)
                or not re.fullmatch(r"[0-9a-fA-F]{64}", manifest["sha256"])):
            raise ValueError("Manifest does not match the selected release")
        signature = base64.b64decode(manifest["signature"], validate=True)
        if len(signature) != 64:
            raise ValueError("Invalid signature length")
    except (ValueError, TypeError, KeyError) as error:
        raise InstallError(f"Invalid signed update manifest: {error}") from None
    environment = host.host_environment()
    openssl = shutil.which("openssl", path=environment.get("PATH"))
    if not openssl:
        raise InstallError("Host OpenSSL 3.0 or later with Ed25519 support is required to verify OpenNOW downloads. "
                           "Update SteamOS or the DroidDeck Linux runtime, then retry")
    payload = ("OpenNOW update manifest v1\n"
               f"version={manifest['version'].removeprefix('v')}\nasset={manifest['asset']}\n"
               f"size={manifest['size']}\nsha256={manifest['sha256'].lower()}\n").encode("ascii")
    with tempfile.TemporaryDirectory(prefix="opennow-verify-") as directory:
        root = Path(directory)
        (root / "public.der").write_bytes(bytes.fromhex("302a300506032b6570032100") + pinned_public_key())
        (root / "payload").write_bytes(payload)
        (root / "signature").write_bytes(signature)
        try:
            result = subprocess.run([openssl, "pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                                     "-inkey", str(root / "public.der"), "-in", str(root / "payload"),
                                     "-sigfile", str(root / "signature")], stdin=subprocess.DEVNULL,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15,
                                    check=False, env=environment)
        except (OSError, subprocess.SubprocessError):
            raise InstallError("OpenSSL could not verify the update signature") from None
    if result.returncode != 0:
        raise InstallError("Update signature is invalid or host OpenSSL lacks Ed25519 support")
    return manifest


def signed_manifest(selected: dict, cancelled: Event) -> dict:
    if cancelled.is_set():
        raise Cancelled("Install cancelled")
    try:
        tag, version, arch, name = (selected[key] for key in ("tag", "version", "architecture", "name"))
        sidecar = selected["manifest"]
        if (not VERSION.fullmatch(version) or tag.removeprefix("v") != version or arch not in ("x64", "arm64")
                or name != f"OpenNOW-Qt-{version}-Linux-{arch}.flatpak" or len(name) > 240 or ".." in name
                or type(selected["size"]) is not int or not 0 < selected["size"] <= MAX_BUNDLE_BYTES
                or sidecar["name"] != name + ".manifest.json" or type(sidecar["size"]) is not int
                or not 0 < sidecar["size"] <= MAX_MANIFEST_BYTES
                or not release_asset_url(selected["url"], tag, name)
                or not release_asset_url(sidecar["url"], tag, sidecar["name"])):
            raise ValueError("Missing or inconsistent selected release identity")
    except (KeyError, TypeError, ValueError):
        raise InstallError("This release has no valid signed Flatpak metadata") from None
    with open_url(sidecar["url"], timeout=30, hosts=DOWNLOAD_HOSTS) as response:
        length = response.headers.get("Content-Length")
        if length is not None and int(length) != sidecar["size"]:
            raise InstallError("Update manifest size does not match the selected release")
        data = response.read(MAX_MANIFEST_BYTES + 1)
    if len(data) != sidecar["size"] or len(data) > MAX_MANIFEST_BYTES:
        raise InstallError("Update manifest size does not match the selected release")
    return verify_manifest(data, selected)


def download(url: str, destination: Path, size: int, sha256: str, progress: Progress, cancelled: Event) -> None:
    require_host(url, DOWNLOAD_HOSTS)
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".part")
    if type(size) is not int or not 0 < size <= MAX_BUNDLE_BYTES:
        raise InstallError("Invalid signed download size")
    received, digest = 0, hashlib.sha256()
    try:
        with open_url(url, timeout=60, hosts=DOWNLOAD_HOSTS) as response, open(partial, "wb") as handle:
            length = response.headers.get("Content-Length")
            if length is not None and int(length) != size:
                raise InstallError("Download size does not match its signed manifest")
            while chunk := response.read(min(CHUNK, size - received + 1)):
                if cancelled.is_set():
                    raise Cancelled("Install cancelled")
                received += len(chunk)
                if received > size:
                    raise InstallError("Download exceeds its signed size")
                handle.write(chunk)
                digest.update(chunk)
                progress("download", received / size)
            if cancelled.is_set():
                raise Cancelled("Install cancelled")
        if received != size or digest.hexdigest() != sha256.lower():
            raise InstallError("Download size or SHA256 does not match its signed manifest")
        partial.replace(destination)
    finally:
        partial.unlink(missing_ok=True)


def run_checked(command: list[str], message: str, timeout: float = 60) -> None:
    try:
        result = host.run(command, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise InstallError(f"{message}: Flatpak didn't finish within {timeout:.0f} seconds") from None
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().splitlines()
        raise InstallError(f"{message}: {detail[-1] if detail else f'exit {result.returncode}'}")


def install_with_droiddeck(bundle: Path, progress: Progress) -> None:
    error = None
    expired = Event()
    with subprocess.Popen([DROIDDECK_FLATPAK, "install-bundle", str(bundle)], stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, text=True, env=host.host_environment(),
                          start_new_session=True) as child:
        watchdog = Timer(INSTALL_TIMEOUT, lambda: (expired.set(), os.killpg(child.pid, signal.SIGKILL)))
        watchdog.start()
        try:
            for line in child.stdout:
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if event.get("e") == "progress":
                    steps = max(int(event.get("n") or 1), 1)
                    done = max(int(event.get("i") or 1) - 1, 0) + float(event.get("percent") or 0) / 100
                    progress("install", min(done / steps, 1.0))
                elif event.get("e") == "error":
                    error = event.get("message")
            child.wait()
        finally:
            watchdog.cancel()
    if expired.is_set():
        raise InstallError(f"Flatpak couldn't install OpenNOW: it didn't finish within {INSTALL_TIMEOUT} seconds")
    if child.returncode != 0:
        raise InstallError(f"Flatpak couldn't install OpenNOW: {error or f'exit {child.returncode}'}")


def install_bundle(selected: dict, work_dir: Path, progress: Progress, cancelled: Event) -> None:
    manifest = signed_manifest(selected, cancelled)
    bundle = work_dir / selected["name"]
    try:
        download(selected["url"], bundle, manifest["size"], manifest["sha256"], progress, cancelled)
        if cancelled.is_set():
            raise Cancelled("Install cancelled")
        progress("install", 0.0)
        droiddeck = os.access(DROIDDECK_FLATPAK, os.X_OK)
        if not shutil.which("flatpak", path=host.host_environment().get("PATH")):
            raise InstallError("Set up Flatpak in DroidDeck's Store first" if droiddeck else "Flatpak isn't installed")
        run_checked(["flatpak", "remote-add", "--user", "--if-not-exists", "flathub", FLATHUB],
                    "Couldn't add Flathub for the KDE runtime")
        if droiddeck and host.run(["flatpak", "info", "--user", FLATPAK_ID], timeout=60).returncode != 0:
            install_with_droiddeck(bundle, progress)
        else:
            run_checked(["flatpak", "install", "--user", "--noninteractive", "-y", "--reinstall", str(bundle)],
                        "Flatpak couldn't install OpenNOW", timeout=INSTALL_TIMEOUT)
        run_checked(["flatpak", "info", "--user", FLATPAK_ID], "OpenNOW isn't installed after the Flatpak step")
        progress("install", 1.0)
    finally:
        bundle.unlink(missing_ok=True)
        bundle.with_name(bundle.name + ".part").unlink(missing_ok=True)
