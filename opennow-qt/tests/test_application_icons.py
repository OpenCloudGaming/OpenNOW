import base64
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import unittest
import xml.etree.ElementTree as ET

from PIL import Image, ImageChops


QT_ROOT = Path(__file__).resolve().parents[1]
PACKAGING = QT_ROOT / "packaging"
ICONS = PACKAGING / "icons"
SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
SPEC = importlib.util.spec_from_file_location("generate_icons", PACKAGING / "generate_icons.py")
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class ApplicationIconTests(unittest.TestCase):
    def test_generated_assets_are_current(self):
        result = subprocess.run(
            [sys.executable, str(PACKAGING / "generate_icons.py"), "--check"],
            capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_pngs_preserve_canonical_brand_and_transparency(self):
        with Image.open(QT_ROOT.parent / "logo.png") as source:
            source = source.convert("RGBA")
            mark = source.crop(source.getchannel("A").getbbox())
        mark.thumbnail((896, 896), Image.Resampling.LANCZOS)
        master = Image.new("RGBA", (1024, 1024))
        master.paste(mark, ((1024 - mark.width) // 2, (1024 - mark.height) // 2))
        for size in SIZES:
            with self.subTest(size=size), Image.open(ICONS / f"opennow-{size}.png") as image:
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.size, (size, size))
                expected = master.resize((size, size), Image.Resampling.LANCZOS)
                self.assertIsNone(ImageChops.difference(image, expected).getbbox(alpha_only=False))
                self.assertEqual(image.getpixel((0, 0))[3], 0)
                self.assertGreater(image.getchannel("A").getextrema()[1], 240)

    def test_windows_icon_contains_all_taskbar_sizes(self):
        with Image.open(ICONS / "OpenNOW.ico") as icon:
            self.assertEqual(icon.ico.sizes(), {(size, size) for size in SIZES if size <= 256})
            for size in sorted(icon.ico.sizes()):
                image = icon.ico.getimage(size).convert("RGBA")
                with Image.open(ICONS / f"opennow-{size[0]}.png") as expected:
                    self.assertEqual(image.tobytes(), expected.tobytes())

    def test_windows_installer_uses_the_application_icon(self):
        installer = (QT_ROOT / "packaging/windows/setup.iss").read_text()
        self.assertIn("SetupIconFile=..\\icons\\OpenNOW.ico", installer)

    def test_macos_icon_contains_standard_and_retina_images(self):
        with Image.open(ICONS / "OpenNOW.icns") as icon:
            sizes = icon.info["sizes"]
            self.assertEqual(set(sizes),
                             {(size, size, 1) for size in (16, 32, 64, 128, 256, 512)}
                             | {(size, size, 2) for size in (16, 32, 128, 256, 512)})
            for width, height, scale in sizes:
                image = icon.icns.getimage((width, height, scale))
                with Image.open(ICONS / f"opennow-macos-{width * scale}.png") as expected:
                    self.assertEqual(image.convert("RGBA").tobytes(), expected.tobytes())

    def test_macos_icons_preserve_the_apple_composer_export(self):
        project = ICONS / "OpenNOW.icon"
        specification = json.loads((project / "icon.json").read_text())
        self.assertEqual(specification["fill-specializations"], [
            {"value": "system-light"},
            {"appearance": "dark", "value": "system-dark"},
        ])
        for group in specification["groups"]:
            for layer in group["layers"]:
                self.assertEqual((project / "Assets" / layer["image-name"]).read_bytes(),
                                 (QT_ROOT.parent / "logo.png").read_bytes())
        with Image.open(ICONS / "macos-source.png") as source:
            self.assertEqual(source.mode, "RGBA")
            self.assertEqual(source.size, (1024, 1024))
            for size in SIZES:
                with self.subTest(size=size), Image.open(ICONS / f"opennow-macos-{size}.png") as image:
                    expected = source.resize((size, size), Image.Resampling.LANCZOS)
                    self.assertEqual(image.size, (size, size))
                    self.assertEqual(image.mode, "RGBA")
                    self.assertEqual(image.tobytes(), expected.tobytes())
                    self.assertEqual(image.getpixel((0, 0))[3], 0)
                    if size >= 128:
                        r, g, b, alpha = image.getpixel((size // 2, size // 5))
                        self.assertEqual(alpha, 255)
                        self.assertGreater(min(r, g, b), 200)

    def test_check_accepts_recompression_but_rejects_changed_pixels(self):
        original = (ICONS / "opennow-macos-32.png").read_bytes()
        with Image.open(io.BytesIO(original)) as source:
            recompressed = io.BytesIO()
            source.save(recompressed, format="PNG", compress_level=0)
            self.assertNotEqual(original, recompressed.getvalue())
            self.assertTrue(GENERATOR.same_asset("icon.png", original, recompressed.getvalue()))
            changed = source.copy()
            r, g, b, alpha = changed.getpixel((16, 16))
            changed.putpixel((16, 16), (r ^ 1, g, b, alpha))
            corrupted = io.BytesIO()
            changed.save(corrupted, format="PNG")
            self.assertFalse(GENERATOR.same_asset("icon.png", original, corrupted.getvalue()))
            smaller = io.BytesIO()
            source.resize((16, 16)).save(smaller, format="PNG")
            self.assertFalse(GENERATOR.same_asset("icon.png", original, smaller.getvalue()))
        self.assertFalse(GENERATOR.same_asset("icon.png", original, b"invalid PNG"))

    def test_check_rejects_missing_container_frames_and_external_svg_images(self):
        with Image.open(ICONS / "opennow-256.png") as source:
            incomplete = io.BytesIO()
            source.save(incomplete, format="ICO", sizes=[(256, 256)])
        self.assertFalse(GENERATOR.same_asset("OpenNOW.ico", (ICONS / "OpenNOW.ico").read_bytes(),
                                              incomplete.getvalue()))
        original = (PACKAGING / "io.github.opencloudgaming.OpenNOW.svg").read_bytes()
        changed = ET.fromstring(original)
        image = changed.find("{http://www.w3.org/2000/svg}image")
        image.set("{http://www.w3.org/1999/xlink}href", "https://example.invalid/icon.png")
        self.assertFalse(GENERATOR.same_asset("icon.svg", original, ET.tostring(changed)))

    def test_linux_svg_embeds_the_same_brand_without_external_files(self):
        svg = ET.parse(PACKAGING / "io.github.opencloudgaming.OpenNOW.svg").getroot()
        image = svg.find("{http://www.w3.org/2000/svg}image")
        data = image.attrib["{http://www.w3.org/1999/xlink}href"]
        self.assertTrue(data.startswith("data:image/png;base64,"))
        self.assertEqual(base64.b64decode(data.split(",", 1)[1]),
                         (ICONS / "opennow-512.png").read_bytes())

    def test_bundle_identity_is_preserved_and_icon_is_declared(self):
        plist = plistlib.loads((PACKAGING / "Info.plist.in").read_bytes())
        self.assertEqual(plist["CFBundleIdentifier"], "${MACOSX_BUNDLE_GUI_IDENTIFIER}")
        self.assertEqual(plist["CFBundleIconFile"], "${MACOSX_BUNDLE_ICON_FILE}")
        self.assertEqual(plist["CFBundleIconName"], "OpenNOW")
        bundle_resources = (QT_ROOT / "cmake/ApplicationIcons.cmake").read_text()
        self.assertIn("packaging/icons/Assets.car packaging/icons/OpenNOW.icns", bundle_resources)
        self.assertTrue((ICONS / "Assets.car").read_bytes().startswith(b"BOMStore"))
        desktop = (PACKAGING / "io.github.opencloudgaming.OpenNOW.desktop").read_text()
        self.assertIn("Icon=io.github.opencloudgaming.OpenNOW\n", desktop)


if __name__ == "__main__":
    unittest.main()
