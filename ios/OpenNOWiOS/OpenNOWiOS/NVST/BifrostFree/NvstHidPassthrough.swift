import Foundation

/// The raw-HID leg of NVST remote input: a client-side controller (DualShock 4 / DualSense class)
/// forwarded as its own HID reports rather than as an XInput pad, and the seat's output reports
/// (rumble, light bar, adaptive triggers) coming back the same way.
///
/// Wire formats recovered from the official client (arm64 `libBifrost2` `RiClientBackend::
/// GetHidChangePacketId`, `sendHidEvent`, `ServerControl::handleServerCommand` case `0x206`/`0x11`;
/// `libGeronimo` `HIDDevicesController::handleHidChangeEvent`, `GeronimoIOInterface::
/// handleDynamicHidChangeEvent`, `RIDeviceMediator::handleHidChangedEvent`). This file is the
/// codec only: what the seat needs to see *before* it accepts a HID device — its `ri.hidDeviceMask`
/// capability bits ("Supports DS4/DS5") and the device-add handshake answered by
/// `GENERIC_DEVICE_RESPONSE_EVENT` (`0x206`/`0x1a`) — is documented in `docs/NVST/HidPassthrough.md`
/// and not yet driven by the app.
public enum NvstHidPassthrough {
    /// `NvstHidControl_t`, the second byte of a change event. Geronimo removes the device on 3 and
    /// treats 1 as added; the mediator's other value is logged as unknown.
    public enum Change: UInt8, Equatable, Sendable {
        case added = 1
        case removed = 3
    }

    /// Client → seat: a HID device appeared or went away. RI packet type `0x12`, 14-byte body:
    /// `[u8 deviceId][u8 change][u32 BE flags][u16 BE vendorId][u16 BE productId][u16 BE field3][u16 BE field4]`.
    /// The two trailing words come straight from Geronimo's `NvstDeviceDataSimple_t` (four u16s:
    /// vendor, product, then two the disassembly does not name — version number and usage are the
    /// likeliest). `flags` bit 1 makes the official mediator ignore the event.
    public struct ChangeEvent: Equatable, Sendable {
        public let deviceId: UInt8
        public let change: Change
        public let flags: UInt32
        public let vendorId: UInt16
        public let productId: UInt16
        public let field3: UInt16
        public let field4: UInt16

        public init(deviceId: UInt8, change: Change, flags: UInt32 = 0, vendorId: UInt16, productId: UInt16, field3: UInt16 = 0, field4: UInt16 = 0) {
            self.deviceId = deviceId
            self.change = change
            self.flags = flags
            self.vendorId = vendorId
            self.productId = productId
            self.field3 = field3
            self.field4 = field4
        }

        /// The RI packet, ready for the `0x206` envelope (`NvstRemoteInput.framed`).
        public var packet: Data {
            var writer = NvstByteWriter(capacity: 14)
            writer.u8(deviceId)
            writer.u8(change.rawValue)
            writer.u32BE(flags)
            writer.u16BE(vendorId)
            writer.u16BE(productId)
            writer.u16BE(field3)
            writer.u16BE(field4)
            return NvstRemoteInput.packet(type: .hidChange, body: writer.data)
        }
    }

    /// Client → seat: one HID input report. Rides command `0x20d` like the gamepad state, in the
    /// same partially-reliable wrapper (`[0x23][u64 BE µs][0x26][deviceId][u16 BE sequence]`), then
    /// a `0x21` length-prefixed event: `[u32 LE 0x11][u8 deviceId][u8 kind][u8 reportType][report]`.
    /// `sendHidEvent` keeps one sequence counter per device (ten slots) and sends with
    /// reliability 3 when the counter fits, 0 otherwise.
    public struct Report: Equatable, Sendable {
        public static let eventType: UInt32 = 0x11
        public let deviceId: UInt8
        /// The second byte of `NvstHidReportEvent_t`; 0 for the plain input reports Geronimo forwards.
        public let kind: UInt8
        /// `NvstHidReportType_t`: 1 input, 2 output, 3 feature (the HID report types in order).
        public let reportType: UInt8
        public let data: Data

        public init(deviceId: UInt8, kind: UInt8 = 0, reportType: UInt8 = 1, data: Data) {
            self.deviceId = deviceId
            self.kind = kind
            self.reportType = reportType
            self.data = data
        }

        /// The event without its wrapper: `[u32 LE 0x11][u8 deviceId][u8 kind][u8 reportType][report]`.
        public var event: Data {
            var writer = NvstByteWriter(capacity: 7 + data.count)
            writer.u32LE(Self.eventType)
            writer.u8(deviceId)
            writer.u8(kind)
            writer.u8(reportType)
            writer.bytes(data)
            return writer.data
        }

        /// The `0x20d` payload for `sequence` and the session clock.
        public func payload(sequence: UInt16, timestampMicroseconds: UInt64) -> Data {
            let event = self.event
            var writer = NvstByteWriter(capacity: 1 + 8 + 4 + 3 + event.count)
            writer.u8(GeronimoInputEnvelope.headerByte)
            writer.u64BE(timestampMicroseconds)
            writer.u8(GeronimoInputEnvelope.partiallyReliablePayloadTag)
            writer.u8(deviceId)
            writer.u16BE(sequence)
            writer.u8(GeronimoInputEnvelope.lengthPrefixedPayloadTag)
            writer.u16BE(UInt16(clamping: event.count))
            writer.bytes(event)
            return writer.data
        }

        public func command(sequence: UInt16, timestampMicroseconds: UInt64) -> NvstControlCommand {
            NvstControlCommand(code: .gamepadEvent, payload: payload(sequence: sequence, timestampMicroseconds: timestampMicroseconds))
        }
    }

    /// Seat → client: an output report for one HID device (rumble, light bar, adaptive triggers),
    /// command `0x206` whose payload is `[u32 LE 0x11][u8 deviceId][u8 kind][u8 reportType][report]`.
    /// The official dispatcher rejects a payload under 7 bytes ("Corrupted STREAMER_RI_COMMAND
    /// PACKET_HID received") and otherwise raises `NvstClientEvent_t` type 21, which Geronimo
    /// writes to the device with `IOHIDDeviceSetReport`.
    public static func parseOutputReport(_ command: NvstControlCommand) -> Report? {
        guard command.code == .remoteInput, command.payload.count >= 7 else { return nil }
        var reader = NvstByteReader(command.payload)
        guard let type = try? reader.u32LE(), type == Report.eventType,
              let deviceId = try? reader.u8(), let kind = try? reader.u8(), let reportType = try? reader.u8() else { return nil }
        return Report(deviceId: deviceId, kind: kind, reportType: reportType, data: Data(reader.unread))
    }

    /// Seat → client: the answer to a change event, `0x206` with `[u32 LE 0x1a][u8 deviceId]
    /// [u8 field][u32 BE requestId][u8 status]` ("handleHidChangeResponseEvent (requestId: %u,
    /// deviceId: %u)").
    public struct ChangeResponse: Equatable, Sendable {
        public static let eventType: UInt32 = 0x1a
        public let deviceId: UInt8
        public let field: UInt8
        public let requestId: UInt32
        public let status: UInt8

        public static func parse(_ command: NvstControlCommand) -> ChangeResponse? {
            guard command.code == .remoteInput, command.payload.count >= 11 else { return nil }
            var reader = NvstByteReader(command.payload)
            guard let type = try? reader.u32LE(), type == eventType,
                  let deviceId = try? reader.u8(), let field = try? reader.u8(),
                  let requestId = try? reader.u32BE(), let status = try? reader.u8() else { return nil }
            return ChangeResponse(deviceId: deviceId, field: field, requestId: requestId, status: status)
        }
    }

    /// `ri.hidDeviceMask` from the seat's DESCRIBE, as Geronimo reads it ("Configuring server
    /// supported Hid devices: Supports DS4: %s, DS5: %s (raw:%x)"). Which bit is which is inferred
    /// from that string's order; the official capture announces 4 back.
    public struct SeatCapability: Equatable, Sendable {
        public let raw: UInt32
        public init(raw: UInt32) { self.raw = raw }
        public var supportsDualShock4: Bool { raw & 0x1 != 0 }
        public var supportsDualSense: Bool { raw & 0x2 != 0 }
        public var summary: String { "hidDeviceMask=0x\(String(raw, radix: 16)) ds4=\(supportsDualShock4) ds5=\(supportsDualSense)" }
    }
}
