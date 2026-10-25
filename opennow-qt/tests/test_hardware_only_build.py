import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]


class HardwareOnlyBuildContractTest(unittest.TestCase):
    def check_build(self, target):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            build = source / "build"
            commands = source / "cargo-commands.jsonl"
            cargo = source / "cargo-fixture"
            cargo.write_text(f'''#!{sys.executable}
import json, pathlib, sys
args = sys.argv[1:]
with open({str(commands)!r}, "a") as log:
    log.write(json.dumps(args) + "\\n")
root = pathlib.Path(args[args.index("--target-dir") + 1])
if "--target" in args:
    root /= args[args.index("--target") + 1]
root /= "release"
root.mkdir(parents=True, exist_ok=True)
name = "libopennow_streamer_ffi.so" if "--lib" in args else "opennow-streamer"
(root / name).touch()
''')
            cargo.chmod(0o700)
            (source / "main.cpp").write_text("int main() { return 0; }\n")
            (source / "FindPkgConfig.cmake").write_text(
                "function(pkg_check_modules)\nendfunction()\n"
            )
            (source / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(HardwareOnlyBuildContract LANGUAGES CXX)
add_executable(opennow-qt main.cpp)
set(CMAKE_MODULE_PATH "{source.as_posix()}")
set(CMAKE_SYSTEM_NAME Linux)
set(OPENNOW_RUST_TARGET "{target}")
set(CARGO_EXECUTABLE "{cargo.as_posix()}")
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
include("{QT_SOURCE.as_posix()}/cmake/NativeRuntime.cmake")
''')
            for policy in (None, True, False):
                configure = ["cmake", "-S", str(source), "-B", str(build),
                             "-G", "Unix Makefiles", "-DCMAKE_BUILD_TYPE=Release",
                             "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON"]
                if policy is not None:
                    configure.append(f'-DNO_SOFT_RENDERING={"ON" if policy else "OFF"}')
                result = subprocess.run(configure, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                compile_commands = json.loads((build / "compile_commands.json").read_text())
                self.assertEqual("-DNO_SOFT_RENDERING" in compile_commands[0]["command"],
                                 bool(policy))
                result = subprocess.run(
                    ["cmake", "--build", str(build), "--target",
                     "opennow-streamer-ffi-build", "opennow-streamer-bin-build"],
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                invocations = [json.loads(line) for line in commands.read_text().splitlines()]
                self.assertEqual(len(invocations), 2)
                self.assertEqual({args[args.index("--package") + 1] for args in invocations},
                                 {"opennow-streamer-ffi", "opennow-streamer"})
                for args in invocations:
                    expected_directory = build / "streamer-rust-target"
                    if policy:
                        expected_directory /= "hardware-only"
                    self.assertEqual(Path(args[args.index("--target-dir") + 1]),
                                     expected_directory)
                    features = [args[index + 1] for index, arg in enumerate(args)
                                if arg == "--features"]
                    self.assertIn("linux-ffmpeg-bundled,linux-vaapi", features)
                    self.assertEqual("hardware-only" in features, bool(policy))
                    self.assertEqual(args[args.index("--target") + 1], target)
                commands.unlink()
                result = subprocess.run(
                    ["cmake", "--build", str(build), "--target",
                     "opennow-streamer-ffi-build", "opennow-streamer-bin-build"],
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertFalse(commands.exists(), "Unchanged policy must reuse streamer artifacts")

    def test_x86_64_default_and_policy_toggle(self):
        self.check_build("x86_64-unknown-linux-gnu")

    def test_aarch64_default_and_policy_toggle(self):
        self.check_build("aarch64-unknown-linux-gnu")

    def test_rejects_unsupported_platform(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            (source / "main.cpp").write_text("int main() { return 0; }\n")
            (source / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(HardwareOnlyUnsupported LANGUAGES CXX)
add_executable(opennow-qt main.cpp)
set(CMAKE_SYSTEM_NAME Darwin)
set(NO_SOFT_RENDERING ON)
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
include("{QT_SOURCE.as_posix()}/cmake/NativeRuntime.cmake")
''')
            result = subprocess.run(["cmake", "-S", str(source), "-B", str(source / "build")],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("NO_SOFT_RENDERING is supported only on Linux", result.stderr)


if __name__ == "__main__":
    unittest.main()
