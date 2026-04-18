package com.opencloudgaming.opennow;

import android.content.Intent;

import androidx.activity.result.ActivityResult;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.ActivityCallback;
import com.getcapacitor.annotation.CapacitorPlugin;

@CapacitorPlugin(name = "LocalhostAuth")
public class LocalhostAuthPlugin extends Plugin {
    private static final int[] PREFERRED_PORTS = {2259, 6460, 7119, 8870, 9096};

    @PluginMethod
    public void startLogin(PluginCall call) {
        String authUrl = call.getString("authUrl");
        if (authUrl == null || authUrl.isEmpty()) {
            call.reject("Missing authUrl");
            return;
        }

        int port = call.getInt("port", PREFERRED_PORTS[0]);
        int timeoutMs = call.getInt("timeoutMs", 180000);

        Intent intent = new Intent(getActivity(), LoginActivity.class);
        intent.putExtra(LoginActivity.EXTRA_AUTH_URL, authUrl);
        intent.putExtra(LoginActivity.EXTRA_EXPECTED_PORT, port);
        intent.putExtra(LoginActivity.EXTRA_TIMEOUT_MS, timeoutMs);
        startActivityForResult(call, intent, "handleLoginResult");
    }

    @ActivityCallback
    private void handleLoginResult(PluginCall call, ActivityResult result) {
        if (call == null) {
            return;
        }

        Intent data = result.getData();
        if (result.getResultCode() == LoginActivity.RESULT_AUTH_SUCCESS && data != null) {
            String code = data.getStringExtra(LoginActivity.EXTRA_RESULT_CODE);
            if (code == null || code.isEmpty()) {
                call.reject("OAuth callback completed without an authorization code");
                return;
            }

            JSObject payload = new JSObject();
            payload.put("code", code);
            String redirectUri = data.getStringExtra(LoginActivity.EXTRA_RESULT_REDIRECT_URI);
            if (redirectUri != null && !redirectUri.isEmpty()) {
                payload.put("redirectUri", redirectUri);
            }
            call.resolve(payload);
            return;
        }

        String message = data != null ? data.getStringExtra(LoginActivity.EXTRA_RESULT_ERROR) : null;
        if (message == null || message.isEmpty()) {
            message = "Login was cancelled before the OAuth callback completed";
        }
        call.reject(message);
    }
}
