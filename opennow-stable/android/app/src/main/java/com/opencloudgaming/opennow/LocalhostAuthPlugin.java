package com.opencloudgaming.opennow;

import com.getcapacitor.JSObject;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.URL;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

@CapacitorPlugin(name = "LocalhostAuth")
public class LocalhostAuthPlugin extends Plugin {
    private static final int[] PREFERRED_PORTS = {2259, 6460, 7119, 8870, 9096};
    private static final String SUCCESS_HTML = "<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenNOW Login</title></head><body style=\"font-family:Segoe UI,Arial,sans-serif;background:#0b1220;color:#dbe7ff;display:flex;justify-content:center;align-items:center;height:100vh\"><div style=\"background:#111a2c;padding:24px 28px;border:1px solid #30425f;border-radius:12px;max-width:460px\"><h2 style=\"margin-top:0\">OpenNOW Login</h2><p>Login complete. You can close this window and return to OpenNOW Stable.</p></div></body></html>";
    private static final String FAILURE_HTML = "<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenNOW Login</title></head><body style=\"font-family:Segoe UI,Arial,sans-serif;background:#0b1220;color:#dbe7ff;display:flex;justify-content:center;align-items:center;height:100vh\"><div style=\"background:#111a2c;padding:24px 28px;border:1px solid #30425f;border-radius:12px;max-width:460px\"><h2 style=\"margin-top:0\">OpenNOW Login</h2><p>Login failed or was cancelled. You can close this window and return to OpenNOW Stable.</p></div></body></html>";

    private final ExecutorService acceptExecutor = Executors.newSingleThreadExecutor();
    private final ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();
    private final Object lock = new Object();
    private ServerSocket serverSocket;
    private CompletableFuture<AuthCallbackResult> callbackFuture;
    private int currentPort;

    @PluginMethod
    public void startServer(PluginCall call) {
        synchronized (lock) {
            closeServerLocked(false, null);
            callbackFuture = new CompletableFuture<>();
            try {
                serverSocket = bindServerSocket();
                currentPort = serverSocket.getLocalPort();
                acceptExecutor.execute(this::acceptCallback);
                JSObject result = new JSObject();
                result.put("port", currentPort);
                call.resolve(result);
            } catch (IOException error) {
                closeServerLocked(true, new IOException("Failed to start localhost OAuth callback server", error));
                call.reject("Failed to start localhost OAuth callback server", error);
            }
        }
    }

    @PluginMethod
    public void waitForCode(PluginCall call) {
        final CompletableFuture<AuthCallbackResult> future;
        synchronized (lock) {
            future = callbackFuture;
        }
        if (future == null) {
            call.reject("Localhost OAuth callback server is not running");
            return;
        }

        final int timeoutMs = call.getInt("timeoutMs", 180000);
        final ScheduledFuture<?> timeoutTask = scheduler.schedule(() -> {
            synchronized (lock) {
                if (callbackFuture == future && !future.isDone()) {
                    closeServerLocked(true, new TimeoutException("Timed out waiting for OAuth callback"));
                }
            }
        }, timeoutMs, TimeUnit.MILLISECONDS);

        future.whenComplete((result, throwable) -> {
            timeoutTask.cancel(false);
            if (throwable == null && result != null) {
                JSObject payload = new JSObject();
                payload.put("code", result.code);
                call.resolve(payload);
                return;
            }

            Throwable cause = throwable instanceof ExecutionException && throwable.getCause() != null
                ? throwable.getCause()
                : throwable;
            if (cause instanceof TimeoutException) {
                call.reject("Timed out waiting for OAuth callback", (Exception) cause);
                return;
            }
            if (cause instanceof Exception) {
                String message = cause.getMessage() != null ? cause.getMessage() : cause.toString();
                call.reject(message, (Exception) cause);
                return;
            }
            call.reject(cause != null && cause.getMessage() != null ? cause.getMessage() : "Authorization failed");
        });
    }

    @PluginMethod
    public void stopServer(PluginCall call) {
        stopServerInternal();
        call.resolve();
    }

    @Override
    protected void handleOnDestroy() {
        stopServerInternal();
        acceptExecutor.shutdownNow();
        scheduler.shutdownNow();
        super.handleOnDestroy();
    }

    private ServerSocket bindServerSocket() throws IOException {
        for (int port : PREFERRED_PORTS) {
            try {
                return createServerSocket(port);
            } catch (IOException ignored) {
            }
        }
        return createServerSocket(0);
    }

    private ServerSocket createServerSocket(int port) throws IOException {
        ServerSocket socket = new ServerSocket();
        socket.setReuseAddress(true);
        socket.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"), port));
        return socket;
    }

    private void acceptCallback() {
        final ServerSocket socket;
        final CompletableFuture<AuthCallbackResult> future;
        synchronized (lock) {
            socket = serverSocket;
            future = callbackFuture;
        }
        if (socket == null || future == null) {
            return;
        }

        try (Socket client = socket.accept()) {
            handleClient(client, future);
        } catch (IOException error) {
            synchronized (lock) {
                if (callbackFuture == future && !future.isDone()) {
                    closeServerLocked(true, new IOException("Localhost OAuth callback failed", error));
                }
            }
        } finally {
            synchronized (lock) {
                if (callbackFuture == future && future.isDone()) {
                    closeServerLocked(false, null);
                }
            }
        }
    }

    private void handleClient(Socket client, CompletableFuture<AuthCallbackResult> future) throws IOException {
        BufferedReader reader = new BufferedReader(new InputStreamReader(client.getInputStream(), StandardCharsets.UTF_8));
        BufferedWriter writer = new BufferedWriter(new OutputStreamWriter(client.getOutputStream(), StandardCharsets.UTF_8));

        String requestLine = reader.readLine();
        String path = extractRequestPath(requestLine);
        URL url = new URL("http://localhost:" + currentPort + path);
        String code = url.getQuery() == null ? null : splitQueryParam(url.getQuery(), "code");
        String error = url.getQuery() == null ? null : splitQueryParam(url.getQuery(), "error");
        boolean success = code != null && !code.isEmpty();

        String html = success ? SUCCESS_HTML : FAILURE_HTML;
        byte[] body = html.getBytes(StandardCharsets.UTF_8);
        writer.write("HTTP/1.1 200 OK\r\n");
        writer.write("Content-Type: text/html; charset=utf-8\r\n");
        writer.write("Content-Length: " + body.length + "\r\n");
        writer.write("Connection: close\r\n\r\n");
        writer.write(html);
        writer.flush();

        if (success) {
            future.complete(new AuthCallbackResult(code));
        } else {
            future.completeExceptionally(new IllegalStateException(error != null && !error.isEmpty() ? error : "Authorization failed"));
        }
    }

    private String extractRequestPath(String requestLine) throws IOException {
        if (requestLine == null || requestLine.isEmpty()) {
            throw new IOException("OAuth callback request was empty");
        }
        String[] parts = requestLine.split(" ");
        if (parts.length < 2) {
            throw new IOException("OAuth callback request line was malformed");
        }
        return parts[1];
    }

    private String splitQueryParam(String query, String key) {
        for (String pair : query.split("&")) {
            int separator = pair.indexOf('=');
            String pairKey = separator >= 0 ? pair.substring(0, separator) : pair;
            if (!key.equals(pairKey)) {
                continue;
            }
            String value = separator >= 0 ? pair.substring(separator + 1) : "";
            return URLDecoder.decode(value, StandardCharsets.UTF_8);
        }
        return null;
    }

    private void stopServerInternal() {
        synchronized (lock) {
            closeServerLocked(true, new IllegalStateException("OAuth callback server stopped before a code was received"));
        }
    }

    private void closeServerLocked(boolean completePending, Throwable cause) {
        if (serverSocket != null) {
            try {
                serverSocket.close();
            } catch (IOException ignored) {
            }
            serverSocket = null;
        }
        currentPort = 0;
        if (completePending && callbackFuture != null && !callbackFuture.isDone()) {
            callbackFuture.completeExceptionally(cause != null ? cause : new IllegalStateException("OAuth callback server stopped before a code was received"));
        }
        if (callbackFuture != null && callbackFuture.isDone()) {
            callbackFuture = null;
        }
    }

    private static final class AuthCallbackResult {
        final String code;

        AuthCallbackResult(String code) {
            this.code = code;
        }
    }
}
