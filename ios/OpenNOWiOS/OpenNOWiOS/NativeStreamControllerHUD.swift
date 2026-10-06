import Foundation
import CoreGraphics

enum NativeStreamControllerShortcutAction: String, Codable, CaseIterable, Identifiable {
    case none, controls, stats
    var id: String { rawValue }
    var label: String {
        switch self { case .none: return "Unassigned"; case .controls: return "Stream HUD"; case .stats: return "Toggle stats" }
    }
}

struct NativeStreamControllerShortcuts: Codable, Equatable {
    var firstButton = ""
    var secondButton = ""
    var firstAction: NativeStreamControllerShortcutAction = .controls
    var secondAction: NativeStreamControllerShortcutAction = .stats
    func action(for button: String) -> NativeStreamControllerShortcutAction {
        guard !button.isEmpty else { return .none }
        if button == firstButton { return firstAction }
        if button == secondButton { return secondAction }
        return .none
    }
}

enum NativeStreamControllerHUDCommand: Equatable { case up, down, left, right, activate, back }
struct NativeStreamControllerHUDInput: Equatable {
    let id = UUID()
    let command: NativeStreamControllerHUDCommand
}

enum NativeStreamControllerHUDRouting {
    static func direction(x: Float, y: Float) -> NativeStreamControllerHUDCommand? {
        guard x.isFinite, y.isFinite, max(abs(x), abs(y)) >= 0.6 else { return nil }
        if abs(y) >= abs(x) { return y > 0 ? .up : .down }
        return x > 0 ? .right : .left
    }
    static func gameState(_ state: NativeStreamGamepadState, captured: Bool) -> NativeStreamGamepadState {
        guard captured, state.connected else { return state }
        return NativeStreamGamepadState(controllerId: state.controllerId, buttons: 0, leftTrigger: 0,
            rightTrigger: 0, leftStickX: 0, leftStickY: 0, rightStickX: 0, rightStickY: 0, connected: true)
    }
}

enum NativeStreamControllerShortcutCapture { static var learning = false }

#if canImport(GameController)
import GameController

enum NativeStreamControllerButtonNames {
    static func buttons(_ controller: GCController) -> [(String, GCControllerButtonInput)] {
        var seen: Set<ObjectIdentifier> = []
        // Prefer an exposed rear/paddle name when aliases refer to the same input.
        return controller.physicalInputProfile.buttons.sorted {
            func rear(_ name: String) -> Bool { name.lowercased().contains("back") || name.lowercased().contains("paddle") }
            if rear($0.key) != rear($1.key) { return rear($0.key) }
            return $0.key < $1.key
        }.compactMap { name, button in
            guard seen.insert(ObjectIdentifier(button)).inserted else { return nil }
            return (name, button)
        }
    }
}
#endif

#if os(iOS)
import SwiftUI

final class NativeStreamControllerButtonLearner: ObservableObject {
    @Published var status = ""
    @Published var listening = false
    private var restorations: [() -> Void] = []
    private var timeout: Task<Void, Never>?

    func start(_ learned: @escaping (String) -> Void) {
        stop()
        let controllers = GCController.controllers()
        guard !controllers.isEmpty else { status = "Connect the controller first."; return }
        listening = true
        NativeStreamControllerShortcutCapture.learning = true
        status = "Press the back button you want to assign…"
        for controller in controllers {
            controller.handlerQueue = .main
            for (name, button) in NativeStreamControllerButtonNames.buttons(controller) {
                let old = button.pressedChangedHandler
                restorations.append { [weak button] in button?.pressedChangedHandler = old }
                button.pressedChangedHandler = { [weak self] _, _, pressed in
                    guard pressed, let self, self.listening else { return }
                    self.stop()
                    self.status = "Detected: \(name)"
                    learned(name)
                }
            }
        }
        timeout = Task { @MainActor [weak self] in
            do { try await Task.sleep(nanoseconds: 10_000_000_000) } catch { return }
            guard let self else { return }
            self.stop()
            self.status = "No button detected. Assign the back button to an unused controller input and try again."
        }
    }
    func stop() {
        timeout?.cancel(); timeout = nil
        restorations.forEach { $0() }; restorations.removeAll()
        listening = false
        NativeStreamControllerShortcutCapture.learning = false
        status = ""
    }
}

struct NativeStreamControllerShortcutsView: View {
    @Binding var settings: AppSettings
    @StateObject private var learner = NativeStreamControllerButtonLearner()
    @Environment(\.scenePhase) private var scenePhase
    var body: some View {
        Form {
            shortcut("Back button 1", first: true)
            shortcut("Back button 2", first: false)
            if !learner.status.isEmpty {
                Section {
                    Text(learner.status).foregroundStyle(.secondary)
                    if learner.listening { Button("Cancel Learning") { learner.stop() } }
                }
            }
            Section {
                DisclosureGroup("How shortcuts work") {
                    Text("Choose Learn Button, then press the button on your controller.")
                    Text("Mirrored buttons share a shortcut. Assign an unused controller input to keep it separate.")
                    Text("In the stream HUD, use the D-pad or left stick to move, A to select, B to go back, and left/right to adjust sliders.")
                }
                .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Controller Shortcuts")
        .navigationBarTitleDisplayMode(.inline)
        .buttonStyle(.borderless)
        .onDisappear { learner.stop() }
        .onChange(of: scenePhase) { phase in if phase != .active { learner.stop() } }
    }
    private func shortcut(_ title: String, first: Bool) -> some View {
        let name = first ? settings.controllerShortcuts.firstButton : settings.controllerShortcuts.secondButton
        return Section {
            LabeledContent("Assigned button", value: name.isEmpty ? "None" : name)
            Picker("Action", selection: Binding(get: {
                first ? settings.controllerShortcuts.firstAction : settings.controllerShortcuts.secondAction
            }, set: { action in
                if first { settings.controllerShortcuts.firstAction = action }
                else { settings.controllerShortcuts.secondAction = action }
            })) {
                ForEach(NativeStreamControllerShortcutAction.allCases) { Text($0.label).tag($0) }
            }
            HStack {
                Button("Learn Button") {
                    learner.start { detected in
                        if first { settings.controllerShortcuts.firstButton = detected }
                        else { settings.controllerShortcuts.secondButton = detected }
                        if first && settings.controllerShortcuts.secondButton == detected { settings.controllerShortcuts.secondButton = "" }
                        if !first && settings.controllerShortcuts.firstButton == detected { settings.controllerShortcuts.firstButton = "" }
                    }
                }.disabled(learner.listening)
                Spacer()
                Button("Clear", role: .destructive) {
                    if first { settings.controllerShortcuts.firstButton = "" }
                    else { settings.controllerShortcuts.secondButton = "" }
                }.disabled(name.isEmpty || learner.listening)
            }
        } header: {
            Text(title)
        }
    }
}

@MainActor
final class NativeStreamControllerHUDNavigator: ObservableObject {
    struct Control {
        var enabled: Bool
        let activate: () -> Void
        let adjust: ((Double) -> Void)?
    }
    @Published var selected: UUID?
    private var controls: [UUID: Control] = [:]
    private var positions: [UUID: CGRect] = [:]
    func register(_ id: UUID, enabled: Bool, activate: @escaping () -> Void, adjust: ((Double) -> Void)?) {
        controls[id] = Control(enabled: enabled, activate: activate, adjust: adjust)
    }
    func remove(_ id: UUID) {
        controls[id] = nil; positions[id] = nil
        if selected == id { selected = nil }
    }
    func updatePositions(_ next: [UUID: CGRect]) {
        for (id, rect) in next where positions[id] == nil { positions[id] = rect }
    }
    func handle(_ command: NativeStreamControllerHUDCommand) {
        let ordered = controls.keys.filter { controls[$0]?.enabled == true && positions[$0] != nil }.sorted {
            let a = positions[$0]!, b = positions[$1]!
            if abs(a.midY - b.midY) < 8 { return a.midX < b.midX }
            return a.midY < b.midY
        }
        guard !ordered.isEmpty else { return }
        guard let current = selected, let index = ordered.firstIndex(of: current) else { selected = ordered.first; return }
        switch command {
        case .activate: controls[current]?.activate()
        case .left, .right:
            if let adjust = controls[current]?.adjust { adjust(command == .left ? -1 : 1) }
            else { selected = ordered[min(max(index + (command == .left ? -1 : 1), 0), ordered.count - 1)] }
        case .up: selected = ordered[max(index - 1, 0)]
        case .down: selected = ordered[min(index + 1, ordered.count - 1)]
        case .back: break
        }
    }
}

struct NativeStreamHUDPositions: PreferenceKey {
    static var defaultValue: [UUID: CGRect] = [:]
    static func reduce(value: inout [UUID: CGRect], nextValue: () -> [UUID: CGRect]) { value.merge(nextValue()) { _, next in next } }
}
private struct NativeStreamHUDNavigatorKey: EnvironmentKey {
    static let defaultValue: NativeStreamControllerHUDNavigator? = nil
}
extension EnvironmentValues {
    var controllerHUDNavigator: NativeStreamControllerHUDNavigator? {
        get { self[NativeStreamHUDNavigatorKey.self] }
        set { self[NativeStreamHUDNavigatorKey.self] = newValue }
    }
}

private struct NativeStreamHUDControl: ViewModifier {
    @Environment(\.controllerHUDNavigator) private var navigator
    let activate: () -> Void
    var adjust: ((Double) -> Void)?
    func body(content: Content) -> some View {
        if let navigator { content.modifier(NativeStreamActiveHUDControl(navigator: navigator, activate: activate, adjust: adjust)) }
        else { content }
    }
}
private struct NativeStreamActiveHUDControl: ViewModifier {
    @ObservedObject var navigator: NativeStreamControllerHUDNavigator
    @Environment(\.isEnabled) private var enabled
    @State private var id = UUID()
    let activate: () -> Void
    let adjust: ((Double) -> Void)?
    func body(content: Content) -> some View {
        content
            .id(id)
            .background(GeometryReader { proxy in Color.clear.preference(key: NativeStreamHUDPositions.self, value: [id: proxy.frame(in: .global)]) })
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.accentColor, lineWidth: navigator.selected == id ? 2 : 0).allowsHitTesting(false))
            .onAppear { navigator.register(id, enabled: enabled, activate: activate, adjust: adjust) }
            .onChange(of: enabled) { next in navigator.register(id, enabled: next, activate: activate, adjust: adjust) }
            .onDisappear { navigator.remove(id) }
    }
}
extension View {
    func controllerHUDControl(activate: @escaping () -> Void, adjust: ((Double) -> Void)? = nil) -> some View {
        modifier(NativeStreamHUDControl(activate: activate, adjust: adjust))
    }
}
#endif
