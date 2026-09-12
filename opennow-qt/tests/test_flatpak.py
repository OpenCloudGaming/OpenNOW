import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = ROOT / "opennow-qt/packaging/flatpak"
SPEC = importlib.util.spec_from_file_location("prepare_sources", PACKAGING / "prepare_sources.py")
PREPARE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREPARE)


class FlatpakTests(unittest.TestCase):
    def test_manifest_preserves_application_identity_and_offline_build(self):
        manifest = json.loads((PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_text())
        self.assertEqual(manifest["app-id"], "io.github.opencloudgaming.OpenNOW")
        self.assertEqual(manifest["command"], "opennow-qt")
        application = manifest["modules"][-1]
        self.assertEqual(application["build-options"]["env"]["CARGO_NET_OFFLINE"], "true")
        self.assertEqual(application["build-options"]["env"]["CARGO_HOME"], "/run/build/opennow/cargo")
        self.assertNotIn("--share=network", application["build-options"].get("build-args", []))
        self.assertTrue(any(source.get("dest") == "cargo" for source in application["sources"]))
        archive = application["sources"][-1]
        self.assertEqual(archive["dest-filename"], "ffmpeg-source.tar.xz")
        self.assertEqual(application["build-options"]["env"]["OPENNOW_FFMPEG_ARCHIVE"],
                         "/run/build/opennow/ffmpeg-source.tar.xz")
        self.assertEqual(len(archive["sha256"]), 64)

    def test_sandbox_supports_native_streaming_without_host_filesystem_access(self):
        manifest = json.loads((PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_text())
        permissions = set(manifest["finish-args"])
        self.assertTrue({"--share=network", "--device=all", "--socket=wayland",
                         "--socket=fallback-x11", "--socket=pulseaudio",
                         "--filesystem=xdg-run/pipewire-0",
                         "--filesystem=xdg-pictures/OpenNOW:create",
                         "--talk-name=org.freedesktop.secrets"}.issubset(permissions))
        self.assertFalse({"--filesystem=host", "--filesystem=home",
                          "--socket=session-bus", "--socket=system-bus"} & permissions)

    def test_vendor_uses_both_lockfiles_and_sandbox_relative_sources(self):
        config = '[source.crates-io]\nreplace-with = "vendored-sources"\n' \
                 '[source.vendored-sources]\ndirectory = "vendor"\n'
        with tempfile.TemporaryDirectory() as directory, patch.object(PREPARE.subprocess, "run") as run:
            root = Path(directory)
            run.return_value = subprocess.CompletedProcess([], 0, stdout=config)
            PREPARE.vendor_sources(root)
            command = run.call_args.args[0]
            self.assertIn("--locked", command)
            self.assertIn(str(root / "native/opennow-core/Cargo.toml"), command)
            self.assertIn(str(root / "native/opennow-streamer/Cargo.toml"), command)
            generated = tomllib.loads((root / "build/flatpak/cargo/config.toml").read_text())
            self.assertEqual(generated["source"]["vendored-sources"]["directory"],
                             "/run/build/opennow/cargo/vendor")

    def test_vendor_failure_does_not_publish_configuration(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(PREPARE.subprocess, "run") as run:
            root = Path(directory)
            run.side_effect = subprocess.CalledProcessError(1, ["cargo", "vendor"])
            with self.assertRaises(subprocess.CalledProcessError):
                PREPARE.vendor_sources(root)
            self.assertFalse((root / "build/flatpak/cargo/config.toml").exists())

    def test_unexpected_cargo_config_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(PREPARE.subprocess, "run") as run:
            run.return_value = subprocess.CompletedProcess([], 0, stdout="")
            with self.assertRaises(ValueError):
                PREPARE.vendor_sources(Path(directory))


if __name__ == "__main__":
    unittest.main()
