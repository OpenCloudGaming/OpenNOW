import Foundation

/// The microphone send path: interleaved PCM in, protected RTP datagrams out.
///
/// Capture and gain are here rather than in the CoreAudio layer so the whole up-path can be
/// exercised without a device: hand it PCM, get back what would go on the wire. Muting is a gate
/// rather than a silence generation — a muted microphone stops producing packets, which is what the
/// seat's jitter buffer conceals most cheaply, and it is also what makes "the mic is off" observable
/// in the counters rather than only audible.
public final class NvstAudioSendPipeline: @unchecked Sendable {
    public struct Counters: Equatable, Sendable {
        public var packetsSent: UInt64 = 0
        public var framesEncoded: UInt64 = 0
        public var muteDrops: UInt64 = 0
        public var encodeFailures: UInt64 = 0
        public var protectFailures: UInt64 = 0
        public var bytesSent: UInt64 = 0
    }

    public static let microphoneSSRC: UInt32 = 1
    public static let opusPayloadType: UInt8 = 111
    public static let clockRate: UInt32 = 48000

    public let channels: Int
    public let framesPerPacket: Int
    public let ssrc: UInt32
    public let payloadType: UInt8

    /// True while capture is gated. Push-to-talk and voice-activity both drive this.
    public var isMuted: Bool {
        get { lock.withLock { isCaptureMuted } }
        set {
            lock.withLock {
                isCaptureMuted = newValue
                if newValue { pendingSamples.removeAll(keepingCapacity: true) }
            }
        }
    }
    /// Applied to the PCM before encoding, 0…1.
    public var gain: Float {
        get { lock.withLock { captureGain } }
        set { lock.withLock { captureGain = min(max(newValue.isFinite ? newValue : 1, 0), 1) } }
    }

    private let srtp: NvstAudioSrtp
    private let lock = NSLock()
    private var isCaptureMuted = false
    private var captureGain: Float = 1
    private var pendingSamples: [Float] = []
    private let encoder: NvstOpusEncoder
    private var packetizer: NvstAudioRtpPacketizer
    private var counters = Counters()
    private var rolloverCounter: UInt32 = 0
    private var lastSequence: UInt16?

    public init(srtp: NvstAudioSrtp,
                framesPerPacket: Int = 240,
                channels: Int = 2,
                ssrc: UInt32 = microphoneSSRC,
                payloadType: UInt8 = opusPayloadType,
                initialSequenceNumber: UInt16,
                initialTimestamp: UInt32) throws {
        self.srtp = srtp
        self.framesPerPacket = framesPerPacket
        self.channels = channels
        self.ssrc = ssrc
        self.payloadType = payloadType
        self.encoder = try NvstOpusEncoder(channels: UInt32(channels), framesPerPacket: framesPerPacket)
        self.packetizer = NvstAudioRtpPacketizer(ssrc: ssrc,
                                                 payloadType: payloadType,
                                                 initialSequenceNumber: initialSequenceNumber,
                                                 initialTimestamp: initialTimestamp)
    }

    public var snapshot: Counters { lock.withLock { counters } }

    /// The current RTP timestamp, which is what the next packet will carry.
    public var timestamp: UInt32 { lock.withLock { packetizer.timestamp } }

    public func push(capturedPCM samples: [Float]) -> [Data] {
        lock.lock()
        defer { lock.unlock() }
        guard !isCaptureMuted else {
            counters.muteDrops += 1
            return []
        }
        guard channels > 0, framesPerPacket > 0, samples.count.isMultiple(of: channels) else { return [] }
        pendingSamples.append(contentsOf: samples)
        let packetSamples = framesPerPacket * channels
        var consumed = 0
        var datagrams: [Data] = []
        while pendingSamples.count - consumed >= packetSamples {
            let frame = Array(pendingSamples[consumed..<consumed + packetSamples])
            consumed += packetSamples
            if let datagram = encodePacket(frame) { datagrams.append(datagram) }
        }
        pendingSamples.removeFirst(consumed)
        return datagrams
    }

    private func encodePacket(_ samples: [Float]) -> Data? {
        let scaled = applyGain(samples)
        let encoded: Data?
        do {
            encoded = try encoder.encode(scaled)
        } catch {
            counters.encodeFailures += 1
            return nil
        }
        // Nil here means the encoder is still priming, not that anything failed.
        guard let encoded, !encoded.isEmpty else { return nil }
        let sequence = packetizer.sequenceNumber
        let rtp = packetizer.packet(payload: encoded, framesPerPacket: framesPerPacket)
        advanceRolloverCounter(sequence)
        do {
            let datagram = try srtp.protect(rtp, rolloverCounter: rolloverCounter)
            counters.packetsSent += 1
            counters.framesEncoded += UInt64(framesPerPacket)
            counters.bytesSent &+= UInt64(datagram.count)
            return datagram
        } catch {
            counters.protectFailures += 1
            return nil
        }
    }

    /// Resets the packet clock; a new session must not resume a sequence the seat has already seen.
    public func reset(initialSequenceNumber: UInt16, initialTimestamp: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        packetizer = NvstAudioRtpPacketizer(ssrc: ssrc,
                                            payloadType: payloadType,
                                            initialSequenceNumber: initialSequenceNumber,
                                            initialTimestamp: initialTimestamp)
        lastSequence = nil
        rolloverCounter = 0
        pendingSamples.removeAll(keepingCapacity: true)
    }

    private func applyGain(_ samples: [Float]) -> [Float] {
        guard captureGain < 1 else { return samples }
        return samples.map { $0 * captureGain }
    }

    /// Mirrors the receive side: the SRTP index needs the sequence wrap, or the IV repeats and the
    /// seat rejects everything after it.
    private func advanceRolloverCounter(_ sequenceNumber: UInt16) {
        guard let previous = lastSequence else {
            lastSequence = sequenceNumber
            return
        }
        if sequenceNumber < previous, previous - sequenceNumber > 0x8000 {
            rolloverCounter &+= 1
        }
        lastSequence = sequenceNumber
    }
}
