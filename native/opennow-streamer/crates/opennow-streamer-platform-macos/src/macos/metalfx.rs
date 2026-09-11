use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use objc2::exception::catch;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_metal::{
    MTLCommandBuffer, MTLDevice, MTLStorageMode, MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};

use crate::spatial::SpatialConfig;

const MTLFX_SPATIAL_PERCEPTUAL: isize = 0;

fn descriptor_class() -> Option<&'static AnyClass> {
    static LOADED: OnceLock<bool> = OnceLock::new();
    if !LOADED.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/Frameworks/MetalFX.framework/MetalFX".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        )
        .is_null()
    }) {
        return None;
    }
    AnyClass::get(c"MTLFXSpatialScalerDescriptor")
}

pub(super) fn supports_device(device: &ProtocolObject<dyn MTLDevice>) -> bool {
    let Some(class) = descriptor_class() else {
        return false;
    };
    catch(AssertUnwindSafe(|| unsafe {
        let responds: bool = msg_send![class, respondsToSelector: sel!(supportsDevice:)];
        responds && msg_send![class, supportsDevice: device]
    }))
    .unwrap_or(false)
}

#[derive(Clone)]
pub(super) struct SpatialResources {
    scaler: Retained<AnyObject>,
    pub input: Retained<ProtocolObject<dyn MTLTexture>>,
    pub output: Retained<ProtocolObject<dyn MTLTexture>>,
}

impl SpatialResources {
    pub fn new(device: &ProtocolObject<dyn MTLDevice>, config: SpatialConfig) -> Option<Self> {
        let class = descriptor_class()?;
        catch(AssertUnwindSafe(|| unsafe {
            let descriptor: Option<Retained<AnyObject>> = msg_send![class, new];
            let descriptor = descriptor?;
            let (): () = msg_send![&descriptor, setInputWidth: config.input_width];
            let (): () = msg_send![&descriptor, setInputHeight: config.input_height];
            let (): () = msg_send![&descriptor, setOutputWidth: config.output_width];
            let (): () = msg_send![&descriptor, setOutputHeight: config.output_height];
            let (): () =
                msg_send![&descriptor, setColorTextureFormat: config.format.pixel_format()];
            let (): () =
                msg_send![&descriptor, setOutputTextureFormat: config.format.pixel_format()];
            let (): () = msg_send![&descriptor, setColorProcessingMode: MTLFX_SPATIAL_PERCEPTUAL];
            let scaler: Option<Retained<AnyObject>> =
                msg_send![&descriptor, newSpatialScalerWithDevice: device];
            let scaler = scaler?;
            let input_usage: MTLTextureUsage = msg_send![&scaler, colorTextureUsage];
            let output_usage: MTLTextureUsage = msg_send![&scaler, outputTextureUsage];
            let input_descriptor =
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    config.format.pixel_format(),
                    config.input_width,
                    config.input_height,
                    false,
                );
            input_descriptor.setStorageMode(MTLStorageMode::Private);
            input_descriptor.setUsage(
                input_usage | MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
            );
            let input = device.newTextureWithDescriptor(&input_descriptor)?;
            let output_descriptor =
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    config.format.pixel_format(),
                    config.output_width,
                    config.output_height,
                    false,
                );
            output_descriptor.setStorageMode(MTLStorageMode::Private);
            output_descriptor.setUsage(output_usage | MTLTextureUsage::ShaderRead);
            let output = device.newTextureWithDescriptor(&output_descriptor)?;
            let (): () = msg_send![&scaler, setInputContentWidth: config.input_width];
            let (): () = msg_send![&scaler, setInputContentHeight: config.input_height];
            let (): () = msg_send![&scaler, setColorTexture: &*input];
            let (): () = msg_send![&scaler, setOutputTexture: &*output];
            Some(Self {
                scaler,
                input,
                output,
            })
        }))
        .ok()
        .flatten()
    }

    pub fn encode(&self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>) -> bool {
        catch(AssertUnwindSafe(|| unsafe {
            let (): () = msg_send![&self.scaler, encodeToCommandBuffer: command_buffer];
        }))
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::MetalFrameFormat;
    use objc2_metal::{
        MTLBlitCommandEncoder, MTLBuffer, MTLClearColor, MTLCommandBufferStatus, MTLCommandEncoder,
        MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLLoadAction, MTLOrigin,
        MTLRenderPassDescriptor, MTLResourceOptions, MTLSize, MTLStoreAction,
    };

    #[test]
    fn weak_metalfx_probe_is_safe_without_a_supported_device() {
        let Some(device) = MTLCreateSystemDefaultDevice() else {
            return;
        };
        let supported = supports_device(&device);
        assert_eq!(supported, supports_device(&device));
        if supported {
            assert!(descriptor_class().is_some());
        }
    }

    #[test]
    #[ignore = "requires a Mac with MetalFX spatial scaling and GPU readback"]
    fn spatial_sdr_preserves_constant_color_and_ten_bit_precision() {
        let device = MTLCreateSystemDefaultDevice().expect("Metal device");
        assert!(supports_device(&device));
        let queue = device.newCommandQueue().expect("Metal queue");
        for format in [MetalFrameFormat::Rgba8Unorm, MetalFrameFormat::Rgb10a2Unorm] {
            let config = SpatialConfig::new(64, 64, 128, 128, format).unwrap();
            let mut cached =
                Some(SpatialResources::new(&device, config).expect("SDR spatial scaler"));
            let max_code = if format == MetalFrameFormat::Rgba8Unorm {
                255
            } else {
                1023
            };
            let code = if max_code == 255 { 83 } else { 333 };
            for iteration in 0..3 {
                let resources = cached.as_ref().unwrap();
                let command = queue.commandBuffer().expect("command buffer");
                let pass = MTLRenderPassDescriptor::renderPassDescriptor();
                let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
                attachment.setTexture(Some(&resources.input));
                attachment.setLoadAction(MTLLoadAction::Clear);
                attachment.setStoreAction(MTLStoreAction::Store);
                attachment.setClearColor(MTLClearColor {
                    red: f64::from(code) / f64::from(max_code),
                    green: f64::from(code) / f64::from(max_code),
                    blue: f64::from(code) / f64::from(max_code),
                    alpha: 1.0,
                });
                command
                    .renderCommandEncoderWithDescriptor(&pass)
                    .unwrap()
                    .endEncoding();
                assert!(resources.encode(&command));
                let readback = device
                    .newBufferWithLength_options(
                        128 * 128 * 4,
                        MTLResourceOptions::StorageModeShared,
                    )
                    .unwrap();
                let blit = command.blitCommandEncoder().unwrap();
                unsafe {
                    blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                        &resources.output, 0, 0, MTLOrigin { x: 0, y: 0, z: 0 },
                        MTLSize { width: 128, height: 128, depth: 1 }, &readback, 0, 128 * 4, 128 * 128 * 4,
                    );
                }
                blit.endEncoding();
                let retained = resources.clone();
                let completed = block2::RcBlock::new(
                    move |_: std::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                        let _retained_until_completion = &retained;
                    },
                );
                unsafe { command.addCompletedHandler(block2::RcBlock::as_ptr(&completed)) };
                drop(completed);
                if iteration == 2 {
                    cached = None;
                }
                command.commit();
                command.waitUntilCompleted();
                assert_eq!(command.status(), MTLCommandBufferStatus::Completed);
                let pixel = unsafe {
                    *readback
                        .contents()
                        .cast::<u32>()
                        .as_ptr()
                        .add(64 * 128 + 64)
                };
                let channels = if max_code == 255 {
                    [pixel & 255, (pixel >> 8) & 255, (pixel >> 16) & 255]
                } else {
                    [pixel & 1023, (pixel >> 10) & 1023, (pixel >> 20) & 1023]
                };
                for channel in channels {
                    assert!(
                        (channel as i32 - code).abs() <= 1,
                        "{format:?}: {channels:?}"
                    );
                }
            }
        }
    }
}
