# OpenNOW-VN — progress log (living document)

Branch: `vn-translation` · Base: upstream v1.0.4, commit `bec4a0b` ·
Fork: `duyzoz/OpenNOW-ORC` (see `UPSTREAM.md`).

Update this file when something is built, decided, found, or proven — with
a date. Do not rewrite history entries; append.

## Goal

In-client EN→VI screen-OCR translation for Wuthering Waves played through
GeForce NOW, inside the Qt OpenNOW desktop app. The translation is an
overlay above the streamed video (`StreamVideoItem`): capture the frame,
OCR the English text, translate it, and draw Vietnamese in place — without
touching the Rust streamer, the NVST transport, or the FFI ABI, and without
changing any upstream behaviour except through the 3 shim files
(`UPSTREAM.md`).

## Base

- Upstream `OpenCloudGaming/OpenNOW` tag **v1.0.4**, commit `bec4a0b`.
- Remotes: `origin` = `duyzoz/OpenNOW-ORC`, `upstream` =
  `OpenCloudGaming/OpenNOW`.
- Old Electron repo `duyzoz/OpenNow-VN` is **abandoned** — never merged,
  never ported from.

## What is built

### 2026-11-10 (docs + verification pass, this commit)

- Docs only. Created `docs/vn-translation/`:
  - `UPDATER.md` — upstream updater findings (Rust core, hardcoded
    `OpenCloudGaming/OpenNOW` URLs, Ed25519 fail-closed) + recommendation:
    ship VN builds with no pinned key + QML-level auto-check guard.
  - `CI.md` — `vn-translation` push does **not** trigger CI; CI runs via PR
    to `dev`/`main`; Qt 6.8.3; how new C++/QML files get registered.
  - `UPSTREAM.md` — fork base, remotes, merge rules, abandoned-repo rule.
  - `PROGRESS.md` — this file.
- `README.md` — added the "OpenNOW-VN — Translation (`vn-translation`
  branch)" section near the top (after the upstream intro, before
  "Get OpenNOW"). No upstream README content deleted.

### Translation implementation — LANDED while this review was in progress

Other subagents created the following during the docs pass (first seen
2026-11-10 ~23:58, re-checked before commit). All paths verified present:

**C++** (`opennow-qt/src/translation/`, 14 files):

- `TranslationEngine.h` / `TranslationEngine.cpp` (407 lines) — QML-facing
  controller, `translationEngine` context property (planned), stability
  timer, crop-hash cache, worker dispatch.
- `OcrEngine.h` / `OcrEngine.cpp` — Gemini Vision (default), Tesseract,
  RapidOCR stub, Windows OCR stub; `createOcrEngine()`.
- `TranslationProvider.h` / `TranslationProvider.cpp` — Google Translate
  (default) + Gemini translate; `createTranslationProvider()`.
- `TranslationHttp.h` / `TranslationHttp.cpp` — sync HTTP with 8 s timeout,
  single 429 retry honoring `Retry-After` (capped 10 s).
- `TranslationWorker.h` / `TranslationWorker.cpp` — own QThread, bounded
  queue (1 running + 1 waiting, newest waiting wins).
- `TranslationCache.h` / `TranslationCache.cpp` — normalized EN→VI cache,
  persisted as `translation-cache.json` in AppData.
- `WuWaDialogueDb.h` / `WuWaDialogueDb.cpp` — pre-translated dialogue DB.
- `data/wuwa-dialogue-db.json` — seed DB, **intentionally empty**
  (`entries: {}`, valid JSON, verified with `python3 -m json.tool`).

**QML** (both registered in `opennow-qt/cmake/QmlModule.cmake`):

- `opennow-qt/qml/desktop/stream/TranslationOverlay.qml` (163 lines) —
  instantiated in `DesktopStreamScreen.qml:171` (`z: 6`, above video),
  `parent.streaming` resolves (`DesktopStreamScreen.qml:34`).
- `opennow-qt/qml/desktop/settings/pages/DesktopSettingsTranslationPage.qml`
  (331 lines) — wired into `DesktopSettingsScreen.qml` as page 14 with a
  "Translation" nav entry; settings under `translation.<key>`.

**Tests + settings + wiring (landed in the working tree, uncommitted, by
other subagents — verified in diffs, not by this reviewer):**

- `opennow-qt/tests/tst_translation.cpp` — crop hash, crop rect, cache,
  DB, worker queue; no network. Registered in `cmake/Tests.cmake` as
  `opennow-translation-tests` (offscreen, 30 s timeout).
- `opennow-qt/cmake/Sources.cmake` — new `OPENNOW_TRANSLATION_SOURCES`
  set with all 14 translation files, added to `qt_add_executable`.
- `opennow-qt/src/app/ApplicationStartup.cpp` — creates the engine (app
  lifetime), connects `screenshotReady` → `onStreamFrame` (queued), sets
  the `translationEngine` context property. **Open gap: settings are never
  synced into the engine — see R3b in Bugs found.**
- `locales/en.json` — Vietnamese qsTr literals added as values (auto keys
  like `dich_man_hinh`) **plus** a hand-written English `"translation"`
  section (see review finding R6). `npm run locales:check` passes locally
  (`node scripts/check-translations.mjs` → "Translations ok (2324 QML
  source texts and 0 keys checked)", exit 0).
- `opennow-qt/qml/state/settings/SettingsState.qml` — new
  `translationSetting(key, fallback)` helper (see R7: currently unused).

Expected (not yet verified) shape, per the fork plan:

- `opennow-qt/src/translation/` — OCR engine interface + Gemini Vision
  implementation, translation cache/DB, screenshot request path reusing
  `NativeStreamRuntime::requestScreenshot()` / `screenshotReady(QImage)`,
  worker thread for OCR (never the GUI thread).
- QML overlay above `StreamVideoItem` in
  `opennow-qt/qml/desktop/stream/DesktopStreamScreen.qml` (shim #2).
- Translation Settings page under `DesktopSettingsScreen.qml` (shim #3):
  source EN, target VI, engine, API key, overlay mode, dialogue area,
  stability delay, cache/DB.

## Decisions

Recorded 2026-11-10 (fork plan, before implementation):

- **OCR default in v1 is Gemini Vision** (cloud OCR via Gemini API).
- **RapidOCR is a stub, not a real engine, in v1** — ONNX Runtime is not
  vendored; bundling it would break CI (new native dependency, larger
  packages, platform matrix risk). A real RapidOCR / PP-OCRv4 ONNX engine
  is a later step if/when ONNX Runtime vendoring is solved.
- **OneOCR model is NOT bundled, ever, for licensing reasons** — its model
  comes from the Windows Snipping Tool; bundling it is a licensing trap.
- **0 ms translation latency is impossible.** OCR + translate always adds
  delay on a streamed (GFN) frame. Do not promise 0 ms anywhere.
- **Latency targets:** < 1 s for new lines (dialogue-crop + change
  detection + pre-translated WuWa DB cache first), < 200 ms for cache hits.
- **Source language is English.** WuWa must be set to English in-game;
  target is Vietnamese.
- **Feature is OFF by default.** Ethan enables it explicitly in
  Settings → Translation.
- **Upstream updates are preserved only via the upstream remote + additive
  module.** VN must use its own updater, NOT upstream signed Ed25519
  update manifests (see `UPDATER.md`). The updater repoint needs its own
  decision + VN keypair; it is not part of the translation work.
- **Never touch `native/`.** Updater, settings defaults, streamer, and FFI
  stay upstream-owned (see `UPSTREAM.md`).

## Bugs found

### Review pass 2026-11-10 ~23:58 (docs reviewer, read-only — did not fix code)

Reviewed every new file listed above (1,897 lines total). Findings, worst
first. **R1–R3 were fixed in the working tree while this review was
being written (see entries); R3b now blocks the feature from actually
running; R4–R8 are follow-ups.**

- **R1 [WAS BLOCKER — FIXED in working tree while reviewing]**
  At first check, `opennow-qt/cmake/Sources.cmake` contained zero
  `translation` entries, so all 7 new `.cpp` files would have been silently
  not compiled. Another subagent fixed it before this commit: `Sources.cmake`
  now defines `OPENNOW_TRANSLATION_SOURCES` (all 14 files) and adds it to
  `qt_add_executable` (verified in the working-tree diff). Keep the
  `CI.md` §3 rule in mind for any future files.
- **R2 [WAS BLOCKER — FIXED in working tree while reviewing]**
  At first check, `opennow-qt/tests/tst_translation.cpp` was not registered
  in `opennow-qt/cmake/Tests.cmake`. Another subagent fixed it: `Tests.cmake`
  now builds `opennow-translation-tests` from the test + all translation
  sources, linked against `Qt6::Test Qt6::Gui Qt6::Network`, offscreen,
  30 s timeout (verified in the working-tree diff).
- **R3 [WAS BLOCKER — FIXED in working tree while reviewing]**
  At first check, nothing outside `src/translation/` referenced
  `TranslationEngine`. Another subagent wired it in
  `opennow-qt/src/app/ApplicationStartup.cpp`: engine created at `:194`
  (app lifetime), `screenshotReady` → `onStreamFrame` connected with
  `Qt::QueuedConnection` at `:254-260` (existing `saveStreamScreenshot`
  connect untouched), context property `translationEngine` set at `:365`.
- **R3b [OPEN — BLOCKER for the feature actually working] Settings are
  never mirrored into the engine.** The settings page saves
  `translation.enabled`, `translation.ocrEngine`,
  `translation.translationProvider`, `translation.overlayMode`,
  `translation.stabilityDelayMs`, `translation.fontSizePx`,
  `translation.showOriginal`, `translation.cacheEnabled` via
  `settingsScreen.setSetting`, and its header comment claims "The C++
  translationEngine mirrors those settings into its live properties" —
  but no code does that. Grep finds no `setEnabled` / `setOcrEngine` /
  `setTranslationProvider` / `setOverlayMode` call from `ApplicationStartup.cpp`,
  `ShellStore.qml`, or `SettingsState.qml`. So `translationEngine.enabled`
  stays at its `false` default forever: the Settings toggle saves, but the
  engine never turns on, and `onStreamFrame` returns immediately. Fix:
  sync `translation.*` settings into the engine at startup and on change
  (e.g. in `ApplicationStartup.cpp` after settings load, or a QML binding
  in `ShellStore`/settings state that calls the engine setters whenever
  the settings change). Note the Gemini key is intentionally in-memory
  only — it must be re-entered every launch by design.
- **R4 [BUG] Discarded `chopped()` result breaks fenced Gemini replies.**
  `opennow-qt/src/translation/OcrEngine.cpp:60`: `raw.chopped(3);` —
  `QString::chopped()` returns a new string and does not modify in place,
  so the trailing ```` ``` ```` fence is never removed. Any Gemini OCR
  reply wrapped in a ```json fence then fails `QJsonDocument::fromJson`
  ("OCR service returned unparsable text"). Fix: `raw = raw.chopped(3);`.
  (The opening-fence strip above it correctly reassigns with `raw =`.)
- **R5 [THREAD SAFETY, minor] Unsynchronized write to `m_lastLookupUs`.**
  `opennow-qt/src/translation/TranslationCache.cpp` (`lookup()`):
  `m_lastLookupUs` is written **after** the `QMutexLocker` scope ends,
  while `lastLookupUs()` reads it under the lock. Make the write inside
  the locked scope (or make the member atomic).
- **R6 [LOCALIZATION, convention] Two `en.json` representations; source
  strings are Vietnamese.** The new QML pages use Vietnamese `qsTr`
  literals (e.g. `qsTr("Dịch màn hình")`) and `en.json` carries them as
  values under auto keys (`dich_man_hinh`, …) — `locales:check` passes,
  verified locally. But `locales/en.json` also gained a hand-written
  English `"translation": {...}` section (44 lines) that **no QML
  references** (pages use `qsTr` literals, never `I18n` keys). Consequences:
  English-locale users see Vietnamese on these pages, Crowdin cannot
  translate the Vietnamese source text, and the English section is dead
  weight. Decide: either keep Vietnamese-first deliberately (delete the
  unused English section) or switch QML to English source strings with
  the Vietnamese coming from Crowdin. Not a CI failure — a product/
  convention decision.
- **R7 [DEAD CODE, minor] `translationSetting()` has no caller.**
  `opennow-qt/qml/state/settings/SettingsState.qml:155` adds
  `translationSetting(key, fallback)`, but the settings page uses
  `settingsScreen.valueSetting("translation." + key, ...)` directly.
  Either use the helper from the page or remove it.
- **R8 [PACKAGING, minor] Dialogue-DB seed is never deployed.**
  `opennow-qt/src/translation/data/wuwa-dialogue-db.json` is not
  referenced in `opennow-qt/cmake/Resources.cmake` or packaging, and
  `resolveDialogueDbPath()` (`TranslationEngine.cpp:25-38`) only looks in
  AppData / next-to-exe / `data/` — none of which a packaged install
  populates. Harmless today (the seed is empty and `loadDialogueDb()`
  tolerates a missing file), but once real entries are added they will
  silently not ship. Fix when the DB gets its first entries: register the
  file as a deployed resource or copy it in packaging.

  Checked and **clean** (no finding):
  - **API key in logs:** none found. Key travels only in the
    `x-goog-api-key` header (`OcrEngine.cpp`, `TranslationProvider.cpp`),
    never in a URL; `TranslationEngine.h: m_geminiApiKey` is documented
    in-memory-only; settings page shows only "Đã lưu key"/"Chưa có key"
    and clears the field after save. `TranslationHttp` error paths use
    HTTP status / `errorString()`, never request headers.
  - **Null QImage:** guarded everywhere — `OcrEngine::recognize`
    (`OcrEngine.cpp`), `cropDialogueImage`/`cropHash`/`onStreamFrame`
    (`TranslationEngine.cpp`), worker empty-crop path. Crops are detached
    copies (`image.copy()`), safe across threads.
  - **Thread safety (worker):** `TranslationWorker` owns its `QThread`;
    jobs cross via queued `invokeMethod` + `jobCompleted` signal;
    `QImage` in the job is a detached copy; `TranslationJobQueue` is
    mutex-guarded with the 1-running + 1-waiting newest-wins rule.
    (Nit, not a bug: `TranslationWorker::stop()` queues `cancelPending`
    and then `quit()`s the thread loop, so the queued slot usually never
    runs — harmless, the queue is dropped at destruction anyway.)
  - **QML ids that do not exist:** none found. `TranslationOverlay`
    reads `parent.streaming` — exists at
    `DesktopStreamScreen.qml:34`. `DesktopSettingsChoice` signal is
    `selected(var value)` (`DesktopSettingsChoice.qml:21`), matching the
    page's `onSelected: value => ...`; `DesktopSettingsSlider` signal is
    `committed(real value)`, matching `onCommitted`. Both new QML files
    are registered in `QmlModule.cmake`.
  - **Default provider risk (note, not a bug):** default provider is
    `google` (`TranslationEngine.h`), which calls the undocumented
    `translate.googleapis.com/translate_a/single?client=gtx` endpoint —
    free but unofficial; expect possible rate-limiting/blocks. Default OCR
    is `gemini-vision`, which correctly returns NotAvailable without a key.
  - **Perf note (measure, don't guess):** `onStreamFrame` runs
    `convertedTo(ARGB32)` + SHA-256 over the full crop on the GUI thread
    per screenshot frame (`TranslationEngine.cpp` `cropHash`). Fine if
    screenshots are on-demand; measure on the EliteBook before calling
    the < 1 s target met.

- **Pre-existing upstream note (not a VN bug):** the Rust settings default
  `autoCheckForUpdates` is `true` (`native/opennow-core/src/settings.rs:1463`)
  while the QML toggle fallback reads `false`
  (`DesktopSettingsUpdatesPage.qml:154`). Editing the QML fallback alone
  does not change the stored default — see `UPDATER.md` §3. This is why the
  VN updater guard must actively set/disable the check (step 2 there).

## CI status

- **Unknown until pushed.** `vn-translation` pushes do not trigger `qt-ci`
  (push branches are only `dev`/`main` — see `CI.md` §1). No PR has been
  opened, so there is no CI result for any VN work: not green, not red,
  unknown. First signal will be the PR `vn-translation` → `dev`
  (contracts + linux-x64 / windows-x64 / macos-arm64 unit-test jobs).
- Package builds (Windows `.exe`/`.zip` Ethan can run) need a manual
  `workflow_dispatch` after that — PR CI only runs unit tests.

## What is UNPROVEN

- **No live GFN login test.** No run against a real GeForce NOW account
  and a real WuWa session has happened from this fork.
- **No OCR latency measured on Ethan's EliteBook** (HP EliteBook 840 G1,
  i5-4300U, HD 4400, 8 GB DDR3). The < 1 s / < 200 ms targets are targets,
  not measurements.
- **No Gemini key tested.** No API key has been entered, no Gemini Vision
  call has been made from the app, no quota/cost check done.
- **No overlay visual check.** In-place replacement (erase streamed English
  text, draw Vietnamese in its place) has not been seen on a real frame.
- **Updater guard not yet implemented or verified** (recommendation only,
  `UPDATER.md` §4). Until it lands, a VN build from this branch still
  auto-checks upstream releases by default (Rust default `true`).
- **No packaged VN build exists yet** (no dispatch, no local package run
  recorded).

## Integration pass (coordinator, after all 3 subagents)

- **R3b FIXED — settings now mirror into the engine.** QML is the only side
  that can read `ShellStore.settings`, so the sync lives in QML:
  `TranslationOverlay.qml` gains `syncEngineFromSettings()` (called on
  `Component.onCompleted` and on every `ShellStore.settingsChanged` —
  `applySetting()` replaces the whole settings object, so the signal fires
  on every save) and `DesktopSettingsTranslationPage.qml` gains
  `pushToEngine(key, value)` called from `setT()` (save + push) and, on page
  load, push-only (no settings writes — an earlier draft called `setT()` on
  load and would have spammed the core with ~10 writes per page open; caught
  in review and fixed before commit). Without this the engine kept
  `enabled=false` forever.
- **Gemini key now persists locally.** `TranslationEngine::setGeminiApiKey`
  writes `translation/geminiApiKey` to `QSettings` (registry on Windows);
  the constructor restores it and forwards it to the worker. Key is never
  logged, never in a URL, never committed. (Header comment corrected:
  it used to say "never persisted".)
- **OCR fallback chain added** (`TranslationWorker::processNext`): try the
  selected engine, then `tesseract`, then `gemini-vision` (deduped, only
  engines reporting `available()`). Falls back only on `NotAvailable`, not
  on `Failed`. This is what makes the default (`gemini-vision`, no key yet)
  work on a fresh install when Tesseract is present. NOT unit-tested —
  needs a live check (no key + Tesseract installed -> OCR via Tesseract).
- **WuWa DB seed is now deployed.** `Resources.cmake` registers
  `src/translation/data/wuwa-dialogue-db.json` as
  `:/translation/wuwa-dialogue-db.json`; the engine checks that qrc path
  last (AppData / next-to-exe win). Seed still has 0 datamined lines.
- **R4/R5 were already fixed** by the C++ subagent before review
  (`raw.chop(3)` at `OcrEngine.cpp:54`; `m_lastLookupUs` written inside
  the mutex lock at `TranslationCache.cpp:46-51`) — the docs review read a
  stale tree. Verified in the committed files.
- **R6 was a false alarm, reverted.** Deleting the English descriptive
  `"translation"` keys in `en.json` broke `npm run locales:check` (5
  missing source texts: "Cache hit", "Gemini API key", "OCR engine",
  "Screen OCR EN to Vietnamese", "Translation") — those keys ARE the
  registry the checker uses. Deletion reverted; `locales:check` passes
  again (exit 0, "Translations ok (2324 QML source texts…)").
  Vietnamese `qsTr` literals in the new QML remain a deliberate deviation
  from the upstream English-source convention (this fork does not run
  Crowdin sync for translation strings).
- **R7 fixed:** dead `translationSetting()` helper removed from
  `SettingsState.qml` (no caller; pages use `settingsScreen.valueSetting`
  / `boolSetting`).
- **Local checks after integration:** Python logic mirror 16/16 PASS;
  `npm run locales:check` exit 0; brace/paren balance OK on all changed
  files; no API key in any log call; `translationSetting` references: 0.
  Full Qt compile + `ctest` still only possible via GitHub Actions CI
  (no Qt in sandbox) — CI status remains unknown until the fork + PR exist.

- **Updater guard IMPLEMENTED (was recommendation-only).** `ShellStore.qml`
  gains `vnTranslationBuild: true` + the auto-update timer is gated off
  and `runAutomaticUpdates()` returns early in VN builds. Manual update
  checks still work but are fail-closed without a pinned key (no download).
  This is a 4th shim file (ShellStore.qml, +9 lines) — the only upstream
  file touched outside the original 3 shims, and it is documented in
  `UPDATER.md` §4. Full repoint to `duyzoz/OpenNOW-ORC` + a VN keypair is
  still a separate approved step touching `native/`.

## 2026-11-11 — Live-test fix: toggle won't stay on + full-screen region (run #14)

Ethan live-tested the run #13 build (video 17:34): Settings > Translation
renders correctly in Vietnamese, but the "Dịch màn hình" toggle flips ON
then back OFF after ~1s, and the TEST section says "Translation engine
chưa sẵn sàng". Root causes, both from the overnight build:

1. `DesktopSettingsTranslationPage.qml` used `page.hasEngine` /
   `page.keySaved` but never declared them (undefined = falsy), so
   `pushToEngine()` always returned early — no setting ever reached the
   engine, and Test / Save-key / Clear-cache stayed disabled.
2. Translation settings were written through `ShellStore.setSetting` to
   the Rust core, whose `SettingsStore::set` rejects unknown keys
   (`settings.rs:206` — "Unknown setting: translation.enabled" ->
   `invalid_setting`). The optimistic apply showed the toggle ON for ~1s,
   then `finishSettingWrite` -> `reconcileSetting` restored the old value.

Fix (no `native/` changes — that area stays untouched per Ethan's order):
- The C++ engine is now the store of record: every setting is loaded
  from / saved to QSettings under `translation/<key>` (same store the
  Gemini key already used). The QML page binds straight to the engine's
  properties; the overlay's `syncEngineFromSettings()` (which would have
  overwritten engine values with ShellStore fallbacks) is deleted.
- New preset "Toàn màn hình" (`full-screen`, rect 0,0,1,1): OCR reads the
  whole frame — dialogue, menus, labels — not just the dialogue box.
  Gemini OCR prompt widened from dialogue-only to all English text.
  Full-screen frames never stabilize (HUD/animation pixels change every
  frame), so the stability-hash gate is replaced by a 2s throttle
  (`FullScreenIntervalMs`) in that mode. An in-place box over the whole
  frame would hide the game, so selecting full-screen auto-switches the
  display mode to subtitle (engine-side) and the overlay forces subtitle
  for full-screen rects (defense in depth).
- Unit test `dialoguePresetRects` (static `dialogueRectForPreset` helper)
  + Python mirror checks; `npm run locales:check` exit 0 (2326 texts,
  2 new qmlSource entries: `toan_man_hinh`, `toan_bo_man_hinh`).

Still NOT fixed by this change — Ethan must do one of these for OCR to
run at all: paste a free Gemini API key (Google AI Studio) into the
Gemini API key field, or install Tesseract. Gemini Vision needs a key;
RapidOCR and Windows OCR are still stubs. The "Dịch thử" TEST button
works without a key (Google Translate, free). Live verification of the
toggle + full-screen translation is still Ethan's eyes.

## v2 — per-line in-place overlay + frame pump (2026-11-11 evening, local commit TBD)

Ethan's live test of run #20 (video, 61s of WuWa gameplay): the translation
never overwrote any English text. The ONLY thing on screen the whole video
was one black subtitle bar at the bottom showing the same sentence —
"Rover đi vào thành phố, tìm kiếm câu trả lời." — the default "Dịch thử"
test input. He is right: v1 did not do what he asked for.

Root causes found by reading the code (not guessing):
1. NO FRAMES: `screenshotReady` fires only for an explicit screenshot
   request (`requestScreenshot()` → fulfilled on the next presented frame
   by `NativeStreamRenderCallback::prepareFrame`). During normal gameplay
   nothing ever requests one, so `TranslationEngine::onStreamFrame` is
   never called and live OCR literally never runs.
2. TEST LEAK: `requestTestTranslation` went through `emitTranslation`,
   which sets `hasTranslation` + emits `translationReady` — so the TEST
   result stuck on the live overlay as a permanent subtitle bar. With no
   jobs ever running, no error ever cleared it after 2s, so it stayed for
   the whole video.
3. DESIGN GAP: even with frames, v1 drew ONE box over the dialogue area
   (or a subtitle bar for full-screen) — never Vietnamese text positioned
   over each English line. That is not "dịch đè lên chữ tiếng Anh cũ".

Fix (this change):
- Frame pump: `TranslationEngine` runs a 1s `QTimer` while enabled and
  emits `frameRequested`; `ApplicationStartup` connects it to
  `NativeStreamRuntime::requestScreenshot()`. Verified safe:
  `requestScreenshot()` is a no-op while no stream is presenting and
  while one is already pending, and `AppController::saveStreamScreenshot`
  only saves when the user took a screenshot (`m_screenshotPending`) —
  the pump can neither pile up nor write files.
- Per-line blocks: OCR now returns `OcrBlock{text, rect}` per line
  (rect normalized 0..1 within the crop). Gemini prompt asks for JSON
  `{"blocks": [{text, x, y, w, h}], "confidence"}` (old `{"text"}` shape
  still accepted as one whole-image block). Tesseract runs `tsv` first
  and groups level-5 word rows into lines by (block, par, line), with a
  plain-text fallback as one whole-image block.
- Worker translates the blocks joined with "\n" in ONE call (Google free
  endpoint preserves line breaks), splits the reply back onto the blocks
  (count mismatch → per-block fallback, never a guessed mapping), maps
  each block rect to full-frame coords via `mapBlockRectToFrame`, and
  never emits an empty block (an empty translation leaves the English
  line visible instead of covering it with an empty box).
- Engine publishes `translationBlocks` (QVariantList of
  `{original, translated, x, y, w, h}`); the overlay's in-place mode is
  now a `Repeater` drawing one box per block, padded 8px, exactly over
  its English line. Subtitle mode joins the live blocks (never
  `lastTranslatedText`). Full-screen no longer forces subtitle anywhere
  (engine `setDialoguePreset` + overlay): per-line boxes work for the
  whole frame.
- TEST separation: `TranslationJob.testJob` flows through the worker
  result; `setTestResult` updates only the TEST section's `last*` fields
  + status — never `hasTranslation`, `translationBlocks`, or
  `translationReady`. A failed test job also leaves the live overlay
  alone.
- Hash cache now stores blocks too, so a repeated identical crop still
  hits before any OCR/network.
- New unit tests: `mapBlockRectToFrame`, `blocksToVariantListRoundTrip`,
  `testJobDoesNotShowOnStream` (engine instance, no network), and
  `framePumpFiresWhenEnabled` (QSignalSpy, 1s pump). Python mirror 25/25;
  `npm run locales:check` exit 0 (2326 texts, no new strings).

Still NOT proven live: per-line box positions from real Gemini JSON
(coordinates are the model's guess), the 1s pump's cost on his HD 4400,
and the full flow end-to-end. Only Ethan's EliteBook can prove it — the
overlay lives inside the Windows app on his machine, so no browser
session (and no login as him) could test it.

## v3 — Custom OpenAI-compatible API + streaming translation (2026-11-12, local commit TBD)

Why: run #34 live test — Gemini answers VERY slowly on Ethan's free key
(text takes multiple responses to translate). The pipeline is sequential
(1s pump + 400ms stability + OCR call + translate call), so Gemini free
tier costs 2–5s per new line. Ethan's ask: keep the Gemini key, ADD a
custom API — user pastes their own base URL + API key, the model picker
is fetched live from that base+key, user freely picks a model, PLUS
streaming for translation with that key. He has a very powerful key in
mind. Generic for ALL providers, not tied to any one company.

What shipped:
- `CustomApi.{h,cpp}` (new, pure + testable): `normalizeCustomApiBase`
  (trims, strips trailing "/", strips a pasted "/chat/completions"),
  `customApiModelsUrl` / `customApiChatUrl`, `parseCustomApiModels`
  (GET {base}/models → {"data":[{"id":...}]}, deduped, server order),
  `parseSseStreamText` (re-parses the ACCUMULATED SSE buffer after
  every chunk, so a chunk split mid-line still parses; skips "[DONE]",
  garbage lines, and never double-counts a delta + message in one
  event), `parseChatCompletionText` (message.content, legacy .text
  fallback), `buildChatCompletionPayload` (text-only user string, or
  vision [text part, image_url data: URL] when an image is given).
- `TranslationHttp::postStream`: synchronous streaming POST with a
  per-chunk callback; the 8s timeout is INACTIVITY-based (restarts on
  every chunk) with a 60s hard cap; 429 retries exactly once like
  `request()`.
- `TranslationProvider`: new virtual `translateStreaming(text, src, tgt,
  onChunk)` — default calls `translate()` once and reports the whole
  text as one chunk, so Google/Gemini work unchanged through the same
  path. New `CustomApiTranslateProvider` ("custom"): non-stream chat
  completion for `translate()`, SSE streaming for
  `translateStreaming()` (emits only growth of the parsed text).
  Same WuWa system prompt as Gemini (names stay English, Rover
  pronouns untouched).
- `OcrEngine`: new `CustomVisionOcrEngine` ("custom-vision"): same
  blocks-JSON prompt as Gemini Vision over chat/completions with a
  base64 JPEG data URL. The Gemini envelope parser was split into
  `parseOcrBlocksJson` (shared) + `parseGeminiOcrText` (envelope only).
  Opt-in only — default OCR stays gemini-vision; a text-only model
  chosen for Vision fails honestly (Failed is never retried on another
  engine).
- `TranslationWorker`: job carries `CustomApiConfig` (base+key+model);
  worker keeps a copy via `setCustomApiConfig` (same pattern as the
  Gemini key). New `jobProgress(original, partial, sourceRect,
  testJob)` signal, emitted from the streaming callback; new
  `fetchCustomApiModels(base, key)` slot → `customApiModelsFetched`
  signal (GET {base}/models, Bearer key).
- `TranslationEngine`: Q_PROPERTYs `customApiBase`, `customApiModel`,
  `customApiModels` (QStringList, live-fetched, never persisted) +
  Q_INVOKABLEs `setCustomApiKey` / `hasCustomApiKey` /
  `fetchCustomApiModels`. All three config values persist in QSettings
  under `translation/customApi*` (local only, key never logged, never
  in the repo). `setPartialTranslation` updates the TEST section or the
  live overlay (one block over the source rect for now) as chunks
  arrive; the final `jobCompleted` always replaces the partial, and
  partials are never cached. Provider list is now
  google/gemini/custom; OCR list adds custom-vision.
- Settings page: "Dịch bằng" gains "API tùy chỉnh" (custom), OCR
  gains "API tùy chỉnh (Vision)". New CUSTOM API panel: Base URL
  field (saved on editingFinished; the fetch button commits it first),
  API key field + "Lưu key" (password, never shown back), "Lấy danh
  sách model" button, Model dropdown fed by the fetched list (falls
  back to the saved model, or "Chưa lấy danh sách"). Descriptions
  point at the panel when custom is selected but not configured.
  `locales:check` exit 0 (2343 texts, 17 new strings in
  `translation.qmlSource`).
- Tests: 6 new C++ slots (base normalization, models parsing, SSE
  parsing incl. mid-line split, chat-completion parsing, payload
  shape incl. vision, custom factory + isConfigured). Python mirror
  extended with the same cases (44 checks, 0 failures).

Still NOT proven live: the whole custom-API flow end to end on his
EliteBook — fetching a real model list from his key's base, picking a
model, streaming translation showing progressively, and custom Vision
OCR with a vision-capable model. Only Ethan's key + machine can prove
it; the sandbox has no key and no Windows build.

Run #41 (gh-runners @ aa2db33) FAILED 11:08 — both Windows jobs, same
cause: `tst_translation.cpp` used `QJsonArray` with no
`#include <QJsonArray>` (C2079 on MSVC, lines 387/399). App code
compiled; only the new test file failed. Fix: add the include (plus
`<QStringList>` as the same class of insurance) and re-dispatch.

## v4 — Chunk mode + white frosted overlay (2026-11-12, local, NOT dispatched yet)

Translation batch finished 2026-11-12 14:47: 54/55 chunks PASSED (0
semantic flags) + chunk 043 partial (1,000/4,000, valid JSON) merged by
merge_vi.py → vi_multi_38.json: 338,677/342,381 entries translated
(98.92%); 3,704 entries still English, all from chunk 043's remaining
3,000 strings. A finisher is translating those 3,000 strings in parallel
(100-item slices); if it passes before the chunk-repo upload, re-run
gen_client_chunks.py and the manifest counts refresh. Client chunk files
(55 × {"version":1,"entries":{en:vi}}, ~43 MB, largest 1.9 MB) generated
by gen_client_chunks.py into ~/workspace/wuwa-vi-translation/client_chunks/
+ manifest.json (game 3.8, 213,384/216,384 strings, 3,042,056/3,133,145
words). Upload to huyzoz/wuwa-vi-chunks (browser only, 2 commits: root
manifest+README, chunks/ 55 files) is in progress.

Client changes (this branch, local commit pending):
- WuWaDialogueDb: loadChunkFile() merges chunk files without clearing;
  lookupFuzzy() = exact → squashed-form exact → Levenshtein ratio >= 0.88
  among entries sharing the first two words (first-two-words + squashed
  indexes, rebuilt on load; O(1) squashed lookup, tiny candidate sets).
  clear() added. similarityRatio() is a pure two-row Levenshtein.
- TranslationWorker: chunk DB pointer + enabled flag (queued slots);
  processNext looks every OCR block up in the DB first (exact, then
  fuzzy); only misses go to the provider (joined call, same split-back
  + per-block fallback as before, merged into reading order); all-hit
  jobs create NO provider at all (translateMs = 0, chunkHit = true).
  Partials suppressed when any line already came from the DB.
  startChunkDownload() GETs the manifest, then each listed file one per
  event-loop turn (queued self-invocation) so jobs keep running between
  downloads; files land in <cache>/<game>/<version>/ + manifest.json.
- TranslationEngine: chunk Q_PROPERTYs (chunkEnabled/chunkGame/chunkGames/
  chunkGameTitle/chunkGameVersion/chunkGameWordsText/chunkGameStringsText/
  chunkTranslatedText/chunkRepoUrl/chunkStatus/chunkProgress/chunkDbSize/
  lastChunkHit), persisted in QSettings translation/chunkEnabled|Game
  (never ShellStore). downloadChunks() → worker; onChunkDownloadFinished
  reloads the DB from the cache. requestTestTranslation checks the chunk
  DB first. Coordinate memory: a line whose OCR rect agrees twice
  (tolerance 0.02) snaps later jitter back to the stored rect (placement
  only, capped at 200 lines). Bundled Be Vietnam Pro (Regular/Medium/
  SemiBold/Bold, OFL) loaded via QFontDatabase in the ctor; resource
  block /fonts added to Resources.cmake.
- Overlay: TranslationBox is now WHITE frosted glass — live blur of the
  video behind it (ShaderEffectSource + MultiEffect blur+mask, rounded
  corners) under a white tint, dark #1a1a2e text in Be Vietnam Pro.
  No video item → plain translucent white box (never black). Status pill
  is white too, and shows "Chunk hit" when the last translation came
  from the DB. DesktopStreamScreen passes videoItem: streamVideo.
- Settings: new CHUNK panel (after CUSTOM API): Dịch Chunk toggle,
  Game choice (only Wuthering Waves now, built as a list), detail rows
  (version, translated count, words, status, Download button, GitHub
  link via Qt.openUrlExternally). 14 new strings in locales/en.json
  (2357 texts, locales:check exit 0).
- Tests: 3 new C++ slots (chunk merge without clearing + missing/invalid
  file handling, clear(), fuzzy lookup incl. typo/punctuation/no-match
  + similarityRatio units) and 9 new python-mirror checks (53 checks,
  0 failures). No new C++ files, so Sources.cmake/Tests.cmake are
  untouched; no workflow changes.

Still NOT proven live: chunk download + DB lookup on the real app,
frosted-blur cost on the HD 4400 (the white base still covers the text
even if the blur layer fails), glyph rendering of Be Vietnam Pro in the
overlay, and the chunk-043 remainder (finisher pending). CI is the
compiler: dispatch on huyzoz gh-runners, watch every 5 min.

## v4.1 — full-screen default + honest chunk state (2026-11-12, live-test failure fix)

Live test of v4 (run #53, Ethan 17:48, 4 screenshots: Terminal/profile,
Character Voicelines, Quests "A Bouquet for Tomorrow", WavesLine chat):
NO translation on any screen, no error shown. Code read after the fact
found the causes — the pipeline itself (frame pump → screenshot → OCR →
chunk DB → overlay) was wired correctly; the failure was upstream of it:

1. DEFAULT CROP MISSED ALL THE TEXT (main cause). The shipped default
   preset "wuthering-waves" crops only the bottom dialogue strip
   (0.05, 0.72, 0.90, 0.24). All four test screens put their text
   outside that strip (quest description, voicelines, chat bubbles,
   profile) → OCR returned empty → the engine silently kept the old
   state. The full-screen preset existed but sat third in the dropdown
   and Ethan's saved preset was the old default. Fix: default preset
   is now "full-screen" (whole frame: dialogue, quests, chat, menus);
   a saved "wuthering-waves" is migrated to "full-screen" once at
   startup (it was the code default, never a deliberate choice;
   "generic" is kept as-is, the strip stays selectable in Settings).
   New pure helper TranslationEngine::migrateDialoguePreset() with a
   C++ slot + 5 python-mirror checks.
2. CHUNK STATUS LIED WHEN CHUNK MODE WAS OFF. loadCachedChunks() only
   ran when chunkEnabled was true, so with the toggle off the panel
   still read "Chưa tải chunk" after a successful download, and the
   "Tải chunk" button stayed enabled forever. Fix: cached chunks load
   on every startup; new chunkDownloaded Q_PROPERTY (DB size > 0);
   the button shows "Đã tải" and disables once downloaded; the Dịch
   Chunk toggle description says "Đã tải sẵn cơ sở dữ liệu — bật để
   dùng…" when the DB is on disk.
3. SILENT NO-TEXT. An OCR job that found no text left the status at
   "Translation ready" — indistinguishable from a dead pipeline. Fix:
   the status now reads "No text in scanned area" when a job returns
   empty (proves the pump runs and the crop is simply empty).

Also corrected here: the chunk DB is 100% (finisher chunk 043 PASSED
15:41, vi_multi_38.json 342,381/342,381, chunk repo huyzoz/wuwa-vi-chunks
manifest 216,384 strings) — the v4 note above saying "chunk-043
remainder (finisher pending)" is stale.

Changes (6 files, local checks green — python mirror 58 checks 0
failures, locales:check 2359 texts exit 0): TranslationEngine.h/.cpp,
DesktopSettingsTranslationPage.qml, locales/en.json (+5 strings, −2
orphaned: the old preset description and detail), tst_translation.cpp
(+1 slot), translation_logic_check.py (+5 checks). No new C++ files;
Sources.cmake/Tests.cmake untouched; no workflow changes.

Still NOT proven live: any translation on the real app (needs the new
build), chunk download + DB lookup on the real app, frosted-blur cost
on the HD 4400, glyph rendering. Also still open: Ethan's answers on
toggle state / saved key / status text from the v4 live test.

## v5 — full A-Z database + mandatory vision key + mandatory chunk download (2026-11-12, Ethan's call 18:01)

Ethan's order: ALL game text A-Z, no key missed — not just main-quest
dialogue; and a redesign: a vision-model API key (Gemini or custom
base) is MANDATORY, chunks MUST all download, the AI reads the
coordinates of every text line on the full screen, the code looks each
line up in the chunk DB, and the Vietnamese is drawn at those exact
coordinates. Assessment written back to him first: the loop he
describes (OCR blocks with coords → DB lookup by text → overlay at
those coords) ALREADY EXISTS in v4 (+ v4.1 full-screen default). The
real v5 gaps are DB coverage + mandatory key + mandatory chunks. One
honest nuance: the DB is keyed by TEXT (textmaps carry no screen
coordinates); coordinates always come from live OCR at runtime.

A-Z inventory (repo 3.8 tree, all 210 EN text files): 220,764 unique
strings / 3,206,219 words total. Dialogue done (216,384). Remainder
only 4,380 unique strings / 73,074 words (multi_text_1sthalf 2,057,
2ndhalf 800, ItemInfo 794, SubtitleText 242, MapMark 199, ...).
Prepped as chunks 056 (4,000) + 057 (380), keys u216385..u220764,
validated zero-overlap with the dialogue set; translation runs in a
coordinator subagent (same GLOSSARY.md + validate_chunk.py pipeline
as the dialogue batch, incl. the chunk-043 off-by-one lesson: verify
each vi matches its OWN en, no *.partial.json left in out/).

Client changes (this commit):
- Mandatory vision key: new visionKeyConfigured property (Gemini key
  OR custom-API key; visionKeyChanged emitted from both setters).
  submitCrop refuses keyless jobs with status "Vision key required —
  paste a Gemini key or set up Custom API" (hash-cache hits still work
  keyless — they need no OCR). The "Dịch màn hình" toggle is disabled
  until a key is saved, with a "Bắt buộc: ..." description.
- Mandatory chunk download: chunkEnabled defaults to true for fresh
  installs; ensureChunksDownloaded() (ctor, setEnabled(true),
  setChunkEnabled(true)) starts the full download whenever chunk mode
  is on and the DB is not complete. downloadChunks() is guarded by
  m_chunkDownloading (one download at a time); the flag clears in
  onChunkDownloadFinished on both paths.
- chunkDownloaded now means COMPLETE, not just non-empty: new pure
  helper isChunkDownloadComplete(dbSize, expectedStrings) with a C++
  slot + 6 python-mirror checks. expectedStrings comes from the
  cached manifest's translatedStrings (0 = unknown, never complete),
  so a partial cache keeps the button live for re-download.
- Catalog numbers updated to the A-Z totals: 3,206,219 words,
  220,764 strings.
- Fuzzy matching deliberately UNCHANGED: with ratio 0.88 it already
  cannot fire on strings shorter than 9 chars (1 edit in 8 chars is
  0.875), so short UI labels can only hit by exact/squashed match.
  The "short label" risk was analyzed, not real in the current code.

Changes (6 files): TranslationEngine.h/.cpp,
DesktopSettingsTranslationPage.qml, locales/en.json (+1 string:
2360 texts, locales:check exit 0), tst_translation.cpp (+1 slot),
translation_logic_check.py (+6 checks, 0 failures). No new C++ files;
Sources.cmake/Tests.cmake untouched; no workflow changes.

Still NOT proven live: anything on the real app (needs the v5 build),
chunk download + DB lookup on the real app, frosted-blur cost on the
HD 4400, glyph rendering. SEQUENCING: v4.1 run #59 is still building
(cron watches it); the v5 upload + dispatch wait until #59 is green.
The chunk-repo refresh (manifest 57 chunks, 220,764 strings) waits
for the 056/057 translation to PASS validation.

## v5.1 — fix run #59 moc failure (2026-11-12 ~18:20, same day)

Run #59 (v4.1) FAILED: Package windows-x64, step 25 "Build Qt
release", exit 1, 18:11:13 +07. The signed-in job log (browser)
gave the exact first error: tst_translation.cpp(329:1): "Parse
error at ')'" from Qt moc, target opennow-translation-tests — the
only compile error in the whole log (1 error, 1 warning; the warning
was just Node.js 20 deprecated).

Root cause — MY edit mistake, not the feature code: the test file
defines slots INLINE in the class (no separate declaration list).
My v4.1 edit that added migrateDialoguePresetCases inserted the new
slot name as a bare line between dialoguePresetRects() and its "{",
and the v5 edit did the same for chunkDownloadCompleteCases — so
three bare "void xxx()" lines piled up with a single body attached
to the last one. moc chokes on the first ")" (it expects ";" or "{").
Local checks never caught it: the python mirror and locales:check
do not compile C++.

Fix (this commit): deleted the two stray bare lines, so each of the
27 slots is defined exactly once with its own body (verified by
script: 0 bare headers, 0 duplicates). The v5 tree is dispatched as
run #60 on top of the fixed v4.1 — run #59 can never go green, so
waiting for it is pointless; its watch cron is replaced by a #60
watch.

Note: the catalog card already shows the A-Z totals (3,206,219
words / 220,764 strings) while the chunk repo still serves 55
chunks / 216,384 strings until the 056/057 translation lands —
display-only mismatch for a few days; the download/completeness
logic reads the SERVER manifest, so it stays correct.
