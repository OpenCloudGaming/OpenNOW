import sys
import types
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "py_modules"))
sys.path.insert(0, str(ROOT))


def selected_asset(version="1.0.4", arch="x64", size=6, manifest_size=512):
    tag = "v" + version
    name = f"OpenNOW-Qt-{version}-Linux-{arch}.flatpak"
    base = f"https://github.com/OpenCloudGaming/OpenNOW/releases/download/{tag}/"
    return {"tag": tag, "version": version, "architecture": arch, "name": name, "url": base + name, "size": size,
            "manifest": {"name": name + ".manifest.json", "url": base + name + ".manifest.json", "size": manifest_size}}


def github_release(version="1.0.4", architectures=("x64", "arm64")):
    assets = []
    for arch in architectures:
        selected = selected_asset(version, arch)
        assets.extend({"name": item["name"], "browser_download_url": item["url"], "size": item["size"]}
                      for item in (selected, selected["manifest"]))
    return {"tag_name": "v" + version, "assets": assets}


def install_fake_decky(home: Path) -> types.ModuleType:
    module = types.ModuleType("decky")
    module.DECKY_USER_HOME = str(home)
    module.DECKY_PLUGIN_SETTINGS_DIR = str(home / "settings")
    module.DECKY_PLUGIN_RUNTIME_DIR = str(home / "runtime")
    module.DECKY_PLUGIN_VERSION = "0.0.0-test"
    module.DECKY_PLUGIN_LOG = str(home / "plugin.log")
    module.emitted = []

    async def emit(event, *args):
        module.emitted.append((event, args))

    module.emit = emit
    module.logger = types.SimpleNamespace(info=print, warning=print, error=print)
    sys.modules["decky"] = module
    return module
