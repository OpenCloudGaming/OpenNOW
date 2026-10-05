import SwiftUI
import UIKit

/// A presented controller owns the system's pointer preference. An embedded SwiftUI
/// video view cannot override the preference of the app's root hosting controller.
struct NativeStreamPresentation<Content: View>: UIViewControllerRepresentable {
    @Environment(\.scenePhase) private var scenePhase
    let content: Content

    init(@ViewBuilder content: () -> Content) { self.content = content() }

    func makeUIViewController(context: Context) -> NativeStreamPresentationController {
        NativeStreamPresentationController(content: AnyView(content.environment(\.scenePhase, scenePhase)))
    }

    func updateUIViewController(_ controller: NativeStreamPresentationController, context: Context) {
        controller.host.rootView = AnyView(content.environment(\.scenePhase, scenePhase))
    }

    static func dismantleUIViewController(_ controller: NativeStreamPresentationController, coordinator: ()) {
        controller.tearDown()
    }
}

final class NativeStreamPresentationController: UIViewController {
    let host: NativeStreamHostingController
    private var dismantled = false

    init(content: AnyView) {
        host = NativeStreamHostingController(rootView: content)
        super.init(nibName: nil, bundle: nil)
        host.modalPresentationStyle = .fullScreen
        host.isModalInPresentation = true
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func loadView() {
        view = UIView()
        view.backgroundColor = .black
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !dismantled, host.presentingViewController == nil else { return }
        present(host, animated: false)
    }

    func tearDown() {
        dismantled = true
        host.pointerCaptureRequested = false
        if host.presentingViewController != nil { host.dismiss(animated: false) }
    }
}

final class NativeStreamHostingController: UIHostingController<AnyView> {
    var pointerCaptureRequested = false {
        didSet {
            guard oldValue != pointerCaptureRequested else { return }
            setNeedsUpdateOfPrefersPointerLocked()
        }
    }

    override var prefersPointerLocked: Bool { pointerCaptureRequested }
}

struct NativeStreamPointerLockPreference: UIViewControllerRepresentable {
    let requested: Bool

    func makeUIViewController(context: Context) -> PreferenceController {
        PreferenceController()
    }

    func updateUIViewController(_ controller: PreferenceController, context: Context) {
        controller.requested = requested
        controller.applyPreference()
    }

    static func dismantleUIViewController(_ controller: PreferenceController, coordinator: ()) {
        controller.requested = false
        controller.applyPreference()
    }

    final class PreferenceController: UIViewController {
        var requested = false

        override func loadView() {
            view = UIView()
            view.isUserInteractionEnabled = false
        }

        override func didMove(toParent parent: UIViewController?) {
            super.didMove(toParent: parent)
            applyPreference()
        }

        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            applyPreference()
        }

        func applyPreference() {
            var ancestor = parent
            while let controller = ancestor {
                if let host = controller as? NativeStreamHostingController {
                    host.pointerCaptureRequested = requested
                    return
                }
                ancestor = controller.parent
            }
        }
    }
}

enum NativeStreamPointerCapturePolicy {
    static func shouldCapture(videoActive: Bool, sceneActive: Bool, controlsVisible: Bool,
                              editing: Bool, guidanceVisible: Bool, pipActive: Bool) -> Bool {
        videoActive && sceneActive && !controlsVisible && !editing && !guidanceVisible && !pipActive
    }
}
