#!/usr/bin/env python3
"""Validate actual Metal 4 MetalFX/CI interop pixels on Apple Silicon."""
from pathlib import Path
import os,subprocess,tempfile
root=Path(__file__).resolve().parents[3]
source="\n".join((root/"ios/OpenNOWiOS/OpenNOWiOS"/name).read_text() for name in ["NativeStreamVideoEffects.swift","NativeStreamHDRMetal.swift","NativeStreamMetal4Effects.swift","NativeStreamMetal4Presentation.swift"])
CHECK = r"""
import QuartzCore
@main struct EffectsCheck {
 @MainActor static func main() async throws {
  setbuf(stdout,nil)
  let device = MTLCreateSystemDefaultDevice()!, queue = device.makeCommandQueue()!
  let context = CIContext(mtlDevice: device,options: [.cacheIntermediates:false])
  let renderer = NativeStreamMetal4EffectsRenderer(device: device)!
  let layer=CAMetalLayer();layer.device=device;layer.pixelFormat = .bgr10a2Unorm
  layer.drawableSize=CGSize(width:128,height:64);layer.framebufferOnly=false
  func fixture(format:OSType,transfer:CFString,phase:Int = 0, width:Int = 64, height:Int = 32) -> CVPixelBuffer {
   var allocation: CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,width,height,format,
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
  func target(width:Int = 128, height:Int = 64) -> any MTLTexture {
   let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgr10a2Unorm,width:width,height:height,mipmapped:false)
   descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
   return device.makeTexture(descriptor:descriptor)!
  }

  func pixels(_ texture: any MTLTexture) -> [UInt32] {
   var v = [UInt32](repeating:0,count:texture.width*texture.height)
   v.withUnsafeMutableBytes { texture.getBytes($0.baseAddress!,bytesPerRow:texture.width*4,from:MTLRegionMake2D(0,0,texture.width,texture.height),mipmapLevel:0) }
   return v
  }
  let formats:[OSType] = [kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
    kCVPixelFormatType_422YpCbCr10BiPlanarFullRange,kCVPixelFormatType_422YpCbCr10BiPlanarVideoRange,
    kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange]
  for format in formats { for transfer in [1,2] { for upscale in [false,true] { for native in [false,true] {
   let input = fixture(format: format,transfer: transfer == 1 ? kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ : kCVImageBufferTransferFunction_ITU_R_2100_HLG)
   let image = CIImage(cvPixelBuffer:input)
   let output = target(), reference = { () -> any MTLTexture in
    let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba16Float,width:128,height:64,mipmapped:false)
    d.storageMode = .shared; d.usage = [.shaderRead,.renderTarget,.shaderWrite]; return device.makeTexture(descriptor:d)!
   }()
   let destination = upscale ? CGRect(x:16,y:8,width:96,height:48) : CGRect(x:32,y:16,width:64,height:32)
   let space = CGColorSpace(name:CGColorSpace.itur_2100_PQ)!
   let legacy = NativeStreamSpatialUpscaler(device:device)
   var scaled:CIImage?
   for _ in 0..<500 {
    let c=queue.makeCommandBuffer()!
    scaled = upscale ? legacy.encode(image:image,sourceSize:image.extent.size,destinationSize:destination.size,hdr:true,context:context,commandBuffer:c) : image
    if let scaled {
     let result = scaled.transformed(by:CGAffineTransform(translationX:destination.minX,y:destination.minY).scaledBy(x:destination.width/scaled.extent.width,y:destination.height/scaled.extent.height))
     context.render(result,to:reference,commandBuffer:c,bounds:CGRect(x:0,y:0,width:128,height:64),colorSpace:space)
    }
    c.commit(); await c.completed(); precondition(c.status == .completed)
    if scaled != nil { break }; try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(scaled != nil)
   var success=false
   for _ in 0..<500 {
    let producer=queue.makeCommandBuffer()!
    let pair=AsyncStream<Bool>.makeStream()
    let finished: @Sendable (Double,NSError?) -> Void = { _,error in
     if let error { print(error) }; pair.continuation.yield(error == nil); pair.continuation.finish()
    }
    success = native
      ? renderer.submit(buffer:input,destination:destination,upscale:upscale,target:output,completion:finished)
      : renderer.submit(image:image,destination:destination,transfer:transfer,upscale:upscale,context:context,producer:producer,target:output,completion:finished)
    if success { for await ok in pair.stream { precondition(ok) }; break }
    producer.commit(); await producer.completed(); try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(success,renderer.status)
   let actual=pixels(output)
   var ref=[UInt16](repeating:0,count:128*64*4)
   ref.withUnsafeMutableBytes { reference.getBytes($0.baseAddress!,bytesPerRow:128*8,from:MTLRegionMake2D(0,0,128,64),mipmapLevel:0) }
   var expected=[UInt32](repeating:0,count:128*64)
   for y in 0..<64 { for x in 0..<128 {
    let i=((63-y)*128+x)*4
    let r=UInt32(min(1023,max(0,(Float(Float16(bitPattern:ref[i]))*1023).rounded())))
    let g=UInt32(min(1023,max(0,(Float(Float16(bitPattern:ref[i+1]))*1023).rounded())))
    let b=UInt32(min(1023,max(0,(Float(Float16(bitPattern:ref[i+2]))*1023).rounded())))
    expected[y*128+x]=r<<20|g<<10|b
   } }
   // CI drawable rendering and Metal fragment output must agree in orientation,
   // encoded transfer, gamut, range and chroma detail. Exclude fit boundaries.
   var maximum=0
   for y in Int(destination.minY+4)..<Int(destination.maxY-4) { for x in Int(destination.minX+4)..<Int(destination.maxX-4) { for shift in [0,10,20] {
    maximum=max(maximum,abs(Int((actual[y*128+x]>>shift)&1023)-Int((expected[y*128+x]>>shift)&1023)))
   } } }
   print("compare",native ? "native" : "CI",String(format:"%08x",format),transfer,upscale,"maximum",maximum,"top",actual[16*128+40]&1023,expected[16*128+40]&1023,"bottom",actual[48*128+40]&1023,expected[48*128+40]&1023)
   precondition(maximum<=4,"Metal 4 effects transfer/orientation differs")
   precondition(actual[0]&0x3fffffff==0,"Fit border not black")
   print(native ? "PASS: Metal 4 native HDR effects" : "PASS: Metal 4 effects",format,transfer,upscale)
  } } } }


  // Change the contents of each recycled IOSurface on every reuse, on another GPU queue. Static
  // CPU fixtures cannot exercise decoder-like aliasing and frame-to-frame reuse.
  let direct = NativeStreamMetal4HDRRenderer(device:device)!
  let compatible = NativeStreamHDRMetalRenderer(device:device)!
  var cache:CVMetalTextureCache?
  precondition(CVMetalTextureCacheCreate(nil,nil,device,nil,&cache)==kCVReturnSuccess)
  let pool=(0..<2).map { _ in fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
    transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ) }
  func plane(_ buffer:CVPixelBuffer,_ index:Int)->(CVMetalTexture,any MTLTexture) {
   var wrapper:CVMetalTexture?
   precondition(CVMetalTextureCacheCreateTextureFromImage(nil,cache!,buffer,nil,index == 0 ? .r16Unorm : .rg16Unorm,
     CVPixelBufferGetWidthOfPlane(buffer,index),CVPixelBufferGetHeightOfPlane(buffer,index),index,&wrapper)==kCVReturnSuccess)
   return (wrapper!,CVMetalTextureGetTexture(wrapper!)!)
  }
  let recycledTarget=target(),goldTarget=target()
  let fit=CGRect(x:32,y:16,width:64,height:32)
  for frame in 0..<120 {
   let surface=pool[frame%pool.count]
   let donor=fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
     transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,phase:(frame / pool.count).isMultiple(of:2) ? 0 : 180)
   let producer=queue.makeCommandBuffer()!,blit=producer.makeBlitCommandEncoder()!
   var retained:[CVMetalTexture]=[]
   for index in 0..<2 {
    let source=plane(donor,index),destination=plane(surface,index)
    retained.append(source.0);retained.append(destination.0)
    blit.copy(from:source.1,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(x:0,y:0,z:0),
      sourceSize:MTLSize(width:source.1.width,height:source.1.height,depth:1),
      to:destination.1,destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin(x:0,y:0,z:0))
   }
   blit.endEncoding();producer.commit();await producer.completed();precondition(producer.status == .completed)
   _=retained
   var accepted=false
   for _ in 0..<500 {
    let pair=AsyncStream<Bool>.makeStream()
    let done:@Sendable(Double,NSError?)->Void={ _,error in
     if let error { print(error) };pair.continuation.yield(error == nil);pair.continuation.finish()
    }
    accepted=frame%3 == 0
     ? direct.submit(buffer:surface,target:recycledTarget,destination:fit,waitForPrevious:false,completion:done)
     : renderer.submit(buffer:surface,destination:fit,upscale:false,target:recycledTarget,waitForPrevious:false,completion:done)
    if accepted { for await ok in pair.stream { precondition(ok) };break }
    try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(accepted)
   // Render the compatible reference afterward so it cannot mask stale Metal 4 reads.
   let reference=queue.makeCommandBuffer()!,pass=MTLRenderPassDescriptor()
   pass.colorAttachments[0].texture=goldTarget;pass.colorAttachments[0].loadAction = .clear
   pass.colorAttachments[0].storeAction = .store;pass.colorAttachments[0].clearColor=MTLClearColorMake(0,0,0,1)
   precondition(compatible.encode(buffer:surface,commandBuffer:reference,descriptor:pass,destination:fit))
   reference.commit();await reference.completed();precondition(reference.status == .completed)
   let actual=pixels(recycledTarget),expected=pixels(goldTarget)
   for y in 18..<46 { for x in 34..<94 { for shift in [0,10,20] {
    let index=y*128+x
    precondition(abs(Int((actual[index]>>shift)&1023)-Int((expected[index]>>shift)&1023))<=4,
      "Recycled GPU-written 4:4:4 surface contains stale pixels")
   } } }
  }
  print("PASS: 120 alternating GPU-written pooled 4:4:4 HDR frames, direct/effects Metal 4 match compatible pixels")


  // Exercise the production private-texture → compatible drawable handoff.
  // Headless layers may exhaust drawables; MTKView coverage lives in its own harness.
  layer.frame=CGRect(x:0,y:0,width:128,height:64)
  layer.colorspace=CGColorSpace(name:CGColorSpace.itur_2100_PQ)
  layer.wantsExtendedDynamicRangeContent=true
  let presentation=NativeStreamMetal4Presentation(queue:queue)
  let presentationTimeline=NativeStreamMetalFrameTimeline(device:device)!
  var drawableFrames=0
  for phase in 0..<12 {
   guard let drawable=layer.nextDrawable() else { break }
   let ticket=presentationTimeline.next()
   guard let frame=presentation.prepare(target:drawable.texture,ticket:ticket) else {fatalError("No copy slot")}
   let pair=AsyncStream<Bool>.makeStream()
   let done:@Sendable(Double,NSError?)->Void={ _,error in precondition(error == nil) }
   let input=fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
     transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,phase:phase.isMultiple(of:2) ? 0 : 180)
   let accepted=phase.isMultiple(of:2)
    ? direct.submit(buffer:input,target:frame.texture,destination:fit,ticket:ticket,completion:done)
    : renderer.submit(buffer:input,destination:CGRect(x:16,y:8,width:96,height:48),upscale:true,
        target:frame.texture,ticket:ticket,completion:done)
   precondition(accepted)
   presentationTimeline.accept(ticket)
   presentation.present(frame,drawable:drawable,presented:{ _ in }) { error in
    pair.continuation.yield(error == nil);pair.continuation.finish()
   }
   for await ok in pair.stream { precondition(ok) }
   drawableFrames+=1
  }
  precondition(drawableFrames>0,"No drawable available for Metal 4 presentation validation")
  print("PASS: private Metal 4 HDR/MetalFX frames use compatible drawable presentation",drawableFrames,"frames")

  // The reported phone geometry must really execute MetalFX, including its
  // private-output → compatible-copy handoff, rather than passing via HDR alone.
  let phone = CGSize(width:2868,height:1320), fullSource = CGSize(width:2560,height:1080)
  let phoneTimeline = NativeStreamMetalFrameTimeline(device:device)!
  for stretch in [false,true] {
   let fitted = NativeStreamVideoEffectsPolicy.presentationSize(source:fullSource,display:phone,stretch:stretch)
   let expectedSize = NativeStreamVideoEffectsPolicy.upscaleSize(source:fullSource,destination:fitted)!
   precondition(expectedSize == CGSize(width:2868,height:stretch ? 1320 : 1210))
   let destination = CGRect(x:0,y:(phone.height-fitted.height)/2,width:fitted.width,height:fitted.height)
   let done = AsyncStream<Bool>.makeStream()
   var captures:[(any MTLTexture,any MTLTexture)] = []
   for phase in 0..<3 {
    let surface = fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
        transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,phase:phase*20,width:2560,height:1080)
    let output = target(width:2868,height:1320), reference = target(width:2868,height:1320)
    let ticket = phoneTimeline.next()
    var accepted = false
    for _ in 0..<500 {
     guard let frame = presentation.prepare(target:output,ticket:ticket) else {fatalError("No phone copy slot")}
     let readback = frame.command.makeBlitCommandEncoder()!
     readback.copy(from:frame.texture,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),
         sourceSize:MTLSize(width:2868,height:1320,depth:1),to:reference,
         destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin())
     readback.endEncoding()
     accepted = renderer.submit(buffer:surface,destination:destination,upscale:true,target:frame.texture,
         ticket:ticket,waitForPrevious:false) { _,error in precondition(error == nil) }
     if accepted {
      phoneTimeline.accept(ticket)
      precondition(renderer.status == "Metal 4 · 2560×1080 → 2868×\(Int(expectedSize.height))",
          "Near-native geometry skipped MetalFX")
      presentation.present(frame,drawable:nil,presented:{ _ in }) { error in done.continuation.yield(error == nil) }
      captures.append((output,reference)); break
     }
     presentation.discard(frame)
     try await Task.sleep(nanoseconds:10_000_000)
    }
    precondition(accepted,renderer.status)
   }
   var finished = 0
   for await success in done.stream { precondition(success); finished += 1; if finished == 3 { break } }
   done.continuation.finish()
   for (output,reference) in captures {
    let actual = pixels(output)
    precondition(actual == pixels(reference),"Display copy changed the upscaled frame")
    let top = Int(destination.minY+destination.height*0.25)*2868+1434
    let bottom = Int(destination.minY+destination.height*0.75)*2868+1434
    precondition((actual[top]&1023) < (actual[bottom]&1023),"Upscaled image is blank or upside down")
    if !stretch { precondition((actual[1434]&0x3fffffff)==0,"Fitted HDR border was not cleared") }
   }
   print("PASS: 2560×1080 native PQ MetalFX →",Int(expectedSize.width),Int(expectedSize.height),
       "three private frames copied exactly; stretch",stretch)
  }

  let nativeDestination = CGRect(x:16,y:8,width:96,height:48)
  let timeline = NativeStreamMetalFrameTimeline(device:device)!
  func renderSwitch(_ input:CVPixelBuffer,_ output:any MTLTexture,native:Bool,
                    ticket:NativeStreamMetalFrameTimeline.Ticket? = nil) async throws {
   for _ in 0..<500 {
    let producer=queue.makeCommandBuffer()!
    if let ticket, ticket.previous > 0 { producer.encodeWaitForEvent(ticket.event,value:ticket.previous) }
    let pair=AsyncStream<Bool>.makeStream()
    let done: @Sendable (Double,NSError?) -> Void = { _,error in
     if let error { print(error) }; pair.continuation.yield(error == nil);pair.continuation.finish()
    }
    let accepted = native
      ? renderer.submit(buffer:input,destination:nativeDestination,upscale:true,target:output,ticket:ticket,completion:done)
      : renderer.submit(image:CIImage(cvPixelBuffer:input),destination:nativeDestination,transfer:1,upscale:true,
          context:context,producer:producer,target:output,ticket:ticket,completion:done)
    if accepted { for await success in pair.stream { precondition(success) }; return }
    try await Task.sleep(nanoseconds:10_000_000)
   }
   fatalError("Effects setup unavailable: \(renderer.status)")
  }
  let warm=fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ)
  let switched=target(),switchReference=target()
  try await renderSwitch(warm,switched,native:true)
  for phase in 0..<40 {
   let input=fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
     transfer:kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,phase:phase)
   let ticket=timeline.next()
   try await renderSwitch(input,switched,native:phase.isMultiple(of:2),ticket:ticket)
   timeline.accept(ticket)
   try await renderSwitch(input,switchReference,native:false)
   let actual=pixels(switched),reference=pixels(switchReference)
   for i in actual.indices { for shift in [0,10,20] {
    precondition(abs(Int((actual[i]>>shift)&1023)-Int((reference[i]>>shift)&1023))<=4,
      "Native/CI switching lost HDR chroma, synchronization or orientation")
   } }
  }
  print("PASS: 40 native PQ ↔ CI MetalFX switches retain pixels and shared timeline")
  let gate=device.makeSharedEvent()!, pending=AsyncStream<Bool>.makeStream()
  let done: @Sendable (Double,NSError?) -> Void = { _,error in pending.continuation.yield(error == nil) }
  precondition(renderer.submit(buffer:warm,destination:nativeDestination,upscale:true,target:switched,
    ticket:NativeStreamMetalFrameTimeline.Ticket(event:gate,previous:1,value:2),completion:done))
  precondition(renderer.submit(buffer:warm,destination:nativeDestination,upscale:true,target:switched,
    ticket:NativeStreamMetalFrameTimeline.Ticket(event:gate,previous:2,value:3),completion:done))
  precondition(renderer.submit(buffer:warm,destination:nativeDestination,upscale:true,target:switched,
    ticket:NativeStreamMetalFrameTimeline.Ticket(event:gate,previous:3,value:4),completion:done))
  precondition(!renderer.submit(buffer:warm,destination:nativeDestination,upscale:true,target:switched,completion:done),
    "Native effects exceeded three slots")
  gate.signaledValue=1;var finished=0
  for await success in pending.stream { precondition(success);finished+=1;if finished==3 { break } }
  pending.continuation.finish()
  print("PASS: native PQ effects bound GPU work to three retained slots")
  for transfer in ["UnknownTransfer" as CFString] {
   let invalid=fixture(format:kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,transfer:transfer)
   precondition(!renderer.submit(buffer:invalid,destination:nativeDestination,upscale:true,target:switched) { _,_ in fatalError("Invalid transfer submitted") })
  }
  CVBufferSetAttachment(warm,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
  precondition(!renderer.submit(buffer:warm,destination:nativeDestination,upscale:true,target:switched) { _,_ in fatalError("Invalid gamut submitted") })
  print("PASS: Unknown transfer or mismatched HDR gamut inputs retain the supported fallback")


  // Direct SDR planes and tagged linear-RGB surfaces must preserve the
  // legacy renderer's color and orientation with/without spatial scaling.
  func compareNative(_ input:CVPixelBuffer,hdr:Bool,upscale:Bool,sharpen:Float = 0,
                     referenceImage:CIImage? = nil,nativeReference:CVPixelBuffer? = nil,label:String) async throws {
   let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:hdr ? .bgr10a2Unorm : .bgra8Unorm,width:128,height:64,mipmapped:false)
   d.storageMode = .shared;d.usage = [.renderTarget,.shaderRead]
   let output=device.makeTexture(descriptor:d)!
   let rd=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba16Float,width:128,height:64,mipmapped:false)
   rd.storageMode = .shared;rd.usage = [.renderTarget,.shaderRead,.shaderWrite]
   let reference=device.makeTexture(descriptor:rd)!
   let image=referenceImage ?? CIImage(cvPixelBuffer:input)
   let destination=upscale ? CGRect(x:16,y:8,width:96,height:48) : CGRect(x:32,y:16,width:64,height:32)
   let legacy=NativeStreamSpatialUpscaler(device:device)
   for _ in 0..<500 {
    let c=queue.makeCommandBuffer()!
    let scaled=upscale ? legacy.encode(image:image,sourceSize:image.extent.size,destinationSize:destination.size,hdr:hdr,context:context,commandBuffer:c) : image
    if let scaled { context.render(scaled.transformed(by:CGAffineTransform(translationX:destination.minX,y:destination.minY).scaledBy(x:destination.width/scaled.extent.width,y:destination.height/scaled.extent.height)),to:reference,commandBuffer:c,bounds:CGRect(x:0,y:0,width:128,height:64),colorSpace:CGColorSpace(name:hdr ? CGColorSpace.itur_2100_PQ : CGColorSpace.sRGB)!) }
    c.commit();await c.completed();precondition(c.status == .completed)
    if scaled != nil { break };try await Task.sleep(nanoseconds:10_000_000)
   }
   var submitted=false
   for _ in 0..<500 {
    let pair=AsyncStream<Bool>.makeStream()
    submitted=renderer.submit(buffer:input,destination:destination,upscale:upscale,target:output,sharpening:sharpen) { _,error in
     if let error { print(error) };pair.continuation.yield(error == nil);pair.continuation.finish()
    }
    if submitted { for await ok in pair.stream { precondition(ok) };break }
    try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(submitted,renderer.status)
   let actual=pixels(output)
   var golden:[UInt32]?
   if let nativeReference {
    let nativeOutput=device.makeTexture(descriptor:d)!
    var accepted=false
    for _ in 0..<500 {
     let pair=AsyncStream<Bool>.makeStream()
     accepted=renderer.submit(buffer:nativeReference,destination:destination,upscale:upscale,target:nativeOutput) { _,error in
      pair.continuation.yield(error == nil);pair.continuation.finish()
     }
     if accepted { for await ok in pair.stream { precondition(ok) };break }
     try await Task.sleep(nanoseconds:10_000_000)
    }
    precondition(accepted);golden=pixels(nativeOutput)
   }
   var values=[UInt16](repeating:0,count:128*64*4)
   values.withUnsafeMutableBytes { reference.getBytes($0.baseAddress!,bytesPerRow:128*8,from:MTLRegionMake2D(0,0,128,64),mipmapLevel:0) }
   var maximum=0,totalError=0,sampleCount=0
   for y in Int(destination.minY+4)..<Int(destination.maxY-4) { for x in Int(destination.minX+4)..<Int(destination.maxX-4) { for component in 0..<3 {
    let value=Float(Float16(bitPattern:values[((63-y)*128+x)*4+component]))
    let limit=hdr ? 1023 : 255
    let shift=hdr ? (2-component)*10 : (2-component)*8
    let expected=golden.map { Int(($0[y*128+x]>>shift)&UInt32(limit)) }
        ?? Int((max(0,min(1,value))*Float(limit)).rounded())
    let error=abs(Int((actual[y*128+x]>>shift)&UInt32(limit))-expected)
    maximum=max(maximum,error);totalError+=error;sampleCount+=1
   } } }
   print("native comparison",label,hdr,upscale,sharpen,"maximum",maximum,"mean",Double(totalError)/Double(sampleCount))
   // CPU float rounding can differ at half-float ties before the HDR scaler.
   // Bound those dark-edge differences separately; other color cases stay strict.
   let tolerance = nativeReference != nil && hdr && upscale ? 6 : (hdr || upscale ? 4 : 2)
   precondition(Double(totalError)/Double(sampleCount) < 1)
   precondition(maximum <= tolerance,"Native SDR/RGB/sharpen differs from reference")
   precondition(actual[0] & (hdr ? 0x3fffffff : 0x00ffffff) == 0,"Native fit boundary changed")
   print("PASS: native color/sharpen",label,hdr,upscale,sharpen)
  }
  let sdrFormats:[OSType] = [kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
   kCVPixelFormatType_422YpCbCr8BiPlanarFullRange,kCVPixelFormatType_422YpCbCr8BiPlanarVideoRange,
   kCVPixelFormatType_444YpCbCr8BiPlanarFullRange,kCVPixelFormatType_444YpCbCr8BiPlanarVideoRange] + formats
  for format in sdrFormats { for transfer in [kCVImageBufferTransferFunction_ITU_R_709_2,kCVImageBufferTransferFunction_sRGB] { for upscale in [false,true] { for wideGamut in [false,true] {
   var allocation:CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,format,[kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&allocation)==kCVReturnSuccess)
   let input=allocation!,ten=NativeStreamTenBitSurface.chroma(format) != nil
   CVPixelBufferLockBaseAddress(input,[])
   for plane in 0..<2 {
    let base=CVPixelBufferGetBaseAddressOfPlane(input,plane)!,stride=CVPixelBufferGetBytesPerRowOfPlane(input,plane)
    let width=CVPixelBufferGetWidthOfPlane(input,plane),height=CVPixelBufferGetHeightOfPlane(input,plane)
    for y in 0..<height { for x in 0..<(plane == 0 ? width : width*2) {
     let value=plane == 0 ? (y<height/2 ? 80 : 160) : (x.isMultiple(of:2) ? 128 : (x%4 == 1 ? 128 : 152))
     if ten { base.assumingMemoryBound(to:UInt16.self)[y*stride/2+x]=UInt16(value*4)<<6 }
     else { base.assumingMemoryBound(to:UInt8.self)[y*stride+x]=UInt8(value) }
    } }
   }
   CVPixelBufferUnlockBaseAddress(input,[])
   CVBufferSetAttachment(input,kCVImageBufferTransferFunctionKey,transfer,.shouldPropagate)
   CVBufferSetAttachment(input,kCVImageBufferColorPrimariesKey,wideGamut ? kCVImageBufferColorPrimaries_ITU_R_2020 : kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
   CVBufferSetAttachment(input,kCVImageBufferYCbCrMatrixKey,wideGamut ? kCVImageBufferYCbCrMatrix_ITU_R_2020 : kCVImageBufferYCbCrMatrix_ITU_R_709_2,.shouldPropagate)
   try await compareNative(input,hdr:false,upscale:upscale,label:"\(format)-\(transfer)-BT2020=\(wideGamut)")
  } } } }

  for transfer in [kCVImageBufferTransferFunction_sRGB,kCVImageBufferTransferFunction_ITU_R_709_2] { for upscale in [false,true] {
   var allocation:CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,kCVPixelFormatType_32BGRA,[kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&allocation)==kCVReturnSuccess)
   let input=allocation!
   CVPixelBufferLockBaseAddress(input,[])
   let base=CVPixelBufferGetBaseAddress(input)!.assumingMemoryBound(to:UInt8.self),stride=CVPixelBufferGetBytesPerRow(input)
   for y in 0..<32 { for x in 0..<64 {
    let i=y*stride+x*4;base[i]=64;base[i+1]=x<32 ? 100 : 150;base[i+2]=y<16 ? 80 : 180;base[i+3]=255
   } }
   CVPixelBufferUnlockBaseAddress(input,[])
   CVBufferSetAttachment(input,kCVImageBufferTransferFunctionKey,transfer,.shouldPropagate)
   CVBufferSetAttachment(input,kCVImageBufferColorPrimariesKey,kCVImageBufferColorPrimaries_ITU_R_709_2,.shouldPropagate)
   try await compareNative(input,hdr:false,upscale:upscale,label:"BGRA-\(transfer)")
  } }
  // A CPU luminance unsharp reference tests the new compute pass independently
  // of Core Image's different sharpening filter, including HDR highlights.
  for hdr in [false,true] { for strength:Float in [0,0.6] { for upscale in [false,true] {
   var allocation:CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,kCVPixelFormatType_64RGBAHalf,[kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&allocation)==kCVReturnSuccess)
   let input=allocation!,space=NativeStreamVideoEffectsPolicy.workingColorSpace(hdr:hdr)
   var linear=[Float](repeating:0,count:64*32*4)
   for y in 0..<32 { for x in 0..<64 {
    let i=(y*64+x)*4,base:Float=y<16 ? 0.05 : (hdr ? 3 : 0.65)
    linear[i]=base;linear[i+1]=base*0.8+(x.isMultiple(of:4) ? 0.12 : 0)
    linear[i+2]=base*0.5;linear[i+3]=1
   } }
   CVPixelBufferLockBaseAddress(input,[])
   let pointer=CVPixelBufferGetBaseAddress(input)!.assumingMemoryBound(to:UInt16.self),stride=CVPixelBufferGetBytesPerRow(input)/2
   for y in 0..<32 { for x in 0..<64 { for c in 0..<4 {
    let i=(y*64+x)*4+c;let v=Float16(linear[i]);pointer[y*stride+x*4+c]=v.bitPattern;linear[i]=Float(v)
   } } }
   CVPixelBufferUnlockBaseAddress(input,[])
   CVBufferSetAttachment(input,kCVImageBufferCGColorSpaceKey,space,.shouldPropagate)
   CVBufferSetAttachment(input,kCVImageBufferTransferFunctionKey,kCVImageBufferTransferFunction_Linear,.shouldPropagate)
   var expected=linear
   let weights:[Float]=hdr ? [0.2627,0.678,0.0593] : [0.2126,0.7152,0.0722]
   for y in 0..<32 { for x in 0..<64 {
    let i=(y*64+x)*4
    var blur:Float=0
    for dy in -1...1 { for dx in -1...1 {
     let j=(max(0,min(31,y+dy))*64+max(0,min(63,x+dx)))*4
     let l=(0..<3).reduce(Float(0)) { $0+linear[j+$1]*weights[$1] }
     blur+=l*Float((dx == 0 ? 2 : 1)*(dy == 0 ? 2 : 1))/16
    } }
    let l=(0..<3).reduce(Float(0)) { $0+linear[i+$1]*weights[$1] }
    for c in 0..<3 { expected[i+c]=Float(Float16(max(0,min(hdr ? 65504 : 1,linear[i+c]+(l-blur)*strength*2)))) }
   } }
   let reference=expected.withUnsafeBytes { CIImage(bitmapData:Data($0),bytesPerRow:64*16,size:CGSize(width:64,height:32),format:.RGBAf,colorSpace:space) }
   var goldAllocation:CVPixelBuffer?
   precondition(CVPixelBufferCreate(nil,64,32,kCVPixelFormatType_64RGBAHalf,[kCVPixelBufferIOSurfacePropertiesKey:[:],kCVPixelBufferMetalCompatibilityKey:true] as CFDictionary,&goldAllocation)==kCVReturnSuccess)
   let gold=goldAllocation!
   CVPixelBufferLockBaseAddress(gold,[])
   let gp=CVPixelBufferGetBaseAddress(gold)!.assumingMemoryBound(to:UInt16.self),gs=CVPixelBufferGetBytesPerRow(gold)/2
   for y in 0..<32 { for x in 0..<64 { for c in 0..<4 { gp[y*gs+x*4+c]=Float16(expected[(y*64+x)*4+c]).bitPattern } } }
   CVPixelBufferUnlockBaseAddress(gold,[]);CVBufferPropagateAttachments(input,gold)
   try await compareNative(input,hdr:hdr,upscale:upscale,sharpen:strength,referenceImage:reference,nativeReference:strength > 0 ? gold : nil,label:"linear-RGB-compute")
  } } }

  for upscale in [false,true] { for sharpen in [false,true] {
   let space = CGColorSpace(name:CGColorSpace.sRGB)!
   let linear = NativeStreamVideoEffectsPolicy.workingColorSpace(hdr:false)
   var rgba=[Float](repeating:0,count:64*32*4)
   for y in 0..<32 { for x in 0..<64 {
    let i=(y*64+x)*4; rgba[i] = y<16 ? 0.05 : 0.7
    rgba[i+1] = x<32 ? 0.3 : 0.1; rgba[i+2]=0.4; rgba[i+3]=1
   } }
   var image=rgba.withUnsafeBytes { CIImage(bitmapData:Data($0),bytesPerRow:64*16,size:CGSize(width:64,height:32),format:.RGBAf,colorSpace:linear) }
   if sharpen { image=image.applyingFilter("CISharpenLuminance",parameters:[kCIInputSharpnessKey:0.6]).cropped(to:image.extent) }
   func texture(_ format:MTLPixelFormat)->any MTLTexture {
    let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:format,width:128,height:64,mipmapped:false)
    d.storageMode = .shared; d.usage = [.renderTarget,.shaderRead,.shaderWrite]; return device.makeTexture(descriptor:d)!
   }
   let output=texture(.bgra8Unorm), reference=texture(.rgba16Float)
   let destination = upscale ? CGRect(x:16,y:8,width:96,height:48) : CGRect(x:32,y:16,width:64,height:32)
   let legacy=NativeStreamSpatialUpscaler(device:device)
   for _ in 0..<500 {
    let command=queue.makeCommandBuffer()!
    let scaled=upscale ? legacy.encode(image:image,sourceSize:image.extent.size,destinationSize:destination.size,hdr:false,context:context,commandBuffer:command) : image
    if let scaled {
     context.render(scaled.transformed(by:CGAffineTransform(translationX:destination.minX,y:destination.minY).scaledBy(x:destination.width/scaled.extent.width,y:destination.height/scaled.extent.height)),
       to:reference,commandBuffer:command,bounds:CGRect(x:0,y:0,width:128,height:64),colorSpace:space)
    }
    command.commit(); await command.completed(); precondition(command.status == .completed)
    if scaled != nil { break }; try await Task.sleep(nanoseconds:10_000_000)
   }
   var success=false
   for _ in 0..<500 {
    let command=queue.makeCommandBuffer()!, pair=AsyncStream<Bool>.makeStream()
    success=renderer.submit(image:image,destination:destination,transfer:0,upscale:upscale,context:context,producer:command,target:output) { _,error in
     pair.continuation.yield(error == nil); pair.continuation.finish()
    }
    if success { for await ok in pair.stream { precondition(ok) }; break }
    command.commit(); await command.completed(); try await Task.sleep(nanoseconds:10_000_000)
   }
   precondition(success)
   let actual=pixels(output)
   var ref=[UInt16](repeating:0,count:128*64*4)
   ref.withUnsafeMutableBytes { reference.getBytes($0.baseAddress!,bytesPerRow:128*8,from:MTLRegionMake2D(0,0,128,64),mipmapLevel:0) }
   var maximum=0
   for y in Int(destination.minY+4)..<Int(destination.maxY-4) { for x in Int(destination.minX+4)..<Int(destination.maxX-4) { for (c,shift) in [16,8,0].enumerated() {
    let expected=min(255,max(0,Int((Float(Float16(bitPattern:ref[((63-y)*128+x)*4+c]))*255).rounded())))
    maximum=max(maximum,abs(Int((actual[y*128+x]>>shift)&255)-expected))
   } } }
   print("SDR compare",upscale,sharpen,"maximum",maximum)
   precondition(maximum<=2,"SDR/sharpen/MetalFX color or orientation changed")
   print("PASS: Metal 4 SDR/sharpen",upscale,sharpen,"maximum",maximum)
  } }
 }
}
"""
with tempfile.TemporaryDirectory(prefix="opennow-metal4-effects-") as temporary:
    swift=Path(temporary)/"Check.swift"; executable=Path(temporary)/"Check"
    swift.write_text(source+"""
enum NativeStreamHDRTransfer { case sdr,pq,hlg; static func detect(in buffer:CVPixelBuffer)->Self { let v=CVBufferCopyAttachment(buffer,kCVImageBufferTransferFunctionKey,nil) as? String; return v == kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ as String ? .pq : v == kCVImageBufferTransferFunction_ITU_R_2100_HLG as String ? .hlg : .sdr } }
"""+CHECK)
    subprocess.run(["xcrun","swiftc","-parse-as-library","-target","arm64-apple-macos26.0",str(swift),"-o",str(executable)],check=True)
    subprocess.run([str(executable)],check=True,env=dict(os.environ,MTL_DEBUG_LAYER="1",MTL_SHADER_VALIDATION="1"),timeout=90)
