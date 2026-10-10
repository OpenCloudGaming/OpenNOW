import SwiftUI

#if os(iOS) && canImport(WebRTC)
enum GeForceNOWTouchControl: String, CaseIterable, Identifiable {
    case lt, lb, rb, rt, back, hub, start
    case up, left, right, down, y, x, b, a
    case leftStick, rightStick, l3, r3

    var id: String { "geForceNOW.\(rawValue)" }
    var label: String {
        switch self {
        case .back: return "Back"
        case .hub: return "Stream controls"
        case .start: return "Start"
        case .up, .left, .right, .down: return "D-pad \(rawValue)"
        case .leftStick: return "Left stick"
        case .rightStick: return "Right stick"
        default: return rawValue.uppercased()
        }
    }
    var point: TouchControlPoint {
        switch self {
        case .lt: return .init(x: 0.07, y: 0.20)
        case .lb: return .init(x: 0.20, y: 0.20)
        case .rb: return .init(x: 0.80, y: 0.20)
        case .rt: return .init(x: 0.93, y: 0.20)
        case .back: return .init(x: 0.41, y: 0.07)
        case .hub: return .init(x: 0.50, y: 0.07)
        case .start: return .init(x: 0.59, y: 0.07)
        case .up: return .init(x: 0.135, y: 0.39)
        case .left: return .init(x: 0.07, y: 0.55)
        case .right: return .init(x: 0.20, y: 0.55)
        case .down: return .init(x: 0.135, y: 0.71)
        case .y: return .init(x: 0.865, y: 0.39)
        case .x: return .init(x: 0.80, y: 0.55)
        case .b: return .init(x: 0.93, y: 0.55)
        case .a: return .init(x: 0.865, y: 0.71)
        case .leftStick: return .init(x: 0.26, y: 0.84)
        case .rightStick: return .init(x: 0.74, y: 0.84)
        case .l3: return .init(x: 0.07, y: 0.93)
        case .r3: return .init(x: 0.93, y: 0.93)
        }
    }
    var button: NativeStreamVirtualGamepadButton? {
        switch self {
        case .lb: return .leftShoulder
        case .rb: return .rightShoulder
        case .back: return .options
        case .start: return .menu
        case .up: return .dpadUp
        case .left: return .dpadLeft
        case .right: return .dpadRight
        case .down: return .dpadDown
        case .y: return .y
        case .x: return .x
        case .b: return .b
        case .a: return .a
        case .l3: return .leftStick
        case .r3: return .rightStick
        default: return nil
        }
    }
    var symbol: String? {
        switch self {
        case .back: return "arrowtriangle.left.fill"
        case .start: return "arrowtriangle.right.fill"
        case .up, .left, .right, .down: return "chevron.\(rawValue)"
        default: return nil
        }
    }
}

/// Screenshot-style layout, using the same input controls and editable geometry as other presets.
struct NativeStreamGeForceNOWControls: View {
    let inputBridge: NativeStreamInputBridge
    let layout: TouchControlLayout
    let settings: TouchSettings
    let editing: Bool
    let onPositionChange: (String, TouchControlPoint) -> Void
    let onOpenHub: () -> Void

    var body: some View {
        GeometryReader { proxy in
            let buttonSize = max(44, min(110, proxy.size.height * 0.15)) * layout.buttonScale
            let stickSize = max(96, min(230, proxy.size.height * 0.32)) * layout.stickScale
            ZStack {
                ForEach(GeForceNOWTouchControl.allCases) { control in
                    let point = layout.independentPositions[control.id] ?? control.point
                    NativeStreamPositionedControlGroup(
                        label: control.label, point: point, containerSize: proxy.size,
                        safeAreaInsets: proxy.safeAreaInsets, scale: layout.scale, opacity: layout.opacity,
                        edgePadding: settings.edgePadding, bottomPadding: settings.bottomPadding,
                        sideOffset: settings.sideOffset(for: point), editing: editing,
                        onPositionChange: { onPositionChange(control.id, $0) }
                    ) {
                        controlView(control, buttonSize: buttonSize, stickSize: stickSize)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func controlView(_ control: GeForceNOWTouchControl, buttonSize: CGFloat, stickSize: CGFloat) -> some View {
        switch control {
        case .hub:
            Button(action: onOpenHub) {
                GeForceNOWHubIcon()
                    .fill(Color.white.opacity(0.70), style: FillStyle(eoFill: true))
                    .frame(width: buttonSize * 0.32, height: buttonSize * 0.32)
                    .frame(width: buttonSize * 0.78, height: buttonSize * 0.78)
                    .background(Color.black.opacity(0.18), in: Circle())
                    .overlay(Circle().stroke(Color.white.opacity(0.42), lineWidth: 1))
            }
            .buttonStyle(.plain).accessibilityLabel("Open stream controls")
        case .leftStick, .rightStick:
            NativeStreamVirtualStickView(
                label: control == .leftStick ? "Left" : "Right", size: stickSize,
                deadZone: settings.joystickDeadZone, followsFinger: settings.joystickMode == .dynamic,
                outlineStyle: true, gripDots: true, concentricRings: true,
                changed: { x, y in inputBridge.setVirtualStick(control == .leftStick ? .left : .right, x: x, y: y) },
                pressed: { _ in }
            )
        default:
            NativeStreamVirtualHoldButton(
                label: control.label, systemImage: control.symbol, size: buttonSize,
                width: control == .back || control == .start ? buttonSize : nil,
                height: control == .back || control == .start ? buttonSize * 0.5 : nil,
                outlineStyle: true,
                pressed: { pressed in
                    if control == .lt || control == .rt {
                        inputBridge.setVirtualTrigger(control == .lt ? .left : .right, value: pressed ? 1 : 0)
                    } else if let button = control.button {
                        inputBridge.setVirtualButton(button, pressed: pressed)
                    }
                }
            )
        }
    }
}

/// Compact controller silhouette and downward indicator from the reference layout.
private struct GeForceNOWHubIcon: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: 0.15, y: 0.24))
        for point in [
            CGPoint(x: 0.22, y: 0.15), CGPoint(x: 0.26, y: 0.15),
            CGPoint(x: 0.26, y: 0.06), CGPoint(x: 0.36, y: 0.06),
            CGPoint(x: 0.36, y: 0.15), CGPoint(x: 0.63, y: 0.15),
            CGPoint(x: 0.63, y: 0.06), CGPoint(x: 0.73, y: 0.06),
            CGPoint(x: 0.73, y: 0.15), CGPoint(x: 0.77, y: 0.15),
            CGPoint(x: 0.84, y: 0.24), CGPoint(x: 0.96, y: 0.66)
        ] { path.addLine(to: point) }
        path.addQuadCurve(to: CGPoint(x: 0.86, y: 0.78), control: CGPoint(x: 0.99, y: 0.78))
        for point in [CGPoint(x: 0.63, y: 0.60), CGPoint(x: 0.37, y: 0.60), CGPoint(x: 0.14, y: 0.78)] {
            path.addLine(to: point)
        }
        path.addQuadCurve(to: CGPoint(x: 0.04, y: 0.66), control: CGPoint(x: 0.01, y: 0.78))
        path.closeSubpath()

        // The directional cross and face button are cut out of the silhouette.
        path.move(to: CGPoint(x: 0.25, y: 0.25))
        for point in [
            CGPoint(x: 0.33, y: 0.25), CGPoint(x: 0.33, y: 0.33),
            CGPoint(x: 0.40, y: 0.33), CGPoint(x: 0.40, y: 0.43),
            CGPoint(x: 0.33, y: 0.43), CGPoint(x: 0.33, y: 0.51),
            CGPoint(x: 0.25, y: 0.51), CGPoint(x: 0.25, y: 0.43),
            CGPoint(x: 0.18, y: 0.43), CGPoint(x: 0.18, y: 0.33),
            CGPoint(x: 0.25, y: 0.33)
        ] { path.addLine(to: point) }
        path.closeSubpath()
        path.addEllipse(in: CGRect(x: 0.65, y: 0.27, width: 0.19, height: 0.19))

        path.move(to: CGPoint(x: 0.34, y: 0.86))
        path.addLine(to: CGPoint(x: 0.66, y: 0.86))
        path.addLine(to: CGPoint(x: 0.50, y: 1))
        path.closeSubpath()
        return path.applying(CGAffineTransform(scaleX: rect.width, y: rect.height)
            .concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY)))
    }
}
#endif
