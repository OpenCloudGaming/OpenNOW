#!/usr/bin/env python3
"""Save the Finder layout using a disposable mounted OpenNOW disk image (macOS)."""

import argparse
from pathlib import Path

from ds_store import DSStore
from mac_alias import Alias


HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("volume", type=Path, help="Mounted disposable OpenNOW volume")
    volume = parser.parse_args().volume.resolve()
    background = volume / ".background/background.tiff"
    if background.read_bytes() != (HERE / "background.tiff").read_bytes():
        parser.error("The mounted volume must contain the current DMG background")
    alias = Alias.for_file(str(background))
    if alias.volume.name != b"OpenNOW":
        parser.error("The disposable volume must be named OpenNOW")
    with DSStore.open(str(HERE / "layout.dsstore"), "w+") as store:
        store["."]["vSrn"] = ("long", 1)
        store["."]["icvl"] = ("type", "icnv")
        store["."]["bwsp"] = {
            "WindowBounds": "{{200, 160}, {720, 500}}",
            "ShowStatusBar": False, "ShowTabView": False,
            "ShowToolbar": False, "ShowPathbar": False, "ShowSidebar": False,
            "ContainerShowSidebar": False, "PreviewPaneVisibility": False,
            "SidebarWidth": 0,
        }
        store["."]["icvp"] = {
            "viewOptionsVersion": 1,
            "backgroundType": 2, "backgroundImageAlias": alias.to_bytes(),
            "backgroundColorRed": 0.0, "backgroundColorGreen": 0.0,
            "backgroundColorBlue": 0.0,
            "gridOffsetX": 0.0, "gridOffsetY": 0.0, "gridSpacing": 100.0,
            "arrangeBy": "none", "showIconPreview": False, "showItemInfo": False,
            "labelOnBottom": True, "textSize": 14.0, "iconSize": 128.0,
            "scrollPositionX": 0.0, "scrollPositionY": 0.0,
        }
        store["OpenNOW.app"]["Iloc"] = (222, 240)
        store["Applications"]["Iloc"] = (498, 240)


if __name__ == "__main__":
    main()
