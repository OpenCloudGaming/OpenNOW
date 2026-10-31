package com.opencloudgaming.buildlogic;

import android.os.HandlerThread;
import java.io.InputStream;
import java.lang.reflect.InvocationTargetException;
import java.util.List;
import org.junit.Before;
import org.junit.Test;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import org.webrtc.VideoCodecStatus;
import org.webrtc.VideoDecoder;
import static org.junit.Assert.*;

public class WebRtcTextureDeliveryTest {
    private Class<?> type;
    private Object helper;
    private TunedClassLoader loader;

    private static class TunedClassLoader extends ClassLoader {
        TunedClassLoader(ClassLoader parent) { super(parent); }
        Class<?> defineTuned(String name, byte[] bytes) {
            return defineClass(name.replace('/', '.'), bytes, 0, bytes.length);
        }
    }

    private Class<?> instrument(String name) throws Exception {
        byte[] original;
        try (InputStream stream = getClass().getResourceAsStream("/" + name + ".class")) {
            original = stream.readAllBytes();
        }
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES | ClassWriter.COMPUTE_MAXS);
        new ClassReader(original).accept(new WebRtcTextureDelivery(writer, name), 0);
        byte[] tuned = writer.toByteArray();
        return loader.defineTuned(name, tuned);
    }

    @Before
    public void setup() throws Exception {
        loader = new TunedClassLoader(getClass().getClassLoader());
        type = instrument(WebRtcTextureDelivery.TEXTURE_HELPER);
        helper = type.getConstructor().newInstance();
    }

    private void size(int width, int height) throws Exception {
        type.getMethod("opennowSetTextureSize", int.class, int.class).invoke(helper, width, height);
    }

    private void rotation(int rotation) throws Exception {
        type.getMethod("opennowSetFrameRotation", int.class).invoke(helper, rotation);
    }

    private Object requests() throws Exception { return type.getField("requests").get(helper); }

    @Test
    public void decoderActuallyUsesOptimizedEntryPoints() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        for (int i = 0; i < 60; i++) {
            decoder.getMethod("deliverTextureFrame", type, int.class, int.class, int.class)
                    .invoke(instance, helper, 1376, 640, 0);
        }
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void nullTextureHelperReturnsErrorAndClearsCallbackBeforeCodecStartup() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        decoder.getField("failHelper").set(instance, true);
        VideoDecoder.Callback callback = new VideoDecoder.Callback() {};
        assertEquals(VideoCodecStatus.ERROR, decoder.getMethod("initDecode", VideoDecoder.Settings.class,
                VideoDecoder.Callback.class).invoke(instance, new VideoDecoder.Settings(1920, 1080), callback));
        assertNull(decoder.getMethod("getCallback").invoke(instance));
        assertNull(decoder.getMethod("getHelper").invoke(instance));
        assertEquals(0, decoder.getField("codecInitializations").get(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        assertEquals(0, decoder.getField("codecReleases").get(instance));
    }

    @Test
    public void successfulTextureStartupPreservesCallbackListenerAndRelease() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        VideoDecoder.Callback callback = new VideoDecoder.Callback() {};
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("initDecode", VideoDecoder.Settings.class,
                VideoDecoder.Callback.class).invoke(instance, new VideoDecoder.Settings(1920, 1080), callback));
        Object decoderHelper = decoder.getMethod("getHelper").invoke(instance);
        assertSame(callback, decoder.getMethod("getCallback").invoke(instance));
        assertSame(instance, type.getField("listener").get(decoderHelper));
        assertEquals(1, decoder.getField("codecInitializations").get(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        assertEquals(1, decoder.getField("codecReleases").get(instance));
        assertNull(decoder.getMethod("getCallback").invoke(instance));
        assertNull(type.getField("listener").get(decoderHelper));
        assertEquals(1, type.getField("disposals").get(decoderHelper));
    }

    @Test
    public void textureStartupCanRecoverAfterNullHelperAndRelease() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        VideoDecoder.Callback callback = new VideoDecoder.Callback() {};
        decoder.getField("failHelper").set(instance, true);
        assertEquals(VideoCodecStatus.ERROR, decoder.getMethod("initDecode", VideoDecoder.Settings.class,
                VideoDecoder.Callback.class).invoke(instance, new VideoDecoder.Settings(1920, 1080), callback));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        decoder.getField("failHelper").set(instance, false);
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("initDecode", VideoDecoder.Settings.class,
                VideoDecoder.Callback.class).invoke(instance, new VideoDecoder.Settings(1920, 1080), callback));
        assertSame(callback, decoder.getMethod("getCallback").invoke(instance));
        assertEquals(1, decoder.getField("codecInitializations").get(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
    }

    @Test
    public void nonTextureStartupDoesNotRequireTextureHelper() throws Exception {
        Class<?> decoder = instrument(WebRtcTextureDelivery.DECODER);
        Object instance = decoder.getConstructor().newInstance();
        decoder.getField("textureMode").set(instance, false);
        decoder.getField("failHelper").set(instance, true);
        VideoDecoder.Callback callback = new VideoDecoder.Callback() {};
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("initDecode", VideoDecoder.Settings.class,
                VideoDecoder.Callback.class).invoke(instance, new VideoDecoder.Settings(1920, 1080), callback));
        assertSame(callback, decoder.getMethod("getCallback").invoke(instance));
        assertNull(decoder.getMethod("getHelper").invoke(instance));
        assertEquals(1, decoder.getField("codecInitializations").get(instance));
        assertEquals(VideoCodecStatus.OK, decoder.getMethod("release").invoke(instance));
        assertEquals(1, decoder.getField("codecReleases").get(instance));
    }

    @Test
    public void originalSettersRemainUnchangedForCameraAndOtherCallers() throws Exception {
        for (int i = 0; i < 2; i++) {
            type.getMethod("setTextureSize", int.class, int.class).invoke(helper, 1280, 720);
            type.getMethod("setFrameRotation", int.class).invoke(helper, 0);
        }
        assertEquals(List.of("size:1280x720", "rotation:0", "size:1280x720", "rotation:0"), requests());
    }

    @Test
    public void steadySixtyFpsPostsMetadataOnceEvenWhileWorkerIsDelayed() throws Exception {
        for (int i = 0; i < 60; i++) {
            size(1376, 640);
            rotation(0);
        }
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
        type.getMethod("drainWorker").invoke(helper);
        size(1376, 640);
        rotation(0);
        assertEquals(List.of(), requests());
        assertEquals(List.of("size:1376x640", "rotation:0"), type.getField("applied").get(helper));
    }

    @Test
    public void resolutionAndRotationTransitionsKeepTheirOrdering() throws Exception {
        size(1376, 640);
        rotation(0);
        size(1280, 720);
        rotation(90);
        size(1376, 640);
        rotation(0);
        assertEquals(List.of("size:1376x640", "rotation:0", "size:1280x720", "rotation:90",
                "size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void eitherDimensionChangingRequiresAnUpdate() throws Exception {
        size(1280, 720);
        size(1280, 800);
        size(1440, 800);
        assertEquals(List.of("size:1280x720", "size:1280x800", "size:1440x800"), requests());
    }

    @Test
    public void newHelperAfterReconnectStillReceivesInitialMetadata() throws Exception {
        size(1376, 640);
        rotation(0);
        helper = type.getConstructor().newInstance();
        size(1376, 640);
        rotation(0);
        assertEquals(List.of("size:1376x640", "rotation:0"), requests());
    }

    @Test
    public void invalidSizesAreNeverCachedOrSilenced() throws Exception {
        for (int[] dimensions : new int[][] {{0, 0}, {0, 720}, {1280, -1}, {0, 0}}) {
            try {
                size(dimensions[0], dimensions[1]);
                fail("Expected upstream size validation");
            } catch (InvocationTargetException e) {
                assertTrue(e.getCause() instanceof IllegalArgumentException);
            }
        }
        size(1280, 720);
        assertEquals(List.of("size:1280x720"), requests());
    }

    @Test
    public void failedSizeRequestCanBeRetriedWithSameDimensions() throws Exception {
        type.getField("failSize").set(helper, true);
        try {
            size(1376, 640);
            fail("Expected the platform error");
        } catch (InvocationTargetException e) {
            assertTrue(e.getCause() instanceof IllegalStateException);
        }
        type.getField("failSize").set(helper, false);
        size(1376, 640);
        assertEquals(List.of("size:1376x640"), requests());
    }

    @Test
    public void decoderTextureThreadGetsDisplayPriorityWithoutChangingCaptureThreads() throws Exception {
        HandlerThread decoder = (HandlerThread) type.getMethod("create", String.class)
                .invoke(null, "decoder-texture-thread");
        HandlerThread camera = (HandlerThread) type.getMethod("create", String.class)
                .invoke(null, "camera-capture-thread");
        assertEquals(-4, decoder.priority);
        assertEquals("decoder-texture-thread", decoder.name);
        assertEquals(0, camera.priority);
    }

    @Test
    public void displayRendererGetsDisplayPriority() throws Exception {
        Class<?> egl = instrument(WebRtcTextureDelivery.EGL_THREAD);
        HandlerThread thread = (HandlerThread) egl.getMethod("create").invoke(null);
        assertEquals(-4, thread.priority);
        assertEquals("EglThread", thread.name);
    }

    @Test
    public void dependencyShapeChangesFailBuildInsteadOfSilentlySkippingTuning() {
        for (String owner : List.of(WebRtcTextureDelivery.TEXTURE_HELPER,
                WebRtcTextureDelivery.EGL_THREAD, WebRtcTextureDelivery.DECODER)) {
            try {
                new WebRtcTextureDelivery(new ClassWriter(0), owner).visitEnd();
                fail("Expected a pinned dependency mismatch");
            } catch (IllegalStateException e) {
                assertTrue(e.getMessage().contains("WebRTC texture API changed"));
            }
        }
    }

    @Test
    public void missingDecoderInitializationGuardSiteFailsBuild() throws Exception {
        ClassWriter stripped = new ClassWriter(0);
        try (InputStream stream = getClass().getResourceAsStream("/" + WebRtcTextureDelivery.DECODER + ".class")) {
            new ClassReader(stream).accept(new ClassVisitor(Opcodes.ASM9, stripped) {
                @Override
                public MethodVisitor visitMethod(int access, String name, String descriptor,
                        String signature, String[] exceptions) {
                    return name.equals("initDecode") ? null
                            : super.visitMethod(access, name, descriptor, signature, exceptions);
                }
            }, 0);
        }
        IllegalStateException error = assertThrows(IllegalStateException.class,
                () -> new ClassReader(stripped.toByteArray()).accept(new WebRtcTextureDelivery(
                        new ClassWriter(0), WebRtcTextureDelivery.DECODER), 0));
        assertTrue(error.getMessage().contains("helperInitializations=0"));
    }
}
