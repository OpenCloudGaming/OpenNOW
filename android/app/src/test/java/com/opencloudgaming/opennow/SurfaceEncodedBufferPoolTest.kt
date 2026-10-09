package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer

class SurfaceEncodedBufferPoolTest {
    @Test fun deferredInputOwnsItsBytesAfterBorrowedNativeStorageChanges() {
        val bytes = byteArrayOf(0, 0, 0, 1, 0x67, 2, 3)
        val original = ByteBuffer.wrap(bytes)
        val copied = SurfaceEncodedBufferPool().copy(original)
        bytes.fill(0)
        assertEquals(0x67, copied.get(4).toInt())
        assertEquals(0, original.position())
    }

    @Test fun copiesOnlyRemainingRangeWithoutChangingSourcePosition() {
        val original = ByteBuffer.wrap(byteArrayOf(9, 1, 2, 3, 9)).apply { position(1); limit(4) }
        val copied = SurfaceEncodedBufferPool().copy(original)
        assertEquals(3, copied.remaining())
        assertEquals(1, copied.get().toInt())
        assertEquals(1, original.position())
        assertEquals(4, original.limit())
    }

    @Test fun reusedBufferHasNoStaleBytesInItsVisibleRange() {
        val pool = SurfaceEncodedBufferPool()
        val first = pool.copy(ByteBuffer.wrap(byteArrayOf(1, 2, 3, 4)))
        pool.recycle(first)
        val second = pool.copy(ByteBuffer.wrap(byteArrayOf(5, 6)))
        assertSame(first, second)
        assertEquals(2, second.remaining())
        assertEquals(5, second.get().toInt())
        assertEquals(6, second.get().toInt())
    }

    @Test fun oversizedBuffersAreNotRetainedByThePool() {
        val pool = SurfaceEncodedBufferPool()
        val large = ByteBuffer.allocate(4 * 1024 * 1024 + 1)
        pool.recycle(large)
        assertNotSame(large, pool.copy(ByteBuffer.wrap(byteArrayOf(1))))
    }
}
