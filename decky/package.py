import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
NAME = "OpenNOW"
PUBLIC_KEY = ROOT.parent / "opennow-qt/packaging/update-public-key.base64"
FILES = ["plugin.json", "package.json", "main.py", "LICENSE", "README.md"]


def package() -> Path:
    if not (ROOT / "dist/index.js").exists():
        raise SystemExit("Run the build first")
    output = ROOT / "out" / f"{NAME}.zip"
    output.parent.mkdir(exist_ok=True)
    modules = list((ROOT / "py_modules").rglob("*.py"))
    built = [path for path in (ROOT / "dist").rglob("*") if path.is_file() and path.suffix != ".map"]
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.write(PUBLIC_KEY, f"{NAME}/py_modules/opennow_decky/update-public-key.base64")
        for path in [ROOT / name for name in FILES] + modules + built:
            archive.write(path, f"{NAME}/{path.relative_to(ROOT)}")
    return output


if __name__ == "__main__":
    print(package())
