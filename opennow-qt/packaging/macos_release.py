import argparse
import base64
import json
import os
from pathlib import Path
import plistlib
import re
import secrets
import shlex
import signal
import subprocess
import tempfile


BUNDLE_ID = "io.github.opencloudgaming.OpenNOW"
ENTITLEMENTS = Path(__file__).with_name("macos-release-entitlements.plist")
SECRET_NAMES = (
    "OPENNOW_MACOS_DEVELOPER_ID_P12_BASE64",
    "OPENNOW_MACOS_DEVELOPER_ID_P12_PASSWORD",
    "OPENNOW_MACOS_SIGN_IDENTITY",
    "OPENNOW_APPLE_API_KEY_BASE64",
    "OPENNOW_APPLE_API_KEY_ID",
    "OPENNOW_APPLE_API_ISSUER_ID",
)
MACHO_MAGIC = {bytes.fromhex(value) for value in (
    "feedface", "cefaedfe", "feedfacf", "cffaedfe", "cafebabe", "bebafeca",
    "cafebabf", "bfbafeca",
)}


def run(*args):
    try:
        result = subprocess.run(
            [str(arg) for arg in args], check=False, text=True, capture_output=True,
            env={key: value for key, value in os.environ.items() if key not in SECRET_NAMES},
            timeout=2400,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"{args[0]} {args[1]} timed out") from None
    if result.returncode:
        raise RuntimeError(f"{args[0]} {args[1]} failed (exit {result.returncode})")
    return result.stdout


def signing_targets(app):
    targets = []
    for path in app.rglob("*"):
        if path.is_symlink():
            continue
        if path.is_dir() and path.suffix in {".app", ".framework", ".xpc", ".bundle", ".plugin"}:
            targets.append(path)
        elif path.is_file():
            with path.open("rb") as source:
                if source.read(4) in MACHO_MAGIC:
                    run("lipo", path, "-verify_arch", "arm64")
                    targets.append(path)
    return sorted(targets, key=lambda path: (-len(path.parts), str(path))) + [app]


def verify_app(app, requirement):
    with (app / "Contents/Info.plist").open("rb") as source:
        if plistlib.load(source).get("CFBundleIdentifier") != BUNDLE_ID:
            raise ValueError("Unexpected application bundle identifier")
    run("codesign", "--verify", "--deep", "--strict", "-R", requirement, app)
    actual_entitlements = plistlib.loads(run(
        "codesign", "--display", "--entitlements", "-", "--xml",
        app / "Contents/MacOS/OpenNOW",
    ).encode())
    with ENTITLEMENTS.open("rb") as source:
        if actual_entitlements != plistlib.load(source):
            raise ValueError("Signed application entitlements do not match the release policy")
    run("xcrun", "stapler", "validate", app)
    run("spctl", "--assess", "--type", "execute", "--verbose=4", app)


def notarize(path, credentials):
    result = json.loads(run(
        "xcrun", "notarytool", "submit", path, "--key", credentials[0],
        "--key-id", credentials[1], "--issuer", credentials[2],
        "--wait", "--timeout", "30m", "--output-format", "json",
    ))
    submission_id = result.get("id", "")
    if isinstance(submission_id, str) and re.fullmatch(r"[0-9a-fA-F-]{36}", submission_id):
        print(f"Notarization submission: {submission_id}", flush=True)
    if result.get("status") != "Accepted":
        raise RuntimeError("Apple notarization did not return Accepted")


def package(build, output, version):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("A numeric release version is required")
    values = {name: os.environ[name] for name in SECRET_NAMES}
    if not all(values.values()):
        raise ValueError("All macOS signing secrets are required")
    identity = values["OPENNOW_MACOS_SIGN_IDENTITY"]
    match = re.fullmatch(r"Developer ID Application: .+ \(([A-Z0-9]{10})\)", identity)
    if not match:
        raise ValueError("An exact Developer ID Application identity is required")
    requirement = (f'anchor apple generic and certificate leaf[subject.OU] = "{match[1]}" '
                   'and certificate leaf[field.1.2.840.113635.100.6.1.13] exists')
    output.mkdir(parents=True, exist_ok=True)
    name = f"OpenNOW-Qt-{version}-Darwin-arm64"
    if any((output / f"{name}.{suffix}").exists() for suffix in ("zip", "dmg")):
        raise ValueError("Candidate outputs already exist")
    with tempfile.TemporaryDirectory(prefix="opennow-macos-signing-", dir=os.environ.get("RUNNER_TEMP")) as directory:
        temporary = Path(directory)
        stage = temporary / "stage"
        app = stage / "OpenNOW.app"
        keychain = temporary / "signing.keychain-db"
        key = temporary / "AuthKey.p8"
        certificate = temporary / "certificate.p12"
        key.write_bytes(base64.b64decode(values["OPENNOW_APPLE_API_KEY_BASE64"], validate=True))
        certificate.write_bytes(base64.b64decode(values["OPENNOW_MACOS_DEVELOPER_ID_P12_BASE64"], validate=True))
        key.chmod(0o600)
        certificate.chmod(0o600)
        password = secrets.token_urlsafe(32)
        original_keychains = shlex.split(run("security", "list-keychains", "-d", "user"))
        try:
            run("security", "create-keychain", "-p", password, keychain)
            run("security", "set-keychain-settings", "-lut", "21600", keychain)
            run("security", "unlock-keychain", "-p", password, keychain)
            run("security", "import", certificate, "-k", keychain, "-P",
                values["OPENNOW_MACOS_DEVELOPER_ID_P12_PASSWORD"], "-T", "/usr/bin/codesign")
            run("security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
                "-s", "-k", password, keychain)
            print("Deploying and stripping the application before signing", flush=True)
            run("cmake", "--install", build, "--config", "Release", "--strip", "--prefix", stage)
            with (app / "Contents/Info.plist").open("rb") as source:
                if plistlib.load(source).get("CFBundleIdentifier") != BUNDLE_ID:
                    raise ValueError("Unexpected application bundle identifier")
            print("Signing and verifying nested code inside-out", flush=True)
            for target in signing_targets(app):
                options = ["--identifier", BUNDLE_ID, "--entitlements", ENTITLEMENTS] if target == app else []
                run("codesign", "--force", "--sign", identity, "--keychain", keychain,
                    "--options", "runtime", "--timestamp", *options, target)
                run("codesign", "--verify", "--strict", "-R", requirement, target)
            run("codesign", "--verify", "--deep", "--strict", "-R", requirement, app)
            credentials = (key, values["OPENNOW_APPLE_API_KEY_ID"], values["OPENNOW_APPLE_API_ISSUER_ID"])
            submission = temporary / "notarization.zip"
            run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, submission)
            print("Notarizing and stapling the application", flush=True)
            notarize(submission, credentials)
            run("xcrun", "stapler", "staple", app)
            verify_app(app, requirement)
            archive = temporary / f"{name}.zip"
            dmg = temporary / f"{name}.dmg"
            run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, archive)
            (stage / "Applications").symlink_to("/Applications")
            run("hdiutil", "create", "-volname", "OpenNOW", "-srcfolder", stage,
                "-format", "UDZO", dmg)
            run("codesign", "--force", "--sign", identity, "--keychain", keychain, "--timestamp", dmg)
            print("Notarizing and stapling the final disk image", flush=True)
            notarize(dmg, credentials)
            run("xcrun", "stapler", "staple", dmg)
            run("codesign", "--verify", "--strict", "-R", requirement, dmg)
            run("xcrun", "stapler", "validate", dmg)
            run("spctl", "--assess", "--type", "open", "--context", "context:primary-signature", "--verbose=4", dmg)
            run("hdiutil", "verify", dmg)
            print("Verifying Gatekeeper signatures and tickets from final packages", flush=True)
            extracted = temporary / "zip-extracted"
            run("ditto", "-x", "-k", archive, extracted)
            verify_app(extracted / "OpenNOW.app", requirement)
            mount = temporary / "mounted"
            mount.mkdir()
            try:
                run("hdiutil", "attach", dmg, "-readonly", "-nobrowse", "-mountpoint", mount)
                relocated = temporary / "dmg-extracted/OpenNOW.app"
                run("ditto", mount / "OpenNOW.app", relocated)
            finally:
                run("hdiutil", "detach", mount)
            verify_app(relocated, requirement)
        finally:
            try:
                run("security", "delete-keychain", keychain)
            finally:
                run("security", "list-keychains", "-d", "user", "-s", *original_keychains)
        archive.replace(output / archive.name)
        dmg.replace(output / dmg.name)


def interrupted(signum, frame):
    raise SystemExit(128 + signum)


def main():
    parser = argparse.ArgumentParser(description="Sign, notarize, and verify macOS ARM64 candidates")
    parser.add_argument("--build", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    os.umask(0o077)
    package(args.build.resolve(), args.output.resolve(), args.version)


if __name__ == "__main__":
    main()
