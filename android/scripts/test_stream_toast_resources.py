#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description="Run extracted toast-resource expressions with real Compose resource APIs")
parser.add_argument("--source", type=Path, default=root / "app/src/main/java/com/opencloudgaming/opennow/OpenNowStreamSurface.kt")
parser.add_argument("--android-home", type=Path, default=os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT")))
parser.add_argument("--kotlinc", default=os.environ.get("KOTLINC", shutil.which("kotlinc")))
args = parser.parse_args()
if not args.kotlinc or not args.android_home:
    parser.error("Provide KOTLINC/ANDROID_HOME or their command-line options")
model = root / "app/build/intermediates/lint_model/debug/generateDebugLintModel/debug-artifact-libraries.xml"
if not model.exists():
    parser.error("Run :app:compileDebugKotlin :app:generateDebugLintModel first; no lint analysis is required")
source = args.source.read_text()
fixture = root / "app/src/test/fixtures/stream-toast-resources"
template = (fixture / "StringProbe.kt.in").read_text()
declarations = []
for name in ("microphonePermissionDeniedMessage", "recordingFailedMessage", "recordingUnavailableMessage"):
    marker = "    val " + name + " by rememberUpdatedState("
    if marker not in source:
        continue
    start = source.index(marker)
    end = source.index("\n", start)
    if source[start:end].rstrip().endswith("("):
        end = source.index("\n    )", start) + len("\n    )")
    declarations.append(source[start:end])
template = template.replace("__DECLARATIONS__", "\n".join(declarations))
regions = (
    ("__PERMISSION__", "val microphonePermissionLauncher =", "val streamSettings =", 0),
    ("__FAILED__", "client.recordingError.collect", "val recordingFolderLauncher =", 0),
    ("__UNAVAILABLE__", "else if (client.canStartStreamRecording())", "onEsc =", -1),
)
for marker, start, end, index in regions:
    section = source[source.index(start):source.index(end, source.index(start))]
    expressions = re.findall(r"Toast\.makeText\(\s*context,\s*(.*?),\s*Toast\.LENGTH_LONG", section, re.S)
    template = template.replace(marker, expressions[index].strip())
libraries = []
for library in ET.parse(model).getroot():
    libraries.extend(path for path in library.get("jars", "").split(os.pathsep) if path and Path(path).is_file())
platforms = list((args.android_home / "platforms").glob("*/android.jar"))
if not platforms:
    parser.error("No installed Android platform")
platform = max(platforms, key=lambda path: tuple(map(int, re.findall(r"\d+", path.parent.name))))
work = Path.home() / ".capy/work"
work.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="stream-toast-resources-", dir=work) as temporary:
    out = Path(temporary)
    stubs = out / "stubs"
    stubs.mkdir()
    subprocess.run(["javac", "-d", str(stubs), *map(str, fixture.glob("*.java"))], check=True)
    classpath = os.pathsep.join((str(stubs), *libraries, str(platform)))
    kotlin = out / "StringProbe.kt"
    kotlin.write_text(template)
    compiler = Path(args.kotlinc).resolve()
    plugin = compiler.parent.parent / "lib/compose-compiler-plugin.jar"
    jar = out / "probe.jar"
    subprocess.run([str(compiler), str(kotlin), "-language-version", "1.9", "-jvm-target", "17", "-cp", classpath, f"-Xplugin={plugin}", "-include-runtime", "-d", str(jar)], check=True)
    subprocess.run(["java", "-cp", str(jar) + os.pathsep + classpath, "StringProbeKt"], check=True)
