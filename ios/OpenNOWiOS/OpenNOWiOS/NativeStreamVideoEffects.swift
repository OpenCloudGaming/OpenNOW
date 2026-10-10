import Foundation
import CoreImage
import CoreMedia
import CoreVideo
import Metal

#if canImport(MetalFX)
import MetalFX
#endif
struct NativeStreamPresentationRates: Equatable {
    let displayedFPS: Double
    var label: String { String(format: "Displayed %.0f FPS", displayedFPS) }
}

/// Count actual drawable presentation callbacks, excluding the anchor frame from
/// each time window; submitted work is not counted as displayed video.
struct NativeStreamPresentationRateMeter {
    private var windowStart: Double?
    private var lastTime: Double?
    private var displayed = 0
    private var measuredAt: Double?
    private var rates: NativeStreamPresentationRates?
    mutating func observe(time: Double) {
        guard time.isFinite, time > (lastTime ?? -.infinity) else { return }
        if windowStart == nil || time - (lastTime ?? time) > 3 {
            windowStart = time; lastTime = time
            displayed = 0; rates = nil; measuredAt = nil
            return
        }
        lastTime = time
        displayed += 1
        let elapsed = time - windowStart!
        guard elapsed >= 1 else { return }
        rates = .init(displayedFPS: Double(displayed)/elapsed)
        measuredAt = time
        windowStart = time; displayed = 0
    }
    func snapshot(now: Double) -> NativeStreamPresentationRates? {
        guard let measuredAt, now.isFinite, now >= measuredAt, now - measuredAt < 3 else { return nil }
        return rates
    }
}

/// Spatial effects operate after decode and preserve the received video format.
enum NativeStreamVideoEffectsPolicy {
    static func canUseDirectHDRPath(upscalingEnabled: Bool, upscaleEligible: Bool,
                                    sharpeningAmount: Double) -> Bool {
        (!upscalingEnabled || !upscaleEligible)
            && sharpeningAmount.isFinite && sharpeningAmount <= 0.001
    }

    static func presentationSize(source: CGSize, display: CGSize, stretch: Bool) -> CGSize {
        guard !stretch, source.width > 0, source.height > 0 else { return display }
        let scale = min(display.width / source.width, display.height / source.height)
        return CGSize(width: source.width * scale, height: source.height * scale)
    }

    /// Share the supported enlargement range between presets and runtime MetalFX.
    static func canSelectUpscaleResolution(source: CGSize, destination: CGSize) -> Bool {
        source.width > 0 && source.height > 0
            && destination.width > source.width * 1.02
            && destination.height > source.height * 1.02
            && destination.width <= source.width * 4
            && destination.height <= source.height * 4
    }

    static func upscaleSize(source: CGSize, destination: CGSize) -> CGSize? {
        guard canSelectUpscaleResolution(source: source, destination: destination) else { return nil }
        return CGSize(width: destination.width.rounded(), height: destination.height.rounded())
    }

    /// Sharpening can overshoot below zero or above SDR white. MetalFX linear
    /// input is defined only in [0,1]; invalid values can produce NaN output.
    /// HDR retains its half-float range and highlights rather than clipping to SDR.
    static func spatialInput(image: CIImage, hdr: Bool) -> CIImage {
        // Clamp in the scaler's RGB space. Clamping HDR in CI's default sRGB
        // working space clips valid saturated BT.2020 colors before upscaling.
        let space = workingColorSpace(hdr: hdr)
        let linear = image.matchedFromWorkingSpace(to: space) ?? image
        let clamped = linear.applyingFilter("CIColorClamp", parameters: [
            "inputMinComponents": CIVector(x: 0, y: 0, z: 0, w: 0),
            "inputMaxComponents": CIVector(x: hdr ? 65504 : 1, y: hdr ? 65504 : 1, z: hdr ? 65504 : 1, w: 1)
        ]).cropped(to: image.extent)
        return clamped.matchedToWorkingSpace(from: space) ?? clamped
    }

    static func workingColorSpace(hdr: Bool) -> CGColorSpace {
        CGColorSpace(name: hdr ? CGColorSpace.extendedLinearITUR_2020 : CGColorSpace.extendedLinearSRGB)!
    }
}

/// One scaler per geometry/color configuration; model/pipeline creation stays off
/// the display thread. Main-thread access only. GPU work uses the renderer's queue.
#if canImport(MetalFX)
final class NativeStreamSpatialUpscaler {
    private struct Key: Equatable { let width, height, outputWidth, outputHeight: Int; let hdr: Bool }
    private struct Resources {
        let scaler: any MTLFXSpatialScaler
        let inputs, outputs: [any MTLTexture]
    }
    private let device: any MTLDevice
    private let setupQueue = DispatchQueue(label: "OpenNOW.MetalFX.setup", qos: .userInitiated)
    private var key: Key?
    private var resources: Resources?
    private var generation = 0
    private var slot = 0
    private(set) var status = "Off"

    init(device: any MTLDevice) { self.device = device }

    func reset() {
        generation += 1; key = nil; resources = nil; status = "Off"
    }

    func encode(image: CIImage, sourceSize: CGSize, destinationSize: CGSize, hdr: Bool,
                context: CIContext, commandBuffer: any MTLCommandBuffer) -> CIImage? {
        guard MTLFXSpatialScalerDescriptor.supportsDevice(device) else {
            status = "Unavailable on this device"; return nil
        }
        guard let size = NativeStreamVideoEffectsPolicy.upscaleSize(source: sourceSize, destination: destinationSize) else {
            status = "No upscale: \(Int(sourceSize.width))×\(Int(sourceSize.height)) → \(Int(destinationSize.width))×\(Int(destinationSize.height))"; return nil
        }
        let next = Key(width: Int(sourceSize.width), height: Int(sourceSize.height),
                       outputWidth: Int(size.width), outputHeight: Int(size.height), hdr: hdr)
        if next != key {
            key = next; resources = nil; generation += 1
            let token = generation, device = device
            status = "Preparing"
            setupQueue.async { [weak self] in
                let descriptor = MTLFXSpatialScalerDescriptor()
                descriptor.inputWidth = next.width; descriptor.inputHeight = next.height
                descriptor.outputWidth = next.outputWidth; descriptor.outputHeight = next.outputHeight
                descriptor.colorTextureFormat = .rgba16Float; descriptor.outputTextureFormat = .rgba16Float
                descriptor.colorProcessingMode = next.hdr ? .hdr : .linear
                var result: Resources?
                if let scaler = descriptor.makeSpatialScaler(device: device) {
                    func textures(width: Int, height: Int, usage: MTLTextureUsage) -> [any MTLTexture] {
                        let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba16Float,
                            width: width, height: height, mipmapped: false)
                        td.storageMode = .private; td.usage = usage.union([.shaderRead, .shaderWrite, .renderTarget])
                        return (0..<3).compactMap { _ in device.makeTexture(descriptor: td) }
                    }
                    let inputs = textures(width: next.width, height: next.height, usage: scaler.colorTextureUsage)
                    let outputs = textures(width: next.outputWidth, height: next.outputHeight, usage: scaler.outputTextureUsage)
                    if inputs.count == 3 && outputs.count == 3 { result = Resources(scaler: scaler, inputs: inputs, outputs: outputs) }
                }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.generation == token else { return }
                    self.resources = result
                    self.status = result == nil ? "Unavailable for this resolution" : "Ready"
                }
            }
        }
        guard let resources else { return nil }
        let input = resources.inputs[slot], output = resources.outputs[slot]
        slot = (slot + 1) % resources.inputs.count
        let space = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: hdr)
        context.render(NativeStreamVideoEffectsPolicy.spatialInput(image: image, hdr: hdr), to: input, commandBuffer: commandBuffer,
                       bounds: CGRect(origin: .zero, size: sourceSize), colorSpace: space)
        resources.scaler.colorTexture = input; resources.scaler.outputTexture = output
        resources.scaler.inputContentWidth = next.width; resources.scaler.inputContentHeight = next.height
        resources.scaler.encode(commandBuffer: commandBuffer)
        // The three texture slots are reused on ONE serial GPU queue, with at most
        // two submissions in flight. Retain the scaler across settings/size changes.
        commandBuffer.addCompletedHandler { [resources] _ in _ = resources }
        status = "\(next.width)×\(next.height) → \(next.outputWidth)×\(next.outputHeight)"
        // CI already accounts for Metal texture coordinates on import. An extra
        // mirror here reverses the normal CI-to-drawable playback orientation.
        return CIImage(mtlTexture: output, options: [.colorSpace: space])
    }
}

#else
final class NativeStreamSpatialUpscaler {
    private(set) var status = "Requires a physical device"
    init(device: any MTLDevice) {}
    func reset() { status = "Off" }
    func encode(image: CIImage, sourceSize: CGSize, destinationSize: CGSize, hdr: Bool,
                context: CIContext, commandBuffer: any MTLCommandBuffer) -> CIImage? {
        status = "Requires a physical device"; return nil
    }
}
#endif

/// Track actual drawable presentations for the user-facing FPS statistic.
final class NativeStreamPresentationTracker: @unchecked Sendable {
    private let lock = NSLock()
    private var meter = NativeStreamPresentationRateMeter()
    func rates(now: Double) -> NativeStreamPresentationRates? {
        lock.lock(); defer { lock.unlock() }
        return meter.snapshot(now: now)
    }
    func resetRates() { lock.lock(); meter = NativeStreamPresentationRateMeter(); lock.unlock() }
    func recordPresentation(at time: Double) {
        lock.lock(); defer { lock.unlock() }
        meter.observe(time: time)
    }
}
