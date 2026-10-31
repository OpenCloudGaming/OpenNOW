import Foundation

enum SessionAvailability: Equatable {
    case checking
    case available
    case unverified
}

struct RemoteSessionEndedError: LocalizedError {
    let status: Int?

    var errorDescription: String? {
        "This cloud session has ended. Start the game again when you're ready."
    }
}

struct SessionTerminationConfirmation {
    let generation: UUID
    let operation: UUID
    let activeID: String?
    let activeStatus: Int?
    let remoteStatuses: [String: Int]

    func matches(active: ActiveSession?, remote: [RemoteSessionCandidate]) -> Bool {
        activeID == active?.id && activeStatus == active?.status
            && remoteStatuses == Dictionary(remote.map { ($0.id, $0.status) }, uniquingKeysWith: { first, _ in first })
    }
}

enum SessionLifecyclePolicy {
    static func allocatedAppID(in session: [String: Any], fallback: String? = nil) -> String? {
        let value = (session["sessionRequestData"] as? [String: Any])?["appId"]
        if let value = value as? String {
            let value = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !value.isEmpty, value.utf8.count <= 256 { return value }
        } else if let value = SessionConnectionPolicy.integer(value), value > 0 {
            return String(value)
        }
        guard let fallback = fallback?.trimmingCharacters(in: .whitespacesAndNewlines),
              !fallback.isEmpty, fallback.utf8.count <= 256 else { return nil }
        return fallback
    }

    static func isTerminal(_ status: Int) -> Bool {
        status > 3 && status != 6
    }

    static func isLaunchable(_ status: Int) -> Bool {
        (1...3).contains(status)
    }

    static func isEnded(_ error: Error) -> Bool {
        if error is RemoteSessionEndedError { return true }
        let error = error as NSError
        return error.domain == "OpenNOW.Session" && [404, 410].contains(error.code)
    }

    static func sessionObject(in payload: [String: Any], httpStatus: Int) throws -> [String: Any] {
        guard httpStatus == 200 else {
            throw NSError(domain: "OpenNOW.Session", code: httpStatus, userInfo: [
                NSLocalizedDescriptionKey: "The cloud session status could not be checked."
            ])
        }
        let request = payload["requestStatus"] as? [String: Any]
        guard SessionConnectionPolicy.integer(request?["statusCode"]) == 1 else {
            throw NSError(domain: "OpenNOW.Session", code: SessionConnectionPolicy.integer(request?["statusCode"]) ?? -1,
                          userInfo: [NSLocalizedDescriptionKey: request?["statusDescription"] as? String
                            ?? "The cloud session status could not be checked."])
        }
        guard let session = payload["session"] as? [String: Any],
              let status = SessionConnectionPolicy.integer(session["status"]), status > 0 else {
            throw NSError(domain: "OpenNOW.Session", code: -1, userInfo: [
                NSLocalizedDescriptionKey: "The provider returned an incomplete session status."
            ])
        }
        if isTerminal(status) { throw RemoteSessionEndedError(status: status) }
        return session
    }
}
