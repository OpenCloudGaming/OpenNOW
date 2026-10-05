import AudioToolbox
import CoreAudio
import Foundation

/// The format decisions the native audio device makes, kept apart from the I/O so they can be
/// checked without a sound device.
///
/// Everything the seat's audio meets, and everything the microphone produces, is 48 kHz linear PCM
/// in signed 16-bit interleaved form: the same container the vendored device used, and the one
/// `AudioConverter` takes on both sides of Opus.
public enum NvstCoreAudioFormat {
    public static let sampleRate: Double = 48000

    /// 2, 6 or 8; anything else is stereo, matching what the bundle's surround SDP can express.
    public static func supportedPlayoutChannelCount(_ requested: Int) -> Int {
        switch requested {
        case 6, 8: requested
        default: 2
        }
    }

    /// What the device should ask the hardware for: a surround count is only used when the device can
    /// actually carry it. libwebrtc cannot fold a surround decode down to fewer channels, so handing
    /// a stereo device a six-channel count produces silence rather than a downmix.
    public static func playoutChannelCount(requested: Int, deviceChannels: Int) -> Int {
        let supported = supportedPlayoutChannelCount(requested)
        guard supported > 2, deviceChannels >= supported else {
            return max(1, min(2, deviceChannels))
        }
        return supported
    }

    public static func captureChannelCount(deviceChannels: Int) -> Int {
        max(1, min(2, deviceChannels))
    }

    public static func stereoCaptureSamples(bufferList: UnsafePointer<AudioBufferList>, frames: UInt32) -> [Float]? {
        guard frames > 0, bufferList.pointee.mNumberBuffers == 1 else { return nil }
        let buffer = bufferList.pointee.mBuffers
        let channels = Int(buffer.mNumberChannels)
        guard (1...2).contains(channels), let storage = buffer.mData,
              Int(buffer.mDataByteSize) / MemoryLayout<Int16>.size >= Int(frames) * channels else { return nil }
        let samples = storage.assumingMemoryBound(to: Int16.self)
        var stereo = [Float](repeating: 0, count: Int(frames) * 2)
        for frame in 0..<Int(frames) {
            stereo[frame * 2] = Float(samples[frame * channels]) / 32768
            stereo[frame * 2 + 1] = Float(samples[frame * channels + channels - 1]) / 32768
        }
        return stereo
    }

    /// Places decoded stereo samples into an interleaved 16-bit buffer of `outputChannels` channels:
    /// mono averages the pair, stereo keeps them, and any further channels stay silent so a hardware
    /// layout wider than the decode never receives shifted samples. The decode here is always stereo,
    /// so a surround device is filled from the front pair rather than misread as interleaved.
    public static func interleavedSamples(fromStereo samples: [Float], frames: Int, outputChannels: Int) -> [Int16] {
        let frameCount = max(0, frames)
        let channels = max(1, outputChannels)
        var output = [Int16](repeating: 0, count: frameCount * channels)
        for frame in 0..<frameCount {
            let left = frame * 2 < samples.count ? samples[frame * 2] : 0
            let right = frame * 2 + 1 < samples.count ? samples[frame * 2 + 1] : 0
            if channels == 1 {
                output[frame] = clamped16((left + right) / 2)
                continue
            }
            output[frame * channels] = clamped16(left)
            output[frame * channels + 1] = clamped16(right)
        }
        return output
    }

    private static func clamped16(_ value: Float) -> Int16 {
        let bounded: Float = value.isFinite ? value : (value > 0 ? 1 : (value < 0 ? -1 : 0))
        let scaled = bounded * Float(Int16.max)
        if scaled <= Float(Int16.min) { return Int16.min }
        if scaled >= Float(Int16.max) { return Int16.max }
        return Int16(scaled.rounded())
    }

    /// The IO buffer both directions ask for: 5 ms, the seat's own Opus frame. A device left at its
    /// default (512 frames, 10.7 ms at 48 kHz; some USB interfaces sit at 4096) adds that much to
    /// every sample's path, on top of the jitter buffer's dwell.
    public static let preferredIOBufferSeconds: TimeInterval = 0.005

    public static func preferredIOBufferFrames(sampleRate: Double) -> UInt32 {
        UInt32(max(1, (preferredIOBufferSeconds * (sampleRate > 0 ? sampleRate : sampleRate)).rounded()))
    }

    /// Clamps the preferred IO buffer into what the device actually offers, so a request the
    /// hardware cannot honour is at least the nearest one it can.
    public static func clampedIOBufferFrames(preferred: UInt32, deviceRange: AudioValueRange?) -> UInt32 {
        guard let deviceRange, deviceRange.mMaximum >= deviceRange.mMinimum, deviceRange.mMaximum > 0 else {
            return preferred
        }
        return min(max(preferred, UInt32(deviceRange.mMinimum)), UInt32(deviceRange.mMaximum))
    }

    public static func linear16Format(sampleRate: Double, channels: UInt32) -> AudioStreamBasicDescription {
        let channelCount = max(1, channels)
        return AudioStreamBasicDescription(
            mSampleRate: sampleRate > 0 ? sampleRate : sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
            mBytesPerPacket: channelCount * UInt32(MemoryLayout<Int16>.size),
            mFramesPerPacket: 1,
            mBytesPerFrame: channelCount * UInt32(MemoryLayout<Int16>.size),
            mChannelsPerFrame: channelCount,
            mBitsPerChannel: 16,
            mReserved: 0
        )
    }

    /// RMS of an interleaved Int16 buffer, normalised 0…1 and scaled the same way as the Settings
    /// microphone meter so the two read identically. Separate from the device so the level a user
    /// sees can be checked against known input.
    public static func level(of samples: UnsafePointer<Int16>, count: Int) -> Double {
        guard count > 0 else { return 0 }
        var sumSquares = 0.0
        for index in 0..<count {
            let sample = Double(samples[index]) / Double(Int16.max)
            sumSquares += sample * sample
        }
        return min(1, (sumSquares / Double(count)).squareRoot() * 6)
    }
}
