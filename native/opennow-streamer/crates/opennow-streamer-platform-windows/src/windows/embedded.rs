use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle, ThreadId};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{LUID, RECT};
use ::windows::Win32::Graphics::Direct3D10::ID3D10Multithread;
use ::windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
    D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_OPTIMAL_SPEED, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    ID3D11VideoContext, ID3D11VideoContext1, ID3D11VideoDevice, ID3D11VideoProcessor,
    ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorEnumerator1, ID3D11VideoProcessorInputView,
    ID3D11VideoProcessorOutputView,
};
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_FORMAT, DXGI_FORMAT_AYUV, DXGI_FORMAT_NV12,
    DXGI_FORMAT_P010, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_Y410,
    DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_COLOR_SPACE_TYPE,
};
use ::windows::Win32::Graphics::Dxgi::IDXGIDevice;
use ::windows::Win32::Media::MediaFoundation::{
    IMFDXGIDeviceManager, MF_VERSION, MFCreateDXGIDeviceManager, MFSTARTUP_LITE, MFShutdown,
    MFStartup,
};
use ::windows::Win32::System::Com::{
    CO_MTA_USAGE_COOKIE, COINIT_MULTITHREADED, CoDecrementMTAUsage, CoIncrementMTAUsage,
    CoInitializeEx, CoUninitialize,
};
use ::windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
};
use ::windows::core::{IUnknown, Interface};

use crate::queue::BoundedQueue;
use crate::{
    ADAPTIVE_VIDEO_QUEUE_CAPACITY, BackendError, BackendEvent, EncodedVideoFrame, PushOutcome,
    Subsystem, VideoFormat, VideoPixelFormat, VideoTransferFunction, WindowsDecoderMode,
};

use super::color::input_color_space;
use super::decoder::{DecodedVideoFrame, Decoder, DecoderDevice};

// Async Media Foundation transforms do not provide a waitable output handle. Polling at one
// millisecond keeps decoder progress independent from Qt's render cadence without spinning an
// entire core. In particular, the final HaveOutput event must be drained even when transport has
// stopped delivering compressed access units temporarily.
const DECODER_POLL_INTERVAL: Duration = Duration::from_millis(1);
pub(super) const MAX_FRAME_SLOTS: usize = 8;
// A broken driver may never return from codec work. Never join it on Qt's render
// thread, but also never accumulate unbounded abandoned workers across retries.
static LIVE_DECODER_WORKERS: AtomicUsize = AtomicUsize::new(0);
struct DecoderWorkerLease;
impl DecoderWorkerLease {
    fn acquire() -> Result<Self, BackendError> {
        LIVE_DECODER_WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 4).then_some(count + 1)
            })
            .map(|_| Self)
            .map_err(|_| {
                BackendError::Startup(
                    "Previous decoder workers are still stopping; restart OpenNOW before retrying"
                        .into(),
                )
            })
    }
}
impl Drop for DecoderWorkerLease {
    fn drop(&mut self) {
        LIVE_DECODER_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn poll_decoder_startup(
    startup: &mut Option<Receiver<Result<(), String>>>,
) -> Result<bool, BackendError> {
    let Some(receiver) = startup.as_ref() else {
        return Ok(true);
    };
    match receiver.try_recv() {
        Err(TryRecvError::Empty) => Ok(false),
        result => {
            *startup = None;
            match result {
                Ok(Ok(())) => Ok(true),
                Ok(Err(error)) => Err(BackendError::Startup(format!(
                    "Media Foundation decoder: {error}"
                ))),
                _ => Err(BackendError::Startup(
                    "Decoder worker exited during startup".into(),
                )),
            }
        }
    }
}

/// Borrowed Qt D3D11 objects used by the embedded frame producer.
///
/// Both pointers must identify the D3D11 device and its immediate context from
/// Qt's adopted QRhi. The producer takes its own COM references and never
/// assumes ownership of either incoming reference.
#[derive(Debug, Clone, Copy)]
pub struct AdoptedD3d11Context {
    pub device: *mut c_void,
    pub immediate_context: *mut c_void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D3d11TextureFormat {
    Rgba8,
    Rgb10A2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D3d11ColorSpace {
    Sdr709,
    Pq2020,
    Hlg2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct D3d11RecordedFrame {
    pub texture: *mut c_void,
    pub texture_format: D3d11TextureFormat,
    pub color_space: D3d11ColorSpace,
    pub width: u32,
    pub height: u32,
    pub frame_slot: u32,
    pub generation: u64,
    pub presentation_time_ns: u64,
}

#[derive(Clone)]
pub struct D3d11FrameSubmitter {
    encoded: Arc<BoundedQueue<EncodedVideoFrame>>,
    events: Arc<BoundedQueue<BackendEvent>>,
}

impl D3d11FrameSubmitter {
    pub fn submit_video(&self, frame: EncodedVideoFrame) -> Result<PushOutcome, BackendError> {
        frame.validate()?;
        let key_frame = frame.key_frame;
        let outcome = self
            .encoded
            .push_or_clear_on_overflow(frame, key_frame)
            .map_err(|_| BackendError::WorkerDisconnected)?;
        if outcome == PushOutcome::DroppedOldest {
            let _ = self
                .events
                .push(BackendEvent::QueueOverflow(Subsystem::VideoDecode));
            // The submitting owner handles DroppedOldest synchronously and
            // invalidates its compressed chain before submitting again. A second
            // asynchronous KeyFrameRequired can arrive after the recovery IDR
            // and incorrectly discard that new, healthy chain.
        }
        Ok(outcome)
    }
}

struct EmbeddedMediaRuntime {
    mta_cookie: usize,
    media_foundation_started: bool,
}

impl EmbeddedMediaRuntime {
    fn initialize() -> Result<Self, String> {
        super::ensure_media_foundation_available()?;
        unsafe {
            let mta_cookie =
                CoIncrementMTAUsage().map_err(|error| format!("CoIncrementMTAUsage: {error}"))?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_LITE) {
                let _ = CoDecrementMTAUsage(mta_cookie);
                return Err(format!("MFStartup: {error}"));
            }
            Ok(Self {
                mta_cookie: mta_cookie.0 as usize,
                media_foundation_started: true,
            })
        }
    }
}

impl Drop for EmbeddedMediaRuntime {
    fn drop(&mut self) {
        unsafe {
            if self.media_foundation_started {
                let _ = MFShutdown();
            }
            let _ = CoDecrementMTAUsage(CO_MTA_USAGE_COOKIE(self.mta_cookie as *mut c_void));
        }
    }
}

struct FrameSlot {
    texture: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
}

impl FrameSlot {
    fn new(
        device: &ID3D11Device,
        video_device: &ID3D11VideoDevice,
        enumerator: &ID3D11VideoProcessorEnumerator,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<Self, String> {
        let description = frame_slot_description(width, height, format);
        let mut texture = None;
        unsafe {
            device
                .CreateTexture2D(&description, None, Some(&mut texture))
                .map_err(|error| format!("CreateTexture2D frame slot: {error}"))?;
        }
        let texture = texture.ok_or("D3D11 returned no frame-slot texture")?;
        let description = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut output_view = None;
        unsafe {
            video_device
                .CreateVideoProcessorOutputView(
                    &texture,
                    enumerator,
                    &description,
                    Some(&mut output_view),
                )
                .map_err(|error| format!("CreateVideoProcessorOutputView: {error}"))?;
        }
        Ok(Self {
            texture,
            output_view: output_view.ok_or("D3D11 returned no frame-slot output view")?,
        })
    }
}

struct ProcessorResources {
    input_width: u32,
    input_height: u32,
    input_format: DXGI_FORMAT,
    output_width: u32,
    output_height: u32,
    output_format: DXGI_FORMAT,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    input_views: HashMap<(usize, u32), ID3D11VideoProcessorInputView>,
    slots: [Option<FrameSlot>; MAX_FRAME_SLOTS],
}

struct AdoptedResources {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    video_context_1: ID3D11VideoContext1,
    manager: IMFDXGIDeviceManager,
    format: VideoFormat,
    generation: u64,
    processor: Option<ProcessorResources>,
    y410: Option<super::y410::Y410Converter>,
}

impl AdoptedResources {
    unsafe fn new(adopted: AdoptedD3d11Context, format: VideoFormat) -> Result<Self, String> {
        if adopted.device.is_null() || adopted.immediate_context.is_null() {
            return Err("Qt supplied a null D3D11 device or immediate context".to_owned());
        }
        let device = unsafe { clone_interface::<ID3D11Device>(adopted.device)? };
        let context = unsafe { clone_interface::<ID3D11DeviceContext>(adopted.immediate_context)? };
        let context_device = unsafe {
            context
                .GetDevice()
                .map_err(|error| format!("ID3D11DeviceContext::GetDevice: {error}"))?
        };
        if com_identity(&device)? != com_identity(&context_device)? {
            return Err(
                "Qt D3D11 immediate context does not belong to the adopted device".to_owned(),
            );
        }
        enable_multithread_protection(&context)?;
        let video_device = device
            .cast()
            .map_err(|error| format!("Qt D3D11 device has no video interface: {error}"))?;
        let video_context: ID3D11VideoContext = context
            .cast()
            .map_err(|error| format!("Qt immediate context has no video interface: {error}"))?;
        let video_context_1 = video_context.cast().map_err(|error| {
            format!("Qt D3D11 context cannot configure explicit color spaces: {error}")
        })?;
        let mut reset_token = 0;
        let mut manager = None;
        unsafe {
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
                .map_err(|error| format!("MFCreateDXGIDeviceManager: {error}"))?;
        }
        let manager = manager.ok_or("MFCreateDXGIDeviceManager returned no manager")?;
        unsafe {
            manager
                .ResetDevice(&device, reset_token)
                .map_err(|error| format!("IMFDXGIDeviceManager::ResetDevice: {error}"))?;
        }
        Ok(Self {
            device,
            context,
            video_device,
            video_context,
            video_context_1,
            manager,
            format,
            generation: 0,
            processor: None,
            y410: None,
        })
    }

    fn reconfigure(&mut self, format: VideoFormat) {
        if self.format != format {
            self.format = format;
            self.processor = None;
            self.y410 = None;
        }
    }

    fn reset_decoder_views(&mut self) {
        if let Some(processor) = self.processor.as_mut() {
            processor.input_views.clear();
        }
    }

    unsafe fn validate_adopted_context(&self, adopted: AdoptedD3d11Context) -> Result<(), String> {
        let device = unsafe { clone_interface::<ID3D11Device>(adopted.device)? };
        let context = unsafe { clone_interface::<ID3D11DeviceContext>(adopted.immediate_context)? };
        if com_identity(&device)? != com_identity(&self.device)?
            || com_identity(&context)? != com_identity(&self.video_context)?
        {
            return Err("D3D11 frame belongs to an older Qt graphics context".to_owned());
        }
        Ok(())
    }

    fn record(
        &mut self,
        frame_slot: u32,
        frame: &DecodedVideoFrame,
    ) -> Result<D3d11RecordedFrame, String> {
        let slot = usize::try_from(frame_slot)
            .ok()
            .filter(|slot| *slot < MAX_FRAME_SLOTS)
            .ok_or_else(|| format!("invalid D3D11 frame slot {frame_slot}"))?;
        let mut input_description = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            frame.texture.GetDesc(&mut input_description);
        }
        let pixel_format = pixel_format_from_dxgi(input_description.Format).ok_or_else(|| {
            format!(
                "Media Foundation returned unsupported D3D11 texture format {}",
                input_description.Format.0
            )
        })?;
        if pixel_format != frame.format.pixel_format {
            return Err(format!(
                "decoder texture format {pixel_format:?} does not match media format {:?}",
                frame.format.pixel_format
            ));
        }
        frame
            .aperture
            .validate_extent(input_description.Width, input_description.Height)?;
        let array_slice = decoder_array_slice(
            frame.subresource,
            input_description.MipLevels,
            input_description.ArraySize,
        )?;
        self.reconfigure(frame.format);
        if frame.format.pixel_format == VideoPixelFormat::Y410 {
            if self.y410.is_none() {
                self.y410 = Some(super::y410::Y410Converter::new(
                    &self.device,
                    &self.context,
                    frame.format,
                )?);
            }
            let texture = self
                .y410
                .as_mut()
                .ok_or("no Y410 converter")?
                .record(slot, frame)?;
            self.generation = self.generation.wrapping_add(1).max(1);
            return Ok(D3d11RecordedFrame {
                texture: texture.as_raw(),
                texture_format: D3d11TextureFormat::Rgb10A2,
                color_space: if frame.format.transfer_function == VideoTransferFunction::Pq {
                    D3d11ColorSpace::Pq2020
                } else {
                    D3d11ColorSpace::Sdr709
                },
                width: frame.format.width,
                height: frame.format.height,
                frame_slot,
                generation: self.generation,
                presentation_time_ns: u64::try_from(frame.timestamp_100ns.max(0))
                    .unwrap_or(0)
                    .saturating_mul(100),
            });
        }
        self.ensure_processor(
            input_description.Width,
            input_description.Height,
            input_description.Format,
            frame.aperture.width,
            frame.aperture.height,
        )?;
        let processor = self
            .processor
            .as_mut()
            .ok_or("embedded video processor is unavailable")?;
        let input_key = (frame.texture.as_raw() as usize, array_slice);
        let input_view = if let Some(view) = processor.input_views.get(&input_key) {
            view.clone()
        } else {
            let description = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: array_slice,
                    },
                },
            };
            let mut view = None;
            unsafe {
                self.video_device
                    .CreateVideoProcessorInputView(
                        &frame.texture,
                        &processor.enumerator,
                        &description,
                        Some(&mut view),
                    )
                    .map_err(|error| format!("CreateVideoProcessorInputView: {error}"))?;
            }
            let view = view.ok_or("D3D11 returned no video processor input view")?;
            processor.input_views.insert(input_key, view.clone());
            view
        };
        let source = RECT {
            left: frame.aperture.x as i32,
            top: frame.aperture.y as i32,
            right: (frame.aperture.x + frame.aperture.width) as i32,
            bottom: (frame.aperture.y + frame.aperture.height) as i32,
        };
        let destination = RECT {
            left: 0,
            top: 0,
            right: processor.output_width as i32,
            bottom: processor.output_height as i32,
        };
        if processor.slots[slot].is_none() {
            processor.slots[slot] = Some(FrameSlot::new(
                &self.device,
                &self.video_device,
                &processor.enumerator,
                processor.output_width,
                processor.output_height,
                processor.output_format,
            )?);
        }
        let active_slot = processor.slots[slot]
            .as_ref()
            .expect("initialized frame slot");
        let output_view = active_slot.output_view.clone();
        unsafe {
            self.video_context.VideoProcessorSetStreamFrameFormat(
                &processor.processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            self.video_context.VideoProcessorSetStreamSourceRect(
                &processor.processor,
                0,
                true,
                Some(&source),
            );
            self.video_context.VideoProcessorSetStreamDestRect(
                &processor.processor,
                0,
                true,
                Some(&destination),
            );
            self.video_context.VideoProcessorSetOutputTargetRect(
                &processor.processor,
                true,
                Some(&RECT {
                    left: 0,
                    top: 0,
                    right: processor.output_width as i32,
                    bottom: processor.output_height as i32,
                }),
            );
            let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(input_view)),
                ..Default::default()
            };
            let result = self.video_context.VideoProcessorBlt(
                &processor.processor,
                &output_view,
                0,
                std::slice::from_ref(&stream),
            );
            ManuallyDrop::drop(&mut stream.pInputSurface);
            result.map_err(|error| format!("VideoProcessorBlt: {error}"))?;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        Ok(D3d11RecordedFrame {
            texture: active_slot.texture.as_raw(),
            texture_format: d3d11_texture_format(processor.output_format),
            color_space: recorded_color_space(self.format.transfer_function),
            width: processor.output_width,
            height: processor.output_height,
            frame_slot,
            generation: self.generation,
            presentation_time_ns: u64::try_from(frame.timestamp_100ns.max(0))
                .unwrap_or(0)
                .saturating_mul(100),
        })
    }

    fn ensure_processor(
        &mut self,
        input_width: u32,
        input_height: u32,
        input_format: DXGI_FORMAT,
        output_width: u32,
        output_height: u32,
    ) -> Result<(), String> {
        self.format
            .validate_color()
            .map_err(|error| error.to_string())?;
        let input_color_space = input_color_space(self.format)?;
        let output_format = output_dxgi_format(self.format);
        if self.processor.as_ref().is_some_and(|processor| {
            processor.input_width == input_width
                && processor.input_height == input_height
                && processor.input_format == input_format
                && processor.output_width == output_width
                && processor.output_height == output_height
                && processor.output_format == output_format
        }) {
            return Ok(());
        }
        let description = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: self.format.frame_rate_numerator.get(),
                Denominator: self.format.frame_rate_denominator.get(),
            },
            InputWidth: input_width,
            InputHeight: input_height,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: self.format.frame_rate_numerator.get(),
                Denominator: self.format.frame_rate_denominator.get(),
            },
            OutputWidth: output_width,
            OutputHeight: output_height,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        let enumerator = unsafe {
            self.video_device
                .CreateVideoProcessorEnumerator(&description)
                .map_err(|error| format!("CreateVideoProcessorEnumerator: {error}"))?
        };
        validate_conversion(&enumerator, input_format, output_format, self.format)?;
        let processor = unsafe {
            self.video_device
                .CreateVideoProcessor(&enumerator, 0)
                .map_err(|error| format!("CreateVideoProcessor: {error}"))?
        };
        unsafe {
            self.video_context_1.VideoProcessorSetStreamColorSpace1(
                &processor,
                0,
                input_color_space,
            );
            self.video_context_1.VideoProcessorSetOutputColorSpace1(
                &processor,
                output_color_space(self.format.transfer_function),
            );
            if self.format.transfer_function != VideoTransferFunction::Sdr {
                self.video_context_1
                    .VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            }
        }
        // QRhi owns slot reuse. Allocate only the slots it actually visits;
        // reserving the ABI maximum would waste seven 4K textures on D3D11.
        let slots = std::array::from_fn(|_| None);
        self.processor = Some(ProcessorResources {
            input_width,
            input_height,
            input_format,
            output_width,
            output_height,
            output_format,
            enumerator,
            processor,
            input_views: HashMap::new(),
            slots,
        });
        Ok(())
    }
}

pub(super) unsafe fn d3d11_adapter_luid(
    device: *mut c_void,
) -> Result<crate::WindowsAdapterLuid, String> {
    if device.is_null() {
        return Err("Qt supplied a null D3D11 device".to_owned());
    }
    let device = unsafe { clone_interface::<ID3D11Device>(device)? };
    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|error| format!("Qt D3D11 DXGI device: {error}"))?;
    let adapter = unsafe {
        dxgi_device
            .GetAdapter()
            .map_err(|error| format!("Qt D3D11 adapter: {error}"))?
    };
    let description = unsafe {
        adapter
            .GetDesc()
            .map_err(|error| format!("Qt D3D11 adapter description: {error}"))?
    };
    let raw = u64::from(description.AdapterLuid.LowPart)
        | ((description.AdapterLuid.HighPart as u32 as u64) << 32);
    crate::WindowsAdapterLuid::new(raw)
        .ok_or_else(|| "Qt D3D11 device reported a zero adapter LUID".to_owned())
}

#[cfg(test)]
pub(super) unsafe fn probe_hdr_conversion(
    adopted: AdoptedD3d11Context,
    format: VideoFormat,
) -> Result<(), String> {
    if format.transfer_function != VideoTransferFunction::Pq
        || format.pixel_format != VideoPixelFormat::P010
    {
        return Err("HDR10 capability requires actual P010/PQ decoder output".to_owned());
    }
    unsafe { probe_conversion(adopted, format) }
}

#[cfg(test)]
unsafe fn probe_conversion(
    adopted: AdoptedD3d11Context,
    format: VideoFormat,
) -> Result<(), String> {
    let input_format = match format.pixel_format {
        VideoPixelFormat::Nv12 => DXGI_FORMAT_NV12,
        VideoPixelFormat::P010 => DXGI_FORMAT_P010,
        VideoPixelFormat::Ayuv => DXGI_FORMAT_AYUV,
        VideoPixelFormat::Y410 => DXGI_FORMAT_Y410,
    };
    let mut resources = unsafe { AdoptedResources::new(adopted, format)? };
    resources.ensure_processor(
        format.width,
        format.height,
        input_format,
        format.width,
        format.height,
    )
}

pub(super) unsafe fn probe_decoded_conversion(
    adopted: AdoptedD3d11Context,
    frame: &DecodedVideoFrame,
) -> Result<(), String> {
    let mut resources = unsafe { AdoptedResources::new(adopted, frame.format)? };
    resources.record(0, frame)?;
    Ok(())
}

fn enable_multithread_protection(context: &ID3D11DeviceContext) -> Result<(), String> {
    // Media Foundation may use the adopted device and immediate context from an asynchronous
    // decoder work-queue thread while Qt records and presents on QSGRenderThread. D3D11 immediate
    // contexts are not thread-safe unless ID3D10Multithread protection is enabled. Without it,
    // NVIDIA's user-mode driver can deadlock one thread in decoder-buffer acquisition and the
    // other in DXGI Present.
    let multithread: ID3D10Multithread = context
        .cast()
        .map_err(|error| format!("Qt D3D11 context has no multithread interface: {error}"))?;
    unsafe {
        let _ = multithread.SetMultithreadProtected(true);
        if !multithread.GetMultithreadProtected().as_bool() {
            return Err("Qt D3D11 context rejected multithread protection".to_owned());
        }
    }
    Ok(())
}

impl DecoderDevice for AdoptedResources {
    fn device_manager(&self) -> &IMFDXGIDeviceManager {
        &self.manager
    }

    fn adapter_luid(&self) -> Result<LUID, String> {
        unsafe {
            let dxgi_device: IDXGIDevice = self
                .device
                .cast()
                .map_err(|error| format!("Qt D3D11 DXGI device: {error}"))?;
            let adapter = dxgi_device
                .GetAdapter()
                .map_err(|error| format!("Qt D3D11 adapter: {error}"))?;
            let description = adapter
                .GetDesc()
                .map_err(|error| format!("Qt D3D11 adapter description: {error}"))?;
            let name_end = description
                .Description
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(description.Description.len());
            video_log!(
                "Qt decoder adapter name={} vendor={:04x} device={:04x} dedicated_video_mb={}",
                String::from_utf16_lossy(&description.Description[..name_end]),
                description.VendorId,
                description.DeviceId,
                description.DedicatedVideoMemory / (1024 * 1024)
            );
            Ok(description.AdapterLuid)
        }
    }

    fn video_format(&self) -> VideoFormat {
        self.format
    }
}

#[derive(Clone)]
struct DecoderDeviceSnapshot {
    manager: IMFDXGIDeviceManager,
    adapter_luid: LUID,
    format: VideoFormat,
}

// IMFDXGIDeviceManager is the documented synchronization boundary for sharing one D3D11 device
// with Media Foundation. The adopted immediate context has ID3D10Multithread protection enabled
// before this snapshot is created, and the snapshot remains alive until the decoder thread joins.
unsafe impl Send for DecoderDeviceSnapshot {}

impl DecoderDeviceSnapshot {
    fn new(resources: &AdoptedResources) -> Result<Self, String> {
        Ok(Self {
            manager: resources.manager.clone(),
            adapter_luid: resources.adapter_luid()?,
            format: resources.format,
        })
    }
}

impl DecoderDevice for DecoderDeviceSnapshot {
    fn device_manager(&self) -> &IMFDXGIDeviceManager {
        &self.manager
    }

    fn adapter_luid(&self) -> Result<LUID, String> {
        Ok(self.adapter_luid)
    }

    fn video_format(&self) -> VideoFormat {
        self.format
    }
}

struct ReadyDecodedFrame {
    frame: DecodedVideoFrame,
    decoder_generation: u64,
}

// The decoder surface AND its Media Foundation sample lease move together.
// The sample prevents decoder-pool reuse while Qt records the video blit on
// the same protected immediate context. Neither is mutated through this queue.
unsafe impl Send for ReadyDecodedFrame {}

struct D3d11Pipeline {
    owner_thread: ThreadId,
    resources: AdoptedResources,
    decoded: Arc<Mutex<VecDeque<ReadyDecodedFrame>>>,
    encoded: Arc<BoundedQueue<EncodedVideoFrame>>,
    events: Arc<BoundedQueue<BackendEvent>>,
    presented_decoder_generation: u64,
    stopping: Arc<AtomicBool>,
    decoder_worker: Option<JoinHandle<()>>,
    startup: Option<Receiver<Result<(), String>>>,
    _runtime: Arc<EmbeddedMediaRuntime>,
}

unsafe impl Send for D3d11Pipeline {}

pub struct D3d11Frame {
    frame: DecodedVideoFrame,
    state: Arc<Mutex<D3d11Pipeline>>,
    sequence: u64,
}

unsafe impl Send for D3d11Frame {}
unsafe impl Sync for D3d11Frame {}

impl D3d11Frame {
    pub fn width(&self) -> u32 {
        self.frame.aperture.width
    }

    pub fn height(&self) -> u32 {
        self.frame.aperture.height
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn presentation_time_ns(&self) -> u64 {
        u64::try_from(self.frame.timestamp_100ns.max(0))
            .unwrap_or(0)
            .saturating_mul(100)
    }

    /// Converts this decoded surface into Qt's matching 8-bit or 10-bit RGB frame-slot target.
    ///
    /// # Safety
    ///
    /// `target.texture` must identify a live `ID3D11Texture2D` from the adopted
    /// Qt device. Recording must run on the render thread that created the
    /// producer.
    pub unsafe fn record(
        &self,
        adopted: AdoptedD3d11Context,
        frame_slot: u32,
    ) -> Result<D3d11RecordedFrame, BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.ensure_render_thread()?;
        unsafe { state.resources.validate_adopted_context(adopted) }.map_err(|message| {
            BackendError::DeviceLost {
                subsystem: Subsystem::VideoPresentation,
                message,
            }
        })?;
        state
            .resources
            .record(frame_slot, &self.frame)
            .map_err(|message| BackendError::DeviceLost {
                subsystem: Subsystem::VideoPresentation,
                message,
            })
    }
}

#[derive(Clone)]
pub struct D3d11FrameProducer {
    state: Arc<Mutex<D3d11Pipeline>>,
    sequence: Arc<AtomicU64>,
}

impl D3d11FrameProducer {
    /// Adopts Qt's D3D11 device and immediate context without creating a
    /// window, swap chain, SDL video subsystem, or presentation path.
    ///
    /// # Safety
    ///
    /// Both COM pointers must be valid for this call and identify live
    /// `ID3D11Device` and `ID3D11DeviceContext` interfaces. The context must be
    /// Qt's immediate context, and all producer methods except submission and
    /// event polling must remain on the creating render thread.
    pub unsafe fn new(
        adopted: AdoptedD3d11Context,
        format: VideoFormat,
        decoder_mode: WindowsDecoderMode,
        frame_ready: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(Self, D3d11FrameSubmitter), BackendError> {
        format.validate()?;
        let worker_lease = DecoderWorkerLease::acquire()?;
        let runtime = Arc::new(EmbeddedMediaRuntime::initialize().map_err(BackendError::Startup)?);
        let resources =
            unsafe { AdoptedResources::new(adopted, format) }.map_err(BackendError::Startup)?;
        let decoder_device =
            DecoderDeviceSnapshot::new(&resources).map_err(BackendError::Startup)?;
        let encoded = Arc::new(BoundedQueue::new(ADAPTIVE_VIDEO_QUEUE_CAPACITY));
        let events = Arc::new(BoundedQueue::new(64));
        let decoded = Arc::new(Mutex::new(VecDeque::with_capacity(
            ADAPTIVE_VIDEO_QUEUE_CAPACITY,
        )));
        let decoder_generation = Arc::new(AtomicU64::new(1));
        let stopping = Arc::new(AtomicBool::new(false));
        let submitter = D3d11FrameSubmitter {
            encoded: Arc::clone(&encoded),
            events: Arc::clone(&events),
        };
        let (startup_sender, startup_receiver) = sync_channel(1);
        let worker_encoded = Arc::clone(&encoded);
        let worker_decoded = Arc::clone(&decoded);
        let worker_events = Arc::clone(&events);
        let worker_generation = Arc::clone(&decoder_generation);
        let worker_stopping = Arc::clone(&stopping);
        let worker_runtime = Arc::clone(&runtime);
        let decoder_worker = thread::Builder::new()
            .name("opennow-mf-video-decode".to_owned())
            .spawn(move || {
                let _worker_lease = worker_lease;
                let _runtime = worker_runtime;
                let notify = Arc::clone(&frame_ready);
                let stopping = Arc::clone(&worker_stopping);
                run_decoder_worker(
                    decoder_device,
                    format,
                    decoder_mode,
                    worker_encoded,
                    worker_decoded,
                    worker_events,
                    worker_generation,
                    worker_stopping,
                    frame_ready,
                    startup_sender,
                );
                // Startup failures must wake the presenter to consume the error.
                if !stopping.load(Ordering::Acquire) {
                    notify();
                }
            })
            .map_err(|error| {
                BackendError::Startup(format!("start embedded decoder worker: {error}"))
            })?;
        let state = Arc::new(Mutex::new(D3d11Pipeline {
            owner_thread: thread::current().id(),
            resources,
            decoded,
            encoded,
            events,
            presented_decoder_generation: 0,
            stopping,
            decoder_worker: Some(decoder_worker),
            startup: Some(startup_receiver),
            _runtime: runtime,
        }));
        Ok((
            Self {
                state,
                sequence: Arc::new(AtomicU64::new(0)),
            },
            submitter,
        ))
    }

    pub fn acquire_latest(&self) -> Result<Option<D3d11Frame>, BackendError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let frame = state.acquire_latest()?;
        Ok(frame.map(|frame| D3d11Frame {
            frame,
            state: Arc::clone(&self.state),
            sequence: self.sequence.fetch_add(1, Ordering::AcqRel) + 1,
        }))
    }

    pub fn try_event(&self) -> Option<BackendEvent> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .events
            .try_pop()
    }
}

impl D3d11Pipeline {
    fn acquire_latest(&mut self) -> Result<Option<DecodedVideoFrame>, BackendError> {
        self.ensure_render_thread()?;
        if !poll_decoder_startup(&mut self.startup)? {
            return Ok(None);
        }
        let mut decoded = self
            .decoded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(ready) = decoded.pop_back() else {
            return Ok(None);
        };
        if !decoded.is_empty() {
            decoded.clear();
            let _ = self
                .events
                .push(BackendEvent::QueueOverflow(Subsystem::VideoPresentation));
        }
        drop(decoded);
        if ready.decoder_generation != self.presented_decoder_generation {
            self.resources.reset_decoder_views();
            self.presented_decoder_generation = ready.decoder_generation;
        }
        Ok(Some(ready.frame))
    }

    fn ensure_render_thread(&self) -> Result<(), BackendError> {
        if thread::current().id() == self.owner_thread {
            Ok(())
        } else {
            Err(BackendError::InvalidConfig(
                "embedded D3D11 frame production must stay on Qt's render thread".to_owned(),
            ))
        }
    }
}

impl Drop for D3d11Pipeline {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.encoded.close();
        // The worker retains its device, queues, and MF runtime until it exits.
        // Dropping a JoinHandle detaches, without waiting for driver shutdown.
        drop(self.decoder_worker.take());
        self.decoded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

struct DecoderThreadApartment;

impl DecoderThreadApartment {
    fn initialize() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| format!("CoInitializeEx: {error}"))?;
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        }
        Ok(Self)
    }
}

impl Drop for DecoderThreadApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_decoder_worker(
    device: DecoderDeviceSnapshot,
    format: VideoFormat,
    mode: WindowsDecoderMode,
    encoded: Arc<BoundedQueue<EncodedVideoFrame>>,
    decoded: Arc<Mutex<VecDeque<ReadyDecodedFrame>>>,
    events: Arc<BoundedQueue<BackendEvent>>,
    decoder_generation: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    frame_ready: Arc<dyn Fn() + Send + Sync>,
    startup_sender: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let _apartment = match DecoderThreadApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            video_log!("Embedded D3D11 COM initialization failed: {error}");
            let _ = startup_sender.send(Err(error));
            return;
        }
    };
    let mut decoder = match Decoder::new(&device, format, mode) {
        Ok(decoder) => decoder,
        Err(error) => {
            video_log!("Embedded D3D11 decoder initialization failed: {error}");
            let _ = startup_sender.send(Err(error));
            return;
        }
    };
    if startup_sender.send(Ok(())).is_err() {
        decoder.stop();
        return;
    }

    opennow_streamer_protocol::log::diagnostic(
        "INFO",
        "decode",
        &format!(
            "Embedded D3D11 decoder worker started codec={} pollMs={}",
            format.codec.label(),
            DECODER_POLL_INTERVAL.as_millis()
        ),
    );
    let mut submitted_any = false;
    // One worker-owned access unit may be waiting for a replacement MFT's
    // NeedInput event. Keep its already-queued descendants in order.
    let mut pending_frame = None;
    let mut submitted_frames = 0_u64;
    let mut produced_frames = 0_u64;
    let mut last_progress_log = Instant::now();
    let mut output = VecDeque::with_capacity(2);
    while !stopping.load(Ordering::Acquire) {
        let mut made_progress = false;
        output.clear();
        match decoder.poll_output(&mut output, &events) {
            Ok(produced) => {
                made_progress |= produced > 0;
                produced_frames = produced_frames.saturating_add(produced as u64);
                if produced > 0 {
                    let generation = decoder_generation.load(Ordering::Acquire);
                    let mut ready = decoded
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    while let Some(frame) = output.pop_front() {
                        if ready.len() == ADAPTIVE_VIDEO_QUEUE_CAPACITY {
                            ready.pop_front();
                            let _ = events
                                .push(BackendEvent::QueueOverflow(Subsystem::VideoPresentation));
                        }
                        ready.push_back(ReadyDecodedFrame {
                            frame,
                            decoder_generation: generation,
                        });
                    }
                    drop(ready);
                    frame_ready();
                }
            }
            Err(message) => {
                decoder.stop();
                decoded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clear();
                encoded.clear();
                decoder_generation.fetch_add(1, Ordering::AcqRel);
                opennow_streamer_protocol::log::diagnostic(
                    "WARN",
                    "decode",
                    &format!("Embedded D3D11 decoder failed: {message}"),
                );
                let _ = events.push(BackendEvent::DeviceLost {
                    subsystem: Subsystem::VideoDecode,
                    message,
                });
                let _ = events.push(BackendEvent::KeyFrameRequired);
                frame_ready();
                pending_frame = wait_for_recovery_keyframe(
                    &device,
                    format,
                    mode,
                    &encoded,
                    &decoded,
                    &events,
                    &decoder_generation,
                    &stopping,
                    frame_ready.as_ref(),
                    &mut decoder,
                    &mut submitted_any,
                );
                continue;
            }
        }

        while decoder.wants_input() {
            let Some(frame) = next_encoded_frame(&mut pending_frame, &encoded) else {
                break;
            };
            if frame.reset_decoder && submitted_any {
                decoder.stop();
                decoded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clear();
                decoder_generation.fetch_add(1, Ordering::AcqRel);
                match Decoder::new(&device, format, mode) {
                    Ok(replacement) => {
                        decoder = replacement;
                        submitted_any = false;
                        // The queued frames follow this keyframe. Clearing them
                        // silently breaks the next P-frame's reference chain.
                        pending_frame = Some(frame);
                        made_progress = true;
                        break;
                    }
                    Err(message) => {
                        opennow_streamer_protocol::log::diagnostic(
                            "WARN",
                            "decode",
                            &format!("Embedded D3D11 decoder reset failed: {message}"),
                        );
                        let _ = events.push(BackendEvent::DeviceLost {
                            subsystem: Subsystem::VideoDecode,
                            message,
                        });
                        let _ = events.push(BackendEvent::KeyFrameRequired);
                        frame_ready();
                        pending_frame = wait_for_recovery_keyframe(
                            &device,
                            format,
                            mode,
                            &encoded,
                            &decoded,
                            &events,
                            &decoder_generation,
                            &stopping,
                            frame_ready.as_ref(),
                            &mut decoder,
                            &mut submitted_any,
                        );
                        made_progress = true;
                        break;
                    }
                }
            }
            if let Err(message) = decoder.submit(frame) {
                decoder.stop();
                decoded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clear();
                encoded.clear();
                decoder_generation.fetch_add(1, Ordering::AcqRel);
                opennow_streamer_protocol::log::diagnostic(
                    "WARN",
                    "decode",
                    &format!("Embedded D3D11 decoder input failed: {message}"),
                );
                let _ = events.push(BackendEvent::DeviceLost {
                    subsystem: Subsystem::VideoDecode,
                    message,
                });
                let _ = events.push(BackendEvent::KeyFrameRequired);
                frame_ready();
                pending_frame = wait_for_recovery_keyframe(
                    &device,
                    format,
                    mode,
                    &encoded,
                    &decoded,
                    &events,
                    &decoder_generation,
                    &stopping,
                    frame_ready.as_ref(),
                    &mut decoder,
                    &mut submitted_any,
                );
                made_progress = true;
                break;
            }
            submitted_any = true;
            submitted_frames = submitted_frames.saturating_add(1);
            made_progress = true;
        }

        if last_progress_log.elapsed() >= Duration::from_secs(10) {
            let decoded_ready = decoded
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .len();
            opennow_streamer_protocol::log::diagnostic(
                "INFO",
                "decode",
                &format!(
                    "Embedded D3D11 decoder progress codec={} submitted={submitted_frames} produced={produced_frames} encodedQueued={} decodedReady={decoded_ready} generation={}",
                    format.codec.label(),
                    encoded.len(),
                    decoder_generation.load(Ordering::Acquire),
                ),
            );
            last_progress_log = Instant::now();
        }

        if !made_progress {
            let _ = encoded.wait_for_decoder(DECODER_POLL_INTERVAL, decoder.wants_input());
        } else {
            thread::yield_now();
        }
    }
    decoder.stop();
    video_log!("Embedded D3D11 decoder worker stopped");
}

fn next_encoded_frame(
    pending: &mut Option<EncodedVideoFrame>,
    encoded: &BoundedQueue<EncodedVideoFrame>,
) -> Option<EncodedVideoFrame> {
    pending.take().or_else(|| encoded.try_pop())
}

#[allow(clippy::too_many_arguments)]
fn wait_for_recovery_keyframe(
    device: &DecoderDeviceSnapshot,
    format: VideoFormat,
    mode: WindowsDecoderMode,
    encoded: &BoundedQueue<EncodedVideoFrame>,
    decoded: &Mutex<VecDeque<ReadyDecodedFrame>>,
    events: &BoundedQueue<BackendEvent>,
    decoder_generation: &AtomicU64,
    stopping: &AtomicBool,
    frame_ready: &dyn Fn(),
    decoder: &mut Decoder,
    submitted_any: &mut bool,
) -> Option<EncodedVideoFrame> {
    while !stopping.load(Ordering::Acquire) {
        let Some(frame) = encoded.pop_timeout(DECODER_POLL_INTERVAL) else {
            continue;
        };
        if !frame.key_frame {
            continue;
        }
        match Decoder::new(device, format, mode) {
            Ok(replacement) => {
                *decoder = replacement;
                *submitted_any = false;
                decoded
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clear();
                decoder_generation.fetch_add(1, Ordering::AcqRel);
                let _ = events.push(BackendEvent::DeviceRecovered(Subsystem::VideoDecode));
                frame_ready();
                return Some(frame);
            }
            Err(message) => {
                opennow_streamer_protocol::log::diagnostic(
                    "WARN",
                    "decode",
                    &format!("Embedded D3D11 decoder recovery failed: {message}"),
                );
                let _ = events.push(BackendEvent::DeviceLost {
                    subsystem: Subsystem::VideoDecode,
                    message,
                });
                let _ = events.push(BackendEvent::KeyFrameRequired);
                frame_ready();
            }
        }
    }
    None
}

unsafe fn clone_interface<T: Interface>(pointer: *mut c_void) -> Result<T, String> {
    if pointer.is_null() {
        return Err("cannot adopt a null COM interface".to_owned());
    }
    let borrowed = ManuallyDrop::new(unsafe { T::from_raw(pointer) });
    Ok((*borrowed).clone())
}

fn com_identity<T: Interface>(interface: &T) -> Result<usize, String> {
    interface
        .cast::<IUnknown>()
        .map(|unknown| unknown.as_raw() as usize)
        .map_err(|error| error.to_string())
}

fn decoder_array_slice(subresource: u32, mip_levels: u32, array_size: u32) -> Result<u32, String> {
    let mip_levels = mip_levels.max(1);
    let array_size = array_size.max(1);
    let total = mip_levels
        .checked_mul(array_size)
        .ok_or("decoder texture subresource count overflowed")?;
    if subresource >= total {
        return Err(format!(
            "decoder subresource {subresource} exceeds texture layout ({mip_levels} mips, {array_size} slices)"
        ));
    }
    Ok(subresource / mip_levels)
}

fn frame_slot_description(width: u32, height: u32, format: DXGI_FORMAT) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    }
}

fn validate_conversion(
    enumerator: &ID3D11VideoProcessorEnumerator,
    input_format: DXGI_FORMAT,
    output_format: DXGI_FORMAT,
    format: VideoFormat,
) -> Result<(), String> {
    let input_color_space = input_color_space(format)?;
    unsafe {
        let support = enumerator
            .CheckVideoProcessorFormat(input_format)
            .map_err(|error| format!("query decoder video-processor input: {error}"))?;
        if support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 == 0 {
            return Err(format!(
                "D3D11 video processor does not support input format {}",
                input_format.0
            ));
        }
        let support = enumerator
            .CheckVideoProcessorFormat(output_format)
            .map_err(|error| format!("query RGB video-processor output: {error}"))?;
        if support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0 {
            return Err(format!(
                "D3D11 video processor does not support output format {}",
                output_format.0
            ));
        }
        let enumerator_1 = enumerator
            .cast::<ID3D11VideoProcessorEnumerator1>()
            .map_err(|error| format!("D3D11 cannot validate explicit color conversion: {error}"))?;
        let supported = enumerator_1
            .CheckVideoProcessorFormatConversion(
                input_format,
                input_color_space,
                output_format,
                output_color_space(format.transfer_function),
            )
            .map_err(|error| format!("query D3D11 embedded conversion: {error}"))?;
        if !supported.as_bool() {
            return Err(format!(
                "D3D11 driver rejects embedded video conversion {} (color space {}) -> {} (color space {})",
                input_format.0,
                input_color_space.0,
                output_format.0,
                output_color_space(format.transfer_function).0,
            ));
        }
    }
    Ok(())
}

fn output_dxgi_format(format: VideoFormat) -> DXGI_FORMAT {
    if format.transfer_function != VideoTransferFunction::Sdr || format.pixel_format.bit_depth() > 8
    {
        DXGI_FORMAT_R10G10B10A2_UNORM
    } else {
        DXGI_FORMAT_R8G8B8A8_UNORM
    }
}

fn output_color_space(transfer: VideoTransferFunction) -> DXGI_COLOR_SPACE_TYPE {
    match transfer {
        VideoTransferFunction::Sdr => DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        VideoTransferFunction::Pq | VideoTransferFunction::Hlg => {
            DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
        }
    }
}

fn recorded_color_space(transfer: VideoTransferFunction) -> D3d11ColorSpace {
    match transfer {
        VideoTransferFunction::Sdr => D3d11ColorSpace::Sdr709,
        VideoTransferFunction::Pq | VideoTransferFunction::Hlg => D3d11ColorSpace::Pq2020,
    }
}

fn d3d11_texture_format(format: DXGI_FORMAT) -> D3d11TextureFormat {
    if format == DXGI_FORMAT_R10G10B10A2_UNORM {
        D3d11TextureFormat::Rgb10A2
    } else {
        D3d11TextureFormat::Rgba8
    }
}

fn pixel_format_from_dxgi(format: DXGI_FORMAT) -> Option<VideoPixelFormat> {
    match format {
        DXGI_FORMAT_NV12 => Some(VideoPixelFormat::Nv12),
        DXGI_FORMAT_P010 => Some(VideoPixelFormat::P010),
        DXGI_FORMAT_AYUV => Some(VideoPixelFormat::Ayuv),
        DXGI_FORMAT_Y410 => Some(VideoPixelFormat::Y410),
        _ => None,
    }
}

#[cfg(test)]
fn chroma_format(format: VideoPixelFormat) -> crate::VideoChromaFormat {
    use crate::VideoChromaFormat;
    match format {
        VideoPixelFormat::Nv12 | VideoPixelFormat::P010 => VideoChromaFormat::Cs420,
        VideoPixelFormat::Ayuv | VideoPixelFormat::Y410 => VideoChromaFormat::Cs444,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VideoChromaFormat, VideoChromaSiting, VideoColorMatrix};
    use ::windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_USAGE_STAGING,
    };
    use ::windows::Win32::Graphics::Dxgi::Common::{
        DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709, DXGI_COLOR_SPACE_YCBCR_FULL_GHLG_TOPLEFT_P2020,
        DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
        DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_LEFT_P2020,
        DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_TOPLEFT_P2020,
        DXGI_COLOR_SPACE_YCBCR_STUDIO_GHLG_TOPLEFT_P2020,
    };

    #[test]
    fn padded_decoder_frames_crop_and_reuse_processor_views_and_slots() {
        let _runtime = EmbeddedMediaRuntime::initialize().expect("Media Foundation");
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("D3D11 test device");
        }
        let device = device.unwrap();
        let context = context.unwrap();
        if let Err(error) = device.cast::<ID3D11VideoDevice>() {
            assert_eq!(
                error.code().0 as u32,
                0x80004002,
                "unexpected video probe error: {error}"
            );
            eprintln!("SKIP GPU conversion: D3D11 video interface unavailable ({error})");
            return;
        }
        let format = VideoFormat {
            codec: crate::VideoCodec::H264,
            width: 1920,
            height: 1080,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 10_000_000,
            pixel_format: VideoPixelFormat::Nv12,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: true,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: crate::VideoTransferFunction::Sdr,
            color_primaries: crate::VideoColorPrimaries::Bt709,
            color_matrix: crate::VideoColorMatrix::Bt709,
        };
        for (allocation_width, aperture) in [
            (
                1920,
                crate::aperture::VideoAperture::new(1920, 1080, None).unwrap(),
            ),
            (
                1936,
                crate::aperture::VideoAperture::new(1936, 1088, Some((8, 4, 1920, 1080))).unwrap(),
            ),
        ] {
            let mut resources = unsafe {
                AdoptedResources::new(
                    AdoptedD3d11Context {
                        device: device.as_raw(),
                        immediate_context: context.as_raw(),
                    },
                    format,
                )
            }
            .unwrap();
            let mut description = frame_slot_description(allocation_width, 1088, DXGI_FORMAT_NV12);
            description.BindFlags =
                ::windows::Win32::Graphics::Direct3D11::D3D11_BIND_DECODER.0 as u32;
            description.ArraySize = 2;
            let mut texture = None;
            let sample = unsafe {
                device
                    .CreateTexture2D(&description, None, Some(&mut texture))
                    .unwrap();
                let buffer =
                    MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &texture.unwrap(), 1, false)
                        .unwrap();
                let sample = MFCreateSample().unwrap();
                sample.AddBuffer(&buffer).unwrap();
                sample.SetSampleTime(1_234_567).unwrap();
                sample.SetSampleDuration(166_667).unwrap();
                sample
            };
            let frame =
                DecodedVideoFrame::from_sample(sample, format, aperture, format.pixel_format)
                    .unwrap();
            let first = resources.record(0, &frame).unwrap();
            let processor = resources.processor.as_ref().unwrap().processor.clone();
            for _ in 0..3 {
                resources.reconfigure(format);
                let recorded = resources.record(0, &frame).unwrap();
                assert_eq!(resources.format, format);
                assert_eq!((recorded.width, recorded.height), (1920, 1080));
                assert_eq!(recorded.texture, first.texture);
                assert_eq!(recorded.presentation_time_ns, 123_456_700);
                assert_eq!(frame.subresource, 1);
                assert_eq!(frame.duration_100ns, 166_667);
                let active = resources.processor.as_ref().unwrap();
                assert_eq!(active.processor.as_raw(), processor.as_raw());
                assert_eq!(
                    (active.input_width, active.input_height),
                    (allocation_width, 1088)
                );
                assert_eq!(active.input_views.len(), 1);
                let mut enabled = Default::default();
                let mut source = RECT::default();
                unsafe {
                    resources.video_context.VideoProcessorGetStreamSourceRect(
                        &processor,
                        0,
                        &mut enabled,
                        &mut source,
                    );
                }
                assert!(enabled.as_bool());
                assert_eq!(
                    (source.left, source.top, source.right, source.bottom),
                    (
                        aperture.x as i32,
                        aperture.y as i32,
                        (aperture.x + 1920) as i32,
                        (aperture.y + 1080) as i32
                    )
                );
            }
            resources.reset_decoder_views();
            assert!(resources.processor.as_ref().unwrap().input_views.is_empty());
            assert_eq!(resources.record(0, &frame).unwrap().texture, first.texture);
        }
    }
    #[test]
    fn decoder_worker_retries_are_bounded_until_old_workers_exit() {
        let mut leases: Vec<_> = (0..4)
            .map(|_| DecoderWorkerLease::acquire().unwrap())
            .collect();
        assert!(DecoderWorkerLease::acquire().is_err());
        leases.pop();
        let replacement = DecoderWorkerLease::acquire().unwrap();
        assert!(DecoderWorkerLease::acquire().is_err());
        drop(replacement);
        drop(leases);
        assert!(DecoderWorkerLease::acquire().is_ok());
    }
    #[test]
    fn decoder_startup_is_nonblocking_and_preserves_failures() {
        let (sender, receiver) = sync_channel(1);
        let mut startup = Some(receiver);
        assert!(!poll_decoder_startup(&mut startup).unwrap());
        sender.send(Ok(())).unwrap();
        assert!(poll_decoder_startup(&mut startup).unwrap());
        assert!(poll_decoder_startup(&mut startup).unwrap());

        let (sender, receiver) = sync_channel(1);
        let mut startup = Some(receiver);
        sender.send(Err("driver refused codec".into())).unwrap();
        assert!(
            poll_decoder_startup(&mut startup)
                .unwrap_err()
                .to_string()
                .contains("driver refused codec")
        );
        let (sender, receiver) = sync_channel(1);
        drop(sender);
        assert!(poll_decoder_startup(&mut Some(receiver)).is_err());
    }
    use ::windows::Win32::Foundation::HMODULE;
    use ::windows::Win32::Graphics::Direct3D::{
        D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
    };
    use ::windows::Win32::Graphics::Direct3D11::{
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
    };
    use ::windows::Win32::Graphics::Dxgi::IDXGIAdapter;
    use ::windows::Win32::Media::MediaFoundation::{
        IMFSample, MFCreateDXGISurfaceBuffer, MFCreateSample,
    };

    // Test-only observation of the concrete MF sample's COM ownership. Keeping
    // a texture alive alone must not satisfy the sample-lease regression check.
    fn sample_ref_count(sample: &IMFSample) -> u32 {
        let identity = sample.cast::<::windows::core::IUnknown>().unwrap();
        unsafe {
            (identity.vtable().AddRef)(identity.as_raw());
            (identity.vtable().Release)(identity.as_raw()) - 1
        }
    }

    fn encoded_frame(sequence: i64, key_frame: bool) -> EncodedVideoFrame {
        EncodedVideoFrame {
            codec: crate::VideoCodec::H264,
            data: vec![0, 0, 0, 1, if key_frame { 0x65 } else { 0x41 }],
            timestamp_100ns: sequence * 166_667,
            duration_100ns: 166_667,
            key_frame,
            reset_decoder: key_frame,
        }
    }

    #[test]
    fn decoder_reset_keeps_keyframe_and_queued_descendants_in_order() {
        let queue = BoundedQueue::new(2);
        let mut pending = Some(encoded_frame(10, true));
        queue.push(encoded_frame(11, false)).unwrap();
        queue.push(encoded_frame(12, false)).unwrap();
        for sequence in 10..=12 {
            let frame = next_encoded_frame(&mut pending, &queue).unwrap();
            assert_eq!(frame.timestamp_100ns, sequence * 166_667);
            assert_eq!(frame.key_frame, sequence == 10);
        }
        assert!(next_encoded_frame(&mut pending, &queue).is_none());
    }

    #[test]
    fn adopted_context_enables_d3d11_multithread_protection() {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("D3D11 test device");
        }
        let context = context.expect("D3D11 immediate context");
        let multithread: ID3D10Multithread = context.cast().expect("multithread interface");
        unsafe {
            let _ = multithread.SetMultithreadProtected(false);
        }
        assert!(!unsafe { multithread.GetMultithreadProtected().as_bool() });

        enable_multithread_protection(&context).expect("enable multithread protection");

        assert!(unsafe { multithread.GetMultithreadProtected().as_bool() });
    }

    #[test]
    fn full_decode_queue_retains_an_incoming_recovery_keyframe() {
        let encoded = Arc::new(BoundedQueue::new(2));
        let events = Arc::new(BoundedQueue::new(8));
        let submitter = D3d11FrameSubmitter {
            encoded: Arc::clone(&encoded),
            events: Arc::clone(&events),
        };
        assert_eq!(
            submitter.submit_video(encoded_frame(0, false)),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            submitter.submit_video(encoded_frame(1, false)),
            Ok(PushOutcome::Queued)
        );

        assert_eq!(
            submitter.submit_video(encoded_frame(2, true)),
            Ok(PushOutcome::DroppedOldest)
        );
        assert!(encoded.try_pop().is_some_and(|frame| frame.key_frame));
        assert!(encoded.try_pop().is_none());
        assert_eq!(
            events.try_pop(),
            Some(BackendEvent::QueueOverflow(Subsystem::VideoDecode))
        );
        assert!(events.try_pop().is_none());
    }

    #[test]
    #[ignore = "requires a hardware HEVC Main10 MFT and P010/PQ-to-RGB10A2/PQ conversion"]
    fn hevc_hdr_hardware_decode_and_conversion_preserve_pq_precision() {
        let _runtime = EmbeddedMediaRuntime::initialize().expect("Media Foundation");
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("D3D11 hardware device");
        }
        let device = device.unwrap();
        let context = context.unwrap();
        let format = VideoFormat {
            transfer_function: VideoTransferFunction::Pq,
            color_primaries: crate::VideoColorPrimaries::Bt2020,
            color_matrix: VideoColorMatrix::Bt2020,
            ..color_test_format()
        };
        let reference = include_bytes!("../../fixtures/probe/hevc-p010-pq-precision-luma.bin")
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        assert_eq!(reference.len(), 877);
        let mut resources = unsafe {
            AdoptedResources::new(
                AdoptedD3d11Context {
                    device: device.as_raw(),
                    immediate_context: context.as_raw(),
                },
                format,
            )
        }
        .unwrap();
        for restart in 0..3 {
            resources.reset_decoder_views();
            let mut decoder =
                Decoder::new(&resources, format, WindowsDecoderMode::Hardware).unwrap();
            let frame = decoder
                .probe_frame(include_bytes!(
                    "../../fixtures/probe/hevc-p010-pq-precision.hevc"
                ))
                .expect("decode valid Main10 PQ precision access unit");
            assert_eq!(frame.format.pixel_format, VideoPixelFormat::P010);
            assert_eq!(frame.format.transfer_function, VideoTransferFunction::Pq);
            let read_row = |texture: &ID3D11Texture2D, subresource: u32| {
                let mut description = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    texture.GetDesc(&mut description);
                }
                description.Usage = D3D11_USAGE_STAGING;
                description.BindFlags = 0;
                description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                description.ArraySize = 1;
                description.MipLevels = 1;
                description.MiscFlags = 0;
                let mut staging = None;
                unsafe {
                    device
                        .CreateTexture2D(&description, None, Some(&mut staging))
                        .unwrap();
                }
                let staging = staging.unwrap();
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                unsafe {
                    context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, subresource, None);
                    context
                        .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                        .unwrap();
                    let bytes = std::slice::from_raw_parts(
                        mapped.pData.cast::<u8>(),
                        877 * if description.Format == DXGI_FORMAT_P010 {
                            2
                        } else {
                            4
                        },
                    )
                    .to_vec();
                    context.Unmap(&staging, 0);
                    (description.Format, bytes)
                }
            };
            let (input_format, input) = read_row(&frame.texture, frame.subresource);
            assert_eq!(input_format, DXGI_FORMAT_P010);
            for (index, bytes) in input.chunks_exact(2).enumerate() {
                let actual = u16::from_le_bytes([bytes[0], bytes[1]]);
                assert_eq!(actual & 63, 0, "P010 sample alignment at {index}");
                assert_eq!(
                    actual >> 6,
                    reference[index],
                    "Main10 decoded luma at {index}"
                );
            }
            let recorded = resources
                .record(0, &frame)
                .expect("convert actual P010/PQ decoder surface");
            assert_eq!(recorded.texture_format, D3d11TextureFormat::Rgb10A2);
            assert_eq!(recorded.color_space, D3d11ColorSpace::Pq2020);
            let output = unsafe { clone_interface::<ID3D11Texture2D>(recorded.texture) }.unwrap();
            let (output_format, bytes) = read_row(&output, 0);
            assert_eq!(output_format, DXGI_FORMAT_R10G10B10A2_UNORM);
            let mut levels = std::collections::HashSet::new();
            for (index, bytes) in bytes.chunks_exact(4).enumerate() {
                let pixel = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let expected = ((f64::from(reference[index]) - 64.0) * 1023.0 / 876.0)
                    .round()
                    .clamp(0.0, 1023.0) as i32;
                levels.insert(pixel & 1023);
                for shift in [0, 10, 20] {
                    let actual = ((pixel >> shift) & 1023) as i32;
                    assert!(
                        (actual - expected).abs() <= 2,
                        "restart={restart} PQ sample={index} channel={shift} actual={actual} expected={expected}"
                    );
                }
            }
            assert!(
                levels.len() >= 800,
                "retained only {} PQ gray levels",
                levels.len()
            );
            eprintln!(
                "HEVC Main10 PQ restart={restart}: P010 source matches software reference; RGB10A2/PQ retains {} gray levels",
                levels.len()
            );
            drop(frame);
            decoder.stop();
        }
    }

    #[test]
    #[ignore = "requires a hardware HEVC Main44410 MFT and Y410 UINT shader conversion"]
    fn hevc_444_hardware_decode_and_conversion_preserve_precision_and_chroma() {
        verify_hevc_444_hardware_precision(false);
    }

    #[test]
    #[ignore = "requires a hardware HEVC Main44410 MFT and Y410 PQ shader conversion"]
    fn hevc_hdr_444_hardware_decode_and_conversion_preserve_precision_and_chroma() {
        verify_hevc_444_hardware_precision(true);
    }

    fn verify_hevc_444_hardware_precision(hdr: bool) {
        let _runtime = EmbeddedMediaRuntime::initialize().expect("Media Foundation");
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("D3D11 hardware device");
        }
        let device = device.unwrap();
        let context = context.unwrap();
        let format = VideoFormat {
            codec: crate::VideoCodec::H265,
            width: 1920,
            height: 1080,
            pixel_format: VideoPixelFormat::Y410,
            chroma_format: crate::VideoChromaFormat::Cs444,
            transfer_function: if hdr {
                VideoTransferFunction::Pq
            } else {
                VideoTransferFunction::Sdr
            },
            color_primaries: if hdr {
                crate::VideoColorPrimaries::Bt2020
            } else {
                crate::VideoColorPrimaries::Bt709
            },
            color_matrix: if hdr {
                VideoColorMatrix::Bt2020
            } else {
                VideoColorMatrix::Bt709
            },
            ..color_test_format()
        };
        let mut resources = unsafe {
            AdoptedResources::new(
                AdoptedD3d11Context {
                    device: device.as_raw(),
                    immediate_context: context.as_raw(),
                },
                format,
            )
        }
        .expect("adopt D3D11 hardware device");
        for restart in 0..3 {
            resources.reset_decoder_views();
            let mut decoder = Decoder::new(&resources, format, WindowsDecoderMode::Hardware)
                .expect("configure hardware HEVC Main44410 decoder");
            let frame = decoder
                .probe_frame(if hdr {
                    include_bytes!("../../fixtures/probe/hevc-y410-pq-precision.hevc")
                } else {
                    include_bytes!("../../fixtures/probe/hevc-y410-precision.hevc")
                })
                .expect("decode lossless Main44410 precision access unit");
            assert_eq!(frame.format.pixel_format, VideoPixelFormat::Y410);
            assert_eq!(frame.format.chroma_format, crate::VideoChromaFormat::Cs444);
            assert_eq!(frame.format.pixel_format.bit_depth(), 10);
            assert_eq!(frame.format.transfer_function, format.transfer_function);
            assert_eq!(frame.format.color_primaries, format.color_primaries);
            assert_eq!(frame.format.color_matrix, format.color_matrix);
            let mut input_description = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                frame.texture.GetDesc(&mut input_description);
            }
            assert_eq!(input_description.Format, DXGI_FORMAT_Y410);
            let read_rows = |texture: &ID3D11Texture2D, subresource: u32| {
                let mut description = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    texture.GetDesc(&mut description);
                }
                description.Usage = D3D11_USAGE_STAGING;
                description.BindFlags = 0;
                description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                description.ArraySize = 1;
                description.MipLevels = 1;
                description.MiscFlags = 0;
                let mut staging = None;
                unsafe {
                    device
                        .CreateTexture2D(&description, None, Some(&mut staging))
                        .unwrap();
                }
                let staging = staging.unwrap();
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                unsafe {
                    context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, subresource, None);
                    context
                        .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                        .unwrap();
                    let gray = std::slice::from_raw_parts(mapped.pData.cast::<u32>(), 877).to_vec();
                    let chroma = std::slice::from_raw_parts(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(mapped.RowPitch as usize * 64)
                            .cast::<u32>(),
                        1920,
                    )
                    .to_vec();
                    context.Unmap(&staging, 0);
                    (gray, chroma)
                }
            };
            let decoded_pixels = read_rows(&frame.texture, frame.subresource);
            let recorded = resources
                .record(0, &frame)
                .expect("convert actual Y410 decoder surface");
            assert_eq!(recorded.texture_format, D3d11TextureFormat::Rgb10A2);
            assert_eq!(
                recorded.color_space,
                if hdr {
                    D3d11ColorSpace::Pq2020
                } else {
                    D3d11ColorSpace::Sdr709
                }
            );
            let output = unsafe { clone_interface::<ID3D11Texture2D>(recorded.texture) }.unwrap();
            let mut output_description = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                output.GetDesc(&mut output_description);
            }
            assert_eq!(output_description.Format, DXGI_FORMAT_R10G10B10A2_UNORM);
            let pixels = read_rows(&output, 0);
            for (index, pixel) in decoded_pixels.0.iter().copied().enumerate() {
                assert_eq!(
                    (pixel >> 10) & 1023,
                    index as u32 + 64,
                    "decoder luma precision at {index}"
                );
                assert_eq!(pixel & 1023, 512, "decoder neutral U at {index}");
                assert_eq!((pixel >> 20) & 1023, 512, "decoder neutral V at {index}");
            }
            for (index, pixel) in decoded_pixels.1.iter().copied().enumerate() {
                assert_eq!(
                    (pixel >> 10) & 1023,
                    512,
                    "decoder chroma-row luma at {index}"
                );
                assert_eq!(
                    pixel & 1023,
                    if index % 2 == 0 { 384 } else { 640 },
                    "decoder U at {index}"
                );
                assert_eq!(
                    (pixel >> 20) & 1023,
                    if index % 2 == 0 { 640 } else { 384 },
                    "decoder V at {index}"
                );
            }
            let mut levels = std::collections::HashSet::new();
            for (index, pixel) in pixels.0.iter().copied().enumerate() {
                let expected = (index as f64 * 1023.0 / 876.0).round() as i32;
                levels.insert(pixel & 1023);
                for shift in [0, 10, 20] {
                    let actual = ((pixel >> shift) & 1023) as i32;
                    assert!(
                        (actual - expected).abs() <= 2,
                        "restart={restart} grayscale sample={index} channel={shift} actual={actual} expected={expected}"
                    );
                }
            }
            assert!(
                levels.len() >= 800,
                "restart={restart} retained only {} grayscale levels",
                levels.len()
            );
            for (index, pixel) in pixels.1.iter().copied().enumerate() {
                let y = (512.0 - 64.0) / 876.0;
                let u = if index % 2 == 0 {
                    -128.0 / 896.0
                } else {
                    128.0 / 896.0
                };
                let v = -u;
                let expected = if hdr {
                    [
                        y + 1.4746 * v,
                        y - 0.1645531268 * u - 0.5713531268 * v,
                        y + 1.8814 * u,
                    ]
                } else {
                    [
                        y + 1.5748 * v,
                        y - 0.1873242729 * u - 0.4681242729 * v,
                        y + 1.8556 * u,
                    ]
                };
                for (channel, expected) in expected.into_iter().enumerate() {
                    let expected = (expected * 1023.0_f64).round() as i32;
                    let actual = ((pixel >> (channel * 10)) & 1023) as i32;
                    assert!(
                        (actual - expected).abs() <= 4,
                        "restart={restart} chroma sample={index} channel={channel} actual={actual} expected={expected}"
                    );
                }
            }
            eprintln!(
                "HEVC Main44410 hardware restart={restart}: actual Y410 -> RGB10A2, {} distinct gray levels, 1920 alternating chroma pixels verified",
                levels.len()
            );
            drop(frame);
            decoder.stop();
        }
    }

    #[test]
    fn conversion_allocates_only_visited_qrhi_slots_and_reuses_them() {
        let _runtime = EmbeddedMediaRuntime::initialize().expect("Media Foundation");
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("D3D11 test device");
        }
        let device = device.unwrap();
        let context = context.unwrap();
        // Headless Windows CI may expose D3D11 without its video interfaces.
        // Only that capability absence is a skip; conversion errors on capable
        // hardware must continue to fail this test.
        if let Err(error) = device.cast::<ID3D11VideoDevice>() {
            assert_eq!(
                error.code().0 as u32,
                0x80004002,
                "unexpected video probe error: {error}"
            );
            eprintln!("SKIP GPU conversion: D3D11 video interface unavailable ({error})");
            return;
        }
        let format = VideoFormat {
            codec: crate::VideoCodec::H264,
            width: 64,
            height: 64,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 10_000_000,
            pixel_format: VideoPixelFormat::Nv12,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: false,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: crate::VideoTransferFunction::Sdr,
            color_primaries: crate::VideoColorPrimaries::Bt709,
            color_matrix: crate::VideoColorMatrix::Bt709,
        };
        let mut resources = unsafe {
            AdoptedResources::new(
                AdoptedD3d11Context {
                    device: device.as_raw(),
                    immediate_context: context.as_raw(),
                },
                format,
            )
        }
        .expect("adopt device");
        resources
            .ensure_processor(64, 64, DXGI_FORMAT_NV12, 64, 64)
            .unwrap();
        assert!(
            resources
                .processor
                .as_ref()
                .unwrap()
                .slots
                .iter()
                .all(Option::is_none)
        );
        let mut description = frame_slot_description(64, 64, DXGI_FORMAT_NV12);
        description.BindFlags = ::windows::Win32::Graphics::Direct3D11::D3D11_BIND_DECODER.0 as u32;
        let mut texture = None;
        unsafe {
            device
                .CreateTexture2D(&description, None, Some(&mut texture))
                .unwrap();
        }
        let texture = texture.unwrap();
        let sample = unsafe {
            let buffer =
                MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &texture, 0, false).unwrap();
            let sample = MFCreateSample().unwrap();
            sample.AddBuffer(&buffer).unwrap();
            sample
        };
        let baseline_refs = sample_ref_count(&sample);
        for preferred in [
            VideoPixelFormat::P010,
            VideoPixelFormat::Ayuv,
            VideoPixelFormat::Y410,
        ] {
            let error = DecodedVideoFrame::from_sample(
                sample.clone(),
                format,
                crate::aperture::VideoAperture::new(format.width, format.height, None).unwrap(),
                preferred,
            )
            .err()
            .expect("startup fallback must not publish a downgraded frame");
            assert!(error.contains("produced Nv12 output below negotiated"));
            assert_eq!(sample_ref_count(&sample), baseline_refs);
        }
        let error = DecodedVideoFrame::from_sample(
            sample.clone(),
            VideoFormat {
                pixel_format: VideoPixelFormat::P010,
                ..format
            },
            crate::aperture::VideoAperture::new(format.width, format.height, None).unwrap(),
            VideoPixelFormat::P010,
        )
        .err()
        .expect("an eight-bit surface cannot claim ten-bit output");
        assert!(error.contains("does not match output media format P010"));
        assert_eq!(sample_ref_count(&sample), baseline_refs);
        let frame = DecodedVideoFrame::from_sample(
            sample.clone(),
            format,
            crate::aperture::VideoAperture::new(format.width, format.height, None).unwrap(),
            format.pixel_format,
        )
        .unwrap();
        assert_eq!(
            sample_ref_count(&sample),
            baseline_refs + 1,
            "decoded frame must lease the sample, not only its texture"
        );
        let mut ready = VecDeque::from([frame]);
        assert_eq!(sample_ref_count(&sample), baseline_refs + 1);
        let frame = ready.pop_back().unwrap();
        let first = resources.record(0, &frame).unwrap();
        assert_eq!(first.texture, resources.record(0, &frame).unwrap().texture);
        assert_ne!(first.texture, resources.record(3, &frame).unwrap().texture);
        assert_eq!(
            sample_ref_count(&sample),
            baseline_refs + 1,
            "recording must not release the sample before the frame token"
        );
        assert_eq!(
            resources
                .processor
                .as_ref()
                .unwrap()
                .slots
                .iter()
                .filter(|slot| slot.is_some())
                .count(),
            2
        );
        assert!(resources.record(8, &frame).is_err());
        drop(frame);
        assert_eq!(
            sample_ref_count(&sample),
            baseline_refs,
            "frame release must return its lease without leaking samples"
        );
        for changed in [
            VideoFormat {
                chroma_siting: VideoChromaSiting::TopLeft,
                ..format
            },
            VideoFormat {
                full_range: true,
                ..format
            },
            VideoFormat {
                transfer_function: VideoTransferFunction::Pq,
                ..format
            },
            VideoFormat {
                color_primaries: crate::VideoColorPrimaries::Bt2020,
                ..format
            },
            VideoFormat {
                color_matrix: VideoColorMatrix::Bt601,
                ..format
            },
        ] {
            resources.reconfigure(changed);
            assert!(
                resources.processor.is_none(),
                "color changes must discard converters and texture slots"
            );
            resources.reconfigure(format);
            resources
                .ensure_processor(64, 64, DXGI_FORMAT_NV12, 64, 64)
                .unwrap();
        }
        resources.reconfigure(VideoFormat {
            width: 128,
            ..format
        });
        assert!(resources.processor.is_none());
        resources
            .ensure_processor(128, 64, DXGI_FORMAT_NV12, 128, 64)
            .unwrap();
        assert!(
            resources
                .processor
                .as_ref()
                .unwrap()
                .slots
                .iter()
                .all(Option::is_none)
        );
        let enumerator = resources.processor.as_ref().unwrap().enumerator.clone();
        let enumerator_1: ID3D11VideoProcessorEnumerator1 = enumerator.cast().unwrap();
        let mut previous_texture = None;
        for (input_format, pixel_format, chroma_format) in [
            (
                DXGI_FORMAT_NV12,
                VideoPixelFormat::Nv12,
                VideoChromaFormat::Cs420,
            ),
            (
                DXGI_FORMAT_P010,
                VideoPixelFormat::P010,
                VideoChromaFormat::Cs420,
            ),
            (
                DXGI_FORMAT_Y410,
                VideoPixelFormat::Y410,
                VideoChromaFormat::Cs444,
            ),
            (
                DXGI_FORMAT_AYUV,
                VideoPixelFormat::Ayuv,
                VideoChromaFormat::Cs444,
            ),
            (
                DXGI_FORMAT_NV12,
                VideoPixelFormat::Nv12,
                VideoChromaFormat::Cs420,
            ),
        ] {
            for full_range in [true, false] {
                let format = VideoFormat {
                    width: 128,
                    pixel_format,
                    chroma_format,
                    full_range,
                    ..format
                };
                resources.reconfigure(format);
                assert!(resources.processor.is_none());
                let output_format = output_dxgi_format(format);
                let supported = unsafe {
                    enumerator.CheckVideoProcessorFormat(input_format).unwrap()
                        & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32
                        != 0
                        && enumerator.CheckVideoProcessorFormat(output_format).unwrap()
                            & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32
                            != 0
                        && enumerator_1
                            .CheckVideoProcessorFormatConversion(
                                input_format,
                                input_color_space(format).unwrap(),
                                output_format,
                                DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                            )
                            .unwrap()
                            .as_bool()
                };
                let result = resources.ensure_processor(128, 64, input_format, 128, 64);
                if !supported {
                    assert!(
                        result.is_err(),
                        "unsupported conversion must not fall back to RGBA8"
                    );
                    assert!(resources.processor.is_none());
                    continue;
                }
                result.unwrap();
                let processor = resources.processor.as_mut().unwrap();
                assert_eq!(processor.output_format, output_format);
                assert!(processor.input_views.is_empty());
                assert!(processor.slots.iter().all(Option::is_none));
                unsafe {
                    assert_eq!(
                        resources
                            .video_context_1
                            .VideoProcessorGetStreamColorSpace1(&processor.processor, 0,),
                        input_color_space(format).unwrap()
                    );
                    assert_eq!(
                        resources
                            .video_context_1
                            .VideoProcessorGetOutputColorSpace1(&processor.processor,),
                        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
                    );
                }
                let slot = FrameSlot::new(
                    &device,
                    &resources.video_device,
                    &processor.enumerator,
                    128,
                    64,
                    processor.output_format,
                )
                .unwrap();
                let mut description = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    slot.texture.GetDesc(&mut description);
                }
                assert_eq!(description.Format, output_format);
                if let Some(previous) = previous_texture.as_ref() {
                    assert_ne!(&slot.texture, previous);
                }
                previous_texture = Some(slot.texture.clone());
                processor.slots[0] = Some(slot);
                let identity = processor.processor.clone();
                resources
                    .ensure_processor(128, 64, input_format, 128, 64)
                    .unwrap();
                assert_eq!(resources.processor.as_ref().unwrap().processor, identity);
                assert!(resources.processor.as_ref().unwrap().slots[0].is_some());
            }
        }
    }

    #[test]
    fn full_decode_queue_reports_loss_synchronously_without_a_stale_recovery_event() {
        let encoded = Arc::new(BoundedQueue::new(2));
        let events = Arc::new(BoundedQueue::new(8));
        let submitter = D3d11FrameSubmitter {
            encoded: Arc::clone(&encoded),
            events: Arc::clone(&events),
        };
        assert_eq!(
            submitter.submit_video(encoded_frame(0, false)),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            submitter.submit_video(encoded_frame(1, false)),
            Ok(PushOutcome::Queued)
        );

        assert_eq!(
            submitter.submit_video(encoded_frame(2, false)),
            Ok(PushOutcome::DroppedOldest)
        );
        assert!(encoded.try_pop().is_none());
        assert_eq!(
            events.try_pop(),
            Some(BackendEvent::QueueOverflow(Subsystem::VideoDecode))
        );
        assert_eq!(events.try_pop(), None);
    }

    #[test]
    fn decoder_subresource_preserves_array_slice() {
        assert_eq!(decoder_array_slice(0, 1, 8), Ok(0));
        assert_eq!(decoder_array_slice(5, 1, 8), Ok(5));
        assert_eq!(decoder_array_slice(7, 1, 8), Ok(7));
        assert_eq!(decoder_array_slice(6, 3, 4), Ok(2));
        assert!(decoder_array_slice(12, 3, 4).is_err());
    }

    #[test]
    fn all_decoder_formats_map_to_video_processor_inputs() {
        let cases = [
            (
                DXGI_FORMAT_NV12,
                VideoPixelFormat::Nv12,
                VideoChromaFormat::Cs420,
            ),
            (
                DXGI_FORMAT_P010,
                VideoPixelFormat::P010,
                VideoChromaFormat::Cs420,
            ),
            (
                DXGI_FORMAT_AYUV,
                VideoPixelFormat::Ayuv,
                VideoChromaFormat::Cs444,
            ),
            (
                DXGI_FORMAT_Y410,
                VideoPixelFormat::Y410,
                VideoChromaFormat::Cs444,
            ),
        ];
        for (dxgi, pixel, chroma) in cases {
            let mapped = pixel_format_from_dxgi(dxgi).expect("supported decoder format");
            assert_eq!(mapped, pixel);
            assert_eq!(chroma_format(mapped), chroma);
        }
        assert_eq!(DXGI_FORMAT_R8G8B8A8_UNORM.0, 28);
    }

    #[test]
    fn frame_slots_convert_decoder_surfaces_to_qt_sdr_composition_targets() {
        let description = frame_slot_description(1920, 1080, DXGI_FORMAT_R8G8B8A8_UNORM);
        assert_eq!(description.Width, 1920);
        assert_eq!(description.Height, 1080);
        assert_eq!(description.Format, DXGI_FORMAT_R8G8B8A8_UNORM);
        assert_ne!(
            description.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32,
            0
        );
        assert_ne!(description.BindFlags & D3D11_BIND_RENDER_TARGET.0 as u32, 0);

        let p010 = VideoFormat {
            pixel_format: VideoPixelFormat::P010,
            ..color_test_format()
        };
        let y410 = VideoFormat {
            pixel_format: VideoPixelFormat::Y410,
            ..color_test_format()
        };
        let ten_bit = frame_slot_description(2560, 1440, output_dxgi_format(p010));
        assert_eq!(ten_bit.Format, DXGI_FORMAT_R10G10B10A2_UNORM);
        assert_eq!(output_dxgi_format(y410), ten_bit.Format);
        assert_eq!(
            d3d11_texture_format(ten_bit.Format),
            D3d11TextureFormat::Rgb10A2
        );
    }

    fn color_test_format() -> VideoFormat {
        VideoFormat {
            codec: crate::VideoCodec::H265,
            width: 1920,
            height: 1080,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 20_000_000,
            pixel_format: VideoPixelFormat::P010,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: false,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: VideoTransferFunction::Sdr,
            color_primaries: crate::VideoColorPrimaries::Bt709,
            color_matrix: VideoColorMatrix::Bt709,
        }
    }

    #[test]
    fn packed_444_color_conversion_does_not_require_subsampled_chroma_siting() {
        for pixel_format in [VideoPixelFormat::Ayuv, VideoPixelFormat::Y410] {
            for full_range in [false, true] {
                let format = VideoFormat {
                    pixel_format,
                    chroma_format: crate::VideoChromaFormat::Cs444,
                    full_range,
                    ..color_test_format()
                };
                assert_eq!(
                    input_color_space(format).unwrap(),
                    input_color_space(VideoFormat {
                        chroma_siting: VideoChromaSiting::TopLeft,
                        ..format
                    })
                    .unwrap(),
                );
            }
        }
        assert!(
            input_color_space(VideoFormat {
                chroma_siting: VideoChromaSiting::TopLeft,
                ..color_test_format()
            })
            .is_err()
        );
    }

    #[test]
    fn hdr_capability_probe_requires_actual_p010_pq_before_gpu_checks() {
        for format in [
            color_test_format(),
            VideoFormat {
                transfer_function: VideoTransferFunction::Hlg,
                ..color_test_format()
            },
            VideoFormat {
                transfer_function: VideoTransferFunction::Pq,
                pixel_format: VideoPixelFormat::Nv12,
                ..color_test_format()
            },
        ] {
            let result = unsafe {
                probe_hdr_conversion(
                    AdoptedD3d11Context {
                        device: std::ptr::null_mut(),
                        immediate_context: std::ptr::null_mut(),
                    },
                    format,
                )
            };
            assert_eq!(
                result.unwrap_err(),
                "HDR10 capability requires actual P010/PQ decoder output"
            );
        }
    }

    #[test]
    fn hdr_targets_preserve_pq_bt2020_in_rgb10a2() {
        for transfer in [VideoTransferFunction::Pq, VideoTransferFunction::Hlg] {
            let format = VideoFormat {
                transfer_function: transfer,
                color_primaries: crate::VideoColorPrimaries::Bt2020,
                color_matrix: VideoColorMatrix::Bt2020,
                ..color_test_format()
            };
            assert_eq!(output_dxgi_format(format), DXGI_FORMAT_R10G10B10A2_UNORM);
            assert_eq!(
                output_color_space(transfer),
                DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
            );
            assert_eq!(recorded_color_space(transfer), D3d11ColorSpace::Pq2020);
            assert_eq!(
                d3d11_texture_format(output_dxgi_format(format)),
                D3d11TextureFormat::Rgb10A2
            );
        }
    }

    #[test]
    fn hdr_input_spaces_preserve_transfer_and_range() {
        let pq = VideoFormat {
            transfer_function: VideoTransferFunction::Pq,
            color_primaries: crate::VideoColorPrimaries::Bt2020,
            color_matrix: VideoColorMatrix::Bt2020,
            ..color_test_format()
        };
        assert_eq!(
            input_color_space(pq).unwrap(),
            DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_LEFT_P2020
        );
        assert_eq!(
            input_color_space(VideoFormat {
                chroma_siting: VideoChromaSiting::TopLeft,
                ..pq
            })
            .unwrap(),
            DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_TOPLEFT_P2020
        );
        assert!(
            input_color_space(VideoFormat {
                full_range: true,
                ..pq
            })
            .is_err()
        );
        let hlg = VideoFormat {
            transfer_function: VideoTransferFunction::Hlg,
            chroma_siting: VideoChromaSiting::TopLeft,
            ..pq
        };
        assert!(
            input_color_space(VideoFormat {
                chroma_siting: VideoChromaSiting::Left,
                ..hlg
            })
            .is_err()
        );
        assert_eq!(
            input_color_space(hlg).unwrap(),
            DXGI_COLOR_SPACE_YCBCR_STUDIO_GHLG_TOPLEFT_P2020
        );
        assert_eq!(
            input_color_space(VideoFormat {
                full_range: true,
                ..hlg
            })
            .unwrap(),
            DXGI_COLOR_SPACE_YCBCR_FULL_GHLG_TOPLEFT_P2020
        );
        assert!(
            input_color_space(VideoFormat {
                pixel_format: VideoPixelFormat::Nv12,
                ..pq
            })
            .is_err()
        );
    }

    #[test]
    fn sdr_conversion_preserves_decoder_precision_and_expands_only_limited_range() {
        for (pixel_format, chroma_format, output_format, texture_format) in [
            (
                VideoPixelFormat::Nv12,
                VideoChromaFormat::Cs420,
                DXGI_FORMAT_R8G8B8A8_UNORM,
                D3d11TextureFormat::Rgba8,
            ),
            (
                VideoPixelFormat::Ayuv,
                VideoChromaFormat::Cs444,
                DXGI_FORMAT_R8G8B8A8_UNORM,
                D3d11TextureFormat::Rgba8,
            ),
            (
                VideoPixelFormat::P010,
                VideoChromaFormat::Cs420,
                DXGI_FORMAT_R10G10B10A2_UNORM,
                D3d11TextureFormat::Rgb10A2,
            ),
            (
                VideoPixelFormat::Y410,
                VideoChromaFormat::Cs444,
                DXGI_FORMAT_R10G10B10A2_UNORM,
                D3d11TextureFormat::Rgb10A2,
            ),
        ] {
            for full_range in [false, true] {
                let format = VideoFormat {
                    codec: crate::VideoCodec::H265,
                    width: 64,
                    height: 64,
                    frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
                    frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
                    average_bitrate: 10_000_000,
                    pixel_format,
                    chroma_format,
                    full_range,
                    chroma_siting: VideoChromaSiting::Left,
                    transfer_function: VideoTransferFunction::Sdr,
                    color_primaries: crate::VideoColorPrimaries::Bt709,
                    color_matrix: VideoColorMatrix::Bt709,
                };
                assert_eq!(output_dxgi_format(format), output_format);
                assert_eq!(d3d11_texture_format(output_format), texture_format);
                assert_eq!(
                    recorded_color_space(format.transfer_function),
                    D3d11ColorSpace::Sdr709
                );
                assert_eq!(
                    output_color_space(format.transfer_function),
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
                );
                assert_eq!(
                    input_color_space(format).unwrap(),
                    if full_range {
                        DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709
                    } else {
                        DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709
                    }
                );
            }
        }
    }

    #[test]
    fn embedded_module_has_no_window_swapchain_sdl_or_present_path() {
        let source = include_str!("embedded.rs");
        for forbidden in [
            concat!("Create", "Window"),
            concat!("Create", "SwapChain"),
            concat!(".Pre", "sent("),
            concat!("sdl", "2::"),
        ] {
            assert!(
                !source.contains(forbidden),
                "embedded producer contains forbidden presentation token {forbidden}"
            );
        }
    }

    #[test]
    fn encoded_submitter_is_safe_to_move_to_the_transport_thread() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<D3d11FrameSubmitter>();
        assert_send_sync::<D3d11FrameProducer>();
        assert_send_sync::<D3d11Frame>();
    }
}
