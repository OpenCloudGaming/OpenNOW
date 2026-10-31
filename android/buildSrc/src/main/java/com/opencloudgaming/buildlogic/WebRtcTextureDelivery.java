package com.opencloudgaming.buildlogic;

import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.Label;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

/**
 * Texture delivery adjustments for WebRTC 144.7559.14. The decoder calls both metadata
 * setters on every output. Upstream resets SurfaceTexture's buffer size and posts two
 * handler messages even when neither value changed. Cache requested (not asynchronously
 * applied) values, preserving the first update, transitions and upstream validation.
 * Keep this pinned-API patch separate from codec tuning and the audio teardown guard.
 */
public final class WebRtcTextureDelivery extends ClassVisitor implements Opcodes {
    public static final String TEXTURE_HELPER = "org/webrtc/SurfaceTextureHelper";
    public static final String EGL_THREAD = "org/webrtc/EglThread";
    public static final String DECODER = "org/webrtc/AndroidVideoDecoder";
    private final String owner;
    private int sizeMethods;
    private int rotationMethods;
    private int threadConstructors;
    private int helperInitializations;

    public WebRtcTextureDelivery(ClassVisitor next, String owner) {
        super(ASM9, next);
        this.owner = owner;
    }

    @Override
    public MethodVisitor visitMethod(int access, String name, String desc,
            String signature, String[] exceptions) {
        if (owner.equals(TEXTURE_HELPER) && (access & ACC_STATIC) == 0) {
            if (name.equals("setTextureSize") && desc.equals("(II)V")) {
                sizeMethods++;
            }
            if (name.equals("setFrameRotation") && desc.equals("(I)V")) {
                rotationMethods++;
            }
        }
        MethodVisitor next = super.visitMethod(access, name, desc, signature, exceptions);
        return new MethodVisitor(ASM9, next) {
            @Override
            public void visitFieldInsn(int opcode, String target, String field, String descriptor) {
                super.visitFieldInsn(opcode, target, field, descriptor);
                if (owner.equals(DECODER) && name.equals("initDecode")
                        && desc.equals("(Lorg/webrtc/VideoDecoder$Settings;Lorg/webrtc/VideoDecoder$Callback;)Lorg/webrtc/VideoCodecStatus;")
                        && opcode == PUTFIELD && target.equals(DECODER)
                        && field.equals("surfaceTextureHelper")
                        && descriptor.equals("Lorg/webrtc/SurfaceTextureHelper;")) {
                    helperInitializations++;
                    Label ready = new Label();
                    super.visitVarInsn(ALOAD, 0);
                    super.visitFieldInsn(GETFIELD, DECODER, field, descriptor);
                    super.visitJumpInsn(IFNONNULL, ready);
                    super.visitVarInsn(ALOAD, 0);
                    super.visitInsn(ACONST_NULL);
                    super.visitFieldInsn(PUTFIELD, DECODER, "callback", "Lorg/webrtc/VideoDecoder$Callback;");
                    super.visitFieldInsn(GETSTATIC, "org/webrtc/VideoCodecStatus", "ERROR", "Lorg/webrtc/VideoCodecStatus;");
                    super.visitInsn(ARETURN);
                    super.visitLabel(ready);
                }
            }

            @Override
            public void visitMethodInsn(int opcode, String target, String method, String descriptor,
                    boolean isInterface) {
                if (owner.equals(DECODER) && name.equals("deliverTextureFrame")
                        && opcode == INVOKEVIRTUAL && target.equals(TEXTURE_HELPER)) {
                    if (method.equals("setTextureSize") && descriptor.equals("(II)V")) {
                        sizeMethods++;
                        super.visitMethodInsn(opcode, target, "opennowSetTextureSize", descriptor, isInterface);
                        return;
                    }
                    if (method.equals("setFrameRotation") && descriptor.equals("(I)V")) {
                        rotationMethods++;
                        super.visitMethodInsn(opcode, target, "opennowSetFrameRotation", descriptor, isInterface);
                        return;
                    }
                }
                if (opcode == INVOKESPECIAL && target.equals("android/os/HandlerThread")
                        && method.equals("<init>") && descriptor.equals("(Ljava/lang/String;)V")) {
                    threadConstructors++;
                    if (owner.equals(TEXTURE_HELPER)) {
                        // Only decoder delivery gets display priority. Camera/capture helpers
                        // retain their original priority. Leave the constructor's name on stack.
                        super.visitInsn(DUP);
                        super.visitLdcInsn("decoder-texture-thread");
                        super.visitInsn(SWAP);
                        super.visitMethodInsn(INVOKEVIRTUAL, "java/lang/String", "equals",
                                "(Ljava/lang/Object;)Z", false);
                        Label normal = new Label();
                        Label ready = new Label();
                        super.visitJumpInsn(IFEQ, normal);
                        super.visitIntInsn(BIPUSH, -4); // Process.THREAD_PRIORITY_DISPLAY
                        super.visitJumpInsn(GOTO, ready);
                        super.visitLabel(normal);
                        super.visitInsn(ICONST_0); // HandlerThread's original default priority.
                        super.visitLabel(ready);
                    } else {
                        super.visitIntInsn(BIPUSH, -4); // EGL display worker, below audio priority.
                    }
                    super.visitMethodInsn(opcode, target, method, "(Ljava/lang/String;I)V", isInterface);
                    return;
                }
                super.visitMethodInsn(opcode, target, method, descriptor, isInterface);
            }
        };
    }

    @Override
    public void visitEnd() {
        if (threadConstructors != (owner.equals(DECODER) ? 0 : 1)
                || helperInitializations != (owner.equals(DECODER) ? 1 : 0)
                || ((owner.equals(TEXTURE_HELPER) || owner.equals(DECODER))
                && (sizeMethods != 1 || rotationMethods != 1))) {
            throw new IllegalStateException("WebRTC texture API changed; review texture delivery tuning: "
                    + owner + " size=" + sizeMethods + " rotation=" + rotationMethods
                    + " threadConstructors=" + threadConstructors
                    + " helperInitializations=" + helperInitializations);
        }
        if (owner.equals(TEXTURE_HELPER)) {
            // Only AndroidVideoDecoder uses these entry points. Preserve upstream setters
            // for camera capture and any other users of SurfaceTextureHelper.
            addCachedSetter("opennowSetTextureSize", "setTextureSize", "Size", 2);
            addCachedSetter("opennowSetFrameRotation", "setFrameRotation", "Rotation", 1);
        }
        super.visitEnd();
    }

    private void addCachedSetter(String methodName, String originalName, String prefix, int args) {
        String valid = "opennow" + prefix + "Requested";
        super.visitField(ACC_PRIVATE, valid, "Z", null, null).visitEnd();
        for (int i = 1; i <= args; i++) {
            super.visitField(ACC_PRIVATE, "opennow" + prefix + i, "I", null, null).visitEnd();
        }
        String desc = args == 2 ? "(II)V" : "(I)V";
        // Serialize requests only; never wait for the texture worker or hold its EGL locks.
        // Comparing applied values would miss identical calls while the first update is queued.
        MethodVisitor m = super.visitMethod(ACC_PUBLIC | ACC_SYNCHRONIZED, methodName, desc, null, null);
        m.visitCode();
        Label update = new Label();
        m.visitVarInsn(ALOAD, 0);
        m.visitFieldInsn(GETFIELD, owner, valid, "Z");
        m.visitJumpInsn(IFEQ, update);
        for (int i = 1; i <= args; i++) {
            m.visitVarInsn(ALOAD, 0);
            m.visitFieldInsn(GETFIELD, owner, "opennow" + prefix + i, "I");
            m.visitVarInsn(ILOAD, i);
            m.visitJumpInsn(IF_ICMPNE, update);
        }
        m.visitInsn(RETURN);
        m.visitLabel(update);
        m.visitVarInsn(ALOAD, 0);
        for (int i = 1; i <= args; i++) m.visitVarInsn(ILOAD, i);
        m.visitMethodInsn(INVOKEVIRTUAL, owner, originalName, desc, false);
        // Cache only successful requests. Invalid sizes and platform failures must still throw.
        for (int i = 1; i <= args; i++) {
            m.visitVarInsn(ALOAD, 0);
            m.visitVarInsn(ILOAD, i);
            m.visitFieldInsn(PUTFIELD, owner, "opennow" + prefix + i, "I");
        }
        m.visitVarInsn(ALOAD, 0);
        m.visitInsn(ICONST_1);
        m.visitFieldInsn(PUTFIELD, owner, valid, "Z");
        m.visitInsn(RETURN);
        m.visitMaxs(0, 0);
        m.visitEnd();
    }
}
