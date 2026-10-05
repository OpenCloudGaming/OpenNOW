import AudioToolbox
import Foundation

/// Decodes the seat's Opus audio to interleaved 32-bit float PCM.
///
/// macOS decodes Opus itself — `kAudioFormatOpus` is in `AudioFormatGetProperty`'s decodable list
/// and an `AudioConverter` from Opus to float PCM builds cleanly — so nothing has to be vendored
/// for this.
///
/// The seat sends 48 kHz stereo in 5 ms frames — a measured RTP timestamp step of 240 at ~200
/// packets a second — and that step is exactly the frame count, so it is passed in rather than
/// guessed.
///
/// The frame count has to be exact. `AudioConverterFillComplexBuffer` treats an input callback
/// that supplies no packet as end-of-stream and latches: asking for more frames than one packet
/// holds decodes the first packet and then returns nothing for every packet after it. Requesting
/// precisely one packet's worth means the callback is never asked past what it has.
public final class NvstOpusDecoder: @unchecked Sendable {
    public enum DecoderError: LocalizedError, Equatable, Sendable {
        case converterUnavailable(OSStatus)
        case decodeFailed(OSStatus)

        public var errorDescription: String? {
            switch self {
            case .converterUnavailable(let status): "Could not create an Opus decoder (status \(status))."
            case .decodeFailed(let status): "Opus decode failed (status \(status))."
            }
        }
    }

    public static let sampleRate: Double = 48000
    public static let channels: UInt32 = 2

    private let converter: AudioConverterRef
    let lock = NSLock()
    private var output: [Float]

    private var decodedPacketCount: UInt64 = 0
    private var decodedFrameCount: UInt64 = 0
    private var failedPacketCount: UInt64 = 0
    private var oversizedPacketCount: UInt64 = 0
    private var lastFailureStatus: OSStatus = 0
    /// Frames produced per packet, which is how the seat's real frame duration is observed rather
    /// than assumed.
    private var framesPerPacketCounts: [Int: UInt64] = [:]

    /// Every counter is written under `lock` on the decode queue and read from telemetry threads,
    /// so the reads take the lock too.
    public var decodedPackets: UInt64 { lock.lock(); defer { lock.unlock() }; return decodedPacketCount }
    public var decodedFrames: UInt64 { lock.lock(); defer { lock.unlock() }; return decodedFrameCount }
    public var failedPackets: UInt64 { lock.lock(); defer { lock.unlock() }; return failedPacketCount }
    public var oversizedPackets: UInt64 { lock.lock(); defer { lock.unlock() }; return oversizedPacketCount }
    public var lastFailure: OSStatus { lock.lock(); defer { lock.unlock() }; return lastFailureStatus }
    public var framesPerPacketSeen: [Int: UInt64] { lock.lock(); defer { lock.unlock() }; return framesPerPacketCounts }

    /// RFC 7845's identification header: magic, version, channel count, pre-skip, input sample
    /// rate, output gain, channel-mapping family.
    static func opusHeadCookie(channels: UInt8, sampleRate: UInt32) -> Data {
        var writer = NvstByteWriter(capacity: 19)
        writer.bytes(Data("OpusHead".utf8))
        writer.u8(1)
        writer.u8(channels)
        writer.zeroes(2)
        writer.u32LE(sampleRate)
        writer.zeroes(2)
        writer.u8(0)
        return writer.data
    }

    public let framesPerPacket: Int

    /// - Parameter framesPerPacket: samples per channel in each packet — the stream's RTP timestamp
    ///   step. 240 is 5 ms at 48 kHz, which is what the seat sends.
    public init(framesPerPacket: Int = 240) throws {
        self.framesPerPacket = framesPerPacket
        var source = AudioStreamBasicDescription(
            mSampleRate: Self.sampleRate,
            mFormatID: kAudioFormatOpus,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: UInt32(framesPerPacket),
            mBytesPerFrame: 0,
            mChannelsPerFrame: Self.channels,
            mBitsPerChannel: 0,
            mReserved: 0
        )
        var destination = AudioStreamBasicDescription(
            mSampleRate: Self.sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4 * Self.channels,
            mFramesPerPacket: 1,
            mBytesPerFrame: 4 * Self.channels,
            mChannelsPerFrame: Self.channels,
            mBitsPerChannel: 32,
            mReserved: 0
        )
        var created: AudioConverterRef?
        let status = AudioConverterNew(&source, &destination, &created)
        guard status == noErr, let created else { throw DecoderError.converterUnavailable(status) }
        converter = created

        // Without an `OpusHead` magic cookie the decoder rejects every packet with 'bada'
        // (`kAudioCodecBadDataError`) — including packets that reference libopus decodes without a
        // single error. Nothing in the AudioToolbox headers says a cookie is required; it simply is.
        let cookie = Self.opusHeadCookie(channels: UInt8(Self.channels), sampleRate: UInt32(Self.sampleRate))
        let cookieStatus = cookie.withUnsafeBytes { raw in
            AudioConverterSetProperty(created, kAudioConverterDecompressionMagicCookie,
                                      UInt32(raw.count), raw.baseAddress!)
        }
        guard cookieStatus == noErr else {
            AudioConverterDispose(created)
            throw DecoderError.converterUnavailable(cookieStatus)
        }
        output = [Float](repeating: 0, count: max(framesPerPacket, 1) * Int(Self.channels))
    }

    deinit { AudioConverterDispose(converter) }

    /// Returned by the input callback when the queue is dry. `AudioConverterFillComplexBuffer`
    /// treats a callback that supplies no packet *and reports success* as end-of-stream and latches
    /// — every later packet then decodes to nothing. Reporting an error instead leaves the
    /// converter's state intact, so the next packet resumes the stream. Opus also has a decoder
    /// delay, so producing one packet's worth of output genuinely needs more than one packet in
    /// hand; the callback being asked twice is normal, not a fault.
    static let noDataAvailable: OSStatus = -13579

    /// Holds packets waiting to be decoded, plus a stable buffer to hand the converter.
    ///
    /// The buffer matters: the converter keeps the pointer it is given until it is finished with
    /// the packet, so it cannot point into a `Data` whose storage the queue may move. Copying into
    /// an allocation the queue owns also lets each invocation consume exactly one packet — an
    /// earlier version removed the head and immediately re-inserted it, which happened to work only
    /// because the converter never asked twice within one call.
    final class Queue {
        var packets: [Data] = []
        var consumedPackets: UInt64 = 0
        /// The converter keeps this pointer for the whole fill, so it must outlive the input
        /// callback: a dedicated allocation instead of a pointer into a closure scope.
        let description = UnsafeMutablePointer<AudioStreamPacketDescription>.allocate(capacity: 1)
        let scratch: UnsafeMutablePointer<UInt8>
        let capacity: Int

        init(capacity: Int) {
            self.capacity = capacity
            scratch = UnsafeMutablePointer<UInt8>.allocate(capacity: capacity)
        }

        deinit {
            scratch.deallocate()
            description.deallocate()
        }
    }

    /// An Opus packet cannot exceed 1275 bytes per frame; the seat's measure about 51.
    let queue = Queue(capacity: 4096)

    /// Hands the converter the next queued packet. Static so it is not rebuilt on every call and
    /// `decode` stays readable; `context` is the unretained `Queue`.
    private static let inputProc: AudioConverterComplexInputDataProc = { _, packetCount, data, descriptions, context in
        let queue = Unmanaged<Queue>.fromOpaque(context!).takeUnretainedValue()
        guard !queue.packets.isEmpty else {
            packetCount.pointee = 0
            return NvstOpusDecoder.noDataAvailable
        }
        let next = queue.packets.removeFirst()
        queue.consumedPackets += 1
        let count = next.count
        next.copyBytes(to: queue.scratch, count: count)
        queue.description.pointee = AudioStreamPacketDescription(
            mStartOffset: 0,
            mVariableFramesInPacket: 0,
            mDataByteSize: UInt32(count)
        )
        data.pointee.mBuffers.mData = UnsafeMutableRawPointer(queue.scratch)
        data.pointee.mBuffers.mDataByteSize = UInt32(count)
        data.pointee.mBuffers.mNumberChannels = NvstOpusDecoder.channels
        data.pointee.mNumberBuffers = 1
        descriptions?.pointee = queue.description
        packetCount.pointee = 1
        return noErr
    }

    /// One converter pass into `output`. `frames` is in-out: capacity in, frames produced out.
    /// The caller already holds `lock`.
    private func fillLocked(frames: inout UInt32) -> OSStatus {
        var status: OSStatus = noErr
        output.withUnsafeMutableBufferPointer { buffer in
            var list = AudioBufferList(
                mNumberBuffers: 1,
                mBuffers: AudioBuffer(
                    mNumberChannels: Self.channels,
                    mDataByteSize: UInt32(buffer.count * 4),
                    mData: buffer.baseAddress
                )
            )
            status = AudioConverterFillComplexBuffer(
                converter,
                Self.inputProc,
                Unmanaged.passUnretained(queue).toOpaque(),
                &frames,
                &list,
                nil
            )
        }
        return status
    }

    /// Decodes one Opus packet, returning whatever interleaved stereo float samples became
    /// available. Early packets can legitimately yield nothing while the decoder fills.
    public func decode(_ packet: Data) throws -> [Float]? {
        guard !packet.isEmpty else { return nil }
        lock.lock()
        defer { lock.unlock() }
        // Truncating to fit the scratch buffer would hand the decoder garbage. Nothing this stream
        // sends approaches the capacity — the seat's packets measure ~51 bytes — so a packet this
        // big is dropped and counted rather than mangled.
        guard packet.count <= queue.capacity else {
            oversizedPacketCount += 1
            return nil
        }

        let consumedBefore = queue.consumedPackets
        queue.packets.append(packet)
        var produced: [Float] = []

        while true {
            var frames = UInt32(framesPerPacket)
            let status = fillLocked(frames: &frames)
            if frames > 0 {
                decodedFrameCount += UInt64(frames)
                framesPerPacketCounts[Int(frames), default: 0] += 1
                produced.append(contentsOf: output[0..<(Int(frames) * Int(Self.channels))])
            }
            if status == Self.noDataAvailable { break }
            if status != noErr {
                failedPacketCount += 1
                lastFailureStatus = status
                break
            }
            if frames == 0 { break }
        }
        // Count what the converter actually consumed. With the decoder's priming delay, counting
        // output chunks instead lags the input by a packet.
        decodedPacketCount += queue.consumedPackets - consumedBefore
        return produced.isEmpty ? nil : produced
    }

    public var framesPerPacketSummary: String {
        lock.lock()
        defer { lock.unlock() }
        // The storage directly, not `framesPerPacketSeen`: that accessor takes `lock` too, and
        // `NSLock` is not recursive, so going through it deadlocks this thread with the lock held.
        return framesPerPacketCounts.sorted { $0.value > $1.value }
            .prefix(4)
            .map { "\($0.key)x\($0.value)" }
            .joined(separator: ",")
    }
}
