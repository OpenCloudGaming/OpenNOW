# Application icons

Windows and Linux use the repository-root `logo.png`. The generator trims its
transparent padding, fits the unmodified cloud into a centered 896-pixel area on
a transparent 1024-pixel canvas, and downsamples with Lanczos.

macOS uses Apple's Icon Composer project in `OpenNOW.icon/`. Its foreground is
an unchanged copy of the canonical `logo.png`, centered at 59.5% scale with Icon
Composer's native effects. Default uses System Light and Dark uses System Dark.
Open this project in Apple Icon Composer to edit it, then export the 1024-pixel
**macOS pre-Tahoe** image in the **Default** appearance as `macos-source.png`.
That export supplies the rounded tile, alpha, lighting, and cloud treatment for
macOS 13–15. The Python generator downsamples and packages the export.

The bundle uses the layered `Assets.car` on Tahoe and later and the `OpenNOW.icns`
fallback on older macOS versions. Compile the catalog with Icon Composer support
in Xcode. The compiled resource is checked in so release builders can keep using
the macOS 13 deployment toolchain.

```sh
mkdir -p build/macos-icon
xcrun actool --compile build/macos-icon --platform macosx --target-device mac \
  --minimum-deployment-target 13.0 --app-icon OpenNOW \
  --output-partial-info-plist build/macos-icon/IconInfo.plist \
  opennow-qt/packaging/icons/OpenNOW.icon
cp build/macos-icon/Assets.car opennow-qt/packaging/icons/Assets.car
```

From the repository root, generate and check the assets with the pinned Pillow
version:

```sh
python3 -m venv build/icon-tools
build/icon-tools/bin/pip install -r opennow-qt/packaging/icon-requirements.txt
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py --check --preview build/icon-preview.png
build/icon-tools/bin/python -m unittest discover -s opennow-qt/tests -p test_application_icons.py
```

After a Mac-only edit, preserve Windows and Linux assets with:

```sh
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py --platform macos
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py --platform macos --check
```

On Windows, use `build/icon-tools/Scripts/python.exe` and
`build/icon-tools/Scripts/pip.exe`. Generation does not require macOS `iconutil` or
Windows tools. Assets are checked in, so normal application builds do not need
Python or Pillow. `--check` regenerates in memory and compares decoded artwork,
color profiles, frame inventories, and the Linux SVG structure. Different
PNG compression from Pillow builds is accepted; changed pixels, missing sizes,
malformed files, and external SVG image references fail. Checks never rewrite
assets.

`OpenNOW.ico` contains 16, 24, 32, 48, 64, 128, and 256-pixel RGBA images for the
Windows executable resource and the setup.exe icon. `OpenNOW.icns` contains
standard images at 16, 32, 64, 128, 256, and 512 pixels plus Retina images through
1024 pixels for the macOS bundle. `opennow-macos-*.png` supplies the matching Qt
resources. `opennow-*.png` supplies Windows, Linux, and Linux hicolor entries.
The generated SVG embeds the shared 512-pixel PNG to preserve the
existing Linux AppImage packaging path. It is a raster-backed SVG, not a traced
vector approximation.

The focused Qt resource and window-inheritance test runs after configuration:

```sh
cmake --build build/opennow-qt --target opennow-applicationicons-tests
ctest --test-dir build/opennow-qt -R '^opennow-applicationicons-tests$' --output-on-failure
```

Changes to a source or generation algorithm must include the corresponding
regenerated assets. Inspect the preview on both backgrounds before publishing.
