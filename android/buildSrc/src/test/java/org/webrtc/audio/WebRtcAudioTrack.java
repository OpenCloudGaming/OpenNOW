package org.webrtc.audio;

import android.media.AudioTrack;
import com.opencloudgaming.buildlogic.WebRtcPlaybackGuardTest;
import java.nio.ByteBuffer;
import org.webrtc.ThreadUtils;

public class WebRtcAudioTrack {
    private AudioTrack audioTrack;
    private ByteBuffer byteBuffer;
    private AudioTrackThread audioThread;
    private final boolean useLowLatency;

    public WebRtcAudioTrack(boolean lowLatency) {
        useLowLatency = lowLatency;
    }

    private int initPlayout(int sampleRate, int channels, double bufferSizeFactor) {
        byteBuffer = ByteBuffer.allocateDirect(1920);
        audioTrack = new AudioTrack();
        return 1920;
    }

    private boolean startPlayout() {
        if (audioThread != null) throw new AssertionError("Worker already exists");
        audioThread = new AudioTrackThread();
        audioThread.setUncaughtExceptionHandler((worker, error) -> WebRtcPlaybackGuardTest.failure.set(error));
        audioThread.start();
        return true;
    }

    private boolean stopPlayout() {
        audioThread.stopThread();
        ThreadUtils.joinUninterruptibly(audioThread, 2000);
        audioThread = null;
        audioTrack.stop();
        doAudioTrackStateCallback(1);
        releaseAudioResources();
        return true;
    }

    private void releaseAudioResources() {
        if (audioTrack != null) {
            audioTrack.release();
            audioTrack = null;
        }
    }

    private void reportWebRtcAudioTrackInitError(String error) {
        WebRtcPlaybackGuardTest.errors++;
    }

    private void doAudioTrackStateCallback(int state) {
        if (state == 1) WebRtcPlaybackGuardTest.stopCallbacks++;
    }

    private void reportWebRtcAudioTrackStartError(JavaAudioDeviceModule.AudioTrackStartErrorCode code, String error) {
        WebRtcPlaybackGuardTest.errors++;
    }

    private static void nativeGetPlayoutData(long pointer, int bytes) {
        WebRtcPlaybackGuardTest.fetch();
    }

    private class AudioTrackThread extends Thread {
        private volatile boolean keepAlive = true;

        @Override
        public void run() {
            if (audioTrack.state != 1) throw new AssertionError("Track released before worker start");
            while (keepAlive) {
                nativeGetPlayoutData(0, byteBuffer.capacity());
                if (audioTrack.write(byteBuffer, byteBuffer.remaining(), 0) < 0) {
                    keepAlive = false;
                    WebRtcPlaybackGuardTest.errors++;
                }
                if (useLowLatency) new LowLatencyAudioBufferManager().maybeAdjustBufferSize(audioTrack);
                byteBuffer.rewind();
            }
        }

        public void stopThread() {
            keepAlive = false;
        }
    }
}
