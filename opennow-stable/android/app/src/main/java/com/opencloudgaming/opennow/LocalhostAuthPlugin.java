package com.opencloudgaming.opennow;

import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;

import com.getcapacitor.Bridge;
import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.net.BindException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

@CapacitorPlugin(name = "LocalhostAuth")
public class LocalhostAuthPlugin extends Plugin {
    private static final int[] PREFERRED_PORTS = {2259, 6460, 7119, 8870, 9096};
    private static final String CALLBACK_HTML =
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenNOW Login</title>" +
        "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"></head>" +
        "<body style=\"font-family:sans-serif;background:#0b1220;color:#fff;display:flex;" +
        "align-items:center;justify-content:center;min-height:100vh;margin:0;padding:24px;text-align:center\">" +
        "<div><h1 style=\"margin:0 0 12px;font-size:24px\">Login complete</h1>" +
        "<p style=\"margin:0;opacity:0.8\">You can return to OpenNOW now.</p></div></body></html>";

    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private final Object authLock = new Object();
    private String activeCallbackId;
    private ServerSocket serverSocket;
    private Thread authThread;

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

        bridge.saveCall(call);
        CountDownLatch listenerReady = new CountDownLatch(1);
        startLoopbackListener(call.getCallbackId(), port, timeoutMs, listenerReady);

        try {
            if (!listenerReady.await(3, TimeUnit.SECONDS)) {
                failLogin(call.getCallbackId(), "Timed out preparing the localhost login callback");
                return;
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            failLogin(call.getCallbackId(), "Login startup was interrupted");
            return;
        }

        try {
            Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(authUrl));
            intent.addCategory(Intent.CATEGORY_BROWSABLE);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            getContext().startActivity(intent);
        } catch (ActivityNotFoundException error) {
            stopActiveAuthServer();
            failLogin(call.getCallbackId(), "No browser is installed to complete sign-in.");
        } catch (RuntimeException error) {
            stopActiveAuthServer();
            failLogin(call.getCallbackId(), "Unable to open the browser for sign-in: " + error.getMessage());
        }
    }

    private void startLoopbackListener(String callbackId, int port, int timeoutMs, CountDownLatch listenerReady) {
        Thread worker = new Thread(() -> {
            try (ServerSocket localServer = new ServerSocket()) {
                localServer.setReuseAddress(true);
                localServer.bind(new InetSocketAddress(port));
                localServer.setSoTimeout(Math.max(timeoutMs, 1000));
                synchronized (authLock) {
                    serverSocket = localServer;
                }
                listenerReady.countDown();

                try (Socket socket = localServer.accept()) {
                    socket.setSoTimeout(5000);
                    LoopbackResult result = readLoopbackResult(socket, port);
                    if (result.code != null && !result.code.isEmpty()) {
                        completeLogin(callbackId, result.code, result.redirectUri);
                    } else {
                        failLogin(callbackId, result.error != null && !result.error.isEmpty()
                            ? result.error
                            : "OAuth callback completed without an authorization code");
                    }
                }
            } catch (SocketTimeoutException error) {
                failLogin(callbackId, "Timed out waiting for OAuth callback");
            } catch (BindException error) {
                failLogin(callbackId, "Unable to bind localhost callback port " + port);
            } catch (Exception error) {
                failLogin(callbackId, "Login failed: " + error.getMessage());
            } finally {
                listenerReady.countDown();
                synchronized (authLock) {
                    if (authThread == Thread.currentThread()) {
                        authThread = null;
                    }
                    serverSocket = null;
                }
            }
        }, "OpenNOW-AuthLoopback");

        synchronized (authLock) {
            authThread = worker;
        }
        worker.start();
    }

    private void stopActiveAuthServer() {
        synchronized (authLock) {
            if (serverSocket != null) {
                try {
                    serverSocket.close();
                } catch (Exception ignored) {
                }
            }
            if (authThread != null) {
                authThread.interrupt();
            }
        }
    }

    private LoopbackResult readLoopbackResult(Socket socket, int port) throws Exception {
        BufferedReader reader = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.UTF_8));
        BufferedWriter writer = new BufferedWriter(new OutputStreamWriter(socket.getOutputStream(), StandardCharsets.UTF_8));

        String requestLine = reader.readLine();
        if (requestLine == null || requestLine.isEmpty()) {
            throw new IllegalStateException("OAuth callback request was empty");
        }

        while (true) {
            String header = reader.readLine();
            if (header == null || header.isEmpty()) {
                break;
            }
        }

        String[] parts = requestLine.split(" ");
        if (parts.length < 2) {
            throw new IllegalStateException("OAuth callback request line was malformed");
        }

        Uri uri = Uri.parse("http://localhost:" + port + parts[1]);
        String code = uri.getQueryParameter("code");
        String error = uri.getQueryParameter("error");

        writer.write("HTTP/1.1 200 OK\r\n");
        writer.write("Content-Type: text/html; charset=UTF-8\r\n");
        writer.write("Cache-Control: no-store\r\n");
        writer.write("Connection: close\r\n");
        writer.write("Content-Length: " + CALLBACK_HTML.getBytes(StandardCharsets.UTF_8).length + "\r\n");
        writer.write("\r\n");
        writer.write(CALLBACK_HTML);
        writer.flush();

        return new LoopbackResult(code, error, "http://localhost:" + port);
    }

    private void completeLogin(String callbackId, String code, String redirectUri) {
        mainHandler.post(() -> {
            PluginCall call = consumeSavedCall(callbackId);
            if (call == null) {
                return;
            }

            JSObject payload = new JSObject();
            payload.put("code", code);
            payload.put("redirectUri", redirectUri);
            bringAppToFront();
            call.resolve(payload);
        });
    }

    private void failLogin(String callbackId, String message) {
        mainHandler.post(() -> {
            PluginCall call = consumeSavedCall(callbackId);
            if (call == null) {
                return;
            }

            bringAppToFront();
            call.reject(message);
        });
    }

    private PluginCall consumeSavedCall(String callbackId) {
        synchronized (authLock) {
            if (activeCallbackId == null || !activeCallbackId.equals(callbackId)) {
                return null;
            }
            activeCallbackId = null;
        }
        Bridge currentBridge = bridge;
        if (currentBridge == null) {
            return null;
        }
        PluginCall call = currentBridge.getSavedCall(callbackId);
        if (call != null) {
            currentBridge.releaseCall(call);
        }
        return call;
    }

    private void bringAppToFront() {
        Intent launchIntent = getContext().getPackageManager().getLaunchIntentForPackage(getContext().getPackageName());
        if (launchIntent == null) {
            return;
        }
        launchIntent.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_NEW_TASK);
        getContext().startActivity(launchIntent);
    }

    @Override
    protected void handleOnDestroy() {
        synchronized (authLock) {
            if (serverSocket != null) {
                try {
                    serverSocket.close();
                } catch (Exception ignored) {
                }
                serverSocket = null;
            }
            if (authThread != null) {
                authThread.interrupt();
                authThread = null;
            }
            activeCallbackId = null;
        }
        super.handleOnDestroy();
    }

    private static class LoopbackResult {
        final String code;
        final String error;
        final String redirectUri;

        LoopbackResult(String code, String error, String redirectUri) {
            this.code = code;
            this.error = error;
            this.redirectUri = redirectUri;
        }
    }
}
