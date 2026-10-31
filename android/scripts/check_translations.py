#!/usr/bin/env python3
"""Check Android locale coverage and Java format arguments, including generated strings."""

from collections import Counter
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET


ANDROID = Path(__file__).resolve().parents[1]
RES = ANDROID / "app/src/main/res"
FORMAT = re.compile(r"%(?:\d+\$)?[-#+ 0,(]*\d*(?:\.\d+)?[a-zA-Z%]")
ANDROID_NAME = "{http://schemas.android.com/apk/res/android}name"


def read_resources(path):
    strings = {}
    for entry in ET.parse(path).getroot():
        if entry.tag not in {"string", "plurals"}:
            continue
        name = entry.attrib["name"]
        if name in strings:
            raise ValueError(f"{path}: duplicate resource {name}")
        strings[name] = entry
    return strings


def main():
    resources = read_resources(RES / "values/strings.xml")
    source = {
        name: "".join(entry.itertext())
        for name, entry in resources.items()
        if entry.tag == "string" and entry.get("translatable") != "false"
    }
    plurals = {
        name: entry
        for name, entry in resources.items()
        if entry.tag == "plurals" and entry.get("translatable") != "false"
    }
    shared = json.loads((ANDROID.parent / "locales/en.json").read_text())
    for group, strings in shared.items():
        if group.startswith("android") and isinstance(strings, dict):
            for name, value in strings.items():
                if name in source or name in plurals:
                    raise ValueError(f"Duplicate Android source string: {name}")
                source[name] = value

    errors = []
    locales = ET.parse(RES / "xml/locale_config.xml").getroot()
    checked = 0
    for locale in locales:
        tag = locale.attrib[ANDROID_NAME]
        if tag == "en":
            continue
        qualifier = "b+" + tag.replace("-", "+") if "-" in tag else {"id": "in"}.get(tag, tag)
        path = RES / f"values-{qualifier}/strings.xml"
        if not path.exists():
            errors.append(f"{tag}: missing {path.relative_to(ANDROID)}")
            continue
        translated = read_resources(path)
        for name, english in source.items():
            if name not in translated:
                errors.append(f"{tag}: missing {name}")
                continue
            value = "".join(translated[name].itertext())
            if not value.strip():
                errors.append(f"{tag}: empty {name}")
            if Counter(FORMAT.findall(english)) != Counter(FORMAT.findall(value)):
                errors.append(f"{tag}: format arguments differ for {name}")
        for name, entry in plurals.items():
            if name not in translated:
                errors.append(f"{tag}: missing plural {name}")
                continue
            forms = translated[name].findall("item")
            quantities = [form.get("quantity") for form in forms]
            if (
                "other" not in quantities
                or len(set(quantities)) != len(quantities)
                or any(q not in {"zero", "one", "two", "few", "many", "other"} for q in quantities)
            ):
                errors.append(f"{tag}: invalid plural forms for {name}")
            english = "".join(entry.find("item[@quantity='other']").itertext())
            expected = Counter(FORMAT.findall(english))
            for form in forms:
                value = "".join(form.itertext())
                actual = Counter(FORMAT.findall(value))
                # Languages such as Arabic spell out singular and dual counts.
                implicit_count = (
                    form.get("quantity") in {"zero", "one", "two"}
                    and not actual
                    and expected in (Counter({"%d": 1}), Counter({"%1$d": 1}))
                )
                if not value.strip() or (actual != expected and not implicit_count):
                    errors.append(f"{tag}: invalid plural text for {name}/{form.get('quantity')}")
        checked += 1

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Android translations OK: {len(source)} strings and {len(plurals)} plurals across {checked} locales.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
