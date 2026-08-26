use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::mem::size_of;
use std::num::NonZeroU32;

use ::windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HINSTANCE, HMODULE, HWND, LPARAM, LRESULT,
    LUID, RECT, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM,
};
use ::windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use ::windows::Win32::Graphics::Direct3D10::ID3D10Multithread;
use ::windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_OPTIMAL_SPEED, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor,
    ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use ::windows::Win32::Graphics::Direct3D11on12::{D3D11On12CreateDevice, ID3D11On12Device};
use ::windows::Win32::Graphics::Direct3D12::{
    D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE,
    D3D12_COMMAND_QUEUE_PRIORITY_NORMAL, D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device,
};
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_RATIONAL,
    DXGI_SAMPLE_DESC,
};
use ::windows::Win32::Graphics::Dxgi::{
    DXGI_FEATURE_PRESENT_ALLOW_TEARING, DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT,
    DXGI_PRESENT_ALLOW_TEARING, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIDevice,
    IDXGIFactory2, IDXGIFactory5, IDXGISwapChain1, IDXGISwapChain2,
};
use ::windows::Win32::Media::MediaFoundation::{IMFDXGIDeviceManager, MFCreateDXGIDeviceManager};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::WaitForSingleObject;
#[cfg(test)]
use ::windows::Win32::UI::WindowsAndMessaging::WS_POPUP;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, HTTRANSPARENT, IsWindow, MA_NOACTIVATE, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassW, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_MOUSEACTIVATE, WM_NCHITTEST, WNDCLASSW,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW,
};
use ::windows::core::{BOOL, IUnknown, Interface, w};

use crate::{
    Bounds, ExistingWindow, OwnedWindow, SurfaceTarget, VideoCodec, VideoFormat, WindowHandle,
    WindowsGraphicsApi,
};

const WINDOW_CLASS: ::windows::core::PCWSTR = w!("OpenNOWStreamerD3D11Surface");

struct DeviceResources {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    manager: IMFDXGIDeviceManager,
    _d3d12: Option<D3d12Owners>,
}

struct D3d12Owners {
    _device: ID3D12Device,
    _queue: ID3D12CommandQueue,
    _on12: ID3D11On12Device,
}

impl DeviceResources {
    fn new(api: WindowsGraphicsApi) -> Result<Self, String> {
        match api {
            WindowsGraphicsApi::D3d11 => Self::new_d3d11(),
            WindowsGraphicsApi::D3d12 => Self::new_d3d12(),
        }
    }

    fn finish_d3d11_device(
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        d3d12: Option<D3d12Owners>,
    ) -> Result<Self, String> {
        unsafe {
            let multithread: ID3D10Multithread =
                context.cast().map_err(|error| error.to_string())?;
            let _ = multithread.SetMultithreadProtected(true);
            let video_device: ID3D11VideoDevice =
                device.cast().map_err(|error| error.to_string())?;
            let video_context: ID3D11VideoContext =
                context.cast().map_err(|error| error.to_string())?;
            let mut reset_token = 0;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
                .map_err(|error| error.to_string())?;
            let manager = manager.ok_or("MFCreateDXGIDeviceManager returned no manager")?;
            manager
                .ResetDevice(&device, reset_token)
                .map_err(|error| error.to_string())?;
            Ok(Self {
                device,
                context,
                video_device,
                video_context,
                manager,
                _d3d12: d3d12,
            })
        }
    }

    fn new_d3d11() -> Result<Self, String> {
        unsafe {
            let mut device = None;
            let mut context = None;
            let levels = [
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
            ];
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|error| error.to_string())?;
            Self::finish_d3d11_device(
                device.ok_or("D3D11CreateDevice returned no device")?,
                context.ok_or("D3D11CreateDevice returned no immediate context")?,
                None,
            )
        }
    }

    fn new_d3d12() -> Result<Self, String> {
        unsafe {
            let mut d3d12_device = None;
            D3D12CreateDevice(None::<&IUnknown>, D3D_FEATURE_LEVEL_11_0, &mut d3d12_device)
                .map_err(|error| format!("D3D12CreateDevice: {error}"))?;
            let d3d12_device: ID3D12Device =
                d3d12_device.ok_or("D3D12CreateDevice returned no device")?;
            let queue: ID3D12CommandQueue = d3d12_device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                    Priority: D3D12_COMMAND_QUEUE_PRIORITY_NORMAL.0,
                    Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
                    NodeMask: 0,
                })
                .map_err(|error| format!("ID3D12Device::CreateCommandQueue: {error}"))?;
            let queue_unknown = queue.cast().map_err(|error| error.to_string())?;
            let mut device = None;
            let mut context = None;
            D3D11On12CreateDevice(
                &d3d12_device,
                (D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT).0 as u32,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                Some(&[Some(queue_unknown)]),
                0,
                Some(&mut device),
                Some(&mut context),
                None,
            )
            .map_err(|error| format!("D3D11On12CreateDevice: {error}"))?;
            let device = device.ok_or("D3D11On12CreateDevice returned no D3D11 device")?;
            let context = context.ok_or("D3D11On12CreateDevice returned no immediate context")?;
            let on12 = device
                .cast()
                .map_err(|error| format!("D3D11-on-12 device interface: {error}"))?;
            Self::finish_d3d11_device(
                device,
                context,
                Some(D3d12Owners {
                    _device: d3d12_device,
                    _queue: queue,
                    _on12: on12,
                }),
            )
        }
    }
}

enum RenderWindow {
    Existing(HWND),
    Owned {
        hwnd: HWND,
        parent: Option<WindowHandle>,
        visible: bool,
    },
}

impl RenderWindow {
    fn new(target: SurfaceTarget) -> Result<Self, String> {
        match target {
            SurfaceTarget::Existing(ExistingWindow { hwnd }) => {
                let hwnd = hwnd_from_handle(hwnd);
                if unsafe { !IsWindow(Some(hwnd)).as_bool() } {
                    return Err("supplied HWND is not a live Win32 window".to_owned());
                }
                Ok(Self::Existing(hwnd))
            }
            SurfaceTarget::Owned(window) => create_owned_window(window).map(|hwnd| Self::Owned {
                hwnd,
                parent: window.parent,
                visible: window.visible,
            }),
        }
    }

    fn hwnd(&self) -> HWND {
        match self {
            Self::Existing(hwnd) | Self::Owned { hwnd, .. } => *hwnd,
        }
    }

    fn can_update(&self, target: SurfaceTarget) -> bool {
        match (self, target) {
            (Self::Existing(current), SurfaceTarget::Existing(target)) => {
                current.0 == hwnd_from_handle(target.hwnd).0
            }
            (
                Self::Owned {
                    parent: current, ..
                },
                SurfaceTarget::Owned(target),
            ) => OwnedWindow {
                parent: *current,
                bounds: target.bounds,
                visible: target.visible,
            }
            .has_same_parent(target),
            _ => false,
        }
    }

    fn update(&mut self, target: SurfaceTarget) -> Result<(), String> {
        match (self, target) {
            (Self::Existing(hwnd), SurfaceTarget::Existing(_)) => {
                if unsafe { IsWindow(Some(*hwnd)).as_bool() } {
                    Ok(())
                } else {
                    Err("supplied HWND is no longer a live Win32 window".to_owned())
                }
            }
            (Self::Owned { hwnd, visible, .. }, SurfaceTarget::Owned(window)) => {
                position_owned_window(*hwnd, window)?;
                *visible = window.visible;
                Ok(())
            }
            _ => Err("surface ownership changed during in-place update".to_owned()),
        }
    }

    fn client_size(&self) -> Result<(u32, u32), String> {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(self.hwnd(), &mut rect).map_err(|error| error.to_string())?;
        }
        Ok((
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        ))
    }

    fn is_visible(&self) -> bool {
        match self {
            Self::Existing(_) => true,
            Self::Owned { visible, .. } => *visible,
        }
    }
}

impl Drop for RenderWindow {
    fn drop(&mut self) {
        if let Self::Owned { hwnd, .. } = self {
            unsafe {
                let _ = DestroyWindow(*hwnd);
            }
        }
    }
}

struct ProcessorResources {
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    input_views: HashMap<(usize, u32), ID3D11VideoProcessorInputView>,
    output_view: ID3D11VideoProcessorOutputView,
}

struct SwapChainResources {
    swap_chain: IDXGISwapChain1,
    frame_latency_waitable: HANDLE,
    flags: DXGI_SWAP_CHAIN_FLAG,
    allow_tearing: bool,
}

pub(super) struct Graphics {
    processor: Option<ProcessorResources>,
    swap_chain: IDXGISwapChain1,
    frame_latency_waitable: HANDLE,
    swap_chain_flags: DXGI_SWAP_CHAIN_FLAG,
    allow_tearing: bool,
    has_presented: bool,
    window: RenderWindow,
    resources: DeviceResources,
    video_format: VideoFormat,
    swap_size: (u32, u32),
}

impl Graphics {
    pub(super) fn probe(api: WindowsGraphicsApi) -> Result<Self, String> {
        let format = VideoFormat {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            frame_rate_numerator: NonZeroU32::new(60).expect("non-zero"),
            frame_rate_denominator: NonZeroU32::new(1).expect("non-zero"),
            average_bitrate: 10_000_000,
        };
        Self::new(
            api,
            SurfaceTarget::Owned(OwnedWindow {
                parent: None,
                bounds: Bounds {
                    x: 0,
                    y: 0,
                    width: 64,
                    height: 64,
                },
                visible: false,
            }),
            format,
        )
    }

    pub(super) fn new(
        api: WindowsGraphicsApi,
        target: SurfaceTarget,
        video_format: VideoFormat,
    ) -> Result<Self, String> {
        let resources = DeviceResources::new(api)?;
        let window = RenderWindow::new(target)?;
        let swap_size = window.client_size()?;
        let swap_chain = create_swap_chain(&resources.device, window.hwnd(), swap_size)?;
        let mut graphics = Self {
            processor: None,
            swap_chain: swap_chain.swap_chain,
            frame_latency_waitable: swap_chain.frame_latency_waitable,
            swap_chain_flags: swap_chain.flags,
            allow_tearing: swap_chain.allow_tearing,
            has_presented: false,
            window,
            resources,
            video_format,
            swap_size,
        };
        graphics.ensure_processor(video_format.width, video_format.height)?;
        Ok(graphics)
    }

    pub(super) fn device_manager(&self) -> &IMFDXGIDeviceManager {
        &self.resources.manager
    }

    pub(super) fn adapter_luid(&self) -> Result<LUID, String> {
        unsafe {
            let dxgi_device: IDXGIDevice = self
                .resources
                .device
                .cast()
                .map_err(|error| error.to_string())?;
            let adapter = dxgi_device
                .GetAdapter()
                .map_err(|error| error.to_string())?;
            Ok(adapter
                .GetDesc()
                .map_err(|error| error.to_string())?
                .AdapterLuid)
        }
    }

    pub(super) fn video_format(&self) -> VideoFormat {
        self.video_format
    }

    pub(super) fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub(super) fn set_surface(
        &mut self,
        target: SurfaceTarget,
        video_format: VideoFormat,
    ) -> Result<(), String> {
        if self.window.can_update(target) {
            self.window.update(target)?;
            self.video_format = video_format;
            self.resize_if_needed()?;
            return self.ensure_processor(video_format.width, video_format.height);
        }
        let window = RenderWindow::new(target)?;
        let swap_size = window.client_size()?;
        let swap_chain = create_swap_chain(&self.resources.device, window.hwnd(), swap_size)?;
        self.processor = None;
        let old_swap_chain = std::mem::replace(&mut self.swap_chain, swap_chain.swap_chain);
        drop(old_swap_chain);
        self.frame_latency_waitable = swap_chain.frame_latency_waitable;
        self.swap_chain_flags = swap_chain.flags;
        self.allow_tearing = swap_chain.allow_tearing;
        self.has_presented = false;
        self.window = window;
        self.swap_size = swap_size;
        self.video_format = video_format;
        self.ensure_processor(video_format.width, video_format.height)
    }

    pub(super) fn reconfigure_video(&mut self, format: VideoFormat) -> Result<(), String> {
        self.video_format = format;
        self.processor = None;
        self.ensure_processor(format.width, format.height)
    }

    pub(super) fn is_present_ready(&self) -> Result<bool, String> {
        if !self.has_presented {
            return Ok(true);
        }
        let result = unsafe { WaitForSingleObject(self.frame_latency_waitable, 0) };
        if result == WAIT_OBJECT_0 {
            Ok(true)
        } else if result == WAIT_TIMEOUT {
            Ok(false)
        } else if result == WAIT_FAILED {
            Err(format!(
                "DXGI frame-latency wait failed: {}",
                unsafe { GetLastError() }.0
            ))
        } else {
            Err(format!(
                "unexpected DXGI frame-latency wait result: {}",
                result.0
            ))
        }
    }

    pub(super) fn present(
        &mut self,
        texture: &ID3D11Texture2D,
        subresource: u32,
        _timestamp_100ns: i64,
        _duration_100ns: i64,
    ) -> Result<(), String> {
        self.resize_if_needed()?;
        let mut texture_description = Default::default();
        unsafe {
            texture.GetDesc(&mut texture_description);
        }
        self.ensure_processor(texture_description.Width, texture_description.Height)?;
        let processor = self
            .processor
            .as_mut()
            .ok_or("video processor is unavailable")?;

        unsafe {
            let input_description = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: subresource,
                    },
                },
            };
            let input_key = (texture.as_raw() as usize, subresource);
            let input_view = if let Some(view) = processor.input_views.get(&input_key) {
                view.clone()
            } else {
                let mut input_view = None;
                self.resources
                    .video_device
                    .CreateVideoProcessorInputView(
                        texture,
                        &processor.enumerator,
                        &input_description,
                        Some(&mut input_view),
                    )
                    .map_err(|error| error.to_string())?;
                let input_view =
                    input_view.ok_or("D3D11 returned no video processor input view")?;
                processor.input_views.insert(input_key, input_view.clone());
                input_view
            };

            let source = RECT {
                left: 0,
                top: 0,
                right: processor.input_width as i32,
                bottom: processor.input_height as i32,
            };
            let destination = fit_rect(
                processor.input_width,
                processor.input_height,
                processor.output_width,
                processor.output_height,
            );
            self.resources
                .video_context
                .VideoProcessorSetStreamFrameFormat(
                    &processor.processor,
                    0,
                    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                );
            self.resources
                .video_context
                .VideoProcessorSetStreamSourceRect(&processor.processor, 0, true, Some(&source));
            self.resources
                .video_context
                .VideoProcessorSetStreamDestRect(&processor.processor, 0, true, Some(&destination));
            self.resources
                .video_context
                .VideoProcessorSetOutputTargetRect(
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
            let blit = self.resources.video_context.VideoProcessorBlt(
                &processor.processor,
                &processor.output_view,
                0,
                std::slice::from_ref(&stream),
            );
            ManuallyDrop::drop(&mut stream.pInputSurface);
            blit.map_err(|error| error.to_string())?;
            if self.resources._d3d12.is_some() {
                self.resources.context.Flush();
            }
            let present_flags = if self.allow_tearing {
                DXGI_PRESENT_ALLOW_TEARING
            } else {
                DXGI_PRESENT(0)
            };
            self.swap_chain
                .Present(0, present_flags)
                .ok()
                .map_err(|error| error.to_string())?;
            self.has_presented = true;
        }
        Ok(())
    }

    pub(super) fn pump_window_messages(&self) {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn resize_if_needed(&mut self) -> Result<(), String> {
        let size = self.window.client_size()?;
        if size == self.swap_size {
            return Ok(());
        }
        self.processor = None;
        unsafe {
            self.swap_chain
                .ResizeBuffers(
                    2,
                    size.0,
                    size.1,
                    DXGI_FORMAT_UNKNOWN,
                    self.swap_chain_flags,
                )
                .map_err(|error| error.to_string())?;
        }
        self.swap_size = size;
        Ok(())
    }

    fn ensure_processor(&mut self, input_width: u32, input_height: u32) -> Result<(), String> {
        if self.processor.as_ref().is_some_and(|processor| {
            processor.input_width == input_width
                && processor.input_height == input_height
                && processor.output_width == self.swap_size.0
                && processor.output_height == self.swap_size.1
        }) {
            return Ok(());
        }
        let description = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: self.video_format.frame_rate_numerator.get(),
                Denominator: self.video_format.frame_rate_denominator.get(),
            },
            InputWidth: input_width,
            InputHeight: input_height,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: self.video_format.frame_rate_numerator.get(),
                Denominator: self.video_format.frame_rate_denominator.get(),
            },
            OutputWidth: self.swap_size.0,
            OutputHeight: self.swap_size.1,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        unsafe {
            let enumerator = self
                .resources
                .video_device
                .CreateVideoProcessorEnumerator(&description)
                .map_err(|error| error.to_string())?;
            let processor = self
                .resources
                .video_device
                .CreateVideoProcessor(&enumerator, 0)
                .map_err(|error| error.to_string())?;
            let back_buffer: ID3D11Texture2D = self
                .swap_chain
                .GetBuffer(0)
                .map_err(|error| error.to_string())?;
            let output_description = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view = None;
            self.resources
                .video_device
                .CreateVideoProcessorOutputView(
                    &back_buffer,
                    &enumerator,
                    &output_description,
                    Some(&mut output_view),
                )
                .map_err(|error| error.to_string())?;
            self.processor = Some(ProcessorResources {
                input_width,
                input_height,
                output_width: self.swap_size.0,
                output_height: self.swap_size.1,
                enumerator,
                processor,
                input_views: HashMap::new(),
                output_view: output_view.ok_or("D3D11 returned no video processor output view")?,
            });
        }
        Ok(())
    }
}

fn create_swap_chain(
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
) -> Result<SwapChainResources, String> {
    unsafe {
        let dxgi_device: IDXGIDevice = device.cast().map_err(|error| error.to_string())?;
        let adapter = dxgi_device
            .GetAdapter()
            .map_err(|error| error.to_string())?;
        let factory: IDXGIFactory2 = adapter.GetParent().map_err(|error| error.to_string())?;
        let allow_tearing = supports_tearing(&factory);
        let flags = if allow_tearing {
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT | DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
        } else {
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT
        };
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: size.0,
            Height: size.1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            // IDXGISwapChain2::SetMaximumFrameLatency is only valid for a
            // waitable swap chain. Preserve the flag in ResizeBuffers too.
            Flags: flags.0 as u32,
        };
        let swap_chain = factory
            .CreateSwapChainForHwnd(device, hwnd, &description, None, None)
            .map_err(|error| error.to_string())?;
        let _ = factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER);
        let swap_chain_2: IDXGISwapChain2 = swap_chain.cast().map_err(|error| error.to_string())?;
        swap_chain_2
            .SetMaximumFrameLatency(1)
            .map_err(|error| error.to_string())?;
        let frame_latency_waitable = swap_chain_2.GetFrameLatencyWaitableObject();
        if frame_latency_waitable.0.is_null() {
            return Err("DXGI returned no frame-latency waitable object".to_owned());
        }
        Ok(SwapChainResources {
            swap_chain,
            frame_latency_waitable,
            flags,
            allow_tearing,
        })
    }
}

fn supports_tearing(factory: &IDXGIFactory2) -> bool {
    let Ok(factory): Result<IDXGIFactory5, _> = factory.cast() else {
        return false;
    };
    let mut supported = BOOL::default();
    unsafe {
        factory
            .CheckFeatureSupport(
                DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                (&mut supported as *mut BOOL).cast(),
                size_of::<BOOL>() as u32,
            )
            .is_ok()
            && supported.as_bool()
    }
}

fn create_owned_window(window: OwnedWindow) -> Result<HWND, String> {
    register_window_class()?;
    let module = unsafe { GetModuleHandleW(None).map_err(|error| error.to_string())? };
    let instance = HINSTANCE(module.0);
    let parent = window.parent.map(hwnd_from_handle);
    let (style, extended_style) = owned_window_styles(parent.is_some());
    let hwnd = unsafe {
        CreateWindowExW(
            extended_style,
            WINDOW_CLASS,
            w!("OpenNOW Stream"),
            style,
            window.bounds.x,
            window.bounds.y,
            window.bounds.width as i32,
            window.bounds.height as i32,
            parent,
            None,
            Some(instance),
            None,
        )
        .map_err(|error| error.to_string())?
    };
    if let Err(error) = position_owned_window(hwnd, window) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    Ok(hwnd)
}

fn owned_window_styles(has_parent: bool) -> (WINDOW_STYLE, WINDOW_EX_STYLE) {
    let style = WS_CLIPCHILDREN
        | WS_CLIPSIBLINGS
        | if has_parent {
            WS_CHILD
        } else {
            WS_OVERLAPPEDWINDOW
        };
    let extended_style = WS_EX_NOACTIVATE
        | WS_EX_TRANSPARENT
        | if has_parent {
            WINDOW_EX_STYLE(0)
        } else {
            WS_EX_APPWINDOW
        };
    (style, extended_style)
}

fn position_owned_window(hwnd: HWND, window: OwnedWindow) -> Result<(), String> {
    let visibility = if window.visible {
        SWP_SHOWWINDOW
    } else {
        SWP_HIDEWINDOW
    };
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            window.bounds.x,
            window.bounds.y,
            window.bounds.width as i32,
            window.bounds.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER | visibility,
        )
        .map_err(|error| error.to_string())
    }
}

fn register_window_class() -> Result<(), String> {
    let module = unsafe { GetModuleHandleW(None).map_err(|error| error.to_string())? };
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(module.0),
        lpszClassName: WINDOW_CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } != 0 {
        return Ok(());
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_CLASS_ALREADY_EXISTS {
        Ok(())
    } else {
        Err(format!("RegisterClassW failed: {error:?}"))
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(result) = owned_window_message_result(message) {
        result
    } else {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

fn owned_window_message_result(message: u32) -> Option<LRESULT> {
    match message {
        WM_NCHITTEST => Some(LRESULT(HTTRANSPARENT as isize)),
        WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
        _ => None,
    }
}

fn hwnd_from_handle(handle: WindowHandle) -> HWND {
    HWND(handle.get() as *mut c_void)
}

fn fit_rect(input_width: u32, input_height: u32, output_width: u32, output_height: u32) -> RECT {
    let input_aspect = input_width as f64 / input_height as f64;
    let output_aspect = output_width as f64 / output_height as f64;
    if output_aspect > input_aspect {
        let width = (output_height as f64 * input_aspect).round() as i32;
        let left = (output_width as i32 - width) / 2;
        RECT {
            left,
            top: 0,
            right: left + width,
            bottom: output_height as i32,
        }
    } else {
        let height = (output_width as f64 / input_aspect).round() as i32;
        let top = (output_height as i32 - height) / 2;
        RECT {
            left: 0,
            top,
            right: output_width as i32,
            bottom: top + height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE;

    #[test]
    fn letterboxes_wide_target() {
        assert_eq!(
            fit_rect(1920, 1080, 1024, 1024),
            RECT {
                left: 0,
                top: 224,
                right: 1024,
                bottom: 800,
            }
        );
    }

    #[test]
    fn owned_child_style_is_non_activating_and_input_transparent() {
        let (style, extended_style) = owned_window_styles(true);

        assert_ne!(style.0 & WS_CHILD.0, 0);
        assert_eq!(style.0 & WS_POPUP.0, 0);
        assert_eq!(style.0 & WS_VISIBLE.0, 0);
        assert_ne!(extended_style.0 & WS_EX_NOACTIVATE.0, 0);
        assert_ne!(extended_style.0 & WS_EX_TRANSPARENT.0, 0);
    }

    #[test]
    fn owned_top_level_style_is_a_distinct_non_activating_app_window() {
        let (style, extended_style) = owned_window_styles(false);

        assert_ne!(style.0 & WS_OVERLAPPEDWINDOW.0, 0);
        assert_eq!(style.0 & WS_POPUP.0, 0);
        assert_eq!(style.0 & WS_CHILD.0, 0);
        assert_eq!(style.0 & WS_VISIBLE.0, 0);
        assert_ne!(extended_style.0 & WS_EX_APPWINDOW.0, 0);
        assert_ne!(extended_style.0 & WS_EX_NOACTIVATE.0, 0);
        assert_ne!(extended_style.0 & WS_EX_TRANSPARENT.0, 0);
    }

    #[test]
    fn owned_window_rejects_hit_testing_and_mouse_activation() {
        assert_eq!(
            owned_window_message_result(WM_NCHITTEST),
            Some(LRESULT(HTTRANSPARENT as isize))
        );
        assert_eq!(
            owned_window_message_result(WM_MOUSEACTIVATE),
            Some(LRESULT(MA_NOACTIVATE as isize))
        );
        assert_eq!(owned_window_message_result(0), None);
    }

    #[test]
    fn d3d11_on_12_exposes_video_processing_interfaces() {
        Graphics::probe(WindowsGraphicsApi::D3d12)
            .expect("D3D11-on-12 must support the complete graphics presentation path");
    }
}
