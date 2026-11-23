#!/usr/bin/env python3
"""
ECC Verification Gate for OpenNOW-ORC (opennow-qt/vn-translation).
"""
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
VN_DIR = REPO / "opennow-qt" / "vn-translation"


def strip_comments_and_strings(code: str) -> str:
    # Single-pass regex that matches C++/QML raw strings, normal strings,
    # char literals, single-line comments, and block comments in order.
    token_re = re.compile(
        r'R"([A-Za-z0-9_]*)\(.*?\)\1"'
        r'|"(?:\\.|[^"\\])*"'
        r"|'(?:\\.|[^'\\])*'"
        r"|//[^\n]*"
        r"|/\*.*?\*/",
        re.DOTALL,
    )
    return token_re.sub("", code)


def main() -> int:
    failures = 0

    def check(label: str, ok: bool, detail: str = "") -> None:
        nonlocal failures
        status = "PASS" if ok else "FAIL"
        print(f"[{status}] {label}" + (f" -- {detail}" if detail else ""))
        if not ok:
            failures += 1

    # 1. Run translation_logic_check.py
    res = subprocess.run(
        [sys.executable, str(VN_DIR / "tests" / "translation_logic_check.py")],
        capture_output=True,
        text=True,
    )
    check(
        "translation_logic_check.py",
        res.returncode == 0,
        res.stdout.strip().splitlines()[-1] if res.stdout else res.stderr,
    )

    # 2. Check locales/en.json against opennow-qt/qml and vn-translation/qml
    en_path = REPO / "locales" / "en.json"
    en_data = json.loads(en_path.read_text(encoding="utf-8"))

    def flatten(v):
        if isinstance(v, str):
            return [v]
        if isinstance(v, dict):
            return [s for c in v.values() for s in flatten(c)]
        return []

    source_texts = set(flatten(en_data))
    pat = re.compile(r'\b(?:qsTr|I18n\.source)\(\s*("(?:\\.|[^"\\])*")')
    used_main = set()
    for f in (REPO / "opennow-qt" / "qml").rglob("*.qml"):
        for m in pat.finditer(f.read_text(encoding="utf-8")):
            s = json.loads(m.group(1))
            if s:
                used_main.add(s)
    missing_main = sorted(used_main - source_texts)
    check(
        "locales/en.json covers all opennow-qt/qml qsTr strings",
        len(missing_main) == 0,
        f"{len(used_main)} checked",
    )

    used_vn = set()
    for f in (VN_DIR / "qml").rglob("*.qml"):
        for m in pat.finditer(f.read_text(encoding="utf-8")):
            s = json.loads(m.group(1))
            if s:
                used_vn.add(s)
    missing_vn = sorted(used_vn - source_texts)
    check(
        "locales/en.json covers all vn-translation/qml qsTr strings",
        len(missing_vn) == 0,
        f"{len(used_vn)} checked",
    )

    # 3. Brace and parenthesis balance across all C++ and QML files in vn-translation
    for ext in ("*.cpp", "*.h", "*.qml"):
        for f in sorted(VN_DIR.rglob(ext)):
            clean = strip_comments_and_strings(f.read_text(encoding="utf-8"))
            b_open, b_close = clean.count("{"), clean.count("}")
            p_open, p_close = clean.count("("), clean.count(")")
            check(
                f"Syntax balance: {f.relative_to(REPO)}",
                b_open == b_close and p_open == p_close,
                f"{{={b_open}/{b_close} (={p_open}/{p_close}",
            )

    # 4. Qt moc slot check on tst_translation.cpp
    tst_txt = (VN_DIR / "tests" / "tst_translation.cpp").read_text(encoding="utf-8")
    bare_slots = re.findall(r"^\s*void\s+\w+\(\)\s*$\n\s*void\s+\w+\(\)", tst_txt, flags=re.M)
    slot_names = re.findall(r"^\s*void\s+(\w+)\(\)", tst_txt, flags=re.M)
    check(
        "tst_translation.cpp moc slots valid & unique",
        len(bare_slots) == 0 and len(slot_names) == len(set(slot_names)),
        f"{len(slot_names)} slots",
    )

    # 5. DesktopSettingsScreen.qml navigation placement (Translation immediately below Recording)
    settings_qml = (
        REPO / "opennow-qt" / "qml" / "desktop" / "settings" / "DesktopSettingsScreen.qml"
    ).read_text(encoding="utf-8")
    rec_pos = settings_qml.find('label: qsTr("Recording")')
    tr_pos = settings_qml.find('label: qsTr("Translation")')
    app_pos = settings_qml.find('label: qsTr("Appearance")')
    check(
        "Translation placed directly under Recording in DesktopSettingsScreen.qml",
        0 < rec_pos < tr_pos < app_pos,
    )

    # 6. Zero ShaderEffectSource / MultiEffect on streamVideo in TranslationOverlay.qml code
    overlay_code = strip_comments_and_strings((VN_DIR / "qml" / "TranslationOverlay.qml").read_text(encoding="utf-8"))
    check(
        "Zero ShaderEffectSource/MultiEffect FBO copy in TranslationOverlay.qml",
        "ShaderEffectSource" not in overlay_code and "MultiEffect" not in overlay_code,
    )

    # 7. Zero fake gemini-3.5-flash-lite model name anywhere in vn-translation
    bad_model_files = [
        str(f.relative_to(REPO))
        for f in VN_DIR.rglob("*.cpp")
        if "gemini-3.5-flash-lite" in f.read_text(encoding="utf-8")
    ]
    check("No non-existent gemini-3.5-flash-lite model name", len(bad_model_files) == 0)

    print(f"\nTotal failures: {failures}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
