package com.opencloudgaming.buildlogic;

import android.media.AudioTrack;
import java.io.InputStream;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.ByteBuffer;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.FieldVisitor;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import static org.junit.Assert.*;

public class WebRtcPlaybackGuardTest {
    public static final AtomicReference<Throwable> failure = new AtomicReference<>();
    public static volatile int errors;
    public static volatile int stopCallbacks;
    private static CountDownLatch fetchEntered;
    private static CountDownLatch fetchResume;
    private Thread worker;
    private AudioTrack track;

    @Before
    public void reset() {
        failure.set(null);
        errors = 0;
        stopCallbacks = 0;
        fetchEntered = new CountDownLatch(1);
        fetchResume = new CountDownLatch(1);
    }

    @After
    public void finishWorker() throws Exception {
        fetchResume.countDown();
        if (track != null) track.writeResume.countDown();
        if (worker != null) {
            Method stop = worker.getClass().getDeclaredMethod("stopThread");
            stop.setAccessible(true);
            stop.invoke(worker);
            worker.join(2000);
            assertFalse("Playback worker did not finish", worker.isAlive());
        }
    }

    public static void fetch() {
        fetchEntered.countDown();
        try { fetchResume.await(); } catch (InterruptedException error) { throw new AssertionError(error); }
    }

    @Test
    public void stopBeforeWriteRetainsResourcesAndRejectsImmediateRestart() throws Exception {
        for (boolean latency : new boolean[] { false, true }) {
            reset();
            Object owner = owner(latency);
            start(owner);
            assertTrue(fetchEntered.await(2, TimeUnit.SECONDS));
            ByteBuffer buffer = (ByteBuffer) field(owner, "byteBuffer");
            assertEquals(true, invoke(owner, "stopPlayout"));
            assertEquals(1, stopCallbacks);
            assertTrue(worker.isAlive());
            assertSame(track, field(owner, "audioTrack"));
            assertSame(buffer, field(owner, "byteBuffer"));
            assertSame(worker, field(owner, "audioThread"));
            assertEquals(0, track.releases);
            assertEquals(-1, invoke(owner, "initPlayout", 48000, 2, 1.0));
            assertEquals(false, invoke(owner, "startPlayout"));
            assertEquals(2, errors);
            assertSame(buffer, field(owner, "byteBuffer"));
            assertSame(track, field(owner, "audioTrack"));
            fetchResume.countDown();
            worker.join(2000);
            assertFalse(worker.isAlive());
            assertNull(failure.get());
            assertEquals(0, track.writes);
            assertEquals(1, track.releases);
            assertNull(field(owner, "audioTrack"));
            assertEquals(true, invoke(owner, "stopPlayout"));
            assertEquals(1, stopCallbacks);
            assertEquals(1920, invoke(owner, "initPlayout", 48000, 2, 1.0));
            assertNotSame(buffer, field(owner, "byteBuffer"));
            assertNotSame(track, field(owner, "audioTrack"));
        }
    }

    @Test
    public void blockedWriteIsNotReleasedUntilWorkerFinishes() throws Exception {
        Object owner = owner(true);
        track.blockWrite = true;
        fetchResume.countDown();
        start(owner);
        assertTrue(track.writeEntered.await(2, TimeUnit.SECONDS));
        assertEquals(true, invoke(owner, "stopPlayout"));
        assertEquals(0, track.releases);
        assertEquals(-1, invoke(owner, "initPlayout", 48000, 2, 1.0));
        track.writeResume.countDown();
        worker.join(2000);
        assertNull(failure.get());
        assertEquals(1, track.writes);
        assertEquals(1, track.releases);
        assertEquals(1, stopCallbacks);
    }

    @Test
    public void ordinaryWriteExceptionsAreNotHidden() throws Exception {
        Object owner = owner(false);
        IllegalStateException expected = new IllegalStateException("Driver write failed");
        track.writeFailure = expected;
        fetchResume.countDown();
        start(owner);
        worker.join(2000);
        assertSame(expected, failure.get());
        assertEquals(true, invoke(owner, "stopPlayout"));
        assertEquals(1, track.releases);
        assertEquals(1, stopCallbacks);
    }

    @Test
    public void negativeWriteResultsStillReportPlaybackErrors() throws Exception {
        Object owner = owner(true);
        track.writeResult = -6;
        fetchResume.countDown();
        start(owner);
        worker.join(2000);
        assertFalse(worker.isAlive());
        assertEquals(1, errors);
        assertNull(failure.get());
        assertEquals(true, invoke(owner, "stopPlayout"));
        assertEquals(1, track.releases);
    }

    @Test
    public void completedStopKeepsNormalInitializationAvailable() throws Exception {
        Object owner = owner(true);
        start(owner);
        assertTrue(fetchEntered.await(2, TimeUnit.SECONDS));
        Method stop = worker.getClass().getDeclaredMethod("stopThread");
        stop.setAccessible(true);
        stop.invoke(worker);
        fetchResume.countDown();
        worker.join(2000);
        assertEquals(true, invoke(owner, "stopPlayout"));
        assertEquals(1, track.releases);
        assertEquals(1920, invoke(owner, "initPlayout", 48000, 2, 1.0));
        assertEquals(0, errors);
        assertEquals(1, stopCallbacks);
    }

    @Test
    public void workerCanFinishBeforeTimeoutHandlerAcquiresOwnership() throws Exception {
        Object owner = owner(true);
        start(owner);
        assertTrue(fetchEntered.await(2, TimeUnit.SECONDS));
        Method stop = worker.getClass().getDeclaredMethod("stopThread");
        stop.setAccessible(true);
        stop.invoke(worker);
        fetchResume.countDown();
        worker.join(2000);
        invoke(owner, "opennowStopTimedOut");
        assertEquals(1, track.releases);
        assertEquals(1, stopCallbacks);
        assertNull(field(owner, "audioTrack"));
        invoke(owner, "releaseAudioResources");
        invoke(owner, "releaseAudioResources");
        assertEquals(true, invoke(owner, "stopPlayout"));
        assertEquals(1, track.releases);
        assertEquals(1, stopCallbacks);
        assertEquals(1920, invoke(owner, "initPlayout", 48000, 2, 1.0));
    }

    @Test
    public void changedLifecycleFailsInstrumentation() {
        try {
            new WebRtcPlaybackGuard(new ClassWriter(0)).visitEnd();
            fail("Expected an API mismatch");
        } catch (IllegalStateException expected) {
            assertTrue(expected.getMessage().contains("lifecycle changed"));
        }
    }

    @Test(expected = IllegalStateException.class)
    public void missingStopCallbackFailsInstrumentation() throws Exception {
        try (InputStream input = getClass().getResourceAsStream("/" + WebRtcPlaybackGuard.CLASS_NAME + ".class")) {
            new ClassReader(input).accept(new ClassVisitor(Opcodes.ASM9, new WebRtcPlaybackGuard(new ClassWriter(0))) {
                @Override
                public MethodVisitor visitMethod(int access, String name, String desc, String signature, String[] exceptions) {
                    return name.equals("doAudioTrackStateCallback") ? null
                            : super.visitMethod(access, name, desc, signature, exceptions);
                }
            }, 0);
        }
    }

    @Test(expected = IllegalStateException.class)
    public void nonVolatileStopFlagFailsInstrumentation() throws Exception {
        try (InputStream input = getClass().getResourceAsStream("/" + WebRtcPlaybackGuard.THREAD_NAME + ".class")) {
            new ClassReader(input).accept(new ClassVisitor(Opcodes.ASM9, new WebRtcPlaybackGuard(new ClassWriter(0))) {
                @Override
                public FieldVisitor visitField(int access, String name, String desc, String signature, Object value) {
                    return super.visitField(name.equals("keepAlive") ? access & ~Opcodes.ACC_VOLATILE : access,
                            name, desc, signature, value);
                }
            }, 0);
        }
    }

    private Object owner(boolean latency) throws Exception {
        ClassLoader loader = new ClassLoader(getClass().getClassLoader()) {
            @Override
            protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
                if (!name.equals(WebRtcPlaybackGuard.CLASS_NAME.replace('/', '.')) &&
                        !name.equals(WebRtcPlaybackGuard.THREAD_NAME.replace('/', '.'))) {
                    return super.loadClass(name, resolve);
                }
                Class<?> type = findLoadedClass(name);
                if (type == null) {
                    try (InputStream input = getResourceAsStream(name.replace('.', '/') + ".class")) {
                        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES | ClassWriter.COMPUTE_MAXS);
                        ClassVisitor visitor = new WebRtcPlaybackGuard(writer);
                        new ClassReader(input).accept(visitor, 0);
                        byte[] bytes = writer.toByteArray();
                        type = defineClass(name, bytes, 0, bytes.length);
                    } catch (Exception error) {
                        throw new ClassNotFoundException(name, error);
                    }
                }
                if (resolve) resolveClass(type);
                return type;
            }
        };
        Object owner = loader.loadClass(WebRtcPlaybackGuard.CLASS_NAME.replace('/', '.'))
                .getConstructor(boolean.class).newInstance(latency);
        assertEquals(1920, invoke(owner, "initPlayout", 48000, 2, 1.0));
        track = (AudioTrack) field(owner, "audioTrack");
        return owner;
    }

    private void start(Object owner) throws Exception {
        assertEquals(true, invoke(owner, "startPlayout"));
        worker = (Thread) field(owner, "audioThread");
    }

    private static Object field(Object owner, String name) throws Exception {
        Field field = owner.getClass().getDeclaredField(name);
        field.setAccessible(true);
        return field.get(owner);
    }

    private static Object invoke(Object owner, String name, Object... args) throws Exception {
        Class<?>[] types = args.length == 0 ? new Class<?>[0] : new Class<?>[] { int.class, int.class, double.class };
        Method method = owner.getClass().getDeclaredMethod(name, types);
        method.setAccessible(true);
        try { return method.invoke(owner, args); }
        catch (InvocationTargetException error) { throw (Exception) error.getCause(); }
    }
}
