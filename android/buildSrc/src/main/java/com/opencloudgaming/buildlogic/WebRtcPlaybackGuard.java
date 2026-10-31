package com.opencloudgaming.buildlogic;

import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.FieldVisitor;
import org.objectweb.asm.Label;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

public final class WebRtcPlaybackGuard extends ClassVisitor implements Opcodes {
    public static final String CLASS_NAME = "org/webrtc/audio/WebRtcAudioTrack";
    public static final String THREAD_NAME = CLASS_NAME + "$AudioTrackThread";
    private static final String THREAD_DESC = "L" + THREAD_NAME + ";";
    private static final String DEFERRED = "opennowDeferredRelease";
    private boolean worker;
    private boolean initFound;
    private boolean startFound;
    private boolean stopFound;
    private boolean releaseFound;
    private boolean callbackFound;
    private int fields;
    private int joins;
    private int fetches;

    public WebRtcPlaybackGuard(ClassVisitor next) {
        super(ASM9, next);
    }

    @Override
    public FieldVisitor visitField(int access, String name, String desc, String signature, Object value) {
        if (name.equals(DEFERRED)) throw new IllegalStateException("WebRTC playback guard field already exists");
        if (worker) {
            if (name.equals("keepAlive") && desc.equals("Z") && (access & ACC_VOLATILE) != 0) fields |= 1;
            if (name.equals("this$0") && desc.equals("L" + CLASS_NAME + ";")) fields |= 2;
        } else {
            if (name.equals("audioThread") && desc.equals(THREAD_DESC)) fields |= 1;
            if (name.equals("audioTrack") && desc.equals("Landroid/media/AudioTrack;")) fields |= 2;
            if (name.equals("byteBuffer") && desc.equals("Ljava/nio/ByteBuffer;")) fields |= 4;
        }
        return super.visitField(access, name, desc, signature, value);
    }

    @Override
    public void visit(int version, int access, String name, String signature, String parent, String[] interfaces) {
        if (!name.equals(CLASS_NAME) && !name.equals(THREAD_NAME)) {
            throw new IllegalStateException("Unexpected WebRTC playback class: " + name);
        }
        worker = name.equals(THREAD_NAME);
        super.visit(version, access, name, signature, parent, interfaces);
    }

    @Override
    public MethodVisitor visitMethod(int access, String name, String desc, String signature, String[] exceptions) {
        if (!worker && name.equals("doAudioTrackStateCallback") && desc.equals("(I)V")) callbackFound = true;
        if (worker && name.equals("run") && desc.equals("()V")) {
            return new MethodVisitor(ASM9, super.visitMethod(ACC_PRIVATE, "opennowRun", desc, signature, exceptions)) {
                @Override
                public void visitCode() {
                    super.visitCode();
                    Label active = new Label();
                    visitVarInsn(ALOAD, 0);
                    visitFieldInsn(GETFIELD, THREAD_NAME, "keepAlive", "Z");
                    visitJumpInsn(IFNE, active);
                    visitInsn(RETURN);
                    visitLabel(active);
                }

                @Override
                public void visitMethodInsn(int opcode, String owner, String method, String descriptor, boolean itf) {
                    super.visitMethodInsn(opcode, owner, method, descriptor, itf);
                    if (opcode == INVOKESTATIC && owner.equals(CLASS_NAME) &&
                            method.equals("nativeGetPlayoutData") && descriptor.equals("(JI)V")) {
                        fetches++;
                        Label active = new Label();
                        visitVarInsn(ALOAD, 0);
                        visitFieldInsn(GETFIELD, THREAD_NAME, "keepAlive", "Z");
                        visitJumpInsn(IFNE, active);
                        visitInsn(RETURN);
                        visitLabel(active);
                    }
                }
            };
        }
        if (!worker && name.equals("releaseAudioResources") && desc.equals("()V")) {
            releaseFound = true;
            return super.visitMethod(access, "opennowReleaseAudioResources", desc, signature, exceptions);
        }
        if (!worker && name.equals("initPlayout") && desc.equals("(IID)I")) {
            initFound = true;
            return new MethodVisitor(ASM9, super.visitMethod(access | ACC_SYNCHRONIZED, name, desc, signature, exceptions)) {
                @Override
                public void visitCode() {
                    super.visitCode();
                    Label ready = new Label();
                    loadWorker(this);
                    visitJumpInsn(IFNULL, ready);
                    loadWorker(this);
                    visitMethodInsn(INVOKEVIRTUAL, "java/lang/Thread", "isAlive", "()Z", false);
                    Label finished = new Label();
                    visitJumpInsn(IFEQ, finished);
                    visitVarInsn(ALOAD, 0);
                    visitLdcInsn("Previous playback worker is still running");
                    visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "reportWebRtcAudioTrackInitError", "(Ljava/lang/String;)V", false);
                    visitInsn(ICONST_M1);
                    visitInsn(IRETURN);
                    visitLabel(finished);
                    visitVarInsn(ALOAD, 0);
                    visitInsn(ACONST_NULL);
                    visitFieldInsn(PUTFIELD, CLASS_NAME, "audioThread", THREAD_DESC);
                    visitVarInsn(ALOAD, 0);
                    visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowReleaseAudioResources", "()V", false);
                    visitLabel(ready);
                    visitVarInsn(ALOAD, 0);
                    visitInsn(ICONST_0);
                    visitFieldInsn(PUTFIELD, CLASS_NAME, DEFERRED, "Z");
                }
            };
        }
        if (!worker && name.equals("startPlayout") && desc.equals("()Z")) {
            startFound = true;
            return new MethodVisitor(ASM9, super.visitMethod(access | ACC_SYNCHRONIZED, name, desc, signature, exceptions)) {
                @Override
                public void visitCode() {
                    super.visitCode();
                    Label ready = new Label();
                    loadWorker(this);
                    visitJumpInsn(IFNULL, ready);
                    visitVarInsn(ALOAD, 0);
                    visitFieldInsn(GETSTATIC, "org/webrtc/audio/JavaAudioDeviceModule$AudioTrackStartErrorCode",
                            "AUDIO_TRACK_START_EXCEPTION", "Lorg/webrtc/audio/JavaAudioDeviceModule$AudioTrackStartErrorCode;");
                    visitLdcInsn("Previous playback worker has not been retired");
                    visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "reportWebRtcAudioTrackStartError",
                            "(Lorg/webrtc/audio/JavaAudioDeviceModule$AudioTrackStartErrorCode;Ljava/lang/String;)V", false);
                    visitInsn(ICONST_0);
                    visitInsn(IRETURN);
                    visitLabel(ready);
                }
            };
        }
        if (!worker && name.equals("stopPlayout") && desc.equals("()Z")) {
            stopFound = true;
            return new MethodVisitor(ASM9, super.visitMethod(access, name, desc, signature, exceptions)) {
                @Override
                public void visitCode() {
                    super.visitCode();
                    Label stopped = new Label();
                    visitVarInsn(ALOAD, 0);
                    visitFieldInsn(GETFIELD, CLASS_NAME, DEFERRED, "Z");
                    visitJumpInsn(IFNE, stopped);
                    loadWorker(this);
                    Label active = new Label();
                    visitJumpInsn(IFNONNULL, active);
                    visitLabel(stopped);
                    visitInsn(ICONST_1);
                    visitInsn(IRETURN);
                    visitLabel(active);
                }

                @Override
                public void visitMethodInsn(int opcode, String owner, String method, String descriptor, boolean itf) {
                    super.visitMethodInsn(opcode, owner, method, descriptor, itf);
                    if (opcode == INVOKESTATIC && owner.equals("org/webrtc/ThreadUtils") &&
                            method.equals("joinUninterruptibly") && descriptor.equals("(Ljava/lang/Thread;J)Z")) {
                        joins++;
                        Label joined = new Label();
                        visitInsn(DUP);
                        visitJumpInsn(IFNE, joined);
                        visitInsn(POP);
                        visitVarInsn(ALOAD, 0);
                        visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowStopTimedOut", "()V", false);
                        visitInsn(ICONST_1);
                        visitInsn(IRETURN);
                        visitLabel(joined);
                    }
                }
            };
        }
        return super.visitMethod(access, name, desc, signature, exceptions);
    }

    @Override
    public void visitEnd() {
        if (worker) {
            if (fetches != 1 || fields != 3) {
                throw new IllegalStateException("WebRTC playback fetch changed; review worker ownership guard");
            }
            emitRun();
        } else {
            if (!initFound || !startFound || !stopFound || !releaseFound || !callbackFound || joins != 1 || fields != 7) {
                throw new IllegalStateException("WebRTC playback lifecycle changed; review worker ownership guard");
            }
            super.visitField(ACC_PRIVATE | ACC_VOLATILE, DEFERRED, "Z", null, null).visitEnd();
            emitRelease();
            emitTimeout();
            emitFinished();
        }
        super.visitEnd();
    }

    private static void loadWorker(MethodVisitor method) {
        method.visitVarInsn(ALOAD, 0);
        method.visitFieldInsn(GETFIELD, CLASS_NAME, "audioThread", THREAD_DESC);
    }

    private void emitRelease() {
        MethodVisitor method = super.visitMethod(ACC_PRIVATE | ACC_SYNCHRONIZED, "releaseAudioResources", "()V", null, null);
        method.visitCode();
        Label release = new Label();
        loadWorker(method);
        method.visitJumpInsn(IFNULL, release);
        loadWorker(method);
        method.visitMethodInsn(INVOKEVIRTUAL, "java/lang/Thread", "isAlive", "()Z", false);
        method.visitJumpInsn(IFEQ, release);
        method.visitVarInsn(ALOAD, 0);
        method.visitInsn(ICONST_1);
        method.visitFieldInsn(PUTFIELD, CLASS_NAME, DEFERRED, "Z");
        method.visitInsn(RETURN);
        method.visitLabel(release);
        method.visitVarInsn(ALOAD, 0);
        method.visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowReleaseAudioResources", "()V", false);
        method.visitInsn(RETURN);
        method.visitMaxs(0, 0);
        method.visitEnd();
    }

    private void emitTimeout() {
        MethodVisitor method = super.visitMethod(ACC_PRIVATE | ACC_SYNCHRONIZED, "opennowStopTimedOut", "()V", null, null);
        method.visitCode();
        method.visitVarInsn(ALOAD, 0);
        method.visitInsn(ICONST_1);
        method.visitFieldInsn(PUTFIELD, CLASS_NAME, DEFERRED, "Z");
        method.visitLdcInsn("WebRtcAudioTrackExternal");
        method.visitLdcInsn("Playback join timed out; retaining worker resources");
        method.visitMethodInsn(INVOKESTATIC, "org/webrtc/Logging", "e", "(Ljava/lang/String;Ljava/lang/String;)V", false);
        Label start = new Label();
        Label end = new Label();
        Label failed = new Label();
        Label done = new Label();
        method.visitTryCatchBlock(start, end, failed, "java/lang/IllegalStateException");
        method.visitLabel(start);
        method.visitVarInsn(ALOAD, 0);
        method.visitFieldInsn(GETFIELD, CLASS_NAME, "audioTrack", "Landroid/media/AudioTrack;");
        method.visitMethodInsn(INVOKEVIRTUAL, "android/media/AudioTrack", "stop", "()V", false);
        method.visitVarInsn(ALOAD, 0);
        method.visitInsn(ICONST_1);
        method.visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "doAudioTrackStateCallback", "(I)V", false);
        method.visitLabel(end);
        method.visitJumpInsn(GOTO, done);
        method.visitLabel(failed);
        method.visitVarInsn(ASTORE, 1);
        method.visitLdcInsn("WebRtcAudioTrackExternal");
        method.visitLdcInsn("AudioTrack stop failed after playback join timeout");
        method.visitVarInsn(ALOAD, 1);
        method.visitMethodInsn(INVOKESTATIC, "org/webrtc/Logging", "e", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/Throwable;)V", false);
        method.visitLabel(done);
        loadWorker(method);
        method.visitMethodInsn(INVOKEVIRTUAL, "java/lang/Thread", "isAlive", "()Z", false);
        Label alive = new Label();
        method.visitJumpInsn(IFNE, alive);
        method.visitVarInsn(ALOAD, 0);
        method.visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowReleaseAudioResources", "()V", false);
        method.visitLabel(alive);
        method.visitInsn(RETURN);
        method.visitMaxs(0, 0);
        method.visitEnd();
    }

    private void emitFinished() {
        MethodVisitor method = super.visitMethod(ACC_SYNCHRONIZED, "opennowPlaybackFinished", "(Ljava/lang/Thread;)V", null, null);
        method.visitCode();
        Label done = new Label();
        method.visitVarInsn(ALOAD, 0);
        method.visitFieldInsn(GETFIELD, CLASS_NAME, DEFERRED, "Z");
        method.visitJumpInsn(IFEQ, done);
        loadWorker(method);
        method.visitVarInsn(ALOAD, 1);
        method.visitJumpInsn(IF_ACMPNE, done);
        method.visitVarInsn(ALOAD, 0);
        method.visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowReleaseAudioResources", "()V", false);
        method.visitLabel(done);
        method.visitInsn(RETURN);
        method.visitMaxs(0, 0);
        method.visitEnd();
    }

    private void emitRun() {
        MethodVisitor method = super.visitMethod(ACC_PUBLIC, "run", "()V", null, null);
        method.visitCode();
        Label start = new Label();
        Label end = new Label();
        Label failed = new Label();
        method.visitTryCatchBlock(start, end, failed, null);
        method.visitLabel(start);
        method.visitVarInsn(ALOAD, 0);
        method.visitMethodInsn(INVOKEVIRTUAL, THREAD_NAME, "opennowRun", "()V", false);
        method.visitLabel(end);
        finishWorker(method);
        method.visitInsn(RETURN);
        method.visitLabel(failed);
        method.visitVarInsn(ASTORE, 1);
        finishWorker(method);
        method.visitVarInsn(ALOAD, 1);
        method.visitInsn(ATHROW);
        method.visitMaxs(0, 0);
        method.visitEnd();
    }

    private static void finishWorker(MethodVisitor method) {
        method.visitVarInsn(ALOAD, 0);
        method.visitFieldInsn(GETFIELD, THREAD_NAME, "this$0", "L" + CLASS_NAME + ";");
        method.visitVarInsn(ALOAD, 0);
        method.visitMethodInsn(INVOKEVIRTUAL, CLASS_NAME, "opennowPlaybackFinished", "(Ljava/lang/Thread;)V", false);
    }
}
