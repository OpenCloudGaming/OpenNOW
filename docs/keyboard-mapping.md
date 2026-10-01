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

## Swedish OEM wire keys

The Linux Swedish map uses US physical-position identifiers for the twelve
classic OEM keys before consulting the generated Windows table. Windows layout
VKs and GFN wire identifiers are not interchangeable here. The generated Swedish
table remains an accurate Windows table; it is not the wire oracle.

This correction is limited to Swedish. It preserves regional letters, digits,
keypad keys, modifiers, other layouts, and the session's requested layout.
Layout aliases resolve to the same Swedish map. Synthetic events without a native
scan code keep their logical-key fallback.

The independent evidence is the official Linux x86_64 payload audited in
[OpenNOW #1003](https://github.com/OpenCloudGaming/OpenNOW/pull/1003), from
`https://files.zortos.me/x86_64.zip`. The archive SHA-256 is
`47ddbe0425b9ab560f64fa42a0052794c9de335ded0fd59637f082dd7a161ad4`.
Geronimo's build ID is `15d0eebc08da503f1f37ea9cae2dbac1d760fea4`, and
Bifrost's is `fa3685038bd71962fe30ad09482bcb0721a54f35`.

An offline x86_64 execution of Geronimo's scancode-table initializer at `0x368980`
and Bifrost's key converter at `0x3bf280` produced the values below. Geronimo's
`InputEvent(SDL_Event...)` constructor reads the SDL scancode at `0x369567` and
indexes that table at `0x3695df`. Bifrost's event dispatcher at `0x319d84` routes
keyboard input through `0x319ad0`, `0x319a06`, and `0x319996` to the serializer
at `0x43de84`. Executing that path produces the same big-endian key field for
both type 3 key-down and type 4 key-up. These addresses identify this binary
build only; they are not stable SDK entry points.

| Swedish key | SDL physical scancode | XKB keycode | Wire VK |
| --- | ---: | ---: | ---: |
| + / ? | 45 | 20 | `0xbd` |
| Acute / grave dead key | 46 | 21 | `0xbb` |
| å | 47 | 34 | `0xdb` |
| Diaeresis dead key | 48 | 35 | `0xdd` |
| Apostrophe / asterisk | 49 | 51 | `0xdc` |
| ö | 51 | 47 | `0xba` |
| ä | 52 | 48 | `0xde` |
| § / ½ | 53 | 49 | `0xc0` |
| Comma | 54 | 59 | `0xbc` |
| Period | 55 | 60 | `0xbe` |
| Minus / underscore | 56 | 61 | `0xbf` |
| ISO angle brackets | 100 | 94 | `0xe2` |

These are independently executed official-client values, not a live server
capture or a public frozen NVST specification. They agree with the
[#924 reporter's known-good .673 behavior](https://github.com/OpenCloudGaming/OpenNOW/issues/924#issuecomment-5835203003).
The .717/.810 regional-table path changed å from `0xdb` to `0xdd`, ö from
`0xba` to `0xc0`, and § from `0xc0` to `0xdc`, while ä stayed `0xde`.
The reporter observed the corresponding regression. A fresh Swedish GFN session
is still required to confirm the corrected release remotely, including dead-key
composition and AltGr. This evidence does not validate the other regional tables.

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
  swedishOemKeysMatchOfficialNativeInput \
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
