import Foundation
import CoreGraphics

#if canImport(GameController)
import GameController
#endif

#if canImport(CoreHaptics)
import CoreHaptics
#endif

#if canImport(UIKit)
import UIKit
#endif

enum NativeStreamControllerRumbleGain {
    static let range: ClosedRange<Double> = 0...64
    static func normalize(_ multiplier: Double) -> Double {
        multiplier.isFinite ? min(max(multiplier, range.lowerBound), range.upperBound) : 1
    }
    static func label(_ multiplier: Double) -> String {
        let gain = normalize(multiplier)
        guard gain > 0 else { return "Off" }
        return String(format: "%.0f%%", gain / range.upperBound * 100)
    }
    static func apply(_ intensity: Float, multiplier: Double) -> Float {
        guard intensity.isFinite else { return 0 }
        return min(max(intensity, 0) * Float(normalize(multiplier)), 1)
    }
}

/// Bounded, account-free status snapshot for isolating controller rumble failures.
final class NativeStreamRumbleDiagnostics {
    static let shared = NativeStreamRumbleDiagnostics()
    private let lock = NSLock()
    private let writer = DispatchQueue(label: "OpenNOW.rumbleStatus")
    private var fields: [String: String] = [:]
    private var counts: [String: Int] = [:]
    private var lastWrite = Date.distantPast

    func record(_ event: String, details: [String: String] = [:]) {
        lock.lock()
        counts[event, default: 0] += 1
        fields.merge(details) { _, new in new }
        fields["lastEvent"] = event
        let now = Date()
        let shouldWrite = now.timeIntervalSince(lastWrite) >= 1
        if shouldWrite { fields["updatedAt"] = ISO8601DateFormatter().string(from: now) }
        let snapshot: [String: Any] = ["fields": fields, "counts": counts]
        if shouldWrite { lastWrite = now }
        lock.unlock()
        guard shouldWrite else { return }
        writer.async {
            guard let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first,
                  let data = try? JSONSerialization.data(withJSONObject: snapshot, options: [.sortedKeys]) else { return }
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try? data.write(to: directory.appendingPathComponent("controller-rumble-status.json"), options: .atomic)
        }
    }
}

protocol NativeStreamInputSink: AnyObject {
    func sendReliableInput(_ data: Data)
    func sendPartiallyReliableInput(_ data: Data)
    func logInputEvent(_ message: String)
}

struct NativeStreamKeyboardMapping: Equatable {
    let virtualKey: UInt16
    let scanCode: UInt16
}

struct NativeStreamGamepadState: Equatable {
    let controllerId: Int
    let buttons: UInt16
    let leftTrigger: UInt8
    let rightTrigger: UInt8
    let leftStickX: Int16
    let leftStickY: Int16
    let rightStickX: Int16
    let rightStickY: Int16
    let connected: Bool
}

struct NativeStreamVirtualGamepadState: Equatable {
    var buttons: UInt16 = 0
    var leftTrigger: UInt8 = 0
    var rightTrigger: UInt8 = 0
    var leftStickX: Int16 = 0
    var leftStickY: Int16 = 0
    var rightStickX: Int16 = 0
    var rightStickY: Int16 = 0
    var leftStickActive = false
    var rightStickActive = false
}

enum NativeStreamMouseCoordinatePolicy {
    /// GameController reports relative mouse Y in Cartesian coordinates (up is positive), while
    /// the host input protocol uses screen coordinates (down is positive). Touch deltas already
    /// arrive in screen coordinates, so the conversion belongs only at the physical-mouse edge.
    static func hostDelta(deltaX: Float, deltaY: Float) -> CGPoint {
        CGPoint(x: CGFloat(deltaX), y: CGFloat(-deltaY))
    }
}

enum NativeStreamGamepadMixer {
    static func merging(
        physical: NativeStreamGamepadState,
        virtual: NativeStreamVirtualGamepadState
    ) -> NativeStreamGamepadState {
        NativeStreamGamepadState(
            controllerId: physical.controllerId,
            buttons: physical.buttons | virtual.buttons,
            leftTrigger: max(physical.leftTrigger, virtual.leftTrigger),
            rightTrigger: max(physical.rightTrigger, virtual.rightTrigger),
            leftStickX: virtual.leftStickActive ? virtual.leftStickX : physical.leftStickX,
            leftStickY: virtual.leftStickActive ? virtual.leftStickY : physical.leftStickY,
            rightStickX: virtual.rightStickActive ? virtual.rightStickX : physical.rightStickX,
            rightStickY: virtual.rightStickActive ? virtual.rightStickY : physical.rightStickY,
            connected: physical.connected
        )
    }
}

struct NativeStreamUnicodeInputBatch: Equatable {
    let characterCount: Int
    let packets: [Data]
}

enum NativeStreamVirtualGamepadButton: UInt16 {
    case dpadUp = 0x0001
    case dpadDown = 0x0002
    case dpadLeft = 0x0004
    case dpadRight = 0x0008
    case menu = 0x0010
    case options = 0x0020
    case leftStick = 0x0040
    case rightStick = 0x0080
    case leftShoulder = 0x0100
    case rightShoulder = 0x0200
    case home = 0x0400
    case a = 0x1000
    case b = 0x2000
    case x = 0x4000
    case y = 0x8000
}

enum NativeStreamVirtualGamepadStick: Hashable {
    case left
    case right
}

enum NativeStreamVirtualGamepadTrigger {
    case left
    case right
}

final class NativeStreamInputEncoder {
    private enum EventType {
        static let heartbeat: UInt32 = 2
        static let keyDown: UInt32 = 3
        static let keyUp: UInt32 = 4
        static let mouseRelative: UInt32 = 7
        static let mouseButtonDown: UInt32 = 8
        static let mouseButtonUp: UInt32 = 9
        static let mouseWheel: UInt32 = 10
        static let gamepad: UInt32 = 12
        static let hapticsEnabled: UInt32 = 13
        static let unicode: UInt32 = 23

        /// Native multi-touch. The host turns these into a Windows digitizer, which is what makes
        /// touch-aware games switch to their mobile UI on their own.
        static let touch: UInt32 = 24
    }

    private var protocolVersion = NativeStreamSDP.defaultInputProtocolVersion
    private var gamepadSequences: [Int: UInt16] = [:]

    func setProtocolVersion(_ version: Int) {
        protocolVersion = max(1, min(255, version))
    }

    func resetGamepadSequences() {
        gamepadSequences.removeAll(keepingCapacity: true)
    }

    func encodeHeartbeat() -> Data {
        var bytes = [UInt8](repeating: 0, count: 4)
        Self.writeUInt32LE(EventType.heartbeat, to: &bytes, at: 0)
        return Data(bytes)
    }

    func encodeKeyDown(mapping: NativeStreamKeyboardMapping, modifiers: UInt16) -> Data {
        encodeKey(type: EventType.keyDown, mapping: mapping, modifiers: modifiers)
    }

    func encodeKeyUp(mapping: NativeStreamKeyboardMapping, modifiers: UInt16) -> Data {
        encodeKey(type: EventType.keyUp, mapping: mapping, modifiers: modifiers)
    }

    func encodeMouseMove(dx: Int, dy: Int) -> Data {
        var bytes = [UInt8](repeating: 0, count: 22)
        Self.writeUInt32LE(EventType.mouseRelative, to: &bytes, at: 0)
        Self.writeInt16BE(Self.clampInt16(dx), to: &bytes, at: 4)
        Self.writeInt16BE(Self.clampInt16(dy), to: &bytes, at: 6)
        Self.writeUInt16BE(0, to: &bytes, at: 8)
        Self.writeUInt32BE(0, to: &bytes, at: 10)
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 14)
        return wrapMouseMove(Data(bytes))
    }

    func encodeMouseButton(button: Int, pressed: Bool) -> Data {
        var bytes = [UInt8](repeating: 0, count: 18)
        Self.writeUInt32LE(pressed ? EventType.mouseButtonDown : EventType.mouseButtonUp, to: &bytes, at: 0)
        bytes[4] = UInt8(min(max(button, 1), 5))
        bytes[5] = 0
        Self.writeUInt32BE(0, to: &bytes, at: 6)
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 10)
        return wrapSingle(Data(bytes))
    }

    func encodeMouseWheel(delta: Int) -> Data {
        var bytes = [UInt8](repeating: 0, count: 22)
        Self.writeUInt32LE(EventType.mouseWheel, to: &bytes, at: 0)
        Self.writeInt16BE(0, to: &bytes, at: 4)
        Self.writeInt16BE(Self.clampInt16(delta), to: &bytes, at: 6)
        Self.writeUInt16BE(0, to: &bytes, at: 8)
        Self.writeUInt32BE(0, to: &bytes, at: 10)
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 14)
        return wrapSingle(Data(bytes))
    }

    /// A batch of finger updates, one packet per event.
    ///
    /// Layout, taken from the official web client's encoder. Note the opcode is little-endian
    /// while everything after it is big-endian — the same split every other packet here uses.
    ///
    /// ```
    /// 0..3    opcode 24            uint32 LE
    /// 4..5    payload size         uint16 BE   = 8 + 16 * count
    /// 6..7    count                uint16 BE
    /// 8+      records, 16 bytes each:
    ///           +0     slot        uint8
    ///           +1     phase       uint8       1=down 2=up 4=move 8=cancel
    ///           +2..3  x           uint16 BE   0..65535 across the video area
    ///           +4..5  y           uint16 BE
    ///           +6     radiusX     uint8
    ///           +7     radiusY     uint8
    ///           +8..15 timestamp   int64 BE    microseconds
    /// ```
    ///
    /// Returns nil for an empty batch, so a caller cannot send a header describing nothing.
    func encodeTouchBatch(_ touches: [NativeTouchRecord]) -> Data? {
        guard !touches.isEmpty else { return nil }
        let nowUs = Self.timestampUs()
        let count = min(touches.count, nativeTouchMaxRecordsPerBatch)
        let payloadSize = 8 + 16 * count
        var bytes = [UInt8](repeating: 0, count: payloadSize)
        Self.writeUInt32LE(EventType.touch, to: &bytes, at: 0)
        Self.writeUInt16BE(UInt16(payloadSize), to: &bytes, at: 4)
        Self.writeUInt16BE(UInt16(count), to: &bytes, at: 6)
        for index in 0..<count {
            let touch = touches[index]
            let offset = 8 + 16 * index
            bytes[offset] = UInt8(min(max(touch.slot, 0), 255))
            bytes[offset + 1] = touch.phase
            Self.writeUInt16BE(UInt16(min(max(touch.x, 0), nativeTouchCoordinateMax)), to: &bytes, at: offset + 2)
            Self.writeUInt16BE(UInt16(min(max(touch.y, 0), nativeTouchCoordinateMax)), to: &bytes, at: offset + 4)
            bytes[offset + 6] = UInt8(min(max(touch.radiusX, 0), 255))
            bytes[offset + 7] = UInt8(min(max(touch.radiusY, 0), 255))
            Self.writeUInt64BE(touch.timestampUs != 0 ? touch.timestampUs : nowUs, to: &bytes, at: offset + 8)
        }
        return wrapSingle(Data(bytes))
    }

    func encodeHapticsEnabled(_ enabled: Bool) -> Data {
        var bytes = [UInt8](repeating: 0, count: 6)
        Self.writeUInt32LE(EventType.hapticsEnabled, to: &bytes, at: 0)
        Self.writeUInt16BE(enabled ? 1 : 0, to: &bytes, at: 4)
        return wrapSingle(Data(bytes))
    }

    /// Official GFN SendUnicode framing is `[0x22][u32 LE type 23][UTF-8]`.
    /// Each packet is independently valid UTF-8 and stays below the vendor's
    /// 1,016-byte text payload limit.
    func encodeUnicodeText(
        _ text: String,
        maximumCharacters: Int = 4_096,
        maximumPayloadBytes: Int = 1_016
    ) -> NativeStreamUnicodeInputBatch {
        let characterLimit = max(0, maximumCharacters)
        let payloadLimit = max(4, maximumPayloadBytes)
        let limitedText = String(text.prefix(characterLimit))
        guard !limitedText.isEmpty else {
            return NativeStreamUnicodeInputBatch(characterCount: 0, packets: [])
        }

        var packets: [Data] = []
        var chunk: [UInt8] = []
        chunk.reserveCapacity(payloadLimit)

        func packet(for payload: [UInt8]) -> Data {
            var bytes = [UInt8](repeating: 0, count: 5 + payload.count)
            bytes[0] = 0x22
            Self.writeUInt32LE(EventType.unicode, to: &bytes, at: 1)
            bytes.replaceSubrange(5..<bytes.count, with: payload)
            return Data(bytes)
        }

        for scalar in limitedText.unicodeScalars {
            let scalarBytes = Array(String(scalar).utf8)
            if chunk.count + scalarBytes.count > payloadLimit {
                if !chunk.isEmpty {
                    packets.append(packet(for: chunk))
                    chunk.removeAll(keepingCapacity: true)
                }
            }
            chunk.append(contentsOf: scalarBytes)
        }
        if !chunk.isEmpty {
            packets.append(packet(for: chunk))
        }

        return NativeStreamUnicodeInputBatch(
            characterCount: limitedText.count,
            packets: packets
        )
    }

    func encodeGamepadState(_ state: NativeStreamGamepadState, bitmap: UInt16, partiallyReliable: Bool) -> Data {
        var bytes = [UInt8](repeating: 0, count: 38)
        Self.writeUInt32LE(EventType.gamepad, to: &bytes, at: 0)
        Self.writeUInt16LE(26, to: &bytes, at: 4)
        Self.writeUInt16LE(UInt16(state.controllerId & 0x03), to: &bytes, at: 6)
        Self.writeUInt16LE(bitmap, to: &bytes, at: 8)
        Self.writeUInt16LE(20, to: &bytes, at: 10)
        Self.writeUInt16LE(state.buttons, to: &bytes, at: 12)
        Self.writeUInt16LE(UInt16(state.leftTrigger) | (UInt16(state.rightTrigger) << 8), to: &bytes, at: 14)
        Self.writeInt16LE(state.leftStickX, to: &bytes, at: 16)
        Self.writeInt16LE(state.leftStickY, to: &bytes, at: 18)
        Self.writeInt16LE(state.rightStickX, to: &bytes, at: 20)
        Self.writeInt16LE(state.rightStickY, to: &bytes, at: 22)
        Self.writeUInt16LE(0, to: &bytes, at: 24)
        Self.writeUInt16LE(85, to: &bytes, at: 26)
        Self.writeUInt16LE(0, to: &bytes, at: 28)
        Self.writeUInt64LE(Self.timestampUs(), to: &bytes, at: 30)
        return partiallyReliable
            ? wrapGamepadPartiallyReliable(Data(bytes), controllerId: state.controllerId)
            : wrapGamepadReliable(Data(bytes))
    }

    private func encodeKey(type: UInt32, mapping: NativeStreamKeyboardMapping, modifiers: UInt16) -> Data {
        var bytes = [UInt8](repeating: 0, count: 18)
        Self.writeUInt32LE(type, to: &bytes, at: 0)
        Self.writeUInt16BE(mapping.virtualKey, to: &bytes, at: 4)
        Self.writeUInt16BE(modifiers, to: &bytes, at: 6)
        Self.writeUInt16BE(mapping.scanCode, to: &bytes, at: 8)
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 10)
        return wrapSingle(Data(bytes))
    }

    private func wrapSingle(_ payload: Data) -> Data {
        guard protocolVersion > 2 else { return payload }
        var bytes = [UInt8](repeating: 0, count: 10 + payload.count)
        bytes[0] = 0x23
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 1)
        bytes[9] = 0x22
        bytes.replaceSubrange(10..<(10 + payload.count), with: payload)
        return Data(bytes)
    }

    private func wrapMouseMove(_ payload: Data) -> Data {
        guard protocolVersion > 2 else { return payload }
        var bytes = [UInt8](repeating: 0, count: 12 + payload.count)
        bytes[0] = 0x23
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 1)
        bytes[9] = 0x21
        Self.writeUInt16BE(UInt16(payload.count), to: &bytes, at: 10)
        bytes.replaceSubrange(12..<(12 + payload.count), with: payload)
        return Data(bytes)
    }

    private func wrapGamepadReliable(_ payload: Data) -> Data {
        guard protocolVersion > 2 else { return payload }
        var bytes = [UInt8](repeating: 0, count: 12 + payload.count)
        bytes[0] = 0x23
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 1)
        bytes[9] = 0x21
        Self.writeUInt16BE(UInt16(payload.count), to: &bytes, at: 10)
        bytes.replaceSubrange(12..<(12 + payload.count), with: payload)
        return Data(bytes)
    }

    private func wrapGamepadPartiallyReliable(_ payload: Data, controllerId: Int) -> Data {
        guard protocolVersion > 2 else { return payload }
        let index = controllerId & 0x03
        let sequence = gamepadSequences[index] ?? 1
        gamepadSequences[index] = sequence &+ 1
        var bytes = [UInt8](repeating: 0, count: 16 + payload.count)
        bytes[0] = 0x23
        Self.writeUInt64BE(Self.timestampUs(), to: &bytes, at: 1)
        bytes[9] = 0x26
        bytes[10] = UInt8(index)
        Self.writeUInt16BE(sequence, to: &bytes, at: 11)
        bytes[13] = 0x21
        Self.writeUInt16BE(UInt16(payload.count), to: &bytes, at: 14)
        bytes.replaceSubrange(16..<(16 + payload.count), with: payload)
        return Data(bytes)
    }

    private static func timestampUs() -> UInt64 {
        UInt64((ProcessInfo.processInfo.systemUptime * 1_000_000).rounded())
    }

    private static func clampInt16(_ value: Int) -> Int16 {
        Int16(min(max(value, Int(Int16.min)), Int(Int16.max)))
    }

    private static func writeUInt16LE(_ value: UInt16, to bytes: inout [UInt8], at offset: Int) {
        bytes[offset] = UInt8(value & 0xff)
        bytes[offset + 1] = UInt8((value >> 8) & 0xff)
    }

    private static func writeUInt16BE(_ value: UInt16, to bytes: inout [UInt8], at offset: Int) {
        bytes[offset] = UInt8((value >> 8) & 0xff)
        bytes[offset + 1] = UInt8(value & 0xff)
    }

    private static func writeInt16LE(_ value: Int16, to bytes: inout [UInt8], at offset: Int) {
        writeUInt16LE(UInt16(bitPattern: value), to: &bytes, at: offset)
    }

    private static func writeInt16BE(_ value: Int16, to bytes: inout [UInt8], at offset: Int) {
        writeUInt16BE(UInt16(bitPattern: value), to: &bytes, at: offset)
    }

    private static func writeUInt32LE(_ value: UInt32, to bytes: inout [UInt8], at offset: Int) {
        bytes[offset] = UInt8(value & 0xff)
        bytes[offset + 1] = UInt8((value >> 8) & 0xff)
        bytes[offset + 2] = UInt8((value >> 16) & 0xff)
        bytes[offset + 3] = UInt8((value >> 24) & 0xff)
    }

    private static func writeUInt32BE(_ value: UInt32, to bytes: inout [UInt8], at offset: Int) {
        bytes[offset] = UInt8((value >> 24) & 0xff)
        bytes[offset + 1] = UInt8((value >> 16) & 0xff)
        bytes[offset + 2] = UInt8((value >> 8) & 0xff)
        bytes[offset + 3] = UInt8(value & 0xff)
    }

    private static func writeUInt64LE(_ value: UInt64, to bytes: inout [UInt8], at offset: Int) {
        for index in 0..<8 {
            bytes[offset + index] = UInt8((value >> UInt64(index * 8)) & 0xff)
        }
    }

    private static func writeUInt64BE(_ value: UInt64, to bytes: inout [UInt8], at offset: Int) {
        for index in 0..<8 {
            let shift = UInt64((7 - index) * 8)
            bytes[offset + index] = UInt8((value >> shift) & 0xff)
        }
    }
}

#if canImport(GameController)
enum NativeStreamPhysicalInput {
    static var keyboardOrMouseConnected: Bool {
        GCKeyboard.coalesced != nil || !GCMouse.mice().isEmpty
    }
}

enum NativeStreamKeyboardMapper {
    static func mapping(for keyCode: GCKeyCode) -> NativeStreamKeyboardMapping? {
        mappingByKey[keyCode]
    }

    static func modifiers(for keyboardInput: GCKeyboardInput, changedKey: GCKeyCode, pressed: Bool) -> UInt16 {
        var flags: UInt16 = 0
        if isPressed(.leftShift, or: .rightShift, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x01 }
        if isPressed(.leftControl, or: .rightControl, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x02 }
        if isPressed(.leftAlt, or: .rightAlt, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x04 }
        if isPressed(.leftGUI, or: .rightGUI, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x08 }
        if isKeyPressed(.capsLock, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x10 }
        if isKeyPressed(.keypadNumLock, in: keyboardInput, changedKey: changedKey, pressed: pressed) { flags |= 0x20 }
        return flags
    }

    private static func isPressed(
        _ lhs: GCKeyCode,
        or rhs: GCKeyCode,
        in keyboardInput: GCKeyboardInput,
        changedKey: GCKeyCode,
        pressed: Bool
    ) -> Bool {
        isKeyPressed(lhs, in: keyboardInput, changedKey: changedKey, pressed: pressed)
            || isKeyPressed(rhs, in: keyboardInput, changedKey: changedKey, pressed: pressed)
    }

    private static func isKeyPressed(
        _ keyCode: GCKeyCode,
        in keyboardInput: GCKeyboardInput,
        changedKey: GCKeyCode,
        pressed: Bool
    ) -> Bool {
        changedKey == keyCode ? pressed : (keyboardInput.button(forKeyCode: keyCode)?.isPressed == true)
    }

    private static let mappingByKey: [GCKeyCode: NativeStreamKeyboardMapping] = [
        .keyA: .init(virtualKey: 0x41, scanCode: 0x001e),
        .keyB: .init(virtualKey: 0x42, scanCode: 0x0030),
        .keyC: .init(virtualKey: 0x43, scanCode: 0x002e),
        .keyD: .init(virtualKey: 0x44, scanCode: 0x0020),
        .keyE: .init(virtualKey: 0x45, scanCode: 0x0012),
        .keyF: .init(virtualKey: 0x46, scanCode: 0x0021),
        .keyG: .init(virtualKey: 0x47, scanCode: 0x0022),
        .keyH: .init(virtualKey: 0x48, scanCode: 0x0023),
        .keyI: .init(virtualKey: 0x49, scanCode: 0x0017),
        .keyJ: .init(virtualKey: 0x4a, scanCode: 0x0024),
        .keyK: .init(virtualKey: 0x4b, scanCode: 0x0025),
        .keyL: .init(virtualKey: 0x4c, scanCode: 0x0026),
        .keyM: .init(virtualKey: 0x4d, scanCode: 0x0032),
        .keyN: .init(virtualKey: 0x4e, scanCode: 0x0031),
        .keyO: .init(virtualKey: 0x4f, scanCode: 0x0018),
        .keyP: .init(virtualKey: 0x50, scanCode: 0x0019),
        .keyQ: .init(virtualKey: 0x51, scanCode: 0x0010),
        .keyR: .init(virtualKey: 0x52, scanCode: 0x0013),
        .keyS: .init(virtualKey: 0x53, scanCode: 0x001f),
        .keyT: .init(virtualKey: 0x54, scanCode: 0x0014),
        .keyU: .init(virtualKey: 0x55, scanCode: 0x0016),
        .keyV: .init(virtualKey: 0x56, scanCode: 0x002f),
        .keyW: .init(virtualKey: 0x57, scanCode: 0x0011),
        .keyX: .init(virtualKey: 0x58, scanCode: 0x002d),
        .keyY: .init(virtualKey: 0x59, scanCode: 0x0015),
        .keyZ: .init(virtualKey: 0x5a, scanCode: 0x002c),
        .one: .init(virtualKey: 0x31, scanCode: 0x0002),
        .two: .init(virtualKey: 0x32, scanCode: 0x0003),
        .three: .init(virtualKey: 0x33, scanCode: 0x0004),
        .four: .init(virtualKey: 0x34, scanCode: 0x0005),
        .five: .init(virtualKey: 0x35, scanCode: 0x0006),
        .six: .init(virtualKey: 0x36, scanCode: 0x0007),
        .seven: .init(virtualKey: 0x37, scanCode: 0x0008),
        .eight: .init(virtualKey: 0x38, scanCode: 0x0009),
        .nine: .init(virtualKey: 0x39, scanCode: 0x000a),
        .zero: .init(virtualKey: 0x30, scanCode: 0x000b),
        .returnOrEnter: .init(virtualKey: 0x0d, scanCode: 0x001c),
        .escape: .init(virtualKey: 0x1b, scanCode: 0x0001),
        .deleteOrBackspace: .init(virtualKey: 0x08, scanCode: 0x000e),
        .tab: .init(virtualKey: 0x09, scanCode: 0x000f),
        .spacebar: .init(virtualKey: 0x20, scanCode: 0x0039),
        .hyphen: .init(virtualKey: 0xbd, scanCode: 0x000c),
        .equalSign: .init(virtualKey: 0xbb, scanCode: 0x000d),
        .openBracket: .init(virtualKey: 0xdb, scanCode: 0x001a),
        .closeBracket: .init(virtualKey: 0xdd, scanCode: 0x001b),
        .backslash: .init(virtualKey: 0xdc, scanCode: 0x002b),
        .semicolon: .init(virtualKey: 0xba, scanCode: 0x0027),
        .quote: .init(virtualKey: 0xde, scanCode: 0x0028),
        .graveAccentAndTilde: .init(virtualKey: 0xc0, scanCode: 0x0029),
        .comma: .init(virtualKey: 0xbc, scanCode: 0x0033),
        .period: .init(virtualKey: 0xbe, scanCode: 0x0034),
        .slash: .init(virtualKey: 0xbf, scanCode: 0x0035),
        .capsLock: .init(virtualKey: 0x14, scanCode: 0x003a),
        .F1: .init(virtualKey: 0x70, scanCode: 0x003b),
        .F2: .init(virtualKey: 0x71, scanCode: 0x003c),
        .F3: .init(virtualKey: 0x72, scanCode: 0x003d),
        .F4: .init(virtualKey: 0x73, scanCode: 0x003e),
        .F5: .init(virtualKey: 0x74, scanCode: 0x003f),
        .F6: .init(virtualKey: 0x75, scanCode: 0x0040),
        .F7: .init(virtualKey: 0x76, scanCode: 0x0041),
        .F8: .init(virtualKey: 0x77, scanCode: 0x0042),
        .F9: .init(virtualKey: 0x78, scanCode: 0x0043),
        .F10: .init(virtualKey: 0x79, scanCode: 0x0044),
        .F11: .init(virtualKey: 0x7a, scanCode: 0x0057),
        .F12: .init(virtualKey: 0x7b, scanCode: 0x0058),
        .printScreen: .init(virtualKey: 0x2c, scanCode: 0xe037),
        .scrollLock: .init(virtualKey: 0x91, scanCode: 0x0046),
        .pause: .init(virtualKey: 0x13, scanCode: 0x0045),
        .insert: .init(virtualKey: 0x2d, scanCode: 0xe052),
        .home: .init(virtualKey: 0x24, scanCode: 0xe047),
        .pageUp: .init(virtualKey: 0x21, scanCode: 0xe049),
        .deleteForward: .init(virtualKey: 0x2e, scanCode: 0xe053),
        .end: .init(virtualKey: 0x23, scanCode: 0xe04f),
        .pageDown: .init(virtualKey: 0x22, scanCode: 0xe051),
        .rightArrow: .init(virtualKey: 0x27, scanCode: 0xe04d),
        .leftArrow: .init(virtualKey: 0x25, scanCode: 0xe04b),
        .downArrow: .init(virtualKey: 0x28, scanCode: 0xe050),
        .upArrow: .init(virtualKey: 0x26, scanCode: 0xe048),
        .leftControl: .init(virtualKey: 0xa2, scanCode: 0x001d),
        .leftShift: .init(virtualKey: 0xa0, scanCode: 0x002a),
        .leftAlt: .init(virtualKey: 0xa4, scanCode: 0x0038),
        .leftGUI: .init(virtualKey: 0x5b, scanCode: 0xe05b),
        .rightControl: .init(virtualKey: 0xa3, scanCode: 0xe01d),
        .rightShift: .init(virtualKey: 0xa1, scanCode: 0x0036),
        .rightAlt: .init(virtualKey: 0xa5, scanCode: 0xe038),
        .rightGUI: .init(virtualKey: 0x5c, scanCode: 0xe05c),
        .keypadEnter: .init(virtualKey: 0x0d, scanCode: 0xe01c),
        .application: .init(virtualKey: 0x5d, scanCode: 0xe05d)
    ]
}

#if canImport(CoreHaptics)
private struct NativeStreamRumbleProfile: Equatable {
    let intensity: Float
    let sharpnessControl: Float

    init(weakMagnitude: Int, strongMagnitude: Int) {
        let weak = min(max(Float(weakMagnitude) / 65_535, 0), 1)
        let strong = min(max(Float(strongMagnitude) / 65_535, 0), 1)
        intensity = min(max((strong * 0.78) + (weak * 0.48), 0), 1)
        let sharpness = min(max((weak * 0.75) + (strong * 0.25), 0), 1)
        sharpnessControl = (sharpness * 2) - 1
    }

    var isStopped: Bool { intensity <= 0.001 }

    func materiallyDiffers(from other: NativeStreamRumbleProfile) -> Bool {
        abs(intensity - other.intensity) >= 0.04
            || abs(sharpnessControl - other.sharpnessControl) >= 0.08
    }
}

private final class NativeStreamHapticPlayback {
    static let loopDuration: TimeInterval = 1

    let engine: CHHapticEngine
    let player: CHHapticAdvancedPatternPlayer
    let controllerIdentifier: ObjectIdentifier?
    var isPlaying = false
    var lastProfile: NativeStreamRumbleProfile?
    var lastUpdateAt: TimeInterval = 0

    init(engine: CHHapticEngine, controllerIdentifier: ObjectIdentifier?) throws {
        self.engine = engine
        self.controllerIdentifier = controllerIdentifier
        let event = CHHapticEvent(
            eventType: .hapticContinuous,
            parameters: [
                CHHapticEventParameter(parameterID: .hapticIntensity, value: 1),
                CHHapticEventParameter(parameterID: .hapticSharpness, value: 0.5)
            ],
            relativeTime: 0,
            duration: Self.loopDuration
        )
        let pattern = try CHHapticPattern(events: [event], parameters: [])
        player = try engine.makeAdvancedPlayer(with: pattern)
        player.loopEnabled = true
        player.loopEnd = Self.loopDuration
    }

    func stopPlayer() {
        if isPlaying {
            try? player.stop(atTime: CHHapticTimeImmediate)
        }
        isPlaying = false
        lastProfile = nil
        lastUpdateAt = 0
    }

    func shutdown() {
        stopPlayer()
        engine.stop(completionHandler: nil)
    }

    func markEngineStopped() {
        isPlaying = false
        lastProfile = nil
        lastUpdateAt = 0
    }
}
#endif

final class NativeStreamInputBridge {
    weak var sink: NativeStreamInputSink?
    var onPhysicalControllerAvailabilityChanged: ((Bool) -> Void)?
    var onPhysicalKeyboardMouseAvailabilityChanged: ((Bool) -> Void)?
    var onControllerShortcut: ((NativeStreamControllerShortcutAction) -> Void)?
    var onControllerHUDCommand: ((NativeStreamControllerHUDCommand) -> Void)?
    private var controllerShortcuts = NativeStreamControllerShortcuts()
    private var controllerHUDActive = false
    private var blockedButtons: Set<ObjectIdentifier> = []
    private var blockedAxesSlots: Set<Int> = []
    private var shortcutButtonIDsBySlot: [Int: Set<ObjectIdentifier>] = [:]
    private var hudDirection: NativeStreamControllerHUDCommand?
    private var hudRepeatAt: TimeInterval = 0
    private var hudRepeatTimer: Timer?

    private let encoder = NativeStreamInputEncoder()
    private var keyboard: GCKeyboard?
    private var mice: [GCMouse] = []
    private var controllersBySlot: [Int: GCController] = [:]
    private var controllerSlots: [ObjectIdentifier: Int] = [:]
    private var lastGamepadStates: [Int: NativeStreamGamepadState] = [:]
    private var gamepadKeepaliveTimer: Timer?
    private var heartbeatTimer: Timer?
    private var mouseEmulationTimer: Timer?
    private var mouseEmulationEnabled = false
    /// Latched so the click can be released even if the button state changes in the same frame.
    private var mouseEmulationHeldButtons: Set<Int> = []
    private var mouseEmulationScrollRemainder: CGFloat = 0
    private var lastHapticsAdvertisementAt: TimeInterval = -.infinity
    private var mouseAccumulator = CGPoint.zero
    private var mouseFlushScheduled = false
    private var mouseSensitivity: CGFloat = 1
    private var mouseScrollSensitivity: CGFloat = 30
    private var mouseAccelerationLevel = 1
    private var phoneRumbleFallbackEnabled = true
    #if os(iOS)
    private let gameSirRumbler = NativeStreamGameSirRumbler()
    private var gameSirLastRumble: (slot: Int, weak: Int, strong: Int)?
    #endif
    private var controllerRumbleStrength: Double = 1
    private var physicalControllerPassthroughEnabled = true
    private var virtualControllerEnabled = false
    private var virtualButtons: UInt16 = 0
    private var virtualLeftTrigger: UInt8 = 0
    private var virtualRightTrigger: UInt8 = 0
    private var virtualLeftStickX: Int16 = 0
    private var virtualLeftStickY: Int16 = 0
    private var virtualRightStickX: Int16 = 0
    private var virtualRightStickY: Int16 = 0
    private var virtualLeftStickActive = false
    private var virtualRightStickActive = false
    private var partiallyReliableGamepadMask = UInt16(NativeStreamSDP.partiallyReliableGamepadMaskAll)
    #if canImport(CoreHaptics)
    private static let hapticUpdateInterval: TimeInterval = 0.035
    private static let phoneHapticsSupported = CHHapticEngine.capabilitiesForHardware().supportsHaptics
    private var controllerHapticsBySlot: [Int: NativeStreamHapticPlayback] = [:]
    private var phoneHapticPlayback: NativeStreamHapticPlayback?
    private var phoneHapticsRetryAfter: TimeInterval = 0
    private var lastHapticsFailureLogAt: TimeInterval = -.infinity
    #endif

    func configure(protocolVersion: Int, partiallyReliableGamepadMask: Int) {
        encoder.setProtocolVersion(protocolVersion)
        self.partiallyReliableGamepadMask = UInt16(partiallyReliableGamepadMask & 0xffff)
    }

    func configureUserPreferences(
        mouseSensitivity: Double,
        mouseAcceleration: Int,
        phoneRumbleFallback: Bool,
        physicalControllerPassthrough: Bool,
        controllerMouseEmulation: Bool = false,
        mouseScrollSensitivity: Int = 30,
        controllerRumbleStrength: Double = 1
    ) {
        setControllerRumbleStrength(controllerRumbleStrength)
        self.mouseSensitivity = CGFloat(min(max(mouseSensitivity, 0.25), 3))
        mouseAccelerationLevel = min(max(mouseAcceleration, 0), 2)
        self.mouseScrollSensitivity = CGFloat(min(max(mouseScrollSensitivity, 10), 100))
        setControllerMouseEmulation(controllerMouseEmulation)
        if phoneRumbleFallbackEnabled, !phoneRumbleFallback {
            stopPhoneRumble(shutdown: true)
        }
        phoneRumbleFallbackEnabled = phoneRumbleFallback
        setPhysicalControllerPassthrough(physicalControllerPassthrough)
        advertiseHaptics(force: true)
    }

    private func setControllerRumbleStrength(_ multiplier: Double) {
        let next = NativeStreamControllerRumbleGain.normalize(multiplier)
        guard controllerRumbleStrength != next else { return }
        controllerRumbleStrength = next
        #if os(iOS)
        if let last = gameSirLastRumble {
            _ = gameSirRumbler.setLowFrequencyMotor(
                NativeStreamGameSirMotorPacket.amplitude(last.strong, gain: next),
                highFrequencyMotor: NativeStreamGameSirMotorPacket.amplitude(last.weak, gain: next))
        }
        #endif
        #if canImport(CoreHaptics)
        for playback in controllerHapticsBySlot.values {
            guard playback.isPlaying, let profile = playback.lastProfile else { continue }
            playback.lastUpdateAt = 0
            do { try updateHapticPlayback(playback, profile: profile) }
            catch { logHapticsFailure("Controller strength update failed: \(error.localizedDescription)") }
        }
        #endif
    }

    func setPhysicalControllerPassthrough(_ enabled: Bool) {
        guard physicalControllerPassthroughEnabled != enabled else { return }
        if !enabled {
            stopAllControllerRumble(shutdown: false)
        }
        physicalControllerPassthroughEnabled = enabled
        attachControllers()
        advertiseHaptics(force: true)
    }

    func setPhoneRumbleFallback(_ enabled: Bool) {
        guard phoneRumbleFallbackEnabled != enabled else { return }
        #if canImport(CoreHaptics)
        if !enabled {
            stopPhoneRumble(shutdown: true)
        }
        #endif
        phoneRumbleFallbackEnabled = enabled
        advertiseHaptics(force: true)
    }

    func setControllerShortcuts(_ shortcuts: NativeStreamControllerShortcuts) {
        guard controllerShortcuts != shortcuts else { return }
        controllerShortcuts = shortcuts
        refreshShortcutButtonIDs()
        sendCurrentGamepads(force: true)
    }

    func setControllerHUDActive(_ active: Bool) {
        guard controllerHUDActive != active else { return }
        controllerHUDActive = active
        hudDirection = nil
        if active {
            updateEmulatedMouseButton(0, pressed: false)
            updateEmulatedMouseButton(1, pressed: false)
            mouseAccumulator = .zero
            mouseEmulationScrollRemainder = 0
        }
        if !active {
            for (slot, controller) in controllersBySlot {
                for (_, button) in NativeStreamControllerButtonNames.buttons(controller) where button.value >= 0.1 {
                    blockedButtons.insert(ObjectIdentifier(button))
                }
                blockedAxesSlots.insert(slot)
            }
        }
        // Preserve the connected gamepad but release all controls while the HUD owns input.
        sendCurrentGamepads(force: true)
    }

    private func controllerButton(_ button: GCControllerButtonInput, name: String, pressed: Bool, slot: Int) {
        if !pressed { blockedButtons.remove(ObjectIdentifier(button)); return }
        guard !NativeStreamControllerShortcutCapture.learning, let controller = controllersBySlot[slot],
              let gamepad = controller.extendedGamepad else { return }
        if controllerHUDActive {
            if button === gamepad.buttonA { onControllerHUDCommand?(.activate); return }
            if button === gamepad.buttonB { onControllerHUDCommand?(.back); return }
        }
        let action = controllerShortcuts.action(for: name)
        guard action != .none else { return }
        if action == .controls { setControllerHUDActive(!controllerHUDActive) }
        NativeStreamRumbleDiagnostics.shared.record("controllerShortcut", details: [
            "shortcutInput": name, "shortcutAction": action.rawValue])
        onControllerShortcut?(action)
    }

    private func updateHUDDirection(slot: Int) {
        guard controllerHUDActive, !NativeStreamControllerShortcutCapture.learning,
              let gamepad = controllersBySlot[slot]?.extendedGamepad else { return }
        let dpad = NativeStreamControllerHUDRouting.direction(x: gamepad.dpad.xAxis.value, y: gamepad.dpad.yAxis.value)
        let next = dpad ?? NativeStreamControllerHUDRouting.direction(x: gamepad.leftThumbstick.xAxis.value, y: gamepad.leftThumbstick.yAxis.value)
        guard next != hudDirection else { return }
        hudDirection = next
        hudRepeatAt = ProcessInfo.processInfo.systemUptime + 0.35
        if let next { onControllerHUDCommand?(next) }
    }

    func attach() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCKeyboardDidConnect,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCKeyboardDidDisconnect,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCMouseDidConnect,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCMouseDidDisconnect,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCControllerDidConnect,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(refreshDevices),
            name: NSNotification.Name.GCControllerDidDisconnect,
            object: nil
        )
        refreshDevices()
        startTimers()
    }

    func detach() {
        NotificationCenter.default.removeObserver(self)
        keyboard?.keyboardInput?.keyChangedHandler = nil
        mice.forEach { mouse in
            guard let input = mouse.mouseInput else { return }
            input.mouseMovedHandler = nil
            input.leftButton.pressedChangedHandler = nil
            input.rightButton?.pressedChangedHandler = nil
            input.middleButton?.pressedChangedHandler = nil
            input.scroll.valueChangedHandler = nil
        }
        controllersBySlot.values.forEach {
            $0.extendedGamepad?.valueChangedHandler = nil
            NativeStreamControllerButtonNames.buttons($0).forEach { $0.1.pressedChangedHandler = nil }
        }
        keyboard = nil
        mice = []
        controllersBySlot.removeAll(keepingCapacity: true)
        controllerSlots.removeAll(keepingCapacity: true)
        lastGamepadStates.removeAll(keepingCapacity: true)
        virtualControllerEnabled = false
        resetVirtualControllerState()
        heartbeatTimer?.invalidate()
        heartbeatTimer = nil
        gamepadKeepaliveTimer?.invalidate()
        gamepadKeepaliveTimer = nil
        mouseEmulationTimer?.invalidate()
        mouseEmulationTimer = nil
        mouseEmulationEnabled = false
        mouseEmulationHeldButtons.removeAll()
        stopAllRumble()
        hudRepeatTimer?.invalidate()
        hudRepeatTimer = nil
        controllerHUDActive = false
        hudDirection = nil
        blockedButtons.removeAll()
        blockedAxesSlots.removeAll()
        shortcutButtonIDsBySlot.removeAll()
        lastHapticsAdvertisementAt = -.infinity
    }

    func sendTouchMouseMove(dx: CGFloat, dy: CGFloat) {
        // Preserve the established touch baseline while applying the same
        // sensitivity and acceleration curve as a physical relative mouse.
        coalesceMouse(dx: dx * 1.35, dy: dy * 1.35)
    }

    /// Sends one batch of finger updates. Reliable, because a dropped lift leaves a finger stuck
    /// down for the rest of the session and no later packet corrects it.
    @discardableResult
    func sendNativeTouch(_ records: [NativeTouchRecord]) -> Bool {
        guard let sink, let packet = encoder.encodeTouchBatch(records) else { return false }
        sink.sendReliableInput(packet)
        return true
    }

    func sendMouseButton(_ button: Int, pressed: Bool) {
        sink?.sendReliableInput(encoder.encodeMouseButton(button: button, pressed: pressed))
    }

    func sendKey(mapping: NativeStreamKeyboardMapping, pressed: Bool, modifiers: UInt16) {
        let data = pressed
            ? encoder.encodeKeyDown(mapping: mapping, modifiers: modifiers)
            : encoder.encodeKeyUp(mapping: mapping, modifiers: modifiers)
        sink?.sendReliableInput(data)
    }

    /// Select all, then delete. This mirrors the Android stream keyboard's Clear action and uses
    /// ordinary key packets so it also works in fields that do not accept Unicode injection.
    func clearRemoteText() {
        let selectAll = NativeStreamKeyboardMapping(virtualKey: 0x41, scanCode: 0x001e)
        let backspace = NativeStreamKeyboardMapping(virtualKey: 0x08, scanCode: 0x000e)
        sendKey(mapping: selectAll, pressed: true, modifiers: 0x02)
        sendKey(mapping: selectAll, pressed: false, modifiers: 0x02)
        sendKey(mapping: backspace, pressed: true, modifiers: 0)
        sendKey(mapping: backspace, pressed: false, modifiers: 0)
    }

    func sendMouseWheel(delta: Int) {
        sink?.sendReliableInput(encoder.encodeMouseWheel(delta: delta))
    }

    @discardableResult
    func sendUnicodeText(_ text: String) -> Int {
        let batch = encoder.encodeUnicodeText(text)
        for packet in batch.packets {
            sink?.sendReliableInput(packet)
        }
        return batch.characterCount
    }

    func advertiseHaptics(
        force: Bool = false,
        now: TimeInterval = ProcessInfo.processInfo.systemUptime
    ) {
        guard force || now - lastHapticsAdvertisementAt >= 5 else { return }
        let controllerAvailable = physicalControllerPassthroughEnabled && controllersBySlot.values.contains {
            #if os(iOS)
            if NativeStreamGameSirRumbler.isTargetController($0) { return gameSirRumbler.canHandleController($0) }
            #endif
            return $0.haptics != nil
        }
        #if canImport(CoreHaptics)
        let phoneAvailable = phoneRumbleFallbackEnabled && Self.phoneHapticsSupported
        #else
        let phoneAvailable = false
        #endif
        NativeStreamRumbleDiagnostics.shared.record("enableRequested", details: [
            "controllerAvailable": String(controllerAvailable), "phoneFallbackAvailable": String(phoneAvailable),
            "registeredSlots": controllersBySlot.keys.sorted().map(String.init).joined(separator: ",")])
        sink?.sendReliableInput(encoder.encodeHapticsEnabled(controllerAvailable || phoneAvailable))
        if force {
            sink?.logInputEvent(
                "Haptics advertised: controller=\(controllerAvailable) phoneFallback=\(phoneAvailable)"
            )
        }
        lastHapticsAdvertisementAt = now
    }

    func primeReliableChannel() {
        sink?.sendReliableInput(encoder.encodeHeartbeat())
        advertiseHaptics(force: true)
    }

    @discardableResult
    func handleServerHandshake(_ data: Data) -> Int? {
        let bytes = [UInt8](data)
        guard let firstByte = bytes.first else { return nil }
        let firstWord = bytes.count >= 2
            ? Int(bytes[0]) | (Int(bytes[1]) << 8)
            : Int(firstByte)
        let version: Int
        if firstWord == 526 {
            version = bytes.count >= 4
                ? Int(bytes[2]) | (Int(bytes[3]) << 8)
                : NativeStreamSDP.defaultInputProtocolVersion
        } else if firstByte == 0x0e {
            version = firstWord
        } else {
            return nil
        }

        let normalizedVersion = max(version, 1)
        encoder.setProtocolVersion(normalizedVersion)
        encoder.resetGamepadSequences()
        primeReliableChannel()
        NSLog("[OpenNOW] input handshake protocol=%d bytes=%d", normalizedVersion, bytes.count)
        return normalizedVersion
    }

    func applyRumble(controllerId: Int, weakMagnitude: Int, strongMagnitude: Int) {
        #if canImport(CoreHaptics)
        guard Thread.isMainThread else {
            DispatchQueue.main.async { [weak self] in
                self?.applyRumble(
                    controllerId: controllerId,
                    weakMagnitude: weakMagnitude,
                    strongMagnitude: strongMagnitude
                )
            }
            return
        }

        NativeStreamRumbleDiagnostics.shared.record("motorEvent", details: [
            "requestedSlot": String(controllerId), "weakMagnitude": String(weakMagnitude),
            "strongMagnitude": String(strongMagnitude), "passthrough": String(physicalControllerPassthroughEnabled),
            "matchedController": controllersBySlot[controllerId]?.vendorName ?? "none",
            "registeredSlots": controllersBySlot.keys.sorted().map(String.init).joined(separator: ",")])
        let profile = NativeStreamRumbleProfile(
            weakMagnitude: weakMagnitude,
            strongMagnitude: strongMagnitude
        )
        if !profile.isStopped {
            NativeStreamRumbleDiagnostics.shared.record("activeMotorEvent", details: [
                "lastActiveWeakMagnitude": String(weakMagnitude), "lastActiveStrongMagnitude": String(strongMagnitude),
                "lastActiveIntensity": String(profile.intensity), "controllerStartup": "unmuted-preconfigured"])
        }
        if profile.isStopped {
            stopControllerRumble(slot: controllerId, shutdown: false)
            stopPhoneRumble(shutdown: false)
            return
        }

        #if os(iOS)
        if physicalControllerPassthroughEnabled,
           let controller = controllersBySlot[controllerId],
           NativeStreamGameSirRumbler.isTargetController(controller) {
            gameSirLastRumble = (controllerId, weakMagnitude, strongMagnitude)
            let low = NativeStreamGameSirMotorPacket.amplitude(strongMagnitude, gain: controllerRumbleStrength)
            let high = NativeStreamGameSirMotorPacket.amplitude(weakMagnitude, gain: controllerRumbleStrength)
            if gameSirRumbler.canHandleController(controller),
               gameSirRumbler.setLowFrequencyMotor(low, highFrequencyMotor: high) {
                NativeStreamRumbleDiagnostics.shared.record("gameSirRumbleQueued", details: [
                    "controllerTransport": "external-accessory", "controllerMultiplier": String(controllerRumbleStrength),
                    "gameSirRequestedLow": String(low), "gameSirRequestedHigh": String(high)])
                stopPhoneRumble(shutdown: true)
                return
            }
            // This model's Core Haptics outputs were observed to vibrate the phone.
            // Use only the explicit accessory transport or the user's phone fallback.
            if phoneRumbleFallbackEnabled, playPhoneRumble(profile) { return }
            logHapticsFailure("GameSir motor accessory session unavailable; phone fallback is off.")
            return
        }
        #endif

        if physicalControllerPassthroughEnabled,
           let controller = controllersBySlot[controllerId],
           controller.haptics != nil,
           playControllerRumble(profile, controller: controller, slot: controllerId) {
            NativeStreamRumbleDiagnostics.shared.record("controllerPlaybackAccepted")
            stopPhoneRumble(shutdown: false)
            return
        }

        if phoneRumbleFallbackEnabled, playPhoneRumble(profile) {
            NativeStreamRumbleDiagnostics.shared.record("phoneFallbackPlaybackAccepted")
            return
        }
        logHapticsFailure("No haptic output available for controller slot \(controllerId)")
        #endif
    }

    func setVirtualControllerEnabled(_ enabled: Bool) {
        guard virtualControllerEnabled != enabled else { return }
        if !enabled {
            let wasMergedWithPhysical = primaryPhysicalControllerSlot != nil
            if !wasMergedWithPhysical {
                let disconnected = virtualGamepadState(connected: false)
                sendGamepadState(disconnected, force: true)
                lastGamepadStates.removeValue(forKey: disconnected.controllerId)
            }
            virtualControllerEnabled = false
            resetVirtualControllerState()
            if let slot = primaryPhysicalControllerSlot {
                sendGamepad(slot: slot, force: true)
            }
            return
        }
        virtualControllerEnabled = enabled
        resetVirtualControllerState()
        if let slot = primaryPhysicalControllerSlot {
            sendGamepad(slot: slot, force: true)
        } else {
            sendGamepadState(virtualGamepadState(connected: true), force: true)
        }
    }

    func setVirtualButton(_ button: NativeStreamVirtualGamepadButton, pressed: Bool) {
        guard virtualControllerEnabled else { return }
        if pressed {
            virtualButtons |= button.rawValue
        } else {
            virtualButtons &= ~button.rawValue
        }
        sendCurrentVirtualGamepadState()
    }

    func setVirtualStick(_ stick: NativeStreamVirtualGamepadStick, x: CGFloat, y: CGFloat) {
        guard virtualControllerEnabled else { return }
        let xValue = int16Axis(Float(min(max(x, -1), 1)))
        let yValue = int16Axis(Float(min(max(y, -1), 1)))
        switch stick {
        case .left:
            virtualLeftStickX = xValue
            virtualLeftStickY = yValue
            virtualLeftStickActive = xValue != 0 || yValue != 0
        case .right:
            virtualRightStickX = xValue
            virtualRightStickY = yValue
            virtualRightStickActive = xValue != 0 || yValue != 0
        }
        sendCurrentVirtualGamepadState()
    }

    func setVirtualTrigger(_ trigger: NativeStreamVirtualGamepadTrigger, value: CGFloat) {
        guard virtualControllerEnabled else { return }
        let scaled = UInt8(min(max(Int((value * 255).rounded()), 0), 255))
        switch trigger {
        case .left: virtualLeftTrigger = scaled
        case .right: virtualRightTrigger = scaled
        }
        sendCurrentVirtualGamepadState()
    }

    @objc private func refreshDevices() {
        let hadKeyboardOrMouse = keyboard != nil || !mice.isEmpty
        attachKeyboard()
        attachMice()
        let hasKeyboardOrMouse = keyboard != nil || !mice.isEmpty
        if hadKeyboardOrMouse != hasKeyboardOrMouse {
            onPhysicalKeyboardMouseAvailabilityChanged?(hasKeyboardOrMouse)
        }
        attachControllers()
        advertiseHaptics(force: true)
    }

    private func startTimers() {
        hudRepeatTimer?.invalidate()
        hudRepeatTimer = Timer.scheduledTimer(withTimeInterval: 0.10, repeats: true) { [weak self] _ in
            guard let self, self.controllerHUDActive, !NativeStreamControllerShortcutCapture.learning,
                  let direction = self.hudDirection else { return }
            let now = ProcessInfo.processInfo.systemUptime
            guard now >= self.hudRepeatAt else { return }
            self.hudRepeatAt = now + 0.12
            self.onControllerHUDCommand?(direction)
        }
        heartbeatTimer?.invalidate()
        heartbeatTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.sink?.sendReliableInput(self.encoder.encodeHeartbeat())
            self.advertiseHaptics()
        }
        gamepadKeepaliveTimer?.invalidate()
        gamepadKeepaliveTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            self?.sendCurrentGamepads(force: true)
        }
    }

    private func attachKeyboard() {
        let nextKeyboard = GCKeyboard.coalesced
        guard nextKeyboard !== keyboard else { return }
        keyboard?.keyboardInput?.keyChangedHandler = nil
        keyboard = nextKeyboard
        keyboard?.keyboardInput?.keyChangedHandler = { [weak self] keyboardInput, _, keyCode, pressed in
            guard let self, let mapping = NativeStreamKeyboardMapper.mapping(for: keyCode) else { return }
            let modifiers = NativeStreamKeyboardMapper.modifiers(
                for: keyboardInput,
                changedKey: keyCode,
                pressed: pressed
            )
            let data = pressed
                ? self.encoder.encodeKeyDown(mapping: mapping, modifiers: modifiers)
                : self.encoder.encodeKeyUp(mapping: mapping, modifiers: modifiers)
            self.sink?.sendReliableInput(data)
        }
    }

    private func attachMice() {
        mice.forEach { mouse in
            guard let input = mouse.mouseInput else { return }
            input.mouseMovedHandler = nil
            input.leftButton.pressedChangedHandler = nil
            input.rightButton?.pressedChangedHandler = nil
            input.middleButton?.pressedChangedHandler = nil
            input.scroll.valueChangedHandler = nil
        }
        mice = GCMouse.mice()
        for mouse in mice {
            guard let input = mouse.mouseInput else { continue }
            input.mouseMovedHandler = { [weak self] _, deltaX, deltaY in
                let delta = NativeStreamMouseCoordinatePolicy.hostDelta(
                    deltaX: deltaX,
                    deltaY: deltaY
                )
                self?.coalesceMouse(dx: delta.x, dy: delta.y)
            }
            input.leftButton.pressedChangedHandler = { [weak self] _, _, pressed in
                self?.sendMouseButton(1, pressed: pressed)
            }
            input.middleButton?.pressedChangedHandler = { [weak self] _, _, pressed in
                self?.sendMouseButton(2, pressed: pressed)
            }
            input.rightButton?.pressedChangedHandler = { [weak self] _, _, pressed in
                self?.sendMouseButton(3, pressed: pressed)
            }
            input.scroll.valueChangedHandler = { [weak self] _, _, yValue in
                let delta = Int((CGFloat(yValue) * 120).rounded())
                guard delta != 0 else { return }
                self?.sendMouseWheel(delta: delta)
            }
            input.auxiliaryButtons?.enumerated().forEach { offset, button in
                button.pressedChangedHandler = { [weak self] _, _, pressed in
                    self?.sendMouseButton(4 + offset, pressed: pressed)
                }
            }
        }
    }

    /// Turns a physical controller into a pointer.
    ///
    /// Games that only accept mouse and keyboard are otherwise unplayable from a couch, and GFN
    /// offers no server-side equivalent. Left stick drives the cursor, right stick scrolls, and
    /// A / B become the two mouse buttons — the mapping Android uses, so muscle memory carries
    /// across.
    ///
    /// While it is on, those inputs are *withheld* from the gamepad state the host receives.
    /// Sending both would make the game see a stick push and a cursor move at once, which is how
    /// a menu ends up scrolling twice per press.
    func setControllerMouseEmulation(_ enabled: Bool) {
        guard mouseEmulationEnabled != enabled else { return }
        mouseEmulationEnabled = enabled
        mouseEmulationTimer?.invalidate()
        mouseEmulationTimer = nil
        mouseEmulationScrollRemainder = 0

        // Release anything held, or the host keeps a button down forever after a mode switch.
        for button in mouseEmulationHeldButtons {
            sendMouseButton(button, pressed: false)
        }
        mouseEmulationHeldButtons.removeAll()

        guard enabled else {
            // Re-send the true controller state so the host stops seeing a centred stick.
            controllersBySlot.keys.forEach { sendGamepad(slot: $0, force: true) }
            return
        }

        mouseEmulationTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            self?.stepControllerMouseEmulation()
        }
    }

    var isControllerMouseEmulationEnabled: Bool { mouseEmulationEnabled }

    private func stepControllerMouseEmulation() {
        guard mouseEmulationEnabled, !controllerHUDActive,
              let slot = controllersBySlot.keys.min(), !blockedAxesSlots.contains(slot),
              let gamepad = controllersBySlot[slot]?.extendedGamepad else { return }

        // A resting stick is never exactly zero, and at 60 Hz even a tiny bias walks the cursor
        // across the screen in a few seconds.
        let deadZone: Float = 0.12
        let x = Self.curvedStickAxis(gamepad.leftThumbstick.xAxis.value, deadZone: deadZone)
        let y = Self.curvedStickAxis(gamepad.leftThumbstick.yAxis.value, deadZone: deadZone)
        if x != 0 || y != 0 {
            // 18 px per frame at full deflection lands close to a comfortable trackpad flick
            // once the shared sensitivity curve is applied on top.
            let speed: CGFloat = 18
            coalesceMouse(dx: CGFloat(x) * speed, dy: CGFloat(-y) * speed)
        }

        let scrollAxis = Self.curvedStickAxis(gamepad.rightThumbstick.yAxis.value, deadZone: deadZone)
        if scrollAxis != 0 {
            // Higher sensitivity numbers mean slower scrolling, matching the settings copy.
            mouseEmulationScrollRemainder += CGFloat(scrollAxis) * (120 / mouseScrollSensitivity)
            let steps = Int(mouseEmulationScrollRemainder)
            if steps != 0 {
                mouseEmulationScrollRemainder -= CGFloat(steps)
                sendMouseWheel(delta: steps)
            }
        } else {
            mouseEmulationScrollRemainder = 0
        }

        let mapped = shortcutButtonIDsBySlot[slot] ?? []
        func mousePressed(_ button: GCControllerButtonInput) -> Bool {
            let id = ObjectIdentifier(button)
            return button.isPressed && !blockedButtons.contains(id) && !mapped.contains(id)
        }
        updateEmulatedMouseButton(0, pressed: mousePressed(gamepad.buttonA))
        updateEmulatedMouseButton(1, pressed: mousePressed(gamepad.buttonB))
    }

    private func updateEmulatedMouseButton(_ button: Int, pressed: Bool) {
        let held = mouseEmulationHeldButtons.contains(button)
        guard held != pressed else { return }
        if pressed {
            mouseEmulationHeldButtons.insert(button)
        } else {
            mouseEmulationHeldButtons.remove(button)
        }
        sendMouseButton(button, pressed: pressed)
    }

    /// Squares the deflection past the dead zone: fine aiming near centre, full speed at the rim.
    static func curvedStickAxis(_ value: Float, deadZone: Float) -> Float {
        let magnitude = abs(value)
        guard magnitude > deadZone else { return 0 }
        let scaled = (magnitude - deadZone) / (1 - deadZone)
        return (value < 0 ? -1 : 1) * scaled * scaled
    }

    private func coalesceMouse(dx: CGFloat, dy: CGFloat) {
        let adjusted = adjustedMouseDelta(dx: dx, dy: dy)
        mouseAccumulator.x += adjusted.x
        mouseAccumulator.y += adjusted.y
        guard !mouseFlushScheduled else { return }
        mouseFlushScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.006) { [weak self] in
            guard let self else { return }
            let dx = Int(self.mouseAccumulator.x.rounded())
            let dy = Int(self.mouseAccumulator.y.rounded())
            self.mouseAccumulator = .zero
            self.mouseFlushScheduled = false
            guard dx != 0 || dy != 0 else { return }
            self.sink?.sendPartiallyReliableInput(self.encoder.encodeMouseMove(dx: dx, dy: dy))
        }
    }

    private func adjustedMouseDelta(dx: CGFloat, dy: CGFloat) -> CGPoint {
        var adjustedX = dx * mouseSensitivity
        var adjustedY = dy * mouseSensitivity
        guard mouseAccelerationLevel > 0 else {
            return CGPoint(x: adjustedX, y: adjustedY)
        }
        let speed = hypot(adjustedX, adjustedY)
        let strength = CGFloat(mouseAccelerationLevel) / 2
        // Match Android Native's gentle curve: preserve low-speed precision and
        // cap the high-speed turn boost at 60 percent.
        let acceleration = 1 + min(0.6 * strength, (speed / 50) * strength)
        adjustedX *= acceleration
        adjustedY *= acceleration
        return CGPoint(x: adjustedX, y: adjustedY)
    }

    private func refreshShortcutButtonIDs() {
        shortcutButtonIDsBySlot = controllersBySlot.mapValues { controller in
            Set(NativeStreamControllerButtonNames.buttons(controller).filter {
                controllerShortcuts.action(for: $0.0) != .none
            }.map { ObjectIdentifier($0.1) })
        }
    }

    private func attachControllers() {
        controllersBySlot.values.forEach {
            $0.extendedGamepad?.valueChangedHandler = nil
            NativeStreamControllerButtonNames.buttons($0).forEach { $0.1.pressedChangedHandler = nil }
        }

        let connectedControllers = GCController.controllers().filter { $0.extendedGamepad != nil }
        let connectedIdentifiers = Set(connectedControllers.map { ObjectIdentifier($0) })
        let disconnectedIdentifiers = controllerSlots.keys.filter { !connectedIdentifiers.contains($0) }
        for identifier in disconnectedIdentifiers {
            guard let slot = controllerSlots.removeValue(forKey: identifier) else { continue }
            _ = controllersBySlot.removeValue(forKey: slot)
            stopControllerRumble(slot: slot, shutdown: true)
            if physicalControllerPassthroughEnabled {
                disconnectGamepad(slot: slot)
            }
        }

        for controller in connectedControllers {
            let identifier = ObjectIdentifier(controller)
            if let slot = controllerSlots[identifier] {
                controllersBySlot[slot] = controller
                continue
            }
            let usedSlots = Set(controllerSlots.values)
            guard let slot = (0..<4).first(where: { !usedSlots.contains($0) }) else { continue }
            controllerSlots[identifier] = slot
            controllersBySlot[slot] = controller
        }

        refreshShortcutButtonIDs()
        blockedAxesSlots.formIntersection(Set(controllersBySlot.keys))
        let hasActivePhysicalController = physicalControllerPassthroughEnabled && !controllersBySlot.isEmpty
        onPhysicalControllerAvailabilityChanged?(hasActivePhysicalController)
        for (slot, controller) in controllersBySlot {
            controller.handlerQueue = .main
            let buttons = NativeStreamControllerButtonNames.buttons(controller)
            NativeStreamRumbleDiagnostics.shared.record("controllerButtonsAvailable", details: [
                "controllerButtons": buttons.map { $0.0 }.joined(separator: ", ")])
            for (name, button) in buttons {
                button.pressedChangedHandler = { [weak self] button, _, pressed in
                    self?.controllerButton(button, name: name, pressed: pressed, slot: slot)
                }
            }
            controller.extendedGamepad?.valueChangedHandler = { [weak self] _, _ in
                guard let self else { return }
                self.updateHUDDirection(slot: slot)
                if self.physicalControllerPassthroughEnabled { self.sendGamepad(slot: slot, force: false) }
            }
        }
        sendCurrentGamepads(force: true)
    }

    private func sendCurrentGamepads(force: Bool) {
        if physicalControllerPassthroughEnabled {
            for slot in controllersBySlot.keys.sorted() {
                sendGamepad(slot: slot, force: force)
            }
        }
        if virtualControllerEnabled, primaryPhysicalControllerSlot == nil {
            sendGamepadState(virtualGamepadState(connected: true), force: force)
        }
        let activeControllerIDs: Set<Int> = {
            var ids = physicalControllerPassthroughEnabled ? Set(controllersBySlot.keys) : []
            if virtualControllerEnabled, primaryPhysicalControllerSlot == nil { ids.insert(0) }
            return ids
        }()
        let disconnectedIDs = lastGamepadStates.compactMap { id, state in
            state.connected && !activeControllerIDs.contains(id) ? id : nil
        }
        if !disconnectedIDs.isEmpty {
            for id in disconnectedIDs {
                disconnectGamepad(slot: id)
            }
        }
    }

    private func sendGamepad(slot: Int, force: Bool) {
        guard let gamepad = controllersBySlot[slot]?.extendedGamepad else { return }
        let mapped = shortcutButtonIDsBySlot[slot] ?? []
        func allowed(_ button: GCControllerButtonInput) -> Bool {
            let id = ObjectIdentifier(button)
            if button.value < 0.1 { blockedButtons.remove(id) }
            return !blockedButtons.contains(id) && !mapped.contains(id)
        }
        func pressed(_ button: GCControllerButtonInput?) -> Bool {
            guard let button else { return false }
            return button.isPressed && allowed(button)
        }
        if blockedAxesSlots.contains(slot),
           max(max(abs(gamepad.leftThumbstick.xAxis.value), abs(gamepad.leftThumbstick.yAxis.value)),
               max(abs(gamepad.rightThumbstick.xAxis.value), abs(gamepad.rightThumbstick.yAxis.value))) < 0.2 {
            blockedAxesSlots.remove(slot)
        }
        let axesBlocked = controllerHUDActive || blockedAxesSlots.contains(slot)
        // While the sticks are driving a cursor, the host must not also see them as sticks.
        let emulatingMouse = mouseEmulationEnabled && slot == controllersBySlot.keys.min()
        var buttons: UInt16 = 0
        if pressed(gamepad.dpad.up) { buttons |= 0x0001 }
        if pressed(gamepad.dpad.down) { buttons |= 0x0002 }
        if pressed(gamepad.dpad.left) { buttons |= 0x0004 }
        if pressed(gamepad.dpad.right) { buttons |= 0x0008 }
        if pressed(gamepad.buttonMenu) { buttons |= 0x0010 }
        if pressed(gamepad.buttonOptions) { buttons |= 0x0020 }
        if pressed(gamepad.leftThumbstickButton) { buttons |= 0x0040 }
        if pressed(gamepad.rightThumbstickButton) { buttons |= 0x0080 }
        if pressed(gamepad.leftShoulder) { buttons |= 0x0100 }
        if pressed(gamepad.rightShoulder) { buttons |= 0x0200 }
        if pressed(gamepad.buttonHome) { buttons |= 0x0400 }
        if pressed(gamepad.buttonA) && !emulatingMouse { buttons |= 0x1000 }
        if pressed(gamepad.buttonB) && !emulatingMouse { buttons |= 0x2000 }
        if pressed(gamepad.buttonX) { buttons |= 0x4000 }
        if pressed(gamepad.buttonY) { buttons |= 0x8000 }

        let physicalState = NativeStreamGamepadState(
            controllerId: slot,
            buttons: buttons,
            leftTrigger: allowed(gamepad.leftTrigger) ? uint8(gamepad.leftTrigger.value) : 0,
            rightTrigger: allowed(gamepad.rightTrigger) ? uint8(gamepad.rightTrigger.value) : 0,
            leftStickX: (emulatingMouse || axesBlocked) ? 0 : int16Axis(gamepad.leftThumbstick.xAxis.value),
            leftStickY: (emulatingMouse || axesBlocked) ? 0 : int16Axis(gamepad.leftThumbstick.yAxis.value),
            rightStickX: (emulatingMouse || axesBlocked) ? 0 : int16Axis(gamepad.rightThumbstick.xAxis.value),
            rightStickY: (emulatingMouse || axesBlocked) ? 0 : int16Axis(gamepad.rightThumbstick.yAxis.value),
            connected: true
        )
        let state = virtualControllerEnabled && slot == primaryPhysicalControllerSlot
            ? NativeStreamGamepadMixer.merging(physical: physicalState, virtual: virtualGamepadInputState)
            : physicalState
        sendGamepadState(state, force: force)
    }

    private func sendCurrentVirtualGamepadState() {
        guard virtualControllerEnabled else { return }
        if let slot = primaryPhysicalControllerSlot {
            sendGamepad(slot: slot, force: false)
        } else {
            sendGamepadState(virtualGamepadState(connected: true), force: false)
        }
    }

    private var primaryPhysicalControllerSlot: Int? {
        guard physicalControllerPassthroughEnabled else { return nil }
        return controllersBySlot.keys.min()
    }

    private func disconnectGamepad(slot: Int) {
        guard lastGamepadStates[slot]?.connected == true else {
            lastGamepadStates.removeValue(forKey: slot)
            return
        }
        let disconnected = NativeStreamGamepadState(
            controllerId: slot,
            buttons: 0,
            leftTrigger: 0,
            rightTrigger: 0,
            leftStickX: 0,
            leftStickY: 0,
            rightStickX: 0,
            rightStickY: 0,
            connected: false
        )
        sendGamepadState(disconnected, force: true)
        lastGamepadStates.removeValue(forKey: slot)
    }

    private func sendGamepadState(_ incoming: NativeStreamGamepadState, force: Bool) {
        let state = NativeStreamControllerHUDRouting.gameState(incoming, captured: controllerHUDActive)
        guard force || lastGamepadStates[state.controllerId] != state else { return }
        lastGamepadStates[state.controllerId] = state
        var bitmap: UInt16 = 0
        for (id, connectedState) in lastGamepadStates where connectedState.connected && id >= 0 && id < 4 {
            bitmap |= UInt16(1 << id)
            bitmap |= UInt16(1 << (id + 8))
        }
        let usePartiallyReliable = (partiallyReliableGamepadMask & UInt16(1 << (state.controllerId & 0x03))) != 0
        let data = encoder.encodeGamepadState(state, bitmap: bitmap, partiallyReliable: usePartiallyReliable)
        if usePartiallyReliable {
            sink?.sendPartiallyReliableInput(data)
        } else {
            sink?.sendReliableInput(data)
        }
    }

    private func virtualGamepadState(connected: Bool) -> NativeStreamGamepadState {
        NativeStreamGamepadState(
            controllerId: 0,
            buttons: virtualButtons,
            leftTrigger: virtualLeftTrigger,
            rightTrigger: virtualRightTrigger,
            leftStickX: virtualLeftStickX,
            leftStickY: virtualLeftStickY,
            rightStickX: virtualRightStickX,
            rightStickY: virtualRightStickY,
            connected: connected
        )
    }

    private var virtualGamepadInputState: NativeStreamVirtualGamepadState {
        NativeStreamVirtualGamepadState(
            buttons: virtualButtons,
            leftTrigger: virtualLeftTrigger,
            rightTrigger: virtualRightTrigger,
            leftStickX: virtualLeftStickX,
            leftStickY: virtualLeftStickY,
            rightStickX: virtualRightStickX,
            rightStickY: virtualRightStickY,
            leftStickActive: virtualLeftStickActive,
            rightStickActive: virtualRightStickActive
        )
    }

    private func resetVirtualControllerState() {
        virtualButtons = 0
        virtualLeftTrigger = 0
        virtualRightTrigger = 0
        virtualLeftStickX = 0
        virtualLeftStickY = 0
        virtualRightStickX = 0
        virtualRightStickY = 0
        virtualLeftStickActive = false
        virtualRightStickActive = false
    }

    private func uint8(_ value: Float) -> UInt8 {
        UInt8(min(max(Int((value * 255).rounded()), 0), 255))
    }

    private func int16Axis(_ value: Float) -> Int16 {
        let clamped = min(max(value, -1), 1)
        let scaled = Int((clamped >= 0 ? clamped * 32767 : clamped * 32768).rounded())
        return Int16(min(max(scaled, Int(Int16.min)), Int(Int16.max)))
    }

    #if canImport(CoreHaptics)
    private func playControllerRumble(
        _ profile: NativeStreamRumbleProfile,
        controller: GCController,
        slot: Int
    ) -> Bool {
        let identifier = ObjectIdentifier(controller)
        do {
            let playback: NativeStreamHapticPlayback
            if let existing = controllerHapticsBySlot[slot],
               existing.controllerIdentifier == identifier {
                playback = existing
            } else {
                stopControllerRumble(slot: slot, shutdown: true)
                guard let haptics = controller.haptics else {
                    throw NSError(domain: "OpenNOW.NativeStreamer.Haptics", code: 1)
                }
                guard let engine = haptics.createEngine(withLocality: .default) else {
                    throw NSError(domain: "OpenNOW.NativeStreamer.Haptics", code: 2)
                }
                engine.playsHapticsOnly = true
                engine.isAutoShutdownEnabled = false
                playback = try NativeStreamHapticPlayback(
                    engine: engine,
                    controllerIdentifier: identifier
                )
                installControllerHapticCallbacks(playback, slot: slot)
                controllerHapticsBySlot[slot] = playback
            }
            NativeStreamRumbleDiagnostics.shared.record("controllerGainApplied", details: [
                "controllerMultiplier": String(controllerRumbleStrength),
                "controllerOutputIntensity": String(NativeStreamControllerRumbleGain.apply(profile.intensity, multiplier: controllerRumbleStrength))])
            try updateHapticPlayback(playback, profile: profile)
            phoneHapticsRetryAfter = 0
            return true
        } catch {
            stopControllerRumble(slot: slot, shutdown: true)
            logHapticsFailure("Controller rumble failed for slot \(slot): \(error.localizedDescription)")
            return false
        }
    }

    private func playPhoneRumble(_ profile: NativeStreamRumbleProfile) -> Bool {
        guard Self.phoneHapticsSupported else { return false }
        let now = ProcessInfo.processInfo.systemUptime
        guard now >= phoneHapticsRetryAfter else { return false }
        do {
            let playback: NativeStreamHapticPlayback
            if let existing = phoneHapticPlayback {
                playback = existing
            } else {
                let engine = try CHHapticEngine()
                engine.playsHapticsOnly = true
                engine.isAutoShutdownEnabled = false
                playback = try NativeStreamHapticPlayback(engine: engine, controllerIdentifier: nil)
                installPhoneHapticCallbacks(playback)
                phoneHapticPlayback = playback
            }
            try updateHapticPlayback(playback, profile: profile)
            return true
        } catch {
            stopPhoneRumble(shutdown: true)
            phoneHapticsRetryAfter = now + 5
            logHapticsFailure("Phone rumble failed: \(error.localizedDescription)")
            return playLegacyPhoneRumble(profile)
        }
    }

    private func updateHapticPlayback(
        _ playback: NativeStreamHapticPlayback,
        profile: NativeStreamRumbleProfile
    ) throws {
        let now = ProcessInfo.processInfo.systemUptime
        if playback.isPlaying,
           now - playback.lastUpdateAt < Self.hapticUpdateInterval,
           let previous = playback.lastProfile,
           !profile.materiallyDiffers(from: previous) {
            return
        }

        let parameters = [
            CHHapticDynamicParameter(
                parameterID: .hapticIntensityControl,
                value: playback.controllerIdentifier == nil ? profile.intensity
                    : NativeStreamControllerRumbleGain.apply(profile.intensity, multiplier: controllerRumbleStrength),
                relativeTime: 0
            ),
            CHHapticDynamicParameter(
                parameterID: .hapticSharpnessControl,
                value: profile.sharpnessControl,
                relativeTime: 0
            )
        ]
        if playback.isPlaying {
            try playback.player.sendParameters(parameters, atTime: CHHapticTimeImmediate)
        } else {
            try playback.engine.start()
            if playback.controllerIdentifier != nil {
                // Configure gain before starting; avoid a muted startup on controller engines.
                try playback.player.sendParameters(parameters, atTime: CHHapticTimeImmediate)
                try playback.player.start(atTime: CHHapticTimeImmediate)
            } else {
                playback.player.isMuted = true
                try playback.player.start(atTime: CHHapticTimeImmediate)
                try playback.player.sendParameters(parameters, atTime: CHHapticTimeImmediate)
                playback.player.isMuted = false
            }
            playback.isPlaying = true
        }
        playback.lastProfile = profile
        playback.lastUpdateAt = now
    }

    private func installControllerHapticCallbacks(_ playback: NativeStreamHapticPlayback, slot: Int) {
        let invalidate = { [weak self, weak playback] in
            DispatchQueue.main.async {
                guard let self, let playback,
                      self.controllerHapticsBySlot[slot] === playback else { return }
                playback.markEngineStopped()
                self.controllerHapticsBySlot.removeValue(forKey: slot)
            }
        }
        playback.engine.stoppedHandler = { _ in invalidate() }
        playback.engine.resetHandler = { invalidate() }
    }

    private func installPhoneHapticCallbacks(_ playback: NativeStreamHapticPlayback) {
        let invalidate = { [weak self, weak playback] in
            DispatchQueue.main.async {
                guard let self, let playback, self.phoneHapticPlayback === playback else { return }
                playback.markEngineStopped()
                self.phoneHapticPlayback = nil
            }
        }
        playback.engine.stoppedHandler = { _ in invalidate() }
        playback.engine.resetHandler = { invalidate() }
    }
    #endif

    private func stopControllerRumble(slot: Int, shutdown: Bool) {
        #if os(iOS)
        if gameSirLastRumble?.slot == slot {
            gameSirLastRumble = nil
            if shutdown { gameSirRumbler.stopAndClose() }
            else { _ = gameSirRumbler.setLowFrequencyMotor(0, highFrequencyMotor: 0) }
        }
        #endif
        #if canImport(CoreHaptics)
        guard let playback = controllerHapticsBySlot[slot] else { return }
        if shutdown {
            controllerHapticsBySlot.removeValue(forKey: slot)
            playback.shutdown()
        } else {
            playback.stopPlayer()
        }
        #endif
    }

    private func stopAllControllerRumble(shutdown: Bool) {
        #if os(iOS)
        gameSirLastRumble = nil
        gameSirRumbler.stopAndClose()
        #endif
        #if canImport(CoreHaptics)
        if shutdown {
            let playbacks = Array(controllerHapticsBySlot.values)
            controllerHapticsBySlot.removeAll(keepingCapacity: true)
            playbacks.forEach { $0.shutdown() }
        } else {
            controllerHapticsBySlot.values.forEach { $0.stopPlayer() }
        }
        #endif
    }

    private func stopPhoneRumble(shutdown: Bool) {
        #if canImport(CoreHaptics)
        guard let playback = phoneHapticPlayback else { return }
        if shutdown {
            phoneHapticPlayback = nil
            playback.shutdown()
        } else {
            playback.stopPlayer()
        }
        #endif
    }

    private func stopAllRumble() {
        stopAllControllerRumble(shutdown: true)
        stopPhoneRumble(shutdown: true)
    }

    #if canImport(CoreHaptics)
    private func logHapticsFailure(_ message: String) {
        NativeStreamRumbleDiagnostics.shared.record("playbackError", details: ["error": message])
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastHapticsFailureLogAt >= 5 else { return }
        lastHapticsFailureLogAt = now
        sink?.logInputEvent(message)
    }

    private func playLegacyPhoneRumble(_ profile: NativeStreamRumbleProfile) -> Bool {
        #if canImport(UIKit)
        let style: UIImpactFeedbackGenerator.FeedbackStyle = profile.sharpnessControl < 0 ? .heavy : .medium
        let generator = UIImpactFeedbackGenerator(style: style)
        generator.prepare()
        generator.impactOccurred(intensity: CGFloat(profile.intensity))
        return true
        #else
        return false
        #endif
    }
    #endif
}
#else
final class NativeStreamInputBridge {
    weak var sink: NativeStreamInputSink?
    var onPhysicalControllerAvailabilityChanged: ((Bool) -> Void)?
    var onPhysicalKeyboardMouseAvailabilityChanged: ((Bool) -> Void)?
    func configure(protocolVersion: Int, partiallyReliableGamepadMask: Int) {}
    func configureUserPreferences(mouseSensitivity: Double, mouseAcceleration: Int, phoneRumbleFallback: Bool, physicalControllerPassthrough: Bool, controllerMouseEmulation: Bool = false, mouseScrollSensitivity: Int = 30, controllerRumbleStrength: Double = 1) {}
    func setControllerMouseEmulation(_ enabled: Bool) {}
    var isControllerMouseEmulationEnabled: Bool { false }
    func setPhysicalControllerPassthrough(_ enabled: Bool) {}
    func setPhoneRumbleFallback(_ enabled: Bool) {}
    func attach() {}
    func detach() {}
    func sendTouchMouseMove(dx: CGFloat, dy: CGFloat) {}
    @discardableResult
    func sendNativeTouch(_ records: [NativeTouchRecord]) -> Bool { false }
    func sendMouseButton(_ button: Int, pressed: Bool) {}
    func sendKey(mapping: NativeStreamKeyboardMapping, pressed: Bool, modifiers: UInt16) {}
    func clearRemoteText() {}
    func sendMouseWheel(delta: Int) {}
    func sendUnicodeText(_ text: String) -> Int { 0 }
    func advertiseHaptics(force: Bool = false, now: TimeInterval = 0) {}
    func applyRumble(controllerId: Int, weakMagnitude: Int, strongMagnitude: Int) {}
    func setVirtualControllerEnabled(_ enabled: Bool) {}
    func setVirtualButton(_ button: NativeStreamVirtualGamepadButton, pressed: Bool) {}
    func setVirtualStick(_ stick: NativeStreamVirtualGamepadStick, x: CGFloat, y: CGFloat) {}
    func setVirtualTrigger(_ trigger: NativeStreamVirtualGamepadTrigger, value: CGFloat) {}
}

enum NativeStreamPhysicalInput {
    static var keyboardOrMouseConnected: Bool { false }
}
#endif
