package org.webrtc;

import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaCrypto;
import android.media.MediaFormat;
import android.os.Bundle;
import android.view.Surface;
import java.nio.ByteBuffer;

/**
 * Bridge to WebRTC's package-private codec wrapper. Only creation/configure/start need tuning;
 * buffer operations use typed calls without reflection, boxing or proxy argument arrays.
 * Compiling against the pinned WebRTC interface also makes API changes fail at build time.
 */
public final class OpenNowMediaCodecTuning {
  public interface ParameterSetter {
    void setParameters(Bundle parameters);
  }

  public interface Tuning {
    String selectCodecName(String originalName);
    void configure(String codecName, MediaFormat format);
    void started(String codecName, ParameterSetter parameters);
    default void initializationFailed(String codecName, String phase, RuntimeException error) {}
    default void initializationCleanupFailed(String codecName, RuntimeException error) {}
    default void inputQueued() {}
    default void outputDequeued(boolean codecConfig) {}
    default void outputReleased(boolean render) {}
    default void outputFormat(String codecName, MediaFormat format) {}
  }

  private OpenNowMediaCodecTuning() {}

  /** The pinned Android decoder copies compressed input into MediaCodec before decode returns. */
  public static boolean consumesEncodedInputSynchronously(VideoDecoder decoder) {
    // A subclass could override decode() and retain input; its ownership must be audited separately.
    return decoder != null && decoder.getClass() == AndroidVideoDecoder.class;
  }

  public static Object wrapFactory(Object factory, Tuning tuning) {
    if (!(factory instanceof MediaCodecWrapperFactory)) {
      throw new IllegalArgumentException("Not a WebRTC MediaCodecWrapperFactory");
    }
    MediaCodecWrapperFactory delegate = (MediaCodecWrapperFactory) factory;
    return (MediaCodecWrapperFactory) originalName -> {
      String name = tuning.selectCodecName(originalName);
      return new TunedCodec(delegate.createByCodecName(name), name, tuning);
    };
  }

  private static final class TunedCodec implements MediaCodecWrapper {
    private final MediaCodecWrapper delegate;
    private final String name;
    private final Tuning tuning;
    private boolean released;

    TunedCodec(MediaCodecWrapper delegate, String name, Tuning tuning) {
      this.delegate = delegate;
      this.name = name;
      this.tuning = tuning;
    }

    @Override public void configure(MediaFormat format, Surface surface, MediaCrypto crypto, int flags) {
      try {
        tuning.configure(name, format);
        delegate.configure(format, surface, crypto, flags);
      } catch (RuntimeException error) {
        initializationFailed("configure", error);
        throw error;
      }
    }
    @Override public void start() {
      try {
        delegate.start();
        tuning.started(name, delegate::setParameters);
      } catch (RuntimeException error) {
        initializationFailed("start", error);
        throw error;
      }
    }
    private void initializationFailed(String phase, RuntimeException error) {
      // In the pinned AndroidVideoDecoder, releaseInternal() skips codec.release() until
      // running=true. That flag is set AFTER configure/start, so failed startup leaks a codec
      // unless this owner releases it. Free it before any fresh-decoder retry can run.
      try {
        release();
      } catch (RuntimeException cleanupError) {
        if (cleanupError != error) error.addSuppressed(cleanupError);
        try {
          tuning.initializationCleanupFailed(name, cleanupError);
        } catch (RuntimeException ignored) {}
      }
      // AndroidVideoDecoder turns platform exceptions into status codes. Retain the cause first.
      try {
        tuning.initializationFailed(name, phase, error);
      } catch (RuntimeException ignored) {}
    }
    @Override public void flush() { delegate.flush(); }
    @Override public void stop() { delegate.stop(); }
    @Override public void release() {
      if (released) return;
      released = true;
      delegate.release();
    }
    @Override public int dequeueInputBuffer(long timeoutUs) { return delegate.dequeueInputBuffer(timeoutUs); }
    @Override public void queueInputBuffer(int index, int offset, int size, long ptsUs, int flags) {
      delegate.queueInputBuffer(index, offset, size, ptsUs, flags);
      tuning.inputQueued();
    }
    @Override public int dequeueOutputBuffer(MediaCodec.BufferInfo info, long timeoutUs) {
      int index = delegate.dequeueOutputBuffer(info, timeoutUs);
      if (index >= 0 && info != null) {
        tuning.outputDequeued((info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0);
      }
      return index;
    }
    @Override public void releaseOutputBuffer(int index, boolean render) {
      delegate.releaseOutputBuffer(index, render);
      tuning.outputReleased(render);
    }
    @Override public MediaFormat getInputFormat() { return delegate.getInputFormat(); }
    @Override public MediaFormat getOutputFormat() {
      MediaFormat format = delegate.getOutputFormat();
      // AndroidVideoDecoder reads this on format changes. Keep diagnostics off the per-frame
      // getOutputFormat(index) path, and never let an observer interrupt video delivery.
      try {
        tuning.outputFormat(name, format);
      } catch (RuntimeException ignored) {}
      return format;
    }
    @Override public MediaFormat getOutputFormat(int index) { return delegate.getOutputFormat(index); }
    @Override public ByteBuffer getInputBuffer(int index) { return delegate.getInputBuffer(index); }
    @Override public ByteBuffer getOutputBuffer(int index) { return delegate.getOutputBuffer(index); }
    @Override public Surface createInputSurface() { return delegate.createInputSurface(); }
    @Override public void setParameters(Bundle parameters) { delegate.setParameters(parameters); }
    @Override public MediaCodecInfo getCodecInfo() { return delegate.getCodecInfo(); }
  }
}
