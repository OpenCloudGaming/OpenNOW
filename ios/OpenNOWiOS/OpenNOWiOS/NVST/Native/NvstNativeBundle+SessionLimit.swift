import Foundation

extension NvstNativeBundle {
    /// Parses the seat's `0x0103` session-limit timer and fans it out. Returns whether the command
    /// was that timer, so the main dispatch can stop there.
    func handleSessionLimitCommand(_ command: NvstControlCommand) -> Bool {
        guard command.code == .terminationTimer,
              let update = StreamSessionLimitUpdate.parse(from: command.payload) else { return false }
        onSessionLimitUpdate?(update)
        return true
    }
}
