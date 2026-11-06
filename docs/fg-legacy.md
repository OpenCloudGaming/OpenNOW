# Native frame generation

On Windows the OpenNOW process uses the [FlyHigh engine](https://github.com/getflyhigh/flyhigh-engine)
(`flyhigh.dll` and the `flyhigh/` model next to the executable, imported through
`FlyHighNetwork`). If that engine cannot start, frame generation turns off. It
does not fall back to block matching.

The block-matching interpolator in
`opennow-qt/src/streaming/rendering/StreamFrameInterpolator.cpp` remains for
builds where FlyHigh is not enabled: test binaries, and platforms other than the
Windows client. It uses QRhi vertex/fragment passes, including
`framegen_motion.frag` and `framegen_synthesize.frag`. The capability-test
profile key is still `motion`. `OPENNOW_FLYHIGH=0` or
`OPENNOW_FRAME_GENERATION_ENGINE=motion` forces that path in a process that
would otherwise load FlyHigh.

No Framegen v7-small neural runtime is bundled. This tree does not contain
`rt_v7s.bin` or `rt_v7s.json`.

## Interpolation and quality limits

The engine retains two source frames, builds reduced image pyramids, estimates
bidirectional block motion and detects scene cuts. For a requested factor N, it
runs synthesis at `t = k / N` for each of the N − 1 generated frames. Each generated
frame has its own render target and uniform buffer. The pacer schedules only the
frames the interpolator actually produced, followed by the current source frame.

Unreliable motion matches and scene cuts can fall back to the current source
frame. Thin edges, flat regions, occlusions and fast motion can produce artifacts
or repeated content. Higher output counters do not prove smoother visible motion;
compare matched foreground gameplay on the actual graphics backend. Capability
tests measure local cost, not cloud rendering performance or perceived quality.

## Levels, target FPS, display refresh and the capability test

`SettingsState` owns the frame-generation settings, and the Rust core validates
the following values:

| key | values | meaning |
|---|---|---|
| `frameGenerationMode` | `off`, `multiplier`, `target`, `display` | how the output rate is chosen; a profile saved before the mode existed with a level set is read as `multiplier` |
| `frameGeneration` | `off`, `2x`, `3x`, `4x` | the level in multiplier mode; in the other modes it mirrors the derived factor |
| `frameGenerationTargetFps` | 0 or 30…1000 | the requested output rate in target mode; the level is derived as `round(target / source fps)` clamped to 2…4. Display mode takes the largest allowed multiplier the display can show |
| `frameGenerationDisplayHz` | 0 or 30…1000 | 0 uses the refresh rate of the display OpenNOW is on; otherwise a manual override for displays that report nothing or the wrong value |
| `frameGenerationProfiles` | object, ≤ 64 entries | capability test measurements keyed by `engine|gpu|WxH`; which levels fit is decided live from the selected frame rate and the current display |

The source frame rate generation works against is the selected stream FPS, or for Auto the
highest rate the membership allows at the selected resolution. The pacer takes the factor
and the effective refresh rate; the display gate accepts up to 5 % overshoot
(`refresh ≥ N × source × 0.95`), and there is no fixed 120 FPS output ceiling: a 120 FPS stream on a
360 Hz display may run 2×. Only a 4 ms source interval floor remains. The engine reports how
many frames it synthesised for the pair (`generatedCount()`), and the schedule follows that
effective factor. The block-matching engine computes the pair's flow once and runs its
synthesis pass once per generated frame at `t = k / N` (previous frame warped by `t` of
the forward flow, current frame by `1 − t` of the backward flow, blended `1 − t : t`), so
2×, 3× and 4× all work; each generated frame has its own render target and uniform buffer
so the passes of one pair never clobber each other's `t`. The presenter
(`StreamVideoTextureRenderer`) keeps five external texture slots, the two history frames
plus up to three generated frames of a pair; with the earlier three slots a 3× or 4× pair
ran out of slots on its second generated frame and generation switched itself off with
`unavailable`.

### Capability test

`FrameGenerationBenchmark` (context property) runs the engine offscreen on a private QRhi of
the same backend, at the selected stream resolution, over a ring of procedural frames with
texture, a flat band and an occluding block, for about three seconds: one second per level,
2×, 3× and 4×. Per source frame it records GPU time from `QRhi::Timestamps`
(`lastCompletedGpuTime`), falling back to wall time around the blocking offscreen frame,
and reports p50/p95/max for 2× (`p95Ms`), the p95 per source frame at 3× and 4× (`p95Ms3`,
`p95Ms4`) and the GPU name (`QRhiDriverInfo`). The decision is pure:

- GPU: `cost(N) ≤ 0.6 × (1000 / fps)` ms, where `cost(N)` is the measured p95 per source
  frame at that level (profiles from before the per-level measurement fall back to
  `p95 × (N − 1)`), leaving 40 % of each source interval for decode, upscaling and
  presentation.
- Display: `refresh ≥ fps × N × 0.975` (skipped when the refresh is unknown).
- Engine: `N ≤ engineMaxFactor`, taken from the installed engine
  (`FrameGenerationBenchmark.engineMaxFactor()`), not from the stored profile, so a
  profile measured by an older engine does not cap a newer one.

The measurement is stored under the engine, GPU and resolution; the cost per source frame
does not depend on the frame rate, so one test per resolution serves every rate.
The decision is recomputed whenever the frame rate or the display changes, and the status
names the reason when nothing fits: the display cannot show fps × 2, the GPU exceeds the
budget, or the engine cannot reach the level. Until a measurement exists and at least one level
fits, the feature is off everywhere: the level and target controls are disabled,
and the stream surfaces ignore a stored level (`frameGenerationFactor()` returns 0 unless the
profile allows the requested factor). Levels and target rates the profile does not allow are
disabled with a hint, and a selection that the test rules out is reset to Off. The first-run
Boost step keeps its Off / 2× choice as a preference that takes effect after the test.

### UI

Desktop (Streaming page) and console (Video settings) show the same controls: a
**capability test** row with the status and the **Run test** button (plus a GitHub icon
that opens the Framegen repository), then one **mode** control, Off / Multiplier / Target
FPS / Display, greyed out until the test has passed, followed by the one control the mode
needs: the multiplier chips, or the output rates the display can show at 2…4× the stream
rate. Display mode needs nothing more; it reports which multiplier the display gets.
**Display refresh override** lives under Advanced: Auto shows the detected rate and only
needs changing when the platform reports nothing or the wrong value. The stats overlay
badge reads `FRAME GEN N×` while generation actually runs.

On desktop, **Run test** opens the capability dialog
(`DesktopFrameGenerationTestDialog.qml`). It measures the GPU once when no profile exists
and then shows every option at once instead of one verdict for the current preset: the
detected display rate with its override, the stream frame rate chips (entitlement-locked
rates dimmed), the mode chips, and a table with one row per stream rate and one cell per
level (2×, 3×, 4×). Each cell shows the output rate and either a check mark or the reason
it does not fit (display, GPU, engine); hovering a greyed cell gives the numbers. Clicking
a row picks the stream rate, clicking a cell picks the level (or the target rate in Target
FPS mode), and **Apply** writes `fps`, the mode and the level or target together.
`--smoke-frame-generation-dialog` seeds a 2 ms / 240 Hz measurement and opens the dialog
for screenshots. The console surface keeps the row controls.

**Mid-game control.** The Ctrl+G stream menu carries a **Frame generation** tile while the
feature is in use (on, or switched off from that menu during the session); switching it off
remembers the mode, level and target in memory and switching it back on restores them. The
same toggle is bound to the `shortcutToggleFrameGeneration` shortcut (default
`Ctrl+Shift+G`, editable with the other stream shortcuts) and reports the result in the
stream control message. Frame generation choices are written through the core like every
other setting, so they survive a restart.

**Pacer tolerance.** The presenter acquires the latest decoded frame, so a decoder or
network burst skips one source frame. `StreamFramePacer` now treats a single skipped frame
as continuous once the cadence is known (the pair around it still interpolates, the
generated frame stands in for the frame that was never shown, and the cadence estimate is
per frame); two or more skipped frames, timestamp rewinds or jumps and arrival gaps above
`max(50 ms, 2 × interval)` still re-warm. The source interval is the median of the last
eight per-frame timestamp deltas (or arrival deltas when timestamps are unusable), so one
jittery frame moves neither the slots nor the display gate. Cloud streams labelled 60 FPS
often run at 61–62 FPS (a 16.2 ms interval), so 4× asks for about 247 Hz on a 240 Hz panel:
the gate accepts up to 5 % overshoot (`refresh ≥ N × fps × 0.95`) and, when the display is
still too slow, lowers the factor for that pair instead of switching generation off; only
when even 2× does not fit is the status `display-refresh`. A
source frame that lands while the previous pair is still presenting drops that pair's
remaining schedule: bursts are normal and the newest pair wins. Only a stalled presenter
(nothing new swapped for `max(100 ms, 4 × interval)`, as the render callback reports) turns
such a frame into the two-second overload cooldown. The stats overlay names the reason
behind a `Discontinuity` (frames skipped, gap length, timestamp rewind or jump, no cadence),
and the render callback writes every frame-generation status change with the pacer's
numbers to `qt-native.log` (throttled to five lines a second). `StreamVideoItem` also runs
a 10 ms probe on the GUI thread and logs `gui-stall ms=…` whenever it runs 35 ms or more
late: frames are requested from the GUI thread, so a GUI stall drops source frames (seen
live as a `sequence-gap` of three frames once a second in a Debug build while the stats
overlay was open). Judge smoothness on a Release build.

## Attribution and redistribution

The interpolation design, pacing rules and capability test credit
[Framegen](https://github.com/MONZikWasTaken/Framegen). The MIT runtime attribution
is retained in [THIRD_PARTY_NOTICES](../THIRD_PARTY_NOTICES) and
[third_party/framegen/README.md](../third_party/framegen/README.md).

The upstream model weights have separate terms, but no such weights are
redistributed in the current Qt/native package. The retained
[weights reference](../third_party/framegen/WEIGHTS_LICENSE.md) is not a notice
that model files are bundled.
