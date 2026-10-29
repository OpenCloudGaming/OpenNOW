import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tests import support  # noqa: F401
from opennow_decky import install

FLATPAK_COMMIT = "9aabdedaf13f04ad3ff22c8b88e8d8ce5b4728844787351e5d0b97d4a2f2d8ca"
FLATPAK_INFO = """
OpenNOW - A controller-first GeForce NOW client

          ID: io.github.opencloudgaming.OpenNOW
         Ref: app/io.github.opencloudgaming.OpenNOW/x86_64/master
        Arch: x86_64
      Branch: master
     License: MIT
      Origin: opennow-origin
  Collection: 
Installation: user
   Installed: 122.3 MB
     Runtime: org.kde.Platform/x86_64/6.10
         Sdk: org.kde.Sdk/x86_64/6.10

      Commit: 9aabdedaf13f04ad3ff22c8b88e8d8ce5b4728844787351e5d0b97d4a2f2d8ca
     Subject: Export io.github.opencloudgaming.OpenNOW
        Date: 2026-10-15 23:48:57 +0000
"""


def runner(user_code=0, system_code=1, info=FLATPAK_INFO, info_code=0):
    calls = []

    def run(command):
        calls.append(command)
        code = user_code if "--user" in command else system_code
        output = FLATPAK_COMMIT + "\n" if "--show-commit" in command else info
        if "--show-commit" not in command and code == 0:
            code = info_code
        return subprocess.CompletedProcess(command, code, output if code == 0 else "", "")

    run.calls = calls
    return run


def executable(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\n")
    path.chmod(0o755)
    return path


class InstallDetectionTests(unittest.TestCase):
    def setUp(self):
        self.home = Path(tempfile.mkdtemp())
        os.environ.pop("XDG_CONFIG_HOME", None)

    def test_user_flatpak_launches_through_flatpak_run_with_its_sandboxed_data_dir(self):
        run = runner()
        found = install.detect(self.home, run=run)
        self.assertEqual(found.kind, "flatpak")
        self.assertIsNone(found.version)
        self.assertEqual(found.revision, "9aabdedaf13f04ad3ff22c8b88e8d8ce5b4728844787351e5d0b97d4a2f2d8ca")
        self.assertEqual(found.exe, "/usr/bin/flatpak")
        self.assertEqual(found.args, ("run", "--user", install.FLATPAK_ID))
        self.assertEqual(found.data_dir, str(self.home / ".var/app/io.github.opencloudgaming.OpenNOW/config/OpenNOW"))
        self.assertEqual(run.calls, [
            ["flatpak", "info", "--user", "--show-commit", install.FLATPAK_ID],
            ["flatpak", "info", "--user", install.FLATPAK_ID],
        ])

    def test_system_flatpak_is_used_when_no_user_install_exists(self):
        run = runner(user_code=1, system_code=0)
        found = install.detect(self.home, run=run)
        self.assertEqual(found.args, ("run", "--system", install.FLATPAK_ID))
        self.assertEqual(found.revision, FLATPAK_COMMIT)
        self.assertEqual(run.calls, [
            ["flatpak", "info", "--user", "--show-commit", install.FLATPAK_ID],
            ["flatpak", "info", "--system", "--show-commit", install.FLATPAK_ID],
            ["flatpak", "info", "--system", install.FLATPAK_ID],
        ])

    def test_localized_flatpak_info_keeps_revision_in_persistent_mapping(self):
        for info in ("版本：1.0.4\n提交：本地化提交\n", "Version: 1.0.4\nCommit: stale-human-readable-commit\n"):
            with self.subTest(info=info):
                found = install.detect_flatpak(self.home, runner(info=info))
                self.assertEqual(found.revision, FLATPAK_COMMIT)
                self.assertEqual(json.loads(json.dumps(found.to_dict())), {
                    "kind": "flatpak",
                    "path": install.FLATPAK_ID,
                    "version": "1.0.4" if info.startswith("Version:") else None,
                    "revision": FLATPAK_COMMIT,
                    "exe": "/usr/bin/flatpak",
                    "args": ["run", "--user", install.FLATPAK_ID],
                    "data_dir": str(self.home / ".var/app" / install.FLATPAK_ID / "config/OpenNOW"),
                })

    def test_failed_version_query_keeps_machine_readable_revision(self):
        found = install.detect_flatpak(self.home, runner(info_code=1))
        self.assertEqual(found.revision, FLATPAK_COMMIT)
        self.assertIsNone(found.version)

    def test_version_query_exception_keeps_machine_readable_revision(self):
        for error in (OSError("metadata unavailable"), subprocess.TimeoutExpired("flatpak", 15)):
            with self.subTest(error=error):
                run = mock.Mock(side_effect=[
                    subprocess.CompletedProcess([], 0, FLATPAK_COMMIT + "\n", ""), error,
                ])
                found = install.detect_flatpak(self.home, run)
                self.assertEqual(found.revision, FLATPAK_COMMIT)
                self.assertIsNone(found.version)

    def test_default_runner_reads_version_in_c_locale_without_changing_parent_environment(self):
        environment = {
            "LC_ALL": "zh_CN.UTF-8", "LANG": "zh_CN.UTF-8", "PATH": "/usr/bin",
            "LD_LIBRARY_PATH": "/decky/bundled", "LD_LIBRARY_PATH_ORIG": "/host/lib",
        }

        def run(command, **kwargs):
            if "--show-commit" in command:
                output = FLATPAK_COMMIT + "\n"
            elif kwargs["env"]["LC_ALL"] == kwargs["env"]["LANG"] == "C":
                output = "Version: 1.0.4\n"
            else:
                output = "版本：1.0.4\n"
            return subprocess.CompletedProcess(command, 0, output, "")

        with mock.patch.dict(os.environ, environment, clear=True), \
                mock.patch.object(install.subprocess, "run", side_effect=run) as execute:
            found = install.detect(self.home)
            self.assertEqual((found.version, found.revision), ("1.0.4", FLATPAK_COMMIT))
            self.assertEqual(dict(os.environ), environment)
        self.assertEqual(execute.call_count, 2)
        for call in execute.call_args_list:
            self.assertEqual(call.kwargs, {
                "capture_output": True, "text": True, "timeout": 15, "check": False,
                "env": {"LC_ALL": "C", "LANG": "C", "PATH": "/usr/bin", "LD_LIBRARY_PATH": "/host/lib"},
            })

    def test_newest_appimage_wins_and_release_beats_its_nightly(self):
        executable(self.home / "Applications/OpenNOW-Qt-1.0.2-Linux-x64.AppImage")
        executable(self.home / "Downloads/OpenNOW-Qt-1.0.3-nightly.1158.1-Linux-x64.AppImage")
        newest = executable(self.home / "Downloads/OpenNOW-Qt-1.0.3-Linux-x64.AppImage")
        found = install.detect(self.home, run=runner(user_code=1, system_code=1))
        self.assertEqual(found.kind, "appimage")
        self.assertEqual(found.path, str(newest))
        self.assertEqual(found.version, "1.0.3")
        self.assertEqual(found.data_dir, str(self.home / ".config/OpenNOW"))

    def test_appimage_without_execute_permission_is_ignored(self):
        path = self.home / "Applications/OpenNOW-Qt-1.0.3-Linux-x64.AppImage"
        path.parent.mkdir(parents=True)
        path.write_text("")
        self.assertIsNone(install.detect_appimage(self.home))

    def test_custom_path_overrides_detection(self):
        custom = executable(self.home / "builds/opennow-qt")
        found = install.detect(self.home, str(custom), run=runner())
        self.assertEqual((found.kind, found.exe), ("custom", str(custom)))
        self.assertIsNone(install.detect(self.home, str(self.home / "missing"), run=runner()))

    def test_missing_flatpak_binary_falls_through(self):
        def missing(command):
            raise FileNotFoundError("flatpak")

        self.assertIsNone(install.detect_flatpak(self.home, missing))

    def test_droiddeck_flatpak_launches_through_its_gamescope_wrapper(self):
        wrapper = executable(self.home / "droiddeck-flatpak-run")
        with mock.patch.object(install, "DROIDDECK_FLATPAK_RUN", str(wrapper)):
            found = install.detect(self.home, run=runner())
        self.assertEqual((found.exe, found.args), (str(wrapper), (install.FLATPAK_ID,)))

    def test_droiddeck_imported_appimage_runs_through_droiddeck(self):
        imported = self.home / "opt/appimages/user"
        executable(imported / "opennow/app/AppRun")
        (imported / "opennow/github-tag").write_text("v1.0.4-nightly.1210.1\n")
        older = executable(imported / "my-cloud-app/app/AppRun").parents[1]
        (older / "github-repo").write_text("OpenCloudGaming/OpenNOW")
        (older / "github-tag").write_text("v1.0.3")
        executable(imported / "duckstation/app/AppRun")
        runner_script = executable(self.home / "droiddeck-appimage-run")
        with mock.patch.object(install, "DROIDDECK_APPIMAGES", imported), \
                mock.patch.object(install, "DROIDDECK_APPIMAGE_RUN", str(runner_script)):
            found = install.detect(self.home, run=runner(user_code=1, system_code=1))
        self.assertEqual(found.kind, "appimage")
        self.assertEqual(found.version, "1.0.4-nightly.1210.1")
        self.assertEqual((found.exe, found.args), (str(runner_script), (str(imported / "opennow"),)))
        self.assertEqual(found.data_dir, str(self.home / ".config/OpenNOW"))

    def test_imported_appimages_are_ignored_outside_droiddeck(self):
        imported = self.home / "opt/appimages/user"
        executable(imported / "opennow/app/AppRun")
        with mock.patch.object(install, "DROIDDECK_APPIMAGES", imported), \
                mock.patch.object(install, "DROIDDECK_APPIMAGE_RUN", str(self.home / "missing")):
            self.assertIsNone(install.detect_droiddeck_appimage(self.home))

    def test_legacy_lowercase_data_dir_is_kept_when_it_is_the_only_one(self):
        (self.home / ".config/opennow").mkdir(parents=True)
        self.assertEqual(install.native_data_dir(self.home), self.home / ".config/opennow")

    def test_version_ordering(self):
        self.assertGreater(install.version_key("1.0.4"), install.version_key("1.0.3"))
        self.assertGreater(install.version_key("1.0.3"), install.version_key("1.0.3-nightly.1158.1"))
        self.assertGreater(install.version_key("1.0.3-nightly.1158.1"), install.version_key("1.0.3-nightly.1127.1"))
        self.assertGreater(install.version_key("1.0.10"), install.version_key("1.0.9"))


if __name__ == "__main__":
    unittest.main()
