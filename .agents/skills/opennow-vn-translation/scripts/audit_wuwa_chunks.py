#!/usr/bin/env python3
"""
ECC Audit & Auto-Repair Script for wuwa-vi-chunks (OpenNOW-ORC).
Uses 2-state Viterbi sequence alignment to detect and repair off-by-one shifted
slices in chunk JSON files where value[i] is actually the translation of key[i+1].
"""
import json
import re
import sys
from pathlib import Path

GLOSSARY = {
    "talk": "nói chuyện",
    "read": "đọc",
    "take photos": "chụp ảnh",
    "take a photo": "chụp ảnh",
    "investigate": "điều tra",
    "inspect": "kiểm tra",
    "activate": "kích hoạt",
    "enter": "vào",
    "open": "mở",
    "interact": "tương tác",
    "leave": "rời",
    "gold": "vàng",
    "silver": "bạc",
    "bronze": "đồng",
    "challenge failed": "thử thách thất bại",
    "challenge complete": "hoàn thành thử thách",
}

COMMON_WORDS = {
    "Take", "Talk", "Find", "Reach", "Clear", "Use", "Complete", "Defeat",
    "Traces", "Scale", "About", "Dark", "Continue", "Descend", "Adjust",
    "Launch", "Gold", "Silver", "Bronze", "Score", "Rank", "Current",
    "Challenge", "Limited", "Capture", "Light", "Search", "Fill",
    "Transform", "Inspect", "Observe", "Start", "Leave", "Reach",
}


def score_pair(en: str, vi: str) -> float:
    if not en or not vi:
        return -5.0
    s = 0.0
    en_l = en.strip().lower()
    vi_l = vi.strip().lower()
    for gk, gv in GLOSSARY.items():
        if en_l == gk:
            s += 8.0 if (vi_l == gv or vi_l.startswith(gv)) else -8.0
        elif en_l.startswith(gk + " ") or en_l.startswith(gk + ":"):
            s += 3.5 if gv in vi_l else -2.5

    ph_en = set(re.findall(r"\{[^}]+\}", en))
    ph_vi = set(re.findall(r"\{[^}]+\}", vi))
    if ph_en or ph_vi:
        s += 6.0 if ph_en == ph_vi else -7.0

    num_en = set(re.findall(r"\b\d+\b", re.sub(r"<[^>]+>", "", en)))
    num_vi = set(re.findall(r"\b\d+\b", re.sub(r"<[^>]+>", "", vi)))
    if num_en or num_vi:
        s += 4.0 if num_en == num_vi else -4.0

    nouns_en = set(re.findall(r"\b[A-Z][a-z]{3,}\b", en)) - COMMON_WORDS
    for n in nouns_en:
        if n in vi:
            s += 2.5

    ratio = len(vi) / max(1, len(en))
    if 0.6 <= ratio <= 2.2:
        s += 1.5
    elif ratio > 3.0 or ratio < 0.33:
        s -= 5.0
    return s


def repair_entries(entries: list[tuple[str, str]]) -> tuple[dict[str, str], int]:
    n = len(entries) - 1
    if n <= 0:
        return dict(entries), 0
    penalty = 3.5
    dp0 = [0.0] * n
    dp1 = [0.0] * n
    back0 = [0] * n
    back1 = [0] * n
    dp0[0] = score_pair(entries[0][0], entries[0][1])
    dp1[0] = score_pair(entries[1][0], entries[0][1]) - penalty

    for i in range(1, n):
        s0 = score_pair(entries[i][0], entries[i][1])
        s1 = score_pair(entries[i + 1][0], entries[i][1])
        if dp0[i - 1] >= dp1[i - 1] - penalty:
            dp0[i] = dp0[i - 1] + s0
            back0[i] = 0
        else:
            dp0[i] = dp1[i - 1] - penalty + s0
            back0[i] = 1
        if dp1[i - 1] >= dp0[i - 1] - penalty:
            dp1[i] = dp1[i - 1] + s1
            back1[i] = 1
        else:
            dp1[i] = dp0[i - 1] - penalty + s1
            back1[i] = 0

    state = [0] * n
    state[-1] = 1 if dp1[-1] > dp0[-1] else 0
    for i in range(n - 2, -1, -1):
        state[i] = back1[i + 1] if state[i + 1] == 1 else back0[i + 1]

    repaired: dict[str, str] = {}
    for i in range(n):
        if state[i] == 0:
            repaired[entries[i][0]] = entries[i][1]
        else:
            repaired[entries[i + 1][0]] = entries[i][1]
    if state[-1] == 0:
        repaired[entries[-1][0]] = entries[-1][1]

    # Ensure common single-word UI verbs are always present and accurate
    FALLBACK_UI = {
        "Talk": "Nói chuyện",
        "Read": "Đọc",
        "Take photos": "Chụp ảnh",
        "Take a photo": "Chụp ảnh",
        "Investigate": "Điều tra",
        "Inspect": "Kiểm tra",
        "Activate": "Kích hoạt",
        "Enter": "Vào",
        "Open": "Mở",
        "Interact": "Tương tác",
        "Leave": "Rời đi",
    }
    for k, v in FALLBACK_UI.items():
        if k in dict(entries) and (k not in repaired or len(repaired[k]) > len(k) * 3):
            repaired[k] = v

    return repaired, sum(state)


def audit_chunk(path: Path, write_fix: bool = False) -> None:
    raw_text = path.read_text(encoding="utf-8")
    raw = json.loads(raw_text)
    # Preserve file order using object_pairs_hook
    pairs = json.loads(raw_text, object_pairs_hook=list)
    entries = []
    for k, v in pairs:
        if k == "entries" and isinstance(v, list):
            entries = v
            break
    repaired, shifted_count = repair_entries(entries)
    print(f"File: {path.name} | Total: {len(entries)} | Repaired shifted items: {shifted_count}")
    for sample_key in [
        "Traces of Mt. Firmament VI",
        "Take a photo of the \"skyglow\" and the \"suspended rocks\"",
        "Talk to Uncle Fu",
        "Take photos",
        "Talk",
        "Read",
        "Use the Chronosorter in the stone room to raise the flagstone",
        "Challenge Failed",
        "Current Score: {q_count}",
        "Gold: 3000",
        "Silver: 2000",
        "Bronze: 1000",
    ]:
        if sample_key in repaired:
            print(f"  FIXED: {sample_key!r} => {repaired[sample_key]!r}")
    if write_fix and shifted_count > 0:
        out_obj = {"version": raw.get("version", 1), "entries": repaired}
        path.write_text(json.dumps(out_obj, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    fix = "--fix" in sys.argv
    for arg in sys.argv[1:]:
        if arg != "--fix":
            audit_chunk(Path(arg), write_fix=fix)
