package org.webrtc;

import android.os.HandlerThread;
import java.util.ArrayList;
import java.util.List;

/** Models upstream's requested-versus-applied metadata, including a delayed worker. */
public class SurfaceTextureHelper {
    public final List<String> requests = new ArrayList<>();
    public final List<String> applied = new ArrayList<>();
    public boolean failSize;
    public Object listener;
    public int disposals;

    public static HandlerThread create(String name) { return new HandlerThread(name); }

    public void startListening(Object listener) { this.listener = listener; }
    public void stopListening() { listener = null; }
    public void dispose() { disposals++; }

    public void setTextureSize(int width, int height) {
        if (width <= 0 || height <= 0) throw new IllegalArgumentException("Invalid size");
        if (failSize) throw new IllegalStateException("Surface unavailable");
        requests.add("size:" + width + "x" + height);
    }

    public void setFrameRotation(int rotation) { requests.add("rotation:" + rotation); }

    public void drainWorker() {
        applied.addAll(requests);
        requests.clear();
    }
}
