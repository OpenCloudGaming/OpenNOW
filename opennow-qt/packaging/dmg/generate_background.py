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
    render_scale = scale * 4
    image = Image.new("RGB", (720 * render_scale, 500 * render_scale), "#000000")
    draw = ImageDraw.Draw(image)

    def text(value, top, size, weight, color):
        font = ImageFont.truetype(str(FONT), size * render_scale)
        font.set_variation_by_axes([weight])
        ascent, descent = font.getmetrics()
        baseline = (top + size / 2) * render_scale + (ascent - descent) / 2
        draw.text((image.width / 2, baseline), value, font=font, fill=color, anchor="ms")

    text("Install OpenNOW", 54, 32, 900, "#FFFFFF")
    text("Drag OpenNOW into Applications.", 101, 15, 500, "#A3A3A3")
    text("Then open OpenNOW from Applications.", 399, 13, 400, "#A3A3A3")
    # Finder owns filename color. This neutral supports both black and white labels.
    # Plates are centered on Finder's measured 14 pt label, descenders included.
    for left, right in ((180, 262), (450, 544)):
        draw.rounded_rectangle((left * render_scale, 314 * render_scale,
                                right * render_scale, 334 * render_scale),
                               radius=5 * render_scale, fill="#767676")
    for points in (((322, 241), (395, 241)), ((383, 231), (395, 241), (383, 251))):
        draw.line([(x * render_scale, y * render_scale) for x, y in points],
                  fill="#56E6A5", width=2 * render_scale, joint="curve")
        for x, y in points:
            draw.ellipse(((x - 1) * render_scale, (y - 1) * render_scale,
                          (x + 1) * render_scale, (y + 1) * render_scale), fill="#56E6A5")
    # Render above the output resolution so text and diagonal strokes stay smooth
    # in both Finder's standard and Retina image representations.
    return image.resize((720 * scale, 500 * scale), Image.Resampling.LANCZOS)


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
