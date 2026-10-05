import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


QT_SOURCE = Path(__file__).resolve().parents[1]


@unittest.skipUnless(sys.platform.startswith("linux"), "ELF install contract")
class LinuxInstallLayoutTest(unittest.TestCase):
    def run_command(self, *command, **kwargs):
        result = subprocess.run(command, capture_output=True, text=True, **kwargs)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_staged_library_layout_and_relocated_loading(self):
        for prefix, bindir, libdir, install_prefix in (
            ("/usr", "bin", "lib", None),
            ("/usr", "bin", "lib64", None),
            ("/usr", "bin", "lib/x86_64-linux-gnu", None),
            ("/opt/opennow", "sbin", "lib/opennow", None),
            ("/usr", "bin", "/usr/lib64", None),
            ("/usr", "/usr/bin", "lib64", None),
            ("/usr", "bin", "/usr/lib64", "/usr"),
            ("/usr", "/usr/bin", "/usr/lib64", None),
            ("/usr", "bin", "lib64", "/opt/opennow"),
            ("/usr", "bin", "/usr/lib64", "/opt/opennow"),
            ("/usr", "/usr/bin", "lib64", "/opt/opennow"),
            ("/usr", "/usr/bin", "/usr/lib64", "/opt/opennow"),
        ):
            with self.subTest(prefix=prefix, bindir=bindir, libdir=libdir, install_prefix=install_prefix):
                with tempfile.TemporaryDirectory() as directory:
                    source = Path(directory)
                    build = source / "build"
                    stage = source / "stage"
                    (source / "packaging").symlink_to(QT_SOURCE / "packaging", target_is_directory=True)
                    (source / "ffi.rs").write_text(
                        '#[no_mangle] pub extern "C" fn install_contract_value() -> i32 { 42 }\n'
                    )
                    self.run_command(
                        "rustc", "--crate-type", "cdylib", str(source / "ffi.rs"),
                        "-C", "link-arg=-Wl,-soname,libopennow_streamer_ffi.so",
                        "-o", str(source / "libopennow_streamer_ffi.so"),
                    )
                    (source / "main.cpp").write_text(
                        'extern "C" int install_contract_value();\n'
                        'int main() { return install_contract_value() == 42 ? 0 : 1; }\n'
                    )
                    (source / "CMakeLists.txt").write_text(f'''cmake_minimum_required(VERSION 3.24)
project(LinuxInstallContract VERSION 1.0.0 LANGUAGES CXX)
include(GNUInstallDirs)
add_executable(opennow-qt main.cpp)
add_library(streamer SHARED IMPORTED)
set_target_properties(streamer PROPERTIES IMPORTED_LOCATION "${{CMAKE_CURRENT_SOURCE_DIR}}/libopennow_streamer_ffi.so")
target_link_libraries(opennow-qt PRIVATE streamer)
set_property(TARGET opennow-qt APPEND PROPERTY BUILD_RPATH "$ORIGIN")
set(CMAKE_CURRENT_SOURCE_DIR "{QT_SOURCE.as_posix()}")
include("{QT_SOURCE.as_posix()}/cmake/BuildMetadata.cmake")
set(OPENNOW_SDL3_RUNTIME_TARGET streamer)
set(OPENNOW_STREAMER_FFI_RUNTIME "${{CMAKE_SOURCE_DIR}}/libopennow_streamer_ffi.so")
set(OPENNOW_STREAMER_BIN_ARTIFACT "${{CMAKE_BINARY_DIR}}/opennow-streamer")
set(OPENNOW_GENERATED_NOTICES "${{CMAKE_BINARY_DIR}}/THIRD_PARTY_NOTICES")
include("{QT_SOURCE.as_posix()}/cmake/Packaging.cmake")
''')
                    self.run_command(
                        "cmake", "-S", str(source), "-B", str(build),
                        f"-DCMAKE_INSTALL_PREFIX={prefix}",
                        f"-DCMAKE_INSTALL_BINDIR={bindir}",
                        f"-DCMAKE_INSTALL_LIBDIR={libdir}",
                    )
                    self.run_command("cmake", "--build", str(build))
                    for name in ("opennow-core", "opennow-acceptance-verify",
                                 "opennow-update-helper", "opennow-streamer"):
                        shutil.copy2(build / "opennow-qt", build / name)
                    (build / "THIRD_PARTY_NOTICES").write_text("install contract\n")
                    command = ["cmake", "--install", str(build)]
                    if install_prefix:
                        command.extend(("--prefix", install_prefix))
                    if (install_prefix and install_prefix != prefix
                            and Path(bindir).is_absolute() != Path(libdir).is_absolute()):
                        result = subprocess.run(command, capture_output=True, text=True,
                                                env={**os.environ, "DESTDIR": str(stage)})
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("mixed absolute and relative", result.stderr)
                        self.assertFalse(stage.exists())
                        continue
                    self.run_command(*command, env={**os.environ, "DESTDIR": str(stage)})
                    prefix = install_prefix or prefix
                    installed_bin = stage / (bindir if bindir.startswith("/") else
                                             f"{prefix}/{bindir}").lstrip("/")
                    installed_lib = stage / (libdir if libdir.startswith("/") else
                                             f"{prefix}/{libdir}").lstrip("/")
                    ffi = installed_lib / "libopennow_streamer_ffi.so"
                    self.assertTrue(ffi.is_file(), f"missing library in {installed_lib}")
                    self.assertFalse((installed_bin / ffi.name).exists())
                    self.assertEqual(len(list(stage.rglob(ffi.name))), 1)
                    manifest = (build / "install_manifest.txt").read_text().splitlines()
                    self.assertIn("/" + str(ffi.relative_to(stage)), manifest)
                    for name in ("opennow-core", "opennow-acceptance-verify",
                                 "opennow-update-helper", "opennow-streamer"):
                        self.assertTrue(os.access(installed_bin / name, os.X_OK))
                    dynamic = self.run_command("readelf", "-d", str(installed_bin / "opennow-qt"))
                    self.assertIn("[libopennow_streamer_ffi.so]", dynamic)
                    self.assertIn("$ORIGIN/" + os.path.relpath(installed_lib, installed_bin), dynamic)
                    self.assertNotIn(str(source), dynamic)
                    relocated = source / "relocated"
                    stage.rename(relocated)
                    shutil.rmtree(build)
                    (source / "libopennow_streamer_ffi.so").unlink()
                    executable = relocated / installed_bin.relative_to(stage) / "opennow-qt"
                    environment = {key: value for key, value in os.environ.items()
                                   if key not in ("LD_LIBRARY_PATH", "LD_PRELOAD")}
                    resolved = self.run_command("ldd", str(executable), env=environment)
                    resolved_ffi = next(line.split("=>", 1)[1].split(" (", 1)[0].strip()
                                        for line in resolved.splitlines()
                                        if line.strip().startswith("libopennow_streamer_ffi.so =>"))
                    self.assertEqual(Path(resolved_ffi).resolve(), relocated / ffi.relative_to(stage))
                    self.run_command(str(executable), env=environment)

    def test_verifier_rejects_ambiguous_absolute_libdir(self):
        result = subprocess.run(
            [sys.executable, str(QT_SOURCE / "packaging/verify_linux_package.py"),
             "/unused/usr/bin", "--libdir", "/usr/lib64"],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("--libdir must be relative", result.stderr)


if __name__ == "__main__":
    unittest.main()
