from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
ISS = ROOT / "opennow-qt/packaging/windows/setup.iss"
BUILD = ROOT / "opennow-qt/packaging/windows/build-setup.ps1"
WORKFLOWS = ROOT / ".github/workflows"
INSTALLER = ROOT / "opennow-qt/cmake/WindowsInstaller.cmake"
APPLY = ROOT / "native/opennow-core/src/update_apply"


class WindowsSetupTests(unittest.TestCase):
    def test_setup_installs_the_portable_tree_without_an_msi_product(self):
        script = ISS.read_text()
        self.assertIn("DefaultDirName={localappdata}\\OpenNOW", script)
        self.assertIn("PrivilegesRequired=lowest", script)
        self.assertIn("{app}\\bin\\{#AppExeName}", script)
        self.assertIn('AppExeName "OpenNOW.exe"', script)
        self.assertNotIn("[Registry]", script)
        setup = script.split("[Setup]", 1)[1].split("[Files]", 1)[0]
        self.assertNotIn("ProductCode", setup)
        self.assertNotIn("UpgradeCode", setup)
        self.assertNotIn("InstallLocation", setup)
        self.assertNotIn("msiexec.exe'), '/i", script)
        self.assertIn("msiexec.exe'), '/x", script)
        self.assertNotIn("{userappdata}\\OpenNOW", script)
        self.assertIn('Type: filesandordirs; Name: "{app}"', script)
        app_id = "7E2A9C14-6B0D-4E58-9F31-0C8A5D2B6E17"
        self.assertIn(app_id, script)
        for upgrade in (
            "6E81F7AE-B19D-4E87-A94A-2B2F01EBF762",
            "9661F4F8-656C-4B64-9035-01B04F4822B1",
            "B3AF8A40-5F44-445A-AD99-CECE00593601",
        ):
            self.assertIn(upgrade, script)
            self.assertNotEqual(app_id, upgrade)

    def test_build_script_requires_the_portable_executables(self):
        script = BUILD.read_text()
        self.assertIn('. "$PSScriptRoot/../windows-release.ps1"', script)
        self.assertIn("$Payload = Resolve-OpenNowSetupPayload -Root $Payload", script)
        resolver = (ROOT / "opennow-qt/packaging/windows-release.ps1").read_text()
        for name in ("bin\\OpenNOW.exe", "bin\\opennow-core.exe", "bin\\opennow-update-helper.exe"):
            self.assertIn(name, resolver)
        self.assertIn("9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732", script)
        self.assertIn("innosetup-6.7.3.exe", script)

    def test_release_jobs_keep_wix_and_add_setup_exe(self):
        installer = INSTALLER.read_text()
        self.assertIn('set(CPACK_GENERATOR "WIX;ZIP")', installer)
        build = (WORKFLOWS / "qt-build.yml").read_text()
        candidate = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        self.assertIn('package-generator: "WIX;ZIP"', build)
        self.assertIn("-G WIX", candidate)
        self.assertIn("build-setup.ps1", build)
        self.assertIn("build-setup.ps1", candidate)
        self.assertIn("OpenNOW-Qt-%s-Windows-%s-setup.exe", candidate)
        self.assertIn('[[ "$assets" -eq 14 ]]', candidate)
        self.assertIn('pkexec', (APPLY / "managed.rs").read_text())
        self.assertIn("extract_dmg", (APPLY / "bundle.rs").read_text())


if __name__ == "__main__":
    unittest.main()
