from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
ISS = ROOT / "opennow-qt/packaging/windows/setup.iss"
BUILD = ROOT / "opennow-qt/packaging/windows/build-setup.ps1"
WORKFLOWS = ROOT / ".github/workflows"
PACKAGING = ROOT / "opennow-qt/cmake/Packaging.cmake"
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
        self.assertIn('Type: dirifempty; Name: "{app}"', script)
        app_id = "7E2A9C14-6B0D-4E58-9F31-0C8A5D2B6E17"
        self.assertIn(app_id, script)
        for upgrade in (
            "6E81F7AE-B19D-4E87-A94A-2B2F01EBF762",
            "9661F4F8-656C-4B64-9035-01B04F4822B1",
            "B3AF8A40-5F44-445A-AD99-CECE00593601",
        ):
            self.assertIn(upgrade, script)
            self.assertNotEqual(app_id, upgrade)

    def test_setup_allows_selecting_a_dedicated_destination_on_any_drive(self):
        script = ISS.read_text()
        self.assertIn("DisableDirPage=no", script)
        self.assertIn("AppendDefaultDirName=yes", script)
        self.assertIn("AllowRootDirectory=no", script)
        self.assertIn("UsePreviousAppDir=yes", script)
        self.assertIn('DestDir: "{app}"', script)
        self.assertIn('WorkingDir: "{app}\\bin"', script)

    def test_uninstall_preserves_untracked_files_in_the_selected_destination(self):
        script = ISS.read_text()
        uninstall = script.split("[UninstallDelete]", 1)[1].split("[Code]", 1)[0]
        self.assertEqual(
            [line.strip() for line in uninstall.splitlines() if line.strip()],
            [
                r'Type: filesandordirs; Name: "{app}\bin"',
                r'Type: filesandordirs; Name: "{app}\plugins"',
                r'Type: filesandordirs; Name: "{app}\qml"',
                r'Type: filesandordirs; Name: "{app}\share"',
                r'Type: filesandordirs; Name: "{app}\translations"',
                'Type: dirifempty; Name: "{app}"',
            ],
        )
        files = script.split("[Files]", 1)[1].split("[Tasks]", 1)[0]
        self.assertNotIn("uninsneveruninstall", files)

    def test_build_script_requires_the_portable_executables(self):
        script = BUILD.read_text()
        self.assertIn('. "$PSScriptRoot/../windows-release.ps1"', script)
        self.assertIn("$Payload = Resolve-OpenNowSetupPayload -Root $Payload", script)
        resolver = (ROOT / "opennow-qt/packaging/windows-release.ps1").read_text()
        for name in ("bin\\OpenNOW.exe", "bin\\opennow-core.exe", "bin\\opennow-update-helper.exe"):
            self.assertIn(name, resolver)
        self.assertIn("9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732", script)
        self.assertIn("innosetup-6.7.3.exe", script)

    def test_setup_rejects_owned_directory_collisions_before_installing(self):
        script = ISS.read_text()
        self.assertIn("function DestinationError", script)
        validation = script.split("function DestinationError", 1)[1].split("function MsiEnumRelatedProducts", 1)[0]
        self.assertIn(r"FileExists(AddBackslash(Directory) + 'bin\{#AppExeName}')", validation)
        for directory in ("bin", "plugins", "qml", "share", "translations"):
            self.assertIn(f"DirExists(AddBackslash(Directory) + '{directory}')", validation)
        self.assertIn("CurPageID <> wpSelectDir", validation)
        self.assertIn("WizardSilent", validation)
        self.assertIn("Error := DestinationError(WizardDirValue);", validation)
        self.assertIn("MsgBox(Error, mbError, MB_OK);", validation)
        self.assertIn("function PrepareToInstall(var NeedsRestart: Boolean): String;", validation)
        self.assertIn("Result := DestinationError(ExpandConstant('{app}'));", validation)
        self.assertIn("Result := '{#DestinationConflictMessage}';", validation)
        build = BUILD.read_text()
        self.assertIn("locales/en.json", build)
        self.assertIn("ConvertFrom-Json -AsHashtable", build)
        self.assertIn(".windowsSetup.destinationConflict", build)
        self.assertIn('"/DDestinationConflictMessage=$destinationConflictMessage"', build)

    def test_release_jobs_ship_setup_exe_and_zip_without_msi(self):
        packaging = PACKAGING.read_text()
        self.assertIn('set(CPACK_GENERATOR "ZIP")', packaging)
        self.assertNotIn("WIX", packaging)
        build = (WORKFLOWS / "qt-build.yml").read_text()
        candidate = (WORKFLOWS / "qt-release-candidate.yml").read_text()
        self.assertIn('package-generator: "ZIP"', build)
        for workflow in (build, candidate):
            self.assertNotIn("WIX", workflow)
            self.assertNotIn(".msi", workflow)
            self.assertIn("build-setup.ps1", workflow)
        self.assertIn("OpenNOW-Qt-%s-Windows-%s-setup.exe", candidate)
        self.assertIn('[[ "$assets" -eq 16 ]]', candidate)
        self.assertIn('pkexec', (APPLY / "managed.rs").read_text())
        self.assertIn("extract_dmg", (APPLY / "bundle.rs").read_text())


if __name__ == "__main__":
    unittest.main()
