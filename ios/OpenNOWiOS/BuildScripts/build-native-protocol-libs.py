#!/usr/bin/env python3
"""Build the pinned NVST protocol libraries for iOS and the arm64 simulator."""
import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import urllib.request

OPENSSL_VERSION = "3.5.8"
OPENSSL_SHA256 = "a8f84a39918ec6415ce765d9b429d313ba97b8143169c172e734b9514464f5b2"
SCTP_REVISION = "07f871bda23943c43c9e74cc54f25130459de830"


def run(args, cwd=None):
    subprocess.run([str(a) for a in args], cwd=cwd, check=True)


def framework(name, binary, headers, target, platform):
    bundle = target / (name + ".framework")
    if bundle.exists():
        shutil.rmtree(bundle)
    (bundle / "Modules").mkdir(parents=True)
    shutil.copytree(headers, bundle / "Headers")
    shutil.copy2(binary, bundle / name)
    if name == "OpenSSL":
        (bundle / "Headers/OpenSSL.h").write_text(
            "#include <openssl/ssl.h>\n#include <openssl/err.h>\n"
            "#include <openssl/x509.h>\n#include <openssl/rand.h>\n")
    header = "OpenSSL.h" if name == "OpenSSL" else "usrsctp.h"
    (bundle / "Modules/module.modulemap").write_text(
        f'framework module {name} {{\n umbrella header "{header}"\n export *\n module * {{ export * }}\n}}\n')
    with (bundle / "Info.plist").open("wb") as out:
        plistlib.dump({
            "CFBundleExecutable": name,
            "CFBundleIdentifier": "com.opencloudgaming.nvst." + name.lower(),
            "CFBundleName": name,
            "CFBundlePackageType": "FMWK",
            "CFBundleShortVersionString": OPENSSL_VERSION if name == "OpenSSL" else "0.9.5",
            "CFBundleVersion": "1",
            "CFBundleSupportedPlatforms": [platform],
            "MinimumOSVersion": "16.0",
        }, out)
    return bundle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cmake", default="cmake")
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    work = root / "Build/native-dependencies"
    output = root / "ios/OpenNOWiOS/Frameworks"
    work.mkdir(parents=True, exist_ok=True)
    archive = work / f"openssl-{OPENSSL_VERSION}.tar.gz"
    if not archive.exists():
        urllib.request.urlretrieve(
            f"https://github.com/openssl/openssl/releases/download/openssl-{OPENSSL_VERSION}/{archive.name}", archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != OPENSSL_SHA256:
        raise RuntimeError("OpenSSL source archive checksum mismatch")
    source = work / f"openssl-{OPENSSL_VERSION}"
    if not source.exists():
        with tarfile.open(archive) as contents:
            for member in contents.getmembers():
                path = (work / member.name).resolve()
                if work.resolve() not in path.parents or member.issym() or member.islnk():
                    raise RuntimeError("Unsafe source archive member")
            contents.extractall(work)
    sctp = work / "usrsctp"
    if not sctp.exists():
        run(["git", "clone", "--depth", "1", "--branch", "0.9.5.0",
             "https://github.com/sctplab/usrsctp.git", sctp])
    revision = subprocess.check_output(["git", "-C", str(sctp), "rev-parse", "HEAD"], text=True).strip()
    if revision != SCTP_REVISION:
        raise RuntimeError("usrsctp source revision mismatch")
    run(["git", "-C", sctp, "diff", "--exit-code", "HEAD", "--", "usrsctplib", "CMakeLists.txt"])
    bundles = {"OpenSSL": [], "usrsctp": []}
    for sdk, configure, minimum, platform in [
        ("iphoneos", "ios64-xcrun", "-miphoneos-version-min=16.0", "iPhoneOS"),
        ("iphonesimulator", "iossimulator-arm64-xcrun", "-mios-simulator-version-min=16.0", "iPhoneSimulator"),
    ]:
        sdk_path = subprocess.check_output(["xcrun", "--sdk", sdk, "--show-sdk-path"], text=True).strip()
        build = work / ("openssl-" + sdk)
        build.mkdir(exist_ok=True)
        stage = work / ("stage-" + sdk)
        run(["perl", source / "Configure", configure, "no-shared", "no-tests", "no-apps",
             "no-module", "no-engine", "--prefix=" + str(stage), minimum,
             "-isysroot", sdk_path], cwd=build)
        run(["make", "-j" + str(max(1, args.jobs))], cwd=build)
        run(["make", "install_sw"], cwd=build)
        combined = stage / "libOpenSSL.a"
        run(["xcrun", "libtool", "-static", "-o", combined,
             stage / "lib/libssl.a", stage / "lib/libcrypto.a"])
        bundles["OpenSSL"].append(framework("OpenSSL", combined, stage / "include", stage, platform))
        sctp_build = work / ("sctp-" + sdk)
        run([args.cmake, "-S", sctp, "-B", sctp_build, "-G", "Unix Makefiles",
             "-DCMAKE_SYSTEM_NAME=iOS", "-DCMAKE_OSX_SYSROOT=" + sdk_path,
             "-DCMAKE_OSX_ARCHITECTURES=arm64", "-DCMAKE_OSX_DEPLOYMENT_TARGET=16.0",
             "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
             "-DCMAKE_C_FLAGS_RELEASE=-O3 -DNDEBUG -Wno-unused-but-set-variable -Wno-strict-prototypes",
             "-Dsctp_build_programs=OFF", "-Dsctp_build_shared_lib=OFF", "-Dsctp_debug=OFF",
             "-Dsctp_inet=OFF", "-Dsctp_inet6=OFF",
             "-Dsctp_werror=ON"])
        run([args.cmake, "--build", sctp_build, "--parallel", str(max(1, args.jobs))])
        sctp_headers = stage / "sctp-headers"
        sctp_headers.mkdir(exist_ok=True)
        shutil.copy2(sctp / "usrsctplib/usrsctp.h", sctp_headers / "usrsctp.h")
        binaries = list(sctp_build.rglob("libusrsctp.a"))
        if len(binaries) != 1:
            raise RuntimeError("Expected one usrsctp archive")
        bundles["usrsctp"].append(framework("usrsctp", binaries[0], sctp_headers, stage, platform))
    for name, slices in bundles.items():
        destination = output / (name + ".xcframework")
        if destination.exists():
            shutil.rmtree(destination)
        run(["xcodebuild", "-create-xcframework", "-framework", slices[0],
             "-framework", slices[1], "-output", destination])
        print("Built", destination, flush=True)


if __name__ == "__main__":
    main()
