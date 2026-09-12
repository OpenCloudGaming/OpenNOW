# Desktop Renew implementation

Reference: Paper **OpenNOW Socket → Desktop Renew**. Settings layout and SVG
paths were transcribed from the inline-style export; `desktop-renew.jpg` is the
background artwork exported from that file. Store logos keep their existing
brand artwork (see `opennow-qt/res/icons/STORE-ICON-SOURCES.md`).

At 1440×900 the settings shell uses a 72px app rail, 60px header, 36px footer,
272px settings navigation, 20px content gaps, 68px rows and 40px icon tiles.
Resolution expands inline into aspect-ratio groups of 118×52px tiles. It keeps
the current selection, disables known unentitled sizes and filters against the
current screen when **Fits monitor** is selected. Codec and color-depth pills
use the existing settings validation; they are not Qt ComboBoxes.

Theme colors remain driven by the existing theme system. The Paper reference
uses Aurora; existing user theme preferences are not overwritten. Small windows
reflow navigation and setting controls instead of scaling text to illegibility.

Visual acceptance runs can use `--smoke-test --desktop --smoke-paper-design`
with `--smoke-width 1440 --smoke-height 900`, `--route settings-streaming`, and
`--screenshot <absolute-path>`. Add `--smoke-resolution-open` for the tile picker.
These flags do not start the core or persist account/settings changes.
The Paper fixture also supplies seven sample store rows (linked, expired and
unlinked) so real brand icons and action states can be checked without an account.

Navigation uses static ink-colored SVG variants in light mode, with no additional
shader layers. Settings pills scale their width together with their text; the
bitrate control reserves a scaled label column instead of squeezing it on large
windows.

Game-details summary cards use theme-aware monitor, globe and clock icons in
Paper's 36px tiles: accent ink in dark mode and dark foreground ink in light mode.
The details action icons use SVG paths rather than font glyphs or fixed-color
assets, including the three-dot menu, and remain readable in both themes.

Network, About and all expanded settings panels share those same rows and
surfaces. Regions and interface languages use the reusable inline
`DesktopSettingsChoice`; short lists such as update channel and stats position
use segmented pills. Proxy and shortcut inputs share `DesktopSettingsField`.
Release notes and long descriptions grow with their content instead of clipping.
The shortcut screen's all-settings reset explicitly confirms its scope.

For isolated Advanced-panel acceptance, add `--smoke-settings-panel` with
`stats`, `audio`, `interface`, `console`, `shortcuts`, `controllers` or
`subscription`. `--smoke-choice-open` opens the region/language picker.
`--smoke-renew-settings-actions` checks the new controls' settings callbacks
without starting the core or making network requests.

The HDR toggle remains disabled because the native session API cannot request
HDR. Mouse capture follows the server cursor and the existing F8 shortcut;
the UI does not pretend that a persistent relative/absolute override exists.

Verified on Windows with Qt 6.11.2: all 104 Qt/acceptance tests, five Rust settings
tests, and localization checks pass. Compared native screenshots with Paper at
1440×900; also inspected compact 960×640, large 2560×1440 and light-mode layouts.
Live NVIDIA login, store linking and gameplay require an account and were not
exercised by the isolated acceptance runs.

The subsequent [motion audit](qt-motion.md) adds interrupted-animation and
presenter-lifetime coverage (108 Qt tests total), with hardware results and the
remaining cold-route first-frame latency limitation documented separately.

## Region latency follow-up

Network is the only settings page with a region selector; the duplicate Account
control is removed without changing the persisted region. Ping regions now
queues behind region discovery, reports progress and failures inline, and
re-enables retry after completion or timeout. Automatic mode displays the best
measured latency without changing the automatic preference. Expanded region
choices distinguish unmeasured regions from failed measurements.

`qml-region-ping-960` and `qml-region-ping-1440` exercise the production settings
controls and ShellStore with an isolated, in-memory transport. They cover sign-in
and core readiness, delayed/empty/failed discovery, duplicate clicks, mixed and
all-failed ping results, zero-millisecond results, timeout/retry, cancellation,
and removal of the Account selector. Run `--smoke-test --desktop --route
settings-network --smoke-region-ping` for the same sequence; `--screenshot` can
capture its final results. The fixture is bundled only with tests.

A separate live core check discovered 24 regions and measured all 24 successfully
(best 6 ms); the measurement backend itself did not need changes.
The updated Windows build passes all 110 Qt/acceptance tests and localization
checks. Native-rendered screenshots were inspected at 1440×900 and 960×900.

## Store pagination follow-up

Store now loads cursor-based pages of at most 100 games, displaying each page
immediately. The Rust core measures the encoded result and refetches the same
cursor with fewer records when necessary to stay within a 768 KiB result budget;
the shell/core protocol's 1 MiB limit is unchanged. Marquee, shelves and filter
definitions are separate bounded requests. Their failure does not hide games.

Page errors preserve already loaded games and expose the actual failure plus a
retry of the same cursor. Search/account changes cancel both request channels
and ignore late results. Automatic loading is limited to 100 pages/10,000 games,
with explicit continuation beyond that bound. See [core protocol](core-protocol.md).

`qml-store-paging` exercises the production Store controls and ShellStore with
an isolated transport: progress, partial failure/retry, overlap deduplication,
empty completion, repeated cursors, scoped errors and stale-response isolation.
Use `--smoke-test --desktop --route store --smoke-store-paging --screenshot
<absolute-path>` to capture its final partial-failure layout without network use.

The read-only `opennow-qt/tests/verify_store_paging.ps1 -CorePath <core-executable>`
check walks the live catalog through the real protocol, then checks storefront
sections, search and continued core connectivity. On September 4, 2026 it loaded
all 5,967 unique games across 60 pages. The largest game-page response was
268,111 bytes; the largest response including storefront sections was 515,117
bytes (below 1,048,576 bytes). All three sections and search succeeded and the
same core remained connected.

All 111 Qt/acceptance tests, all 92 Rust core/workspace tests and localization
checks pass. Native-rendered partial-failure layouts were inspected at 960×900
and 1440×900. The progress/error feedback uses the existing Desktop Renew
surfaces and controls; the Store header now reads the Store count, not Library.
After deploying both binaries, the reopened desktop completed 60 Store pages
and all three presentation requests with zero Store errors, one core handshake,
no QML warnings and a responsive window. The full 111-test Qt suite also passed
against that exact deployed build.

## Store artwork and layout follow-up

Before this follow-up, existing work was checkpointed in three grouped commits:
`2642b1e4` (catalog protocol), `a6c893c9` (Desktop Renew integration), and
`3c985411` (poster motion/artwork retention).

Main storefront shelves now execute the explicit GraphQL document requesting
`GAME_BOX_ART`. The successful persisted MainV2 query previously returned only
landscape hero artwork, bypassing the richer fallback document. A live comparison
of NBA 2K27, The Blood of Dawnwalker and Onimusha confirmed the old shelf URLs
were `HERO_IMAGE` and the updated URLs exactly match the catalog's `GAME_BOX_ART`.
`verify_store_artwork.ps1 -CorePath <executable>` checks this without modifying
the account or library. The full catalog still loads 5,967 games across 60 pages;
the largest response including the richer shelves was 592,582 bytes, below 1 MiB.

Store chips center an intrinsic-size row inside their control content item.
Store and Library filters wrap, provider logos share one mapping with Settings,
and the Store menus show readable labels in a bounded scrollable list without
discarding choices after the seventh entry. Game titles allow two lines and
expose the full title on hover. Unpriced games no longer show “Available”; real
ownership, free-to-play and price information remains supported.

The sidebar is opaque to prevent underlying page text showing through. Expanded
labels stay hidden while its width animates, account labels reserve room for
controls, and a pinned sidebar reserves page space (hover expansion remains an
overlay). Use `--smoke-store-paging --smoke-store-appearance` with the desktop
Store smoke route to inspect long names, all provider icons and an expanded
sidebar. Add `--smoke-store-genres` for the genre menu.

Store retains its catalog in memory without a timer. Route re-entry reuses
the current results; clearing a search restores one bounded browse snapshot,
including its cursor when pagination was interrupted. Account changes discard
the in-memory cache and cancel outstanding requests. The core also persists
successful Store pages and presentation documents in an account-scoped disk
cache, reused after app/core restarts. Refresh (or F5 on Store) explicitly
invalidates this context's disk cache; existing games remain visible while new
pages load. The disk cache is bounded to 64 MiB / 512 entries, with no automatic
ten-minute expiry. Cache hits yield briefly between pages without network pacing.

The cache regression covers route re-entry, search restoration, explicit refresh, partial
pagination resume, stale replies and account invalidation. The live desktop
loaded all 5,967 games, navigated to Library and back, and issued zero additional
catalog requests. All 115 Qt tests, 93 Rust core/workspace tests and the
891-key localization check passed. Compact appearance fixtures should include
`--allow-multiple-instances` when another OpenNOW window is running.

At narrow widths, footer shortcuts use their compact style and secondary status
text hides when it would collide with them.

`verify_store_cache.ps1 -CorePath <executable>` populates the full catalog and
presentation cache, exits that core, then starts another and requires every
response to be a disk-cache hit with the same game count.

On September 5, 2026 this loaded 5,970 games / 60 pages plus all three
presentation documents in 52.30 seconds on the cold pass. A newly started core
returned the same catalog with 63/63 disk-cache hits in 2.21 seconds. The six
disk-cache regression tests also pass (restart, scope isolation, invalidation,
corruption/size limits, in-flight invalidation and bounded entry count).
The persistent-cache build passed all 115 Qt tests, all 99 Rust core/workspace
tests, localization validation, and the native 960×640 Store appearance fixture.
