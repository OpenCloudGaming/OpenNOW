import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = ROOT / "opennow-qt/packaging"
HOOK = PACKAGING / "opennow-vaapi-hook.sh"
sys.path.insert(0, str(PACKAGING))
from verify_linux_package import verify_apprun


class VaapiLauncherTest(unittest.TestCase):
    def launch(self, architecture, override=None, driver=None, repeat=False):
        with tempfile.TemporaryDirectory() as directory:
            uname = Path(directory) / "uname"
            uname.write_text(f'#!/bin/sh\nprintf "%s\\n" "{architecture}"\n')
            uname.chmod(0o755)
            env = {key: value for key, value in os.environ.items()
                   if key not in ("LIBVA_DRIVERS_PATH", "LIBVA_DRIVER_NAME")}
            env["PATH"] = directory + os.pathsep + env["PATH"]
            if override is not None:
                env["LIBVA_DRIVERS_PATH"] = override
            if driver is not None:
                env["LIBVA_DRIVER_NAME"] = driver
            source = '. "$1"\n' * (2 if repeat else 1)
            report = (
                'printf "LIBVA_DRIVERS_PATH=%s\\n" "$LIBVA_DRIVERS_PATH"\n'
                'if [ "${LIBVA_DRIVER_NAME+x}" = x ]; then\n'
                '    printf "LIBVA_DRIVER_NAME=%s\\n" "$LIBVA_DRIVER_NAME"\n'
                'fi\n'
            )
            result = subprocess.run(
                ["sh", "-eu", "-c", source + 'exec sh -eu -c "$2"',
                 "vaapi-test", str(HOOK), report],
                env=env, capture_output=True, text=True, check=True,
            )
            return dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)

    def test_portable_paths_cover_arch_fedora_and_matching_debian_architecture(self):
        for architecture, multiarch in (("x86_64", "x86_64-linux-gnu"),
                                       ("aarch64", "aarch64-linux-gnu")):
            with self.subTest(architecture=architecture):
                env = self.launch(architecture)
                self.assertEqual(env["LIBVA_DRIVERS_PATH"].split(":"),
                                 ["/usr/lib/dri", "/usr/lib64/dri", f"/usr/lib/{multiarch}/dri"])
                self.assertNotIn("LIBVA_DRIVER_NAME", env)

    def test_explicit_paths_and_driver_selection_are_preserved(self):
        for override in ("", "/custom driver/dri", "/one:/two"):
            with self.subTest(override=override):
                env = self.launch("x86_64", override=override, driver="i965")
                self.assertEqual(env["LIBVA_DRIVERS_PATH"], override)
                self.assertEqual(env["LIBVA_DRIVER_NAME"], "i965")

    def test_repeated_sourcing_does_not_duplicate_paths(self):
        self.assertEqual(self.launch("x86_64")["LIBVA_DRIVERS_PATH"],
                         self.launch("x86_64", repeat=True)["LIBVA_DRIVERS_PATH"])

    def test_unknown_architecture_keeps_generic_paths(self):
        self.assertEqual(self.launch("other")["LIBVA_DRIVERS_PATH"],
                         "/usr/lib/dri:/usr/lib64/dri")

    def test_both_release_workflows_install_and_verify_the_hook(self):
        for name in ("qt-build.yml", "qt-release-candidate.yml"):
            with self.subTest(workflow=name):
                workflow = (ROOT / ".github/workflows" / name).read_text()
                install = workflow.index("install -Dm644 opennow-qt/packaging/opennow-vaapi-hook.sh")
                deploy = workflow.index("--plugin qt")
                verify = workflow.index("--appdir build/AppDir", deploy)
                package_deb = workflow.index("LinuxBundledDeb.cmake")
                self.assertLess(install, deploy)
                self.assertLess(deploy, verify)
                self.assertLess(verify, package_deb)
                self.assertIn("build/AppDir/apprun-hooks/opennow-vaapi-hook.sh", workflow)

    def test_package_verifier_requires_current_hook_and_launcher_registration(self):
        with tempfile.TemporaryDirectory() as directory:
            appdir = Path(directory)
            hooks = appdir / "apprun-hooks"
            hooks.mkdir()
            (appdir / "AppRun").write_text('#!/bin/sh\nexec "$this_dir"/AppRun.wrapped "$@"\n')
            with self.assertRaisesRegex(ValueError, "missing or stale"):
                verify_apprun(appdir)
            deployed = hooks / HOOK.name
            deployed.write_bytes(HOOK.read_bytes())
            with self.assertRaisesRegex(ValueError, "does not source"):
                verify_apprun(appdir)
            (appdir / "AppRun").write_text(
                f'source "$this_dir"/apprun-hooks/"{HOOK.name}"\n'
                'exec "$this_dir"/AppRun.wrapped "$@"\n'
            )
            verify_apprun(appdir)
            deployed.write_text("outdated hook\n")
            with self.assertRaisesRegex(ValueError, "missing or stale"):
                verify_apprun(appdir)


if __name__ == "__main__":
    unittest.main()
