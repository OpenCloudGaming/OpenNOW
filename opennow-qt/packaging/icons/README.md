# Application icons

The repository-root `logo.png` is the source for every Qt application icon. The
generator trims transparent padding, fits the unmodified cloud into a centered
896-pixel area on a transparent 1024-pixel canvas, and downsamples with Lanczos.
It does not redraw, recolor, or add a background to the artwork.

From the repository root, generate and check the assets with the pinned Pillow
version:

```sh
python3 -m venv build/icon-tools
build/icon-tools/bin/pip install -r opennow-qt/packaging/icon-requirements.txt
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py
build/icon-tools/bin/python opennow-qt/packaging/generate_icons.py --check --preview build/icon-preview.png
build/icon-tools/bin/python -m unittest discover -s opennow-qt/tests -p test_application_icons.py
```

On Windows, use `build/icon-tools/Scripts/python.exe` and
`build/icon-tools/Scripts/pip.exe`. Generation does not require macOS `iconutil` or
Windows tools. Assets are checked in, so normal application builds do not need
Python or Pillow. `--check` regenerates in memory and fails without changing any
assets when the checked-in bytes differ.

`OpenNOW.ico` contains 16, 24, 32, 48, 64, 128, and 256-pixel RGBA images for the
Windows executable resource and the MSI's installed-product icon. `OpenNOW.icns` contains standard and Retina images
through 1024 pixels for the macOS bundle. PNGs supply Qt's window icon and Linux
hicolor entries. The generated SVG embeds the 512-pixel PNG to preserve the
existing Linux AppImage packaging path. It is a raster-backed SVG, not a traced
vector approximation.

The focused Qt resource and window-inheritance test runs after configuration:

```sh
cmake --build build/opennow-qt --target opennow-applicationicons-tests
ctest --test-dir build/opennow-qt -R '^opennow-applicationicons-tests$' --output-on-failure
```

Changes to the source logo or generation algorithm must include regenerated
assets. Inspect the preview on both backgrounds before publishing them.
