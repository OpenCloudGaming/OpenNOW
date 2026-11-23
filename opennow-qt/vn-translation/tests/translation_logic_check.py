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
  4. WuWaDialogueDb::lookupFuzzy -- exact, then squashed-form exact, then
     Levenshtein ratio >= 0.88 among entries sharing the first two words.
     similarityRatio() is the Levenshtein-based ratio.

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


def dialogue_rect_for_preset(preset):
    # Mirrors TranslationEngine::dialogueRectForPreset: normalized (x, y, w, h),
    # or None for an unknown preset.
    if preset == "wuthering-waves":
        return (0.05, 0.72, 0.90, 0.24)
    if preset == "generic":
        return (0.05, 0.60, 0.90, 0.30)
    if preset == "full-screen":
        return (0, 0, 1, 1)
    return None


def migrate_dialogue_preset(saved):
    # Mirrors TranslationEngine::migrateDialoguePreset (v4.1): the old
    # shipped default "wuthering-waves" becomes "full-screen"; "generic"
    # and "full-screen" pass through; empty/unknown become "full-screen".
    if saved in ("generic", "full-screen"):
        return saved
    return "full-screen"


def is_chunk_download_complete(db_size, expected_strings):
    # Mirrors TranslationEngine::isChunkDownloadComplete (v5): complete
    # iff the manifest promises at least one string and the DB holds at
    # least that many. expected == 0 means unknown, never complete.
    return expected_strings > 0 and db_size >= expected_strings


def map_block_rect_to_frame(block, source):
    # Mirrors mapBlockRectToFrame: an OCR block's rect (0..1 within the
    # crop) into full-frame coords through the crop's source rect, clamped.
    def clamp(v, lo, hi):
        return max(lo, min(v, hi))
    x = clamp(source[0] + block[0] * source[2], 0.0, 1.0)
    y = clamp(source[1] + block[1] * source[3], 0.0, 1.0)
    w = clamp(block[2] * source[2], 0.0, 1.0 - x)
    h = clamp(block[3] * source[3], 0.0, 1.0 - y)
    return (x, y, w, h)


def blocks_to_variant_list(blocks):
    # Mirrors TranslationEngine::blocksToVariantList: one dict per block.
    return [{"original": b[0], "translated": b[1],
             "x": b[2][0], "y": b[2][1], "w": b[2][2], "h": b[2][3]}
            for b in blocks]


# 4. Custom OpenAI-compatible API (v3): base normalization, /models
# parsing, SSE stream parsing, chat-completion parsing + payload shape.
import json


def normalize_custom_api_base(base):
    # Mirrors normalizeCustomApiBase: trim, strip trailing "/", strip a
    # trailing "/chat/completions".
    normalized = base.strip()
    while normalized.endswith("/"):
        normalized = normalized[:-1]
    if normalized.endswith("/chat/completions"):
        normalized = normalized[: -len("/chat/completions")]
    while normalized.endswith("/"):
        normalized = normalized[:-1]
    return normalized


def custom_api_models_url(base):
    normalized = normalize_custom_api_base(base)
    return normalized + "/models" if normalized else ""


def custom_api_chat_url(base):
    normalized = normalize_custom_api_base(base)
    return normalized + "/chat/completions" if normalized else ""


def parse_custom_api_models(body):
    # Mirrors parseCustomApiModels: {"data": [{"id": ...}]} in server
    # order, duplicates removed; (models, error).
    try:
        doc = json.loads(body)
    except (ValueError, TypeError):
        return [], "Invalid response from API"
    data = doc.get("data") if isinstance(doc, dict) else None
    if not data:
        return [], "API returned no models"
    models = []
    for item in data:
        model_id = (item.get("id") or "").strip() if isinstance(item, dict) else ""
        if model_id and model_id not in models:
            models.append(model_id)
    if not models:
        return [], "API returned no models"
    return models, ""


def parse_sse_stream_text(sse_bytes):
    # Mirrors parseSseStreamText: "data: {...}" lines, delta.content
    # concatenated, "[DONE]" and garbage lines skipped.
    text = ""
    for raw_line in sse_bytes.split("\n"):
        line = raw_line.strip()
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if not payload or payload == "[DONE]":
            continue
        try:
            doc = json.loads(payload)
        except ValueError:
            continue
        choices = doc.get("choices") if isinstance(doc, dict) else None
        if not choices:
            continue
        choice = choices[0]
        delta = (choice.get("delta") or {}).get("content") or ""
        if not delta:
            delta = (choice.get("message") or {}).get("content") or ""
        text += delta
    return text


def parse_chat_completion_text(body):
    # Mirrors parseChatCompletionText: choices[0].message.content (or
    # legacy choices[0].text); (text, error).
    try:
        doc = json.loads(body)
    except (ValueError, TypeError):
        return "", "Invalid response from API"
    choices = doc.get("choices") if isinstance(doc, dict) else None
    if not choices:
        return "", "API returned no result"
    choice = choices[0]
    text = ((choice.get("message") or {}).get("content") or "").strip()
    if not text:
        text = (choice.get("text") or "").strip()
    if not text:
        return "", "API returned empty text"
    return text, ""


def build_chat_completion_payload(model, system_prompt, user_text, image_base64, stream):
    # Mirrors buildChatCompletionPayload.
    messages = [{"role": "system", "content": system_prompt}]
    if not image_base64:
        messages.append({"role": "user", "content": user_text})
    else:
        messages.append({"role": "user", "content": [
            {"type": "text", "text": user_text},
            {"type": "image_url",
             "image_url": {"url": "data:image/jpeg;base64," + image_base64}}]})
    body = {"model": model, "messages": messages}
    if stream:
        body["stream"] = True
    return body


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


def squash_key(normalized):
    # Mirrors squashKey(): every non-alphanumeric stripped.
    return "".join(c for c in normalized if c.isalnum())


def levenshtein(a, b):
    # Two-row Levenshtein, as in WuWaDialogueDb::similarityRatio.
    n, m = len(a), len(b)
    prev = list(range(m + 1))
    cur = [0] * (m + 1)
    for i in range(1, n + 1):
        cur[0] = i
        for j in range(1, m + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
        prev, cur = cur, prev
    return prev[m]


def similarity_ratio(a, b):
    n, m = len(a), len(b)
    if n == 0 and m == 0:
        return 1.0
    if n == 0 or m == 0:
        return 0.0
    return 1.0 - levenshtein(a, b) / max(n, m)


def first_two_words(key):
    # Mirrors WuWaDialogueDb::firstTwoWords: up to the second space.
    parts = key.split(" ", 2)
    return " ".join(parts[:2]) if len(parts) >= 2 else key


def lookup_fuzzy(entries, text):
    # Mirrors WuWaDialogueDb::lookupFuzzy over a {normalized key: vi} dict.
    key = normalize_key(text)
    if not key:
        return None
    if key in entries:
        return entries[key]
    sq = squash_key(key)
    if sq:
        for k, v in entries.items():
            if squash_key(k) == sq:
                return v
    best, best_ratio = None, 0.0
    for k, v in entries.items():
        if first_two_words(k) != first_two_words(key):
            continue
        r = similarity_ratio(key, k)
        if r > best_ratio:
            best_ratio, best = r, v
    return best if best_ratio >= 0.88 else None


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

    # Dialogue presets mirror TranslationEngine::dialogueRectForPreset.
    check("preset wuthering-waves rect",
          dialogue_rect_for_preset("wuthering-waves") == (0.05, 0.72, 0.90, 0.24))
    check("preset generic rect",
          dialogue_rect_for_preset("generic") == (0.05, 0.60, 0.90, 0.30))
    check("preset full-screen covers whole frame",
          dialogue_rect_for_preset("full-screen") == (0, 0, 1, 1))
    check("preset unknown -> None",
          dialogue_rect_for_preset("bogus") is None)

    # v4.1 preset migration mirrors TranslationEngine::migrateDialoguePreset.
    check("migrate old default wuthering-waves -> full-screen",
          migrate_dialogue_preset("wuthering-waves") == "full-screen")
    check("migrate generic stays generic",
          migrate_dialogue_preset("generic") == "generic")
    check("migrate full-screen stays full-screen",
          migrate_dialogue_preset("full-screen") == "full-screen")
    check("migrate empty -> full-screen",
          migrate_dialogue_preset("") == "full-screen")
    check("migrate unknown -> full-screen",
          migrate_dialogue_preset("bogus") == "full-screen")

    # v5 chunk completeness mirrors TranslationEngine::isChunkDownloadComplete.
    check("chunk complete: empty/unknown -> False",
          is_chunk_download_complete(0, 0) is False)
    check("chunk complete: empty/expected -> False",
          is_chunk_download_complete(0, 220764) is False)
    check("chunk complete: one short -> False",
          is_chunk_download_complete(220763, 220764) is False)
    check("chunk complete: exact -> True",
          is_chunk_download_complete(220764, 220764) is True)
    check("chunk complete: over -> True",
          is_chunk_download_complete(220765, 220764) is True)
    check("chunk complete: unknown expected -> False",
          is_chunk_download_complete(5, 0) is False)

    # 2b. Block rect mapping mirrors mapBlockRectToFrame (v2 per-line overlay).
    def near(a, b):
        return all(abs(x - y) < 1e-9 for x, y in zip(a, b))
    check("block in dialogue crop maps into frame",
          near(map_block_rect_to_frame((0.1, 0.2, 0.5, 0.3), (0.05, 0.72, 0.90, 0.24)),
               (0.14, 0.768, 0.45, 0.072)))
    check("block in full-screen crop is already frame coords",
          near(map_block_rect_to_frame((0.2, 0.1, 0.4, 0.05), (0, 0, 1, 1)),
               (0.2, 0.1, 0.4, 0.05)))
    check("block past the frame edge is clamped",
          near(map_block_rect_to_frame((0.9, 0.9, 0.5, 0.5), (0, 0, 1, 1)),
               (0.9, 0.9, 0.1, 0.1)))
    variant = blocks_to_variant_list(
        [("Hello", "Xin chào", (0.1, 0.2, 0.5, 0.1)),
         ("World", "Thế giới", (0.1, 0.35, 0.4, 0.1))])
    check("variant list keeps both blocks in order",
          len(variant) == 2 and variant[0]["translated"] == "Xin chào"
          and variant[1]["x"] == 0.1 and variant[1]["h"] == 0.1)

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

    # 4. Custom API (v3): base normalization, model list, SSE, payload.
    check("base normalization keeps a clean base",
          normalize_custom_api_base("https://api.openai.com/v1") == "https://api.openai.com/v1")
    check("base normalization strips trailing slash",
          normalize_custom_api_base("https://api.openai.com/v1/") == "https://api.openai.com/v1")
    check("base normalization strips /chat/completions",
          normalize_custom_api_base("https://x.ai/v1/chat/completions") == "https://x.ai/v1")
    check("base normalization trims whitespace",
          normalize_custom_api_base("  https://x.ai/v1/  ") == "https://x.ai/v1")
    check("base normalization empty -> empty",
          normalize_custom_api_base("") == "")
    check("models url appends /models",
          custom_api_models_url("https://x.ai/v1/") == "https://x.ai/v1/models")
    check("chat url appends /chat/completions",
          custom_api_chat_url("https://x.ai/v1/chat/completions") == "https://x.ai/v1/chat/completions")
    check("models url empty base -> empty",
          custom_api_models_url("") == "")
    models, err = parse_custom_api_models(
        '{"object":"list","data":[{"id":"gpt-4o-mini"},{"id":"llama-3.3-70b"},{"id":"gpt-4o-mini"}]}')
    check("models parsed in order, duplicates removed",
          models == ["gpt-4o-mini", "llama-3.3-70b"] and err == "")
    models, err = parse_custom_api_models("not json")
    check("models bad json -> empty + error", models == [] and err != "")
    models, err = parse_custom_api_models('{"data":[]}')
    check("models empty data -> empty + error", models == [] and err != "")
    sse = ('data: {"choices":[{"delta":{"content":"Xin"}}]}\n\n'
           'data: {"choices":[{"delta":{"content":" chào"}}]}\n\n'
           'data: [DONE]\n\n')
    check("SSE deltas concatenate", parse_sse_stream_text(sse) == "Xin chào")
    check("SSE partial buffer yields only the complete event",
          parse_sse_stream_text('data: {"choices":[{"delta":{"content":"Xin"}}]}\n\ndata: {"cho') == "Xin")
    check("SSE garbage lines skipped",
          parse_sse_stream_text(": ping\n\ndata: [DONE]\n\n") == "")
    check("SSE full-message event used as-is",
          parse_sse_stream_text('data: {"choices":[{"message":{"content":"Rover"}}]}\n\n') == "Rover")
    text, err = parse_chat_completion_text(
        '{"choices":[{"message":{"content":"  Xin chào Rover  "}}]}')
    check("chat completion content trimmed", text == "Xin chào Rover" and err == "")
    text, err = parse_chat_completion_text('{"choices":[{"text":"Chào"}]}')
    check("chat completion legacy text shape", text == "Chào")
    text, err = parse_chat_completion_text('{"choices":[]}')
    check("chat completion no choices -> error", text == "" and err != "")
    payload = build_chat_completion_payload("gpt-4o-mini", "sys", "Hello", "", True)
    check("payload text call shape",
          payload["model"] == "gpt-4o-mini" and payload["stream"] is True
          and payload["messages"][0]["role"] == "system"
          and payload["messages"][1]["content"] == "Hello")
    payload = build_chat_completion_payload("gpt-4o", "sys", "Read.", "aGVsbG8=", False)
    content = payload["messages"][1]["content"]
    check("payload vision call has image_url part",
          "stream" not in payload and len(content) == 2
          and content[1]["type"] == "image_url"
          and content[1]["image_url"]["url"].startswith("data:image/jpeg;base64,"))

    # 4. Fuzzy DB lookup (mirrors WuWaDialogueDb::lookupFuzzy + tst_translation).
    db = {normalize_key("Talk to Shilang"): "Nói chuyện với Shilang",
          normalize_key("The Rover walks into the city"): "Rover bước vào thành phố",
          normalize_key("Leave the room and take a look outside"): "Rời khỏi phòng và nhìn ra ngoài"}
    check("fuzzy exact", lookup_fuzzy(db, "Talk to Shilang") == "Nói chuyện với Shilang")
    check("fuzzy punctuation noise", lookup_fuzzy(db, "Talk to Shilang!") == "Nói chuyện với Shilang")
    check("fuzzy one-char typo", lookup_fuzzy(db, "Talk to Shilanj") == "Nói chuyện với Shilang")
    check("fuzzy different line sharing first words misses",
          lookup_fuzzy(db, "Talk to the vendor") is None)
    check("fuzzy empty query misses", lookup_fuzzy(db, "   ") is None)
    check("similarity identical is 1.0", similarity_ratio("abc", "abc") == 1.0)
    check("similarity one sub is 2/3", abs(similarity_ratio("abc", "abd") - 2 / 3) < 0.001)
    check("similarity empty/empty is 1.0", similarity_ratio("", "") == 1.0)
    check("similarity one empty is 0.0", similarity_ratio("a", "") == 0.0)

    # 5. RichText stripping, HUD noise filter, and dynamic template matching.
    def strip_rich_text(s):
        return re.sub(r"</?(?:color|size|b|i|u|material|quad)(?:=[^>]*)?>", "", s, flags=re.I)

    def is_hud_noise(s):
        c = strip_rich_text(s).strip()
        if not c or len(c) == 1:
            return True
        if re.match(r"^[\d\s.,:%+\-/()x×*]+$", c):
            return True
        if re.match(r"^(?:uid\s*:?\s*\d+|lv\.?\s*\d+|\d+\s*(?:fps|ms|ping|mb/s|kb/s)|esc|tab|shift|ctrl|alt|space)$", c, flags=re.I):
            return True
        return False

    check("strip RichText tags removes <color> and <size>",
          strip_rich_text("<color=#ffd12f>Base HP</color>: <size=32>100</size>") == "Base HP: 100")
    check("HUD noise filters FPS, ms, UID, LV, numbers, single hotkeys",
          all(is_hud_noise(x) for x in ["60 FPS", "28ms", "UID: 900123", "LV.90", "18450/18450", "E", "Q"])
          and not is_hud_noise("Talk to Shilang"))

    print(f"\n{check.failures} failure(s)")
    return 1 if check.failures else 0


if __name__ == "__main__":
    sys.exit(main())
