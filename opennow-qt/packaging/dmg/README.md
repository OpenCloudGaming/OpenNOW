# macOS disk image

The DMG uses the approved [Paper design A](https://app.paper.design/file/01M5SKRE8ET2F3B1KZCSS4YCZ2).
The background contains instructional text, the mint arrow, and filename backings. Finder supplies the real
OpenNOW app icon, Applications alias, labels, and window chrome.
Rounded neutral backings, centered on the label text, keep Finder's filenames readable with either black or white
native label text; the app and folder remain normal draggable Finder items.

Nightly builds and signed release packaging share `../macos-dmg.cmake` and the
saved Finder layout `layout.dsstore`. Builds need no GUI session or Finder
Automation permission. Signed releases stage the notarized app through CPack without
reinstalling or stripping it, then sign and notarize the resulting DMG as before.

`background.tiff` is checked in, with 1x and 2x representations at the same logical
720 x 500 size. Extra black below the artwork covers native title-bar height
differences. Normal builds need none of the Python asset-generation dependencies.
To regenerate the artwork on macOS, install the pinned `requirements.txt` and run:

```sh
python3 opennow-qt/packaging/dmg/generate_background.py
```

To regenerate the layout, mount a disposable HFS+ image named `OpenNOW` containing
the current background at `.background/background.tiff`, then run:

```sh
python3 opennow-qt/packaging/dmg/generate_layout.py /Volumes/OpenNOW
```

The generator writes the saved layout directly, without automating Finder.
Keep its real item positions aligned with the artwork coordinates. The artwork
excludes the native title bar. Do not bake the app icon, folder, file labels,
or title bar into the background.
