# Qt motion audit

The Desktop Renew motion pass fixes these ownership/lifetime problems:

- `Main.qml` faded the whole route loader out and back in on every navigation,
  including desktop modal routes. The shell and native presenter now stay opaque.
- Details, command palette, context/provider/filter menus and shared overlays
  retain their content through dismissal. `MotionProgress` owns one interruptible
  0–1 progress value; opacity and a small centered/edge-anchored zoom derive from it.
  Reversing an animation starts at its current value, without `from` resets or
  overshoot. Input is disabled as soon as dismissal begins.
- The stream menu had Behaviors, an initial animation and a close animation all
  writing the same properties. It now uses the shared progress owner, and actions
  dispatch on close completion instead of an independent timer. Menu/confirmation
  visuals can finish closing without retaining gameplay-input ownership.
- Desktop poster hover animations now transform only the visual child. Their
  hit-test rectangles stay stationary, and their stacking order remains raised
  until zoom-out finishes.
- Retained artwork no longer clears its source when hidden. Changing the artwork
  identity while hidden still resets deferred loading, keeping recycled items
  from requesting unseen replacement assets.
- The session-start reveal no longer fades in and then jumps down before moving
  up. Translation and opacity now share the same timeline. Reduced motion also
  suppresses hover/press zooms and the store pagination animation.

Existing fixed-size filter/platform/keyboard popups already retain their content
through close and use non-overshooting, retargetable Behaviors; those stay intact.
The mode-switch curtain and intentional waiting/status pulses remain distinct
from popup transforms. All `OutBack` popup overshoot and `visible ? opacity`
close-cutoff patterns have been removed from the QML tree.

The progress value intentionally uses `NumberAnimation`: visibility, close
completion and input lifetime must observe its intermediate value. Qt's
[Animator types](https://doc.qt.io/qt-6/qml-qtquick-animator.html) update the scene
graph directly and only commit QML properties at completion, so they are not a
drop-in replacement for this lifecycle owner. No full-window texture/layer or
second stream presenter is introduced.

## Regression checks

`qml-motion-{normal,reduced}-{windowed,fullscreen}` runs production UI through
normal, interrupted and repeated open/close cycles. It checks bounded/monotonic
progress, intermediate frames versus immediate reduced-motion changes, retained
page/presenter identity, fullscreen state, close completion and absence of a
second overlay dimming layer. QML warnings fail the run.

```sh
ctest --test-dir build/opennow-qt -R 'qml-motion|qml-fullscreen' --output-on-failure
```

The same sequence is available with `--smoke-test --desktop --smoke-paper-design
--smoke-motion`, optionally `--smoke-motion-fullscreen` and `--reduced-motion`.
These runs do not start the account core or launch a real stream. Live gameplay
and device-specific frame pacing still require their own hardware validation.

## Windows verification (2026-09-04)

- Qt 6.11.2 MinGW build succeeds; all 108 Qt/acceptance tests pass.
- The new motion sequence also passes in visible windowed and fullscreen runs
  using the Windows hardware renderer, not only the offscreen test backend.
- At 1440×900 on the 165 Hz display, the hardware performance run passes its
  frame-pacing checks: route p95 frame interval 7.19 ms, popup p95 6.25 ms.
  Popup p95 first-frame latency is 16.10 ms (previous build: 17.72 ms).
- **Remaining failure:** `route p95 first-frame latency exceeded budget`.
  Cold Library/Settings creation takes 42.30/46.60 ms against a 36.35 ms budget.
  The previous build also fails this check (route p95 44.01 ms). Details opens
  in 2.70 ms in the new run. This is not a passing overall performance report;
  cold route construction remains separate follow-up work.

The failing hardware command (run with a visible native window) was:

```powershell
C:/tmp/opennow-renew-d960/OpenNOW.exe --smoke-test --allow-multiple-instances --desktop --performance-report C:/Users/Zortos/.codex/worktrees/d960/OpenNOW/build/opennow-qt/motion-hardware-after.json --performance-width 1440 --performance-height 900 --performance-cycles 1 --performance-label motion-after --performance-require-hardware
```

Before/after reports are kept locally in `build/opennow-qt/motion-hardware-before.json`
and `build/opennow-qt/motion-hardware-after.json`. No live account or stream was used.

## Store row navigation follow-up

Store hover previously called the keyboard focus-and-scroll function. As a row
scrolled beneath a stationary pointer, the newly hovered row started another
scroll, creating an up/down feedback loop. Hover now only selects; it is ignored
while the viewport or navigation animation is moving. It does not steal keyboard
focus from search. Keyboard/controller row navigation reveals only the required
part of a row, retargets from the current viewport on reversal, and cancels flick
inertia. Direct viewport movement stops the navigation animation. Shelf lookup
uses the row identity rather than assuming a visible hero occupies the first slot.

`qml-store-navigation-{960,1440}-{normal,reduced}` exercises the real Store shelf
hover signal, key delivery and scroll animation using the isolated paging fixture.
It checks monotonic movement despite hover events, no scrolling from idle hover,
rapid up/down reversal, selected-row visibility, return to the hero, and manual
viewport movement taking ownership. The standalone command is `OpenNOW.exe
--smoke-test --allow-multiple-instances --desktop --route store
--smoke-store-paging --smoke-store-navigation`; add `--reduced-motion` as needed.

Verified with Qt 6.11.2: all 115 Qt/acceptance tests pass. The Store navigation
sequence also passes in visible Windows-rendered windows at 960×900 and
1440×900, without QML warnings. These are interaction checks, not a new
frame-pacing benchmark; the cold-route limitation above is unchanged.
