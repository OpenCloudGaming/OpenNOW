import asyncio
import fcntl
import json
import shlex
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from tests import support


class PluginTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.home = Path(tempfile.mkdtemp())
        self.decky = support.install_fake_decky(self.home)
        import main
        main.decky = self.decky
        self.commit = "a" * 64
        mock.patch("opennow_decky.install.run_quiet", side_effect=lambda command: subprocess.CompletedProcess(
            command, 0, self.commit + "\n" if "--show-commit" in command else "提交：" + self.commit + "\n", "")).start()
        self.addCleanup(mock.patch.stopall)
        with mock.patch.object(main.Plugin, "_warm_catalog", new=mock.AsyncMock()):
            self.plugin = main.Plugin()
            await self.plugin._main()
        self.data_dir = self.home / ".var/app/io.github.opencloudgaming.OpenNOW/config/OpenNOW"

    async def test_game_shortcut_runs_the_flatpak_in_game_mode_with_a_direct_launch(self):
        command = await self.plugin.launch_command("102279111", "Tower of \"Fantasy\" & Co")
        self.assertEqual(command["exe"], '"/usr/bin/flatpak"')
        self.assertEqual(shlex.split(command["options"]), [
            "run", "--user", "io.github.opencloudgaming.OpenNOW", "--console",
            "--fullscreen", "--quit-after-session", "--launch-app-id=102279111", "--launch-title=Tower of \"Fantasy\" & Co",
        ])

    async def test_desktop_layout_preference_drops_the_console_flag(self):
        await self.plugin.set_preference("consoleLayout", False)
        command = await self.plugin.launch_command()
        self.assertNotIn("--console", shlex.split(command["options"]))

    async def test_plain_launcher_keeps_its_window_and_stays_open(self):
        options = shlex.split((await self.plugin.launch_command())["options"])
        self.assertNotIn("--fullscreen", options)
        self.assertNotIn("--quit-after-session", options)

    async def test_close_requests_qt_confirmation_through_the_installed_flatpak(self):
        with mock.patch("opennow_decky.process.request_quit", return_value=True, create=True) as request:
            self.assertTrue(await self.plugin.close_opennow())
        install = request.call_args.args[0]
        self.assertEqual([install.exe, *install.args, "--request-quit"], [
            "/usr/bin/flatpak", "run", "--user", "io.github.opencloudgaming.OpenNOW", "--request-quit",
        ])

    async def test_cancelled_qt_confirmation_does_not_report_a_closed_application(self):
        with mock.patch("opennow_decky.process.request_quit", return_value=False, create=True):
            self.assertFalse(await self.plugin.close_opennow())

    async def test_game_page_waits_for_the_first_catalog_load(self):
        game = {"id": "g", "title": "Silksong", "variants": [{"id": "1", "store": "STEAM", "storeId": "1030300"}], "art": {}}

        def load(force=False):
            self.plugin.catalog._index([game], self.plugin.catalog.clock())

        with mock.patch.object(self.plugin.catalog, "ensure", side_effect=load):
            matches = await self.plugin.game_page_matches(1030300)
        self.assertEqual([match["variant"]["id"] for match in matches], ["1"])

    async def test_non_numeric_variant_is_refused(self):
        with self.assertRaises(ValueError):
            await self.plugin.launch_command("1; rm -rf ~")

    async def test_stream_settings_write_to_the_flatpak_data_dir(self):
        result = await self.plugin.get_stream_settings()
        self.assertFalse(result["locked"])
        await self.plugin.set_stream_setting("fps", 90)
        self.assertEqual(json.loads((self.data_dir / "settings.json").read_text())["fps"], 90)

    async def test_stream_settings_lock_while_the_core_holds_its_lock(self):
        with mock.patch("opennow_decky.process.lock_holder", return_value=123):
            self.assertTrue((await self.plugin.get_stream_settings())["locked"])
            self.assertTrue((await self.plugin.get_status())["running"])
        self.data_dir.mkdir(parents=True)
        with open(self.data_dir / "core.lock", "w") as core:
            fcntl.flock(core, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(RuntimeError):
                await self.plugin.set_stream_setting("fps", 90)
        self.assertFalse((self.data_dir / "settings.json").exists())

    async def test_status_reports_whether_opennow_has_a_signed_in_account(self):
        self.assertFalse((await self.plugin.get_status())["signedIn"])
        self.data_dir.mkdir(parents=True)
        (self.data_dir / "accounts.json").write_text(json.dumps({"accounts": [{"userId": "a"}]}))
        self.assertTrue((await self.plugin.get_status())["signedIn"])

    async def test_cancelled_install_reports_no_error(self):
        self.plugin.release = {"version": "1.0.4", "asset": {"url": "https://github.com/x", "size": 1}, "updateAvailable": True}
        started = asyncio.Event()
        loop = asyncio.get_running_loop()

        def slow_install(selected, work_dir, progress, cancelled):
            loop.call_soon_threadsafe(started.set)
            cancelled.wait(5)
            from opennow_decky import installer
            raise installer.Cancelled("Install cancelled")

        with mock.patch("opennow_decky.installer.install_bundle", side_effect=slow_install):
            await self.plugin.install_opennow()
            await started.wait()
            self.assertTrue(await self.plugin.cancel_install())
            await self.plugin.install_task
        self.assertEqual(self.decky.emitted[-1], ("install_done", (False, "")))
        self.assertFalse(await self.plugin.cancel_install())

    async def test_first_install_reserves_ownership_during_release_lookup(self):
        entered, release_lookup = asyncio.Event(), asyncio.Event()

        async def lookup():
            entered.set()
            await release_lookup.wait()
            return {"version": "1.0.4", "asset": {"name": "selected"}}

        with mock.patch.object(self.plugin, "check_update", side_effect=lookup) as check, \
                mock.patch("opennow_decky.installer.install_bundle") as install:
            first = asyncio.create_task(self.plugin.install_opennow())
            await entered.wait()
            second = asyncio.create_task(self.plugin.install_opennow())
            await asyncio.sleep(0)
            try:
                self.assertTrue(second.done(), "Second caller entered the release lookup")
                self.assertFalse(await second)
                self.assertIsNotNone((await self.plugin.get_status())["installProgress"])
                self.assertTrue(await self.plugin.cancel_install())
            finally:
                release_lookup.set()
                await asyncio.gather(first, second)
                if self.plugin.install_task:
                    await self.plugin.install_task
        check.assert_awaited_once()
        install.assert_not_called()
        self.assertIsNone(self.plugin.install_progress)

    async def test_first_install_lookup_and_worker_have_one_owner(self):
        entered, release_lookup, worker_started = asyncio.Event(), asyncio.Event(), asyncio.Event()
        finish_worker = threading.Event()
        selected = support.selected_asset()
        loop = asyncio.get_running_loop()

        async def lookup():
            entered.set()
            await release_lookup.wait()
            return {"version": "1.0.4", "asset": selected}

        def worker(asset, directory, progress, cancelled):
            loop.call_soon_threadsafe(worker_started.set)
            finish_worker.wait(5)

        with mock.patch.object(self.plugin, "check_update", side_effect=lookup) as check, \
                mock.patch("opennow_decky.installer.install_bundle", side_effect=worker) as install:
            self.assertTrue(await self.plugin.install_opennow())
            await entered.wait()
            self.assertFalse(await self.plugin.install_opennow(True))
            self.assertFalse(self.plugin.state.preferences["nightlyChannel"])
            release_lookup.set()
            await worker_started.wait()
            owner = self.plugin.install_task
            try:
                self.assertFalse(await self.plugin.install_opennow())
                owner.cancel()
                await asyncio.sleep(0)
                self.assertFalse(await self.plugin.install_opennow())
                self.assertIs(self.plugin.install_task, owner)
            finally:
                finish_worker.set()
                await owner
        check.assert_awaited_once()
        install.assert_called_once()
        self.assertIs(install.call_args.args[0], selected)
        self.assertIsNone(self.plugin.install_progress)
        self.assertEqual(self.decky.emitted[-1], ("install_done", (False, "")))

    async def test_lookup_failure_releases_ownership_and_a_retry_can_install(self):
        with mock.patch.object(self.plugin, "check_update", side_effect=ValueError("lookup failed")):
            self.assertTrue(await self.plugin.install_opennow())
            await self.plugin.install_task
        self.assertEqual(self.decky.emitted[-1], ("install_done", (False, "lookup failed")))
        self.assertIsNone(self.plugin.install_progress)
        self.assertFalse(await self.plugin.cancel_install())
        self.plugin.release = {"version": "1.0.4", "asset": support.selected_asset()}
        with mock.patch("opennow_decky.installer.install_bundle") as install:
            self.assertTrue(await self.plugin.install_opennow())
            await self.plugin.install_task
        install.assert_called_once()

    async def test_localized_flatpak_retains_installed_version_and_update_tracking_after_reload(self):
        self.plugin.release = {"version": "1.0.4", "asset": support.selected_asset()}
        with mock.patch("opennow_decky.installer.install_bundle"):
            await self.plugin.install_opennow()
            await self.plugin.install_task
        import main
        with mock.patch.object(main.Plugin, "_warm_catalog", new=mock.AsyncMock()):
            reloaded = main.Plugin()
            await reloaded._main()
        status = await reloaded.get_status()
        self.assertEqual(status["install"]["revision"], self.commit)
        self.assertEqual(status["install"]["version"], "1.0.4")
        with mock.patch("opennow_decky.releases.fetch_latest", return_value=support.github_release("1.0.5")):
            release = await reloaded.check_update()
        self.assertTrue(release["installedVersionKnown"])
        self.assertTrue(release["updateAvailable"])

    async def test_an_unexpected_install_failure_still_ends_the_install(self):
        self.plugin.release = {"version": "1.0.4", "asset": {"url": "https://github.com/x", "size": 1}, "updateAvailable": True}
        with mock.patch("opennow_decky.installer.install_bundle", side_effect=subprocess.TimeoutExpired(["flatpak"], 60)):
            await self.plugin.install_opennow()
            await self.plugin.install_task
        self.assertEqual(self.decky.emitted[-1][0], "install_done")
        self.assertFalse(self.decky.emitted[-1][1][0])
        self.assertIsNone(self.plugin.install_progress)

    async def test_install_reports_progress_and_completion(self):
        release = {"version": "1.0.4", "asset": {"url": "https://github.com/x", "size": 1}, "updateAvailable": True}
        self.plugin.release = release

        def fake_install(selected, work_dir, progress, cancelled):
            progress("download", 0.5)
            progress("install", 1.0)

        with mock.patch("opennow_decky.installer.install_bundle", side_effect=fake_install):
            self.assertTrue(await self.plugin.install_opennow())
            await self.plugin.install_task
        await asyncio.sleep(0)
        events = [event for event, _ in self.decky.emitted]
        self.assertIn("install_progress", events)
        self.assertEqual(self.decky.emitted[-1], ("install_done", (True, "1.0.4")))
        self.assertIsNone(self.plugin.install_progress)
        status = await self.plugin.get_status()
        self.assertEqual(status["install"]["version"], "1.0.4")
        with mock.patch("opennow_decky.releases.fetch_latest", return_value={"tag_name": "v1.0.4", "assets": []}):
            self.assertFalse((await self.plugin.check_update())["updateAvailable"])
        with mock.patch("opennow_decky.releases.fetch_latest", return_value={"tag_name": "v1.0.5", "assets": []}):
            self.assertTrue((await self.plugin.check_update())["updateAvailable"])

    async def test_an_install_from_the_old_channel_does_not_replace_the_new_channel_release(self):
        stable = {"version": "1.0.3", "asset": {"url": "https://github.com/s", "size": 1}, "updateAvailable": False}
        nightly = {"version": "1.0.4-nightly.1201.1", "asset": {"url": "https://github.com/n", "size": 1}, "updateAvailable": True}
        self.plugin.release = stable
        started, release_install = asyncio.Event(), threading.Event()
        loop = asyncio.get_running_loop()

        def held_install(selected, work_dir, progress, cancelled):
            loop.call_soon_threadsafe(started.set)
            release_install.wait(5)

        with mock.patch("opennow_decky.installer.install_bundle", side_effect=held_install):
            await self.plugin.install_opennow()
            await started.wait()
            await self.plugin.set_preference("nightlyChannel", True)
            self.plugin.release = nightly
            release_install.set()
            await self.plugin.install_task
        self.assertIs(self.plugin.release, nightly)

    async def test_choosing_nightly_at_install_switches_the_update_channel(self):
        self.plugin.release = {"version": "1.0.3", "asset": {"url": "https://github.com/x", "size": 1}}
        nightly = support.github_release("1.0.4-nightly.1201.1", ("x64",))
        with mock.patch("opennow_decky.releases.fetch_latest", return_value=nightly) as fetch, \
                mock.patch("opennow_decky.releases.platform.machine", return_value="x86_64"), \
                mock.patch("opennow_decky.installer.install_bundle") as install:
            self.assertTrue(await self.plugin.install_opennow(True))
            await self.plugin.install_task
        fetch.assert_called_once_with(True)
        self.assertEqual(install.call_args.args[0], support.selected_asset("1.0.4-nightly.1201.1"))
        self.assertTrue((await self.plugin.get_state())["preferences"]["nightlyChannel"])
        self.assertEqual(self.decky.emitted[-1], ("install_done", (True, "1.0.4-nightly.1201.1")))


if __name__ == "__main__":
    unittest.main()
