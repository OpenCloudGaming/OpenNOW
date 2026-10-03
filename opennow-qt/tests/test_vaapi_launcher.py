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


APPDIR = "/opt/opennow app"
FALLBACK = APPDIR + "/usr/lib/libva-fallback"


def ld_cache(abi, libraries=("libva.so.2", "libva-drm.so.2")):
    return "".join(f"\t{library} (libc6,{abi}) => /usr/lib64/{library}\n" for library in libraries)


class VaapiLauncherTest(unittest.TestCase):
    def launch(self, architecture, cache=None, override=None, driver=None, library_path=None,
               repeat=False):
        with tempfile.TemporaryDirectory() as directory:
            uname = Path(directory) / "uname"
            uname.write_text(f'#!/bin/sh\nprintf "%s\\n" "{architecture}"\n')
            uname.chmod(0o755)
            ldconfig = Path(directory) / "ldconfig"
            if cache is None:
                ldconfig.write_text("#!/bin/sh\nexit 1\n")
            else:
                (Path(directory) / "cache").write_text(cache)
                ldconfig.write_text(f'#!/bin/sh\n[ "$1" = -p ] && cat "{directory}/cache"\n')
            ldconfig.chmod(0o755)
            env = {key: value for key, value in os.environ.items()
                   if key not in ("LIBVA_DRIVERS_PATH", "LIBVA_DRIVER_NAME", "LD_LIBRARY_PATH")}
            env["PATH"] = directory + os.pathsep + env["PATH"]
            env["this_dir"] = APPDIR
            if override is not None:
                env["LIBVA_DRIVERS_PATH"] = override
            if driver is not None:
                env["LIBVA_DRIVER_NAME"] = driver
            if library_path is not None:
                env["LD_LIBRARY_PATH"] = library_path
            source = '. "$1"\n' * (2 if repeat else 1)
            report = "".join(
                f'if [ "${{{name}+x}}" = x ]; then printf "{name}=%s\\n" "${name}"; fi\n'
                for name in ("LIBVA_DRIVERS_PATH", "LIBVA_DRIVER_NAME", "LD_LIBRARY_PATH")
            )
            result = subprocess.run(
                ["sh", "-eu", "-c", source + 'exec sh -eu -c "$2"',
                 "vaapi-test", str(HOOK), report],
                env=env, capture_output=True, text=True, check=True,
            )
            return dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)

    def test_host_libva_is_used_with_its_own_driver_search(self):
        for architecture, abi in (("x86_64", "x86-64"), ("aarch64", "AArch64")):
            with self.subTest(architecture=architecture):
                env = self.launch(architecture, cache=ld_cache(abi))
                self.assertNotIn("LD_LIBRARY_PATH", env)
                self.assertNotIn("LIBVA_DRIVERS_PATH", env)
                self.assertNotIn("LIBVA_DRIVER_NAME", env)

    def test_host_libva_keeps_explicit_settings(self):
        env = self.launch("x86_64", cache=ld_cache("x86-64"), override="/one:/two", driver="radeonsi",
                          library_path="/custom")
        self.assertEqual(env, {"LIBVA_DRIVERS_PATH": "/one:/two", "LIBVA_DRIVER_NAME": "radeonsi",
                               "LD_LIBRARY_PATH": "/custom"})

    def test_incomplete_foreign_or_unreadable_host_cache_uses_the_bundled_libva(self):
        for name, cache in (("no ldconfig", None),
                            ("empty", ""),
                            ("missing libva-drm", ld_cache("x86-64", ("libva.so.2",))),
                            ("32-bit only", "\tlibva.so.2 (libc6) => /usr/lib/libva.so.2\n"
                                            "\tlibva-drm.so.2 (libc6) => /usr/lib/libva-drm.so.2\n"),
                            ("other architecture", ld_cache("AArch64"))):
            with self.subTest(cache=name):
                self.assertEqual(self.launch("x86_64", cache=cache)["LD_LIBRARY_PATH"], FALLBACK)

    def test_bundled_libva_paths_cover_arch_fedora_and_matching_debian_architecture(self):
        for architecture, multiarch in (("x86_64", "x86_64-linux-gnu"),
                                       ("aarch64", "aarch64-linux-gnu")):
            with self.subTest(architecture=architecture):
                env = self.launch(architecture)
                self.assertEqual(env["LIBVA_DRIVERS_PATH"].split(":"),
                                 ["/usr/lib/dri", "/usr/lib64/dri", f"/usr/lib/{multiarch}/dri"])
                self.assertEqual(env["LD_LIBRARY_PATH"], FALLBACK)
                self.assertNotIn("LIBVA_DRIVER_NAME", env)

    def test_bundled_libva_keeps_explicit_paths_and_driver_selection(self):
        for override in ("", "/custom driver/dri", "/one:/two"):
            with self.subTest(override=override):
                env = self.launch("x86_64", override=override, driver="i965", library_path="/custom")
                self.assertEqual(env["LIBVA_DRIVERS_PATH"], override)
                self.assertEqual(env["LIBVA_DRIVER_NAME"], "i965")
                self.assertEqual(env["LD_LIBRARY_PATH"], FALLBACK + ":/custom")

    def test_repeated_sourcing_does_not_duplicate_paths(self):
        self.assertEqual(self.launch("x86_64"), self.launch("x86_64", repeat=True))

    def test_unknown_architecture_uses_the_bundled_libva_with_generic_paths(self):
        env = self.launch("other", cache=ld_cache("x86-64"))
        self.assertEqual(env["LIBVA_DRIVERS_PATH"], "/usr/lib/dri:/usr/lib64/dri")
        self.assertEqual(env["LD_LIBRARY_PATH"], FALLBACK)

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
                fallback = workflow.index('"build/AppDir/usr/lib/libva-fallback/$library.so.2"')
                self.assertLess(fallback, deploy)
                self.assertIn("--exclude-library 'libva.so.*'", workflow)
                self.assertIn("--exclude-library 'libva-drm.so.*'", workflow)
                self.assertNotIn("--library", workflow)

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
            with self.assertRaisesRegex(ValueError, "missing its fallback libva.so.2"):
                verify_apprun(appdir)
            fallback = appdir / "usr/lib/libva-fallback"
            fallback.mkdir(parents=True)
            for library in ("libva.so.2", "libva-drm.so.2"):
                (fallback / library).write_text("fallback\n")
            verify_apprun(appdir)
            (appdir / "usr/lib/libva-drm.so.2").write_text("shadow\n")
            with self.assertRaisesRegex(ValueError, "libva-drm.so.2 must not shadow"):
                verify_apprun(appdir)
            (appdir / "usr/lib/libva-drm.so.2").unlink()
            deployed.write_text("outdated hook\n")
            with self.assertRaisesRegex(ValueError, "missing or stale"):
                verify_apprun(appdir)


if __name__ == "__main__":
    unittest.main()
