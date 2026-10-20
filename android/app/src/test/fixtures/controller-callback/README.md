# Controller callback lifecycle regression

This host-side probe runs the resolved Android Compose runtime and compiler plugin. It extracts the production session-keyed state, client callback, toggle, mouse-assist lifecycle methods and loop predicate without rewriting their bodies.

Prepare the existing Android build dependencies with `:app:compileDebugKotlin`, then run:

```sh
python3 android/scripts/test_controller_mouse_callback.py \
  --android-home "$ANDROID_HOME" \
  --kotlinc /path/to/kotlinc/bin/kotlinc
```

The compiler must match the app's Kotlin version and include `lib/compose-compiler-plugin.jar`. The runner uses the highest cached versions of the Compose, collection, annotation and coroutine artifacts from `GRADLE_USER_HOME` (default `~/.gradle`); run in a dependency cache prepared for this checkout. Use `--android-root /path/to/old/android` to exercise an older source tree with the same fixture.

The assertions cover a screen composed before allocation, assignment of a session, replacement of a session without recreating the client, visible runtime state, disabling the mode, persisted auto-arm preference, cancellation of the existing mouse loop, and release of both held pointer buttons and stick caches. The old callback fails because it updates the forgotten pre-session state cell, leaving the visible toggle Off while the client is On.

Android tracing/main-thread discovery, the client shell, preferences, diagnostics and transport sinks are shims. The host coroutine scope uses `Dispatchers.Unconfined`, not Android's main dispatcher. Composition, state cells, callback/toggle bodies and the production mouse mode/loop/button-release logic execute. This is not a rendered AndroidTV UI, real InputDevice, full-client initialization, JNI or GFN acceptance test. The original mouse-emulation mode is outside this fixture's scope. No account credentials or network connection are needed.
