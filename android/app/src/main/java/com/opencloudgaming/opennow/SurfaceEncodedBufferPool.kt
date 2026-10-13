package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.util.ArrayDeque

/** WebRTC's Java EncodedImage may borrow native memory valid only during decode(). */
internal class SurfaceEncodedBufferPool {
    private val available = ArrayDeque<ByteBuffer>()
    private var availableBytes = 0

    fun copy(source: ByteBuffer): ByteBuffer {
        val iterator = available.iterator()
        var buffer: ByteBuffer? = null
        while (iterator.hasNext()) {
            val candidate = iterator.next()
            if (candidate.capacity() >= source.remaining()) {
                iterator.remove()
                availableBytes -= candidate.capacity()
                buffer = candidate
                break
            }
        }
        return (buffer ?: ByteBuffer.allocate(source.remaining())).apply {
            clear()
            put(source.duplicate())
            flip()
        }
    }

    fun recycle(buffer: ByteBuffer) {
        if (available.size < 8 && availableBytes + buffer.capacity() <= 4 * 1024 * 1024) {
            available.addLast(buffer)
            availableBytes += buffer.capacity()
        }
    }

    fun clear() { available.clear(); availableBytes = 0 }
}
