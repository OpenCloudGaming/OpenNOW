package com.opencloudgaming.opennow

import android.app.PendingIntent
import android.content.*
import android.hardware.input.InputManager
import android.hardware.usb.*
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.InputDevice
import androidx.core.content.ContextCompat
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.locks.LockSupport

/** App-owned USB output companion. Never reads or claims the gamepad input interface. */
internal class KishiHapticsManager(private val context: Context) {
    private val usb = context.getSystemService(Context.USB_SERVICE) as? UsbManager
    private val inputs = context.getSystemService(Context.INPUT_SERVICE) as InputManager
    private val main = Handler(Looper.getMainLooper())
    private val permissionAction = context.packageName + ".KISHI_HAPTICS_PERMISSION"
    private val permissionPrompt = KishiUsbPermissionPrompt()
    private val callbacks = CopyOnWriteArrayList<() -> Unit>()
    private val mutableStatus = MutableStateFlow("Kishi USB haptics disabled")
    val status = mutableStatus.asStateFlow()
    private var enabled = false
    private var registered = false
    private var streamActive = false
    private var suspended = false
    @Volatile private var strength = 40
    @Volatile private var current: Output? = null

    private data class Target(val strong: Int = 0, val weak: Int = 0, val expires: Long = 0, val test: Boolean = false)
    private class Output(val device: UsbDevice, val iface: UsbInterface) {
        @Volatile var stopping = false
        @Volatile var ready = false
        val target = java.util.concurrent.atomic.AtomicReference(Target())
        lateinit var thread: Thread
    }

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context, intent: Intent) {
            when (intent.action) {
                permissionAction -> {
                    @Suppress("DEPRECATION")
                    val device = intent.getParcelableExtra<UsbDevice>(UsbManager.EXTRA_DEVICE)
                    if (device != null && permissionPrompt.complete(device.deviceName)) {
                        NativeInputDiagnostics.addRetained("kishi.permission",
                            "Kishi USB permission result granted=${usb?.hasPermission(device) == true}")
                        scan(false)
                    }
                }
                UsbManager.ACTION_USB_DEVICE_ATTACHED -> scan(false, autoPermission = true)
                UsbManager.ACTION_USB_DEVICE_DETACHED -> {
                    if (current?.device?.deviceName !in usb?.deviceList.orEmpty().keys) stopOutput()
                    scan(false)
                }
            }
        }
    }
    private val inputListener = object : InputManager.InputDeviceListener {
        override fun onInputDeviceAdded(id: Int) = notifyAvailability()
        override fun onInputDeviceChanged(id: Int) { if (inputDeviceId() == null) silence(); notifyAvailability() }
        override fun onInputDeviceRemoved(id: Int) { silence(); notifyAvailability() }
    }

    fun configure(active: Boolean, strengthPercent: Int) {
        strength = strengthPercent.coerceIn(0, 100)
        if (enabled == active) return
        enabled = active
        if (active) suspended = false
        if (active && Build.VERSION.SDK_INT >= 26) {
            if (!registered) {
                ContextCompat.registerReceiver(context, receiver, IntentFilter(permissionAction).apply {
                    addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED)
                    addAction(UsbManager.ACTION_USB_DEVICE_DETACHED)
                }, ContextCompat.RECEIVER_NOT_EXPORTED)
                inputs.registerInputDeviceListener(inputListener, main)
                registered = true
            }
            scan(false, autoPermission = true)
        } else {
            permissionPrompt.reset()
            stopOutput()
            if (registered) {
                context.unregisterReceiver(receiver)
                inputs.unregisterInputDeviceListener(inputListener)
                registered = false
            }
            publish(if (active) "Requires Android 8 or newer" else "Kishi USB haptics disabled")
        }
    }

    fun addAvailabilityListener(listener: () -> Unit): () -> Unit {
        callbacks.add(listener)
        return { callbacks.remove(listener) }
    }

    private fun notifyAvailability() { main.post { callbacks.forEach { it() } } }
    private fun publish(message: String) {
        mutableStatus.value = message
        NativeInputDiagnostics.addRetained("kishi.status", message)
        notifyAvailability()
    }

    /** Explicit UI action: request permission or retry a failed initialization. */
    fun authorize() { suspended = false; scan(true) }

    /** Also covers an already-connected controller on cold start or return from Home/PiP. */
    fun onAppForegrounded() {
        permissionPrompt.onForeground()
        scan(false, autoPermission = true)
    }

    fun onAppBackgrounded() { permissionPrompt.onBackground() }

    fun beginStream() { streamActive = true; suspended = false; scan(false, autoPermission = true) }

    fun endStream() {
        streamActive = false
        suspendOutput()
    }

    fun closeStandaloneTest() {
        cancelTest()
        if (!streamActive) suspendOutput()
    }

    private fun suspendOutput() {
        suspended = true
        stopOutput()
        if (enabled) publish("Kishi output paused — authorize to test, or start a game")
    }

    private fun scan(requestPermission: Boolean, autoPermission: Boolean = false) {
        if (!enabled || Build.VERSION.SDK_INT < 26) return
        val usb = this.usb ?: run { publish("USB host unavailable on this device"); return }
        val devices = usb.deviceList.values.filter { kishiSensaIdentity(it.vendorId, it.productId, it.productName) }
        permissionPrompt.retainDevices(devices.map { it.deviceName }.toSet())
        if (suspended && !autoPermission) return
        if (devices.size != 1) {
            stopOutput()
            publish(if (devices.isEmpty()) "Connect Kishi V3 Pro / Pro XL" else "Multiple Kishi controllers: USB haptics paused")
            return
        }
        val device = devices.single()
        val interfaces = (0 until device.interfaceCount).map(device::getInterface)
        val iface = interfaces.singleOrNull { candidate ->
            KishiSensaDescriptor(device.vendorId, device.productId, candidate.id, candidate.alternateSetting,
                candidate.interfaceClass, (0 until candidate.endpointCount).map { i ->
                    val ep = candidate.getEndpoint(i); Triple(ep.address, ep.type, ep.maxPacketSize)
                }, device.productName).supported()
        }
        if (iface == null) {
            stopOutput()
            publish("Unsupported Kishi USB layout; Sensa interface 4 required")
            return
        }
        if (current != null) return // A closing worker must release its interface before any replacement.
        if (!usb.hasPermission(device)) {
            publish("USB permission required — select Authorize / Retry")
            if ((requestPermission || autoPermission) &&
                permissionPrompt.request(device.deviceName, manual = requestPermission)) {
                try {
                    val pending = PendingIntent.getBroadcast(context, 724,
                        Intent(permissionAction).setPackage(context.packageName), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
                    usb.requestPermission(device, pending)
                    NativeInputDiagnostics.addRetained("kishi.permission",
                        "Kishi USB permission requested ${if (requestPermission) "manually" else "automatically"}")
                } catch (error: Exception) {
                    permissionPrompt.complete(device.deviceName)
                    publish("USB permission request failed — select Authorize / Retry")
                }
            }
            return
        }
        permissionPrompt.complete(device.deviceName)
        if (suspended) return
        val output = Output(device, iface)
        current = output
        publish("Initializing Kishi Sensa ${device.vendorId.toString(16)}:${device.productId.toString(16)}")
        output.thread = Thread({ run(output) }, "opennow-kishi-haptics").apply { isDaemon = true }
        output.thread.start()
    }

    fun inputDeviceId(): Int? {
        val output = current ?: return null
        val usb = this.usb ?: return null
        if (!output.ready || output.stopping) return null
        if (usb.deviceList.values.count { it.vendorId == output.device.vendorId && it.productId == output.device.productId } != 1) return null
        return InputDevice.getDeviceIds().asSequence().mapNotNull(InputDevice::getDevice).filter {
            AndroidControllerInput.isControllerDevice(it) && it.vendorId == output.device.vendorId && it.productId == output.device.productId
        }.singleOrNull()?.id
    }

    fun rumble(strong: Int, weak: Int): Boolean {
        val output = current ?: return false
        if (!output.ready || output.stopping || inputDeviceId() == null) return false
        output.target.set(Target(strong.coerceIn(0, 65535), weak.coerceIn(0, 65535)))
        LockSupport.unpark(output.thread)
        return true
    }

    fun test(left: Boolean) {
        val output = current ?: return
        if (inputDeviceId() == null) return
        output.target.set(Target(if (left) 32768 else 0, if (left) 0 else 32768,
            SystemClock.elapsedRealtime() + 400, true))
        NativeInputDiagnostics.addRetained("kishi.test", "Kishi local test side=${if (left) "left" else "right"} durationMs=400")
        LockSupport.unpark(output.thread)
    }

    fun cancelTest() {
        val output = current ?: return
        if (output.target.get().test) silence()
    }

    fun silence() {
        current?.let { it.target.set(Target()); LockSupport.unpark(it.thread) }
    }

    private fun stopOutput() {
        current?.let { it.stopping = true; it.ready = false; it.target.set(Target()); LockSupport.unpark(it.thread) }
        notifyAvailability()
    }

    private fun run(output: Output) {
        var transport: KishiSensaUsb? = null
        var failureMessage: String? = null
        var packets = 0L
        try {
            val connection = usb?.openDevice(output.device) ?: error("USB device cannot be opened")
            transport = KishiSensaUsb(connection, output.iface)
            transport.initialize { output.stopping }
            if (output.stopping) return
            output.ready = true
            publish("Kishi Sensa ready — game rumble and left/right tests available")
            val encoder = KishiSensaRumble()
            var playing = false
            while (!output.stopping) {
                var target = output.target.get()
                if (target.expires != 0L && SystemClock.elapsedRealtime() >= target.expires) {
                    // Do not overwrite a newer game command that replaced the test.
                    output.target.compareAndSet(target, Target())
                    target = output.target.get()
                }
                val active = target.strong != 0 || target.weak != 0
                if (active || playing) {
                    val started = System.nanoTime()
                    val gain = if (target.test) minOf(strength, 40) else strength
                    transport.write(encoder.frame(target.strong, target.weak, gain)) { output.stopping }
                    packets++
                    if (active) {
                        NativeInputDiagnostics.retainThrottled("kishi.active-transfer", 1_000L) {
                            "Kishi firmware acknowledged active frame=$packets strong=${target.strong} weak=${target.weak} test=${target.test}"
                        }
                    }
                    if (packets == 1L || packets % 100 == 0L || !active) {
                        NativeInputDiagnostics.addRetained("kishi.transfers", "Kishi firmware acknowledged frames=$packets strong=${target.strong} weak=${target.weak} test=${target.test}")
                    }
                    playing = active
                    LockSupport.parkNanos((10_000_000L - (System.nanoTime() - started)).coerceAtLeast(0))
                } else LockSupport.parkNanos(50_000_000L)
            }
        } catch (failure: Exception) {
            if (!output.stopping) failureMessage = failure.message ?: failure.javaClass.simpleName
        } finally {
            output.ready = false
            runCatching { transport?.close() }.onFailure {
                NativeInputDiagnostics.addRetained("kishi.cleanup.error", "Kishi release failed: ${it.javaClass.simpleName}")
            }
            NativeInputDiagnostics.addRetained("kishi.output", "Kishi output closed acknowledgedFrames=$packets")
            main.post {
                if (current === output) {
                    current = null
                    if (failureMessage != null && enabled) publish("Kishi failed: $failureMessage — select Authorize / Retry")
                    else if (enabled) scan(false, autoPermission = true)
                    else notifyAvailability()
                }
            }
        }
    }
}
