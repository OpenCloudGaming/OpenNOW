#!/usr/bin/env python3
"""Render the approved DMG background with the app's Nunito font (macOS)."""

from pathlib import Path
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont


HERE = Path(__file__).resolve().parent
FONT = HERE.parents[1] / "res/fonts/Nunito-Variable.ttf"


def background(scale):
    # Coordinates exclude Paper's 36 px title bar. Extra black below the artwork
    # covers Finder title-bar height differences without exposing a white edge.
    image = Image.new("RGB", (720 * scale, 500 * scale), "#000000")
    draw = ImageDraw.Draw(image)

    def text(value, top, size, weight, color, tracking=0):
        font = ImageFont.truetype(str(FONT), size * scale)
        font.set_variation_by_axes([weight])
        spacing = tracking * size * scale
        width = sum(font.getlength(letter) for letter in value) + spacing * (len(value) - 1)
        x = (image.width - width) / 2
        ascent, descent = font.getmetrics()
        baseline = (top + size / 2) * scale + (ascent - descent) / 2
        for letter in value:
            draw.text((x, baseline), letter, font=font, fill=color, anchor="ls")
            x += font.getlength(letter) + spacing

    text("Install OpenNOW", 54, 32, 900, "#FFFFFF", -0.035)
    text("Drag OpenNOW into Applications.", 101, 15, 500, "#A3A3A3")
    text("Then open OpenNOW from Applications.", 399, 13, 400, "#A3A3A3")
    # Finder owns filename color. This neutral supports both black and white labels.
    for left, right in ((174, 270), (440, 556)):
        draw.rectangle((left * scale, 306 * scale, right * scale, 330 * scale),
                       fill="#767676")
    draw.line([(x * scale, y * scale) for x, y in ((322, 241), (395, 241))],
              fill="#56E6A5", width=2 * scale)
    draw.line([(x * scale, y * scale) for x, y in ((383, 231), (395, 241), (383, 251))],
              fill="#56E6A5", width=2 * scale, joint="curve")
    return image


def main():
    with tempfile.TemporaryDirectory(prefix="opennow-dmg-background-") as directory:
        paths = []
        for scale in (1, 2):
            path = Path(directory) / f"background-{scale}x.tiff"
            background(scale).save(path, dpi=(72 * scale, 72 * scale), compression="tiff_lzw")
            paths.append(path)
        subprocess.run(["tiffutil", "-cathidpicheck", *paths, "-out", HERE / "background.tiff"],
                       check=True)


if __name__ == "__main__":
    main()
