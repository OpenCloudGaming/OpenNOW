#!/usr/bin/env python3
"""Check the production Metal 4 HDR renderer against legacy on an Apple Silicon GPU.

Uses real GPU readback, both ranges, PQ/HLG, 4:2:0/4:2:2/4:4:4, fit/orientation,
bounded admission and cross-queue switching. Does not establish iOS throughput.
"""
from pathlib import Path
import platform
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    raise SystemExit('Requires Apple Silicon, macOS 26+ and Xcode.')
source = (root/'ios/OpenNOWiOS/OpenNOWiOS/NativeStreamHDRMetal.swift').read_text()
CHECK = r'''
@main struct Metal4Check {
 @MainActor static func main() async throws {
  setbuf(stdout,nil)
  guard let device = MTLCreateSystemDefaultDevice(), device.supportsFamily(.metal4),
        let legacyQueue = device.makeCommandQueue(), let legacy = NativeStreamHDRMetalRenderer(device:device),
        let metal4 = NativeStreamMetal4HDRRenderer(device:device) else { fatalError("Metal 4 setup unavailable") }
  print("Metal 4 GPU:",device.name)
  func fixture(format:OSType,transfer:CFString,phase:Int = 0) -> CVPixelBuffer {
   var allocation: CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,format,
    [kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&allocation) == kCVReturnSuccess)
   let result = allocation!
   CVPixelBufferLockBaseAddress(result,[])
   for plane in 0..<2 {
    let pointer = CVPixelBufferGetBaseAddressOfPlane(result,plane)!.assumingMemoryBound(to:UInt16.self)
    let stride = CVPixelBufferGetBytesPerRowOfPlane(result,plane)/2
    let width = CVPixelBufferGetWidthOfPlane(result,plane),height = CVPixelBufferGetHeightOfPlane(result,plane)
    for y in 0..<height { for x in 0..<width {
     if plane == 0 { pointer[y*stride+x] = UInt16((y < height/2 ? 320 : 640)+phase) << 6 }
     else { pointer[y*stride+2*x] = 512 << 6; pointer[y*stride+2*x+1] = UInt16(x.isMultiple(of:2) ? 512 : 640) << 6 }
    } }
   }
   CVPixelBufferUnlockBaseAddress(result,[])
   CVBufferSetAttachment(result,kCVImageBufferTransferFunctionKey,transfer,.shouldPropagate)
   CVBufferSetAttachment(result,kCVImageBufferYCbCrMatrixKey,kCVImageBufferYCbCrMatrix_ITU_R_2020,.shouldPropagate)
   CVBufferSetAttachment(result,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_2020,.shouldPropagate)
   return result
  }
  func target() -> any MTLTexture {
   let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgr10a2Unorm,width:128,height:64,mipmapped:false)
   descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
   return device.makeTexture(descriptor:descriptor)!
  }
  let destination = CGRect(x:16,y:8,width:96,height:48)
  func pixels(_ texture: any MTLTexture) -> [UInt32] {
   var values = [UInt32](repeating:0,count:128*64)
   values.withUnsafeMutableBytes { texture.getBytes($0.baseAddress!,bytesPerRow:128*4,from:MTLRegionMake2D(0,0,128,64),mipmapLevel:0) }
   return values
  }
  func legacyCommand(_ input: CVPixelBuffer,_ output:any MTLTexture,_ ticket:NativeStreamMetalFrameTimeline.Ticket? = nil) -> any MTLCommandBuffer {
   let command = legacyQueue.makeCommandBuffer()!
   if let ticket,ticket.previous > 0 { command.encodeWaitForEvent(ticket.event,value:ticket.previous) }
   let pass = MTLRenderPassDescriptor(); pass.colorAttachments[0].texture = output
   pass.colorAttachments[0].loadAction = .clear; pass.colorAttachments[0].storeAction = .store
   pass.colorAttachments[0].clearColor = MTLClearColorMake(0,0,0,1)
   precondition(legacy.encode(buffer:input,commandBuffer:command,descriptor:pass,destination:destination))
   if let ticket { command.encodeSignalEvent(ticket.event,value:ticket.value) }
   return command
  }
  let formats: [OSType] = [kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
    kCVPixelFormatType_422YpCbCr10BiPlanarFullRange,kCVPixelFormatType_422YpCbCr10BiPlanarVideoRange,
    kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange]
  for format in formats { for transfer in [kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,kCVImageBufferTransferFunction_ITU_R_2100_HLG] {
   let input = fixture(format:format,transfer:transfer),reference = target(),output = target()
   let command = legacyCommand(input,reference); command.commit(); await command.completed()
   precondition(command.status == .completed)
   try await withCheckedThrowingContinuation { (continuation:CheckedContinuation<Void,Error>) in
    precondition(metal4.submit(buffer:input,target:output,destination:destination) { _,error in
     if let error { continuation.resume(throwing:error) } else { continuation.resume() }
    })
   }
   let expected = pixels(reference), actual = pixels(output)
   for index in expected.indices { for shift in [0,10,20] {
    let lhs = Int((expected[index] >> shift)&1023),rhs = Int((actual[index] >> shift)&1023)
    precondition(abs(lhs-rhs) <= 1,"Metal 4 differs from legacy HDR/color/orientation")
   } }
   precondition((actual[16*128+40]&1023) < (actual[48*128+40]&1023),"Image upside down or uniform")
   precondition((actual[0]&0x3fffffff) == 0,"Fit border not cleared")
   if NativeStreamTenBitSurface.chroma(format) == "4:4:4" {
    let row = 20*128
    let reds = (20..<108).map { Int((actual[row+$0] >> 20)&1023) }
    precondition(reds.max()!-reds.min()! > 90,"4:4:4 chroma detail lost")
   }
   print("PASS: Metal 4 matches legacy",String(format:"%08x",format),transfer,"fit/color/orientation/chroma")
  } }
  let input = fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ)
  let output = target()
  // Block the GPU with an event so all three slots remain in flight.
  let event = device.makeSharedEvent()!
  let pair = AsyncStream<Bool>.makeStream()
  let first = NativeStreamMetalFrameTimeline.Ticket(event:event,previous:1,value:2)
  let second = NativeStreamMetalFrameTimeline.Ticket(event:event,previous:2,value:3)
  let third = NativeStreamMetalFrameTimeline.Ticket(event:event,previous:3,value:4)
  let completion: @Sendable (Double,NSError?) -> Void = { _,error in pair.continuation.yield(error == nil) }
  precondition(metal4.submit(buffer:input,target:output,destination:destination,ticket:first,completion:completion))
  precondition(metal4.submit(buffer:input,target:output,destination:destination,ticket:second,completion:completion))
  precondition(metal4.submit(buffer:input,target:output,destination:destination,ticket:third,completion:completion))
  precondition(!metal4.submit(buffer:input,target:output,destination:destination,completion:completion),"Unbounded Metal 4 admission")
  event.signaledValue = 1
  var count = 0
  for await success in pair.stream { precondition(success); count += 1; if count == 3 { break } }
  pair.continuation.finish()
  print("PASS: three slots bound GPU work; a fourth is rejected; completion frees slots")

  // Submit independent frames without timeline waits, then reuse their surfaces
  // only after GPU completion. Each surface changes on every reuse.
  let surfaces = (0..<3).map { _ in fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
    transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ) }
  let outputs = (0..<3).map { _ in target() }
  for batch in 0..<40 {
   let finished = AsyncStream<Bool>.makeStream()
   for index in surfaces.indices {
    let surface = surfaces[index]
    precondition(CVPixelBufferLockBaseAddress(surface,[]) == kCVReturnSuccess)
    let y = CVPixelBufferGetBaseAddressOfPlane(surface,0)!.assumingMemoryBound(to:UInt16.self)
    let stride = CVPixelBufferGetBytesPerRowOfPlane(surface,0)/2
    for row in 0..<32 { for column in 0..<64 {
     y[row*stride+column] = UInt16(((column+batch*7)%64 < 32 ? 240 : 720)+index*30) << 6
    } }
    CVPixelBufferUnlockBaseAddress(surface,[])
    precondition(metal4.submit(buffer:surface,target:outputs[index],destination:destination,
      waitForPrevious:false) { _,error in finished.continuation.yield(error == nil) })
   }
   var completions = 0
   for await success in finished.stream {
    precondition(success); completions += 1
    if completions == surfaces.count { break }
   }
   finished.continuation.finish()
   for index in surfaces.indices {
    let reference = target(), command = legacyCommand(surfaces[index],reference)
    command.commit(); await command.completed(); precondition(command.status == .completed)
    precondition(pixels(outputs[index]) == pixels(reference),
      "Unserialized pooled HDR frames contain stale or mixed pixels")
   }
  }
  print("PASS: 120 changing pooled HDR frames without timeline waits match compatible pixels")

  let timeline = NativeStreamMetalFrameTimeline(device:device)!
  for i in 0..<40 {
   let ticket = timeline.next()
   let command = legacyCommand(input,output,ticket); command.commit(); timeline.accept(ticket)
   let next = timeline.next()
   let changed = fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,
     transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,phase:i)
   try await withCheckedThrowingContinuation { (continuation:CheckedContinuation<Void,Error>) in
    precondition(metal4.submit(buffer:changed,target:output,destination:destination,ticket:next) { _,error in
     if let error { continuation.resume(throwing:error) } else { continuation.resume() }
    })
    timeline.accept(next)
   }
   await command.completed(); precondition(command.status == .completed)
   let reference = target(),expected = legacyCommand(changed,reference); expected.commit(); await expected.completed()
   precondition(pixels(output) == pixels(reference),"Legacy/Metal 4 switching presented stale/wrong image")
  }
  print("PASS: 40 legacy → Metal 4 switches share a GPU timeline without CPU waits")
  let invalid = fixture(format:kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,transfer:kCVImageBufferTransferFunction_ITU_R_709_2)
  precondition(!metal4.submit(buffer:invalid,target:output,destination:destination) { _,_ in fatalError("Unsupported input submitted") })
  print("PASS: unsupported transfer rejects before submission, retaining legacy fallback")
 }
}
'''
with tempfile.TemporaryDirectory(prefix='opennow-metal4-hdr-') as temporary:
    swift = Path(temporary)/'Metal4Check.swift'; executable = Path(temporary)/'Metal4Check'
    swift.write_text(source+CHECK)
    subprocess.run(['xcrun','swiftc','-parse-as-library','-target','arm64-apple-macos26.0',str(swift),'-o',str(executable)],check=True)
    env = dict(os.environ,MTL_DEBUG_LAYER='1',MTL_SHADER_VALIDATION='1')
    subprocess.run([str(executable)],check=True,env=env,timeout=60)
