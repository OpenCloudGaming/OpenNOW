# Game Mode design and verification

The console shell implements the [approved Paper proposal](https://app.paper.design/file/01M4671HRE46SBPQRWAX181X7F/p-1-0).
Desktop-specific screens keep their existing presentation. Shared session state,
settings persistence, catalog operations, and native streaming remain in their existing owners.

## Component ownership

| Component | Responsibility |
| --- | --- |
| `ConsoleActionButton` and `ConsoleListRow` | Console action targets and focus treatment. |
| `ConsoleSheetFrame` | Edge-panel geometry, scrim, and one interruptible reveal. |
| `ConsoleChoiceSheet` | Saved versus focused choices, disabled options, group navigation, and cancellation. |
| `ConsoleFilterSheet` | Library filter groups and their live selections. |
| `ConsoleWarningSheet` | Safe default focus, focused-action activation, notices, and destructive confirmations. The caller owns side effects and closing. |
| `ConsoleActionColumn` | Directional navigation between actions on supporting settings pages. |
| `ConsoleStores` | Existing store marks, display names, and catalog ownership labels. |
| `LaunchStage` | Shared console layout for queue, connection, errors, conflicts, and session reports. Callers provide authoritative state. |
| `QueueAdPlayback` | One implementation of required-ad playback and reporting for both shells. |
| `GuideOverlay` | In-stream actions and nested controller/help panels, without navigating away from the native video item. |
| `ConsoleLaunchAnimation` | The live cloud-shaped reveal, entry timing, reduced-motion fallback, and startup sound cues. Main owns when it runs and which screen it reveals. |

`Main.qml` selects the appropriate confirmation UI and coordinates shell input ownership.
`OverlayHost.qml` selects console-only friends, conflict, ad, and session-report views;
the desktop fallback retains its previous views. `MotionProgress` owns reveal interruption
and reduced-motion changes without stopping a child animation inside a behavior.

## Game Mode entry

The cold entry runs once after startup settings select Game Mode. Desktop-to-console
entry uses a shorter version. Both reveal the actual destination without waiting for
the catalog, artwork, or network. Returning from a game, navigating Back, closing the
guide, and restarting the core do not replay the intro. Active sessions, queued launches,
recovery, and direct game launches bypass it.

The logo uses fixed brand colours and a vector cut-out over the live QML screen.
Its geometry stays fixed while item transforms animate. There is no video playback or
full-window screenshot texture. The normal cold sequence takes about 1.5 seconds and
quick entry takes about 0.68 seconds. Reduced motion and renderers without GPU curve
support use a crossfade instead. Losing focus, opening a modal, or leaving Game Mode
cancels the animation.

Any input can skip the intro. `InputModeTracker` consumes keyboard and pointer input
before input-mode heuristics run. `ControllerInput` tracks physical controller release,
including held sticks. The shell regains input only after the reveal and all held inputs
are released, so the skip press cannot launch the selected game.

**UI sounds** controls the original startup cues independently of reduced motion. It is
enabled by default and is available in console Settings under Themes and in desktop
Settings under Interface. Playback uses the system audio output and respects system mute.
Skip fades the cues out. A disabled preference prevents audio loading and playback.
The deterministic standard-library generator is `opennow-qt/tools/synth-launch-sounds.py`.

## Interaction contracts

- The console does not hand gameplay input to the native video item until streaming status
  includes the current attempt's first-frame metadata. A ready cloud seat is not sufficient.
- Guide panels, warning sheets, and their focus changes do not replace the native video item.
  Statistics remain a nonblocking HUD. The guide does not claim to pause the remote game.
- A/Enter activates the focused control. End-session and quit confirmations initially focus
  the safe action; there is no console-wide Enter shortcut that bypasses this focus.
- Global console warnings disable input to the underlying page and reclaim focus if a
  deferred page callback tries to take it. Background navigation keys remain blocked.
- Choice sheets open at the saved enabled value. Navigating does not save it. A commits;
  B/Escape cancels. Library filters are different: each chosen filter applies immediately,
  and B closes the filter sheet.
- Warnings retain their action target while open. Refreshing a list cannot change which
  profile, linked account, capture, or storage location the user confirms.
- The 10-bit notice appears after the setting is saved. Dismissing it does not undo the
  selection. Its opt-out remains available.
- Queue polling cannot replace an exit or application-quit confirmation. A required ad's
  existing view remains underneath the confirmation; returning to it does not recreate
  its player. Required ads have no skip action.
- A screenshot requested from the console guide closes that guide first. The root waits
  for the reveal to disappear and subsequent rendered frames before capture. A changed
  session, new overlay, lost focus, or lost media readiness cancels that pending capture.
- Home's Y action opens Library search. Friends is a Coming soon page; local controller
  setup remains available.

## Account-free verification

Build using the commands in `opennow-qt/README.md`, then run the focused controls and
session checks:

```sh
ctest --test-dir build/opennow-qt --output-on-failure \
  -R 'opennow-(consolecontrols|consoleactions|consolelayout|embedded-orchestration)-tests|qml-console-session-|qml-stream-exit-console|qml-console-initial-warning'
```

The entry-specific checks cover readiness, skipping, cancellation, resizing, held-input
release, destination preservation, and session bypass:

```sh
ctest --test-dir build/opennow-qt --output-on-failure \
  -R 'opennow-consolelaunch.*-tests|qml-console-launch-'
```

For an interactive entry capture with sample artwork, run the application without
`--reduced-motion` and add `--smoke-interactive` to the console-design workload. This
keeps the fixture open after startup instead of taking a screenshot and exiting.

The console-session workload drives queue cancellation, first-frame handoff, guide
subpages, safe confirmation, reconnect, and error recovery in windowed/fullscreen modes
with normal and reduced motion. It checks native-item identity and local/gameplay input
ownership throughout. It does not connect to a live cloud session.

Capture actual app screens with explicit public-safe sample data:

```sh
build/opennow-qt/opennow-qt --smoke-test --allow-multiple-instances \
  --console --smoke-console-design --route home \
  --smoke-width 1920 --smoke-height 1080 --reduced-motion \
  --screenshot /absolute/path/console-home.png
```

Use `library`, `store`, `game-detail`, `inserting`, `stream`, or a settings route in
place of `home`. Add `--overlay guide-session` or `--overlay desktop-stream-exit-confirm`
for the stream panels. The latter identifier is retained for routing compatibility;
console mode renders the console warning sheet.

The settings routes are `settings-account`, `settings-streaming`, `settings-video`,
`settings-input`, `settings-network`, `settings-themes`, and `settings-advanced`.
Select Recording from the settings sidebar to inspect the eighth section.
`settings-video-dropdown` opens a choice sheet. Repeat important
screens at 1280×800 and 960×540, and with `--smoke-light-theme`.

For keyboard, pointer, and motion inspection, omit `--screenshot` and add
`--smoke-interactive`. This option only keeps the app open when combined with
`--smoke-test --smoke-console-design`; the core is not started and sample settings are
not persisted. Close the window through the normal confirmation afterward.

Before release, also run the complete Qt suite and the real-account/controller acceptance
in `docs/qt-acceptance.md`. Fixture screenshots do not establish live playback quality,
hardware decoding support, or physical-controller compatibility.
