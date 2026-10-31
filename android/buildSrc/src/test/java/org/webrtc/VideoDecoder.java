package org.webrtc;

public interface VideoDecoder {
    final class Settings {
        public final int width;
        public final int height;

        public Settings(int width, int height) {
            this.width = width;
            this.height = height;
        }
    }

    interface Callback {}
}
