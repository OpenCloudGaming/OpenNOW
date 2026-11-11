#include <jni.h>
#include <android/api-level.h>
#include <dlfcn.h>
#include <media/NdkMediaCodec.h>
#include <media/NdkMediaFormat.h>
#include <sstream>
#include <string>

// The metadata API is available from API 36. Resolve it dynamically so the same library
// still loads on API 24, including when built with an older NDK's headers/stubs.
struct AMediaCodecInfo;
using FindDecoderForFormat = media_status_t (*)(const AMediaFormat *, const AMediaCodecInfo **);

static jboolean decoderListed(JNIEnv *env, const char *mimeType) {
    static const auto findDecoder = reinterpret_cast<FindDecoderForFormat>(
        dlsym(RTLD_DEFAULT, "AMediaCodecStore_findNextDecoderForFormat"));
    if (findDecoder == nullptr) {
        env->ThrowNew(env->FindClass("java/lang/IllegalStateException"), "NDK codec metadata unavailable");
        return JNI_FALSE;
    }
    AMediaFormat *format = AMediaFormat_new();
    if (format == nullptr) {
        env->ThrowNew(env->FindClass("java/lang/IllegalStateException"), "NDK codec format unavailable");
        return JNI_FALSE;
    }
    AMediaFormat_setString(format, AMEDIAFORMAT_KEY_MIME, mimeType);
    const AMediaCodecInfo *info = nullptr;
    const media_status_t status = findDecoder(format, &info);
    AMediaFormat_delete(format);
    if (status != AMEDIA_OK && status != AMEDIA_ERROR_UNSUPPORTED) {
        env->ThrowNew(env->FindClass("java/lang/IllegalStateException"), "NDK codec metadata query failed");
        return JNI_FALSE;
    }
    return status == AMEDIA_OK && info != nullptr ? JNI_TRUE : JNI_FALSE;
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_opencloudgaming_opennow_NativeCodecProbe_nativeRuntimeSummary(JNIEnv *env, jobject) {
    std::ostringstream out;
    out << "{";
    out << "\"nativeLibrary\":\"opennow_native\",";
    out << "\"mediaNdk\":true,";
    out << "\"decoderProbe\":\"" << (android_get_device_api_level() >= 36 ? "metadata" : "allocation") << "\",";
    out << "\"rtpPacketSize\":1140,";
    out << "\"inputProtocolVersion\":3";
    out << "}";
    const std::string value = out.str();
    return env->NewStringUTF(value.c_str());
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeCodecProbe_nativeDecoderAvailable(JNIEnv *env, jobject, jstring mimeType) {
    if (mimeType == nullptr) {
        return JNI_FALSE;
    }

    const char *rawMimeType = env->GetStringUTFChars(mimeType, nullptr);
    if (rawMimeType == nullptr) {
        return JNI_FALSE;
    }

    if (android_get_device_api_level() >= 36) {
        // Diagnostics must not allocate a real decoder on Android 16+. Allocation/release
        // drives MediaCodec::onMessageReceived even before a stream starts, and native
        // framework/vendor aborts cannot be caught by the Kotlin runCatching caller.
        // On a failed metadata query report unknown through JNI; never retry allocation.
        const jboolean available = decoderListed(env, rawMimeType);
        env->ReleaseStringUTFChars(mimeType, rawMimeType);
        return available;
    }
    AMediaCodec *codec = AMediaCodec_createDecoderByType(rawMimeType);
    env->ReleaseStringUTFChars(mimeType, rawMimeType);
    if (codec == nullptr) {
        return JNI_FALSE;
    }

    AMediaCodec_delete(codec);
    return JNI_TRUE;
}
