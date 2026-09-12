import base64
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
        installer = (QT_ROOT / "cmake/WindowsInstaller.cmake").read_text()
        self.assertIn('set(CPACK_WIX_PRODUCT_ICON "${CMAKE_CURRENT_SOURCE_DIR}/packaging/icons/OpenNOW.ico")',
                      installer)

    def test_macos_icon_contains_standard_and_retina_images(self):
        with Image.open(ICONS / "OpenNOW.icns") as icon:
            sizes = icon.info["sizes"]
            self.assertIn((512, 512, 2), sizes)
            self.assertIn((16, 16, 2), sizes)
            self.assertIn((32, 32, 2), sizes)
            for width, height, scale in sizes:
                image = icon.icns.getimage((width, height, scale))
                with Image.open(ICONS / f"opennow-{width * scale}.png") as expected:
                    self.assertEqual(image.convert("RGBA").tobytes(), expected.tobytes())

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
        desktop = (PACKAGING / "io.github.opencloudgaming.OpenNOW.desktop").read_text()
        self.assertIn("Icon=io.github.opencloudgaming.OpenNOW\n", desktop)


if __name__ == "__main__":
    unittest.main()
