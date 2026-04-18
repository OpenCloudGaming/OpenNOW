package com.opencloudgaming.opennow;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Intent;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.ViewGroup;
import android.webkit.CookieManager;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import androidx.annotation.Nullable;
import androidx.appcompat.app.AppCompatActivity;

public class LoginActivity extends AppCompatActivity {
    public static final String EXTRA_AUTH_URL = "authUrl";
    public static final String EXTRA_EXPECTED_PORT = "expectedPort";
    public static final String EXTRA_TIMEOUT_MS = "timeoutMs";
    public static final String EXTRA_RESULT_CODE = "code";
    public static final String EXTRA_RESULT_ERROR = "error";
    public static final String EXTRA_RESULT_REDIRECT_URI = "redirectUri";
    public static final int RESULT_AUTH_SUCCESS = Activity.RESULT_FIRST_USER + 1;

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable timeoutRunnable = () -> finishWithError("Timed out waiting for OAuth callback");

    private WebView webView;
    private int expectedPort;
    private boolean completed;

    @Override
    protected void onCreate(@Nullable Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        String authUrl = getIntent().getStringExtra(EXTRA_AUTH_URL);
        expectedPort = getIntent().getIntExtra(EXTRA_EXPECTED_PORT, 2259);
        int timeoutMs = getIntent().getIntExtra(EXTRA_TIMEOUT_MS, 180000);

        if (authUrl == null || authUrl.isEmpty()) {
            finishWithError("Missing authUrl");
            return;
        }

        handler.postDelayed(timeoutRunnable, Math.max(1000, timeoutMs));
        webView = new WebView(this);
        webView.setLayoutParams(new ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        setContentView(webView);
        configureWebView(webView);
        webView.loadUrl(authUrl);
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void configureWebView(WebView view) {
        CookieManager cookieManager = CookieManager.getInstance();
        cookieManager.setAcceptCookie(true);
        cookieManager.setAcceptThirdPartyCookies(view, true);

        WebSettings settings = view.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setDatabaseEnabled(true);
        settings.setAllowContentAccess(true);
        settings.setAllowFileAccess(false);
        settings.setJavaScriptCanOpenWindowsAutomatically(true);
        settings.setSupportMultipleWindows(false);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);

        view.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return maybeHandleRedirect(request != null ? request.getUrl() : null);
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, String url) {
                return maybeHandleRedirect(url != null ? Uri.parse(url) : null);
            }

            @Override
            public void onPageStarted(WebView view, String url, Bitmap favicon) {
                maybeHandleRedirect(url != null ? Uri.parse(url) : null);
                super.onPageStarted(view, url, favicon);
            }
        });
    }

    private boolean maybeHandleRedirect(@Nullable Uri uri) {
        if (completed || uri == null) {
            return false;
        }

        String host = uri.getHost();
        if (!"http".equalsIgnoreCase(uri.getScheme()) || host == null || !"localhost".equalsIgnoreCase(host) || uri.getPort() != expectedPort) {
            return false;
        }

        String error = uri.getQueryParameter("error");
        String code = uri.getQueryParameter("code");
        if (code != null && !code.isEmpty()) {
            finishWithCode(code, "http://localhost:" + expectedPort);
        } else {
            finishWithError(error != null && !error.isEmpty() ? error : "Authorization failed");
        }
        return true;
    }

    private void finishWithCode(String code, String redirectUri) {
        if (completed) {
            return;
        }
        completed = true;
        handler.removeCallbacks(timeoutRunnable);
        Intent data = new Intent();
        data.putExtra(EXTRA_RESULT_CODE, code);
        data.putExtra(EXTRA_RESULT_REDIRECT_URI, redirectUri);
        setResult(RESULT_AUTH_SUCCESS, data);
        finish();
    }

    private void finishWithError(String message) {
        if (completed) {
            return;
        }
        completed = true;
        handler.removeCallbacks(timeoutRunnable);
        Intent data = new Intent();
        data.putExtra(EXTRA_RESULT_ERROR, message);
        setResult(Activity.RESULT_CANCELED, data);
        finish();
    }

    @Override
    public void onBackPressed() {
        if (webView != null && webView.canGoBack()) {
            webView.goBack();
            return;
        }
        finishWithError("Login was cancelled before the OAuth callback completed");
    }

    @Override
    protected void onDestroy() {
        handler.removeCallbacks(timeoutRunnable);
        if (webView != null) {
            webView.stopLoading();
            webView.setWebViewClient(null);
            webView.destroy();
            webView = null;
        }
        super.onDestroy();
    }
}
