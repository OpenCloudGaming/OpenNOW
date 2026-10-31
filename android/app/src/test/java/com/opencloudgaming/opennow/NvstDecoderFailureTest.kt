package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class NvstDecoderFailureTest {
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
