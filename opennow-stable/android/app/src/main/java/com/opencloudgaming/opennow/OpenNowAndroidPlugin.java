package com.opencloudgaming.opennow;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;

import java.util.List;

import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;

@CapacitorPlugin(name = "OpenNowAndroid")
public class OpenNowAndroidPlugin extends Plugin {
    private static final String[] APP_ID_KEYS = {
        "appId",
        "app_id",
        "id",
        "launchAppId",
        "launch_app_id",
        "gfnAppId",
        "gfn_app_id",
        "nvidiaAppId",
        "nvidia_app_id",
        "cloudGameId",
        "cloud_game_id",
        "com.opencloudgaming.opennow.APP_ID",
        "com.opencloudgaming.opennow.LAUNCH_APP_ID",
        "com.opencloudgaming.opennow.extra.APP_ID",
        "com.opencloudgaming.opennow.extra.LAUNCH_APP_ID"
    };
    private static final String[] TITLE_KEYS = {
        "title",
        "name",
        "game",
        "gameTitle",
        "game_title",
        "romName",
        "rom_name",
        "label",
        "com.opencloudgaming.opennow.TITLE",
        "com.opencloudgaming.opennow.extra.TITLE",
        "com.opencloudgaming.opennow.extra.GAME_TITLE"
    };
    private static final String[] STORE_KEYS = {
        "store",
        "appStore",
        "app_store",
        "variantStore",
        "variant_store",
        "com.opencloudgaming.opennow.STORE",
        "com.opencloudgaming.opennow.extra.STORE"
    };
    private static final String[] SOURCE_KEYS = {
        "source",
        "frontend",
        "launcher",
        "caller",
        "com.opencloudgaming.opennow.SOURCE",
        "com.opencloudgaming.opennow.extra.SOURCE"
    };

    private View pointerCaptureView;
    private JSObject pendingLaunchIntent;
    private int launchIntentSequence = 0;
    private boolean pointerCaptureRequested = false;
    private final Handler pointerCaptureHandler = new Handler(Looper.getMainLooper());
    private final Runnable pointerCaptureRefreshRunnable = new Runnable() {
        @Override
        public void run() {
            if (!pointerCaptureRequested) {
                return;
            }

            View view = pointerCaptureView;
            if (view != null && view.isAttachedToWindow()) {
                requestNativePointerCapture(view);
            }
            pointerCaptureHandler.postDelayed(this, 250);
        }
    };
    private static volatile boolean immersiveFullscreenRequested = false;
    private static int previousRequestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED;
    private static boolean previousRequestedOrientationCaptured = false;

    @Override
    public void load() {
        super.load();
        pointerCaptureView = getBridge().getWebView();
        installPointerCaptureListener(pointerCaptureView);
    }

    @PluginMethod
    public void setImmersiveFullscreen(PluginCall call) {
        boolean enabled = call.getBoolean("enabled", false);
        getBridge().executeOnMainThread(() -> {
            applyImmersiveFullscreen(enabled);
            JSObject payload = new JSObject();
            payload.put("enabled", enabled);
            call.resolve(payload);
        });
    }

    @PluginMethod
    public void setPointerCapture(PluginCall call) {
        boolean enabled = call.getBoolean("enabled", false);
        getBridge().executeOnMainThread(() -> {
            JSObject payload = new JSObject();
            payload.put("supported", Build.VERSION.SDK_INT >= Build.VERSION_CODES.O);

            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
                payload.put("enabled", false);
                call.resolve(payload);
                return;
            }

            View view = pointerCaptureView != null ? pointerCaptureView : getBridge().getWebView();
            installPointerCaptureListener(view);
            pointerCaptureRequested = enabled;

            if (enabled) {
                requestNativePointerCapture(view);
                schedulePointerCaptureRefresh();
            } else if (view.hasPointerCapture()) {
                stopPointerCaptureRefresh();
                view.releasePointerCapture();
            } else {
                stopPointerCaptureRefresh();
            }

            payload.put("enabled", view.hasPointerCapture());
            call.resolve(payload);
        });
    }

    @PluginMethod
    public void consumeLaunchIntent(PluginCall call) {
        JSObject payload = new JSObject();
        payload.put("intent", pendingLaunchIntent);
        pendingLaunchIntent = null;
        call.resolve(payload);
    }

    @Override
    protected void handleOnNewIntent(Intent intent) {
        super.handleOnNewIntent(intent);
        JSObject payload = parseLaunchIntent(intent);
        if (payload == null) {
            return;
        }

        pendingLaunchIntent = payload;
        notifyListeners("launchIntent", payload, true);
    }

    private void installPointerCaptureListener(View view) {
        if (view == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }

        pointerCaptureView = view;
        view.setOnCapturedPointerListener((capturedView, event) -> {
            if (!isSupportedPointerSource(event.getSource())) {
                return false;
            }

            int action = event.getActionMasked();
            long timestampMs = event.getEventTime();

            if (action == MotionEvent.ACTION_HOVER_MOVE || action == MotionEvent.ACTION_MOVE) {
                float dx = event.getAxisValue(MotionEvent.AXIS_RELATIVE_X);
                float dy = event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y);
                if (dx == 0f && dy == 0f) {
                    return false;
                }

                JSObject payload = new JSObject();
                payload.put("dx", dx);
                payload.put("dy", dy);
                payload.put("timestampMs", timestampMs);
                notifyListeners("nativeMouseMove", payload);
                return true;
            }

            if (action == MotionEvent.ACTION_BUTTON_PRESS || action == MotionEvent.ACTION_BUTTON_RELEASE) {
                int button = mapMouseButton(event.getActionButton());
                if (button < 0) {
                    return false;
                }

                JSObject payload = new JSObject();
                payload.put("button", button);
                payload.put("pressed", action == MotionEvent.ACTION_BUTTON_PRESS);
                payload.put("timestampMs", timestampMs);
                notifyListeners("nativeMouseButton", payload);
                return true;
            }

            if (action == MotionEvent.ACTION_SCROLL) {
                float vertical = event.getAxisValue(MotionEvent.AXIS_VSCROLL);
                if (vertical == 0f) {
                    return false;
                }

                JSObject payload = new JSObject();
                payload.put("delta", Math.round(vertical * 120f));
                payload.put("timestampMs", timestampMs);
                notifyListeners("nativeMouseWheel", payload);
                return true;
            }

            return false;
        });
    }

    private int mapMouseButton(int actionButton) {
        switch (actionButton) {
            case MotionEvent.BUTTON_PRIMARY:
                return 0;
            case MotionEvent.BUTTON_TERTIARY:
                return 1;
            case MotionEvent.BUTTON_SECONDARY:
                return 2;
            case MotionEvent.BUTTON_BACK:
                return 3;
            case MotionEvent.BUTTON_FORWARD:
                return 4;
            default:
                return -1;
        }
    }

    private boolean isSupportedPointerSource(int source) {
        if ((source & InputDevice.SOURCE_MOUSE) == InputDevice.SOURCE_MOUSE) {
            return true;
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
            && (source & InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE) {
            return true;
        }
        // Android exposes some controller touchpads as SOURCE_TOUCHPAD. Capturing
        // those as mice can interfere with the normal WebView gamepad path.
        return false;
    }

    private void requestNativePointerCapture(View view) {
        if (view == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }

        if (view.hasPointerCapture()) {
            return;
        }

        view.setFocusable(true);
        view.setFocusableInTouchMode(true);
        view.requestFocus();
        view.requestPointerCapture();
    }

    private void schedulePointerCaptureRefresh() {
        pointerCaptureHandler.removeCallbacks(pointerCaptureRefreshRunnable);
        pointerCaptureHandler.postDelayed(pointerCaptureRefreshRunnable, 250);
    }

    private void stopPointerCaptureRefresh() {
        pointerCaptureHandler.removeCallbacks(pointerCaptureRefreshRunnable);
    }

    private JSObject parseLaunchIntent(Intent intent) {
        if (intent == null) {
            return null;
        }

        Uri data = intent.getData();
        Bundle extras = intent.getExtras();
        String appId = normalizeAppId(firstNonEmpty(firstExtra(extras, APP_ID_KEYS), firstUriValue(data, APP_ID_KEYS)));
        if (appId == null) {
            appId = firstNumericUriPart(data);
        }

        String title = firstNonEmpty(firstExtra(extras, TITLE_KEYS), firstUriValue(data, TITLE_KEYS));
        String store = firstNonEmpty(firstExtra(extras, STORE_KEYS), firstUriValue(data, STORE_KEYS));
        String source = firstNonEmpty(firstExtra(extras, SOURCE_KEYS), firstUriValue(data, SOURCE_KEYS));
        String action = trimToNull(intent.getAction());
        String dataString = trimToNull(intent.getDataString());

        if (appId == null && title == null && store == null && source == null && dataString == null) {
            return null;
        }

        JSObject payload = new JSObject();
        payload.put("sequence", ++launchIntentSequence);
        payload.put("receivedAtMs", System.currentTimeMillis());
        putIfPresent(payload, "action", action);
        putIfPresent(payload, "data", dataString);
        putIfPresent(payload, "appId", appId);
        putIfPresent(payload, "title", title);
        putIfPresent(payload, "store", store);
        putIfPresent(payload, "source", source);
        return payload;
    }

    private String firstUriValue(Uri uri, String[] keys) {
        if (uri == null) {
            return null;
        }

        for (String key : keys) {
            String value = queryParameter(uri, key);
            if (value != null) {
                return value;
            }
        }
        return null;
    }

    private String queryParameter(Uri uri, String key) {
        try {
            return trimToNull(uri.getQueryParameter(key));
        } catch (UnsupportedOperationException ignored) {
            return null;
        }
    }

    private String firstNumericUriPart(Uri uri) {
        if (uri == null) {
            return null;
        }

        String host = normalizeAppId(uri.getHost());
        if (host != null) {
            return host;
        }

        List<String> segments = uri.getPathSegments();
        for (String segment : segments) {
            String normalized = normalizeAppId(segment);
            if (normalized != null) {
                return normalized;
            }
        }
        return null;
    }

    private String firstExtra(Bundle extras, String[] keys) {
        if (extras == null) {
            return null;
        }

        for (String key : keys) {
            if (!extras.containsKey(key)) {
                continue;
            }
            String value = valueToString(extras.get(key));
            if (value != null) {
                return value;
            }
        }
        return null;
    }

    private String valueToString(Object value) {
        if (value instanceof String || value instanceof Number || value instanceof Boolean) {
            return trimToNull(String.valueOf(value));
        }
        return null;
    }

    private String firstNonEmpty(String left, String right) {
        return left != null ? left : right;
    }

    private String trimToNull(String value) {
        if (value == null) {
            return null;
        }
        String trimmed = value.trim();
        return trimmed.isEmpty() ? null : trimmed;
    }

    private String normalizeAppId(String value) {
        String trimmed = trimToNull(value);
        if (trimmed == null || !trimmed.matches("^[0-9]+$")) {
            return null;
        }
        try {
            return Long.parseLong(trimmed) > 0L ? trimmed : null;
        } catch (NumberFormatException ignored) {
            return null;
        }
    }

    private void putIfPresent(JSObject payload, String key, String value) {
        if (value != null) {
            payload.put(key, value);
        }
    }

    private void applyImmersiveFullscreen(boolean enabled) {
        Activity activity = getActivity();
        if (activity == null) {
            return;
        }

        immersiveFullscreenRequested = enabled;
        applyImmersiveFullscreen(activity, enabled);
    }

    public static boolean isImmersiveFullscreenRequested() {
        return immersiveFullscreenRequested;
    }

    public static void applyImmersiveFullscreen(Activity activity, boolean enabled) {
        if (activity == null) {
            return;
        }

        Window window = activity.getWindow();
        View decorView = window.getDecorView();
        WindowInsetsControllerCompat controller = WindowCompat.getInsetsController(window, decorView);
        WindowManager.LayoutParams attributes = window.getAttributes();

        decorView.setBackgroundColor(Color.BLACK);
        window.setStatusBarColor(Color.BLACK);
        window.setNavigationBarColor(Color.BLACK);

        WindowCompat.setDecorFitsSystemWindows(window, !enabled);
        if (enabled) {
            if (!previousRequestedOrientationCaptured) {
                previousRequestedOrientation = activity.getRequestedOrientation();
                previousRequestedOrientationCaptured = true;
            }
            activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                window.setAttributes(attributes);
            }
            window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN);
            controller.setSystemBarsBehavior(
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            );
            controller.hide(WindowInsetsCompat.Type.statusBars() | WindowInsetsCompat.Type.navigationBars());
            decorView.setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    | View.SYSTEM_UI_FLAG_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
            );
            return;
        }

        if (previousRequestedOrientationCaptured) {
            activity.setRequestedOrientation(previousRequestedOrientation);
            previousRequestedOrientationCaptured = false;
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            attributes.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT;
            window.setAttributes(attributes);
        }
        window.clearFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN);
        controller.show(WindowInsetsCompat.Type.statusBars() | WindowInsetsCompat.Type.navigationBars());
        decorView.setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
    }
}
