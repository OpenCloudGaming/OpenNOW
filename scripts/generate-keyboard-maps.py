import argparse
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.request import urlopen
import xml.etree.ElementTree as ET


LAYOUTS = {
    "en-US": "KBDUS", "en-GB": "KBDUK", "tr-TR": "KBDTUQ", "de-DE": "KBDGR",
    "fr-FR": "KBDFR", "es-ES": "KBDSP", "es-MX": "KBDLA", "it-IT": "KBDIT",
    "pt-PT": "KBDPO", "pt-BR": "KBDBR", "pl-PL": "KBDPL1", "da-DK": "KBDDA",
    "nb-NO": "KBDNO", "sv-SE": "KBDSW", "fi-FI": "KBDFI", "ru-RU": "KBDRU",
    "uk-UA": "KBDUR", "ja-JP": "KBD106", "ko-KR": "KBDKOR",
    "zh-CN": "KBDUS", "zh-TW": "KBDUS",
}
VIRTUAL_KEYS = {
    "OEM_1": 0xBA, "OEM_PLUS": 0xBB, "OEM_COMMA": 0xBC, "OEM_MINUS": 0xBD,
    "OEM_PERIOD": 0xBE, "OEM_2": 0xBF, "OEM_3": 0xC0, "ABNT_C1": 0xC1,
    "ABNT_C2": 0xC2, "OEM_4": 0xDB, "OEM_5": 0xDC, "OEM_6": 0xDD,
    "OEM_7": 0xDE, "OEM_8": 0xDF, "OEM_102": 0xE2, "OEM_AUTO": 0xF3,
    "OEM_COPY": 0xF2, "CONVERT": 0x1C, "NONCONVERT": 0x1D,
    "DBE_ALPHANUMERIC": 0xF0,
}
PRINTABLE_SCANCODES = (
    set(range(2, 14)) | set(range(16, 28)) | set(range(30, 42))
    | set(range(43, 54)) | {0x56, 0x73}
)
INTERNATIONAL_SCANCODES = {0x70: 93, 0x73: 89, 0x79: 92, 0x7B: 94, 0x7D: 124, 0x7E: 121}
OUTPUT = Path(__file__).resolve().parents[1] / "opennow-qt/src/streaming/PhysicalKeyMapData.h"


def read_layout(driver, source_dir):
    if source_dir:
        data = (source_dir / f"{driver}.xml").read_bytes()
    else:
        with urlopen(f"https://kbdlayout.info/{driver}/download/xml", timeout=30) as response:
            data = response.read()
    root = ET.fromstring(data)
    keys = [0] * 128
    selected = PRINTABLE_SCANCODES | ({0x7E} if driver == "KBDBR" else set())
    if driver == "KBD106":
        selected = selected | {0x3A, 0x70, 0x79, 0x7B, 0x7D}
    for key in root.findall("./PhysicalKeys/PK"):
        scan = int(key.get("SC", "0"), 16)
        if scan not in selected:
            continue
        name = key.attrib["VK"].removeprefix("VK_")
        vk = ord(name) if len(name) == 1 and name.isascii() and name.isalnum() else VIRTUAL_KEYS[name]
        evdev = INTERNATIONAL_SCANCODES.get(scan, scan)
        if keys[evdev] and keys[evdev] != vk:
            raise ValueError(f"Conflicting mapping for {driver}: {scan:x}")
        keys[evdev] = vk
    if sum(bool(key) for key in keys) < 47:
        raise ValueError(f"Incomplete layout: {driver}")
    return keys


def main():
    parser = argparse.ArgumentParser(description="Regenerate Windows regional VK tables from kbdlayout.info.")
    parser.add_argument("--source-dir", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    drivers = list(dict.fromkeys(LAYOUTS.values()))
    with ThreadPoolExecutor(max_workers=6) as pool:
        tables = dict(zip(drivers, pool.map(lambda driver: read_layout(driver, args.source_dir), drivers)))
    lines = ['#pragma once', '', '#include "streaming/PhysicalKeyMap.h"', '',
             'namespace PhysicalKeyMap {', '', 'inline constexpr Layout layouts[] = {']
    for locale, driver in LAYOUTS.items():
        lines.append(f'    {{"{locale}", {{{{')
        keys = tables[driver]
        for offset in range(0, len(keys), 16):
            lines.append('        ' + ', '.join(f'0x{key:02x}' for key in keys[offset:offset + 16]) + ',')
        lines.append('    }}},')
    lines.extend(['};', '', '}', ''])
    result = '\n'.join(lines)
    if args.check:
        if OUTPUT.read_text() != result:
            raise SystemExit(f"Stale keyboard maps: run {Path(__file__).name}")
        print(f"Verified {len(LAYOUTS)} regional keyboard maps")
    else:
        OUTPUT.write_text(result)


if __name__ == "__main__":
    main()
