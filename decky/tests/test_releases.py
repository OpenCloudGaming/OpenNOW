import subprocess
import copy
import unittest
from unittest import mock

from tests import support  # noqa: F401
from opennow_decky import releases

RELEASE = support.github_release()


class ReleaseTests(unittest.TestCase):
    def test_picks_the_flatpak_bundle_for_the_machine(self):
        self.assertEqual(releases.flatpak_asset(RELEASE, "x86_64"), support.selected_asset())
        self.assertEqual(releases.flatpak_asset(RELEASE, "aarch64"), support.selected_asset(arch="arm64"))
        self.assertIsNone(releases.flatpak_asset(RELEASE, "riscv64"))

    def test_flatpaks_architecture_wins_over_an_emulated_python(self):
        flatpak = subprocess.CompletedProcess([], 0, "aarch64\n", "")
        with mock.patch.object(releases.host, "run", return_value=flatpak) as run, \
                mock.patch.object(releases.platform, "machine", return_value="x86_64"):
            self.assertEqual(releases.flatpak_asset(RELEASE), support.selected_asset(arch="arm64"))
        run.assert_called_once_with(["flatpak", "--default-arch"], timeout=15)

    def test_architecture_falls_back_to_the_machine_without_flatpak(self):
        with mock.patch.object(releases.host, "run", side_effect=FileNotFoundError("flatpak")), \
                mock.patch.object(releases.platform, "machine", return_value="x86_64"):
            self.assertEqual(releases.flatpak_asset(RELEASE), support.selected_asset())

    def test_update_is_offered_only_for_a_newer_installed_version(self):
        self.assertTrue(releases.summarize(RELEASE, "1.0.3", "x86_64")["updateAvailable"])
        self.assertFalse(releases.summarize(RELEASE, "1.0.4", "x86_64")["updateAvailable"])
        self.assertFalse(releases.summarize(RELEASE, None, "x86_64")["updateAvailable"])
        self.assertEqual(releases.summarize(RELEASE, None, "x86_64")["version"], "1.0.4")

    def test_nightly_is_the_newest_published_prerelease_with_a_flatpak_for_the_machine(self):
        def release(tag, prerelease=True, draft=False, arch="x64"):
            return {**support.github_release(tag[1:], (arch,)), "prerelease": prerelease, "draft": draft}

        recent = [release("v1.0.4-nightly.1203.1", draft=True), release("v1.0.4-nightly.1202.1", arch="arm64"),
                  release("v1.0.4-rc.1"), release("v1.0.3", prerelease=False), release("v1.0.4-nightly.1201.1"),
                  release("v1.0.4-nightly.1191.1")]
        self.assertEqual(releases.latest_nightly(recent, "x86_64")["tag_name"], "v1.0.4-nightly.1201.1")
        self.assertEqual(releases.latest_nightly(recent, "aarch64")["tag_name"], "v1.0.4-nightly.1202.1")
        self.assertIsNone(releases.latest_nightly([release("v1.0.3", prerelease=False)], "x86_64"))
        self.assertTrue(releases.summarize(recent[4], "1.0.3", "x86_64")["updateAvailable"])

    def test_an_unknown_installed_version_is_reported_rather_than_called_current(self):
        self.assertFalse(releases.summarize(RELEASE, None, "x86_64")["installedVersionKnown"])
        self.assertTrue(releases.summarize(RELEASE, "1.0.4", "x86_64")["installedVersionKnown"])

    def test_missing_duplicate_wrong_release_or_unbounded_assets_are_not_selected(self):
        for mutation in (lambda assets: assets.pop(1), lambda assets: assets.append(assets[0]),
                         lambda assets: assets[1].update(size=releases.MAX_MANIFEST_BYTES + 1),
                         lambda assets: assets[0].update(size=0),
                         lambda assets: assets[0].update(size=releases.MAX_BUNDLE_BYTES + 1),
                         lambda assets: assets[1].update(browser_download_url="https://github.com/other"),
                         lambda assets: assets[0].update(name="OpenNOW-Qt-1.0.3-Linux-x64.flatpak")):
            release = copy.deepcopy(RELEASE)
            mutation(release["assets"])
            self.assertIsNone(releases.flatpak_asset(release, "x86_64"))


if __name__ == "__main__":
    unittest.main()
