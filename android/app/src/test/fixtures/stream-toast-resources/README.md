# Stream toast current-resource check

Prepare the app's actual dependencies and model with `./gradlew :app:compileDebugKotlin :app:generateDebugLintModel` from `android/`. Model generation does not run FIR lint analysis. Then run from the repository root:

```sh
python3 android/scripts/test_stream_toast_resources.py \
  --android-home "$ANDROID_HOME" --kotlinc /path/to/kotlinc/bin/kotlinc
```

The compiler must match the app's Kotlin/plugin version. The runner uses the generated model's actual dependency jars, including Compose runtime/UI. It extracts the three production `rememberUpdatedState(stringResource(...))` declarations and actual text expressions passed to the three existing Toast calls. Pass `--source /path/to/original/OpenNowStreamSurface.kt` to test a baseline.

The host check uses real Compose composition, `LocalResources`, `stringResource` and `rememberUpdatedState`. Explicit configuration frames replace the supplied Resources object while the event callbacks remain registered once and keep the same identities. All three callback values must follow the four resource configurations. The controlled composition is explicitly invalidated/flushed; this tests the current-value contract, not Android's automatic configuration event delivery.

Android Resources returns deterministic language/key markers through a host shim, rather than loading an APK resource table. Android tracing/thread discovery, Context and callback registration are also shims. No actual Toast/window, Activity/permission result, recording flow, Android locale configuration, media session or device is executed. Baseline failure demonstrates that a retained callback can capture the old resource context under this explicit replacement model; it is not evidence that a reporter experienced stale Toast text. Source inspection and SDK compilation verify that production Toast timing, resource keys and existing owners remain unchanged.
