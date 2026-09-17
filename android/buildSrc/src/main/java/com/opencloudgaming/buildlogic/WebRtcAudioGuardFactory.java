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
        return data.getClassName().equals(WebRtcAudioGuard.CLASS_NAME.replace('/', '.'));
    }

    @Override
    public ClassVisitor createClassVisitor(ClassContext context, ClassVisitor next) {
        return new WebRtcAudioGuard(next);
    }
}
