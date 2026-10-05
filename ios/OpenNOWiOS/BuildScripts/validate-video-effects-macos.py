#!/usr/bin/env python3
"""Validate the shared video-effects implementations on a physical Mac GPU.

MetalFX is absent from the iOS simulator SDK. These synthetic checks exercise
real MetalFX HDR upscaling/readback for decoded video.
They do not establish iPhone/iPad capabilities or real-time performance.
Run from the repository root on an Apple Silicon Mac with Xcode providing the macOS 27 SDK.
"""
from pathlib import Path
import platform
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
source = (root / 'ios/OpenNOWiOS/OpenNOWiOS/NativeStreamVideoEffects.swift').read_text()
if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    raise SystemExit('These checks require an Apple Silicon Mac.')

METALFX = r"""
@main struct MetalFXCheck {
 @MainActor static func main() async throws {
  setbuf(stdout, nil)
  guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else { fatalError("No Metal device") }
  precondition(MTLFXSpatialScalerDescriptor.supportsDevice(device), "MetalFX spatial unsupported")
  let context = CIContext(mtlDevice: device, options: [.cacheIntermediates: false])
  let space = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: true)
  let scaler = NativeStreamSpatialUpscaler(device: device)
  var pixels = [Float](repeating: 0, count: 64 * 32 * 4)
  for y in 0..<32 { for x in 0..<64 {
   let i = (y * 64 + x) * 4
   pixels[i] = x < 32 ? 4 : 0.1
   pixels[i+1] = y < 16 ? 0.1 : 2
   pixels[i+2] = 0.25; pixels[i+3] = 1
  } }
  let image = pixels.withUnsafeBytes { CIImage(bitmapData: Data($0), bytesPerRow: 64 * 16,
   size: CGSize(width: 64, height: 32), format: .RGBAf, colorSpace: space) }
  var result: CIImage?
  for _ in 0..<500 {
   let command = queue.makeCommandBuffer()!
   result = scaler.encode(image: image, sourceSize: image.extent.size, destinationSize: CGSize(width: 128, height: 64),
    hdr: true, context: context, commandBuffer: command)
   command.commit(); await command.completed()
   precondition(command.status == .completed, "GPU command failed")
   if result != nil { break }
   try await Task.sleep(nanoseconds: 10_000_000)
  }
  guard let output = result else { fatalError(scaler.status) }
  let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: 128, height: 64, mipmapped: false)
  td.storageMode = .shared; td.usage = [.renderTarget, .shaderRead, .shaderWrite]
  let texture = device.makeTexture(descriptor: td)!, command = queue.makeCommandBuffer()!
  context.render(output, to: texture, commandBuffer: command, bounds: output.extent, colorSpace: space)
  command.commit(); await command.completed()
  precondition(command.status == .completed)
  var values = [Float](repeating: 0, count: 128 * 64 * 4)
  values.withUnsafeMutableBytes { texture.getBytes($0.baseAddress!, bytesPerRow: 128*16,
   from: MTLRegionMake2D(0,0,128,64), mipmapLevel: 0) }
  let reference = device.makeTexture(descriptor: td)!, referenceCommand = queue.makeCommandBuffer()!
  context.render(image.transformed(by: CGAffineTransform(scaleX: 2, y: 2)), to: reference,
   commandBuffer: referenceCommand, bounds: CGRect(x:0,y:0,width:128,height:64), colorSpace: space)
  referenceCommand.commit(); await referenceCommand.completed()
  var baseline = [Float](repeating: 0, count: 128 * 64 * 4)
  baseline.withUnsafeMutableBytes { reference.getBytes($0.baseAddress!, bytesPerRow: 128*16,
   from: MTLRegionMake2D(0,0,128,64), mipmapLevel: 0) }
  print("Orientation reference green vs MetalFX green:", [baseline[4129],baseline[28705]], [values[4129],values[28705]])
  let reds = [values[4128], values[4576]]
  let greens = [values[4129], values[28705]]
  print("MetalFX HDR output", output.extent.size, "red", reds, "green", greens)
  precondition(reds[0] > 3 && reds[1] < 0.5 && abs(greens[0]-baseline[4129]) < 0.1 && abs(greens[1]-baseline[28705]) < 0.1,
   "HDR highlight or orientation regression")
  print("PASS: real MetalFX spatial GPU encode, HDR highlights >1, orientation matches ordinary playback")
  // Compare actual decoded-buffer layouts against ordinary playback as well.
  for hdr in [false, true] {
   var allocation: CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil, 64, 32, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    [kCVPixelBufferIOSurfacePropertiesKey: [:], kCVPixelBufferMetalCompatibilityKey: true] as CFDictionary, &allocation) == kCVReturnSuccess)
   let buffer = allocation!
   CVPixelBufferLockBaseAddress(buffer, [])
   let yBase = CVPixelBufferGetBaseAddressOfPlane(buffer,0)!.assumingMemoryBound(to: UInt8.self)
   let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer,0)
   for y in 0..<32 { for x in 0..<64 { yBase[y*stride+x] = y < 16 ? (x < 32 ? 40 : 90) : (x < 32 ? 160 : 220) } }
   memset(CVPixelBufferGetBaseAddressOfPlane(buffer,1)!,128,CVPixelBufferGetBytesPerRowOfPlane(buffer,1)*16)
   CVPixelBufferUnlockBaseAddress(buffer, [])
   CVBufferSetAttachment(buffer,kCVImageBufferColorPrimariesKey,hdr ? kCVImageBufferColorPrimaries_ITU_R_2020 : kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
   CVBufferSetAttachment(buffer,kCVImageBufferTransferFunctionKey,hdr ? kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ : kCVImageBufferTransferFunction_ITU_R_709_2,.shouldPropagate)
   CVBufferSetAttachment(buffer,kCVImageBufferYCbCrMatrixKey,hdr ? kCVImageBufferYCbCrMatrix_ITU_R_2020 : kCVImageBufferYCbCrMatrix_ITU_R_709_2,.shouldPropagate)
   let inputImage = CIImage(cvPixelBuffer: buffer)
   let sampleSpace = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr: hdr)
   var effect: CIImage?
   for _ in 0..<500 {
    let command = queue.makeCommandBuffer()!
    effect = scaler.encode(image: inputImage, sourceSize: inputImage.extent.size,
     destinationSize: CGSize(width:128,height:64),hdr:hdr,context:context,commandBuffer:command)
    command.commit(); await command.completed(); precondition(command.status == .completed)
    if effect != nil { break }; try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(effect != nil,scaler.status)
   func pixels(_ image: CIImage) async -> [Float] {
    let target = device.makeTexture(descriptor:td)!, command = queue.makeCommandBuffer()!
    context.render(image,to:target,commandBuffer:command,bounds:CGRect(x:0,y:0,width:128,height:64),colorSpace:sampleSpace)
    command.commit(); await command.completed(); precondition(command.status == .completed)
    var value = [Float](repeating:0,count:128*64*4)
    value.withUnsafeMutableBytes { target.getBytes($0.baseAddress!,bytesPerRow:128*16,from:MTLRegionMake2D(0,0,128,64),mipmapLevel:0) }
    return value
   }
   let expected = await pixels(inputImage.transformed(by:CGAffineTransform(scaleX:2,y:2)))
   let actual = await pixels(effect!)
   for offset in [4128,4576,28704,29152] {
    precondition(abs(actual[offset]-expected[offset]) < max(0.05,expected[offset]*0.05),"NV12 MetalFX orientation/color differs from normal playback")
   }
   print("PASS: NV12",hdr ? "PQ HDR" : "SDR","MetalFX drawable orientation matches normal playback")
  }
 }
}
"""

with tempfile.TemporaryDirectory(prefix='opennow-video-effects-') as temporary:
    for name, body in [('MetalFX', METALFX)]:
        text = source
        swift = Path(temporary) / (name + '.swift')
        executable = Path(temporary) / name
        swift.write_text(text + body)
        subprocess.run(['xcrun', 'swiftc', '-module-name', 'OpenNOWVideoEffectsCheck', '-parse-as-library', '-target',
                        'arm64-apple-macos26.0', str(swift), '-o', str(executable)], check=True)
        subprocess.run([str(executable)], check=True, timeout=60)
