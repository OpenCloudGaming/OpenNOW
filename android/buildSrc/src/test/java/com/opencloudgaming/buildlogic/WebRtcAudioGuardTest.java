package com.opencloudgaming.buildlogic;

import android.media.AudioTrack;
import java.io.InputStream;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import org.junit.Before;
import org.junit.Test;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import static org.junit.Assert.*;

public class WebRtcAudioGuardTest {
    private Object manager;
    private Method adjust;

    @Before
    public void instrumentAndLoad() throws Exception {
        String name = WebRtcAudioGuard.CLASS_NAME;
        byte[] original;
        try (InputStream stream = getClass().getResourceAsStream("/" + name + ".class")) {
            original = stream.readAllBytes();
        }
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES | ClassWriter.COMPUTE_MAXS);
        new ClassReader(original).accept(new WebRtcAudioGuard(writer), 0);
        byte[] guarded = writer.toByteArray();
        Class<?> type = new ClassLoader(getClass().getClassLoader()) {
            Class<?> loadGuarded() { return defineClass(name.replace('/', '.'), guarded, 0, guarded.length); }
        }.loadGuarded();
        manager = type.getConstructor().newInstance();
        adjust = type.getMethod("maybeAdjustBufferSize", AudioTrack.class);
    }

    @Test
    public void nullTrackAfterTeardownIsIgnored() throws Exception {
        adjust.invoke(manager, new Object[] { null });
    }

    @Test
    public void releasedTrackIsNotQueried() throws Exception {
        AudioTrack track = new AudioTrack();
        track.state = 0;
        adjust.invoke(manager, track);
        assertEquals(0, track.adjustments);
    }

    @Test
    public void liveTrackKeepsUpstreamAdjustment() throws Exception {
        AudioTrack track = new AudioTrack();
        adjust.invoke(manager, track);
        assertEquals(1, track.adjustments);
    }

    @Test
    public void releaseBetweenStateCheckAndBufferAccessDoesNotCrash() throws Exception {
        AudioTrack track = new AudioTrack();
        track.failure = new IllegalStateException("AudioTrack has been released");
        adjust.invoke(manager, track);
    }

    @Test
    public void unrelatedErrorsAreNotSwallowed() throws Exception {
        AudioTrack track = new AudioTrack();
        track.failure = new IllegalArgumentException("unexpected");
        try {
            adjust.invoke(manager, track);
            fail("Expected the original exception");
        } catch (InvocationTargetException error) {
            assertSame(track.failure, error.getCause());
        }
    }

    @Test
    public void changedBufferTuningSignatureFailsBuild() {
        try {
            new WebRtcAudioGuard(new ClassWriter(0)).visitEnd();
            fail("Expected an API mismatch");
        } catch (IllegalStateException expected) {
            assertTrue(expected.getMessage().contains("WebRTC audio API changed"));
        }
    }
}
