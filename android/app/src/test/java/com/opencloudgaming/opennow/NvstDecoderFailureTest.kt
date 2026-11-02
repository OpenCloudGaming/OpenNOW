package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import org.webrtc.VideoCodecStatus

class NvstDecoderFailureTest {
    @Test fun unrecoverableStatusUsesLocalDecoderFailureInsteadOfReopeningRtsp() {
        for (status in listOf(VideoCodecStatus.FALLBACK_SOFTWARE, VideoCodecStatus.UNINITIALIZED,
            VideoCodecStatus.ERR_PARAMETER, VideoCodecStatus.TIMEOUT)) {
            val failure = assertThrows(NvstDecoderFailure::class.java) {
                nvstDecoderOperation("decode") { nvstDecodeAccepted(status) }
            }
            val detail = nvstFailureDetail(failure)
            assertTrue(detail.contains(status.toString()))
            assertTrue(isNvstDecoderFailure(detail))
            assertFalse(nvstAttachmentRequiresResume(detail, retriesExhausted = true))
        }
        assertTrue(nvstDecodeAccepted(VideoCodecStatus.OK))
        assertFalse(nvstDecodeAccepted(VideoCodecStatus.NO_OUTPUT))
        assertFalse(nvstDecodeAccepted(VideoCodecStatus.ERROR))
    }
    @Test fun rejectedDecoderInitializationRetainsCauseAndCannotTriggerCloudResume() {
        val cause = IllegalStateException("NVST decoder initialization failed: FALLBACK_SOFTWARE")
        val failure = assertThrows(NvstDecoderFailure::class.java) {
            nvstDecoderOperation("initialization") { throw cause }
        }
        assertSame(cause, failure.cause)
        val detail = nvstFailureDetail(failure)
        assertTrue(detail.contains("initialization"))
        assertTrue(detail.contains("FALLBACK_SOFTWARE"))
        assertTrue(isNvstDecoderFailure(detail))
        assertFalse(nvstAttachmentRequiresResume(detail))
        assertFalse(nvstAttachmentRequiresResume(detail, retriesExhausted = true))
    }

    @Test fun decoderRuntimeFailureUsesSameLocalFailureRoute() {
        val failure = assertThrows(NvstDecoderFailure::class.java) {
            nvstDecoderOperation("decode") { throw IllegalStateException("decoder thread lost") }
        }
        assertTrue(isNvstDecoderFailure(nvstFailureDetail(failure)))
        assertTrue(failure.message!!.contains("decode"))
        assertSame(failure, assertThrows(NvstDecoderFailure::class.java) {
            nvstDecoderOperation("decode") { throw failure }
        })
    }

    @Test fun healthyDecoderResultIsPassedThrough() {
        val marker = Any()
        assertSame(marker, nvstDecoderOperation("decode") { marker })
    }

    @Test fun transportFailuresRemainEligibleForExistingRecovery() {
        val detail = "nvst-attachment-rejected: OPTIONS failed: 400 OK"
        assertFalse(isNvstDecoderFailure(detail))
        assertTrue(nvstAttachmentRequiresResume(detail))
        assertFalse(isNvstDecoderFailure(nvstFailureDetail(IllegalStateException("transport failed"))))
    }
}
