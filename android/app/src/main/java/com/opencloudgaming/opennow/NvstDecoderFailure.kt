package com.opencloudgaming.opennow

/** A local decoder failure cannot be repaired by reconnecting or reclaiming a cloud seat. */
internal class NvstDecoderFailure(phase: String, cause: RuntimeException) : RuntimeException(
    "Video decoder $phase failed: ${cause.javaClass.simpleName}: ${cause.message.orEmpty().take(512)}",
    cause,
)

internal inline fun <T> nvstDecoderOperation(phase: String, operation: () -> T): T = try {
    operation()
} catch (error: NvstDecoderFailure) {
    throw error
} catch (error: RuntimeException) {
    throw NvstDecoderFailure(phase, error)
}

internal fun nvstFailureDetail(error: Exception): String = when (error) {
    is NvstDecoderFailure -> "nvst-decoder-failed: ${error.message}"
    else -> "NVST failed: ${error.javaClass.simpleName}: ${error.message.orEmpty().take(512)}"
}

internal fun isNvstDecoderFailure(detail: String): Boolean = detail.startsWith("nvst-decoder-failed:")
