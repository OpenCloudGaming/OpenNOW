#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zipfile


parser = argparse.ArgumentParser(description="Exercise production controller callbacks with the resolved Compose runtime")
parser.add_argument("--android-root", type=Path, default=Path(__file__).resolve().parents[1])
parser.add_argument("--gradle-home", type=Path, default=Path(os.environ.get("GRADLE_USER_HOME", Path.home() / ".gradle")))
parser.add_argument("--android-home", type=Path, default=os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT")))
parser.add_argument("--kotlinc", default=os.environ.get("KOTLINC", shutil.which("kotlinc")))
args = parser.parse_args()
if not args.kotlinc or not args.android_home:
    parser.error("Provide --kotlinc and --android-home, or set KOTLINC and ANDROID_HOME")

fixtures = Path(__file__).resolve().parents[1] / "app/src/test/fixtures/controller-callback"
sources = args.android_root / "app/src/main/java/com/opencloudgaming/opennow"
surface = (sources / "OpenNowStreamSurface.kt").read_text()
runtime = (sources / "Streaming.kt").read_text()
encoder = (sources / "StreamInputEncoder.kt").read_text()
template = (fixtures / "CallbackProbe.kt.in").read_text()


def callback(name, indentation):
    start = surface.index(indentation + name + " = ")
    end = surface.index("\n" + indentation + "},", start) + len("\n" + indentation + "}")
    return surface[start:end].split(" = ", 1)[1]


state = next(line.strip() for line in surface.splitlines() if "var controllerMouseAssistEnabled by remember(" in line)
updated = ""
marker = "    val updateControllerMouseAssistEnabled by rememberUpdatedState"
if marker in surface:
    start = surface.index(marker)
    updated = surface[start:surface.index("\n    }", start) + len("\n    }")].strip()
for marker, value in (
    ("__STATE__", state),
    ("__UPDATED__", updated),
    ("__CALLBACK__", callback("onControllerMouseAssistChanged", "            ")),
    ("__TOGGLE__", callback("onControllerMouseAssistToggle", "                    ")),
):
    template = template.replace(marker, value)

methods = []
for name in ("updateControllerMouseAssistAutoArm", "setControllerMouseAssistEnabled", "startControllerMouseLoop", "updateControllerMouseLoop", "stopControllerMouseLoop", "armControllerMouseAssistForSession", "setControllerMouseAssistActive", "emitControllerMouseAssistChanged", "releaseControllerMouseButtons", "setControllerMouseButton"):
    declaration = next(line for line in runtime.splitlines() if f"fun {name}(" in line)
    start = runtime.index(declaration)
    body = runtime.index("{", start)
    depth = 0
    for end in range(body, len(runtime)):
        depth += (runtime[end] == "{") - (runtime[end] == "}")
        if depth == 0:
            methods.append(runtime[start:end + 1])
            break
template = template.replace("__RUNTIME_METHODS__", "\n".join(methods))
start = encoder.index("internal fun shouldRunControllerMouseLoop(")
template = template.replace("__LOOP_RULE__", encoder[start:encoder.index("\n\n", start)])


def version(path):
    return tuple(map(int, re.findall(r"\d+", path.parts[-3])))


def artifact(group, module, extension):
    candidates = list((args.gradle_home / "caches/modules-2/files-2.1" / group / module).glob(f"*/*/*.{extension}"))
    if not candidates:
        parser.error(f"Missing cached {module}; run the app's compileDebugKotlin task first")
    return max(candidates, key=version)


platforms = list((args.android_home / "platforms").glob("*/android.jar"))
if not platforms:
    parser.error("No installed Android platform jar")
platform = max(platforms, key=lambda path: tuple(map(int, re.findall(r"\d+", path.parent.name))))
work = Path.home() / ".capy/work"
work.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="controller-callback-", dir=work) as temporary:
    out = Path(temporary)
    classpath = [artifact("androidx.collection", "collection-jvm", "jar"), artifact("org.jetbrains.kotlinx", "kotlinx-coroutines-core-jvm", "jar"), artifact("androidx.annotation", "annotation-jvm", "jar")]
    for module in ("runtime-android", "runtime-annotation-android"):
        archive = artifact("androidx.compose.runtime", module, "aar")
        classes = out / f"{module}.jar"
        with zipfile.ZipFile(archive) as aar:
            classes.write_bytes(aar.read("classes.jar"))
        classpath.append(classes)
        print(f"Using {module} {archive.parts[-3]}", flush=True)
    source = out / "CallbackProbe.kt"
    source.write_text(template)
    stubs = out / "platform-stubs"
    stubs.mkdir()
    subprocess.run(["javac", "-d", str(stubs), str(fixtures / "Trace.java"), str(fixtures / "Looper.java")], check=True)
    cp = os.pathsep.join(map(str, classpath))
    compiler = Path(args.kotlinc).resolve()
    plugin = compiler.parent.parent / "lib/compose-compiler-plugin.jar"
    jar = out / "probe.jar"
    subprocess.run([str(compiler), str(source), "-language-version", "1.9", "-jvm-target", "17", "-cp", cp, f"-Xplugin={plugin}", "-include-runtime", "-d", str(jar)], check=True)
    subprocess.run(["java", "-cp", os.pathsep.join((str(stubs), str(jar), cp, str(platform))), "CallbackProbeKt"], check=True)
