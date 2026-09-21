# Regional keyboard mapping

On Linux, the embedded Qt client translates XKB physical keycodes from X11 and
Wayland to the Windows virtual keys used by the session's requested keyboard
layout. The layout comes from the core's session snapshot, not live preferences.
Change **Settings → Controls → Keyboard layout** and restart or resume the session
to request a different layout remotely.

The tables cover US and UK English, Turkish Q, German, French AZERTY, Spanish,
Latin American Spanish, Italian, Portuguese, Brazilian Portuguese, Polish
Programmers, Danish, Norwegian, Swedish, Finnish, Russian, Ukrainian, Japanese
JIS 106, Korean, and simplified/traditional Chinese. Chinese mappings provide
the physical keys for the remote IME; they do not transmit locally composed IME
text. Alternative variants such as Dvorak, Turkish F, and French BÉPO are not
separate supported choices.

The Windows and macOS clients retain their platform-specific physical-key
mapping, including native macOS keycode zero. Unknown Linux layouts and synthetic
events without a native scan code retain logical-key fallback. Layout lookup occurs when the property
changes; each gameplay event uses a bounded array lookup without allocation.

The Rust core owns the keyboard choices and wire identifiers. Japanese uses
`ja-106`, with `ja-JP` and `Japanese106` accepted as saved aliases; Spanish uses
`es-ES_tradnl`, with `es-ES` accepted as a saved alias. Native table lookup resolves
these canonical identifiers to the corresponding regional tables. Ukrainian
`uk-UA` and Russian `ru-RU` remain separate choices.

## Table sources and regeneration

`opennow-qt/src/streaming/PhysicalKeyMapData.h` is generated from the Windows
keyboard tables published by [kbdlayout.info](https://kbdlayout.info/).
`scripts/generate-keyboard-maps.py` lists the source driver for every locale.
Japanese uses `KBD106`, not the US-shaped `KBDJPN` stub. Chinese uses the US
physical-key arrangement. The generator converts PC scan codes to Linux evdev
codes for the international keys, including Brazilian ABNT keys.

The source is mapping data, not text-translation data. Keep dead keys as key
events and let the remote Windows layout compose them. AltGr is sent as right
Alt, with Control+Alt modifier bits on the accompanying keys. It must not match
an unmodified local shortcut. Key-up uses the virtual key saved on key-down,
even if the logical key or selected map changes before release.

Regenerate or compare with the published tables:

```sh
python3 scripts/generate-keyboard-maps.py
python3 scripts/generate-keyboard-maps.py --check
```

Both commands need network access. For an offline comparison, download the
driver XML files from `https://kbdlayout.info/<DRIVER>/download/xml` into one
directory as `<DRIVER>.xml` and pass `--source-dir <directory>`. Neither normal
builds nor the application fetch these files.

## Verification

```sh
cmake --build build/opennow-qt --target opennow-streamvideo-tests
QT_QPA_PLATFORM=offscreen build/opennow-qt/opennow-streamvideo-tests \
  linuxRegionalKeysUsePhysicalPositions \
  linuxPhysicalKeysFollowTheRequestedKeyboardLayout \
  regionalLayoutLookupIsBoundedAndNormalizesLocaleNames \
  altGrPreservesWireModifiersWithoutTriggeringLocalShortcuts
node --test scripts/check-keyboard-layouts.test.mjs
cargo test --manifest-path native/opennow-core/Cargo.toml cloudmatch
```

The regional Qt cases exercise press and release through `StreamVideoItem` and
its typed native-runtime boundary. Existing stream tests cover focus loss,
overlays, and fullscreen transitions. A real GFN session is still required to
verify the remote layout and IME installed by the provider. For each layout,
check letters, shifted digits, OEM punctuation, dead-key composition, AltGr,
and keypad input, then repeat with the local overlay open and after returning
from fullscreen. Local shortcuts must stay local and releasing keys after
focus loss must not leave remote keys held.
