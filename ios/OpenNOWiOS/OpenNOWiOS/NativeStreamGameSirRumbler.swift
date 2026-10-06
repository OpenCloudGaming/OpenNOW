// GameSir G8+ MFi accessory transport for OpenNOW.
// Protocol framing reference: VoidLink/Input/GameSirG8MFiRumbler.swift,
// by chrisnch and True砖家. The transport below is implemented for OpenNOW.
import Foundation

enum NativeStreamGameSirMotorPacket {
    static let accessoryProtocol = "com.xiaoji.M2boot"
    static func isTarget(vendorName: String?) -> Bool {
        guard let name = vendorName?.lowercased() else { return false }
        return name.contains("gamesir") && name.contains("g8+") && name.contains("mfi")
    }
    static func amplitude(_ magnitude: Int, gain: Double) -> UInt16 {
        UInt16(min(max(Double(magnitude), 0) * NativeStreamControllerRumbleGain.normalize(gain), 65535).rounded())
    }
    static func packet(low: UInt16, high: UInt16) -> Data {
        func byte(_ value: UInt16) -> UInt8 { UInt8((UInt32(value) * 255 + 32767) / 65535) }
        return Data([0x04, byte(low), 0x01, byte(high), 0x01, 0, 0, 0, 0])
    }
}

#if os(iOS)
import ExternalAccessory
import GameController
import UIKit

final class NativeStreamGameSirRumbler: NSObject, StreamDelegate {
    private var session: EASession?
    private var pending: Data?
    private var writing: Data?
    private var offset = 0
    private var held: Data?
    private var heartbeat: Timer?
    private var active = UIApplication.shared.applicationState == .active

    override init() {
        super.init()
        EAAccessoryManager.shared().registerForLocalNotifications()
        let center = NotificationCenter.default
        center.addObserver(self, selector: #selector(background), name: UIApplication.willResignActiveNotification, object: nil)
        center.addObserver(self, selector: #selector(foreground), name: UIApplication.didBecomeActiveNotification, object: nil)
        center.addObserver(self, selector: #selector(disconnected(_:)), name: .EAAccessoryDidDisconnect, object: nil)
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
        assert(Thread.isMainThread || session == nil)
        if Thread.isMainThread { stopAndClose() }
    }

    static func isTargetController(_ controller: GCController) -> Bool {
        NativeStreamGameSirMotorPacket.isTarget(vendorName: controller.vendorName)
    }

    func canHandleController(_ controller: GCController) -> Bool {
        Self.isTargetController(controller) && accessory() != nil
    }

    private func accessory() -> EAAccessory? {
        EAAccessoryManager.shared().connectedAccessories.first {
            $0.isConnected && $0.protocolStrings.contains(NativeStreamGameSirMotorPacket.accessoryProtocol)
                && $0.manufacturer.caseInsensitiveCompare("GameSir") == .orderedSame
                && $0.modelNumber.caseInsensitiveCompare("G8+ MFi") == .orderedSame
        }
    }

    @discardableResult
    func setLowFrequencyMotor(_ low: UInt16, highFrequencyMotor high: UInt16) -> Bool {
        precondition(Thread.isMainThread)
        guard active, open() else { return false }
        let packet = NativeStreamGameSirMotorPacket.packet(low: low, high: high)
        if low == 0 && high == 0 {
            held = nil
            heartbeat?.invalidate()
            heartbeat = nil
        } else {
            held = packet
            if heartbeat == nil {
                let timer = Timer(timeInterval: 0.167253, repeats: true) { [weak self] _ in
                    guard let self, self.active, let held = self.held else { return }
                    self.pending = held
                    self.flush()
                }
                heartbeat = timer
                RunLoop.main.add(timer, forMode: .common)
            }
        }
        // Keep at most a partial packet and the latest requested state.
        pending = packet
        flush()
        return session != nil
    }

    private func open() -> Bool {
        if session != nil { return true }
        guard let accessory = accessory(),
              let opened = EASession(accessory: accessory, forProtocol: NativeStreamGameSirMotorPacket.accessoryProtocol),
              let input = opened.inputStream, let output = opened.outputStream else {
            NativeStreamRumbleDiagnostics.shared.record("gameSirSessionUnavailable")
            return false
        }
        session = opened
        for stream in [input, output] {
            stream.delegate = self
            stream.schedule(in: .main, forMode: .common)
            stream.open()
        }
        NativeStreamRumbleDiagnostics.shared.record("gameSirSessionOpened", details: [
            "controllerTransport": "external-accessory", "accessoryProtocol": NativeStreamGameSirMotorPacket.accessoryProtocol])
        return true
    }

    private func flush() {
        guard let output = session?.outputStream, output.streamStatus == .open else { return }
        while output.hasSpaceAvailable {
            if writing == nil {
                guard let next = pending else { return }
                writing = next
                pending = nil
                offset = 0
            }
            guard let packet = writing else { return }
            let count = packet.withUnsafeBytes { bytes -> Int in
                guard let base = bytes.bindMemory(to: UInt8.self).baseAddress else { return -1 }
                return output.write(base.advanced(by: offset), maxLength: packet.count - offset)
            }
            guard count > 0 else {
                if count < 0 {
                    NativeStreamRumbleDiagnostics.shared.record("gameSirWriteError", details: [
                        "accessoryError": output.streamError?.localizedDescription ?? "Accessory write failed"])
                    close()
                }
                return
            }
            offset += count
            if offset == packet.count {
                NativeStreamRumbleDiagnostics.shared.record("gameSirPacketWritten", details: [
                    "controllerTransport": "external-accessory", "gameSirLowMotor": String(packet[1]),
                    "gameSirHighMotor": String(packet[3])])
                writing = nil
                offset = 0
            }
        }
    }

    func stopAndClose() {
        precondition(Thread.isMainThread)
        // Finish any partial packet before attempting the explicit zero command.
        if session != nil {
            pending = NativeStreamGameSirMotorPacket.packet(low: 0, high: 0)
            flush()
        }
        close()
    }

    private func close() {
        heartbeat?.invalidate()
        heartbeat = nil
        held = nil
        if let session {
            for stream in [session.inputStream, session.outputStream].compactMap({ $0 }) {
                stream.close()
                stream.remove(from: .main, forMode: .common)
                stream.delegate = nil
            }
        }
        session = nil
        pending = nil
        writing = nil
        offset = 0
    }

    @objc private func background() { active = false; stopAndClose() }
    @objc private func foreground() { active = true }
    @objc private func disconnected(_ notification: Notification) {
        guard let disconnected = notification.userInfo?[EAAccessoryKey] as? EAAccessory,
              disconnected.connectionID == session?.accessory?.connectionID else { return }
        stopAndClose()
    }

    func stream(_ stream: Stream, handle eventCode: Stream.Event) {
        guard let session, stream === session.inputStream || stream === session.outputStream else { return }
        switch eventCode {
        case .openCompleted, .hasSpaceAvailable:
            if stream === session.outputStream { flush() }
        case .hasBytesAvailable:
            guard let input = session.inputStream else { return }
            var bytes = [UInt8](repeating: 0, count: 64)
            while input.hasBytesAvailable {
                if input.read(&bytes, maxLength: bytes.count) <= 0 { break }
            }
        case .errorOccurred, .endEncountered:
            NativeStreamRumbleDiagnostics.shared.record("gameSirTransportError", details: [
                "accessoryError": stream.streamError?.localizedDescription ?? "Accessory stream ended"])
            stopAndClose()
        default: break
        }
    }
}
#endif
