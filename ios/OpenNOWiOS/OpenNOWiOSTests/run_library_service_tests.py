#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile


root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description="Compile and exercise production library service methods without Apple frameworks")
parser.add_argument("--source", type=Path, default=root / "OpenNOWiOS/OpenNOWStore.swift")
args = parser.parse_args()
source = args.source.read_text()


def method(name):
    start = source.index(f"    {name}")
    end = re.search(r"\n    (?:private )?(?:static )?func ", source[start + 1:])
    return source[start:start + 1 + end.start()]


methods = [method("func fetchLibraryGames("), method("private func enrichGamesWithMetadata(")]
if "    private func fetchLibraryPage(" in source:
    methods.append(method("private func fetchLibraryPage("))
for name in ("searchResultsAsPanelPayload", "flattenPanels", "extractGameMetadata", "mergeGameMetadata", "selectedVariant", "launchOptions", "extractFeatureLabels", "toOptionalStringArray", "imageURLs", "mergedImageURLs", "formatReleaseDate", "optimizedImageURL", "toOptionalString"):
    methods.append(method(f"private static func {name}("))

types = []
for start, end in (("struct CloudGame:", "func gameMatchesCatalogSearch("), ("enum GFNCatalogLabelParser", "struct StreamRegion:"), ("struct GameLaunchOption:", "struct SessionTelemetry:")):
    types.append(source[source.index(start):source.index(end)])

harness = (root / "OpenNOWiOSTests/LibraryServiceFixture.swift.in").read_text()
harness = harness.replace("__PRODUCTION_METHODS__", "\n".join(methods))
harness = harness.replace("__PRODUCTION_TYPES__", "\n".join(types))
with tempfile.TemporaryDirectory(prefix="opennow-library-") as directory:
    swift = Path(directory) / "LibraryServiceFixture.swift"
    executable = Path(directory) / "library-tests"
    swift.write_text(harness)
    subprocess.run([os.environ.get("SWIFTC", "swiftc"), "-swift-version", "5", "-parse-as-library", str(swift), "-o", str(executable)], check=True)
    subprocess.run([str(executable)], check=True)
