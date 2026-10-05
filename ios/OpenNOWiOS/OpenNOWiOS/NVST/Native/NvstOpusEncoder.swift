import AudioToolbox
import Foundation

/// Encodes the microphone's PCM into the Opus the bundle's mic section carries.
///
/// macOS encodes Opus itself, exactly as it decodes it, so the microphone needs no vendored codec.
/// The packet shape follows `NvstOpusDecoder`'s: 48 kHz, 5 ms frames, which is the grid the seat's
/// own audio runs on.
public final class NvstOpusEncoder: @unchecked Sendable {
    public enum EncoderError: LocalizedError, Equatable, Sendable {
        case converterUnavailable(OSStatus)
        case encodeFailed(OSStatus)

        public var errorDescription: String? {
            switch self {
            case .converterUnavailable(let status): "Could not create an Opus encoder (status \(status))."
            case .encodeFailed(let status): "Opus encode failed (status \(status))."
            }
        }
    }

    public static let sampleRate: Double = 48000

    public let channels: UInt32
    public let framesPerPacket: Int
    public var samplesPerPacket: Int { framesPerPacket * Int(channels) }

    private let converter: AudioConverterRef
    let lock = NSLock()
    private var input: [Float]
    private var output: [UInt8]

    private var encodedPacketCount: UInt64 = 0
    private var encodedFrameCount: UInt64 = 0
    private var failedPacketCount: UInt64 = 0
    private var lastFailureStatus: OSStatus = 0

    public var encodedPackets: UInt64 { lock.lock(); defer { lock.unlock() }; return encodedPacketCount }
    public var encodedFrames: UInt64 { lock.lock(); defer { lock.unlock() }; return encodedFrameCount }
    public var failedPackets: UInt64 { lock.lock(); defer { lock.unlock() }; return failedPacketCount }
    public var lastFailure: OSStatus { lock.lock(); defer { lock.unlock() }; return lastFailureStatus }

    /// - Parameters:
    ///   - channels: 2 for the bundle's `opus/48000/2` mic section. A mono capture has to be
    ///     interleaved up to this before it is handed in.
    ///   - framesPerPacket: samples per channel per packet; 240 is 5 ms at 48 kHz.
    public init(channels: UInt32 = 2, framesPerPacket: Int = 240) throws {
        self.channels = max(1, channels)
        self.framesPerPacket = max(1, framesPerPacket)
        var source = AudioStreamBasicDescription(
            mSampleRate: Self.sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4 * self.channels,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4 * self.channels,
            mChannelsPerFrame: self.channels,
            mBitsPerChannel: 32,
            mReserved: 0
        )
        var destination = AudioStreamBasicDescription(
            mSampleRate: Self.sampleRate,
            mFormatID: kAudioFormatOpus,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: UInt32(self.framesPerPacket),
            mBytesPerFrame: 0,
            mChannelsPerFrame: self.channels,
            mBitsPerChannel: 0,
            mReserved: 0
        )
        var created: AudioConverterRef?
        let status = AudioConverterNew(&source, &destination, &created)
        guard status == noErr, let created else { throw EncoderError.converterUnavailable(status) }
        converter = created

        // The decoder needs an `OpusHead` cookie; the encoder rejects the same cookie with '!siz',
        // because it identifies the stream itself and has none to be told.
        input = [Float](repeating: 0, count: max(1, framesPerPacket) * Int(self.channels))
        output = [UInt8](repeating: 0, count: 4096)
    }

    deinit { AudioConverterDispose(converter) }

    /// Holds the PCM the converter is being fed, plus a stable description it may keep.
    final class Feed {
        var samples: [Float] = []
        var consumed = false
        var channels = 1
        let description = UnsafeMutablePointer<AudioStreamPacketDescription>.allocate(capacity: 1)

        deinit { description.deallocate() }
    }

    let feed = Feed()

    /// Supplies the queued PCM once. Reporting end-of-stream after that is correct here — the caller
    /// hands in exactly one packet's worth and expects exactly one packet back.
    private static let inputProc: AudioConverterComplexInputDataProc = { _, packetCount, data, descriptions, context in
        let feed = Unmanaged<Feed>.fromOpaque(context!).takeUnretainedValue()
        guard !feed.consumed, !feed.samples.isEmpty else {
            packetCount.pointee = 0
            return noErr
        }
        feed.consumed = true
        let channels = feed.channels
        let frames = UInt32(feed.samples.count / channels)
        feed.samples.withUnsafeMutableBufferPointer { buffer in
            data.pointee.mBuffers.mData = UnsafeMutableRawPointer(buffer.baseAddress)
            data.pointee.mBuffers.mDataByteSize = UInt32(buffer.count * MemoryLayout<Float>.size)
            data.pointee.mBuffers.mNumberChannels = UInt32(channels)
        }
        data.pointee.mNumberBuffers = 1
        feed.description.pointee = AudioStreamPacketDescription(
            mStartOffset: 0,
            mVariableFramesInPacket: frames,
            mDataByteSize: UInt32(feed.samples.count * MemoryLayout<Float>.size)
        )
        descriptions?.pointee = feed.description
        // The source is PCM with `mFramesPerPacket = 1`, so one input packet is one frame: report the
        // frame count. Reporting 1 made CoreAudio warn it had been handed 480 packets for 3840 bytes.
        packetCount.pointee = frames
        return noErr
    }

    /// Encodes exactly one packet's worth of interleaved samples, or returns nil while the encoder
    /// is priming. Fewer samples than a packet are ignored rather than padded.
    public func encode(_ samples: [Float]) throws -> Data? {
        guard samples.count >= samplesPerPacket else { return nil }
        lock.lock()
        defer { lock.unlock() }
        feed.samples = Array(samples[0..<samplesPerPacket])
        feed.channels = Int(channels)
        feed.consumed = false

        var packetCount: UInt32 = 1
        var descriptions = [AudioStreamPacketDescription](repeating: AudioStreamPacketDescription(), count: 1)
        var status: OSStatus = noErr
        output.withUnsafeMutableBufferPointer { buffer in
            var list = AudioBufferList(
                mNumberBuffers: 1,
                mBuffers: AudioBuffer(
                    mNumberChannels: channels,
                    mDataByteSize: UInt32(buffer.count),
                    mData: buffer.baseAddress
                )
            )
            status = AudioConverterFillComplexBuffer(
                converter,
                Self.inputProc,
                Unmanaged.passUnretained(feed).toOpaque(),
                &packetCount,
                &list,
                &descriptions
            )
        }
        guard status == noErr else {
            failedPacketCount += 1
            lastFailureStatus = status
            throw EncoderError.encodeFailed(status)
        }
        guard packetCount > 0 else { return nil }
        let description = descriptions[0]
        let start = Int(description.mStartOffset)
        let size = Int(description.mDataByteSize)
        guard size > 0, start + size <= output.count else { return nil }
        encodedPacketCount += 1
        encodedFrameCount += UInt64(framesPerPacket)
        return Data(output[start..<(start + size)])
    }
}
