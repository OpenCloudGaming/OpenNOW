package com.opencloudgaming.opennow;

import android.graphics.Color;
import android.os.Bundle;
import android.webkit.WebView;

import androidx.core.splashscreen.SplashScreen;

import com.getcapacitor.BridgeActivity;

public class MainActivity extends BridgeActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        SplashScreen.installSplashScreen(this);
        registerPlugin(LocalhostAuthPlugin.class);
        registerPlugin(OpenNowAndroidPlugin.class);
        super.onCreate(savedInstanceState);
        getWindow().getDecorView().setBackgroundColor(Color.BLACK);
        getWindow().setStatusBarColor(Color.BLACK);
        getWindow().setNavigationBarColor(Color.BLACK);
        WebView webView = getBridge().getWebView();
        if (webView != null) {
            webView.setBackgroundColor(Color.BLACK);
        }
    }

    @Override
    public void onResume() {
        super.onResume();
        if (OpenNowAndroidPlugin.isImmersiveFullscreenRequested()) {
            OpenNowAndroidPlugin.applyImmersiveFullscreen(this, true);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus && OpenNowAndroidPlugin.isImmersiveFullscreenRequested()) {
            OpenNowAndroidPlugin.applyImmersiveFullscreen(this, true);
        }
    }
}
