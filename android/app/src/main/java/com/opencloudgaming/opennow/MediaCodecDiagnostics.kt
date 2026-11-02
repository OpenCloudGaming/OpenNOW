package com.opencloudgaming.opennow

import android.media.MediaCodec

/** Retain platform failure details before decoder adapters reduce errors to a status code. */
internal fun mediaCodecFailureDetail(error: Throwable): String {
    val platform = if (error is MediaCodec.CodecException) {
        "diagnostic=${error.diagnosticInfo} code=${error.errorCode} " +
            "recoverable=${error.isRecoverable} transient=${error.isTransient} "
    } else ""
    return sanitizeDiagnosticLogPayload(platform + boundedJavaCrashTrace(error), limit = 2400)
}
