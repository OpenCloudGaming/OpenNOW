import CoreGraphics
import Foundation

/// Which games get native touch.
///
/// GeForce NOW ships no per-game touch layouts. The games that "support touch" are simply the ones
/// whose Windows build already reacts to a Windows digitizer — invariably because they also ship on
/// phones or tablets — and they switch to their own mobile UI the moment one appears. So this file
/// decides nothing about *how* touch works, only *where* it gets turned on.
///
/// The catalog carries the answer: a launch option's `supportedControls` includes `TOUCHSCREEN`,
/// the same capability signal the official client uses. Reading that signal replaces the
/// title-matched "fortnite" check the iOS build shipped with, which missed every other touch title
/// and broke for localized names.
///
/// Ported from `NativeTouchGames.kt` in the Android build.
enum NativeTouchSupport {

    /// The value the catalog uses to mark a touch-capable variant.
    static let supportedControlTouchscreen = "TOUCHSCREEN"

    /// Whether the catalog itself claims this game takes touch, across any of its launch options.
    static func catalogClaimsTouchSupport(_ game: CloudGame?) -> Bool {
        guard let game else { return false }
        return game.launchOptions.contains { option in
            option.supportedControls?.contains {
                $0.caseInsensitiveCompare(supportedControlTouchscreen) == .orderedSame
            } ?? false
        }
    }

    static func shouldUseNativeTouch(mode: NativeTouchMode, game: CloudGame?) -> Bool {
        switch mode {
        case .never: return false
        case .always: return true
        case .automatic: return catalogClaimsTouchSupport(game)
        }
    }

    /// An explicit Always choice must provision touch even with a keyboard/mouse attached.
    /// Automatic mode may prefer physical input; claims keep the allocation's original choice.
    static func shouldProvisionNativeTouch(mode: NativeTouchMode, game: CloudGame?,
                                          keyboardMouseConnected: Bool) -> Bool {
        shouldUseNativeTouch(mode: mode, game: game)
            && (mode == .always || !keyboardMouseConnected)
    }

    /// The initial live route follows the same preference used when requesting input devices.
    static func shouldStartWithNativeTouch(mode: NativeTouchMode, game: CloudGame?,
                                          keyboardMouseConnected: Bool, provisioned: Bool?,
                                          preferVirtualController: Bool) -> Bool {
        provisioned != false && !preferVirtualController
            && shouldProvisionNativeTouch(mode: mode, game: game,
                                          keyboardMouseConnected: keyboardMouseConnected)
    }

    /// Resolves native touch for a live stream after the player has made a session-level choice.
    /// A catalog capability is useful guidance, but it must not lock the player out of the
    /// on-screen controller when they have deliberately asked for it.
    static func shouldUseNativeTouchForStream(
        mode: NativeTouchMode,
        game: CloudGame?,
        preferVirtualController: Bool
    ) -> Bool {
        !preferVirtualController && shouldUseNativeTouch(mode: mode, game: game)
    }

    /// Which stored touch-control layout the on-screen controller should load.
    ///
    /// Fortnite is the one title that still needs its own preset, because its mobile UI puts the
    /// build and edit controls where a standard gamepad layout puts the face buttons. Everything
    /// else uses the default preset.
    static func touchLayoutProfile(gameTitle: String, settings: AppSettings) -> String {
        guard settings.touch.nativeTouchMode != .never else { return "default" }
        return gameTitle.localizedCaseInsensitiveContains("fortnite") ? "fortnite-mobile" : "default"
    }

    /// The launch mode the session has to be *created* with for any of the above to matter.
    ///
    /// The host provisions its virtual input devices from this value once, when the session is
    /// created, and never revisits it. The official client gates its whole touch pipeline on the
    /// same value — `enableTouchInput: appLaunchMode === AppLaunchMode.TouchFriendly` — so a
    /// session created under any other mode drops perfectly well-formed touch packets on the
    /// floor. That is why this reads exactly the same predicate the stream reads: if the two ever
    /// disagree, touch is dead for the whole session with nothing on the wire to say so.
    ///
    /// Mirrors `appLaunchModeFor` in `OpenNowViewModel.kt`.
    static func appLaunchMode(mode: NativeTouchMode, game: CloudGame?) -> GFNAppLaunchMode {
        shouldUseNativeTouch(mode: mode, game: game) ? .touchFriendly : .default
    }

    /// One line per session recording the catalog signal and the decision it produced, so a
    /// "touch did not work" report can be answered without guessing.
    static func diagnostics(game: CloudGame?, enabled: Bool, provisioned: Bool) -> String {
        guard let game else {
            return "native touch enabled=\(enabled) provisioned=\(provisioned) game=none"
        }
        let controls = game.launchOptions
            .compactMap(\.supportedControls)
            .flatMap { $0 }
            .reduce(into: [String]()) { acc, value in
                if !acc.contains(value) { acc.append(value) }
            }
            .joined(separator: "|")
        return "native touch enabled=\(enabled) provisioned=\(provisioned) "
            + "id=\(game.id) title=\(game.title) "
            + "catalogTouch=\(catalogClaimsTouchSupport(game)) "
            + "supportedControls=\(controls.isEmpty ? "none" : controls)"
    }
}

// MARK: - Wire format

/// Phase of a single finger, as the host expects it on the wire.
enum NativeTouchPhase {
    static let down: UInt8 = 1
    static let up: UInt8 = 2
    static let move: UInt8 = 4
    static let cancel: UInt8 = 8
}

/// Touch coordinates travel as an unsigned 16-bit fraction of the video area.
let nativeTouchCoordinateMax = 65_535

/// The host tracks at most this many fingers at once.
let nativeTouchMaxConcurrentTouches = 8

/// One packet carries at most this many records.
let nativeTouchMaxRecordsPerBatch = 40

/// One finger in one packet. `slot` is the host's finger index — deliberately not the platform's
/// touch identity, see `NativeTouchSlotAllocator`.
struct NativeTouchRecord: Equatable {
    var slot: Int
    var phase: UInt8
    var x: Int
    var y: Int
    var radiusX: Int = 0
    var radiusY: Int = 0
    /// Zero means "stamp it at encode time", so a caller does not have to reach for the same clock
    /// the encoder already uses for every other packet.
    var timestampUs: UInt64 = 0
}

/// Maps platform touch identities onto the small, dense finger indices the host expects.
///
/// UIKit hands back `UITouch` objects whose identity is stable only for the life of one finger, and
/// it recycles them freely across gestures. The host, meanwhile, wants the lowest free index,
/// reused as soon as a finger lifts. Forwarding a raw identity would make it see fingers appear at
/// arbitrary indices and eventually run past its own limit.
///
/// Generic over the identity type so the whole thing is testable without UIKit.
/// Ported from `TouchSlotAllocator` in the Android build.
struct NativeTouchSlotAllocator<Pointer: Hashable> {
    private var slotByPointer: [Pointer: Int] = [:]
    private var usedSlots: Set<Int> = []

    init() {}

    var activeCount: Int { slotByPointer.count }

    var activePointers: [Pointer] { Array(slotByPointer.keys) }

    /// Slot for `pointer`, allocating the lowest free one. Nil when every slot is in use.
    mutating func acquire(_ pointer: Pointer) -> Int? {
        if let existing = slotByPointer[pointer] { return existing }
        var slot = 0
        while usedSlots.contains(slot) { slot += 1 }
        guard slot < nativeTouchMaxConcurrentTouches else { return nil }
        slotByPointer[pointer] = slot
        usedSlots.insert(slot)
        return slot
    }

    /// Slot for `pointer` without allocating one.
    func peek(_ pointer: Pointer) -> Int? { slotByPointer[pointer] }

    /// Frees the slot held by `pointer`, returning it so the caller can still report the lift.
    mutating func release(_ pointer: Pointer) -> Int? {
        guard let slot = slotByPointer.removeValue(forKey: pointer) else { return nil }
        usedSlots.remove(slot)
        return slot
    }

    mutating func removeAll() {
        slotByPointer.removeAll(keepingCapacity: true)
        usedSlots.removeAll(keepingCapacity: true)
    }
}

/// One finger as UIKit reported it, before any mapping.
struct NativeTouchPointerSample<Pointer: Hashable> {
    let pointer: Pointer
    let location: CGPoint
    var radiusX: CGFloat = 0
    var radiusY: CGFloat = 0
}

/// Geometry and slot rules for native touch — everything that is easy to get subtly wrong, kept
/// free of UIKit so all of it is testable.
///
/// Ported from `buildTouchBatch` / `streamPointForTouch` in `StreamInteraction.kt`.
enum NativeTouchGeometry {

    /// Maps a touch inside a view onto the stream's pixel space, undoing the presentation
    /// zoom/pan first, then the letterbox/pillarbox bars the renderer adds whenever the view and
    /// the stream disagree about aspect ratio.
    ///
    /// Everything it needs arrives as an argument and the result is expressed as a fraction of the
    /// view, which is why a rotation or a Split View resize needs no bookkeeping at all.
    ///
    /// - Parameter clamp: right for a cursor, which must land somewhere. Native touch passes
    ///   `false` so it can tell a finger on the letterbox bar from one at the edge of the picture,
    ///   and drop it.
    static func streamPoint(
        touch: CGPoint,
        viewSize: CGSize,
        streamSize: CGSize,
        stretchToFill: Bool,
        zoomScale: CGFloat = 1,
        zoomOffset: CGSize = .zero,
        clamp: Bool = true
    ) -> CGPoint {
        guard viewSize.width > 0, viewSize.height > 0,
              streamSize.width > 0, streamSize.height > 0 else {
            return .zero
        }
        guard touch.x.isFinite, touch.y.isFinite else {
            return CGPoint(x: CGFloat.nan, y: CGFloat.nan)
        }

        // The renderer scales about the view's centre, so undoing it is the same operation in
        // reverse — no per-gesture cursor state is involved.
        let scale = (zoomScale.isFinite && zoomScale >= 1) ? zoomScale : 1
        let translationX = zoomOffset.width.isFinite ? zoomOffset.width : 0
        let translationY = zoomOffset.height.isFinite ? zoomOffset.height : 0
        let centreX = viewSize.width / 2
        let centreY = viewSize.height / 2
        let untransformedX = centreX + (touch.x - centreX - translationX) / scale
        let untransformedY = centreY + (touch.y - centreY - translationY) / scale

        var videoWidth = viewSize.width
        var videoHeight = viewSize.height
        var offsetX: CGFloat = 0
        var offsetY: CGFloat = 0

        // Fill mode really does fill: `RTCMTLVideoView` is switched to `.scaleToFill`, so there is
        // no bar to compensate for. Fit mode letterboxes or pillarboxes.
        if !stretchToFill {
            let streamAspect = streamSize.width / streamSize.height
            let viewAspect = viewSize.width / viewSize.height
            if viewAspect > streamAspect {
                videoWidth = viewSize.height * streamAspect
                offsetX = (viewSize.width - videoWidth) / 2
            } else if viewAspect < streamAspect {
                videoHeight = viewSize.width / streamAspect
                offsetY = (viewSize.height - videoHeight) / 2
            }
        }
        guard videoWidth > 0, videoHeight > 0 else { return .zero }

        var x = (untransformedX - offsetX) / videoWidth * streamSize.width
        var y = (untransformedY - offsetY) / videoHeight * streamSize.height
        if clamp {
            x = min(max(x, 0), streamSize.width)
            y = min(max(y, 0), streamSize.height)
        }
        return CGPoint(x: x, y: y)
    }

    /// Turns one event's fingers into the records for a single packet.
    static func buildBatch<Pointer: Hashable>(
        allocator: inout NativeTouchSlotAllocator<Pointer>,
        phase: UInt8,
        pointers: [NativeTouchPointerSample<Pointer>],
        viewSize: CGSize,
        streamSize: CGSize,
        stretchToFill: Bool,
        zoomScale: CGFloat = 1,
        zoomOffset: CGSize = .zero,
        timestampUs: UInt64 = 0
    ) -> [NativeTouchRecord] {
        guard viewSize.width > 0, viewSize.height > 0,
              streamSize.width > 0, streamSize.height > 0 else {
            return []
        }

        // A lift must always be reported, wherever the finger ended up. Swallowing one leaves the
        // host holding that finger down for the rest of the session.
        let lifting = phase == NativeTouchPhase.up || phase == NativeTouchPhase.cancel
        let maximum = CGFloat(nativeTouchCoordinateMax)
        var records: [NativeTouchRecord] = []
        records.reserveCapacity(min(pointers.count, nativeTouchMaxRecordsPerBatch))

        for pointer in pointers {
            if records.count >= nativeTouchMaxRecordsPerBatch { break }

            let point = streamPoint(
                touch: pointer.location,
                viewSize: viewSize,
                streamSize: streamSize,
                stretchToFill: stretchToFill,
                zoomScale: zoomScale,
                zoomOffset: zoomOffset,
                clamp: false
            )
            let x = point.x / streamSize.width * maximum
            let y = point.y / streamSize.height * maximum
            let radiusX = pointer.radiusX / streamSize.width * maximum
            let radiusY = pointer.radiusY / streamSize.height * maximum

            let finiteSample = x.isFinite && y.isFinite && radiusX.isFinite && radiusY.isFinite
            if !finiteSample && !lifting { continue }

            let safeX = x.isFinite ? x : 0
            let safeY = y.isFinite ? y : 0
            let safeRadiusX = radiusX.isFinite ? max(radiusX, 0) : 0
            let safeRadiusY = radiusY.isFinite ? max(radiusY, 0) : 0
            let outside = safeX < -safeRadiusX || safeX > maximum + safeRadiusX
                || safeY < -safeRadiusY || safeY > maximum + safeRadiusY
            if outside && !lifting { continue }

            guard let slot = lifting ? allocator.release(pointer.pointer) : allocator.acquire(pointer.pointer) else {
                continue
            }

            records.append(
                NativeTouchRecord(
                    slot: slot,
                    phase: phase,
                    x: Int(min(max(safeX.rounded(), 0), maximum)),
                    y: Int(min(max(safeY.rounded(), 0), maximum)),
                    radiusX: Int(max(pointer.radiusX.isFinite ? pointer.radiusX.rounded() : 0, 0)),
                    radiusY: Int(max(pointer.radiusY.isFinite ? pointer.radiusY.rounded() : 0, 0)),
                    timestampUs: timestampUs
                )
            )
        }
        return records
    }

    /// Lifts every finger the host still believes is down. Used when touch is switched off or the
    /// surface goes away, because in neither case will UIKit deliver the missing `touchesEnded`.
    static func cancelAll<Pointer: Hashable>(
        allocator: inout NativeTouchSlotAllocator<Pointer>
    ) -> [NativeTouchRecord] {
        let records = allocator.activePointers.compactMap { pointer -> NativeTouchRecord? in
            guard let slot = allocator.release(pointer) else { return nil }
            return NativeTouchRecord(slot: slot, phase: NativeTouchPhase.cancel, x: 0, y: 0)
        }
        allocator.removeAll()
        return records
    }
}
