import Foundation

public struct StreamSessionLimitUpdate: Codable, Equatable, Sendable {
    public let remainingSeconds: Int
    public let presentDurationSeconds: Int?
    public let timerType: String

    public init?(remainingSeconds: Int, presentDurationSeconds: Int? = nil, timerType: String = "") {
        guard remainingSeconds > 0, remainingSeconds <= 86_400 else { return nil }
        self.remainingSeconds = remainingSeconds
        self.presentDurationSeconds = presentDurationSeconds.map { max(0, $0) }
        self.timerType = timerType
    }

    public static func parse(from data: Data) -> StreamSessionLimitUpdate? {
        guard let value = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return parse(from: value)
    }

    private static func parse(from value: Any) -> StreamSessionLimitUpdate? {
        if let dictionary = value as? [String: Any] { return parse(from: dictionary) }
        if let array = value as? [Any] { return array.lazy.compactMap(parse(from:)).first }
        if let text = value as? String,
           let data = text.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data) {
            return parse(from: json)
        }
        return nil
    }

    private static func parse(from dictionary: [String: Any]) -> StreamSessionLimitUpdate? {
        if isSessionLengthTimer(dictionary), let update = update(from: dictionary) ?? nestedTimerUpdate(from: dictionary) {
            return update
        }
        for key in ["message", "payload", "data", "eventData", "customMessage"] {
            if let value = dictionary[key], let update = parse(from: value) { return update }
        }
        return nil
    }

    private static func nestedTimerUpdate(from dictionary: [String: Any]) -> StreamSessionLimitUpdate? {
        guard let timerData = dictionary["timerData"] as? [String: Any] else { return nil }
        return update(from: timerData)
    }

    private static func update(from dictionary: [String: Any]) -> StreamSessionLimitUpdate? {
        guard let remainingSeconds = remainingSeconds(in: dictionary) else { return nil }
        let presentDurationSeconds = milliseconds(dictionary["presentDurationMS"]).map { Int(($0 / 1000.0).rounded()) }
        return StreamSessionLimitUpdate(
            remainingSeconds: remainingSeconds,
            presentDurationSeconds: presentDurationSeconds,
            timerType: string(dictionary["timerType"])
        )
    }

    private static func remainingSeconds(in dictionary: [String: Any]) -> Int? {
        for key in ["beforeEventMS", "remainingSessionLimitMs", "remainingSessionLimitMilliseconds", "sessionLimitRemainingMs", "sessionLimitRemainingMilliseconds"] {
            if let value = milliseconds(dictionary[key]) {
                let seconds = Int((value / 1000.0).rounded())
                if seconds > 0 && seconds <= 86_400 { return seconds }
            }
        }
        for key in ["timeRemaining", "remainingTime", "remainingTimeInSeconds", "remainingSessionTimeInSeconds", "sessionTimeRemainingInSeconds", "timeRemainingInSeconds", "remainingSessionLimitSeconds", "sessionLimitRemainingSeconds"] {
            if let value = number(dictionary[key]) {
                let seconds = Int(value.rounded())
                if seconds > 0 && seconds <= 86_400 { return seconds }
            }
        }
        for key in ["remainingTimeInMinutes", "remainingSessionTimeInMinutes", "sessionTimeRemainingInMinutes", "timeRemainingInMinutes", "remainingSessionLimitMinutes", "sessionLimitRemainingMinutes"] {
            if let value = number(dictionary[key]) {
                let seconds = Int((value * 60.0).rounded())
                if seconds > 0 && seconds <= 86_400 { return seconds }
            }
        }
        return nil
    }

    private static func isSessionLengthTimer(_ dictionary: [String: Any]) -> Bool {
        ["messageType", "type", "eventType"].contains { key in
            string(dictionary[key]).caseInsensitiveCompare("SESSION_LENGTH_TIMER") == .orderedSame
        }
    }

    private static func milliseconds(_ value: Any?) -> Double? { number(value) }

    private static func number(_ value: Any?) -> Double? {
        if let value = value as? NSNumber { return value.doubleValue }
        if let value = value as? String { return Double(value.trimmingCharacters(in: .whitespacesAndNewlines)) }
        return nil
    }

    static func string(_ value: Any?) -> String {
        if let value = value as? String { return value }
        if let value = value as? NSNumber { return value.stringValue }
        return ""
    }
}
