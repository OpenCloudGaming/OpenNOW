import Foundation
import CoreImage
import CoreVideo
import Metal
#if canImport(MetalFX)
import MetalFX
#endif

#if !targetEnvironment(simulator)
/// Metal 4 presentation and spatial upscaling for decoded video images.
/// Native SDR/PQ/HLG planes and linear RGB processing surfaces convert directly.
/// Core Image producers for other inputs signal a GPU event without CPU waits.
@available(iOS 26.0, macOS 26.0, *)
final class NativeStreamMetal4EffectsRenderer {
    private struct Key: Hashable {
        let width, height, outputWidth, outputHeight: Int
        let transfer: Int
        let upscale, sharpen: Bool
    }
    private struct Slot {
        let allocator: any MTL4CommandAllocator
        let command: any MTL4CommandBuffer
        let arguments: any MTL4ArgumentTable
        let conversionArguments, sharpeningArguments: any MTL4ArgumentTable
        let residency: any MTLResidencySet
        let uniforms: any MTLBuffer
        let input, output: any MTLTexture
        let sharpened: (any MTLTexture)?
        let scaler: (any MTL4FXSpatialScaler)?
    }
    private final class Resources {
        let slots: [Slot]
        private let lock = NSLock()
        private var available = [0,1]
        init(slots: [Slot]) { self.slots = slots }
        func take() -> Int? { lock.lock(); defer { lock.unlock() }; return available.popLast() }
        func release(_ index: Int) { lock.lock(); available.append(index); lock.unlock() }
    }
    private let device: any MTLDevice
    private let queue: any MTL4CommandQueue
    private let compiler: any MTL4Compiler
    private let sdrPipeline, hdrPipeline, conversionPipeline: any MTLRenderPipelineState
    private let sharpeningPipeline: any MTLComputePipelineState
    // A static ColorSync transform preserves Apple's HLG display interpretation.
    // Build once off the display thread; no per-frame Core Image producer is needed.
    private let hlgConversion: any MTLTexture
    private let videoTransfer: any MTLTexture
    private let textureCache: CVMetalTextureCache
    private let drawableResidency: NativeStreamMetalDrawableResidency
    func setDrawableResidency(_ residency: any MTLResidencySet) {
        drawableResidency.update(residency)
    }

    private let producerEvent: any MTLSharedEvent
    private var producerValue: UInt64 = 0
    private let setupQueue = DispatchQueue(label: "OpenNOW.Metal4FX.setup", qos: .userInitiated)
    private var resources: [Key: Resources] = [:]
    private var preparing: Set<Key> = []
    private var rejected: Set<Key> = []
    private var order: [Key] = []
    private(set) var status = "Preparing Metal 4"
    init?(device: any MTLDevice) {
        guard NativeStreamMetal4HDRRenderer.isSupported(device: device),
              let queue = device.makeMTL4CommandQueue(), let event = device.makeSharedEvent(),
              let hlg = Self.makeHLGConversion(device:device),
              let video = Self.makeVideoTransfer(device:device) else { return nil }
        var cache: CVMetalTextureCache?
        guard CVMetalTextureCacheCreate(nil, nil, device, nil, &cache) == kCVReturnSuccess,
              let cache else { return nil }
        do {
            let compiler = try device.makeCompiler(descriptor: MTL4CompilerDescriptor())
            let compileOptions = MTLCompileOptions()
            compileOptions.mathMode = .safe
            let library = try device.makeLibrary(source: NativeStreamHDRMetalProgram.shader + "\n" + Self.shader, options: compileOptions)
            func pipeline(_ format: MTLPixelFormat, fragmentName: String = "effectsFragment") throws -> any MTLRenderPipelineState {
                let descriptor = MTL4RenderPipelineDescriptor()
                let vertex = MTL4LibraryFunctionDescriptor(); vertex.library = library; vertex.name = "effectsVertex"
                let fragment = MTL4LibraryFunctionDescriptor(); fragment.library = library; fragment.name = fragmentName
                descriptor.vertexFunctionDescriptor = vertex; descriptor.fragmentFunctionDescriptor = fragment
                descriptor.colorAttachments[0].pixelFormat = format
                return try compiler.makeRenderPipelineState(descriptor: descriptor)
            }
            sdrPipeline = try pipeline(.bgra8Unorm); hdrPipeline = try pipeline(.bgr10a2Unorm)
            conversionPipeline = try pipeline(.rgba16Float, fragmentName: "nativeLinearFragment")
            let compute = MTL4ComputePipelineDescriptor()
            let function = MTL4LibraryFunctionDescriptor(); function.library = library; function.name = "sharpenLinear"
            compute.computeFunctionDescriptor = function
            sharpeningPipeline = try compiler.makeComputePipelineState(descriptor: compute)
            textureCache = cache; hlgConversion = hlg; videoTransfer = video
            self.device = device; self.queue = queue; self.compiler = compiler; producerEvent = event
            drawableResidency = NativeStreamMetalDrawableResidency(queue: queue)
        } catch { return nil }
    }
    /// Called on the display thread. False leaves the producer uncommitted for legacy fallback.
    func submit(image: CIImage, destination: CGRect, transfer: Int, upscale: Bool,
                context: CIContext, producer: any MTLCommandBuffer, target: any MTLTexture,
                drawable: (any MTLDrawable)? = nil, ticket: NativeStreamMetalFrameTimeline.Ticket? = nil,
                presented: (@Sendable (Double) -> Void)? = nil,
                presentAt: Double? = nil, completion: @escaping @Sendable (Double, NSError?) -> Void) -> Bool {
        submit(image: image, native: nil, destination: destination, transfer: transfer, upscale: upscale,
               context: context, producer: producer, sharpening: 0, target: target, drawable: drawable, ticket: ticket,
               presented: presented, presentAt: presentAt, completion: completion)
    }
    /// Zero-copy native SDR/PQ/HLG or a tagged linear RGB surface.
    /// Unknown color metadata/warm-up retains the compatible CI fallback.
    func submit(buffer: CVPixelBuffer, destination: CGRect, upscale: Bool, target: any MTLTexture,
                sharpening: Float = 0, producer: (any MTLCommandBuffer)? = nil,
                drawable: (any MTLDrawable)? = nil, ticket: NativeStreamMetalFrameTimeline.Ticket? = nil,
                presented: (@Sendable (Double) -> Void)? = nil, presentAt: Double? = nil,
                completion: @escaping @Sendable (Double, NSError?) -> Void) -> Bool {
        guard let input = NativeStreamMetalVideoInput.Input(buffer: buffer, cache: textureCache, target: target) else { return false }
        return submit(image: nil, native: input, destination: destination,
                      transfer: input.color.presentationTransfer, upscale: upscale,
                      context: nil, producer: producer, sharpening: sharpening,
                      target: target, drawable: drawable, ticket: ticket,
                      presented: presented, presentAt: presentAt, completion: completion)
    }
    private func submit(image: CIImage?, native: NativeStreamMetalVideoInput.Input?,
                destination: CGRect, transfer: Int, upscale: Bool,
                context: CIContext?, producer: (any MTLCommandBuffer)?, sharpening: Float, target: any MTLTexture,
                drawable: (any MTLDrawable)?, ticket: NativeStreamMetalFrameTimeline.Ticket?,
                presented: (@Sendable (Double) -> Void)?, presentAt: Double?,
                completion: @escaping @Sendable (Double, NSError?) -> Void) -> Bool {
        guard drawable == nil || drawableResidency.isRegistered else { return false }
        let size = native.map { CGSize(width: $0.y.width, height: $0.y.height) } ?? image?.extent.size ?? .zero
        guard size.width.isFinite, size.height.isFinite, size.width > 0, size.height > 0,
              native != nil || (image != nil && context != nil && producer != nil),
              destination.width.isFinite, destination.height.isFinite, destination.width > 0, destination.height > 0,
              transfer >= 0, transfer <= 2,
              target.pixelFormat == (transfer == 0 ? .bgra8Unorm : .bgr10a2Unorm) else { return false }
        let outputSize = upscale ? NativeStreamVideoEffectsPolicy.upscaleSize(source: size, destination: destination.size) : nil
        let key = Key(width: Int(size.width.rounded()), height: Int(size.height.rounded()),
            outputWidth: Int((outputSize?.width ?? size.width).rounded()),
            outputHeight: Int((outputSize?.height ?? size.height).rounded()), transfer: transfer, upscale: outputSize != nil, sharpen: sharpening.isFinite && sharpening > 0.001)
        guard let resource = resources[key] else {
            prepare(key); status = rejected.contains(key) ? "Metal 4 unavailable for this resolution" : "Preparing Metal 4"
            return false
        }
        guard let index = resource.take() else { return false }
        let slot = resource.slots[index]
        if let image, let context, let producer {
            let space = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: transfer != 0)
            let source = key.upscale ? NativeStreamVideoEffectsPolicy.spatialInput(image: image, hdr: transfer != 0) : image
            context.render(source.transformed(by: CGAffineTransform(translationX: -image.extent.minX, y: -image.extent.minY)),
                to: slot.input, commandBuffer: producer,
                bounds: CGRect(x: 0, y: 0, width: key.width, height: key.height), colorSpace: space)
            producer.addCompletedHandler { [resource] _ in _ = resource }
        }
        slot.allocator.reset(); slot.residency.removeAllAllocations()
        for texture in [slot.input, slot.output, target, hlgConversion, videoTransfer] { slot.residency.addAllocation(texture) }
        if let sharp = slot.sharpened { slot.residency.addAllocation(sharp) }
        if let native { slot.residency.addAllocation(native.y); slot.residency.addAllocation(native.uv) }
        slot.residency.addAllocation(slot.uniforms); slot.residency.commit()
        var uniforms = SIMD4<Float>(Float(transfer), sharpening.isFinite ? min(max(sharpening,0),1) : 0, 0, 0)
        withUnsafeBytes(of: &uniforms) { slot.uniforms.contents().copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
        slot.arguments.setAddress(slot.uniforms.gpuAddress, index: 0)
        slot.arguments.setTexture(slot.output.gpuResourceID, index: 0)
        slot.command.beginCommandBuffer(allocator: slot.allocator); slot.command.useResidencySet(slot.residency)
        if let native {
            var conversion = native.uniforms
            let offset = MemoryLayout<SIMD4<Float>>.stride
            withUnsafeBytes(of: &conversion) { slot.uniforms.contents().advanced(by: offset).copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
            slot.conversionArguments.setAddress(slot.uniforms.gpuAddress + UInt64(offset), index: 0)
            slot.conversionArguments.setTexture(native.y.gpuResourceID, index: 0)
            slot.conversionArguments.setTexture(native.uv.gpuResourceID, index: 1)
            slot.conversionArguments.setTexture(hlgConversion.gpuResourceID, index: 2)
            slot.conversionArguments.setTexture(videoTransfer.gpuResourceID, index: 3)
            let conversionPass = MTL4RenderPassDescriptor()
            conversionPass.colorAttachments[0].texture = slot.input
            conversionPass.colorAttachments[0].loadAction = .dontCare
            conversionPass.colorAttachments[0].storeAction = .store
            guard let encoder = slot.command.makeRenderCommandEncoder(descriptor: conversionPass) else {
                slot.command.endCommandBuffer(); resource.release(index); return false
            }
            // Pooled decoder IOSurfaces can arrive with a different texture view
            // over recycled memory. Explicitly make aliased reads coherent.
            encoder.barrier(afterQueueStages: .all, beforeStages: .fragment,
                            visibilityOptions: [.device, .resourceAlias])
            encoder.setRenderPipelineState(conversionPipeline)
            encoder.setViewport(MTLViewport(originX: 0, originY: 0, width: Double(key.width), height: Double(key.height), znear: 0, zfar: 1))
            encoder.setArgumentTable(slot.conversionArguments, stages: .fragment)
            encoder.drawPrimitives(primitiveType: .triangleStrip, vertexStart: 0, vertexCount: 4)
            // Conversion's fragment writes must be visible to subsequent compute or render passes.
            encoder.barrier(afterStages: [.fragment, .tile],
                            beforeQueueStages: key.upscale || key.sharpen ? [.dispatch, .blit] : [.fragment, .tile],
                            visibilityOptions: .device)
            encoder.endEncoding()
        }
        let processingInput = slot.sharpened ?? slot.input
        if let sharp = slot.sharpened {
            guard let encoder = slot.command.makeComputeCommandEncoder() else {
                slot.command.endCommandBuffer(); resource.release(index); return false
            }
            encoder.barrier(afterQueueStages: [.fragment, .tile], beforeStages: .dispatch, visibilityOptions: .device)
            slot.sharpeningArguments.setAddress(slot.uniforms.gpuAddress, index: 0)
            slot.sharpeningArguments.setTexture(slot.input.gpuResourceID, index: 0)
            slot.sharpeningArguments.setTexture(sharp.gpuResourceID, index: 1)
            encoder.setComputePipelineState(sharpeningPipeline)
            encoder.setArgumentTable(slot.sharpeningArguments)
            encoder.dispatchThreads(threadsPerGrid: MTLSize(width:key.width,height:key.height,depth:1),
                threadsPerThreadgroup:MTLSize(width:8,height:8,depth:1))
            encoder.barrier(afterStages: .dispatch, beforeQueueStages: [.dispatch, .blit, .fragment, .tile], visibilityOptions: .device)
            encoder.endEncoding()
        }
        slot.arguments.setTexture(slot.output.gpuResourceID, index: 0)
        if let scaler = slot.scaler {
            scaler.colorTexture = processingInput; scaler.outputTexture = slot.output
            scaler.inputContentWidth = key.width; scaler.inputContentHeight = key.height
            scaler.encode(commandBuffer: slot.command)
        }
        let pass = MTL4RenderPassDescriptor()
        pass.colorAttachments[0].texture = target; pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store; pass.colorAttachments[0].clearColor = MTLClearColorMake(0,0,0,1)
        guard let encoder = slot.command.makeRenderCommandEncoder(descriptor: pass) else {
            slot.command.endCommandBuffer(); resource.release(index); return false
        }
        // Metal 4 does not infer dependencies between previous stages and fragment read.
        encoder.barrier(afterQueueStages: key.upscale || key.sharpen ? [.dispatch, .blit] : [.fragment, .tile],
                        beforeStages: .fragment, visibilityOptions: [.device, .resourceAlias])
        encoder.setRenderPipelineState(transfer == 0 ? sdrPipeline : hdrPipeline)
        encoder.setViewport(MTLViewport(originX: destination.minX, originY: destination.minY,
            width: destination.width, height: destination.height, znear: 0, zfar: 1))
        encoder.setArgumentTable(slot.arguments, stages: .fragment)
        encoder.drawPrimitives(primitiveType: .triangleStrip, vertexStart: 0, vertexCount: 4)
        // Store tile results before handing this drawable to the compositor,
        // which consumes another view of the same IOSurface allocation.
        encoder.barrier(afterStages: [.fragment, .tile], beforeQueueStages: .all,
                        visibilityOptions: [.device, .resourceAlias])
        encoder.endEncoding(); slot.command.endCommandBuffer()
        // A failed producer can skip its GPU signal. Only unblock after completion;
        // report the error to the consumer completion as well as recovering the event.
        let producerFailure = ProducerFailure()
        if let producer {
            producerValue += 1
            let value = producerValue, event = producerEvent
            producer.encodeSignalEvent(event, value: value)
            producer.addCompletedHandler { command in
                if command.status == .error {
                    producerFailure.set(command.error as NSError?)
                    event.signaledValue = max(event.signaledValue, value)
                }
            }
        }
        let options = MTL4CommitOptions()
        options.addFeedbackHandler { [resource, slot, target, drawable, producer, native, hlgConversion, videoTransfer] feedback in
            _ = (slot, target, drawable, native, hlgConversion, videoTransfer)
            let error = producerFailure.error ?? feedback.error as NSError?
            if error != nil { ticket?.recoverAfterGPUFailure() }
            resource.release(index)
            let producerDuration = producer.map { max($0.gpuEndTime - $0.gpuStartTime, 0) } ?? 0
            let duration = max(feedback.gpuEndTime - feedback.gpuStartTime, 0) + producerDuration
            completion(duration, error)
        }
        if let drawable, let presented { drawable.addPresentedHandler { value in
            if value.presentedTime > 0 { presented(value.presentedTime) }
        } }
        if let producer {
            producer.commit(); queue.waitForEvent(producerEvent, value: producerValue)
        } else if let ticket, ticket.previous > 0 { queue.waitForEvent(ticket.event, value: ticket.previous) }
        if let drawable { queue.waitForDrawable(drawable) }
        queue.commit([slot.command], options: options)
        if let ticket { queue.signalEvent(ticket.event, value: ticket.value) }
        if let drawable { queue.signalDrawable(drawable); if let presentAt { drawable.present(at:presentAt) } else { drawable.present() } }
        status = key.upscale ? "Metal 4 · \(key.width)×\(key.height) → \(key.outputWidth)×\(key.outputHeight)"
            : upscale ? "No upscale: \(key.width)×\(key.height) → \(Int(destination.width))×\(Int(destination.height))" : "Metal 4 · presentation"
        return true
    }
    private final class ProducerFailure: @unchecked Sendable {
        private let lock = NSLock(); private var stored: NSError?
        var error: NSError? { lock.lock(); defer { lock.unlock() }; return stored }
        func set(_ error: NSError?) { lock.lock(); stored = error ?? NSError(domain: "OpenNOW.Metal4Producer", code: -1); lock.unlock() }
    }
    private func prepare(_ key: Key) {
        guard !preparing.contains(key), !rejected.contains(key), preparing.count < 2 else { return }
        preparing.insert(key)
        let device = device, compiler = compiler
        setupQueue.async { [weak self] in
            var slots: [Slot] = []
            do {
                for _ in 0..<2 {
                    var scaler: (any MTL4FXSpatialScaler)?
                    if key.upscale {
                        let descriptor = MTLFXSpatialScalerDescriptor()
                        descriptor.inputWidth = key.width; descriptor.inputHeight = key.height
                        descriptor.outputWidth = key.outputWidth; descriptor.outputHeight = key.outputHeight
                        descriptor.colorTextureFormat = .rgba16Float; descriptor.outputTextureFormat = .rgba16Float
                        descriptor.colorProcessingMode = key.transfer == 0 ? .linear : .hdr
                        scaler = descriptor.makeSpatialScaler(device: device, compiler: compiler)
                        guard scaler != nil else { throw NSError(domain: "OpenNOW.Metal4FX", code: 1) }
                    }
                    func texture(_ width: Int, _ height: Int, _ usage: MTLTextureUsage) -> (any MTLTexture)? {
                        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba16Float, width: width, height: height, mipmapped: false)
                        descriptor.storageMode = .private; descriptor.usage = usage.union([.shaderRead, .shaderWrite, .renderTarget])
                        return device.makeTexture(descriptor: descriptor)
                    }
                    guard let input = texture(key.width, key.height, scaler?.colorTextureUsage ?? []),
                          let output = key.upscale || key.sharpen ? texture(key.outputWidth, key.outputHeight, scaler?.outputTextureUsage ?? []) : input,
                          let allocator = device.makeCommandAllocator(), let command = device.makeCommandBuffer(),
                          let uniforms = device.makeBuffer(length: MemoryLayout<SIMD4<Float>>.stride + MemoryLayout<NativeStreamHDRMetalProgram.Uniforms>.stride, options: .storageModeShared)
                        else { throw NSError(domain: "OpenNOW.Metal4FX", code: 2) }
                    let table = MTL4ArgumentTableDescriptor(); table.maxTextureBindCount = 1; table.maxBufferBindCount = 1
                    let conversionTable = MTL4ArgumentTableDescriptor(); conversionTable.maxTextureBindCount = 4; conversionTable.maxBufferBindCount = 1
                    let sharp = key.sharpen ? (key.upscale ? texture(key.width,key.height,[]) : output) : nil
                    if key.sharpen && sharp == nil { throw NSError(domain: "OpenNOW.Metal4FX", code: 3) }
                    let residency = MTLResidencySetDescriptor(); residency.initialCapacity = 8
                    slots.append(Slot(allocator: allocator, command: command, arguments: try device.makeArgumentTable(descriptor: table),
                        conversionArguments: try device.makeArgumentTable(descriptor: conversionTable),
                        sharpeningArguments: try device.makeArgumentTable(descriptor: conversionTable),
                        residency: try device.makeResidencySet(descriptor: residency), uniforms: uniforms, input: input, output: output, sharpened: sharp, scaler: scaler))
                }
            } catch { slots.removeAll() }
            let result = slots.count == 2 ? Resources(slots: slots) : nil
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.preparing.remove(key)
                if let result {
                    self.resources[key] = result; self.order.append(key)
                    // Preserve cached resources when live video geometry changes.
                    // Keep two configurations; in-flight completions retain evicted resources.
                    while self.order.count > 2 { self.resources.removeValue(forKey: self.order.removeFirst()) }
                } else { self.rejected.insert(key) }
            }
        }
    }
    private static func makeVideoTransfer(device:any MTLDevice) -> (any MTLTexture)? {
        let edge = 1024
        var values = [UInt16](repeating:0,count:edge*2)
        for row in 0..<2 {
            let hdrGamut = row == 1
            var allocation:CVPixelBuffer?
            guard CVPixelBufferCreate(nil,edge,2,kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,
                [kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,
                &allocation) == kCVReturnSuccess, let input = allocation else { return nil }
            CVPixelBufferLockBaseAddress(input,[])
            for plane in 0..<2 {
                let base = CVPixelBufferGetBaseAddressOfPlane(input,plane)!.assumingMemoryBound(to:UInt16.self)
                let stride = CVPixelBufferGetBytesPerRowOfPlane(input,plane)/2
                for y in 0..<2 { for x in 0..<(plane == 0 ? edge : edge*2) {
                    base[y*stride+x] = UInt16(plane == 0 ? x : 512)<<6
                } }
            }
            CVPixelBufferUnlockBaseAddress(input,[])
            CVBufferSetAttachment(input,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_ITU_R_709_2,.shouldPropagate)
            CVBufferSetAttachment(input,kCVImageBufferColorPrimariesKey,hdrGamut ? kCVImageBufferColorPrimaries_ITU_R_2020 : kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
            CVBufferSetAttachment(input,kCVImageBufferYCbCrMatrixKey,hdrGamut ? kCVImageBufferYCbCrMatrix_ITU_R_2020 : kCVImageBufferYCbCrMatrix_ITU_R_709_2,.shouldPropagate)
            let linear = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr:hdrGamut)
            let image = CIImage(cvPixelBuffer:input)
            var converted = [UInt16](repeating:0,count:edge*2*4)
            let context = CIContext(mtlDevice:device,options:[.cacheIntermediates:false,.workingColorSpace:linear])
            converted.withUnsafeMutableBytes { context.render(image,toBitmap:$0.baseAddress!,rowBytes:edge*8,
                bounds:image.extent,format:.RGBAh,colorSpace:linear) }
            for x in 0..<edge { values[row*edge+x] = converted[x*4] }
        }
        guard values.allSatisfy({ Float(Float16(bitPattern:$0)).isFinite }) else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.r16Float,width:edge,height:2,mipmapped:false)
        descriptor.storageMode = .shared; descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor:descriptor) else { return nil }
        values.withUnsafeBytes { texture.replace(region:MTLRegionMake2D(0,0,edge,2),mipmapLevel:0,
            withBytes:$0.baseAddress!,bytesPerRow:edge*2) }
        return texture
    }
    private static func makeHLGConversion(device: any MTLDevice) -> (any MTLTexture)? {
        let edge = 65
        var encoded = [Float](repeating:0,count:edge*edge*edge*4)
        for b in 0..<edge { for g in 0..<edge { for r in 0..<edge {
            let i = ((b*edge+g)*edge+r)*4
            encoded[i] = Float(r)/Float(edge-1); encoded[i+1] = Float(g)/Float(edge-1)
            encoded[i+2] = Float(b)/Float(edge-1); encoded[i+3] = 1
        } } }
        let sourceSpace = CGColorSpace(name:CGColorSpace.itur_2100_HLG)!
        let linear = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr:true)
        let image = encoded.withUnsafeBytes { CIImage(bitmapData:Data($0),bytesPerRow:edge*edge*16,
            size:CGSize(width:edge*edge,height:edge),format:.RGBAf,colorSpace:sourceSpace) }
        var converted = [UInt16](repeating:0,count:encoded.count)
        let context = CIContext(mtlDevice:device,options:[.cacheIntermediates:false,.workingColorSpace:linear])
        converted.withUnsafeMutableBytes { context.render(image,toBitmap:$0.baseAddress!,rowBytes:edge*edge*8,
            bounds:image.extent,format:.RGBAh,colorSpace:linear) }
        let descriptor = MTLTextureDescriptor()
        descriptor.textureType = .type3D; descriptor.pixelFormat = .rgba16Float
        descriptor.width = edge; descriptor.height = edge; descriptor.depth = edge
        descriptor.storageMode = .shared; descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor:descriptor),
              converted.allSatisfy({ Float(Float16(bitPattern:$0)).isFinite }) else { return nil }
        converted.withUnsafeBytes { texture.replace(region:MTLRegionMake3D(0,0,0,edge,edge,edge),mipmapLevel:0,
            slice:0,withBytes:$0.baseAddress!,bytesPerRow:edge*8,bytesPerImage:edge*edge*8) }
        return texture
    }
    private static let shader = """
    #include <metal_stdlib>
    using namespace metal;
    struct V { float4 position [[position]]; float2 uv; };
    vertex V effectsVertex(uint id [[vertex_id]]) {
        const float2 p[] = {float2(-1,1),float2(-1,-1),float2(1,1),float2(1,-1)};
        const float2 uv[] = {float2(0,1),float2(0,0),float2(1,1),float2(1,0)};
        return {float4(p[id],0,1),uv[id]};
    }
    float3 pqToLinear(float3 encoded) {
        float3 p = pow(clamp(encoded,0.0f,1.0f), float3(32.0f/2523.0f));
        return pow(max(p-3424.0f/4096.0f,0.0f) /
            max(2413.0f/128.0f-(2392.0f/128.0f)*p,1e-6f),float3(16384.0f/2610.0f))/0.0203f;
    }
    fragment float4 nativeLinearFragment(V v [[stage_in]], texture2d<float> y [[texture(0)]],
        texture2d<float> uv [[texture(1)]], texture3d<float> hlg [[texture(2)]], texture2d<float> video [[texture(3)]], constant Uniforms &u [[buffer(0)]]) {
        constexpr sampler sample(filter::linear,address::clamp_to_edge);
        uint2 pixel = uint2(clamp(v.uv*float2(y.get_width(),y.get_height()),float2(0),
                                 float2(y.get_width()-1,y.get_height()-1)));
        float3 rgb = u.coefficients.w > 0 ? y.read(pixel).rgb : hdrEncodedRGB(y,uv,v.uv,u);
        int transfer = int(u.green.z);
        if (transfer == 1) { rgb = pqToLinear(rgb); }
        else if (transfer == 2) {
            constexpr sampler lookup(filter::linear,address::clamp_to_edge);
            float edge = float(hlg.get_width());
            rgb = hlg.sample(lookup,clamp(rgb,0.0f,1.0f)*((edge-1.0f)/edge)+0.5f/edge).rgb;
        } else if (transfer == 3) {
            constexpr sampler lookup(filter::linear,address::clamp_to_edge);
            float edge = float(video.get_width());
            float3 e = clamp(rgb,0.0f,1.0f)*((edge-1.0f)/edge)+0.5f/edge;
            float row = u.green.w > 0 ? 0.75f : 0.25f;
            rgb = float3(video.sample(lookup,float2(e.r,row)).r,video.sample(lookup,float2(e.g,row)).r,
                         video.sample(lookup,float2(e.b,row)).r);
        } else if (transfer == 0) {
            float3 e = max(rgb,0.0f);
            rgb = select(pow((e+0.055f)/1.055f,float3(2.4f)),e/12.92f,e<=0.04045f);
        }
        if (u.green.w > 0 && transfer != 1 && transfer != 2 && transfer != 4) {
            // Linear BT.2020 to linear sRGB for SDR presentation.
            rgb = float3(dot(rgb,float3(1.660491f,-0.587641f,-0.072850f)),
                         dot(rgb,float3(-0.124550f,1.132900f,-0.008349f)),
                         dot(rgb,float3(-0.018151f,-0.100579f,1.118730f)));
        }
        return float4(clamp(rgb,0.0f,transfer == 1 || transfer == 2 || (transfer == 4 && u.green.w > 0) ? 65504.0f : 1.0f),1);
    }
    kernel void sharpenLinear(texture2d<float,access::read> input [[texture(0)]],
        texture2d<float,access::write> output [[texture(1)]],constant float4 &u [[buffer(0)]],
        uint2 position [[thread_position_in_grid]]) {
        if (position.x >= output.get_width() || position.y >= output.get_height()) return;
        int2 size = int2(input.get_width(),input.get_height());
        float3 weights = u.x > 0 ? float3(0.2627f,0.6780f,0.0593f) : float3(0.2126f,0.7152f,0.0722f);
        float blurred = 0;
        for (int y=-1;y<=1;y++) { for (int x=-1;x<=1;x++) {
            int2 p = clamp(int2(position)+int2(x,y),int2(0),size-1);
            blurred += dot(input.read(uint2(p)).rgb,weights) * (x==0?2.0f:1.0f) * (y==0?2.0f:1.0f)/16.0f;
        } }
        float3 rgb = input.read(position).rgb;
        float detail = (dot(rgb,weights)-blurred)*u.y*2.0f;
        // Keep HDR highlights and full chroma; bound SDR/MetalFX overshoot.
        output.write(float4(clamp(rgb+detail,0.0f,u.x>0?65504.0f:1.0f),1),position);
    }
    fragment float4 effectsFragment(V v [[stage_in]], texture2d<float> image [[texture(0)]], constant float4 &u [[buffer(0)]]) {
        constexpr sampler s(filter::linear,address::clamp_to_edge);
        float3 rgb = max(image.sample(s,v.uv).rgb,0.0f);
        if (u.x > 0) {
            // Native conversion and CI fallback use linear BT.2020.
            // Normalize both HDR transfers to PQ presentation.
            // CI extended linear BT.2020 uses 203 nit reference white.
            float3 y = pow(rgb * 0.0203f, float3(2610.0f/16384.0f));
            rgb = pow((3424.0f/4096.0f + (2413.0f/128.0f)*y)/(1.0f+(2392.0f/128.0f)*y),float3(2523.0f/32.0f));
        } else {
            rgb = select(1.055f*pow(rgb,float3(1.0f/2.4f))-0.055f,12.92f*rgb,rgb<=0.0031308f);
        }
        return float4(rgb,1);
    }
    """
}
#else
@available(iOS 26.0, macOS 26.0, *)
final class NativeStreamMetal4EffectsRenderer {
    init?(device: any MTLDevice) { return nil }
    func setDrawableResidency(_ residency: any MTLResidencySet) {}
}
#endif
