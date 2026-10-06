import SwiftUI
import UIKit

/// A full-screen controller owns the stream's pointer and status bar preferences.
/// An embedded SwiftUI video view cannot override the app's root controller.
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
        host.statusBarHidden = false
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

    var statusBarHidden = false {
        didSet {
            guard oldValue != statusBarHidden else { return }
            // SwiftUI applies this during a view update. Invalidate after that transaction
            // so UIKit observes the new preference on repeated show/hide changes.
            DispatchQueue.main.async { [weak self] in self?.setNeedsStatusBarAppearanceUpdate() }
        }
    }

    override var prefersStatusBarHidden: Bool { statusBarHidden }

    // The full-screen container owns this preference, independent of SwiftUI children.
    override var childForStatusBarHidden: UIViewController? { nil }
}

struct NativeStreamPresentationPreferences: UIViewControllerRepresentable {
    let pointerCaptureRequested: Bool
    let statusBarHidden: Bool

    func makeUIViewController(context: Context) -> PreferenceController {
        PreferenceController()
    }

    func updateUIViewController(_ controller: PreferenceController, context: Context) {
        controller.pointerCaptureRequested = pointerCaptureRequested
        controller.statusBarHidden = statusBarHidden
        controller.applyPreferences()
    }

    static func dismantleUIViewController(_ controller: PreferenceController, coordinator: ()) {
        controller.pointerCaptureRequested = false
        controller.statusBarHidden = false
        controller.applyPreferences()
    }

    final class PreferenceController: UIViewController {
        var pointerCaptureRequested = false
        var statusBarHidden = false

        override func loadView() {
            view = UIView()
            view.isUserInteractionEnabled = false
        }

        override func didMove(toParent parent: UIViewController?) {
            super.didMove(toParent: parent)
            applyPreferences()
        }

        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            applyPreferences()
        }

        func applyPreferences() {
            var ancestor = parent
            while let controller = ancestor {
                if let host = controller as? NativeStreamHostingController {
                    host.pointerCaptureRequested = pointerCaptureRequested
                    host.statusBarHidden = statusBarHidden
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
