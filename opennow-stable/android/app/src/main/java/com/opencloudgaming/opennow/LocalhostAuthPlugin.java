package com.opencloudgaming.opennow;

import android.app.Activity;
import android.content.Intent;
import android.os.SystemClock;

import androidx.activity.result.ActivityResult;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.ActivityCallback;
import com.getcapacitor.annotation.CapacitorPlugin;

import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.URL;
import java.util.ArrayList;
import java.util.List;

@CapacitorPlugin(name = "LocalhostAuth")
public class LocalhostAuthPlugin extends Plugin {
    private static final int[] PREFERRED_PORTS = {2259, 6460, 7119, 8870, 9096};

    private final Object authLock = new Object();
    private String activeCallbackId;

    @PluginMethod
    public void startLogin(PluginCall call) {
        String authUrl = call.getString("authUrl");
        if (authUrl == null || authUrl.isEmpty()) {
            call.reject("Missing authUrl");
            return;
        }

        int port = call.getInt("port", PREFERRED_PORTS[0]);
        int timeoutMs = call.getInt("timeoutMs", 180000);

        synchronized (authLock) {
            if (activeCallbackId != null) {
                call.reject("Login is already in progress");
                return;
            }
            activeCallbackId = call.getCallbackId();
        }

        Intent intent = new Intent(getContext(), LoginActivity.class);
        intent.putExtra(LoginActivity.EXTRA_AUTH_URL, authUrl);
        intent.putExtra(LoginActivity.EXTRA_EXPECTED_PORT, port);
        intent.putExtra(LoginActivity.EXTRA_TIMEOUT_MS, timeoutMs);

        try {
            startActivityForResult(call, intent, "handleLoginResult");
        } catch (RuntimeException error) {
            clearActiveLogin(call.getCallbackId());
            call.reject("Unable to open the sign-in window: " + error.getMessage());
        }
    }

    @PluginMethod
    public void tcpPing(PluginCall call) {
        String rawUrl = call.getString("url");
        if (rawUrl == null || rawUrl.isEmpty()) {
            call.reject("Missing url");
            return;
        }

        int timeoutMs = Math.max(500, call.getInt("timeoutMs", 3000));
        int samples = Math.max(1, Math.min(5, call.getInt("samples", 3)));
        boolean warmup = call.getBoolean("warmup", true);

        new Thread(() -> {
            try {
                URL url = new URL(rawUrl);
                String host = url.getHost();
                int port = url.getPort();
                if (port <= 0) {
                    port = "http".equalsIgnoreCase(url.getProtocol()) ? 80 : 443;
                }

                if (host == null || host.isEmpty()) {
                    resolveTcpPing(call, null, "Invalid URL host");
                    return;
                }

                if (warmup) {
                    tcpConnectMs(host, port, timeoutMs);
                }

                List<Long> timings = new ArrayList<>();
                for (int index = 0; index < samples; index++) {
                    if (index > 0) {
                        Thread.sleep(100);
                    }
                    Long timing = tcpConnectMs(host, port, timeoutMs);
                    if (timing != null) {
                        timings.add(timing);
                    }
                }

                if (timings.isEmpty()) {
                    resolveTcpPing(call, null, "All ping tests failed");
                    return;
                }

                long total = 0;
                for (Long timing : timings) {
                    total += timing;
                }
                resolveTcpPing(call, Math.round((double) total / timings.size()), null);
            } catch (Exception error) {
                resolveTcpPing(call, null, error.getMessage() != null ? error.getMessage() : "Ping failed");
            }
        }, "OpenNOW-TcpPing").start();
    }

    @ActivityCallback
    private void handleLoginResult(PluginCall call, ActivityResult result) {
        if (call == null) {
            clearActiveLogin(null);
            return;
        }

        clearActiveLogin(call.getCallbackId());

        Intent data = result.getData();
        if (result.getResultCode() == LoginActivity.RESULT_AUTH_SUCCESS && data != null) {
            String code = data.getStringExtra(LoginActivity.EXTRA_RESULT_CODE);
            if (code != null && !code.isEmpty()) {
                JSObject payload = new JSObject();
                payload.put("code", code);
                payload.put("redirectUri", data.getStringExtra(LoginActivity.EXTRA_RESULT_REDIRECT_URI));
                call.resolve(payload);
                return;
            }
        }

        String message = data != null ? data.getStringExtra(LoginActivity.EXTRA_RESULT_ERROR) : null;
        if (message == null || message.isEmpty()) {
            message = result.getResultCode() == Activity.RESULT_CANCELED
                ? "Login was cancelled before the OAuth callback completed"
                : "Authorization failed";
        }
        call.reject(message);
    }

    private void clearActiveLogin(String callbackId) {
        synchronized (authLock) {
            if (callbackId == null || callbackId.equals(activeCallbackId)) {
                activeCallbackId = null;
            }
        }
    }

    private Long tcpConnectMs(String host, int port, int timeoutMs) {
        long startedAt = SystemClock.elapsedRealtime();
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress(host, port), timeoutMs);
            return SystemClock.elapsedRealtime() - startedAt;
        } catch (Exception ignored) {
            return null;
        }
    }

    private void resolveTcpPing(PluginCall call, Long pingMs, String error) {
        getActivity().runOnUiThread(() -> {
            JSObject payload = new JSObject();
            if (pingMs != null) {
                payload.put("pingMs", pingMs);
            }
            if (error != null && !error.isEmpty()) {
                payload.put("error", error);
            }
            call.resolve(payload);
        });
    }

    @Override
    protected void handleOnDestroy() {
        synchronized (authLock) {
            activeCallbackId = null;
        }
        super.handleOnDestroy();
    }
}
