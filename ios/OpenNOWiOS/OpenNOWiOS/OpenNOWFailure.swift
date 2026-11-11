import Foundation

/// A failure the user can be told about, with the one thing they can do next.
///
/// Raw `localizedDescription` should never reach the screen: it is written for a developer reading
/// a stack trace, not for someone whose game will not start. Classifying first means the message
/// can name the actual cause, and — more usefully — offer the action that fixes it, which is the
/// difference between "Session launch failed: The operation couldn't be completed" and "All rigs
/// in this region are busy → Change server".
struct OpenNOWFailure: Identifiable, Equatable {
    enum Kind: String, Equatable {
        case offline
        case timeout
        case authExpired
        case notEntitled
        case sessionLimit
        case capacity
        case maintenance
        case transportLost
        case rejected
        case unknown
    }

    /// What the user can do. The view decides how to render it; the store decides what it does.
    enum Recovery: Equatable {
        case retry
        case signIn
        case changeServer
        case seePlans
        case manageSessions
        case reportProblem
        case none

        var label: String? {
            switch self {
            case .retry: return "Try Again"
            case .signIn: return "Sign In"
            case .changeServer: return "Change Server"
            case .seePlans: return "See Plans"
            case .manageSessions: return "Manage Sessions"
            case .reportProblem: return "Report a Problem"
            case .none: return nil
            }
        }
    }

    let kind: Kind
    let message: String
    let recovery: Recovery
    /// Shown small and selectable so it can be pasted into a report. Never the whole message.
    let code: String?

    var id: String { "\(kind.rawValue)|\(message)" }

    // MARK: Classification

    static func classify(_ error: Error, context: Context) -> OpenNOWFailure {
        let nsError = error as NSError
        let raw = nsError.localizedDescription

        if let limit = error as? CloudMatchSessionLimitError {
            return OpenNOWFailure(
                kind: .sessionLimit,
                message: "Another cloud session is already running or queued. Resume it or end it before starting a new one.",
                recovery: .manageSessions,
                code: limit.code
            )
        }
        if let object = jsonObject(in: raw), CloudMatchSessionLimitError.isSessionLimit(object) {
            return classify(CloudMatchSessionLimitError(sessions: [], code: CloudMatchSessionLimitError.code(in: object)), context: context)
        }

        if nsError.domain == NSURLErrorDomain {
            switch nsError.code {
            case NSURLErrorNotConnectedToInternet, NSURLErrorDataNotAllowed:
                return OpenNOWFailure(
                    kind: .offline,
                    message: "No internet connection.",
                    recovery: .retry,
                    code: nil
                )
            case NSURLErrorTimedOut:
                return OpenNOWFailure(
                    kind: .timeout,
                    message: "The server took too long to answer.",
                    recovery: .retry,
                    code: nil
                )
            case NSURLErrorNetworkConnectionLost, NSURLErrorCannotConnectToHost:
                return OpenNOWFailure(
                    kind: .transportLost,
                    message: "Lost the connection partway through.",
                    recovery: .retry,
                    code: nil
                )
            default:
                break
            }
        }

        // HTTP status carried through the app's own error domains.
        switch nsError.code {
        case 401:
            return OpenNOWFailure(
                kind: .authExpired,
                message: "Your session expired. Sign in again.",
                recovery: .signIn,
                code: nil
            )
        case 403 where raw.localizedCaseInsensitiveContains("token"):
            return OpenNOWFailure(
                kind: .authExpired,
                message: "Your session expired. Sign in again.",
                recovery: .signIn,
                code: nil
            )
        case 402:
            return OpenNOWFailure(
                kind: .notEntitled,
                message: serverMessage(raw) ?? "Your membership doesn't include this.",
                recovery: .seePlans,
                code: nil
            )
        case 403:
            let reason = jsonObject(in: raw).flatMap(preferredMessage) ?? raw
            if reason.localizedCaseInsensitiveContains("not_entitled")
                || reason.localizedCaseInsensitiveContains("not entitled")
                || reason.localizedCaseInsensitiveContains("membership") {
                return OpenNOWFailure(
                    kind: .notEntitled,
                    message: serverMessage(raw) ?? "Your membership doesn't include this.",
                    recovery: .seePlans,
                    code: nil
                )
            }
            return OpenNOWFailure(
                kind: .rejected,
                message: serverMessage(raw) ?? "The server refused this request.",
                recovery: .reportProblem,
                code: "403"
            )
        case 503:
            return OpenNOWFailure(
                kind: .capacity,
                message: serverMessage(raw) ?? "All rigs in this region are busy right now.",
                recovery: context == .launch ? .changeServer : .retry,
                code: nil
            )
        default:
            break
        }

        if raw.localizedCaseInsensitiveContains("maintenance") {
            return OpenNOWFailure(
                kind: .maintenance,
                message: serverMessage(raw) ?? "GeForce NOW is under maintenance.",
                recovery: .none,
                code: nil
            )
        }
        if raw.localizedCaseInsensitiveContains("invalid_grant")
            || raw.localizedCaseInsensitiveContains("login_required") {
            return OpenNOWFailure(
                kind: .authExpired,
                message: "Your session expired. Sign in again.",
                recovery: .signIn,
                code: nil
            )
        }

        // Anything the server actually explained is better than anything invented here.
        if let explained = serverMessage(raw) {
            return OpenNOWFailure(
                kind: .rejected,
                message: explained,
                recovery: .retry,
                code: nsError.code > 0 ? String(nsError.code) : nil
            )
        }

        return OpenNOWFailure(
            kind: .unknown,
            message: context.unknownMessage,
            recovery: .reportProblem,
            code: nsError.code > 0 ? "\(nsError.domain) \(nsError.code)" : nil
        )
    }

    /// Where the failure happened, which decides both the fallback wording and which recovery
    /// makes sense — "Change server" is meaningless outside a launch.
    enum Context: Equatable {
        case launch
        case session
        case catalog
        case account

        var unknownMessage: String {
            switch self {
            case .launch: return "The game couldn't start."
            case .session: return "Something went wrong with the session."
            case .catalog: return "Couldn't load the catalog."
            case .account: return "Couldn't reach your account."
            }
        }
    }

    /// A sentence the *server* wrote, or nil.
    ///
    /// Deliberately only reads JSON. An `NSError` with no user info still has a
    /// `localizedDescription` — "The operation couldn't be completed. (Domain error 918.)" — and an
    /// earlier version of this passed that straight through as though the server had said it,
    /// which is exactly the developer-facing text this whole type exists to keep off the screen.
    private static func serverMessage(_ raw: String) -> String? {
        guard let object = jsonObject(in: raw),
              let message = preferredMessage(in: object) else { return nil }
        let collapsed = message
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard !collapsed.isEmpty, collapsed.count <= 200 else { return nil }
        return collapsed
    }

    private static func jsonObject(in raw: String) -> [String: Any]? {
        guard let start = raw.firstIndex(of: "{"), let end = raw.lastIndex(of: "}"), start < end else {
            return nil
        }
        let candidate = String(raw[start...end])
        guard let data = candidate.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    /// Walks one level into a nested `error` object, which is where GFN and the report API both
    /// put the human sentence.
    private static func preferredMessage(in object: [String: Any]) -> String? {
        let keys = ["error_description", "errorMessage", "statusDescription", "message", "detail", "description"]
        for key in keys {
            if let value = object[key] as? String,
               !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                return value
            }
        }
        if let nested = object["error"] as? [String: Any] {
            return preferredMessage(in: nested)
        }
        if let nested = object["requestStatus"] as? [String: Any] {
            return preferredMessage(in: nested)
        }
        return nil
    }
}
