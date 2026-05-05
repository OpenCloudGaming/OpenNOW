package com.opencloudgaming.opennow;

import android.app.Activity;
import android.os.Build;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.view.View;
import android.view.Window;

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
    private View pointerCaptureView;

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
            view.setFocusable(true);
            view.setFocusableInTouchMode(true);
            view.requestFocus();

            if (enabled) {
                view.requestPointerCapture();
            } else if (view.hasPointerCapture()) {
                view.releasePointerCapture();
            }

            payload.put("enabled", view.hasPointerCapture());
            call.resolve(payload);
        });
    }

    private void installPointerCaptureListener(View view) {
        if (view == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }

        pointerCaptureView = view;
        view.setOnCapturedPointerListener((capturedView, event) -> {
            if ((event.getSource() & InputDevice.SOURCE_MOUSE) != InputDevice.SOURCE_MOUSE) {
                return false;
            }

            int action = event.getActionMasked();
            if (action != MotionEvent.ACTION_HOVER_MOVE && action != MotionEvent.ACTION_MOVE) {
                return false;
            }

            float dx = event.getAxisValue(MotionEvent.AXIS_RELATIVE_X);
            float dy = event.getAxisValue(MotionEvent.AXIS_RELATIVE_Y);
            if (dx == 0f && dy == 0f) {
                return false;
            }

            JSObject payload = new JSObject();
            payload.put("dx", dx);
            payload.put("dy", dy);
            notifyListeners("nativeMouseMove", payload);
            return true;
        });
    }

    private void applyImmersiveFullscreen(boolean enabled) {
        Activity activity = getActivity();
        if (activity == null) {
            return;
        }

        Window window = activity.getWindow();
        View decorView = window.getDecorView();
        WindowInsetsControllerCompat controller = WindowCompat.getInsetsController(window, decorView);

        WindowCompat.setDecorFitsSystemWindows(window, !enabled);
        if (enabled) {
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

        controller.show(WindowInsetsCompat.Type.statusBars() | WindowInsetsCompat.Type.navigationBars());
        decorView.setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
    }
}
