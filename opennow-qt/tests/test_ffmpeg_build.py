import os
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]
ROOT = QT_SOURCE.parent
CRATE = ROOT / "native/opennow-streamer/vendor/ffmpeg-sys-next"
BUILD_SCRIPT = CRATE / "build.rs"


class FFmpegBuildContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.scratch = Path(cls.directory.name)
        source = BUILD_SCRIPT.read_text()
        validation_start = source.index("    if use_v4l2_request() {", source.index("    // run ./configure"))
        harness = cls.scratch / "source_policy.rs"
        harness.write_text(
            "use std::{env, fs, io, path::{Path, PathBuf}, process::Command};\n"
            + source[source.index("fn version()"):source.index("fn switch(")]
            + '\nfn validate(source_dir: PathBuf) -> io::Result<()> {\n'
            + source[validation_start:source.index("    // run make", validation_start)]
            + 'Ok(())\n}\n'
            + '\nfn main() {\n'
            'println!("{}", use_v4l2_request());\n'
            'println!("{}", source().display());\n'
            'println!("{}", search().display());\n'
            'if env::var_os("TEST_FETCH").is_some() { fetch().unwrap(); }\n'
            'if let Some(path) = env::var_os("TEST_CONFIG") { validate(path.into()).unwrap(); }\n'
            '}\n'
        )
        cls.policy = cls.scratch / "source_policy"
        subprocess.run(["rustc", "--edition=2024", str(harness), "-o", str(cls.policy)],
                       check=True, capture_output=True, text=True)

    def policy_run(self, target_os, arch, **extra):
        return subprocess.run(
            [str(self.policy)], capture_output=True, text=True,
            env={**os.environ, "CARGO_CFG_TARGET_OS": target_os,
                 "CARGO_CFG_TARGET_ARCH": arch, "CARGO_PKG_VERSION_MAJOR": "9",
                 "CARGO_PKG_VERSION_MINOR": "0", "OUT_DIR": str(self.scratch),
                 "CARGO_MANIFEST_DIR": str(CRATE), **extra},
        )

    def test_source_and_install_are_pinned_only_for_linux_arm64(self):
        for target_os, arch in (("linux", "aarch64"), ("linux", "x86_64"),
                                ("linux", "arm"), ("macos", "aarch64"),
                                ("windows", "aarch64"), ("android", "aarch64")):
            with self.subTest(target_os=target_os, arch=arch):
                result = self.policy_run(target_os, arch)
                self.assertEqual(result.returncode, 0, result.stderr)
                selected, source, install = result.stdout.splitlines()
                request = target_os == "linux" and arch == "aarch64"
                self.assertEqual(selected, str(request).lower())
                if request:
                    patchset = Path(install).name.removeprefix("dist-")
                    self.assertRegex(patchset, r"^v4l2-request-[0-9a-f]{16}$")
                    self.assertEqual(Path(source).name, f"ffmpeg-9.0-{patchset}")
                else:
                    self.assertEqual(Path(source).name, "ffmpeg-9.0")
                    self.assertEqual(Path(install).name, "dist")

    def test_patchset_name_changes_with_patch_contents(self):
        def install_name(crate):
            result = self.policy_run("linux", "aarch64", CARGO_MANIFEST_DIR=str(crate))
            self.assertEqual(result.returncode, 0, result.stderr)
            return Path(result.stdout.splitlines()[2]).name

        with tempfile.TemporaryDirectory() as directory:
            crate = Path(directory)
            patches = crate / "patches/v4l2-request"
            patches.mkdir(parents=True)
            (patches / "0001-first.patch").write_text("one\n")
            first = install_name(crate)
            (patches / "0001-first.patch").write_text("two\n")
            self.assertNotEqual(install_name(crate), first)

    def test_offline_archive_extracts_into_private_output_and_replaces_stale_sources(self):
        archive = self.scratch / "ffmpeg.tar.xz"
        with tarfile.open(archive, "w:xz") as output:
            entry = tarfile.TarInfo("ffmpeg-9.0/configure")
            data = b"#!/bin/sh\nexit 0\n"
            entry.size = len(data)
            entry.mode = 0o755
            output.addfile(entry, io.BytesIO(data))
        source = self.scratch / "ffmpeg-9.0"
        source.mkdir(exist_ok=True)
        (source / "stale").write_text("old source")
        result = self.policy_run("linux", "x86_64", TEST_FETCH="1", OPENNOW_FFMPEG_ARCHIVE=str(archive))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((source / "configure").read_bytes(), data)
        self.assertFalse((source / "stale").exists())

    def test_offline_archive_errors_do_not_fall_back_to_network_fetch(self):
        for name in ("absent.tar.xz", "invalid.tar.xz", "empty.tar.xz"):
            with self.subTest(name=name):
                archive = self.scratch / name
                if name == "invalid.tar.xz":
                    archive.write_bytes(b"not an archive")
                elif name == "empty.tar.xz":
                    with tarfile.open(archive, "w:xz"):
                        pass
                result = self.policy_run("linux", "x86_64", TEST_FETCH="1",
                                         OPENNOW_FFMPEG_ARCHIVE=str(archive))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("offline FFmpeg source extraction failed", result.stderr)

    def test_arm64_fetch_clones_the_release_tag_then_applies_every_patch_in_order(self):
        fake_bin = self.scratch / "bin"
        fake_bin.mkdir(exist_ok=True)
        git = fake_bin / "git"
        git.write_text('#!/bin/sh\nprintf "git %s\\n" "$*" >> "$TOOL_LOG"\n'
                       'for target; do :; done\nmkdir -p "$target"\n'
                       'exit "${CLONE_STATUS:-0}"\n')
        patch = fake_bin / "patch"
        patch.write_text('#!/bin/sh\nfor input; do :; done\n'
                         'printf "patch %s\\n" "$(basename "$input")" >> "$TOOL_LOG"\n'
                         'exit "${PATCH_STATUS:-0}"\n')
        for tool in (git, patch):
            tool.chmod(0o755)
        names = sorted(path.name for path in (CRATE / "patches/v4l2-request").glob("*.patch"))
        self.assertGreaterEqual(len(names), 3)
        for clone_status, patch_status in ((0, 0), (1, 0), (0, 1)):
            with self.subTest(clone_status=clone_status, patch_status=patch_status):
                log = self.scratch / f"tools-{clone_status}-{patch_status}.log"
                result = self.policy_run(
                    "linux", "aarch64", PATH=f"{fake_bin}:{os.environ['PATH']}",
                    TOOL_LOG=str(log), CLONE_STATUS=str(clone_status),
                    PATCH_STATUS=str(patch_status), TEST_FETCH="1",
                )
                commands = log.read_text().splitlines()
                self.assertTrue(commands[0].startswith(
                    "git clone --depth=1 -b n9.0 https://github.com/FFmpeg/FFmpeg "))
                if clone_status:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(len(commands), 1)
                elif patch_status:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(commands[1:], [f"patch {names[0]}"])
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(commands[1:], [f"patch {name}" for name in names])

    def test_arm64_offline_archive_is_patched_without_fuzz(self):
        archive = self.scratch / "ffmpeg-arm.tar.xz"
        with tarfile.open(archive, "w:xz") as output:
            for name, data in (("ffmpeg-9.0/configure", b"#!/bin/sh\nexit 0\n"),
                               ("ffmpeg-9.0/marker", b"before\n")):
                entry = tarfile.TarInfo(name)
                entry.size = len(data)
                entry.mode = 0o755
                output.addfile(entry, io.BytesIO(data))
        for marker, applies in (("before", True), ("other", False)):
            with self.subTest(applies=applies), tempfile.TemporaryDirectory() as directory:
                crate = Path(directory)
                patches = crate / "patches/v4l2-request"
                patches.mkdir(parents=True)
                (patches / "0001-marker.patch").write_text(
                    f"header\n--- a/marker\n+++ b/marker\n@@ -1 +1 @@\n-{marker}\n+after\n")
                result = self.policy_run("linux", "aarch64", TEST_FETCH="1",
                                         CARGO_MANIFEST_DIR=str(crate),
                                         OPENNOW_FFMPEG_ARCHIVE=str(archive))
                self.assertEqual(result.returncode == 0, applies, result.stderr)
                if applies:
                    source = Path(result.stdout.splitlines()[1])
                    self.assertEqual((source / "marker").read_text(), "after\n")

    def test_request_options_are_target_gated_and_portable_is_retained(self):
        source = BUILD_SCRIPT.read_text()
        start = source.index('    if use_v4l2_request() {', source.index('enable!(configure, "BUILD_DRM"'))
        options = source[start:source.index('\n    }', start)]
        for option in ("libdrm", "libudev", "v4l2-request"):
            self.assertIn(f'"--enable-{option}"', options)
        for option in ("v4l2-m2m", "sand"):
            self.assertNotIn(f'"--enable-{option}"', options)
        manifest = ROOT / "native/opennow-streamer/crates/opennow-streamer-platform-linux/Cargo.toml"
        self.assertIn('"ffmpeg-sys-next/build-portable"', manifest.read_text())
        self.assertIn('"ffmpeg-sys-next/build-drm"', manifest.read_text())
        self.assertNotIn("cargo:rpi=true", source)
        bundled = source[source.index('let include_paths: Vec<PathBuf> = if env::var("CARGO_FEATURE_BUILD")'):]
        self.assertIn('println!("cargo:rerun-if-changed=patches/v4l2-request");',
                      bundled[:bundled.index('link_to_libraries(statik, &target_os);')])

    def test_missing_configured_capabilities_are_rejected(self):
        config = self.scratch / "config"
        config.mkdir()
        core = ("CONFIG_V4L2_REQUEST",)
        hwaccels = ("CONFIG_H264_V4L2REQUEST_HWACCEL", "CONFIG_HEVC_V4L2REQUEST_HWACCEL",
                    "CONFIG_AV1_V4L2REQUEST_HWACCEL")
        for missing in (None, *core, *hwaccels):
            with self.subTest(missing=missing):
                (config / "config.h").write_text("".join(
                    f"#define {capability} {int(capability != missing)}\n" for capability in core
                ))
                (config / "config_components.h").write_text("".join(
                    f"#define {capability} {int(capability != missing)}\n"
                    for capability in hwaccels
                ))
                result = self.policy_run("linux", "aarch64", TEST_CONFIG=str(config))
                self.assertEqual(result.returncode == 0, missing is None, result.stderr)
                if missing:
                    self.assertIn(f"configuration is missing #define {missing} 1", result.stderr)

    def test_cmake_requires_request_dependencies_only_for_linux_arm64(self):
        for arch, target, expected in (("aarch64", "", True), ("arm64", "", True),
                                       ("x86_64", "", False),
                                       ("x86_64", "aarch64-unknown-linux-gnu", True),
                                       ("aarch64", "x86_64-unknown-linux-gnu", False)):
            with self.subTest(arch=arch, target=target), tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                (path / "main.cpp").write_text("int main() { return 0; }\n")
                (path / "FindPkgConfig.cmake").write_text(
                    'function(pkg_check_modules prefix)\n'
                    'file(APPEND "${CMAKE_BINARY_DIR}/packages.txt" "${ARGV}\\n")\n'
                    'endfunction()\n'
                )
                (path / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(FFmpegBuildContract LANGUAGES CXX)
add_executable(opennow-qt main.cpp)
set(CMAKE_MODULE_PATH "{path.as_posix()}")
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR "{arch}")
set(OPENNOW_RUST_TARGET "{target}")
set(CARGO_EXECUTABLE cargo)
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
include("{QT_SOURCE.as_posix()}/cmake/NativeRuntime.cmake")
''')
                result = subprocess.run(["cmake", "-S", directory, "-B", str(path / "build")],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                packages = (path / "build/packages.txt").read_text()
                self.assertIn("OPENNOW_VAAPI;REQUIRED;libva;libva-drm;libdrm", packages)
                self.assertEqual("OPENNOW_V4L2_REQUEST;REQUIRED;libdrm;libudev" in packages, expected)


if __name__ == "__main__":
    unittest.main()
