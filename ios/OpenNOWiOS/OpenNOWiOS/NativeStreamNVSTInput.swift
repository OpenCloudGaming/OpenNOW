import Foundation

/// Converts the existing iOS input bridge's Geronimo packets to the native control wire.
/// All native timestamps use the same session clock as video acknowledgements.
struct NativeStreamNVSTInput {
    enum Output {
        case control(NvstControlCommand), gamepad(NvstGamepadPacket), heartbeat
        case touch(NvstControlCommand, records: Int)
    }
    enum InputError: Error { case malformed, unsupported(UInt32) }

    static func translate(_ data: Data, timestamp: UInt64, sequence: UInt16) throws -> [Output] {
        var bytes = [UInt8](data)
        if bytes.first == 0x23 {
            guard bytes.count >= 10 else { throw InputError.malformed }
            bytes.removeFirst(9)
            if bytes.first == 0x26 {
                guard bytes.count >= 5 else { throw InputError.malformed }
                bytes.removeFirst(4)
            }
            if bytes.first == 0x21 {
                guard bytes.count >= 3 else { throw InputError.malformed }
                let count = Int(bytes[1]) << 8 | Int(bytes[2])
                bytes.removeFirst(3)
                guard bytes.count == count else { throw InputError.malformed }
            } else if bytes.first == 0x22 { bytes.removeFirst() }
            else { throw InputError.malformed }
        } else if bytes.first == 0x22 { bytes.removeFirst() }
        guard bytes.count >= 4 else { throw InputError.malformed }
        func word(_ i: Int, little: Bool = false) -> UInt16 {
            little ? UInt16(bytes[i]) | UInt16(bytes[i+1]) << 8 : UInt16(bytes[i]) << 8 | UInt16(bytes[i+1])
        }
        let type = (0..<4).reduce(UInt32(0)) { $0 | UInt32(bytes[$1]) << ($1 * 8) }
        let packet: Data
        switch type {
        case 2: return [.heartbeat]
        case 3, 4:
            guard bytes.count == 18 else { throw InputError.malformed }
            packet = NvstRemoteInput.keyboard(virtualKey: word(4), modifiers: word(6), isPressed: type == 3)
        case 7:
            guard bytes.count == 22 else { throw InputError.malformed }
            packet = NvstRemoteInput.mouseMove(deltaX: Int16(bitPattern: word(4)), deltaY: Int16(bitPattern: word(6)), flags: word(8))
        case 8, 9:
            guard bytes.count == 18, let button = NvstRemoteInput.Button(rawValue: bytes[4]) else { throw InputError.malformed }
            packet = NvstRemoteInput.mouseButton(button, isPressed: type == 8)
        case 10:
            guard bytes.count == 22 else { throw InputError.malformed }
            packet = NvstRemoteInput.mouseWheel(delta: Int16(bitPattern: word(6)))
        case 12:
            guard bytes.count == 38 else { throw InputError.malformed }
            let bitmap = word(8, little: true)
            return [.gamepad(NvstGamepadPacket(sequence: sequence, timestampMicroseconds: timestamp,
                buttons: word(12, little: true), leftTrigger: bytes[14], rightTrigger: bytes[15],
                leftStickX: Int16(bitPattern: word(16, little: true)), leftStickY: Int16(bitPattern: word(18, little: true)),
                rightStickX: Int16(bitPattern: word(20, little: true)), rightStickY: Int16(bitPattern: word(22, little: true)),
                gamepadIndex: word(6, little: true), connectedBitmap: bitmap | (bitmap << 8)))]
        case 13:
            guard bytes.count == 6 else { throw InputError.malformed }
            packet = NvstRemoteInput.hapticsState(enabled: word(4) != 0)
        case 23:
            guard let text = String(bytes: bytes.dropFirst(4), encoding: .utf8) else { throw InputError.malformed }
            return NvstRemoteInput.utf8TextPackets(forText: text).packets.map {
                .control(NvstControlCommand(code: .remoteInput, payload: NvstRemoteInput.framed($0, framing: .enveloped, sequence: sequence, timestampMicroseconds: timestamp)))
            }
        case 24:
            guard bytes.count >= 8 else { throw InputError.malformed }
            let count = Int(word(6))
            guard count > 0, bytes.count == 8 + count * 16 else { throw InputError.malformed }
            for record in 0..<count {
                for byte in 0..<8 { bytes[16 + record * 16 + byte] = UInt8(truncatingIfNeeded: timestamp >> ((7-byte)*8)) }
            }
            // Native RI adds its BE length word before the existing type/body.
            var writer = NvstByteWriter(capacity: bytes.count + 4)
            writer.u32BE(UInt32(bytes.count)); writer.bytes(bytes)
            return [.touch(NvstControlCommand(code: .remoteInput, payload:
                NvstRemoteInput.framed(writer.data, framing: .enveloped,
                    sequence: sequence, timestampMicroseconds: timestamp)), records: count)]
        default: throw InputError.unsupported(type)
        }
        return [.control(NvstControlCommand(code: .remoteInput, payload:
            NvstRemoteInput.framed(packet, framing: .enveloped, sequence: sequence, timestampMicroseconds: timestamp)))]
    }
}
