import Foundation

/// The seat's surround configuration notification, control command `0x0408`.
///
/// `libBifrost2` logs it as `STREAMER_AUDIO_SURROUND_INFO channels: %d, streams: %d,
/// coupled_streams: %d, bUseMultiMappingMode: %d`, and its Opus setup reads the mapping from the
/// same message ("Got the right opus info from server, surround mode, channels %u mapping %s").
/// The payload is read as consecutive little-endian words in that log order, followed by the
/// channel mapping as one byte per channel; the word layout is inferred from the log line, not
/// captured, so a mismatch with what the bundle was built for is reported rather than acted on.
public struct NvstAudioSurroundInfo: Equatable, Sendable {
    public static let commandCode = NvstControlCommandCode.audioSurroundInfo

    public let channels: UInt32
    public let streams: UInt32
    public let coupledStreams: UInt32
    public let usesMultiMappingMode: Bool
    public let channelMapping: [UInt8]

    public init(channels: UInt32, streams: UInt32, coupledStreams: UInt32, usesMultiMappingMode: Bool, channelMapping: [UInt8]) {
        self.channels = channels
        self.streams = streams
        self.coupledStreams = coupledStreams
        self.usesMultiMappingMode = usesMultiMappingMode
        self.channelMapping = channelMapping
    }

    /// Nil when `command` is not the surround notification or too short to hold the channel word.
    public static func parse(_ command: NvstControlCommand) -> NvstAudioSurroundInfo? {
        guard command.code == commandCode else { return nil }
        var reader = NvstByteReader(command.payload)
        guard let channels = try? reader.u32LE(), channels <= 8 else { return nil }
        let streams = (try? reader.u32LE()) ?? 0
        let coupled = (try? reader.u32LE()) ?? 0
        let multiMapping = (try? reader.u32LE()) ?? 0
        var mapping: [UInt8] = []
        for _ in 0..<Int(channels) {
            guard let value = try? reader.u8() else { break }
            mapping.append(value)
        }
        return NvstAudioSurroundInfo(channels: channels, streams: streams, coupledStreams: coupled,
                                     usesMultiMappingMode: multiMapping != 0, channelMapping: mapping)
    }

    public var summary: String {
        "channels=\(channels) streams=\(streams) coupled=\(coupledStreams) multiMapping=\(usesMultiMappingMode)"
            + (channelMapping.isEmpty ? "" : " mapping=\(channelMapping.map(String.init).joined(separator: ","))")
    }
}
