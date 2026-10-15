#!/usr/bin/env python3
import argparse
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description="Compile production JNI guard bodies with instrumented host-side boundary shims")
parser.add_argument("--source", type=Path, default=root / "nvst/src/lib.rs")
parser.add_argument("--rustc", default="rustc")
args = parser.parse_args()
source = args.source.read_text()
functions = []
for name in ("keyframe", "run"):
    declaration = f'pub extern "system" fn Java_com_opencloudgaming_opennow_NvstBridge_{name}('
    start = source.rfind('#[unsafe(no_mangle)]', 0, source.index(declaration))
    body = source.index("{", source.index(declaration))
    depth = 0
    for end in range(body, len(source)):
        depth += (source[end] == "{") - (source[end] == "}")
        if depth == 0:
            functions.append(source[start:end + 1])
            break
fixture = (root / "nvst/tests/control_bridge_fixture.rs.in").read_text().replace("__PRODUCTION_FUNCTIONS__", "\n".join(functions))
work = Path.home() / ".capy/work"
work.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="jni-control-guards-", dir=work) as temporary:
    out = Path(temporary)
    path = out / "guards.rs"
    path.write_text(fixture)
    executable = out / "guards"
    subprocess.run([args.rustc, "--edition=2024", str(path), "-o", str(executable)], check=True)
    subprocess.run([str(executable)], check=True)
