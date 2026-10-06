package com.opencloudgaming.opennow

import android.hardware.usb.*
import android.os.SystemClock
import java.nio.ByteBuffer

/** The output thread is the sole owner of this connection and its asynchronous requests. */
@android.annotation.SuppressLint("NewApi") // Manager refuses initialization below API 26.
internal class KishiSensaUsb(private val connection: UsbDeviceConnection, private val iface: UsbInterface) : AutoCloseable {
    private val rx = UsbRequest()
    private val tx = UsbRequest()
    private var claimed = false
    private var savedMode: Int? = null
    private var changedMode = false

    fun initialize(cancelled: () -> Boolean) {
        check(!cancelled()) { "Initialization cancelled" }
        // Only this validated dedicated haptics interface may be detached from a kernel driver.
        claimed = connection.claimInterface(iface, false) || connection.claimInterface(iface, true)
        check(claimed) { "Haptics interface unavailable" }
        val endpoints = (0 until iface.endpointCount).map(iface::getEndpoint)
        check(rx.initialize(connection, endpoints.single { it.address == 0x84 }))
        check(tx.initialize(connection, endpoints.single { it.address == 4 }))
        val size = exchange(0x90, byteArrayOf(0, 0), cancelled)
        check(size.size == 2) { "Metadata length response invalid" }
        val length = ((size[0].toInt() and 255) shl 8) + (size[1].toInt() and 255)
        check(length in 1..KishiSensaProtocol.MAX_METADATA) { "Metadata too large or empty" }
        val bytes = ByteArray(length)
        var offset = 0
        while (offset < length) {
            val count = minOf(50, length - offset)
            val query = byteArrayOf((offset ushr 8).toByte(), offset.toByte(), count.toByte(), 0, 0)
            val reply = exchange(0x91, query, cancelled)
            check(reply.size == count + 3 && reply.take(3) == query.take(3)) { "Metadata offset/count mismatch" }
            reply.copyInto(bytes, destinationOffset = offset, startIndex = 3)
            offset += count
        }
        KishiSensaProtocol.validateMetadata(bytes)
        val mode = exchange(0x87, byteArrayOf(0), cancelled)
        check(mode.size == 1 && mode[0].toInt() in setOf(0, 2)) { "Unsupported Sensa mode" }
        savedMode = mode[0].toInt()
        if (savedMode != 0) {
            changedMode = true // Mode may change even when its ACK is lost.
            check(exchange(7, byteArrayOf(0), cancelled).contentEquals(byteArrayOf(0))) { "Mode change rejected" }
        }
        write(KishiSensaRumble().frame(0, 0, 0), cancelled)
        NativeInputDiagnostics.addRetained("kishi.metadata", "Kishi Sensa metadata validated bytes=$length bodies=216,116 savedMode=$savedMode")
    }

    fun write(report: ByteArray, cancelled: () -> Boolean) {
        val reply = transfer(report, cancelled)
        val body = report.copyOfRange(5, (report[1].toInt() and 255) + 1)
        check(reply.contentEquals(body)) { "Haptic frame ACK mismatch" }
    }

    private fun exchange(command: Int, body: ByteArray, cancelled: () -> Boolean): ByteArray =
        transfer(KishiSensaProtocol.report(command, body), cancelled)

    private fun transfer(report: ByteArray, cancelled: () -> Boolean): ByteArray {
        check(!cancelled()) { "Output cancelled" }
        val read = ByteBuffer.allocateDirect(64)
        val write = ByteBuffer.allocateDirect(64).apply { put(report); flip() }
        check(rx.queue(read)) { "Cannot queue USB input" }
        check(tx.queue(write)) { "Cannot queue USB output" }
        val deadline = SystemClock.elapsedRealtime() + 150
        var sent = false
        var response: ByteArray? = null
        while (!sent || response == null) {
            check(!cancelled()) { "Output cancelled" }
            val remaining = deadline - SystemClock.elapsedRealtime()
            check(remaining > 0) { "USB ACK timed out" }
            when (connection.requestWait(remaining)) {
                tx -> { check(write.position() == 64) { "Short USB write" }; sent = true }
                rx -> {
                    val result = ByteArray(read.position())
                    read.flip(); read.get(result)
                    response = KishiSensaProtocol.reply(report, result)
                    if (response == null) { read.clear(); check(rx.queue(read)) }
                }
                else -> error("Unexpected USB completion")
            }
        }
        return checkNotNull(response)
    }

    override fun close() {
        // Request cancellation is bounded by the prior requestWait deadline. Never reuse failed requests.
        runCatching { rx.cancel() }; runCatching { tx.cancel() }
        runCatching { rx.close() }; runCatching { tx.close() }
        try {
            if (claimed && savedMode != null) {
                val out = (0 until iface.endpointCount).map(iface::getEndpoint).single { it.address == 4 }
                val stop = KishiSensaRumble().frame(0, 0, 0)
                val silenced = connection.bulkTransfer(out, stop, stop.size, 150) == stop.size
                val restored = if (changedMode) {
                    val restore = KishiSensaProtocol.report(7, byteArrayOf(checkNotNull(savedMode).toByte()))
                    connection.bulkTransfer(out, restore, restore.size, 150) == restore.size
                } else true
                NativeInputDiagnostics.addRetained("kishi.cleanup", "Kishi cleanup silenceWritten=$silenced modeRestoreWritten=$restored (best effort)")
            }
        } finally {
            try { if (claimed) connection.releaseInterface(iface) } finally { connection.close() }
        }
    }
}
