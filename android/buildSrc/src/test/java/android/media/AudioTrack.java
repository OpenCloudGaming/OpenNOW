package android.media;

/** JVM fixture for the calls made by WebRTC's buffer tuner. */
public class AudioTrack {
    public int state = 1;
    public int adjustments;
    public RuntimeException failure;

    public int getState() { return state; }

    public int getUnderrunCount() {
        if (failure != null) throw failure;
        adjustments++;
        return 0;
    }
}
