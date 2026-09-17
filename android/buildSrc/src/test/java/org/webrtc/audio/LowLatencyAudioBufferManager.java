package org.webrtc.audio;

import android.media.AudioTrack;

/** Fixture retains the upstream method signature and its first AudioTrack access. */
public class LowLatencyAudioBufferManager {
    public void maybeAdjustBufferSize(AudioTrack track) {
        track.getUnderrunCount();
    }
}
