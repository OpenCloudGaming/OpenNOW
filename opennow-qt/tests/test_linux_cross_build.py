import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("linux_handoff", ROOT / ".github/scripts/linux-arm64-handoff.py")
handoff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(handoff)
ARM_ELF = b"\x7fELF\x02\x01" + bytes(12) + (183).to_bytes(2, "little")


class LinuxCrossBuildTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name) / "checkout"
        self.root.mkdir()
        self.temp = Path(directory.name) / "runner-temp"
        self.sdl = self.temp / "SDL-install/lib"
        self.sdl.mkdir(parents=True)
        self.build = self.root / handoff.BUILD
        self.build.mkdir(parents=True)
        (self.build / "opennow-qt").write_bytes(ARM_ELF)
        (self.build / "opennow-qt").chmod(0o755)
        (self.build / "CTestTestfile.cmake").write_text(f'add_test(smoke "{self.build}/opennow-qt")\n')
        rust = self.build / "streamer-rust-target/aarch64-unknown-linux-gnu/release"
        rust.mkdir(parents=True)
        for name in ("libopennow_streamer_ffi.so", "opennow-streamer"):
            (rust / name).write_bytes(ARM_ELF)
        (rust / "unused.a").write_bytes(b"intermediate")
        (self.sdl / "libSDL3.so.0").write_bytes(ARM_ELF)
        (self.sdl / "libSDL3.so").symlink_to("libSDL3.so.0")
        self.archive = self.root.parent / "handoff.tar.gz"
        self.addCleanup(patch.stopall)
        check_output = handoff.subprocess.check_output
        patch.object(handoff.subprocess, "check_output", side_effect=lambda command, **options:
                     "fixture-commit" if command[0] == "git" else check_output(command, **options)).start()
        patch.object(handoff, "ROOT", self.root).start()
        patch.dict(os.environ, {
            "RUNNER_TEMP": str(self.temp), "QT_ROOT_DIR": str(self.root.parent / "Qt"),
            "OPENNOW_ARM64_SDK_ID": "test-sdk",
        }).start()

    def pack(self):
        handoff.pack(self.archive, "0.1.0")
        shutil.rmtree(self.build)
        shutil.rmtree(self.temp / "SDL-install")

    def rewrite(self, change):
        with tarfile.open(self.archive) as source:
            entries = [(member.name, source.extractfile(member).read()) for member in source.getmembers()]
        with tarfile.open(self.archive, "w:gz") as target:
            for name, content in entries:
                if name == "handoff.json":
                    manifest = json.loads(content)
                    change(manifest)
                    content = json.dumps(manifest).encode()
                info = tarfile.TarInfo(name)
                info.size = len(content)
                target.addfile(info, io.BytesIO(content))

    def test_round_trip_preserves_native_inventory_permissions_and_nested_install_inputs(self):
        self.pack()
        handoff.restore(self.archive, "0.1.0")
        self.assertEqual(handoff.inventory(), ["smoke"])
        self.assertEqual((self.build / "opennow-qt").stat().st_mode & 0o777, 0o755)
        self.assertTrue((self.build / "streamer-rust-target/aarch64-unknown-linux-gnu/release/libopennow_streamer_ffi.so").is_file())
        self.assertFalse((self.build / "streamer-rust-target/aarch64-unknown-linux-gnu/release/unused.a").exists())
        self.assertEqual((self.sdl / "libSDL3.so").read_bytes(), ARM_ELF)

    def test_handoff_materializes_cached_cargo_directory_links(self):
        original = self.build / "streamer-rust-target"
        cached = self.temp / "cached-cargo"
        original.rename(cached)
        original.symlink_to(cached, target_is_directory=True)
        self.pack()
        handoff.restore(self.archive, "0.1.0")
        self.assertEqual((original / "aarch64-unknown-linux-gnu/release/opennow-streamer").read_bytes(), ARM_ELF)

    def test_host_binaries_are_rejected_before_upload(self):
        (self.build / "opennow-qt").write_bytes(ARM_ELF[:18] + (62).to_bytes(2, "little"))
        with self.assertRaisesRegex(ValueError, "Not an ARM64 ELF"):
            handoff.pack(self.archive, "0.1.0")

    def test_version_and_source_and_paths_must_match(self):
        for key in ("commit", "version", "workspace", "runner_temp", "qt_root"):
            with self.subTest(key=key):
                handoff.pack(self.archive, "0.1.0")
                self.rewrite(lambda manifest: manifest.update({key: "different"}))
                with self.assertRaisesRegex(ValueError, f"{key} mismatch"):
                    handoff.restore(self.archive, "0.1.0")

    def test_changed_files_and_test_inventory_are_rejected(self):
        self.pack()
        self.rewrite(lambda manifest: manifest["files"]["build/opennow-qt-release/opennow-qt"].update(sha256="wrong"))
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            handoff.restore(self.archive, "0.1.0")

    def test_missing_native_tests_are_rejected(self):
        self.pack()
        self.rewrite(lambda manifest: manifest.update(tests=[]))
        with self.assertRaisesRegex(ValueError, "test inventory"):
            handoff.restore(self.archive, "0.1.0")

    def test_restore_rejects_existing_sdl_links(self):
        self.pack()
        outside = self.root.parent / "outside"
        outside.mkdir()
        (self.temp / "SDL-install").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "fresh destination"):
            handoff.restore(self.archive, "0.1.0")
        self.assertEqual(list(outside.iterdir()), [])

    def test_restore_rejects_linked_build_ancestors(self):
        self.pack()
        outside = self.root.parent / "outside"
        outside.mkdir()
        (self.root / "build").rmdir()
        (self.root / "build").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "linked destination ancestors"):
            handoff.restore(self.archive, "0.1.0")
        self.assertEqual(list(outside.iterdir()), [])

    def test_experiment_uses_native_arm_tests_and_packages_without_rebuilding(self):
        workflow = (ROOT / ".github/workflows/qt-build.yml").read_text()
        cross = (ROOT / ".github/workflows/qt-linux-arm64-cross.yml").read_text()
        packages = (ROOT / ".github/actions/linux-package/action.yml").read_text()
        baseline = workflow.split("  packages:\n", 1)[1].split("  flatpak:\n", 1)[0]
        validate = cross.split("  validate:\n", 1)[1]
        self.assertIn("runs-on: blacksmith-8vcpu-ubuntu-2404\n", cross)
        self.assertIn("os: blacksmith-8vcpu-ubuntu-2404-arm", baseline)
        self.assertIn("    needs: metadata\n", baseline)
        self.assertNotIn("linux_arm64_host", baseline)
        self.assertIn("    needs: compile\n", validate)
        self.assertIn("runs-on: blacksmith-8vcpu-ubuntu-2404-arm", validate)
        self.assertNotIn("cmake --build", validate)
        self.assertNotIn("cmake -S", validate)
        self.assertIn("uses: ./.github/actions/linux-package", baseline)
        self.assertIn("uses: ./.github/actions/linux-package", validate)
        for name in ("Create Linux AppImage", "Create bundled Linux DEB", "Smoke-test Linux AppImage"):
            self.assertIn(f"- name: {name}\n", packages)
        self.assertIn("--no-tests=error -LE interactive-desktop", workflow)
        self.assertIn("--parallel 8 --no-tests=error -LE interactive-desktop", validate)
        self.assertIn("name: linux-arm64-cross-measurement-packages", validate)
        self.assertNotIn("BUILD_TESTING=OFF", cross)
        self.assertNotIn("qemu", cross.lower())
        self.assertEqual((ROOT / ".github/workflows/qt-ci.yml").read_text().count('".github/cmake/**"'), 2)
        self.assertIn(".github/workflows/qt-linux-arm64-cross.yml", (ROOT / ".github/workflows/qt-checks.yml").read_text())


if __name__ == "__main__":
    unittest.main()
