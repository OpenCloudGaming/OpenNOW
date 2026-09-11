from pathlib import Path
import re
import subprocess
import tempfile
import time
import unittest


CMAKE_FILE = Path(__file__).resolve().parents[1] / "CMakeLists.txt"


class CIBuildCacheTest(unittest.TestCase):
    def run_command(self, *command):
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_nightly_version_only_rebuilds_version_consumers(self):
        statements = [
            statement for statement in re.findall(
                r"set_source_files_properties\s*\([^)]*\)", CMAKE_FILE.read_text()
            )
            if "OPENNOW_VERSION" in statement
        ]
        self.assertEqual(len(statements), 1, "Expected one source-specific OPENNOW_VERSION definition")
        statement = statements[0]
        self.assertIn("src/app/ApplicationStartup.cpp", statement)
        self.assertIn("src/core/CoreClient.cpp", statement)
        self.assertIn("${OPENNOW_BUILD_VERSION}", statement)

        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            build = Path(directory) / "build"
            files = {
                "CMakeLists.txt": (
                    "cmake_minimum_required(VERSION 3.24)\n"
                    "project(BuildCacheContract LANGUAGES CXX)\n"
                    "add_executable(cache-app main.cpp unrelated.cpp "
                    "src/app/ApplicationStartup.cpp src/core/CoreClient.cpp)\n"
                    "add_executable(cache-core-test test_main.cpp unrelated.cpp src/core/CoreClient.cpp)\n"
                    + statement + "\n"
                    'file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/executables.txt" CONTENT '
                    '"$<TARGET_FILE:cache-app>\n$<TARGET_FILE:cache-core-test>\n")\n'
                ),
                "src/app/ApplicationStartup.cpp": (
                    "const char *app_version() { return OPENNOW_VERSION; }\n"
                ),
                "src/core/CoreClient.cpp": (
                    "const char *core_version() { return OPENNOW_VERSION; }\n"
                ),
                "unrelated.cpp": (
                    "#ifdef OPENNOW_VERSION\n"
                    '#error "Version definition leaked into an unrelated source"\n'
                    "#endif\n"
                    'const char *unrelated() { return "unrelated"; }\n'
                ),
                "main.cpp": (
                    "#include <iostream>\n"
                    "const char *app_version();\n"
                    "const char *core_version();\n"
                    "const char *unrelated();\n"
                    "int main() { std::cout << app_version() << '\\n' "
                    "<< core_version() << '\\n' << unrelated() << '\\n'; }\n"
                ),
                "test_main.cpp": (
                    "#include <iostream>\n"
                    "const char *core_version();\n"
                    "const char *unrelated();\n"
                    "int main() { std::cout << core_version() << '\\n' "
                    "<< unrelated() << '\\n'; }\n"
                ),
            }
            for name, content in files.items():
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)

            original_mtimes = None
            for version in ("1.0.0-nightly.123.1", "1.0.0-nightly.124.1"):
                with self.subTest(version=version):
                    self.run_command(
                        "cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
                        "-DCMAKE_BUILD_TYPE=Release", f"-DOPENNOW_BUILD_VERSION={version}",
                    )
                    self.run_command("cmake", "--build", str(build))
                    app, core_test = (build / "executables.txt").read_text().splitlines()
                    self.assertEqual(self.run_command(app).splitlines(), [version, version, "unrelated"])
                    self.assertEqual(self.run_command(core_test).splitlines(), [version, "unrelated"])
                    objects = sorted(
                        path for path in build.rglob("unrelated.cpp.*")
                        if path.suffix in {".o", ".obj"}
                    )
                    self.assertEqual(len(objects), 2, "Expected an unrelated object for each target")
                    mtimes = {path: path.stat().st_mtime_ns for path in objects}
                    if original_mtimes is None:
                        original_mtimes = mtimes
                        time.sleep(1.1)
                    else:
                        self.assertEqual(mtimes, original_mtimes, "Nightly version rebuilt unrelated sources")


if __name__ == "__main__":
    unittest.main()
