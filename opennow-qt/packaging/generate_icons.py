#!/usr/bin/env python3
"""Generate shared brand icons and macOS icons from the Apple Icon Composer export."""

import argparse
import base64
import io
from pathlib import Path
import struct
import xml.etree.ElementTree as ET

from PIL import Image, ImageDraw


PACKAGING = Path(__file__).resolve().parent
SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
ICNS_FRAMES = {
    b"icp4": 16, b"icp5": 32, b"icp6": 64,
    b"ic07": 128, b"ic08": 256, b"ic09": 512, b"ic10": 1024,
    b"ic11": 32, b"ic12": 64, b"ic13": 256, b"ic14": 512,
}


def generate_shared_assets():
    with Image.open(PACKAGING.parents[1] / "logo.png") as source:
        source = source.convert("RGBA")
        bounds = source.getchannel("A").getbbox()
        if bounds is None:
            raise ValueError("logo.png must contain visible artwork")
        mark = source.crop(bounds)
    mark.thumbnail((896, 896), Image.Resampling.LANCZOS)
    master = Image.new("RGBA", (1024, 1024))
    master.paste(mark, ((1024 - mark.width) // 2, (1024 - mark.height) // 2))
    images = {size: master.resize((size, size), Image.Resampling.LANCZOS) for size in SIZES}
    assets = {}
    for size, image in images.items():
        output = io.BytesIO()
        image.save(output, format="PNG")
        assets[f"icons/opennow-{size}.png"] = output.getvalue()
    output = io.BytesIO()
    images[256].save(output, format="ICO", sizes=[(s, s) for s in SIZES if s <= 256],
                     append_images=[images[s] for s in SIZES if s < 256])
    assets["icons/OpenNOW.ico"] = output.getvalue()
    png = base64.b64encode(assets["icons/opennow-512.png"]).decode("ascii")
    assets["io.github.opencloudgaming.OpenNOW.svg"] = (
        '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
        'width="512" height="512" viewBox="0 0 512 512" role="img" aria-label="OpenNOW">\n'
        f'  <image width="512" height="512" xlink:href="data:image/png;base64,{png}"/>\n'
        '</svg>\n'
    ).encode("ascii")
    return assets


def generate_macos_assets():
    with Image.open(PACKAGING / "icons/macos-source.png") as source:
        if source.size != (1024, 1024) or source.mode != "RGBA":
            raise ValueError("macos-source.png must be the 1024-pixel RGBA Icon Composer export")
        master = source.copy()
    images = {size: master.resize((size, size), Image.Resampling.LANCZOS) for size in SIZES}
    assets = {}
    for size, image in images.items():
        output = io.BytesIO()
        image.save(output, format="PNG")
        assets[f"icons/opennow-macos-{size}.png"] = output.getvalue()
    # Pillow's ICNS writer omits the 16/32-pixel standard representations.
    # ICNS permits lossless PNG chunks; include every standard and Retina frame
    # without requiring Apple's iconutil on Windows or Linux build hosts.
    chunks = []
    for kind, size in ICNS_FRAMES.items():
        png = assets[f"icons/opennow-macos-{size}.png"]
        chunks.append(kind + struct.pack(">I", 8 + len(png)) + png)
    payload = b"".join(chunks)
    assets["icons/OpenNOW.icns"] = b"icns" + struct.pack(">I", 8 + len(payload)) + payload
    return assets


def generate_assets(platform="all"):
    assets = generate_shared_assets() if platform in ("all", "shared") else {}
    if platform in ("all", "macos"):
        assets.update(generate_macos_assets())
    return assets


def same_image(actual, expected):
    return (actual.size == expected.size and actual.mode == expected.mode
            and actual.info.get("icc_profile") == expected.info.get("icc_profile")
            and actual.convert("RGBA").tobytes() == expected.convert("RGBA").tobytes())


def same_asset(relative_path, actual, expected):
    """Compare artwork and frame inventories, independently of PNG compression."""
    if actual == expected:
        return True
    try:
        suffix = Path(relative_path).suffix.lower()
        if suffix == ".svg":
            actual_root, expected_root = ET.fromstring(actual), ET.fromstring(expected)
            href = "{http://www.w3.org/1999/xlink}href"
            images = [root.find("{http://www.w3.org/2000/svg}image")
                      for root in (actual_root, expected_root)]
            prefix = "data:image/png;base64,"
            if any(image is None or not image.get(href, "").startswith(prefix) for image in images):
                return False
            payloads = [base64.b64decode(image.get(href)[len(prefix):], validate=True)
                        for image in images]
            for image in images:
                image.set(href, "embedded-png")
            return (ET.tostring(actual_root) == ET.tostring(expected_root)
                    and same_asset("embedded.png", *payloads))
        with Image.open(io.BytesIO(actual)) as left, Image.open(io.BytesIO(expected)) as right:
            if left.format != right.format:
                return False
            if suffix == ".ico":
                return (left.ico.sizes() == right.ico.sizes()
                        and all(same_image(left.ico.getimage(size), right.ico.getimage(size))
                                for size in right.ico.sizes()))
            if suffix == ".icns":
                return (set(left.info["sizes"]) == set(right.info["sizes"])
                        and all(same_image(left.icns.getimage(size), right.icns.getimage(size))
                                for size in right.info["sizes"]))
            return suffix == ".png" and same_image(left, right)
    except (OSError, ValueError, KeyError, SyntaxError):
        return False


def write_preview(assets, path):
    preview = Image.new("RGB", (960, 480))
    draw = ImageDraw.Draw(preview)
    for column, background in enumerate(("#f4f4f4", "#202124")):
        x = column * 480
        draw.rectangle((x, 0, x + 479, 479), fill=background)
        foreground = "#202124" if column == 0 else "#f4f4f4"
        prefix = "icons/opennow-macos" if "icons/opennow-macos-256.png" in assets else "icons/opennow"
        draw.text((x + 24, 20), "OpenNOW / Apple Icon Composer" if "macos" in prefix else "OpenNOW / logo.png", fill=foreground)
        with Image.open(io.BytesIO(assets[f"{prefix}-256.png"])) as image:
            preview.paste(image, (x + 112, 55), image)
        position = x + 24
        for size in (16, 24, 32, 48, 64, 128):
            with Image.open(io.BytesIO(assets[f"{prefix}-{size}.png"])) as image:
                preview.paste(image, (position, 365 - size // 2), image)
            draw.text((position, 442), str(size), fill=foreground)
            position += size + 14
    path.parent.mkdir(parents=True, exist_ok=True)
    preview.save(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Verify decoded artwork and frame inventories")
    parser.add_argument("--platform", choices=("all", "shared", "macos"), default="all",
                        help="Generate or verify only the selected platform's assets")
    parser.add_argument("--preview", type=Path, help="Write a light/dark size contact sheet")
    args = parser.parse_args()
    assets = generate_assets(args.platform)
    stale = []
    for relative_path, content in assets.items():
        path = PACKAGING / relative_path
        if args.check:
            if not path.exists() or not same_asset(relative_path, path.read_bytes(), content):
                stale.append(relative_path)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
    if args.preview:
        write_preview(assets, args.preview)
    if stale:
        parser.exit(1, "Stale application icons: " + ", ".join(stale) + "\n")
    print(f"{'Verified' if args.check else 'Generated'} {len(assets)} application icon assets")


if __name__ == "__main__":
    main()
