import SwiftUI
import UIKit

#if os(iOS) && canImport(WebRTC)
/// Owns a single finger through UIKit cancellation, interruption and view removal.
/// SwiftUI drag onEnded is not called when the system cancels a gesture.
final class NativeStreamVirtualStickTouchControl: UIView {
    var deadZone: Double = 0
    var followsFinger = false
    var changed: (CGFloat, CGFloat) -> Void = { _, _ in }
    var pressed: (Bool) -> Void = { _ in }
    var offsetChanged: (CGSize) -> Void = { _ in }
    private var trackedTouch: UITouch?
    private var origin: CGPoint?
    private var moved = false
    private var pendingRelease: DispatchWorkItem?
    private var stickPressed = false
    private var interruptionObserver: NSObjectProtocol?
    private var previousSize = CGSize.zero

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isMultipleTouchEnabled = true
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.willResignActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.endTouch(cancelled: true) }
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    deinit {
        pendingRelease?.cancel()
        if let interruptionObserver { NotificationCenter.default.removeObserver(interruptionObserver) }
    }

    func beginTouch(at point: CGPoint) {
        endTouch(cancelled: true)
        origin = followsFinger ? point : CGPoint(x: bounds.midX, y: bounds.midY)
        moved = false
        moveTouch(to: point)
    }

    func moveTouch(to point: CGPoint) {
        guard let origin else { return }
        let radius = max(1, min(bounds.width, bounds.height) * 0.34)
        let raw = CGSize(width: point.x - origin.x, height: point.y - origin.y)
        let magnitude = hypot(raw.width, raw.height)
        let gain = magnitude > radius ? radius / magnitude : 1
        let offset = CGSize(width: raw.width * gain, height: raw.height * gain)
        moved = moved || magnitude > 5
        offsetChanged(offset)
        let vector = TouchStickMath.applyDeadZone(x: offset.width / radius,
                                                 y: -offset.height / radius, deadZone: deadZone)
        changed(vector.0, vector.1)
    }

    func endTouch(cancelled: Bool) {
        let tapped = origin != nil && !moved && !cancelled
        trackedTouch = nil
        origin = nil
        moved = false
        changed(0, 0)
        offsetChanged(.zero)
        pendingRelease?.cancel()
        pendingRelease = nil
        if stickPressed { pressed(false); stickPressed = false }
        if tapped {
            stickPressed = true
            pressed(true)
            let release = DispatchWorkItem { [weak self] in
                guard let self else { return }
                self.stickPressed = false
                self.pendingRelease = nil
                self.pressed(false)
            }
            pendingRelease = release
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.06, execute: release)
        }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard trackedTouch == nil, let touch = touches.first else { return }
        beginTouch(at: touch.location(in: self))
        trackedTouch = touch
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = trackedTouch, touches.contains(touch) else { return }
        moveTouch(to: touch.location(in: self))
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = trackedTouch, touches.contains(touch) else { return }
        endTouch(cancelled: false)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = trackedTouch, touches.contains(touch) else { return }
        endTouch(cancelled: true)
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil { endTouch(cancelled: true) }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if previousSize != .zero, previousSize != bounds.size { endTouch(cancelled: true) }
        previousSize = bounds.size
    }
}

private struct NativeStreamVirtualStickTouchSurface: UIViewRepresentable {
    let deadZone: Double
    let followsFinger: Bool
    let changed: (CGFloat, CGFloat) -> Void
    let pressed: (Bool) -> Void
    let offsetChanged: (CGSize) -> Void

    func makeUIView(context: Context) -> NativeStreamVirtualStickTouchControl {
        NativeStreamVirtualStickTouchControl(frame: .zero)
    }

    func updateUIView(_ view: NativeStreamVirtualStickTouchControl, context: Context) {
        view.deadZone = deadZone
        view.followsFinger = followsFinger
        view.changed = changed
        view.pressed = pressed
        view.offsetChanged = offsetChanged
    }

    static func dismantleUIView(_ view: NativeStreamVirtualStickTouchControl, coordinator: ()) {
        view.endTouch(cancelled: true)
    }
}

struct NativeStreamVirtualStickView: View {
    let label: String
    let size: CGFloat
    /// Fraction of travel ignored around centre. Rests at 0 — a resting thumb on a stick with no
    /// dead zone still sends tiny deflections, which reads as drift in games that poll raw axes.
    var deadZone: Double = 0
    /// `.dynamic` re-centres the stick under wherever the thumb lands rather than pinning it to
    /// the drawn circle, which is what most touch shooters expect.
    var followsFinger: Bool = false
    var outlineStyle: Bool = false
    var gripDots = false
    var concentricRings = false
    let changed: (CGFloat, CGFloat) -> Void
    let pressed: (Bool) -> Void

    @State private var knobOffset = CGSize.zero
    var body: some View {
        ZStack {
            Circle()
                .fill(outlineStyle ? AnyShapeStyle(Color.black.opacity(0.18)) : AnyShapeStyle(.ultraThinMaterial))
                .overlay(Circle().stroke(Color.white.opacity(outlineStyle ? 0.42 : 0.22), lineWidth: outlineStyle ? 1.5 : 1))
            if concentricRings {
                Circle()
                    .stroke(Color(white: 0.25).opacity(0.70), lineWidth: 1.5)
                    .frame(width: size * 0.68, height: size * 0.68)
            }
            Circle()
                .fill(Color.white.opacity(gripDots ? 0.70 : outlineStyle ? 0.14 : 0.30))
                .overlay(Circle().strokeBorder(Color.white.opacity(outlineStyle && !gripDots ? 0.55 : 0), lineWidth: 1.5))
                .frame(width: size * (concentricRings ? 0.41 : 0.48), height: size * (concentricRings ? 0.41 : 0.48))
                .overlay {
                    if gripDots {
                        Canvas { context, bounds in
                            let step = bounds.width / 5
                            let diameter = step * 0.30
                            for row in 0..<5 {
                                for column in 0..<5 {
                                    // Round off the pattern by leaving the four corners empty.
                                    guard !((row == 0 || row == 4) && (column == 0 || column == 4)) else { continue }
                                    let dot = CGRect(x: (CGFloat(column) + 0.5) * step - diameter / 2,
                                        y: (CGFloat(row) + 0.5) * step - diameter / 2,
                                        width: diameter, height: diameter)
                                    context.fill(Path(ellipseIn: dot), with: .color(.black.opacity(0.40)))
                                }
                            }
                        }
                        .frame(width: size * 0.18, height: size * 0.18)
                    } else {
                        Text(label).font(.caption2.bold()).foregroundStyle(.white)
                    }
                }
                .offset(knobOffset)
        }
        .frame(width: size, height: size)
        .contentShape(Circle())
        .overlay {
            NativeStreamVirtualStickTouchSurface(
                deadZone: deadZone, followsFinger: followsFinger,
                changed: changed, pressed: pressed,
                offsetChanged: { knobOffset = $0 }
            )
        }
        .accessibilityLabel("\(label) thumbstick")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: changed(0, 1)
            case .decrement: changed(0, -1)
            @unknown default: break
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { changed(0, 0) }
        }
    }

}

#endif
