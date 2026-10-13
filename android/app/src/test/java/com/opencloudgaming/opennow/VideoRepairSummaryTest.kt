package com.opencloudgaming.opennow
import org.junit.Assert.*
import org.junit.Test
class VideoRepairSummaryTest {
    private val sdp = """
        m=video 9 UDP/TLS/RTP/SAVPF 100 101 102
        a=rtpmap:100 H265/90000
        a=rtpmap:101 rtx/90000
        a=fmtp:101 apt=100
        a=rtpmap:102 flexfec-03/90000
        a=rtcp-fb:* nack
        a=rtcp-fb:100 nack pli
        a=ssrc-group:FEC-FR 123 456
        a=ice-pwd:private-secret
        c=IN IP4 private-address
    """.trimIndent()
    @Test fun reportsMatchingRepairWithoutSensitiveData() {
        val result = SdpTools.videoRepairSummary(sdp)
        assertEquals("active=true nack=true pli=true rtx=true flexfec=true red=false ulpfec=false fecGroup=true", result)
        assertFalse(result.contains("123"))
        assertFalse(result.contains("private"))
        assertEquals(result, SdpTools.videoRepairSummary(sdp.replace("\n", "\r\n")))
    }
    @Test fun rejectedMediaAndAudioDoNotReportRepairs() {
        val expected = "active=false nack=false pli=false rtx=false flexfec=false red=false ulpfec=false fecGroup=false"
        assertEquals(expected, SdpTools.videoRepairSummary(sdp.replace("m=video 9", "m=video 0")))
        assertEquals(expected, SdpTools.videoRepairSummary(sdp.replace("m=video", "m=audio")))
    }
    @Test fun mismatchedRtxAndPliAreNotGenericNack() {
        val result = SdpTools.videoRepairSummary(sdp.replace("apt=100", "apt=999").replace("a=rtcp-fb:* nack\n", ""))
        assertTrue(result.contains("nack=false pli=true rtx=false"))
    }
}
