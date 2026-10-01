import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]


class LicenseBuildContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cargo = shutil.which("cargo")
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        cls.host = version.split("host: ", 1)[1].splitlines()[0]

    def build_notices(self, app_target="", env_target="", config_target="", fast=False):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            build = source / "build"
            fixture = source / "fixture"
            fixture.mkdir()
            bins = ("opennow-core", "opennow-acceptance-verify", "opennow-license-report")
            (fixture / "Cargo.toml").write_text(
                '[package]\nname = "license-build-fixture"\nversion = "0.0.0"\nedition = "2024"\n'
                + "".join(f'[[bin]]\nname = "{name}"\npath = "main.rs"\n' for name in bins)
            )
            (fixture / "main.rs").write_text(
                'fn main() {\n'
                '    let args: Vec<String> = std::env::args().collect();\n'
                '    if args.len() > 2 { std::fs::write(&args[2], "host notice fixture").unwrap(); }\n'
                '}\n'
            )
            if config_target:
                (fixture / ".cargo").mkdir()
                (fixture / ".cargo/config.toml").write_text(
                    f'[build]\ntarget = "{config_target}"\n'
                )
            commands = source / "cargo-commands.jsonl"
            cargo = source / "cargo-fixture"
            cargo.write_text(f'''#!{sys.executable}
import json, subprocess, sys
args = sys.argv[1:]
with open({str(commands)!r}, "a") as log:
    log.write(json.dumps(args) + "\\n")
args[args.index("--manifest-path") + 1] = {str(fixture / "Cargo.toml")!r}
sys.exit(subprocess.call([{self.cargo!r}, *args], cwd={str(fixture)!r}))
''')
            cargo.chmod(0o700)
            (source / "main.cpp").write_text("int main() { return 0; }\n")
            (source / "FindPkgConfig.cmake").write_text(
                "function(pkg_check_modules)\nendfunction()\n"
            )
            (source / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(LicenseBuildContract LANGUAGES CXX)
add_executable(opennow-qt main.cpp)
set(CMAKE_MODULE_PATH "{source.as_posix()}")
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR x86_64)
set(OPENNOW_RUST_TARGET "{app_target}")
set(CARGO_EXECUTABLE "{cargo.as_posix()}")
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
include("{QT_SOURCE.as_posix()}/cmake/NativeRuntime.cmake")
''')
            env = {key: value for key, value in os.environ.items()
                   if key not in ("CARGO_BUILD_TARGET", "CARGO_TARGET_DIR", "RUSTFLAGS",
                                  "CARGO_ENCODED_RUSTFLAGS")}
            env["CARGO_HOME"] = str(source / "cargo-home")
            if env_target:
                env["CARGO_BUILD_TARGET"] = env_target
            result = subprocess.run(
                ["cmake", "-S", str(source), "-B", str(build), "-G", "Unix Makefiles",
                 "-DCMAKE_BUILD_TYPE=Release"], env=env, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            rule = (build / "CMakeFiles/opennow-license-notices.dir/build.make").read_text()
            if app_target:
                for name in ("opennow-core", "opennow-update-helper-build",
                             "opennow-streamer-ffi-build", "opennow-streamer-bin-build"):
                    app_rule = (build / f"CMakeFiles/{name}.dir/build.make").read_text()
                    self.assertIn(f"--target {app_target}", app_rule)
            target = "opennow-license-notices/fast" if fast else "opennow-license-notices"
            result = subprocess.run(["cmake", "--build", str(build), "--target", target],
                                    env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((build / "THIRD_PARTY_NOTICES.generated").read_text(),
                             "host notice fixture")
            self.assertIn(f"--target {self.host}", rule)
            self.assertIn(f"rust-target/{self.host}/release/opennow-license-report", rule)
            invocations = [json.loads(line) for line in commands.read_text().splitlines()]
            license_args = next(args for args in invocations if "opennow-license-report" in args)
            self.assertEqual(license_args[license_args.index("--target") + 1], self.host)

    def test_native_build_generates_notices(self):
        self.build_notices()

    def test_explicit_app_target_generates_host_notices(self):
        self.build_notices(app_target=self.host)

    def test_cargo_build_target_does_not_select_license_target(self):
        self.build_notices(app_target=self.host, env_target=self.host)

    def test_cargo_config_target_does_not_select_license_target(self):
        self.build_notices(app_target=self.host, config_target=self.host)

    def test_foreign_app_and_environment_targets_keep_license_tool_on_host(self):
        foreign = "aarch64-unknown-linux-gnu" if not self.host.startswith("aarch64-") else "x86_64-unknown-linux-gnu"
        self.build_notices(app_target=foreign, env_target=foreign, config_target=foreign, fast=True)


if __name__ == "__main__":
    unittest.main()
