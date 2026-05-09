package com.opencloudgaming.opennow;

import android.app.Activity;
import android.app.ActivityManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;
import android.widget.FrameLayout;

import java.util.ArrayList;
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
    private NativeTouchControllerView nativeTouchControllerView;
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

    @PluginMethod
    public void getPerformanceInfo(PluginCall call) {
        Activity activity = getActivity();
        JSObject payload = new JSObject();
        ActivityManager.MemoryInfo memoryInfo = readMemoryInfo(activity);
        if (memoryInfo != null) {
            payload.put("totalMemBytes", memoryInfo.totalMem);
            payload.put("availMemBytes", memoryInfo.availMem);
            payload.put("thresholdBytes", memoryInfo.threshold);
            payload.put("lowMemory", memoryInfo.lowMemory);
            payload.put("liteTouchRecommended", memoryInfo.totalMem > 0 && memoryInfo.totalMem < 3L * 1024L * 1024L * 1024L);
        } else {
            payload.put("liteTouchRecommended", false);
        }
        call.resolve(payload);
    }

    @PluginMethod
    public void setNativeTouchControls(PluginCall call) {
        boolean enabled = call.getBoolean("enabled", false);
        Double sizeValue = call.getDouble("size", 1.0);
        Double opacityValue = call.getDouble("opacity", 1.0);
        String placement = call.getString("placement", "default");
        float size = clampFloat(sizeValue != null ? sizeValue.floatValue() : 1f, 0.72f, 1.35f);
        float opacity = clampFloat(opacityValue != null ? opacityValue.floatValue() : 1f, 0.25f, 1f);

        getBridge().executeOnMainThread(() -> {
            if (enabled) {
                installNativeTouchController(size, opacity, placement);
            } else {
                removeNativeTouchController();
            }

            JSObject payload = new JSObject();
            payload.put("enabled", nativeTouchControllerView != null && nativeTouchControllerView.isAttachedToWindow());
            call.resolve(payload);
        });
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

    private void installNativeTouchController(float size, float opacity, String placement) {
        Activity activity = getActivity();
        if (activity == null) {
            return;
        }

        ViewGroup root = activity.findViewById(android.R.id.content);
        if (root == null) {
            return;
        }

        if (nativeTouchControllerView == null) {
            nativeTouchControllerView = new NativeTouchControllerView(activity, this::emitNativeTouchGamepadState);
            FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            );
            root.addView(nativeTouchControllerView, params);
        }
        nativeTouchControllerView.configure(size, opacity, placement);
        nativeTouchControllerView.bringToFront();
    }

    private void removeNativeTouchController() {
        NativeTouchControllerView view = nativeTouchControllerView;
        if (view == null) {
            return;
        }

        view.disconnect();
        ViewGroup parent = view.getParent() instanceof ViewGroup ? (ViewGroup) view.getParent() : null;
        if (parent != null) {
            parent.removeView(view);
        }
        nativeTouchControllerView = null;
    }

    private void emitNativeTouchGamepadState(
        boolean connected,
        int buttons,
        float leftTrigger,
        float rightTrigger,
        float leftStickX,
        float leftStickY,
        float rightStickX,
        float rightStickY,
        long timestampMs
    ) {
        JSObject payload = new JSObject();
        payload.put("connected", connected);
        payload.put("buttons", buttons);
        payload.put("leftTrigger", leftTrigger);
        payload.put("rightTrigger", rightTrigger);
        payload.put("leftStickX", leftStickX);
        payload.put("leftStickY", leftStickY);
        payload.put("rightStickX", rightStickX);
        payload.put("rightStickY", rightStickY);
        payload.put("timestampMs", timestampMs);
        notifyListeners("nativeTouchGamepad", payload);
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

    private ActivityManager.MemoryInfo readMemoryInfo(Activity activity) {
        if (activity == null) {
            return null;
        }
        Object service = activity.getSystemService(Context.ACTIVITY_SERVICE);
        if (!(service instanceof ActivityManager)) {
            return null;
        }
        ActivityManager.MemoryInfo memoryInfo = new ActivityManager.MemoryInfo();
        ((ActivityManager) service).getMemoryInfo(memoryInfo);
        return memoryInfo;
    }

    private static float clampFloat(float value, float min, float max) {
        if (Float.isNaN(value) || Float.isInfinite(value)) {
            return min;
        }
        return Math.max(min, Math.min(max, value));
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

    private interface NativeTouchStateListener {
        void onState(
            boolean connected,
            int buttons,
            float leftTrigger,
            float rightTrigger,
            float leftStickX,
            float leftStickY,
            float rightStickX,
            float rightStickY,
            long timestampMs
        );
    }

    private static final class NativeTouchControllerView extends View {
        private static final int GAMEPAD_DPAD_UP = 0x0001;
        private static final int GAMEPAD_DPAD_DOWN = 0x0002;
        private static final int GAMEPAD_DPAD_LEFT = 0x0004;
        private static final int GAMEPAD_DPAD_RIGHT = 0x0008;
        private static final int GAMEPAD_START = 0x0010;
        private static final int GAMEPAD_BACK = 0x0020;
        private static final int GAMEPAD_LB = 0x0100;
        private static final int GAMEPAD_RB = 0x0200;
        private static final int GAMEPAD_A = 0x1000;
        private static final int GAMEPAD_B = 0x2000;
        private static final int GAMEPAD_X = 0x4000;
        private static final int GAMEPAD_Y = 0x8000;
        private static final int KIND_BUTTON = 1;
        private static final int KIND_LEFT_TRIGGER = 2;
        private static final int KIND_RIGHT_TRIGGER = 3;
        private static final int KIND_LEFT_STICK = 4;
        private static final int KIND_RIGHT_STICK = 5;
        private static final long MIN_EMIT_INTERVAL_MS = 33L;

        private final NativeTouchStateListener listener;
        private final Paint fillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint strokePaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint textPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint thumbPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final List<TouchRegion> regions = new ArrayList<>();
        private final RectF leftStickRect = new RectF();
        private final RectF rightStickRect = new RectF();
        private float size = 1f;
        private float opacity = 1f;
        private String placement = "default";
        private int buttons = 0;
        private float leftTrigger = 0f;
        private float rightTrigger = 0f;
        private float leftStickX = 0f;
        private float leftStickY = 0f;
        private float rightStickX = 0f;
        private float rightStickY = 0f;
        private int lastButtons = -1;
        private float lastLeftTrigger = -1f;
        private float lastRightTrigger = -1f;
        private float lastLeftStickX = -2f;
        private float lastLeftStickY = -2f;
        private float lastRightStickX = -2f;
        private float lastRightStickY = -2f;
        private boolean connected = false;
        private long lastEmitTimestampMs = 0L;

        NativeTouchControllerView(Context context, NativeTouchStateListener listener) {
            super(context);
            this.listener = listener;
            setWillNotDraw(false);
            setBackgroundColor(Color.TRANSPARENT);
            setFocusable(false);
            setClickable(false);
            setLayerType(View.LAYER_TYPE_SOFTWARE, null);

            fillPaint.setStyle(Paint.Style.FILL);
            strokePaint.setStyle(Paint.Style.STROKE);
            strokePaint.setStrokeWidth(dp(1.5f));
            textPaint.setColor(Color.WHITE);
            textPaint.setTextAlign(Paint.Align.CENTER);
            textPaint.setTypeface(Typeface.create(Typeface.DEFAULT, Typeface.BOLD));
            thumbPaint.setStyle(Paint.Style.FILL);
        }

        void configure(float size, float opacity, String placement) {
            this.size = clampFloat(size, 0.72f, 1.35f);
            this.opacity = clampFloat(opacity, 0.25f, 1f);
            this.placement = placement != null ? placement : "default";
            layoutControls(getWidth(), getHeight());
            invalidate();
        }

        void disconnect() {
            resetState();
            emitState(true, System.currentTimeMillis(), false);
        }

        @Override
        protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
            super.onSizeChanged(width, height, oldWidth, oldHeight);
            layoutControls(width, height);
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            int alpha = Math.round(255f * Math.max(0.92f, opacity));
            fillPaint.setColor(Color.argb(alpha, 13, 17, 21));
            strokePaint.setColor(Color.argb(128, 255, 255, 255));
            thumbPaint.setColor(Color.argb(235, 214, 222, 230));
            textPaint.setTextSize(dp(13f) * size);

            for (TouchRegion region : regions) {
                float radius = region.kind == KIND_LEFT_STICK || region.kind == KIND_RIGHT_STICK
                    ? region.rect.width() / 2f
                    : dp(10f) * size;
                canvas.drawRoundRect(region.rect, radius, radius, fillPaint);
                canvas.drawRoundRect(region.rect, radius, radius, strokePaint);
                if (region.label != null) {
                    Paint.FontMetrics metrics = textPaint.getFontMetrics();
                    float baseline = region.rect.centerY() - (metrics.ascent + metrics.descent) / 2f;
                    canvas.drawText(region.label, region.rect.centerX(), baseline, textPaint);
                }
            }

            drawStickThumb(canvas, leftStickRect, leftStickX, leftStickY);
            drawStickThumb(canvas, rightStickRect, rightStickX, rightStickY);
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            if (event == null || regions.isEmpty()) {
                return false;
            }

            int action = event.getActionMasked();
            if (action == MotionEvent.ACTION_DOWN && hitTest(event.getX(), event.getY()) == null) {
                return false;
            }

            long timestampMs = event.getEventTime();
            if (action == MotionEvent.ACTION_CANCEL) {
                resetState();
                emitState(true, timestampMs, false);
                invalidate();
                return true;
            }

            resetState();
            int actionIndex = event.getActionIndex();
            for (int i = 0; i < event.getPointerCount(); i++) {
                if ((action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP) && i == actionIndex) {
                    continue;
                }
                applyPointer(event.getX(i), event.getY(i));
            }

            boolean active = isActive();
            boolean wasConnected = connected;
            connected = active || wasConnected;
            boolean force = action == MotionEvent.ACTION_DOWN
                || action == MotionEvent.ACTION_POINTER_DOWN
                || action == MotionEvent.ACTION_UP
                || action == MotionEvent.ACTION_POINTER_UP;
            emitState(force, timestampMs, connected);
            if (!active && (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP)) {
                connected = false;
            }
            invalidate();
            return true;
        }

        private void layoutControls(int width, int height) {
            regions.clear();
            if (width <= 0 || height <= 0) {
                return;
            }

            float margin = dp(24f) * size;
            float bottom = height - dp("lower".equals(placement) ? 24f : 34f) * size;
            float stick = dp("compact".equals(placement) ? 92f : 108f) * size;
            float button = dp("compact".equals(placement) ? 48f : 56f) * size;
            float gap = dp(10f) * size;
            float shoulderWidth = dp(72f) * size;
            float shoulderHeight = dp(42f) * size;

            leftStickRect.set(margin, bottom - stick, margin + stick, bottom);
            rightStickRect.set(width - margin - stick, bottom - stick, width - margin, bottom);
            regions.add(new TouchRegion(leftStickRect, KIND_LEFT_STICK, 0, null));
            regions.add(new TouchRegion(rightStickRect, KIND_RIGHT_STICK, 0, null));

            float dpadLeft = leftStickRect.right + gap;
            float dpadTop = leftStickRect.top + (stick - button * 2f - gap) / 2f;
            addButton(dpadLeft + button + gap, dpadTop, button, button, GAMEPAD_DPAD_UP, "U");
            addButton(dpadLeft, dpadTop + button + gap, button, button, GAMEPAD_DPAD_LEFT, "L");
            addButton(dpadLeft + (button + gap) * 2f, dpadTop + button + gap, button, button, GAMEPAD_DPAD_RIGHT, "R");
            addButton(dpadLeft + button + gap, dpadTop + (button + gap) * 2f, button, button, GAMEPAD_DPAD_DOWN, "D");

            float faceLeft = rightStickRect.left - gap - button * 3f - gap * 2f;
            float faceTop = rightStickRect.top + (stick - button * 2f - gap) / 2f;
            addButton(faceLeft + button + gap, faceTop, button, button, GAMEPAD_Y, "Y");
            addButton(faceLeft, faceTop + button + gap, button, button, GAMEPAD_X, "X");
            addButton(faceLeft + (button + gap) * 2f, faceTop + button + gap, button, button, GAMEPAD_B, "B");
            addButton(faceLeft + button + gap, faceTop + (button + gap) * 2f, button, button, GAMEPAD_A, "A");

            float centerY = bottom - stick - dp(58f) * size;
            float centerButtonWidth = dp(74f) * size;
            float centerLeft = width / 2f - centerButtonWidth - gap / 2f;
            addButton(centerLeft, centerY, centerButtonWidth, shoulderHeight, GAMEPAD_BACK, "View");
            addButton(centerLeft + centerButtonWidth + gap, centerY, centerButtonWidth, shoulderHeight, GAMEPAD_START, "Menu");

            float shoulderTop = dp(18f) * size;
            addButton(margin, shoulderTop, shoulderWidth, shoulderHeight, GAMEPAD_LB, "LB");
            addTrigger(margin + shoulderWidth + gap, shoulderTop, shoulderWidth, shoulderHeight, true, "LT");
            addTrigger(width - margin - shoulderWidth * 2f - gap, shoulderTop, shoulderWidth, shoulderHeight, false, "RT");
            addButton(width - margin - shoulderWidth, shoulderTop, shoulderWidth, shoulderHeight, GAMEPAD_RB, "RB");
        }

        private void addButton(float left, float top, float width, float height, int buttonMask, String label) {
            regions.add(new TouchRegion(new RectF(left, top, left + width, top + height), KIND_BUTTON, buttonMask, label));
        }

        private void addTrigger(float left, float top, float width, float height, boolean leftTrigger, String label) {
            regions.add(new TouchRegion(
                new RectF(left, top, left + width, top + height),
                leftTrigger ? KIND_LEFT_TRIGGER : KIND_RIGHT_TRIGGER,
                0,
                label
            ));
        }

        private void drawStickThumb(Canvas canvas, RectF stickRect, float x, float y) {
            if (stickRect.isEmpty()) {
                return;
            }
            float radius = stickRect.width() * 0.16f;
            float travel = stickRect.width() * 0.28f;
            canvas.drawCircle(stickRect.centerX() + x * travel, stickRect.centerY() + y * travel, radius, thumbPaint);
        }

        private void applyPointer(float x, float y) {
            TouchRegion region = hitTest(x, y);
            if (region == null) {
                return;
            }

            switch (region.kind) {
                case KIND_BUTTON:
                    buttons |= region.buttonMask;
                    break;
                case KIND_LEFT_TRIGGER:
                    leftTrigger = 1f;
                    break;
                case KIND_RIGHT_TRIGGER:
                    rightTrigger = 1f;
                    break;
                case KIND_LEFT_STICK:
                    float[] left = stickValue(region.rect, x, y);
                    leftStickX = left[0];
                    leftStickY = left[1];
                    break;
                case KIND_RIGHT_STICK:
                    float[] right = stickValue(region.rect, x, y);
                    rightStickX = right[0];
                    rightStickY = right[1];
                    break;
                default:
                    break;
            }
        }

        private TouchRegion hitTest(float x, float y) {
            for (TouchRegion region : regions) {
                if (region.rect.contains(x, y)) {
                    return region;
                }
            }
            return null;
        }

        private float[] stickValue(RectF rect, float x, float y) {
            float rawX = (x - rect.centerX()) / Math.max(1f, rect.width() / 2f);
            float rawY = (y - rect.centerY()) / Math.max(1f, rect.height() / 2f);
            float magnitude = (float) Math.sqrt(rawX * rawX + rawY * rawY);
            float scale = magnitude > 1f ? 1f / magnitude : 1f;
            return new float[] {
                quantize(rawX * scale),
                quantize(rawY * scale)
            };
        }

        private float quantize(float value) {
            return Math.round(clampFloat(value, -1f, 1f) * 100f) / 100f;
        }

        private void resetState() {
            buttons = 0;
            leftTrigger = 0f;
            rightTrigger = 0f;
            leftStickX = 0f;
            leftStickY = 0f;
            rightStickX = 0f;
            rightStickY = 0f;
        }

        private boolean isActive() {
            return buttons != 0
                || leftTrigger > 0f
                || rightTrigger > 0f
                || leftStickX != 0f
                || leftStickY != 0f
                || rightStickX != 0f
                || rightStickY != 0f;
        }

        private void emitState(boolean force, long timestampMs, boolean connectedValue) {
            boolean changed = force
                || buttons != lastButtons
                || leftTrigger != lastLeftTrigger
                || rightTrigger != lastRightTrigger
                || leftStickX != lastLeftStickX
                || leftStickY != lastLeftStickY
                || rightStickX != lastRightStickX
                || rightStickY != lastRightStickY;
            if (!changed) {
                return;
            }
            if (!force && timestampMs - lastEmitTimestampMs < MIN_EMIT_INTERVAL_MS) {
                return;
            }

            lastEmitTimestampMs = timestampMs;
            lastButtons = buttons;
            lastLeftTrigger = leftTrigger;
            lastRightTrigger = rightTrigger;
            lastLeftStickX = leftStickX;
            lastLeftStickY = leftStickY;
            lastRightStickX = rightStickX;
            lastRightStickY = rightStickY;
            listener.onState(
                connectedValue,
                buttons,
                leftTrigger,
                rightTrigger,
                leftStickX,
                leftStickY,
                rightStickX,
                rightStickY,
                timestampMs
            );
        }

        private float dp(float value) {
            return value * getResources().getDisplayMetrics().density;
        }

        private static final class TouchRegion {
            final RectF rect;
            final int kind;
            final int buttonMask;
            final String label;

            TouchRegion(RectF rect, int kind, int buttonMask, String label) {
                this.rect = new RectF(rect);
                this.kind = kind;
                this.buttonMask = buttonMask;
                this.label = label;
            }
        }
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
