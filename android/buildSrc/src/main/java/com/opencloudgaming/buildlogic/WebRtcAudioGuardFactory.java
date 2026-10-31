package com.opencloudgaming.buildlogic;

import com.android.build.api.instrumentation.AsmClassVisitorFactory;
import com.android.build.api.instrumentation.ClassContext;
import com.android.build.api.instrumentation.ClassData;
import com.android.build.api.instrumentation.InstrumentationParameters;
import org.objectweb.asm.ClassVisitor;

/** Applies the audio teardown guard to WebRTC in both debug and release APKs. */
public abstract class WebRtcAudioGuardFactory
        implements AsmClassVisitorFactory<InstrumentationParameters.None> {
    @Override
    public boolean isInstrumentable(ClassData data) {
        String name = data.getClassName().replace('.', '/');
        return name.equals(WebRtcAudioGuard.CLASS_NAME) || name.equals(WebRtcPlaybackGuard.CLASS_NAME) ||
                name.equals(WebRtcPlaybackGuard.THREAD_NAME);
    }

    @Override
    public ClassVisitor createClassVisitor(ClassContext context, ClassVisitor next) {
        return context.getCurrentClassData().getClassName().replace('.', '/').equals(WebRtcAudioGuard.CLASS_NAME)
                ? new WebRtcAudioGuard(next) : new WebRtcPlaybackGuard(next);
    }
}
