// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.zip.GZIPInputStream
import java.util.zip.Inflater
import java.util.zip.InflaterInputStream
import kotlin.math.abs
import kotlin.math.max

internal data class StreamRemoteCursorState(
    val absolute: Boolean = true,
    val visible: Boolean = true,
    val normalizedX: Float = 0.5f,
    val normalizedY: Float = 0.5f,
    val name: String = "default",
    val bitmap: Bitmap? = null,
    val hotSpotX: Float = 0f,
    val hotSpotY: Float = 0f,
)

internal data class DecodedBoosteroidCursor(
    val bitmap: Bitmap,
    val hotSpotX: Float,
    val hotSpotY: Float,
)

private const val BOOSTEROID_CURSOR_BASE64_LIMIT = 3_000_000
private const val BOOSTEROID_CURSOR_DECODED_LIMIT = 8 * 1024 * 1024
private const val BOOSTEROID_CURSOR_MAX_DIMENSION = 512

internal fun decodeBoosteroidCursorResource(
    resource: String,
    zipped: Boolean,
    targetSizePx: Int,
): DecodedBoosteroidCursor? {
    val encoded = resource.trim()
    if (encoded.isEmpty() || encoded.length > BOOSTEROID_CURSOR_BASE64_LIMIT) return null
    val packed = runCatching { Base64.decode(encoded, Base64.DEFAULT) }.getOrNull() ?: return null
    val bytes = if (zipped) inflateBoosteroidCursor(packed) ?: return null else packed
    if (bytes.isEmpty() || bytes.size > BOOSTEROID_CURSOR_DECODED_LIMIT) return null
    return decodeBoosteroidCur(bytes, targetSizePx)
}

private fun inflateBoosteroidCursor(bytes: ByteArray): ByteArray? {
    val attempts = listOf<() -> ByteArray?>(
        { inflateBoosteroidCursor(bytes, nowrap = false) },
        { inflateBoosteroidCursor(bytes, nowrap = true) },
        {
            runCatching {
                GZIPInputStream(ByteArrayInputStream(bytes)).use(::readBoosteroidCursorBytes)
            }.getOrNull()
        },
    )
    return attempts.firstNotNullOfOrNull { attempt -> attempt() }
}

private fun inflateBoosteroidCursor(bytes: ByteArray, nowrap: Boolean): ByteArray? {
    val inflater = Inflater(nowrap)
    return try {
        runCatching {
            InflaterInputStream(ByteArrayInputStream(bytes), inflater).use(::readBoosteroidCursorBytes)
        }.getOrNull()
    } finally {
        inflater.end()
    }
}

private fun readBoosteroidCursorBytes(input: java.io.InputStream): ByteArray? {
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(16 * 1024)
    while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        if (count == 0) continue
        if (output.size() + count > BOOSTEROID_CURSOR_DECODED_LIMIT) return null
        output.write(buffer, 0, count)
    }
    return output.toByteArray()
}

private fun decodeBoosteroidCur(bytes: ByteArray, targetSizePx: Int): DecodedBoosteroidCursor? {
    if (bytes.size < 6 || bytes.leUShort(0) != 0 || bytes.leUShort(2) != 2) {
        return decodeBoosteroidCursorBitmap(bytes, 0, bytes.size)?.let { DecodedBoosteroidCursor(it, 0f, 0f) }
    }
    val count = bytes.leUShort(4).coerceAtMost(64)
    if (count <= 0 || 6 + count * 16 > bytes.size) return null
    data class Entry(
        val width: Int,
        val height: Int,
        val hotSpotX: Int,
        val hotSpotY: Int,
        val length: Int,
        val offset: Int,
    )
    val entries = buildList {
        repeat(count) { index ->
            val start = 6 + index * 16
            val width = (bytes[start].toInt() and 0xff).takeIf { it > 0 } ?: 256
            val height = (bytes[start + 1].toInt() and 0xff).takeIf { it > 0 } ?: 256
            val length = bytes.leUInt(start + 8)?.takeIf { it in 1L..Int.MAX_VALUE.toLong() }?.toInt() ?: return@repeat
            val offset = bytes.leUInt(start + 12)?.takeIf { it <= Int.MAX_VALUE.toLong() }?.toInt() ?: return@repeat
            if (offset < 0 || length <= 0 || offset > bytes.size - length) return@repeat
            add(Entry(width, height, bytes.leUShort(start + 4), bytes.leUShort(start + 6), length, offset))
        }
    }.sortedWith(
        compareBy<Entry> { kotlin.math.abs(max(it.width, it.height) - targetSizePx.coerceAtLeast(1)) }
            .thenByDescending { it.width * it.height },
    )
    for (entry in entries) {
        val bitmap = decodeBoosteroidCursorBitmap(bytes, entry.offset, entry.length)
            ?: decodeBoosteroidDibCursor(bytes, entry.offset, entry.length)
            ?: continue
        return DecodedBoosteroidCursor(
            bitmap = bitmap,
            hotSpotX = entry.hotSpotX.coerceIn(0, (bitmap.width - 1).coerceAtLeast(0)).toFloat(),
            hotSpotY = entry.hotSpotY.coerceIn(0, (bitmap.height - 1).coerceAtLeast(0)).toFloat(),
        )
    }
    return null
}

private fun decodeBoosteroidCursorBitmap(bytes: ByteArray, offset: Int, length: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, offset, length, bounds)
    if (
        bounds.outWidth !in 1..BOOSTEROID_CURSOR_MAX_DIMENSION ||
        bounds.outHeight !in 1..BOOSTEROID_CURSOR_MAX_DIMENSION
    ) {
        return null
    }
    return BitmapFactory.decodeByteArray(bytes, offset, length)
}

private fun decodeBoosteroidDibCursor(bytes: ByteArray, offset: Int, length: Int): Bitmap? {
    if (length < 40 || offset < 0 || offset > bytes.size - length) return null
    val headerSize = bytes.leUInt(offset)?.takeIf { it in 40L..length.toLong() }?.toInt() ?: return null
    val width = bytes.leInt(offset + 4)?.let { kotlin.math.abs(it) } ?: return null
    val storedHeight = bytes.leInt(offset + 8) ?: return null
    val height = (kotlin.math.abs(storedHeight.toLong()) / 2L).toInt()
    if (width !in 1..BOOSTEROID_CURSOR_MAX_DIMENSION || height !in 1..BOOSTEROID_CURSOR_MAX_DIMENSION) return null
    val bitsPerPixel = bytes.leUShort(offset + 14)
    val compression = bytes.leUInt(offset + 16)?.toInt() ?: return null
    if (bitsPerPixel !in setOf(24, 32) || compression !in setOf(0, 3)) return null
    val bitfieldBytes = if (compression == 3 && headerSize == 40) 12 else 0
    val pixelOffset = offset + headerSize + bitfieldBytes
    val rowStride = (((width.toLong() * bitsPerPixel) + 31L) / 32L * 4L).toInt()
    val pixelBytes = rowStride.toLong() * height
    if (pixelOffset < offset || pixelBytes > length.toLong() || pixelOffset.toLong() + pixelBytes > offset.toLong() + length) return null
    val pixels = IntArray(width * height)
    var anyAlpha = bitsPerPixel == 24
    for (y in 0 until height) {
        val sourceRow = if (storedHeight > 0) height - 1 - y else y
        val rowStart = pixelOffset + sourceRow * rowStride
        for (x in 0 until width) {
            val source = rowStart + x * (bitsPerPixel / 8)
            val blue = bytes[source].toInt() and 0xff
            val green = bytes[source + 1].toInt() and 0xff
            val red = bytes[source + 2].toInt() and 0xff
            val alpha = if (bitsPerPixel == 32) bytes[source + 3].toInt() and 0xff else 0xff
            if (alpha != 0) anyAlpha = true
            pixels[y * width + x] = (alpha shl 24) or (red shl 16) or (green shl 8) or blue
        }
    }
    val maskOffset = pixelOffset + pixelBytes.toInt()
    val maskStride = ((width + 31) / 32) * 4
    val maskAvailable = maskOffset >= offset && maskOffset.toLong() + maskStride.toLong() * height <= offset.toLong() + length
    for (y in 0 until height) {
        val maskRow = if (storedHeight > 0) height - 1 - y else y
        for (x in 0 until width) {
            val index = y * width + x
            val transparent = maskAvailable &&
                ((bytes[maskOffset + maskRow * maskStride + x / 8].toInt() ushr (7 - x % 8)) and 1) == 1
            pixels[index] = when {
                transparent -> pixels[index] and 0x00ffffff
                !anyAlpha -> pixels[index] or -0x1000000
                else -> pixels[index]
            }
        }
    }
    return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
}

private fun ByteArray.leUShort(offset: Int): Int {
    if (offset < 0 || offset + 2 > size) return -1
    return (this[offset].toInt() and 0xff) or ((this[offset + 1].toInt() and 0xff) shl 8)
}

private fun ByteArray.leUInt(offset: Int): Long? {
    if (offset < 0 || offset + 4 > size) return null
    return (this[offset].toLong() and 0xffL) or
        ((this[offset + 1].toLong() and 0xffL) shl 8) or
        ((this[offset + 2].toLong() and 0xffL) shl 16) or
        ((this[offset + 3].toLong() and 0xffL) shl 24)
}

private fun ByteArray.leInt(offset: Int): Int? = leUInt(offset)?.toInt()
