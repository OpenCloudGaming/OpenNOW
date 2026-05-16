package com.opencloudgaming.opennow

private val QR_DATA_CODEWORDS_LOW = intArrayOf(0, 19, 34, 55, 80, 108, 136, 156, 194, 232)
private val QR_ECC_CODEWORDS_LOW = intArrayOf(0, 7, 10, 15, 20, 26, 18, 20, 24, 30)
private val QR_BLOCKS_LOW = intArrayOf(0, 1, 1, 1, 1, 1, 2, 2, 2, 2)
private val QR_ALIGN_POSITIONS = arrayOf(
    intArrayOf(),
    intArrayOf(),
    intArrayOf(6, 18),
    intArrayOf(6, 22),
    intArrayOf(6, 26),
    intArrayOf(6, 30),
    intArrayOf(6, 34),
    intArrayOf(6, 22, 38),
    intArrayOf(6, 24, 42),
    intArrayOf(6, 26, 46),
)

data class QrCode(
    val size: Int,
    private val modules: BooleanArray,
) {
    fun isDark(x: Int, y: Int): Boolean =
        x in 0 until size && y in 0 until size && modules[y * size + x]

    companion object {
        fun encodeText(text: String): QrCode? {
            val bytes = text.encodeToByteArray()
            val version = (1 until QR_DATA_CODEWORDS_LOW.size).firstOrNull { version ->
                val capacityBits = QR_DATA_CODEWORDS_LOW[version] * 8
                4 + 8 + bytes.size * 8 <= capacityBits
            } ?: return null
            return encodeBytes(bytes, version)
        }

        private fun encodeBytes(bytes: ByteArray, version: Int): QrCode {
            val dataCodewords = QR_DATA_CODEWORDS_LOW[version]
            val bits = ArrayList<Boolean>(dataCodewords * 8)
            appendBits(bits, 0x4, 4)
            appendBits(bits, bytes.size, 8)
            bytes.forEach { appendBits(bits, it.toInt() and 0xff, 8) }
            repeat(minOf(4, dataCodewords * 8 - bits.size)) { bits += false }
            while (bits.size % 8 != 0) bits += false
            val data = ArrayList<Int>(dataCodewords)
            for (i in bits.indices step 8) {
                var value = 0
                for (j in 0 until 8) value = (value shl 1) or if (bits[i + j]) 1 else 0
                data += value
            }
            var pad = 0xec
            while (data.size < dataCodewords) {
                data += pad
                pad = pad xor 0xfd
            }
            val allCodewords = addErrorCorrection(data, version)
            val builder = QrBuilder(version)
            builder.drawFunctionPatterns()
            builder.drawFormatBits(mask = 0)
            builder.drawCodewords(allCodewords)
            builder.drawFormatBits(mask = 0)
            return QrCode(builder.size, builder.modules)
        }

        private fun appendBits(bits: MutableList<Boolean>, value: Int, count: Int) {
            for (i in count - 1 downTo 0) bits += ((value ushr i) and 1) != 0
        }

        private fun addErrorCorrection(data: List<Int>, version: Int): List<Int> {
            val blockCount = QR_BLOCKS_LOW[version]
            val eccLen = QR_ECC_CODEWORDS_LOW[version]
            val generator = reedSolomonGenerator(eccLen)
            val shortBlockLen = data.size / blockCount
            val blocks = (0 until blockCount).map { blockIndex ->
                val start = blockIndex * shortBlockLen
                data.subList(start, start + shortBlockLen)
            }
            val eccBlocks = blocks.map { reedSolomonRemainder(it, generator) }
            val output = ArrayList<Int>(data.size + blockCount * eccLen)
            for (i in 0 until shortBlockLen) {
                blocks.forEach { output += it[i] }
            }
            for (i in 0 until eccLen) {
                eccBlocks.forEach { output += it[i] }
            }
            return output
        }

        private fun reedSolomonGenerator(degree: Int): IntArray {
            val result = IntArray(degree)
            result[degree - 1] = 1
            var root = 1
            repeat(degree) {
                for (i in result.indices) {
                    result[i] = gfMultiply(result[i], root)
                    if (i + 1 < result.size) {
                        result[i] = result[i] xor result[i + 1]
                    }
                }
                root = gfMultiply(root, 2)
            }
            return result
        }

        private fun reedSolomonRemainder(data: List<Int>, generator: IntArray): IntArray {
            val result = IntArray(generator.size)
            data.forEach { value ->
                val factor = value xor result[0]
                for (i in 0 until result.lastIndex) result[i] = result[i + 1]
                result[result.lastIndex] = 0
                for (i in generator.indices) result[i] = result[i] xor gfMultiply(generator[i], factor)
            }
            return result
        }

        private fun gfMultiply(x: Int, y: Int): Int {
            var a = x
            var b = y
            var result = 0
            while (b != 0) {
                if ((b and 1) != 0) result = result xor a
                a = a shl 1
                if ((a and 0x100) != 0) a = a xor 0x11d
                b = b ushr 1
            }
            return result
        }
    }
}

private class QrBuilder(private val version: Int) {
    val size = version * 4 + 17
    val modules = BooleanArray(size * size)
    private val functionModules = BooleanArray(size * size)

    fun drawFunctionPatterns() {
        drawFinder(3, 3)
        drawFinder(size - 4, 3)
        drawFinder(3, size - 4)
        for (i in 8 until size - 8) {
            setFunction(6, i, i % 2 == 0)
            setFunction(i, 6, i % 2 == 0)
        }
        val align = QR_ALIGN_POSITIONS[version]
        for (x in align) {
            for (y in align) {
                val overlapsFinder = (x == 6 && y == 6) || (x == 6 && y == size - 7) || (x == size - 7 && y == 6)
                if (!overlapsFinder) drawAlignment(x, y)
            }
        }
        setFunction(8, size - 8, true)
    }

    fun drawCodewords(codewords: List<Int>) {
        var bitIndex = 0
        var upward = true
        var right = size - 1
        while (right >= 1) {
            if (right == 6) right--
            for (vertical in 0 until size) {
                val y = if (upward) size - 1 - vertical else vertical
                for (dx in 0..1) {
                    val x = right - dx
                    if (functionModules[index(x, y)]) continue
                    val bit = if (bitIndex < codewords.size * 8) {
                        ((codewords[bitIndex / 8] ushr (7 - bitIndex % 8)) and 1) != 0
                    } else {
                        false
                    }
                    val masked = bit xor ((x + y) % 2 == 0)
                    modules[index(x, y)] = masked
                    bitIndex++
                }
            }
            upward = !upward
            right -= 2
        }
    }

    fun drawFormatBits(mask: Int) {
        val bits = formatBits(mask)
        for (i in 0..5) setFunction(8, i, bit(bits, i))
        setFunction(8, 7, bit(bits, 6))
        setFunction(8, 8, bit(bits, 7))
        setFunction(7, 8, bit(bits, 8))
        for (i in 9..14) setFunction(14 - i, 8, bit(bits, i))
        for (i in 0..7) setFunction(size - 1 - i, 8, bit(bits, i))
        for (i in 8..14) setFunction(8, size - 15 + i, bit(bits, i))
        setFunction(8, size - 8, true)
    }

    private fun drawFinder(cx: Int, cy: Int) {
        for (dy in -4..4) {
            for (dx in -4..4) {
                val x = cx + dx
                val y = cy + dy
                if (x !in 0 until size || y !in 0 until size) continue
                val dist = maxOf(kotlin.math.abs(dx), kotlin.math.abs(dy))
                setFunction(x, y, dist != 2 && dist != 4)
            }
        }
    }

    private fun drawAlignment(cx: Int, cy: Int) {
        for (dy in -2..2) {
            for (dx in -2..2) {
                setFunction(cx + dx, cy + dy, maxOf(kotlin.math.abs(dx), kotlin.math.abs(dy)) != 1)
            }
        }
    }

    private fun setFunction(x: Int, y: Int, dark: Boolean) {
        modules[index(x, y)] = dark
        functionModules[index(x, y)] = true
    }

    private fun index(x: Int, y: Int): Int = y * size + x

    private fun formatBits(mask: Int): Int {
        var data = (1 shl 3) or mask
        var rem = data
        repeat(10) {
            rem = (rem shl 1) xor if ((rem and (1 shl 9)) != 0) 0x537 else 0
        }
        return ((data shl 10) or rem) xor 0x5412
    }

    private fun bit(value: Int, index: Int): Boolean = ((value ushr index) and 1) != 0
}
