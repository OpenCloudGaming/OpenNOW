import Foundation

/// CloudMatch provisioning for the experimental native connection, shared by launch and hand-over.
enum NativeStreamNVSTConfiguration {
    static func endpoints(sessionObj: [String: Any], fallbackHost: String?) -> [String] {
        NvstRtspEndpoints.collect(connections: sessionObj["connectionInfo"] as? [[String: Any]] ?? [],
            fallbackHost: fallbackHost, allowsAssumedControlPort: false)
    }

    static func usesBrowserIdentity(enabled: Bool, streamingBaseURL: String?) -> Bool {
        guard !enabled, let host = streamingBaseURL.flatMap({ URL(string: $0)?.host })?.lowercased() else { return false }
        return ["cloudmatchbeta.nvidiagrid.net", "cloudmatch.nvidiagrid.net"].contains {
            host == $0 || host.hasSuffix("." + $0)
        }
    }

    static func requestHeaders(_ headers: [String: String], enabled: Bool, streamingBaseURL: String?) -> [String: String] {
        guard usesBrowserIdentity(enabled: enabled, streamingBaseURL: streamingBaseURL) else { return headers }
        var browser = headers
        browser["nv-client-streamer"] = "WEBRTC"
        browser["nv-client-type"] = "BROWSER"
        return browser
    }

    static func requestBody(_ body: [String: Any], enabled: Bool, streamingBaseURL: String? = nil) -> [String: Any] {
        guard var request = body["sessionRequestData"] as? [String: Any] else { return body }
        if !enabled {
            guard usesBrowserIdentity(enabled: enabled, streamingBaseURL: streamingBaseURL) else { return body }
            var browser = body
            request["clientPlatformName"] = "browser"
            request["enablePersistingInGameSettings"] = false
            browser["sessionRequestData"] = request
            return browser
        }
        var native = body
        request["secureRTSPSupported"] = true
        request["streamerVersion"] = "14"
        request["sdkVersion"] = "2.0"
        request["enhancedStreamMode"] = 0
        request["requestedAudioFormat"] = 0
        request["transport"] = NSNull()
        if let metadata = request["metaData"] as? [[String: String]] {
            request["metaData"] = metadata.filter { $0["key"] != "GSStreamerType" }
        }
        native["sessionRequestData"] = request
        return native
    }
}
