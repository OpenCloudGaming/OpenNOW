import CoreImage
import CoreMedia
import CoreVideo

/// PiP owns small, independent display surfaces instead of retaining the decoder's
/// 4K HDR IOSurfaces. Conversion runs only on admitted PiP frames, off the decode queue.
final class NativeStreamPiPFrameConverter {
    // iOS prohibits app-owned Metal work in the background. PiP conversion uses
    // Core Image's CPU renderer; the main video still uses hardware decode + Metal.
    private lazy var context = CIContext(options: [.cacheIntermediates: false, .useSoftwareRenderer: true])
    private let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private var pool: CVPixelBufferPool?
    private var dimensions = CGSize.zero

    static func outputSize(width: Int, height: Int) -> CGSize {
        guard width > 0, height > 0 else { return .zero }
        let scale = min(1, min(1280 / Double(width), 720 / Double(height)))
        return CGSize(width: max(1, Int(Double(width) * scale)), height: max(1, Int(Double(height) * scale)))
    }

    func convert(_ source: CVPixelBuffer) -> CVPixelBuffer? {
        let size = Self.outputSize(width: CVPixelBufferGetWidth(source), height: CVPixelBufferGetHeight(source))
        guard size != .zero else { return nil }
        if pool == nil || dimensions != size {
            let attributes: [CFString: Any] = [
                kCVPixelBufferWidthKey: Int(size.width), kCVPixelBufferHeightKey: Int(size.height),
                kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
                kCVPixelBufferIOSurfacePropertiesKey: [:], kCVPixelBufferMetalCompatibilityKey: true
            ]
            var created: CVPixelBufferPool?
            guard CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &created) == kCVReturnSuccess else { return nil }
            pool = created
            dimensions = size
        }
        guard let pool else { return nil }
        var output: CVPixelBuffer?
        // A blocked display must not grow a surface pool without a bound.
        let options = [kCVPixelBufferPoolAllocationThresholdKey: 4] as CFDictionary
        guard CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(nil, pool, options, &output) == kCVReturnSuccess,
              let output else { return nil }
        let image = CIImage(cvPixelBuffer: source, options: [.toneMapHDRtoSDR: true])
        let scaled = image.transformed(by: CGAffineTransform(scaleX: size.width / image.extent.width,
                                                            y: size.height / image.extent.height))
        context.render(scaled, to: output, bounds: CGRect(origin: .zero, size: size), colorSpace: colorSpace)
        CVBufferSetAttachment(output, kCVImageBufferCGColorSpaceKey, colorSpace, .shouldPropagate)
        return output
    }
}

enum NativeStreamPiPSampleTiming {
    static func make(at time: CMTime = CMClockGetTime(CMClockGetHostTimeClock())) -> CMSampleTimingInfo {
        CMSampleTimingInfo(duration: CMTime(value: 1, timescale: 30),
                           presentationTimeStamp: time, decodeTimeStamp: .invalid)
    }
}
