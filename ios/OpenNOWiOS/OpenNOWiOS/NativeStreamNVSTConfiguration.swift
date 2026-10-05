import Foundation

/// CloudMatch provisioning for the experimental native connection, shared by launch and hand-over.
enum NativeStreamNVSTConfiguration {
    static func endpoints(sessionObj: [String: Any], fallbackHost: String?) -> [String] {
        NvstRtspEndpoints.collect(connections: sessionObj["connectionInfo"] as? [[String: Any]] ?? [],
            fallbackHost: fallbackHost, allowsAssumedControlPort: false)
    }

    static func requestBody(_ body: [String: Any], enabled: Bool) -> [String: Any] {
        guard enabled, var request = body["sessionRequestData"] as? [String: Any] else { return body }
        var native = body
        request["secureRTSPSupported"] = true
        if let metadata = request["metaData"] as? [[String: String]] {
            request["metaData"] = metadata.filter { $0["key"] != "GSStreamerType" }
        }
        native["sessionRequestData"] = request
        return native
    }
}
