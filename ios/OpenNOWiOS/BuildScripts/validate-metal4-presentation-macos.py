#!/usr/bin/env python3
"""Validate bounded Metal 4 → compatible presentation using real MTKView drawables.

Read back changing frames, resize drawable pools, and hold three GPU frames at once.
This exercises production synchronization/slot ownership; it cannot reproduce the
physical iPhone's reported tearing or establish its sustained display frame rate.
"""
from pathlib import Path
import os
import platform
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    raise SystemExit('Requires Apple Silicon, macOS 26+ and Xcode.')
source = '\n'.join((root/'ios/OpenNOWiOS/OpenNOWiOS'/name).read_text() for name in
    ['NativeStreamHDRMetal.swift', 'NativeStreamMetal4Presentation.swift'])
CHECK = r'''
import AppKit
import MetalKit

final class DrawDelegate: NSObject, MTKViewDelegate {
 var action: ((MTKView) -> Void)?
 func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}
 func draw(in view: MTKView) { action?(view) }
}
final class PresentationCount: @unchecked Sendable {
 private let lock = NSLock()
 private var count = 0
 func record() { lock.lock(); count += 1; lock.unlock() }
 var value: Int { lock.lock(); defer { lock.unlock() }; return count }
}
@main struct Check {
 @MainActor static func main() async throws {
  setbuf(stdout,nil)
  let app = NSApplication.shared
  app.setActivationPolicy(.accessory)
  let device = MTLCreateSystemDefaultDevice()!, queue = device.makeCommandQueue()!
  let renderer = NativeStreamMetal4HDRRenderer(device:device)!
  let compatible = NativeStreamHDRMetalRenderer(device:device)!
  let presentation = NativeStreamMetal4Presentation(queue:queue)
  let window = NSWindow(contentRect:NSRect(x:100,y:100,width:320,height:160),
      styleMask:.titled,backing:.buffered,defer:false)
  let view = MTKView(frame:window.contentView!.bounds,device:device)
  view.isPaused = true; view.enableSetNeedsDisplay = false; view.framebufferOnly = false
  view.colorPixelFormat = .bgr10a2Unorm
  let delegate = DrawDelegate(); view.delegate = delegate
  window.contentView = view; window.orderFront(nil)
  let layer = view.layer as! CAMetalLayer
  layer.maximumDrawableCount = 3
  layer.colorspace = CGColorSpace(name:CGColorSpace.itur_2100_PQ)
  layer.wantsExtendedDynamicRangeContent = true
  view.autoResizeDrawable = false
  let callbacks = PresentationCount()
  var total = 0
  func fixture(_ phase:Int) -> CVPixelBuffer {
   var allocation:CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
    [kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true]
    as CFDictionary,&allocation) == kCVReturnSuccess)
   let buffer = allocation!
   CVPixelBufferLockBaseAddress(buffer,[])
   for plane in 0..<2 {
    let pixels = CVPixelBufferGetBaseAddressOfPlane(buffer,plane)!.assumingMemoryBound(to:UInt16.self)
    let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer,plane)/2
    for row in 0..<32 { for column in 0..<64 {
     if plane == 0 { pixels[row*stride+column] = UInt16((column+phase*7)%64 < 32 ? 240 : 720)<<6 }
     else { pixels[row*stride+2*column] = 512<<6; pixels[row*stride+2*column+1] = UInt16(row < 16 ? 480 : 580)<<6 }
    } }
   }
   CVPixelBufferUnlockBaseAddress(buffer,[])
   CVBufferSetAttachment(buffer,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,.shouldPropagate)
   CVBufferSetAttachment(buffer,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_2020,.shouldPropagate)
   CVBufferSetAttachment(buffer,kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_2020,.shouldPropagate)
   return buffer
  }
  func sharedTarget(_ width:Int,_ height:Int) -> any MTLTexture {
   let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgr10a2Unorm,
       width:width,height:height,mipmapped:false)
   descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
   return device.makeTexture(descriptor:descriptor)!
  }
  func pixels(_ texture:any MTLTexture) -> [UInt32] {
   var result = [UInt32](repeating:0,count:texture.width*texture.height)
   result.withUnsafeMutableBytes { texture.getBytes($0.baseAddress!,bytesPerRow:texture.width*4,
       from:MTLRegionMake2D(0,0,texture.width,texture.height),mipmapLevel:0) }
   return result
  }
  // Let producers finish while deliberately holding the compatible copy queue.
  // Producer feedback must not make its output texture available for reuse.
  let consumerGate = device.makeSharedEvent()!
  let producerTimeline = NativeStreamMetalFrameTimeline(device:device)!
  let producers = AsyncStream<Bool>.makeStream(), consumers = AsyncStream<Bool>.makeStream()
  let heldTarget = sharedTarget(128,64)
  for index in 0..<3 {
   let ticket = producerTimeline.next(), frame = presentation.prepare(target:heldTarget,ticket:ticket)!
   frame.command.encodeWaitForEvent(consumerGate,value:1)
   precondition(renderer.submit(buffer:fixture(index),target:frame.texture,
     destination:CGRect(x:0,y:0,width:128,height:64),ticket:ticket,waitForPrevious:false) { _,error in
    producers.continuation.yield(error == nil)
   })
   producerTimeline.accept(ticket)
   presentation.present(frame,drawable:nil,presented:{ _ in }) { error in
    consumers.continuation.yield(error == nil)
   }
  }
  var producersDone = 0
  for await success in producers.stream { precondition(success); producersDone += 1; if producersDone == 3 { break } }
  producers.continuation.finish()
  precondition(presentation.prepare(target:heldTarget,ticket:producerTimeline.next()) == nil,
    "Producer feedback released a texture still owned by the copy consumer")
  consumerGate.signaledValue = 1
  var consumersDone = 0
  for await success in consumers.stream { precondition(success); consumersDone += 1; if consumersDone == 3 { break } }
  consumers.continuation.finish()
  print("PASS: completed producers cannot reuse textures until compatible copies finish")
  for dimensions in [(128,64),(192,96),(128,64)] {
   view.drawableSize = CGSize(width:dimensions.0,height:dimensions.1)
   view.releaseDrawables()
   for batch in 0..<10 {
    let finished = AsyncStream<Bool>.makeStream()
    let gate = device.makeSharedEvent()!
    var captures:[(CVPixelBuffer,any MTLTexture)] = []
    for index in 0..<3 {
     let input = fixture(total), ticket = NativeStreamMetalFrameTimeline.Ticket(event:gate,previous:1,value:UInt64(index+2))
     var drew = false
     delegate.action = { view in
      guard let drawable = view.currentDrawable,
            let frame = presentation.prepare(target:drawable.texture,ticket:ticket) else {fatalError("No presentation frame")}
      let capture = sharedTarget(drawable.texture.width,drawable.texture.height)
      // Append test-only readback before presentation, without reading on the CPU
      // or submitting compatible work before the Metal 4 producer.
      let readback = frame.command.makeBlitCommandEncoder()!
      readback.copy(from:drawable.texture,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),
        sourceSize:MTLSize(width:capture.width,height:capture.height,depth:1),to:capture,
        destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin())
      readback.endEncoding()
      let destination = CGRect(x:0,y:0,width:capture.width,height:capture.height)
      precondition(renderer.submit(buffer:input,target:frame.texture,destination:destination,ticket:ticket) { _,error in
       precondition(error == nil,"Metal 4 producer failed")
      })
      presentation.present(frame,drawable:drawable,presented:{ _ in callbacks.record() }) { error in
       finished.continuation.yield(error == nil)
      }
      captures.append((input,capture)); drew = true
     }
     view.draw(); precondition(drew); total += 1
    }
    // All producer and copy work is blocked by the unsignaled event. The copy
    // consumer, rather than producer feedback, must own the bounded slots.
    let temporary = sharedTarget(dimensions.0,dimensions.1)
    let fourth = NativeStreamMetalFrameTimeline.Ticket(event:gate,previous:0,value:5)
    precondition(presentation.prepare(target:temporary,ticket:fourth) == nil,"Unbounded copy admission")
    gate.signaledValue = 1
    var completions = 0
    for await success in finished.stream { precondition(success); completions += 1; if completions == 3 { break } }
    finished.continuation.finish()
    for (input,capture) in captures {
     let reference = sharedTarget(capture.width,capture.height), command = queue.makeCommandBuffer()!
     let pass = MTLRenderPassDescriptor(); pass.colorAttachments[0].texture = reference
     pass.colorAttachments[0].loadAction = .clear; pass.colorAttachments[0].storeAction = .store
     pass.colorAttachments[0].clearColor = MTLClearColorMake(0,0,0,1)
     precondition(compatible.encode(buffer:input,commandBuffer:command,descriptor:pass,
       destination:CGRect(x:0,y:0,width:capture.width,height:capture.height)))
     command.commit(); await command.completed(); precondition(command.status == .completed)
     precondition(pixels(capture) == pixels(reference),"Presented frame contains stale or mixed pixels")
    }
    // A rejected producer must return its uncommitted copy slot to fallback.
    for _ in 0..<4 {
     let frame = presentation.prepare(target:temporary,ticket:fourth)!
     presentation.discard(frame)
    }
    if batch == 9 { print("PASS: 30 changing MTKView frames",dimensions,"match compatible pixels; three slots bound producer + copy") }
   }
  }
  delegate.action = nil
  for _ in 0..<100 { if callbacks.value > 0 { break }; try await Task.sleep(nanoseconds:10_000_000) }
  precondition(callbacks.value > 0,"No actual drawable presentation callbacks")
  print("PASS: actual presentation callbacks",callbacks.value,"/",total,"frames; rejected producer slots reusable")
  window.close()
 }
}
'''
with tempfile.TemporaryDirectory(prefix='opennow-metal4-presentation-') as temporary:
    swift = Path(temporary)/'Check.swift'
    executable = Path(temporary)/'Check'
    swift.write_text(source+CHECK)
    subprocess.run(['xcrun','swiftc','-parse-as-library','-target','arm64-apple-macos26.0',str(swift),'-o',str(executable)],check=True)
    subprocess.run([str(executable)],check=True,
        env=dict(os.environ,MTL_DEBUG_LAYER='1',MTL_SHADER_VALIDATION='1'),timeout=60)
