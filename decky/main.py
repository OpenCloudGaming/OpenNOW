import asyncio
import base64
import os
import shlex
import threading
from dataclasses import replace
from pathlib import Path

import decky

from opennow_decky import account, installer, process, releases, stream_settings
from opennow_decky.catalog import Catalog, CatalogError, fetch_artwork
from opennow_decky.install import Install, detect
from opennow_decky.state import PluginState

CONSOLE_FLAG = "--console"
GAME_LAUNCH_FLAGS = ["--fullscreen", "--quit-after-session"]


def quoted(path: str) -> str:
    return '"' + path.replace('"', '\\"') + '"'


class Plugin:
    async def _main(self):
        self.home = Path(decky.DECKY_USER_HOME)
        self.state = PluginState(Path(decky.DECKY_PLUGIN_SETTINGS_DIR) / "state.json")
        self.catalog = Catalog(Path(decky.DECKY_PLUGIN_RUNTIME_DIR) / "catalog.json")
        self.catalog_lock = asyncio.Lock()
        self.release: dict | None = None
        self.install_task: asyncio.Task | None = None
        self.install_progress: dict | None = None
        self.install_cancel = threading.Event()
        asyncio.get_running_loop().create_task(self._warm_catalog())

    async def _unload(self):
        self.install_cancel.set()

    async def _warm_catalog(self):
        try:
            await self._ensure_catalog()
        except CatalogError as error:
            decky.logger.warning(str(error))

    async def _ensure_catalog(self, force: bool = False):
        async with self.catalog_lock:
            await asyncio.to_thread(self.catalog.ensure, force)

    def _install(self) -> Install | None:
        found = detect(self.home, self.state.custom_path)
        if found and found.version is None and found.revision in self.state.flatpak_versions:
            return replace(found, version=self.state.flatpak_versions[found.revision])
        return found

    def _data_dir(self) -> Path | None:
        install = self._install()
        return Path(install.data_dir) if install and install.data_dir else None

    async def get_status(self) -> dict:
        install = await asyncio.to_thread(self._install)
        data_dir = Path(install.data_dir) if install else None
        return {
            "install": install.to_dict() if install else None,
            "running": data_dir is not None and process.lock_holder(data_dir) is not None,
            "signedIn": data_dir is not None and account.signed_in(data_dir),
            "release": self.release,
            "installProgress": self.install_progress,
            "pluginVersion": decky.DECKY_PLUGIN_VERSION,
            "logPath": decky.DECKY_PLUGIN_LOG,
        }

    async def get_state(self) -> dict:
        return self.state.to_dict()

    async def set_preference(self, key: str, value: bool) -> dict:
        if key == "nightlyChannel" and value != self.state.preferences.get(key):
            self.release = None
        return self.state.set_preference(key, value)

    async def set_custom_path(self, path: str) -> dict:
        if path and not os.path.isfile(path):
            raise ValueError(f"{path} doesn't exist")
        return self.state.set_custom_path(path)

    async def set_launcher(self, app_id: int | None) -> dict:
        return self.state.set_launcher(app_id)

    async def add_game(self, app_id: int, variant_id: str, title: str, store: str, art: str) -> dict:
        return self.state.add_game(app_id, variant_id, title, store, art)

    async def remove_game(self, app_id: int) -> dict:
        return self.state.remove_game(app_id)

    async def launch_command(self, variant_id: str = "", title: str = "") -> dict:
        install = await asyncio.to_thread(self._install)
        if not install:
            raise RuntimeError("OpenNOW isn't installed")
        arguments = list(install.args)
        if self.state.preferences["consoleLayout"]:
            arguments.append(CONSOLE_FLAG)
        if variant_id:
            if not variant_id.isdigit():
                raise ValueError("Invalid GeForce NOW variant id")
            arguments.extend(GAME_LAUNCH_FLAGS)
            arguments.append(f"--launch-app-id={variant_id}")
            if title:
                arguments.append(f"--launch-title={title}")
        return {
            "exe": quoted(install.exe),
            "startDir": quoted(os.path.dirname(install.exe)),
            "options": shlex.join(arguments),
        }

    async def get_stream_settings(self) -> dict:
        data_dir = await asyncio.to_thread(self._data_dir)
        options = {
            "resolutions": list(stream_settings.RESOLUTIONS),
            "frameRates": list(stream_settings.FRAME_RATES),
            "codecs": list(stream_settings.CODECS),
            "bitrate": list(stream_settings.BITRATE_RANGE),
        }
        if data_dir is None:
            return {"values": dict(stream_settings.DEFAULTS), "options": options, "locked": True,
                    "error": "OpenNOW isn't installed"}
        try:
            values, error = stream_settings.read(data_dir), None
        except stream_settings.SettingsError as failure:
            values, error = dict(stream_settings.DEFAULTS), str(failure)
        running = process.lock_holder(data_dir) is not None
        return {"values": values, "options": options, "locked": running or error is not None, "error": error}

    async def set_stream_setting(self, key: str, value) -> dict:
        data_dir = await asyncio.to_thread(self._data_dir)
        if data_dir is None:
            raise RuntimeError("OpenNOW isn't installed")
        try:
            return await asyncio.to_thread(stream_settings.write, data_dir, key, value)
        except process.CoreRunning as error:
            raise RuntimeError(str(error)) from None

    async def search_catalog(self, query: str) -> list[dict]:
        await self._ensure_catalog()
        return self.catalog.search(query)

    async def refresh_catalog(self) -> int:
        await self._ensure_catalog(force=True)
        return len(self.catalog.games)

    async def game_page_matches(self, steam_app_id: int, shortcut_title: str = "") -> list[dict]:
        await self._ensure_catalog()
        if shortcut_title:
            return self.catalog.title_matches(shortcut_title)
        return self.catalog.steam_matches(str(steam_app_id))

    async def artwork(self, url: str) -> dict:
        data, kind = await asyncio.to_thread(fetch_artwork, url)
        return {"data": base64.b64encode(data).decode("ascii"), "type": kind}

    async def check_update(self) -> dict:
        install = await asyncio.to_thread(self._install)
        release = await asyncio.to_thread(releases.fetch_latest, self.state.preferences["nightlyChannel"])
        self.release = await asyncio.to_thread(releases.summarize, release, install.version if install else None)
        return self.release

    async def install_opennow(self, nightly: bool | None = None) -> bool:
        if self.install_task and not self.install_task.done():
            return False
        loop = asyncio.get_running_loop()
        self.install_cancel = threading.Event()
        cancelled = self.install_cancel
        self.install_progress = {"phase": "download", "fraction": 0.0, "version": ""}

        async def run():
            try:
                if nightly is not None:
                    await self.set_preference("nightlyChannel", nightly)
                release = self.release or await self.check_update()
                if cancelled.is_set():
                    raise installer.Cancelled("Install cancelled")
                asset = release.get("asset")
                if not asset:
                    raise RuntimeError("This release has no signed Flatpak for this device")

                def report(phase: str, fraction: float):
                    previous = self.install_progress
                    progress = {"phase": phase, "fraction": round(fraction, 2), "version": release["version"]}
                    if previous == progress:
                        return
                    self.install_progress = progress
                    asyncio.run_coroutine_threadsafe(decky.emit("install_progress", progress), loop)

                report("download", 0.0)
                worker = loop.create_task(asyncio.to_thread(installer.install_bundle, asset,
                                          Path(decky.DECKY_PLUGIN_RUNTIME_DIR), report, cancelled))
                try:
                    await asyncio.shield(worker)
                except asyncio.CancelledError:
                    cancelled.set()
                    await worker
                    raise
                installed = await asyncio.to_thread(self._install)
                if installed and installed.revision:
                    self.state.remember_flatpak_version(installed.revision, release["version"])
                if self.release is release:
                    self.release = {**release, "updateAvailable": False, "installedVersionKnown": True}
                await decky.emit("install_done", True, release["version"])
            except (installer.Cancelled, asyncio.CancelledError):
                await decky.emit("install_done", False, "")
            except Exception as error:
                decky.logger.error(f"Install failed: {error}")
                await decky.emit("install_done", False, str(error))
            finally:
                self.install_progress = None

        self.install_task = loop.create_task(run())
        return True

    async def cancel_install(self) -> bool:
        if not self.install_task or self.install_task.done():
            return False
        self.install_cancel.set()
        return True

    async def close_opennow(self) -> bool:
        data_dir = await asyncio.to_thread(self._data_dir)
        return data_dir is not None and await asyncio.to_thread(process.terminate, data_dir)
