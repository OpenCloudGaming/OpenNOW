import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/scripts/prepare-macos-target.sh"


class MacOSCrossHostTest(unittest.TestCase):
    def prepare(self, host, target, expected_host, rosetta=True):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        commands = {
            "uname": f"printf '%s\\n' '{host}'",
            "rustc": f"printf 'host: {'aarch64' if host == 'arm64' else 'x86_64'}-apple-darwin\\n'",
            "arch": 'echo "arch $*" >> "$COMMAND_LOG"; test ! -f "$ROSETTA_MISSING"',
            "softwareupdate": 'echo "softwareupdate $*" >> "$COMMAND_LOG"; rm -f "$ROSETTA_MISSING"',
            "xcrun": "printf '/Applications/Xcode.app/Contents/Developer/SDKs/MacOSX.sdk\\n'",
        }
        for name, command in commands.items():
            path = root / name
            path.write_text("#!/bin/sh\n" + command + "\n")
            path.chmod(0o700)
        output = root / "env"
        output.touch()
        missing = root / "missing-rosetta"
        if not rosetta:
            missing.touch()
        log = root / "commands"
        result = subprocess.run(["bash", str(SCRIPT)], capture_output=True, text=True, env={
            **os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
            "OPENNOW_MACOS_ARCH": target, "OPENNOW_MACOS_HOST_ARCH": expected_host,
            "GITHUB_ENV": str(output), "COMMAND_LOG": str(log), "ROSETTA_MISSING": str(missing),
        })
        return result, output.read_text(), log.read_text() if log.exists() else ""

    def test_native_hosts_do_not_enable_translation(self):
        for arch in ("arm64", "x86_64"):
            with self.subTest(arch=arch):
                result, environment, commands = self.prepare(arch, arch, arch)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(environment, "")
                self.assertEqual(commands, "")

    def test_arm_host_prepares_rosetta_only_for_intel_target(self):
        result, environment, commands = self.prepare("arm64", "x86_64", "arm64")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER=/usr/bin/arch -x86_64", environment)
        self.assertIn("SDKROOT=/Applications/Xcode.app/Contents/Developer/SDKs/MacOSX.sdk", environment)
        self.assertIn("arch -x86_64 /usr/bin/true", commands)
        self.assertNotIn("softwareupdate", commands)

    def test_missing_rosetta_is_installed_then_verified(self):
        result, environment, commands = self.prepare("arm64", "x86_64", "arm64", rosetta=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("softwareupdate --install-rosetta --agree-to-license", commands)
        self.assertEqual(commands.count("arch -x86_64 /usr/bin/true"), 2)
        self.assertIn("CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER=", environment)

    def test_unexpected_host_fails_instead_of_mislabeling_a_build(self):
        result, environment, _ = self.prepare("arm64", "x86_64", "x86_64")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(environment, "")

    def test_manual_experiment_changes_only_intel_host_and_keeps_native_default(self):
        ci = (ROOT / ".github/workflows/qt-ci.yml").read_text()
        build = (ROOT / ".github/workflows/qt-build.yml").read_text()
        self.assertIn("macos_intel_host:", ci)
        self.assertIn("macos_intel_host: ${{ inputs.macos_intel_host }}", ci)
        self.assertIn("matrix.label == 'macos-x64' && inputs.macos_intel_host == 'arm64'", build)
        self.assertIn("'blacksmith-6vcpu-macos-15' || matrix.os", build)
        self.assertIn("default: intel", build)
        self.assertIn('rust_test_target=()', build)
        self.assertIn('if [[ "$OPENNOW_MACOS_HOST_ARCH" != "$OPENNOW_MACOS_ARCH" ]]', build)
        self.assertIn('rust_test_target=(--target "${{ matrix.rust-target }}")', build)
        self.assertIn('cargo test --manifest-path native/opennow-core/Cargo.toml --locked "${rust_test_target[@]}"', build)
        self.assertIn('cargo test --manifest-path native/opennow-streamer/Cargo.toml --locked "${rust_test_target[@]}"', build)
        self.assertIn("bash .github/scripts/prepare-macos-target.sh", build)


if __name__ == "__main__":
    unittest.main()
