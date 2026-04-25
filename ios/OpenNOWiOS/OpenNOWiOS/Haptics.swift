#if os(tvOS)
enum Haptics {
    static func light() {}
    static func medium() {}
    static func selection() {}
}
#else
import UIKit

/// Taptic feedback only on iPhone (skips iPad and Mac Catalyst).
enum Haptics {
    static var isIPhone: Bool {
        UIDevice.current.userInterfaceIdiom == .phone
    }

    static func light() {
        guard isIPhone else { return }
        let gen = UIImpactFeedbackGenerator(style: .light)
        gen.prepare()
        gen.impactOccurred()
    }

    static func medium() {
        guard isIPhone else { return }
        let gen = UIImpactFeedbackGenerator(style: .medium)
        gen.prepare()
        gen.impactOccurred()
    }

    static func selection() {
        guard isIPhone else { return }
        let gen = UISelectionFeedbackGenerator()
        gen.prepare()
        gen.selectionChanged()
    }

    static func notify(_ type: UINotificationFeedbackGenerator.FeedbackType) {
        guard isIPhone else { return }
        let gen = UINotificationFeedbackGenerator()
        gen.prepare()
        gen.notificationOccurred(type)
    }
}
#endif
