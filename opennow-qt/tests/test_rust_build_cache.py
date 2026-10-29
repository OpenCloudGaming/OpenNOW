import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("rust_build_cache", ROOT / ".github/scripts/rust-build-cache.py")
CACHE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CACHE)


class RustBuildCacheTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cargo"), "Cargo is required for the cache freshness check")
    def test_real_cargo_skips_unchanged_checkout_and_rebuilds_changed_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = root / "native/opennow-core"
            (workspace / "src").mkdir(parents=True)
            (workspace / "Cargo.toml").write_text('[package]\nname="cache-proof"\nversion="0.1.0"\nedition="2024"\n')
            source = workspace / "src/lib.rs"
            source.write_text("pub fn answer() -> u32 { 42 }\n")
            obsolete = workspace / "obsolete.txt"
            obsolete.write_text("an obsolete build input")
            (workspace / "build.rs").write_text('''fn main() {
                let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("build-count");
                let count: u32 = std::fs::read_to_string(&path).unwrap_or_default().parse().unwrap_or(0);
                std::fs::write(path, (count + 1).to_string()).unwrap();
            }''')
            for command in (["git", "init", "-q"],
                            ["cargo", "generate-lockfile", "--manifest-path", str(workspace / "Cargo.toml")],
                            ["git", "add", "native"]):
                subprocess.run(command, cwd=root, check=True, capture_output=True, timeout=60)
            manifest = root / "disk/source-mtimes.json"
            target = workspace / "target"
            command = ["cargo", "build", "--manifest-path", str(workspace / "Cargo.toml"), "--verbose"]
            CACHE.restore_source_mtimes(root, manifest, [target])
            subprocess.run(command, cwd=root, check=True, capture_output=True, timeout=60)
            count = next(target.glob("debug/build/cache-proof-*/out/build-count"))
            self.assertEqual(count.read_text(), "1")
            for relative, previous in json.loads(manifest.read_text()).items():
                os.utime(root / relative, ns=(previous["mtime_ns"] + 1_000_000, previous["mtime_ns"] + 1_000_000))
            CACHE.restore_source_mtimes(root, manifest, [target])
            warm = subprocess.run(command, cwd=root, check=True, capture_output=True, text=True, timeout=60)
            self.assertIn("Fresh cache-proof", warm.stderr)
            self.assertEqual(count.read_text(), "1")
            source.write_text("pub fn answer() -> u32 { 43 }\n")
            CACHE.restore_source_mtimes(root, manifest, [target])
            subprocess.run(command, cwd=root, check=True, capture_output=True, timeout=60)
            self.assertEqual(count.read_text(), "2")
            obsolete.unlink()
            CACHE.restore_source_mtimes(root, manifest, [target])
            subprocess.run(command, cwd=root, check=True, capture_output=True, timeout=60)
            self.assertEqual(count.read_text(), "3")

    def environment(self, root):
        return {
            "GITHUB_WORKSPACE": str(root), "CARGO_HOME": str(root / "cargo-home"),
            "RUNNER_TEMP": str(root / "temp"), "GITHUB_OUTPUT": str(root / "output"),
            "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64", "GITHUB_REPOSITORY": "owner/repo",
            "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/dev", "GITHUB_SHA": "a" * 40,
            "RUST_CACHE_SHARED_KEY": "qt-checks-linux", "RUST_CACHE_WORKSPACES": "native/core -> target",
        }

    def fixture(self, root):
        workspace = root / "native/core"
        workspace.mkdir(parents=True)
        (workspace / "Cargo.toml").write_text('[package]\nname = "fixture"\nversion = "0.1.0"\n')
        (workspace / "Cargo.lock").write_text("lock-v1")
        (root / "temp").mkdir()
        return workspace

    def test_swatinem_paths_are_relative_to_each_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = self.fixture(root)
            workspaces, targets = CACHE.workspace_targets(root, "\n".join([
                "native/core -> target", "native/core -> ../../build/qt/rust-target",
                "native/core -> ../../build/qt/peer-probe-rust-target", "native/core -> target",
            ]))
            self.assertEqual(workspaces, [workspace])
            self.assertCountEqual(targets, [workspace / "target", root / "build/qt/rust-target",
                                            root / "build/qt/peer-probe-rust-target"])

    def test_invalid_mappings_cannot_cache_sources_or_outside_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            self.fixture(root)
            for mapping in ("", "native/core", "native/core ->", "native/core -> /tmp/cache",
                            "native/core -> ../../../outside", "native/core -> .",
                            "native/core -> ../..", "missing -> target",
                            "native/core -> target\nnative/core -> target/nested"):
                with self.subTest(mapping=mapping), self.assertRaises(ValueError):
                    CACHE.workspace_targets(root, mapping)

    def test_target_parents_cannot_escape_through_symlinks(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            root = Path(directory).resolve()
            self.fixture(root)
            (root / "build").symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                CACHE.workspace_targets(root, "native/core -> ../../build/target")

    def test_only_trusted_branch_events_at_checked_out_commit_can_write(self):
        env = self.environment(Path("/fixture"))
        for event in ("push", "workflow_dispatch", "pull_request", "pull_request_target", "schedule"):
            for ref in ("refs/heads/dev", "refs/heads/main", "refs/heads/feature", "refs/tags/v1", "refs/pull/7/merge"):
                with self.subTest(event=event, ref=ref):
                    current = {**env, "GITHUB_EVENT_NAME": event, "GITHUB_REF": ref}
                    expected = event in {"push", "workflow_dispatch"} and ref in {"refs/heads/dev", "refs/heads/main"}
                    self.assertEqual(CACHE.trusted_writer(current, env["GITHUB_SHA"]), expected)
                    self.assertFalse(CACHE.trusted_writer({**current, "RUST_CACHE_SAVE_IF": "false"}, env["GITHUB_SHA"]))
                    self.assertFalse(CACHE.trusted_writer(current, "b" * 40))

    def test_new_commits_refresh_archives_without_changing_sticky_identity(self):
        env = self.environment(Path("/fixture"))
        first = CACHE.cache_keys(env, {"rustc": "1.90"}, {"Cargo.lock": "one"}, "a" * 40)
        second = CACHE.cache_keys(env, {"rustc": "1.90"}, {"Cargo.lock": "one"}, "b" * 40)
        changed_lock = CACHE.cache_keys(env, {"rustc": "1.90"}, {"Cargo.lock": "two"}, "b" * 40)
        self.assertEqual(first["disk-key"], second["disk-key"])
        self.assertEqual(first["disk-key"], changed_lock["disk-key"])
        self.assertNotEqual(first["archive-key"], second["archive-key"])
        self.assertNotEqual(second["archive-key"], changed_lock["archive-key"])
        lock_prefix, environment_prefix = first["restore-keys"].splitlines()
        self.assertTrue(second["archive-key"].startswith(lock_prefix))
        self.assertTrue(changed_lock["archive-key"].startswith(environment_prefix))

    def test_cache_identity_isolates_platform_tools_and_caller(self):
        env = self.environment(Path("/fixture"))
        identity = {"rustc": "1.90", "clang": "20", "abi": "-Ctarget-feature=+crt-static"}
        base = CACHE.cache_keys(env, identity, {}, "a")["disk-key"]
        for variable, value in (("RUNNER_OS", "Windows"), ("RUNNER_ARCH", "ARM64"),
                                ("RUST_CACHE_SHARED_KEY", "qt-package"), ("GITHUB_REPOSITORY", "other/repo")):
            self.assertNotEqual(CACHE.cache_keys({**env, variable: value}, identity, {}, "a")["disk-key"], base)
        for variable, value in (("rustc", "1.91"), ("clang", "21"), ("abi", "different")):
            self.assertNotEqual(CACHE.cache_keys(env, {**identity, variable: value}, {}, "a")["disk-key"], base)

    def test_abi_environment_excludes_secrets_and_per_run_values(self):
        for variable in ("RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "CARGO_ENCODED_RUSTFLAGS", "CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER",
                         "CARGO_PROFILE_RELEASE_LTO", "CC_aarch64_unknown_linux_gnu", "LIBCLANG_PATH",
                         "MACOSX_DEPLOYMENT_TARGET", "WindowsSDKVersion", "SDKROOT"):
            self.assertIsNotNone(CACHE.ABI_VARIABLES.fullmatch(variable), variable)
        for variable in ("GITHUB_TOKEN", "CARGO_REGISTRIES_PRIVATE_TOKEN", "AWS_SECRET_ACCESS_KEY",
                         "GITHUB_SHA", "CARGO_BUILD_JOBS", "OPENNOW_BUILD_VERSION", "OPENNOW_UPDATE_PUBLIC_KEY"):
            self.assertIsNone(CACHE.ABI_VARIABLES.fullmatch(variable), variable)

    def test_actual_rustc_and_configured_native_tool_binaries_are_probed(self):
        with tempfile.TemporaryDirectory() as directory:
            compiler = Path(directory) / "custom-cc"
            compiler.write_bytes(b"native compiler version one")
            env = {"RUNNER_OS": "Linux", "CC": str(compiler), "RUSTC": "custom-rustc"}
            locate = lambda tool: str(compiler) if tool == str(compiler) else None
            with patch.object(CACHE.shutil, "which", side_effect=locate), \
                    patch.object(CACHE, "command_output", return_value="tool version") as probe:
                first = CACHE.tool_identity(env)
                probe.assert_any_call(["custom-rustc", "-vV"], required=True)
                probe.assert_any_call([str(compiler), "--version"])
                compiler.write_bytes(b"native compiler version two")
                second = CACHE.tool_identity(env)
            self.assertNotEqual(first[str(compiler)]["binary"], second[str(compiler)]["binary"])

    def test_prepare_selects_only_downloads_and_complete_target_trees(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = self.fixture(root)
            env = self.environment(root)
            with patch.object(CACHE, "tool_identity", return_value={"rustc": "fixture"}), \
                    patch.object(CACHE, "command_output", return_value=env["GITHUB_SHA"]):
                CACHE.prepare(env)
            output = (root / "output").read_text()
            for expected in (str(workspace / "target"), str(root / "cargo-home/registry"), str(root / "cargo-home/git")):
                self.assertIn(expected, output)
            for forbidden in ("cargo-home/bin", "cargo-home/config", "cargo-home/credentials"):
                self.assertNotIn(forbidden, output)
            plan = json.loads((root / "temp/opennow-rust-cache-plan.json").read_text())
            self.assertEqual(plan["save"], "true")
            for entry in plan["links"]:
                self.assertTrue(Path(entry["destination"]).is_relative_to(root / "temp/opennow-rust-cache"))

    def test_vendored_native_recipe_or_probe_dependencies_select_a_new_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            self.fixture(root)
            env = self.environment(root)
            snapshots = []
            for vendor_tree in ("old-ffmpeg-and-str0m-tree", "new-ffmpeg-recipe", "new-str0m-source"):
                def output(command, required=False):
                    return vendor_tree if "ls-tree" in command else env["GITHUB_SHA"]
                with patch.object(CACHE, "tool_identity", return_value={"rustc": "fixture"}), \
                        patch.object(CACHE, "command_output", side_effect=output) as probe:
                    CACHE.prepare(env)
                    probe.assert_any_call([
                        "git", "-C", str(root), "ls-tree", "HEAD", "--", "native/opennow-streamer/vendor",
                    ], required=True)
                plan = json.loads((root / "temp/opennow-rust-cache-plan.json").read_text())
                snapshots.append(plan["disk-key"])
            self.assertEqual(len(set(snapshots)), 3)

    def test_linking_preserves_native_outputs_fingerprints_and_timestamps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "checkout/target"
            destination = root / "disk/targets/core"
            files = ["release/build/ffmpeg-sys/out/lib/libavcodec.a", "release/.fingerprint/ffmpeg-sys/build-script",
                     "release/deps/libcore.rlib", "release/app"]
            for name in files:
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            mtimes = {name: (source / name).stat().st_mtime_ns for name in files}
            CACHE.link_directory(source, destination)
            CACHE.link_directory(source, destination)
            self.assertTrue(source.is_symlink())
            self.assertEqual(source.resolve(), destination)
            for name in files:
                self.assertEqual((source / name).read_text(), name)
                self.assertEqual((source / name).stat().st_mtime_ns, mtimes[name])
            source.unlink()
            CACHE.link_directory(source, destination)
            self.assertTrue((source / files[0]).is_file())

    def test_linking_merges_existing_downloads_without_deleting_cached_entries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source, destination = root / "cargo/registry", root / "disk/cargo/registry"
            source.mkdir(parents=True)
            destination.mkdir(parents=True)
            (source / "new").write_text("new")
            (destination / "old").write_text("old")
            CACHE.link_directory(source, destination)
            self.assertEqual(sorted(path.name for path in source.iterdir()), ["new", "old"])

    def test_unrelated_symlinks_are_not_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "source"
            source.symlink_to(root / "unrelated")
            with self.assertRaises(ValueError):
                CACHE.link_directory(source, root / "disk")

    def test_matching_source_bytes_reuse_only_the_recorded_mtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "native/opennow-streamer/vendor/opus/encoder.c"
            source.parent.mkdir(parents=True)
            source.write_text("unchanged vendored C source")
            original = 1_600_000_000_000_000_000
            fresh = original + 100_000_000_000
            os.utime(source, ns=(original, original))
            manifest = root / "disk/source-mtimes.json"
            tracked = b"native/opennow-streamer/vendor/opus/encoder.c\0"
            with patch.object(CACHE.subprocess, "check_output", return_value=tracked):
                CACHE.restore_source_mtimes(root, manifest, [])
                recorded = source.stat().st_mtime_ns
                os.utime(source, ns=(fresh, fresh))
                CACHE.restore_source_mtimes(root, manifest, [])
                self.assertEqual(source.stat().st_mtime_ns, recorded)
                self.assertEqual(source.read_text(), "unchanged vendored C source")
                CACHE.restore_source_mtimes(root, manifest, [])
                self.assertEqual(source.stat().st_mtime_ns, recorded)
            record = json.loads(manifest.read_text())[str(source.relative_to(root))]
            self.assertEqual(record["sha256"], CACHE.file_digest(source))
            self.assertEqual(record["mtime_ns"], recorded)

    def test_changed_new_and_deleted_sources_update_manifest_without_reusing_mtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = root / "native/opennow-core"
            workspace.mkdir(parents=True)
            changed, deleted, new = (workspace / name for name in ("changed.rs", "deleted.rs", "new.rs"))
            original = 1_600_000_000_000_000_000
            fresh = original + 100_000_000_000
            for path in (changed, deleted):
                path.write_text("old")
                os.utime(path, ns=(original, original))
            manifest = root / "disk/source-mtimes.json"
            tracked = b"native/opennow-core/changed.rs\0native/opennow-core/deleted.rs\0native/opennow-core/new.rs\0"
            with patch.object(CACHE.subprocess, "check_output", return_value=tracked):
                CACHE.restore_source_mtimes(root, manifest, [])
                changed.write_text("new")
                new.write_text("new file")
                deleted.unlink()
                for path in (changed, new):
                    os.utime(path, ns=(fresh, fresh))
                CACHE.restore_source_mtimes(root, manifest, [])
            records = json.loads(manifest.read_text())
            self.assertNotIn(str(deleted.relative_to(root)), records)
            for path in (changed, new):
                self.assertGreaterEqual(path.stat().st_mtime_ns, fresh)
                self.assertEqual(records[str(path.relative_to(root))]["mtime_ns"], path.stat().st_mtime_ns)
                self.assertEqual(records[str(path.relative_to(root))]["sha256"], CACHE.file_digest(path))

    def test_executable_bit_changes_keep_fresh_source_timestamps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "native/opennow-core/build-tool.sh"
            source.parent.mkdir(parents=True)
            source.write_text("printf 'build tool'")
            original = 1_600_000_000_000_000_000
            fresh = original + 100_000_000_000
            source.chmod(0o644)
            os.utime(source, ns=(original, original))
            manifest = root / "disk/source-mtimes.json"
            with patch.object(CACHE.subprocess, "check_output", return_value=b"native/opennow-core/build-tool.sh\0"):
                CACHE.restore_source_mtimes(root, manifest, [])
                checksum = CACHE.file_digest(source)
                source.chmod(0o755)
                os.utime(source, ns=(fresh, fresh))
                CACHE.restore_source_mtimes(root, manifest, [])
            record = json.loads(manifest.read_text())[str(source.relative_to(root))]
            self.assertGreaterEqual(source.stat().st_mtime_ns, fresh)
            self.assertEqual(record["sha256"], checksum)
            self.assertEqual(record["mode"], 0o755)
            self.assertEqual(record["mtime_ns"], source.stat().st_mtime_ns)

    def test_source_symlinks_and_symlinked_parent_directories_are_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            workspace = root / "native/opennow-streamer"
            workspace.mkdir(parents=True)
            outside = root / "outside"
            outside.mkdir()
            protected = outside / "source.c"
            protected.write_text("outside source")
            fresh = 1_700_000_000_000_000_000
            os.utime(protected, ns=(fresh, fresh))
            (workspace / "source.c").symlink_to(protected)
            (workspace / "vendor").symlink_to(outside, target_is_directory=True)
            manifest = root / "disk/source-mtimes.json"
            manifest.parent.mkdir()
            names = ["native/opennow-streamer/source.c", "native/opennow-streamer/vendor/source.c"]
            record = {"sha256": CACHE.file_digest(protected), "mtime_ns": fresh - 100_000_000_000}
            manifest.write_text(json.dumps({name: record for name in names}))
            tracked = b"\0".join(name.encode() for name in names) + b"\0"
            with patch.object(CACHE.subprocess, "check_output", return_value=tracked), \
                    patch.object(CACHE, "file_digest", side_effect=AssertionError("Symlink content was read")):
                CACHE.restore_source_mtimes(root, manifest, [])
            self.assertEqual(protected.stat().st_mtime_ns, fresh)
            self.assertEqual(json.loads(manifest.read_text()), {})

    def test_real_git_enumeration_ignores_untracked_sources_and_all_target_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            names = ["native/opennow-streamer/vendor/source.c", "native/opennow-core/target/release/lib.a",
                     "native/opennow-streamer/custom-target/output.a", "other/source.c"]
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(name)
            subprocess.run(["git", "-C", str(root), "add", "--", *names], check=True)
            untracked = root / "native/opennow-streamer/untracked.c"
            untracked.write_text("untracked")
            before = {name: (root / name).stat().st_mtime_ns for name in names}
            manifest = root / "disk/source-mtimes.json"
            CACHE.restore_source_mtimes(root, manifest, [root / "native/opennow-streamer/custom-target"])
            self.assertEqual(set(json.loads(manifest.read_text())), {names[0]})
            self.assertGreaterEqual((root / names[0]).stat().st_mtime_ns, before[names[0]])
            for name in names[1:]:
                self.assertEqual((root / name).stat().st_mtime_ns, before[name])

    def test_corrupt_source_manifest_is_a_cache_miss(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "native/opennow-core/source.rs"
            source.parent.mkdir(parents=True)
            source.write_text("source")
            original = source.stat().st_mtime_ns
            manifest = root / "source-mtimes.json"
            for content in ("not JSON", "[]", '{"native/opennow-core/source.rs": {"sha256": "wrong", "mtime_ns": 1}}'):
                manifest.write_text(content)
                with patch.object(CACHE.subprocess, "check_output", return_value=b"native/opennow-core/source.rs\0"):
                    CACHE.restore_source_mtimes(root, manifest, [])
                self.assertGreaterEqual(source.stat().st_mtime_ns, original)
                self.assertEqual(json.loads(manifest.read_text())["native/opennow-core/source.rs"]["mtime_ns"], source.stat().st_mtime_ns)


if __name__ == "__main__":
    unittest.main()
