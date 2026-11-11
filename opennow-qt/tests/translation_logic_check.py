#!/usr/bin/env python3
"""Standalone logic check for the translation module (no Qt needed).

Re-implements, in Python, the three pure-logic pieces the C++ unit tests
cover, and asserts the same cases:
  1. normalize_key()  -- TranslationCache::normalizeKey (trim, collapse
     whitespace, lower-case).
  2. crop_hash()      -- TranslationEngine::cropHash (SHA-256 over ARGB32
     pixel bytes; empty for a null/empty image).
  3. TranslationJobQueue -- 1 running + 1 waiting; a new submit replaces
     the waiting job and counts it as dropped-stale.

Run:  python3 opennow-qt/tests/translation_logic_check.py
Exit code 0 = all checks pass.
"""

import hashlib
import re
import sys


def normalize_key(text: str) -> str:
    # Mirrors QString::simplified().toLower(): trim + collapse every
    # whitespace run to one space + lower-case.
    return re.sub(r"\s+", " ", text.strip()).lower()


def crop_hash(argb32_bytes: bytes) -> str:
    # Mirrors TranslationEngine::cropHash: SHA-256 hex of raw ARGB32 bytes.
    if not argb32_bytes:
        return ""
    return hashlib.sha256(argb32_bytes).hexdigest()


def crop_image(pixels, width, height, rect):
    """Crop a flat ARGB32 pixel list by a normalized (x, y, w, h) rect."""
    x = int(rect[0] * width)
    y = int(rect[1] * height)
    w = int(rect[2] * width)
    h = int(rect[3] * height)
    out = []
    for row in range(y, min(y + h, height)):
        out.extend(pixels[row * width + x: row * width + min(x + w, width)])
    return out


class TranslationJobQueue:
    """Mirrors TranslationJobQueue: 1 running + 1 waiting, newest wins."""

    def __init__(self):
        self.running = None
        self.waiting = None
        self.dropped_stale = 0

    def submit(self, job):
        if self.running is None:
            self.running = job
            return True
        if self.waiting is not None:
            self.dropped_stale += 1
        self.waiting = job
        return True

    def take_running(self):
        if self.running is not None:
            return self.running
        if self.waiting is None:
            return None
        self.running, self.waiting = self.waiting, None
        return self.running

    def finish_running(self):
        self.running = None


def check(name, cond):
    print(f"{'PASS' if cond else 'FAIL'}  {name}")
    if not cond:
        check.failures += 1


check.failures = 0


def main():
    # 1. Normalization: trim, collapse whitespace, case-insensitive key.
    check("normalize trims + collapses + lowercases",
          normalize_key("  Hello   World  ") == "hello world")
    check("normalize case-insensitive",
          normalize_key("HELLO   WORLD") == normalize_key("hello world"))
    check("normalize tabs/newlines collapse",
          normalize_key("\nhello\tworld\n") == "hello world")
    check("normalize '  A   B ' -> 'a b'", normalize_key("  A   B ") == "a b")

    # Cache hit/miss on normalized keys (mirrors tst_translation cache tests).
    cache = {}
    cache[normalize_key("  Hello   World  ")] = "xin chào thế giới"
    check("cache hit on normalized key",
          cache.get(normalize_key("hello world")) == "xin chào thế giới")
    check("cache miss on unknown key", cache.get(normalize_key("goodbye")) is None)

    # 2. Crop hash: identical pixels -> same hash; one pixel change -> differs.
    white = b"\xff\xff\xff\xff" * (64 * 32)
    changed = bytearray(white)
    changed[10 * 64 * 4 + 10 * 4] = 0  # one pixel to black-ish
    check("hash non-empty for real pixels", crop_hash(white) != "")
    check("hash same for identical pixels",
          crop_hash(white) == crop_hash(b"\xff\xff\xff\xff" * (64 * 32)))
    check("hash changes when one pixel changes",
          crop_hash(white) != crop_hash(bytes(changed)))
    check("hash empty for empty image", crop_hash(b"") == "")

    # Crop geometry mirrors the default dialogue rect (0.05, 0.72, 0.90, 0.24).
    frame = [0] * (200 * 100)
    crop = crop_image(frame, 200, 100, (0.05, 0.72, 0.90, 0.24))
    check("default dialogue crop is 180x24", len(crop) == 180 * 24)

    # 3. Bounded queue drops the stale waiting job.
    q = TranslationJobQueue()
    assert q.submit("first") and q.running == "first" and q.waiting is None
    assert q.submit("second") and q.waiting == "second"
    check("no stale drop yet", q.dropped_stale == 0)
    assert q.submit("third")
    check("third submit drops one stale job", q.dropped_stale == 1)
    check("running is still 'first'", q.take_running() == "first")
    q.finish_running()
    check("waiting job is 'third' (second was dropped)",
          q.take_running() == "third")
    q.finish_running()
    check("queue drains to empty", q.take_running() is None)

    print(f"\n{check.failures} failure(s)")
    return 1 if check.failures else 0


if __name__ == "__main__":
    sys.exit(main())
