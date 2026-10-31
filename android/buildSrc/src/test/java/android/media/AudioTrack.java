package android.media;

/** JVM fixture for the calls made by WebRTC's buffer tuner. */
public class AudioTrack {
    public int state = 1;
    public int adjustments;
    public RuntimeException failure;
    public RuntimeException writeFailure;
    public int writeResult = Integer.MIN_VALUE;
    public int writes;
    public int releases;
    public volatile boolean blockWrite;
    public final java.util.concurrent.CountDownLatch writeEntered = new java.util.concurrent.CountDownLatch(1);
    public final java.util.concurrent.CountDownLatch writeResume = new java.util.concurrent.CountDownLatch(1);

    public int getState() { return state; }

    public int write(java.nio.ByteBuffer buffer, int size, int mode) {
        writeEntered.countDown();
        if (blockWrite) {
            boolean waiting = true;
            while (waiting) {
                try { writeResume.await(); waiting = false; } catch (InterruptedException ignored) {}
            }
        }
        if (writeFailure != null) throw writeFailure;
        if (writeResult != Integer.MIN_VALUE) return writeResult;
        if (state != 1) throw new IllegalStateException("Track released while writing");
        writes++;
        buffer.position(buffer.position() + size);
        return size;
    }

    public void stop() {}

    public void release() { state = 0; releases++; }

    public int getUnderrunCount() {
        if (failure != null) throw failure;
        adjustments++;
        return 0;
    }
}
