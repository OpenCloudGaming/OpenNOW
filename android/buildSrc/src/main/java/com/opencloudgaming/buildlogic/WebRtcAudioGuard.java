package com.opencloudgaming.buildlogic;

import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.Label;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

/**
 * WebRTC 144.7559.14 can finish a blocking audio write after stopPlayout's join timeout
 * has released/nullified its AudioTrack. Buffer tuning is optional at that point.
 * Guard only that operation; preserve upstream tuning and bounded underrun recovery.
 * Remove once the pinned dependency handles this race itself.
 */
public final class WebRtcAudioGuard extends ClassVisitor implements Opcodes {
    public static final String CLASS_NAME = "org/webrtc/audio/LowLatencyAudioBufferManager";
    private static final String METHOD = "maybeAdjustBufferSize";
    private static final String DESC = "(Landroid/media/AudioTrack;)V";
    private static final String GUARDED = "opennowAdjustBufferSize";
    private boolean found;

    public WebRtcAudioGuard(ClassVisitor next) {
        super(ASM9, next);
    }

    @Override
    public MethodVisitor visitMethod(int access, String name, String desc,
            String signature, String[] exceptions) {
        if (name.equals(METHOD) && desc.equals(DESC) && (access & ACC_STATIC) == 0) {
            found = true;
            // Retain the upstream method body behind the guard.
            return super.visitMethod(ACC_PRIVATE, GUARDED, desc, signature, exceptions);
        }
        return super.visitMethod(access, name, desc, signature, exceptions);
    }

    @Override
    public void visitEnd() {
        if (!found) {
            throw new IllegalStateException("WebRTC audio API changed; review the low-latency teardown guard");
        }
        MethodVisitor method = super.visitMethod(ACC_PUBLIC, METHOD, DESC, null, null);
        method.visitCode();
        Label done = new Label();
        Label start = new Label();
        Label end = new Label();
        Label invalidState = new Label();
        method.visitTryCatchBlock(start, end, invalidState, "java/lang/IllegalStateException");
        method.visitVarInsn(ALOAD, 1);
        method.visitJumpInsn(IFNULL, done);
        method.visitLabel(start);
        method.visitVarInsn(ALOAD, 1);
        method.visitMethodInsn(INVOKEVIRTUAL, "android/media/AudioTrack", "getState", "()I", false);
        method.visitInsn(ICONST_1); // AudioTrack.STATE_INITIALIZED
        method.visitJumpInsn(IF_ICMPNE, done);
        method.visitVarInsn(ALOAD, 0);
        method.visitVarInsn(ALOAD, 1);
        method.visitMethodInsn(INVOKESPECIAL, CLASS_NAME, GUARDED, DESC, false);
        method.visitLabel(end);
        method.visitJumpInsn(GOTO, done);
        method.visitLabel(invalidState);
        // release() can race the state check above. No buffer work remains during teardown.
        method.visitInsn(POP);
        method.visitLabel(done);
        method.visitInsn(RETURN);
        method.visitMaxs(0, 0); // AGP computes frames/maxima for the instrumented method.
        method.visitEnd();
        super.visitEnd();
    }
}
