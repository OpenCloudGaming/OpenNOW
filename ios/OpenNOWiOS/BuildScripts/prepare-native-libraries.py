#!/usr/bin/env python3
"""Install the checksum-pinned native protocol libraries for iOS build 128."""
import hashlib
from pathlib import Path
import shutil
import tempfile
import urllib.error
import urllib.request
import zipfile

ARCHIVE = "OpenNOW-iOS-128-native-libraries.zip"
URL = "https://github.com/joemossjr16/ios-apps/releases/download/opennow-128/" + ARCHIVE
# The public asset API is an alternate download route when a CI proxy returns
# an HTML/error response from the release page. Both routes require the same hash.
ASSET_URL = "https://api.github.com/repos/joemossjr16/ios-apps/releases/assets/602261069"
SHA256 = "700e218ace883213de500707095ff322d8ec179235b231ceb61d0007a353ea56"
PREFIXES = (
    "ios/OpenNOWiOS/Frameworks/OpenSSL.xcframework/",
    "ios/OpenNOWiOS/Frameworks/usrsctp.xcframework/",
)


def download_archive(archive):
    errors = []
    for url in (URL, ASSET_URL):
        with tempfile.NamedTemporaryFile(dir=archive.parent, suffix=".download", delete=False) as temporary:
            download = Path(temporary.name)
        try:
            request = urllib.request.Request(url, headers={
                "Accept": "application/octet-stream",
                "User-Agent": "OpenNOW-iOS-native-libraries",
            })
            with urllib.request.urlopen(request, timeout=60) as source, download.open("wb") as out:
                content_type = source.headers.get("Content-Type", "unknown")
                shutil.copyfileobj(source, out)
            digest = hashlib.sha256(download.read_bytes()).hexdigest()
            if digest != SHA256:
                raise RuntimeError(f"Native library archive checksum mismatch: got {digest}, "
                                   f"{download.stat().st_size} bytes, type {content_type}; expected {SHA256}")
            download.replace(archive)
            return
        except (urllib.error.URLError, TimeoutError, RuntimeError) as error:
            message = f"{url}: {error}"
            print(message, flush=True)
            errors.append(message)
        finally:
            download.unlink(missing_ok=True)
    raise RuntimeError("Unable to download checksum-pinned native libraries:\n" + "\n".join(errors))


def prepare():
    root = Path(__file__).resolve().parents[3]
    cache = root / "Build/native-library-cache"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / ARCHIVE
    if not archive.exists():
        download_archive(archive)
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
