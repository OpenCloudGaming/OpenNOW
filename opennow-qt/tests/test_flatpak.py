import base64
import fnmatch
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import tomllib
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
PACKAGING = ROOT / "opennow-qt/packaging/flatpak"
SPEC = importlib.util.spec_from_file_location("prepare_sources", PACKAGING / "prepare_sources.py")
PREPARE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREPARE)


class FlatpakTests(unittest.TestCase):
    def test_sticky_state_keeps_sdk_build_cache_and_app_on_one_filesystem(self):
        workflow = (ROOT / ".github/workflows/qt-flatpak.yml").read_text()
        mount = workflow.split("      - name: Mount Flatpak SDK and build state\n", 1)[1].split("      - name:", 1)[0]
        self.assertIn("uses: useblacksmith/stickydisk@3f6be1451e3cad893b778bc93495c80452a5da26", mount)
        self.assertIn("key: opennow-flatpak-v1-${{ inputs.arch }}-", mount)
        self.assertIn("          path: build/flatpak\n", mount)
        self.assertIn("github.event_name == 'push' || github.event_name == 'workflow_dispatch'", mount)
        self.assertIn("github.ref == 'refs/heads/dev' || github.ref == 'refs/heads/main'", mount)
        self.assertIn("FLATPAK_USER_DIR: ${{ github.workspace }}/build/flatpak/user", workflow)
        self.assertIn("CARGO_HOME: ${{ github.workspace }}/build/flatpak/cargo-home", workflow)
        self.assertLess(workflow.index("Mount Flatpak SDK and build state"), workflow.index("Install Flatpak tools and SDK"))
        self.assertIn("flatpak update --user --noninteractive -y", workflow)
        self.assertIn("--state-dir=build/flatpak/state --repo=build/flatpak/repo", workflow)
        self.assertIn("build/flatpak/app opennow-qt/packaging/flatpak/", workflow)
        self.assertIn("flatpak-builder --user --force-clean", workflow)
        self.assertNotIn("--disable-cache", workflow)

    def test_release_manifest_pins_version_and_public_key_without_changing_sources(self):
        public_key = base64.b64encode(bytes(range(32))).decode()
        original = (PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_bytes()
        for version in ("1.2.3", "1.2.3-nightly.123.2", "1.2.3-supporter.123.2"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                packaging = root / "opennow-qt/packaging/flatpak"
                packaging.mkdir(parents=True)
                template = packaging / "io.github.opencloudgaming.OpenNOW.json"
                template.write_bytes(original)
                destination = PREPARE.prepare_manifest(root, version, public_key)
                manifest = json.loads(destination.read_text())
                application = manifest["modules"][-1]
                self.assertEqual(application["config-opts"], [
                    "-DCMAKE_BUILD_TYPE=Release",
                    "-DBUILD_TESTING=OFF",
                    f"-DOPENNOW_BUILD_VERSION={version}",
                    f"-DOPENNOW_UPDATE_ED25519_PUBLIC_KEY={public_key}",
                ])
                self.assertEqual(destination.parent, template.parent)
                del application["config-opts"][2:]
                self.assertEqual(manifest, json.loads(original))
                self.assertEqual(template.read_bytes(), original)
                generated = destination.read_bytes()
                PREPARE.prepare_manifest(root, version, public_key)
                self.assertEqual(destination.read_bytes(), generated)

    def test_release_manifest_rejects_invalid_identity_before_writing(self):
        public_key = base64.b64encode(bytes(32)).decode()
        for version, key in (("01.2.3", public_key), ("1.2.3-nightly.0.1", public_key),
                             ("1.2.3\n", public_key), ("1.2.3", "invalid"),
                             ("1.2.3", base64.b64encode(bytes(31)).decode())):
            with self.subTest(version=version, key=key), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaises(ValueError):
                    PREPARE.prepare_manifest(root, version, key)
                self.assertEqual(list(root.iterdir()), [])

    def test_release_workflows_require_both_flatpak_architectures_before_inventory(self):
        for workflow, dependency, version in (
                ("qt-build.yml", "metadata", "needs.metadata.outputs.version"),
                ("qt-release-candidate.yml", "preflight", "inputs.version")):
            with self.subTest(workflow=workflow):
                contents = (ROOT / ".github/workflows" / workflow).read_text()
                inventory_header = "  artifact-inventory:\n" if workflow == "qt-build.yml" else "  inventory:\n"
                flatpak = contents.split("  flatpak:\n", 1)[1].split(inventory_header, 1)[0]
                self.assertIn(f"needs: {dependency}", flatpak)
                self.assertIn("arch: [x86_64, aarch64]", flatpak)
                self.assertIn("uses: ./.github/workflows/qt-flatpak.yml", flatpak)
                self.assertIn("version: ${{ " + version + " }}", flatpak)
                self.assertIn("update_public_key: ${{ inputs.update_public_key }}", flatpak)
                inventory = contents.split(inventory_header, 1)[1]
                self.assertIn(", flatpak]", inventory)
                self.assertNotIn("continue-on-error", flatpak + inventory)
        reusable = (ROOT / ".github/workflows/qt-flatpak.yml").read_text()
        self.assertIn("  workflow_call:\n", reusable)
        self.assertIn("  workflow_dispatch:\n", reusable)
        self.assertIn("ref: ${{ github.sha }}", reusable)
        self.assertNotIn("secrets.", reusable)
        self.assertIn("--version \"$RELEASE_VERSION\" --update-public-key \"$OPENNOW_UPDATE_PUBLIC_KEY\"", reusable)
        self.assertIn("name: opennow-qt-${{ steps.identity.outputs.version }}-linux-${{ steps.identity.outputs.package_arch }}-flatpak-unsigned", reusable)
        self.assertIn("path: ${{ steps.identity.outputs.bundle }}", reusable)
        self.assertLess(reusable.index("Install and verify the exported bundle"),
                        reusable.index("actions/upload-artifact"))

    def test_workflow_bundle_identity_matches_release_inventory_on_each_architecture(self):
        workflow = (ROOT / ".github/workflows/qt-flatpak.yml").read_text()
        step = workflow.split("      - name: Verify native build architecture\n", 1)[1]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1].split("      - uses:", 1)[0])
        for arch, package_arch in (("x86_64", "x64"), ("aarch64", "arm64")):
            for version in ("1.2.3", "1.2.3-nightly.123.2", ""):
                with self.subTest(arch=arch, version=version), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    uname = root / "uname"
                    uname.write_text('#!/bin/sh\nprintf "%s\\n" "$FLATPAK_ARCH"\n')
                    uname.chmod(0o700)
                    output = root / "output"
                    subprocess.run(["bash", "-euo", "pipefail", "-c", script], check=True, cwd=ROOT,
                                   env={**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                                        "FLATPAK_ARCH": arch, "RELEASE_VERSION": version,
                                        "GITHUB_RUN_NUMBER": "123", "GITHUB_RUN_ATTEMPT": "2",
                                        "GITHUB_OUTPUT": str(output)})
                    identity = dict(line.split("=", 1) for line in output.read_text().splitlines())
                    project = (ROOT / "opennow-qt/CMakeLists.txt").read_text()
                    resolved = version or project.split("project(OpenNOWQt VERSION ", 1)[1].split()[0]
                    self.assertEqual(identity, {
                        "version": resolved, "package_arch": package_arch,
                        "bundle": f"build/flatpak/OpenNOW-Qt-{resolved}-Linux-{package_arch}.flatpak",
                    })

    def test_every_architecture_uses_the_ffmpeg_release_the_request_patches_target(self):
        manifest = json.loads((PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_text())
        sources = manifest["modules"][-1]["sources"]
        archives = [source for source in sources if source.get("dest-filename") == "ffmpeg-source.tar"]
        self.assertEqual(len(archives), 1)
        archive = archives[0]
        self.assertNotIn("only-arches", archive)
        self.assertEqual(archive["url"], "https://ffmpeg.org/releases/ffmpeg-9.0.tar.xz")
        self.assertEqual(len(archive["sha256"]), 64)
        patches = ROOT / "native/opennow-streamer/vendor/ffmpeg-sys-next/patches/v4l2-request"
        self.assertTrue(sorted(patches.glob("*.patch")))

    def test_export_excludes_oversized_icons_and_preserves_supported_icons(self):
        manifest = json.loads((PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_text())
        cleanup = manifest.get("cleanup", [])
        for size in (16, 24, 32, 48, 64, 128, 256, 512, 1024):
            path = f"/share/icons/hicolor/{size}x{size}/apps/{manifest['app-id']}.png"
            removed = any(fnmatch.fnmatch(path, pattern.rstrip("/") + "/*") for pattern in cleanup)
            self.assertEqual(removed, size > 512, path)

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
        self.assertEqual(archive["dest-filename"], "ffmpeg-source.tar")
        self.assertEqual(application["build-options"]["env"]["OPENNOW_FFMPEG_ARCHIVE"],
                         "/run/build/opennow/ffmpeg-source.tar")
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

    def test_platform_selection_ignores_host_qt_qpa_platform(self):
        manifest = json.loads((PACKAGING / "io.github.opencloudgaming.OpenNOW.json").read_text())
        self.assertIn("--env=QT_QPA_PLATFORM=wayland;xcb", manifest["finish-args"])

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
