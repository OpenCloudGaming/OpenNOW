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
