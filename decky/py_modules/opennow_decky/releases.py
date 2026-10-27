import platform
import re
import subprocess
from urllib.parse import unquote, urlparse

from . import host
from .install import version_key
from .net import http_json

LATEST_RELEASE = "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases/latest"
RECENT_RELEASES = "https://api.github.com/repos/OpenCloudGaming/OpenNOW/releases?per_page=30"
ARCHITECTURES = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}
GITHUB_JSON = {"Accept": "application/vnd.github+json"}
MAX_MANIFEST_BYTES = 64 * 1024
MAX_BUNDLE_BYTES = 4 * 1024 * 1024 * 1024
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?(?:\+[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?")


def release_asset_url(url: str, tag: str, name: str) -> bool:
    parsed = urlparse(url)
    return (parsed.scheme == "https" and parsed.netloc == "github.com" and not parsed.query and not parsed.fragment
            and unquote(parsed.path) == f"/OpenCloudGaming/OpenNOW/releases/download/{tag}/{name}")


def device_machine() -> str:
    try:
        result = host.run(["flatpak", "--default-arch"], timeout=15)
    except (OSError, subprocess.SubprocessError):
        return platform.machine()
    return (result.returncode == 0 and result.stdout.strip()) or platform.machine()


def fetch_latest(nightly: bool = False, timeout: float = 20) -> dict:
    if not nightly:
        return http_json(LATEST_RELEASE, headers=GITHUB_JSON, timeout=timeout, hosts=("api.github.com",))
    release = latest_nightly(http_json(RECENT_RELEASES, headers=GITHUB_JSON, timeout=timeout, hosts=("api.github.com",)))
    if not release:
        raise ValueError("No nightly with a Flatpak for this device has been published")
    return release


def latest_nightly(releases: list, machine: str | None = None) -> dict | None:
    machine = machine or device_machine()
    return next((release for release in releases
                 if release.get("prerelease") and not release.get("draft")
                 and "-nightly." in (release.get("tag_name") or "")
                 and flatpak_asset(release, machine)), None)


def flatpak_asset(release: dict, machine: str | None = None) -> dict | None:
    arch = ARCHITECTURES.get((machine or device_machine()).lower())
    if not arch:
        return None
    tag = release.get("tag_name") or ""
    version = tag.removeprefix("v")
    if not VERSION.fullmatch(version):
        return None
    name = f"OpenNOW-Qt-{version}-Linux-{arch}.flatpak"
    assets = release.get("assets") or []
    bundles = [asset for asset in assets if asset.get("name") == name]
    manifests = [asset for asset in assets if asset.get("name") == name + ".manifest.json"]
    if len(bundles) != 1 or len(manifests) != 1:
        return None
    bundle, manifest = bundles[0], manifests[0]
    for asset, limit in ((bundle, MAX_BUNDLE_BYTES), (manifest, MAX_MANIFEST_BYTES)):
        if (type(asset.get("size")) is not int or not 0 < asset["size"] <= limit
                or not release_asset_url(asset.get("browser_download_url") or "", tag, asset["name"])):
            return None
    return {"tag": tag, "version": version, "architecture": arch, "name": name,
            "url": bundle["browser_download_url"], "size": bundle["size"],
            "manifest": {"name": manifest["name"], "url": manifest["browser_download_url"], "size": manifest["size"]}}


def summarize(release: dict, installed_version: str | None, machine: str | None = None) -> dict:
    version = str(release.get("tag_name") or "").removeprefix("v")
    return {
        "version": version,
        "asset": flatpak_asset(release, machine),
        "installedVersionKnown": bool(installed_version),
        "updateAvailable": bool(installed_version) and version_key(version) > version_key(installed_version),
    }
