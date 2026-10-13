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

    default StartupTuning configureCodec(String codecName, MediaFormat format, ConfigurationAttempt attempt) {
      configure(codecName, format);
      attempt.configure(format);
      return parameters -> started(codecName, parameters);
    }
  }

  public interface ConfigurationAttempt {
    void configure(MediaFormat format);
    void recreate();
  }

  public interface StartupTuning {
    void started(ParameterSetter parameters);
  }

  private OpenNowMediaCodecTuning() {}

  public static Object wrapFactory(Object factory, Tuning tuning) {
    if (!(factory instanceof MediaCodecWrapperFactory)) {
      throw new IllegalArgumentException("Not a WebRTC MediaCodecWrapperFactory");
    }
    MediaCodecWrapperFactory delegate = (MediaCodecWrapperFactory) factory;
    return (MediaCodecWrapperFactory) originalName -> {
      String name = tuning.selectCodecName(originalName);
      return new TunedCodec(delegate.createByCodecName(name), delegate, name, tuning);
    };
  }

  private static final class TunedCodec implements MediaCodecWrapper {
    private MediaCodecWrapper delegate;
    private final MediaCodecWrapperFactory factory;
    private final String name;
    private final Tuning tuning;
    private StartupTuning startupTuning;

    TunedCodec(MediaCodecWrapper delegate, MediaCodecWrapperFactory factory, String name, Tuning tuning) {
      this.delegate = delegate;
      this.factory = factory;
      this.name = name;
      this.tuning = tuning;
    }

    @Override public void configure(MediaFormat format, Surface surface, MediaCrypto crypto, int flags) {
      startupTuning = null;
      startupTuning = tuning.configureCodec(name, format, new ConfigurationAttempt() {
        @Override public void configure(MediaFormat candidate) {
          delegate.configure(candidate, surface, crypto, flags);
        }
        @Override public void recreate() {
          delegate.release();
          try {
            delegate = factory.createByCodecName(name);
          } catch (java.io.IOException error) {
            throw new IllegalStateException("Could not recreate MediaCodec", error);
          }
        }
      });
    }
    @Override public void start() {
      delegate.start();
      if (startupTuning != null) startupTuning.started(delegate::setParameters);
      else tuning.started(name, delegate::setParameters);
    }
    @Override public void flush() { delegate.flush(); }
    @Override public void stop() { delegate.stop(); }
    @Override public void release() { delegate.release(); }
    @Override public int dequeueInputBuffer(long timeoutUs) { return delegate.dequeueInputBuffer(timeoutUs); }
    @Override public void queueInputBuffer(int index, int offset, int size, long ptsUs, int flags) {
      delegate.queueInputBuffer(index, offset, size, ptsUs, flags);
    }
    @Override public int dequeueOutputBuffer(MediaCodec.BufferInfo info, long timeoutUs) {
      return delegate.dequeueOutputBuffer(info, timeoutUs);
    }
    @Override public void releaseOutputBuffer(int index, boolean render) { delegate.releaseOutputBuffer(index, render); }
    @Override public MediaFormat getInputFormat() { return delegate.getInputFormat(); }
    @Override public MediaFormat getOutputFormat() { return delegate.getOutputFormat(); }
    @Override public MediaFormat getOutputFormat(int index) { return delegate.getOutputFormat(index); }
    @Override public ByteBuffer getInputBuffer(int index) { return delegate.getInputBuffer(index); }
    @Override public ByteBuffer getOutputBuffer(int index) { return delegate.getOutputBuffer(index); }
    @Override public Surface createInputSurface() { return delegate.createInputSurface(); }
    @Override public void setParameters(Bundle parameters) { delegate.setParameters(parameters); }
    @Override public MediaCodecInfo getCodecInfo() { return delegate.getCodecInfo(); }
  }
}
