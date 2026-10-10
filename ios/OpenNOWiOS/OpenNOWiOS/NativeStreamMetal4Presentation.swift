import Foundation
import Metal

/// Metal 4 renders into private textures; the compatible queue owns the drawable
/// copy and presentation. Slots remain occupied until the copy finishes, so a
/// producer cannot overwrite an image that the display queue is still reading.
final class NativeStreamMetal4Presentation {
    final class Frame: @unchecked Sendable {
        let texture: any MTLTexture
        fileprivate let index: Int
        fileprivate let command: any MTLCommandBuffer
        fileprivate init(texture: any MTLTexture, index: Int, command: any MTLCommandBuffer) {
            self.texture = texture; self.index = index; self.command = command
        }
    }

    private let queue: any MTLCommandQueue
    private let lock = NSLock()
    private var available = NativeStreamMetal4FrameSlotPolicy.indices
    private var textures = [Optional<any MTLTexture>](repeating: nil,
        count: NativeStreamMetal4FrameSlotPolicy.inFlightCount)

    init(queue: any MTLCommandQueue) { self.queue = queue }

    /// Prepare the complete consumer before submitting its producer. Failure is
    /// still safe for legacy fallback: no Metal 4 work has been committed yet.
    func prepare(target: any MTLTexture, ticket: NativeStreamMetalFrameTimeline.Ticket) -> Frame? {
        lock.lock(); let index = available.popLast(); lock.unlock()
        guard let index else { return nil }
        var prepared = false
        defer { if !prepared { release(index) } }
        if textures[index]?.width != target.width || textures[index]?.height != target.height
            || textures[index]?.pixelFormat != target.pixelFormat {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: target.pixelFormat,
                width: target.width, height: target.height, mipmapped: false)
            descriptor.storageMode = .private
            descriptor.usage = [.renderTarget, .shaderRead]
            textures[index] = queue.device.makeTexture(descriptor: descriptor)
        }
        guard let texture = textures[index], let command = queue.makeCommandBuffer() else { return nil }
        command.encodeWaitForEvent(ticket.event, value: ticket.value)
        guard let encoder = command.makeBlitCommandEncoder() else { return nil }
        encoder.copy(from: texture, sourceSlice: 0, sourceLevel: 0, sourceOrigin: MTLOrigin(),
            sourceSize: MTLSize(width: target.width, height: target.height, depth: 1),
            to: target, destinationSlice: 0, destinationLevel: 0, destinationOrigin: MTLOrigin())
        encoder.endEncoding()
        prepared = true
        return Frame(texture: texture, index: index, command: command)
    }

    func discard(_ frame: Frame) { release(frame.index) }

    func present(_ frame: Frame, drawable: (any MTLDrawable)?,
                 presented: @escaping @Sendable (Double) -> Void,
                 completion: @escaping @Sendable (NSError?) -> Void) {
        #if !targetEnvironment(simulator)
        if let drawable { drawable.addPresentedHandler { value in
            if value.presentedTime > 0 { presented(value.presentedTime) }
        } }
        #endif
        frame.command.addCompletedHandler { [self, frame] command in
            release(frame.index)
            completion(command.error as NSError?)
        }
        if let drawable { frame.command.present(drawable) }
        frame.command.commit()
    }

    private func release(_ index: Int) {
        lock.lock(); available.append(index); lock.unlock()
    }
}
