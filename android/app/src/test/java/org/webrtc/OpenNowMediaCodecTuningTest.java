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
  public void codecCreationFailureKeepsOriginalException() {
    java.io.IOException failure = new java.io.IOException("codec unavailable");
    MediaCodecWrapperFactory factory = name -> { throw failure; };
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(factory, noTuning());
    assertSame(failure, assertThrows(java.io.IOException.class, () -> tuned.createByCodecName("decoder")));
  }

  @Test
  public void bufferFailureKeepsOriginalException() throws Exception {
    IllegalStateException failure = new IllegalStateException("codec released");
    MediaCodecWrapper codec = (MediaCodecWrapper) Proxy.newProxyInstance(
        MediaCodecWrapper.class.getClassLoader(), new Class<?>[] {MediaCodecWrapper.class},
        (proxy, method, args) -> { throw failure; });
    MediaCodecWrapperFactory tuned = (MediaCodecWrapperFactory) OpenNowMediaCodecTuning.wrapFactory(
        (MediaCodecWrapperFactory) name -> codec, noTuning());
    MediaCodecWrapper result = tuned.createByCodecName("decoder");
    assertSame(failure, assertThrows(IllegalStateException.class, () -> result.dequeueInputBuffer(0)));
  }

  private OpenNowMediaCodecTuning.Tuning noTuning() {
    return new OpenNowMediaCodecTuning.Tuning() {
      @Override public String selectCodecName(String name) { return name; }
      @Override public void configure(String name, android.media.MediaFormat format) {}
      @Override public void started(String name, OpenNowMediaCodecTuning.ParameterSetter setter) {}
    };
  }
}
