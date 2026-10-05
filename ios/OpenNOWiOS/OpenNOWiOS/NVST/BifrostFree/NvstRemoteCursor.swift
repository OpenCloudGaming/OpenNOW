import Foundation

/// The seat's cursor notifications, and what they say about the pointer the game wants drawn.
///
/// The seat can either composite its own cursor into the encoded video (`0x308` mouse cursor
/// capture) or leave the picture clean and tell the client what to draw. Doing both at once is the
/// double-cursor bug: the seat's pointer is baked into the frames while the client's own pointer
/// floats over them. NVIDIA's client enables capture only for startup, then switches to these
/// notifications and turns capture back off — which is only possible if the client understands
/// them, so the parse lives here.
public struct NvstRemoteCursor: Equatable, Sendable {
    /// A predefined system cursor: an id, an optional position, an optional visibility byte.
    public static let systemCursorCode: NvstControlCommandCode = 0x010f
    /// Settled by the official client's `handleServerCommand` dispatch
    /// (`docs/NVST/OfficialClientAudit.md`): `0x0110` IS a bitmap cursor — the official handler
    /// logs "Server sent bitmap cursor info with ID: %u, size: %u" — and `0x0111` is
    /// video-stream-progress.
    public static let bitmapCursorCode: NvstControlCommandCode = 0x0110

    /// Which notification the state came from. The two carry different authority, and conflating
    /// them is a real bug rather than a tidiness point: `0x010f` states what the game wants of the
    /// pointer, while `0x0110` only says a shape was pushed. A game holding the cursor captive for
    /// mouselook still has the seat pushing shape updates, so reading those as "visible" is exactly
    /// what leaves a pointer floating over the game.
    public enum Source: Equatable, Sendable {
        case systemCursor
        case bitmapCursor
    }

    /// The two numbers the official handler names when it logs a bitmap cursor. Everything after
    /// them — the pixel format, the dimensions, the hotspot, the image bytes — is unrecovered, so
    /// nothing here decodes an image.
    public struct Bitmap: Equatable, Sendable {
        /// The seat's handle for this cursor shape, repeated for every push of the same shape.
        public let id: UInt32
        /// The `size` the official handler logs beside the id. Whether it counts image bytes or
        /// pixels is not recovered — it is carried as the seat sent it and used only for the log.
        public let byteCount: UInt32

        public init(id: UInt32, byteCount: UInt32) {
            self.id = id
            self.byteCount = byteCount
        }

        /// UNVERIFIED offsets. The only recovered content of `0x0110` is the official log line
        /// "Server sent bitmap cursor info with ID: %u, size: %u", so the two words are read at the
        /// front of the payload in the order that line prints them, `%u` wide. Little-endian
        /// because every recovered `0x1xx` notification is (`NvstHapticEvent`,
        /// `NvstHdrModeNotification`) — the system-cursor id above reads big-endian only because
        /// nothing there does more than compare it against zero.
        static func parse(_ payload: Data) -> Bitmap? {
            var reader = NvstByteReader(payload)
            guard let id = try? reader.u32LE(), let byteCount = try? reader.u32LE() else { return nil }
            return Bitmap(id: id, byteCount: byteCount)
        }
    }

    /// Whether the game wants a pointer on screen at all. False means it has hidden the cursor —
    /// mouselook — and the client should hide its own pointer rather than leave one floating.
    /// Authoritative only for `.systemCursor`; see `visibility(following:)`.
    public let isVisible: Bool
    public let source: Source
    /// Present only for `.bitmapCursor`, and only what the official log line names.
    public let bitmap: Bitmap?

    public init(isVisible: Bool, source: Source = .systemCursor, bitmap: Bitmap? = nil) {
        self.isVisible = isVisible
        self.source = source
        self.bitmap = bitmap
    }

    /// What the pointer state becomes after this notification, given what it is now. `nil` means
    /// the notification says nothing about visibility and the current state stands.
    ///
    /// This is where the two sources are kept apart. A bitmap push may confirm a pointer that is
    /// already on screen; it can never raise a hidden one, because the seat keeps pushing shapes
    /// throughout mouselook and un-hiding on those is the double-cursor bug in its other form.
    public func visibility(following current: Bool?) -> Bool? {
        switch source {
        case .systemCursor: isVisible
        case .bitmapCursor: current
        }
    }

    public var summary: String {
        switch source {
        case .systemCursor: "system \(isVisible ? "visible" : "hidden")"
        case .bitmapCursor: "bitmap id=\(bitmap?.id ?? 0) size=\(bitmap?.byteCount ?? 0)"
        }
    }

    public static func from(_ command: NvstControlCommand) -> NvstRemoteCursor? {
        switch command.code {
        case systemCursorCode:
            return systemCursor(payload: command.payload)
        case bitmapCursorCode:
            // A payload too short to carry the pair the official handler logs is not something this
            // can claim to have read: it stays in the unparsed log, where a real capture can still
            // settle the layout.
            guard let bitmap = Bitmap.parse(command.payload) else { return nil }
            // A shape push means the seat has a pointer to draw, so it agrees with a visible
            // pointer — but it is only ever a confirmation, never a transition.
            return NvstRemoteCursor(isVisible: true, source: .bitmapCursor, bitmap: bitmap)
        default:
            return nil
        }
    }

    private static func systemCursor(payload: Data) -> NvstRemoteCursor? {
        guard payload.count >= 4 else { return nil }
        var reader = NvstByteReader(payload)
        let cursorID = (try? reader.u32BE()) ?? 0
        // Predefined cursor 0 is the "no cursor" shape. Only a *system* cursor id of 0 means
        // hidden — a bitmap cursor with id 0 is still a visible image, which is why the two
        // commands are kept apart above.
        //
        // The leading id is the only field recovered from `0x010f`. An earlier revision scanned a
        // longer payload for an explicit visibility byte at offset 8, but that offset was never
        // verified and any other field landing there flipped the state on unrelated data — the
        // pointer vanished mid-game. A wrongly shown pointer is a cosmetic double cursor; a
        // wrongly hidden one is a session with no pointer, so this decides from the id it can prove.
        return NvstRemoteCursor(isVisible: cursorID != 0)
    }
}
