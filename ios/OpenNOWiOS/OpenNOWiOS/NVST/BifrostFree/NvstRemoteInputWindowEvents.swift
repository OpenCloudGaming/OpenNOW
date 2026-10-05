import Foundation

/// The window-facing corner of the NVST remote-input space: RI packet types 6, 15 and 16.
///
/// This file is the codec only. Nothing in the app sends any of these, exactly as
/// `NvstHidPassthrough` carries the DS4/DS5 wire formats with no runtime call site — the shape is
/// worth keeping in a form that tests can pin, the wire is not worth guessing at.
///
/// What is recovered, per type:
///
/// - **6, the extended absolute pointer move.** The id sits in the `ValidId` space verified
///   against the official client's `RiClientBackend::Get*PacketId` dispatch, and the static read of
///   the builder records it as the flag `0x1000` form of the type-5 absolute move "carrying two
///   extra u16 fields". Type 5 itself is byte-exact against an `SSL_write` capture, so the
///   extended form is a known-good ten-byte body plus two words. That is enough to encode; what
///   the two words *mean* is not, and `ExtendedAbsoluteMouseMove` says so field by field.
/// - **15, window focus.** A 6-byte body, and nothing else — no field, no unit, no capture, and
///   the direction is contested inside our own tables: the RI table reads 15 as a client-to-server
///   focus event while `GeronimoInputEventType.haptic` reads the same id as a server-to-client
///   haptic event. Six bytes cannot be filled honestly from that, so no encoder exists here.
///   Focus already travels as control command `0x0320` with `activeWindowState`, which is what
///   makes the seat publish cursor mode at all.
/// - **16, window geometry.** A 14-byte body, and nothing else. Same verdict, with a sharper
///   reason not to guess: geometry is negotiated correctly today through ANNOUNCE and the type-5
///   viewport, and a seat that acted on a fabricated geometry could resize or letterbox a stream
///   that currently works.
extension NvstRemoteInput {
    /// The builder flag that selects packet type 6 rather than type 5.
    ///
    /// Recovered as a value, not as a rule: whether the seat expects it alone, or ORed with
    /// `absoluteFlag`, is not established — the flags word of `ExtendedAbsoluteMouseMove` is
    /// therefore supplied by the caller instead of being written for it.
    public static let extendedAbsoluteFlag: UInt16 = 0x1000

    /// RI type 6: the absolute pointer position in its extended form, a 14-byte body.
    ///
    /// The first five words are the type-5 body, which is byte-exact from a capture of the
    /// official client (`0x065f 0x02af 0x0800 0x0660 0x02b0` for a pointer at 1631,687 in a
    /// 1632x688 view) — position and viewport are view coordinates, not stream-resolution ones,
    /// the same as `absoluteMouseMove`.
    ///
    /// The last two words are the "two extra u16 fields" and are the whole of what type 6 adds.
    /// They are encoded appended, after the viewport, because that is the only placement the
    /// recovered description constrains; an interleaved layout would fit the same sentence.
    /// Nothing here is a drop-in replacement for `absoluteMouseMove`: that path is live and
    /// capture-verified, this one has never been on a wire.
    public struct ExtendedAbsoluteMouseMove: Equatable, Sendable {
        public let x: UInt16
        public let y: UInt16
        /// The flags word. `absoluteFlag` (`0x0800`) is what type 5 carries and
        /// `extendedAbsoluteFlag` (`0x1000`) is what selects type 6; which of the two, or both,
        /// the seat reads here is unrecovered, so the caller states it.
        public let flags: UInt16
        public let viewportWidth: UInt16
        public let viewportHeight: UInt16
        /// Body word 6. UNIDENTIFIED: recovered only as "an extra u16", with no name, no unit and
        /// no captured value.
        public let field6: UInt16
        /// Body word 7. UNIDENTIFIED on the same terms as `field6`.
        public let field7: UInt16

        /// Every field is required. The two extra words and the flags word are exactly the parts
        /// of this packet whose meaning is unknown, so a caller has to name a value for each of
        /// them rather than inherit a default that would look like knowledge.
        public init(x: UInt16,
                    y: UInt16,
                    flags: UInt16,
                    viewportWidth: UInt16,
                    viewportHeight: UInt16,
                    field6: UInt16,
                    field7: UInt16) {
            self.x = x
            self.y = y
            self.flags = flags
            self.viewportWidth = viewportWidth
            self.viewportHeight = viewportHeight
            self.field6 = field6
            self.field7 = field7
        }

        /// The 14-byte body, every word big-endian as the rest of the pointer packets are.
        public var body: Data {
            var writer = NvstByteWriter(capacity: 14)
            for word in [x, y, flags, viewportWidth, viewportHeight, field6, field7] {
                writer.u16BE(word)
            }
            return writer.data
        }

        /// The RI packet, ready for `NvstRemoteInput.framed` and the `0x206` command.
        public var packet: Data {
            NvstRemoteInput.packet(type: .absoluteMouseMoveExtended, body: body)
        }
    }
}
