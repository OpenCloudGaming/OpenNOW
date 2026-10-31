package org.webrtc;

import static org.junit.Assert.*;

import java.lang.reflect.Proxy;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import org.junit.Test;

public class OpenNowMediaCodecTuningTest {
  @Test
  public void tunesCreationAndStartupBeforeForwardingBufferOperations() throws Exception {
    List<String> events = new ArrayList<>();
    ByteBuffer input = ByteBuffer.allocate(16);
    MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
        MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
        (proxy, method, args) -> {
          events.add(method.getName());
          switch (method.getName()) {
            case "dequeueInputBuffer":
              assertEquals(700L, args[0]);
              return 4;
            case "getInputBuffer":
              assertEquals(4, args[0]);
              return input;
            case "queueInputBuffer":
              assertArrayEquals(new Object[] {4, 2, 10, 360L, 1}, args);
              return null;
            case "dequeueOutputBuffer":
              assertNull(args[0]);
              assertEquals(800L, args[1]);
              return 9;
            case "releaseOutputBuffer":
              assertArrayEquals(new Object[] {9, true}, args);
              return null;
            default:
              return null;
          }
        });
    MediaCodecWrapperFactory factory = name -> {
      assertEquals("decoder.lowlatency", name);
      events.add("create");
      return codec;
    };
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
        factory, new OpenNowMediaCodecTuning.Tuning() {
          @Override public String selectCodecName(String name) { return name + ".lowlatency"; }
          @Override public void configure(String name, android.media.MediaFormat format) {
            assertEquals("decoder.lowlatency", name);
            events.add("tuneConfigure");
          }
          @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {
            events.add("tuneStarted");
            setter.setParameters(null);
          }
        });
    MediaCodecWrapper result = tuned.createByCodecName("decoder");
    result.configure(null, null, null, 0);
    result.start();
    assertEquals(Arrays.asList("create", "tuneConfigure", "configure", "start", "tuneStarted", "setParameters"), events);
    assertEquals(4, result.dequeueInputBuffer(700));
    assertSame(input, result.getInputBuffer(4));
    result.queueInputBuffer(4, 2, 10, 360, 1);
    assertEquals(9, result.dequeueOutputBuffer(null, 800));
    result.releaseOutputBuffer(9, true);
    result.flush();
    result.stop();
    result.release();
    assertEquals(Arrays.asList("dequeueInputBuffer", "getInputBuffer", "queueInputBuffer", "dequeueOutputBuffer",
        "releaseOutputBuffer", "flush", "stop", "release"), events.subList(6, events.size()));
  }

  @Test
  public void observesFormatChangesWithoutInspectingEveryOutputBuffer() throws Exception {
    List<String> events = new ArrayList<>();
    IllegalStateException failure = new IllegalStateException("codec released");
    boolean[] formatFails = {false};
    MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
        MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
        (proxy, method, args) -> {
          if (method.getName().equals("getOutputFormat")) {
            if (args == null || args.length == 0) {
              events.add("format");
              if (formatFails[0]) throw failure;
            } else {
              assertEquals(7, args[0]);
              events.add("bufferFormat");
            }
          }
          // Framework MediaFormat cannot be constructed in this JVM test; the bridge must
          // pass even this placeholder through without reading or replacing the format.
          return null;
        });
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
        (MediaCodecWrapperFactory) name -> codec, new OpenNowMediaCodecTuning.Tuning() {
          @Override public String selectCodecName(String name) { return name; }
          @Override public void configure(String name, android.media.MediaFormat format) {}
          @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
          @Override public void outputFormat(String name, android.media.MediaFormat format) {
            assertEquals("decoder", name);
            assertNull(format);
            events.add("observed");
            throw new IllegalStateException("diagnostics unavailable");
          }
        });
    MediaCodecWrapper result = tuned.createByCodecName("decoder");
    assertNull(result.getOutputFormat());
    assertNull(result.getOutputFormat(7));
    assertEquals(Arrays.asList("format", "observed", "bufferFormat"), events);
    formatFails[0] = true;
    assertSame(failure, assertThrows(IllegalStateException.class, result::getOutputFormat));
    assertEquals(Arrays.asList("format", "observed", "bufferFormat", "format"), events);
  }

  @Test
  public void codecCreationFailureKeepsOriginalException() {
    java.io.IOException failure = new java.io.IOException("codec unavailable");
    MediaCodecWrapperFactory factory = name -> { throw failure; };
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(factory, noTuning());
    assertSame(failure, assertThrows(java.io.IOException.class, () -> tuned.createByCodecName("decoder")));
  }

  @Test
  public void bufferFailureKeepsOriginalException() throws Exception {
    IllegalStateException failure = new IllegalStateException("codec released");
    int[] notifications = {0};
    MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
        MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
        (proxy, method, args) -> { throw failure; });
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
        (MediaCodecWrapperFactory) name -> codec, new OpenNowMediaCodecTuning.Tuning() {
          @Override public String selectCodecName(String name) { return name; }
          @Override public void configure(String name, android.media.MediaFormat format) {}
          @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
          @Override public void inputQueued() { notifications[0]++; }
          @Override public void outputDequeued(boolean codecConfig) { notifications[0]++; }
          @Override public void outputReleased(boolean render) { notifications[0]++; }
        });
    MediaCodecWrapper result = tuned.createByCodecName("decoder");
    assertSame(failure, assertThrows(IllegalStateException.class, () -> result.dequeueInputBuffer(0)));
    assertSame(failure, assertThrows(IllegalStateException.class, () -> result.queueInputBuffer(0, 0, 0, 0, 0)));
    assertSame(failure, assertThrows(IllegalStateException.class, () -> result.dequeueOutputBuffer(null, 0)));
    assertSame(failure, assertThrows(IllegalStateException.class, () -> result.releaseOutputBuffer(0, true)));
    assertEquals(0, notifications[0]);
  }

  private OpenNowMediaCodecTuning.Tuning noTuning() {
    return new OpenNowMediaCodecTuning.Tuning() {
      @Override public String selectCodecName(String name) { return name; }
      @Override public void configure(String name, android.media.MediaFormat format) {}
      @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
    };
  }

  @Test
  public void releasesRejectedStartupBeforeFallbackAndNeverReleasesCodecTwice() throws Exception {
    for (String phase : Arrays.asList("configure", "start")) {
      List<String> calls = new ArrayList<>();
      IllegalStateException failure = new IllegalStateException("rejected " + phase);
      MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
          MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
          (proxy, method, args) -> {
            calls.add(method.getName());
            if (method.getName().equals(phase)) throw failure;
            return null;
          });
      MediaCodecWrapperFactory factory = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
          (MediaCodecWrapperFactory) name -> codec, new OpenNowMediaCodecTuning.Tuning() {
            @Override public String selectCodecName(String name) { return name; }
            @Override public void configure(String name, android.media.MediaFormat format) {}
            @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
            @Override public void initializationFailed(String name, String at, RuntimeException error) {
              assertEquals(Arrays.asList(phase, "release"), calls);
            }
          });
      MediaCodecWrapper result = factory.createByCodecName("decoder");
      assertSame(failure, assertThrows(IllegalStateException.class, () -> {
        if (phase.equals("configure")) result.configure(null, null, null, 0);
        else result.start();
      }));
      // The pinned WebRTC startup path never makes this call. A future release that does
      // must still be safe, as must a caller cleaning up a failed decoder explicitly.
      result.release();
      result.release();
      assertEquals(Arrays.asList(phase, "release"), calls);
    }
  }

  @Test
  public void cleanupFailureIsReportedAndPreservesBothCauses() throws Exception {
    IllegalStateException startupFailure = new IllegalStateException("configure failed");
    IllegalStateException cleanupFailure = new IllegalStateException("release failed");
    List<String> observed = new ArrayList<>();
    MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
        MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
        (proxy, method, args) -> { throw method.getName().equals("release") ? cleanupFailure : startupFailure; });
    MediaCodecWrapperFactory factory = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
        (MediaCodecWrapperFactory) name -> codec, new OpenNowMediaCodecTuning.Tuning() {
          @Override public String selectCodecName(String name) { return name; }
          @Override public void configure(String name, android.media.MediaFormat format) {}
          @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
          @Override public void initializationCleanupFailed(String name, RuntimeException error) {
            assertSame(cleanupFailure, error);
            observed.add("cleanup-failed");
          }
          @Override public void initializationFailed(String name, String phase, RuntimeException error) {
            assertSame(startupFailure, error);
            assertArrayEquals(new Throwable[] {cleanupFailure}, error.getSuppressed());
            observed.add("startup-failed");
          }
        });
    MediaCodecWrapper result = factory.createByCodecName("decoder");
    assertSame(startupFailure, assertThrows(IllegalStateException.class, () -> result.configure(null, null, null, 0)));
    assertEquals(Arrays.asList("cleanup-failed", "startup-failed"), observed);
  }

  @Test
  public void retainsPlatformStartupCauseBeforeWebRtcConvertsItToStatus() throws Exception {
    for (String phase : Arrays.asList("configure", "start")) {
      IllegalStateException failure = new IllegalStateException("platform rejected " + phase);
      List<String> observed = new ArrayList<>();
      MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
          MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
          (proxy, method, args) -> { throw failure; });
      MediaCodecWrapperFactory factory = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
          (MediaCodecWrapperFactory) name -> codec, new OpenNowMediaCodecTuning.Tuning() {
            @Override public String selectCodecName(String name) { return name; }
            @Override public void configure(String name, android.media.MediaFormat format) {}
            @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
            @Override public void initializationFailed(String name, String at, RuntimeException error) {
              assertEquals("decoder", name);
              assertSame(failure, error);
              observed.add(at);
              throw new IllegalStateException("observer failed");
            }
          });
      MediaCodecWrapper result = factory.createByCodecName("decoder");
      assertSame(failure, assertThrows(IllegalStateException.class, () -> {
        if (phase.equals("configure")) result.configure(null, null, null, 0);
        else result.start();
      }));
      assertEquals(Arrays.asList(phase), observed);
    }
  }
}
