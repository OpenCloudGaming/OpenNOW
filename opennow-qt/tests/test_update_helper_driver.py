import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from run_update_helper_integration import MANIFEST, build_fixtures


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


if __name__ == "__main__":
    unittest.main()
