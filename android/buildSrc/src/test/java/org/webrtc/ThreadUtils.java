package org.webrtc;

public class ThreadUtils {
    public static boolean joinUninterruptibly(Thread worker, long timeoutMs) {
        long deadline = System.nanoTime() + 50_000_000L;
        while (worker.isAlive() && System.nanoTime() < deadline) {
            try {
                worker.join(1);
            } catch (InterruptedException ignored) {
            }
        }
        return !worker.isAlive();
    }
}
