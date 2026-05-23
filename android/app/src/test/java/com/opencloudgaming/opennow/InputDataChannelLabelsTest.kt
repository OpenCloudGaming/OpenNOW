package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class InputDataChannelLabelsTest {
    @Test
    fun classifiesOnlyInputChannelsAsInputTransport() {
        assertEquals(InputDataChannelRole.Reliable, InputDataChannelLabels.classify("input_channel_v1"))
        assertEquals(InputDataChannelRole.PartiallyReliable, InputDataChannelLabels.classify("input_channel_partially_reliable"))
        assertEquals(InputDataChannelRole.Other, InputDataChannelLabels.classify("control_channel"))
        assertEquals(InputDataChannelRole.Other, InputDataChannelLabels.classify("remote_trace_channel"))
    }
}
