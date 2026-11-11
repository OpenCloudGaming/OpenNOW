# Edge-to-edge windows

`MainActivity.onCreate()` calls AndroidX Activity's `enableEdgeToEdge()` before
attaching Compose. Keep that compatibility setup: it configures transparent bars,
icon contrast, and display cutouts on the supported Android versions.

Google Play's deprecated-API recommendation can include dependency code. The
bundled Activity 1.13.0 implementation contains `setStatusBarColor`,
`setNavigationBarColor`, and the API 28–29 `SHORT_EDGES` fallback. Its API 30+
cutout mode is `ALWAYS`. Core 1.19.0's `WindowCompat.enableEdgeToEdge()` also
contains these compatibility calls, so migrating to it does not by itself remove
the recommendation. OpenNOW does not call the listed deprecated APIs directly.
Only Play's analysis of an uploaded release can confirm its recommendation status.

`OpenNowSystemBars.kt` owns immersive visibility transitions. It keeps decor
fitting disabled both when hiding and restoring bars, including on API 24–29.
Do not reset `systemUiVisibility` to zero when returning from streaming: that
clears the edge-to-edge layout flags and changes the available Compose viewport.

The normal app shell uses Material 3 Scaffold insets. Space UI handles display
cutouts itself; expanded stream controls and the touch editor use safe drawing
insets. Video intentionally fills the window. The activity uses `adjustResize`
for IME insets.

## Validation

Run `SystemBarsInstrumentedTest` on an older and a current Android emulator. It
checks repeated immersive hide/restore transitions and verifies that the Compose
viewport continues to fill the decor window. On API 24–29 it also checks that the
layout flags remain set after restoring system bars.

Before a Play release, check setup, catalog, Settings, queue, and expanded stream
controls with gesture and three-button navigation, portrait and landscape,
display cutouts, the keyboard, and a desktop window caption. Check stream return
and PiP transitions on physical hardware. These device checks and the Play upload
are separate from the instrumented viewport test.

References: [Android edge-to-edge setup](https://developer.android.com/develop/ui/compose/system/setup-e2e)
and [Android 15 behavior changes](https://developer.android.com/about/versions/15/behavior-changes-15#edge-to-edge).
