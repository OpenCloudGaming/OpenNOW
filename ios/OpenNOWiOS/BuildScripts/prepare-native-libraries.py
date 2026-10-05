#!/usr/bin/env python3
"""Install the checksum-pinned native protocol libraries for iOS build 128."""
import hashlib
from pathlib import Path
import shutil
import tempfile
import urllib.request
import zipfile

ARCHIVE = "OpenNOW-iOS-128-native-libraries.zip"
URL = "https://github.com/joemossjr16/ios-apps/releases/download/opennow-128/" + ARCHIVE
SHA256 = "700e218ace883213de500707095ff322d8ec179235b231ceb61d0007a353ea56"
PREFIXES = (
    "ios/OpenNOWiOS/Frameworks/OpenSSL.xcframework/",
    "ios/OpenNOWiOS/Frameworks/usrsctp.xcframework/",
)


def prepare():
    root = Path(__file__).resolve().parents[3]
    cache = root / "Build/native-library-cache"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / ARCHIVE
    if not archive.exists():
        with tempfile.NamedTemporaryFile(dir=cache, suffix=".download", delete=False) as temporary:
            download = Path(temporary.name)
        try:
            with urllib.request.urlopen(URL, timeout=60) as source, download.open("wb") as out:
                shutil.copyfileobj(source, out)
            if hashlib.sha256(download.read_bytes()).hexdigest() != SHA256:
                raise RuntimeError("Native library archive checksum mismatch")
            download.replace(archive)
        finally:
            download.unlink(missing_ok=True)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError("Cached native library archive checksum mismatch; remove it and retry")
    with tempfile.TemporaryDirectory(dir=cache, prefix="extract-") as temporary:
        stage = Path(temporary)
        with zipfile.ZipFile(archive) as contents:
            for item in contents.infolist():
                if not item.filename.startswith(PREFIXES):
                    continue
                destination = (stage / item.filename).resolve()
                if stage.resolve() not in destination.parents:
                    raise RuntimeError("Unsafe archive member")
                if item.is_dir():
                    destination.mkdir(parents=True, exist_ok=True)
                else:
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with contents.open(item) as source, destination.open("wb") as out:
                        shutil.copyfileobj(source, out)
        for prefix in PREFIXES:
            source = stage / prefix
            if not (source / "Info.plist").is_file():
                raise RuntimeError("Missing XCFramework manifest")
        for prefix in PREFIXES:
            destination = root / prefix
            if destination.exists():
                shutil.rmtree(destination)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(stage / prefix, destination)
            print("Installed", destination.relative_to(root))


if __name__ == "__main__":
    prepare()
