import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from run_update_helper_integration import MANIFEST, build_fixtures, build_update_helper, target_directory_for


class UpdateHelperDriverTest(unittest.TestCase):
    @patch("run_update_helper_integration.run")
    def test_fixture_compiler_uses_cargo_host_linker(self, run):
        environment = {
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER": "C:/Program Files/MSVC/link.exe",
            "CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER": "C:/Program Files/ARM64/link.exe",
        }
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            build_fixtures(directory, environment)
            run.assert_called_once_with([
                "cargo", "build", "--offline", "--manifest-path", str(directory / "Cargo.toml"), "--bins",
            ], environment)

    @patch("run_update_helper_integration.run")
    def test_fixture_compiler_keeps_default_without_host_override(self, run):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            build_fixtures(directory, {})
            self.assertEqual(run.call_args.args[1], {})
            self.assertEqual((directory / "Cargo.lock").read_bytes(), MANIFEST.with_name("Cargo.lock").read_bytes())
            manifest = (directory / "Cargo.toml").read_text()
            self.assertIn('path = "candidate.rs"', manifest)
            self.assertIn('path = "previous.rs"', manifest)
            self.assertIn("[dependencies]\nopennow-core = { path = ", manifest)

    @patch("run_update_helper_integration.run")
    def test_fixture_compiler_propagates_cargo_failures(self, run):
        run.side_effect = subprocess.CalledProcessError(1, ["cargo", "build"])
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(subprocess.CalledProcessError):
                build_fixtures(Path(temporary), {})

    def test_ci_target_directory_reuses_the_restored_core_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            configured = directory / "cargo-target"
            environment = {"OPENNOW_UPDATE_TEST_TARGET_DIR": str(configured)}
            self.assertEqual(target_directory_for(directory, environment), configured)

    def test_local_target_directory_stays_temporary(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.assertEqual(target_directory_for(directory, {}), directory / "target")

    @patch("run_update_helper_integration.run")
    def test_cached_helper_build_cleans_workspace_crate_before_recompiling(self, run):
        environment = {
            "CARGO_TARGET_DIR": "cache/core-target",
            "OPENNOW_UPDATE_TEST_TARGET_DIR": "cache/core-target",
            "OPENNOW_UPDATE_ED25519_PUBLIC_KEY": "ephemeral-key",
        }
        build_update_helper(environment)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args_list[0].args, ([
            "cargo", "clean", "--manifest-path", str(MANIFEST), "-p", "opennow-core",
        ], environment))
        self.assertEqual(run.call_args_list[1].args, ([
            "cargo", "build", "--locked", "--manifest-path", str(MANIFEST),
            "--lib", "--bin", "opennow-update-helper",
        ], environment))

    @patch("run_update_helper_integration.run")
    def test_local_helper_build_preserves_fully_isolated_target(self, run):
        environment = {"CARGO_TARGET_DIR": "temporary/target"}
        build_update_helper(environment)
        run.assert_called_once_with([
            "cargo", "build", "--locked", "--manifest-path", str(MANIFEST),
            "--lib", "--bin", "opennow-update-helper",
        ], environment)

    @patch("run_update_helper_integration.run")
    def test_cached_helper_build_stops_when_clean_fails(self, run):
        run.side_effect = subprocess.CalledProcessError(1, ["cargo", "clean"])
        environment = {
            "CARGO_TARGET_DIR": "cache/core-target",
            "OPENNOW_UPDATE_TEST_TARGET_DIR": "cache/core-target",
        }
        with self.assertRaises(subprocess.CalledProcessError):
            build_update_helper(environment)
        self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
