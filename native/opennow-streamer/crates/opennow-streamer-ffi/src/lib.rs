use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};

use opennow_streamer_core::{Engine, EventSender};
use opennow_streamer_platform::{
    CapturedInput, CapturedInputQueue, EmbeddedInputCapture, EmbeddedLocalAction,
    EmbeddedRuntimeConfig, GraphicsApi, GraphicsContext, GraphicsFramePublisher,
    GraphicsFrameToken, GraphicsRecordCommand, GraphicsRecordedFrame, GraphicsRuntimeError,
    GraphicsTextureFormat, RenderThreadGraphics, SharedVulkanDevice, WindowsAdapterLuid,
    create_embedded_runtime_with_config,
};
use opennow_streamer_protocol::log;
use opennow_streamer_protocol::{Command, error};
use serde_json::Value;

static FIRST_FRAME_LOGGED: AtomicBool = AtomicBool::new(false);

pub const OPENNOW_STREAMER_FFI_ABI_VERSION: u32 = 10;
pub const OPENNOW_STREAMER_MAX_TEXT_BYTES: usize =
    opennow_streamer_protocol::text_input::MAX_TEXT_BYTES;
pub const OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION: u32 = 1;
const DEFAULT_MAX_COMMAND_BYTES: usize = 1024 * 1024;
const MAX_QUEUE_CAPACITY: usize = 4096;
const MAX_COMMAND_BYTES: usize = 16 * 1024 * 1024;

pub type OpenNowStreamerCallback =
    Option<unsafe extern "C" fn(bytes: *const u8, length: usize, user_data: *mut c_void)>;
pub type OpenNowStreamerFrameAvailableCallback =
    Option<unsafe extern "C" fn(user_data: *mut c_void)>;
pub type OpenNowStreamerCursorCallback =
    Option<unsafe extern "C" fn(bytes: *const u8, length: usize, user_data: *mut c_void)>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpenNowStreamerConfig {
    pub abi_version: u32,
    pub struct_size: usize,
    pub command_queue_capacity: usize,
    pub response_queue_capacity: usize,
    pub event_queue_capacity: usize,
    pub max_command_bytes: usize,
    pub response_callback: OpenNowStreamerCallback,
    pub event_callback: OpenNowStreamerCallback,
    pub frame_available_callback: OpenNowStreamerFrameAvailableCallback,
    pub cursor_callback: OpenNowStreamerCursorCallback,
    pub user_data: *mut c_void,
    pub vulkan_device: *const OpenNowStreamerVulkanDevice,
    pub windows_adapter_luid: u64,
}

pub struct OpenNowStreamerVulkanDevice {
    device: Arc<SharedVulkanDevice>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OpenNowStreamerVulkanDeviceInfo {
    pub version: u32,
    pub struct_size: usize,
    pub instance: *mut c_void,
    pub physical_device: *mut c_void,
    pub device: *mut c_void,
    pub graphics_queue: *mut c_void,
    pub graphics_queue_family_index: u32,
    pub graphics_queue_index: u32,
    pub api_version: u32,
}

#[unsafe(no_mangle)]
/// Creates an owned device for adoption by the shell and embedded decoder.
///
/// # Safety
/// `output` must point to writable storage for one handle pointer.
pub unsafe extern "C" fn opennow_streamer_vulkan_device_create(
    output: *mut *mut OpenNowStreamerVulkanDevice,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if output.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        unsafe { output.write(ptr::null_mut()) };
        #[cfg(target_os = "linux")]
        {
            let device = match SharedVulkanDevice::create() {
                Ok(device) => device,
                Err(error) => {
                    use std::io::Write as _;
                    let reason: String = error
                        .to_string()
                        .chars()
                        .take(512)
                        .map(|character| {
                            if character.is_control() {
                                ' '
                            } else {
                                character
                            }
                        })
                        .collect();
                    log::log_line("WARN", "vulkan-device", &reason);
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "embedded Vulkan unavailable: {reason}"
                    );
                    return OpenNowStreamerStatus::GraphicsUnavailable;
                }
            };
            unsafe {
                output.write(Box::into_raw(Box::new(OpenNowStreamerVulkanDevice {
                    device,
                })))
            };
            OpenNowStreamerStatus::Ok
        }
        #[cfg(not(target_os = "linux"))]
        OpenNowStreamerStatus::GraphicsUnavailable
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Returns borrowed native objects retained by the device owner.
///
/// # Safety
/// `handle` must be live and must not race with destroy. `output` must point to writable
/// storage for one complete info structure. Native objects must not outlive their owner.
pub unsafe extern "C" fn opennow_streamer_vulkan_device_info(
    handle: *const OpenNowStreamerVulkanDevice,
    output: *mut OpenNowStreamerVulkanDeviceInfo,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() || output.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        #[cfg(target_os = "linux")]
        {
            let info = unsafe { &*handle }.device.info();
            unsafe {
                output.write(OpenNowStreamerVulkanDeviceInfo {
                    version: OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION,
                    struct_size: size_of::<OpenNowStreamerVulkanDeviceInfo>(),
                    instance: info.instance as *mut c_void,
                    physical_device: info.physical_device as *mut c_void,
                    device: info.device as *mut c_void,
                    graphics_queue: info.graphics_queue as *mut c_void,
                    graphics_queue_family_index: info.graphics_queue_family_index,
                    graphics_queue_index: info.graphics_queue_index,
                    api_version: info.api_version,
                });
            }
            OpenNowStreamerStatus::Ok
        }
        #[cfg(not(target_os = "linux"))]
        OpenNowStreamerStatus::GraphicsUnavailable
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Releases the device owner after the shell has stopped using its native objects.
///
/// # Safety
/// `handle` must be a live handle returned by device creation and consumed exactly once,
/// without concurrent access. The shell must release all adopted graphics resources first.
pub unsafe extern "C" fn opennow_streamer_vulkan_device_destroy(
    handle: *mut OpenNowStreamerVulkanDevice,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        unsafe { drop(Box::from_raw(handle)) };
        OpenNowStreamerStatus::Ok
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenNowStreamerStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidConfig = 2,
    MessageTooLarge = 3,
    QueueFull = 4,
    Closed = 5,
    NoFrame = 6,
    GraphicsUnavailable = 7,
    WrongThread = 8,
    StaleFrame = 9,
    RenderFailed = 10,
    SceneGraphActive = 11,
    FrameAlreadyRecorded = 12,
    Panic = 255,
}

pub const OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION: u32 = 3;
pub const OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT: u32 = 1;
pub const OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT: u32 = 2;
pub const OPENNOW_STREAMER_RENDER_COMMAND_VERSION: u32 = 3;
pub const OPENNOW_STREAMER_GRAPHICS_API_D3D11: u32 = 1;
pub const OPENNOW_STREAMER_GRAPHICS_API_VULKAN: u32 = 2;
pub const OPENNOW_STREAMER_GRAPHICS_API_METAL: u32 = 3;
pub const OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8: u32 = 1;
pub const OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2: u32 = 2;
pub const OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F: u32 = 3;
pub const OPENNOW_STREAMER_COLOR_SPACE_SDR709: u32 = 0;
pub const OPENNOW_STREAMER_COLOR_SPACE_PQ2020: u32 = 1;
pub const OPENNOW_STREAMER_COLOR_SPACE_HLG2020: u32 = 2;
pub const OPENNOW_STREAMER_LOCAL_ACTION_GUIDE: u32 = 1;
pub const OPENNOW_STREAMER_LOCAL_ACTION_SCREENSHOT: u32 = 2;
pub const OPENNOW_STREAMER_LOCAL_ACTION_RECORDING_TOGGLE: u32 = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OpenNowStreamerGraphicsContext {
    pub version: u32,
    pub struct_size: usize,
    pub graphics_api: u32,
    pub instance: *mut c_void,
    pub physical_device: *mut c_void,
    pub device: *mut c_void,
    pub queue: *mut c_void,
    pub queue_family_index: u32,
    pub enabled_capabilities: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OpenNowStreamerRecordCommand {
    pub version: u32,
    pub struct_size: usize,
    pub command_buffer: *mut c_void,
    pub frame_slot: u32,
    pub upscale_width: u32,
    pub upscale_height: u32,
    pub upscale_sharpness: u32,
    pub upscale_denoise: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenNowStreamerFrameInfo {
    pub width: u32,
    pub height: u32,
    pub sequence: u64,
    pub presentation_time_ns: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenNowStreamerRecordedFrame {
    pub resource: u64,
    pub resource_view: u64,
    pub graphics_api: u32,
    pub texture_format: u32,
    pub color_space: u32,
    pub width: u32,
    pub height: u32,
    pub frame_slot: u32,
    pub generation: u64,
    pub presentation_time_ns: u64,
}

pub struct OpenNowStreamerFrame {
    token: GraphicsFrameToken,
}

#[derive(Clone, Copy)]
struct Callback {
    function: OpenNowStreamerCallback,
    user_data: usize,
}

#[derive(Clone, Copy)]
struct FrameAvailableCallback {
    function: OpenNowStreamerFrameAvailableCallback,
    user_data: usize,
}

impl FrameAvailableCallback {
    fn invoke(self) {
        let Some(function) = self.function else {
            return;
        };
        unsafe {
            function(self.user_data as *mut c_void);
        }
    }
}

impl Callback {
    fn invoke(self, value: &Value) {
        let Some(function) = self.function else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(value) else {
            return;
        };
        unsafe {
            function(bytes.as_ptr(), bytes.len(), self.user_data as *mut c_void);
        }
    }

    fn invoke_bytes(self, bytes: &[u8]) {
        let Some(function) = self.function else {
            return;
        };
        unsafe {
            function(bytes.as_ptr(), bytes.len(), self.user_data as *mut c_void);
        }
    }
}

enum WorkerCommand {
    Send(Vec<u8>),
    Destroy,
}

pub struct OpenNowStreamer {
    commands: Option<SyncSender<WorkerCommand>>,
    worker: Option<JoinHandle<()>>,
    response_dispatcher: Option<JoinHandle<()>>,
    event_dispatcher: Option<JoinHandle<()>>,
    cursor_dispatcher: Option<JoinHandle<()>>,
    max_command_bytes: usize,
    graphics: RenderThreadGraphics,
    frame_publisher: GraphicsFramePublisher,
    input: EmbeddedInputCapture,
    #[cfg(target_os = "windows")]
    windows_adapter_luid: Option<WindowsAdapterLuid>,
}

impl OpenNowStreamer {
    fn create(
        config: OpenNowStreamerConfig,
        captured_input: Arc<CapturedInputQueue>,
        engine_factory: impl FnOnce(EventSender, GraphicsFramePublisher, SyncSender<Vec<u8>>) -> Engine
        + Send
        + 'static,
        exit_hook: impl FnOnce() + Send + 'static,
    ) -> Result<Self, OpenNowStreamerStatus> {
        validate_config(&config)?;
        let callback = |function| Callback {
            function,
            user_data: config.user_data as usize,
        };
        let frame_available = FrameAvailableCallback {
            function: config.frame_available_callback,
            user_data: config.user_data as usize,
        };
        let (graphics, frame_publisher) =
            RenderThreadGraphics::new(move || frame_available.invoke());
        let engine_frame_publisher = frame_publisher.clone();
        let (commands, command_receiver) = sync_channel(config.command_queue_capacity);
        let (responses, response_receiver) = sync_channel(config.response_queue_capacity);
        let (events, event_receiver) = sync_channel(config.event_queue_capacity);
        let (cursor_updates, cursor_receiver) = sync_channel(config.event_queue_capacity);
        let response_dispatcher = spawn_dispatcher(
            "opennow-ffi-responses",
            response_receiver,
            callback(config.response_callback),
        )?;
        let event_dispatcher = match spawn_dispatcher(
            "opennow-ffi-events",
            event_receiver,
            callback(config.event_callback),
        ) {
            Ok(dispatcher) => dispatcher,
            Err(status) => {
                drop(responses);
                let _ = response_dispatcher.join();
                return Err(status);
            }
        };
        let cursor_dispatcher =
            match spawn_cursor_dispatcher(cursor_receiver, callback(config.cursor_callback)) {
                Ok(dispatcher) => dispatcher,
                Err(status) => {
                    drop(responses);
                    drop(events);
                    let _ = response_dispatcher.join();
                    let _ = event_dispatcher.join();
                    return Err(status);
                }
            };
        let worker = match thread::Builder::new()
            .name("opennow-ffi-engine".to_owned())
            .spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    run_engine(
                        command_receiver,
                        responses,
                        EventSender::bounded(events),
                        move |events| {
                            engine_factory(events, engine_frame_publisher, cursor_updates)
                        },
                    );
                }));
                exit_hook();
            }) {
            Ok(worker) => worker,
            Err(_) => {
                drop(commands);
                let _ = response_dispatcher.join();
                let _ = event_dispatcher.join();
                let _ = cursor_dispatcher.join();
                return Err(OpenNowStreamerStatus::Closed);
            }
        };
        Ok(Self {
            commands: Some(commands),
            worker: Some(worker),
            response_dispatcher: Some(response_dispatcher),
            event_dispatcher: Some(event_dispatcher),
            cursor_dispatcher: Some(cursor_dispatcher),
            max_command_bytes: if config.max_command_bytes == 0 {
                DEFAULT_MAX_COMMAND_BYTES
            } else {
                config.max_command_bytes
            },
            graphics,
            frame_publisher,
            input: EmbeddedInputCapture::new(captured_input),
            #[cfg(target_os = "windows")]
            windows_adapter_luid: WindowsAdapterLuid::new(config.windows_adapter_luid),
        })
    }

    fn send(&self, bytes: &[u8]) -> OpenNowStreamerStatus {
        if bytes.len() > self.max_command_bytes {
            return OpenNowStreamerStatus::MessageTooLarge;
        }
        let Some(commands) = self.commands.as_ref() else {
            return OpenNowStreamerStatus::Closed;
        };
        match commands.try_send(WorkerCommand::Send(bytes.to_vec())) {
            Ok(()) => OpenNowStreamerStatus::Ok,
            Err(TrySendError::Full(_)) => OpenNowStreamerStatus::QueueFull,
            Err(TrySendError::Disconnected(_)) => OpenNowStreamerStatus::Closed,
        }
    }

    fn shutdown(&mut self) {
        if let Some(commands) = self.commands.take() {
            let _ = commands.send(WorkerCommand::Destroy);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(dispatcher) = self.response_dispatcher.take() {
            let _ = dispatcher.join();
        }
        if let Some(dispatcher) = self.event_dispatcher.take() {
            let _ = dispatcher.join();
        }
        if let Some(dispatcher) = self.cursor_dispatcher.take() {
            let _ = dispatcher.join();
        }
    }

    #[cfg(test)]
    fn frame_publisher(&self) -> GraphicsFramePublisher {
        self.frame_publisher.clone()
    }
}

impl Drop for OpenNowStreamer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn validate_config(config: &OpenNowStreamerConfig) -> Result<(), OpenNowStreamerStatus> {
    let valid_capacity = |capacity| (1..=MAX_QUEUE_CAPACITY).contains(&capacity);
    if config.abi_version != OPENNOW_STREAMER_FFI_ABI_VERSION
        || config.struct_size < size_of::<OpenNowStreamerConfig>()
        || !valid_capacity(config.command_queue_capacity)
        || !valid_capacity(config.response_queue_capacity)
        || !valid_capacity(config.event_queue_capacity)
        || config.max_command_bytes > MAX_COMMAND_BYTES
        || config.response_callback.is_none()
    {
        return Err(OpenNowStreamerStatus::InvalidConfig);
    }
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
fn validate_windows_adapter_luid(
    configured: WindowsAdapterLuid,
    actual: WindowsAdapterLuid,
) -> Result<(), String> {
    if configured == actual {
        return Ok(());
    }
    Err(format!(
        "configured Windows adapter LUID {:#018x} does not match Qt D3D11 device LUID {:#018x}",
        configured.get(),
        actual.get(),
    ))
}

fn spawn_dispatcher(
    name: &str,
    receiver: Receiver<Value>,
    callback: Callback,
) -> Result<JoinHandle<()>, OpenNowStreamerStatus> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let mut last_telemetry = std::time::Instant::now();
            while let Ok(value) = receiver.recv() {
                let kind = value["type"].as_str().unwrap_or("");
                if should_log_event(kind, &mut last_telemetry, std::time::Instant::now()) {
                    log::log_line(
                        match (kind, value["level"].as_str()) {
                            ("error", _) | (_, Some("error")) => "ERROR",
                            (_, Some("warn")) => "WARN",
                            (_, Some("debug")) => "DEBUG",
                            _ => "INFO",
                        },
                        "native-to-qt",
                        &log::message_summary(&value),
                    );
                }
                callback.invoke(&value);
            }
        })
        .map_err(|_| OpenNowStreamerStatus::Closed)
}

fn should_log_event(
    kind: &str,
    last_telemetry: &mut std::time::Instant,
    now: std::time::Instant,
) -> bool {
    if kind == "controller-rumble" {
        return false;
    }
    if !matches!(kind, "telemetry" | "stats") {
        return true;
    }
    if now.duration_since(*last_telemetry) < std::time::Duration::from_secs(10) {
        return false;
    }
    *last_telemetry = now;
    true
}

fn spawn_cursor_dispatcher(
    receiver: Receiver<Vec<u8>>,
    callback: Callback,
) -> Result<JoinHandle<()>, OpenNowStreamerStatus> {
    thread::Builder::new()
        .name("opennow-ffi-cursor".to_owned())
        .spawn(move || {
            while let Ok(bytes) = receiver.recv() {
                callback.invoke_bytes(&bytes);
            }
        })
        .map_err(|_| OpenNowStreamerStatus::Closed)
}

fn run_engine(
    commands: Receiver<WorkerCommand>,
    responses: SyncSender<Value>,
    events: EventSender,
    engine_factory: impl FnOnce(EventSender) -> Engine,
) {
    let mut engine = engine_factory(events);
    while let Ok(command) = commands.recv() {
        let bytes = match command {
            WorkerCommand::Send(bytes) => bytes,
            WorkerCommand::Destroy => break,
        };
        let command: Command = match serde_json::from_slice(&bytes) {
            Ok(command) => command,
            Err(parse_error) => {
                if responses
                    .send(error(None, "invalid-command", parse_error.to_string()))
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let (messages, keep_running) = engine.handle(command);
        for message in messages {
            if responses.send(message).is_err() {
                return;
            }
        }
        if !keep_running {
            break;
        }
    }
}

fn graphics_context(
    context: OpenNowStreamerGraphicsContext,
) -> Result<GraphicsContext, OpenNowStreamerStatus> {
    if context.version != OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION
        || context.struct_size < size_of::<OpenNowStreamerGraphicsContext>()
    {
        return Err(OpenNowStreamerStatus::InvalidConfig);
    }
    let api = match context.graphics_api {
        OPENNOW_STREAMER_GRAPHICS_API_D3D11 => GraphicsApi::D3d11,
        OPENNOW_STREAMER_GRAPHICS_API_VULKAN => GraphicsApi::Vulkan,
        OPENNOW_STREAMER_GRAPHICS_API_METAL => GraphicsApi::Metal,
        _ => return Err(OpenNowStreamerStatus::InvalidConfig),
    };
    let supported_capabilities = OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT
        | OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT;
    if context.enabled_capabilities & !supported_capabilities != 0
        || (api != GraphicsApi::Vulkan && context.enabled_capabilities != 0)
    {
        return Err(OpenNowStreamerStatus::InvalidConfig);
    }
    Ok(GraphicsContext {
        api,
        instance: context.instance as usize,
        physical_device: context.physical_device as usize,
        device: context.device as usize,
        queue: context.queue as usize,
        queue_family_index: context.queue_family_index,
        vulkan_dmabuf_import_enabled: context.enabled_capabilities
            & OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT
            != 0,
        vulkan_dmabuf_buffer_import_enabled: context.enabled_capabilities
            & OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT
            != 0,
    })
}

fn render_command(
    command: OpenNowStreamerRecordCommand,
) -> Result<GraphicsRecordCommand, OpenNowStreamerStatus> {
    if command.version != OPENNOW_STREAMER_RENDER_COMMAND_VERSION
        || command.struct_size < size_of::<OpenNowStreamerRecordCommand>()
        || command.upscale_sharpness > 15
        || command.upscale_denoise > 20
    {
        return Err(OpenNowStreamerStatus::InvalidConfig);
    }
    Ok(GraphicsRecordCommand {
        command_buffer: command.command_buffer as usize,
        frame_slot: command.frame_slot,
        upscale_width: command.upscale_width,
        upscale_height: command.upscale_height,
        upscale_sharpness: command.upscale_sharpness,
        upscale_denoise: command.upscale_denoise,
    })
}

fn recorded_frame(
    api: GraphicsApi,
    frame: GraphicsRecordedFrame,
) -> Result<OpenNowStreamerRecordedFrame, OpenNowStreamerStatus> {
    if frame.resource == 0 || frame.width == 0 || frame.height == 0 {
        return Err(OpenNowStreamerStatus::RenderFailed);
    }
    if frame.color_space != opennow_streamer_platform::GraphicsColorSpace::Sdr709
        && frame.texture_format == GraphicsTextureFormat::Rgba8
    {
        return Err(OpenNowStreamerStatus::RenderFailed);
    }
    Ok(OpenNowStreamerRecordedFrame {
        resource: frame.resource,
        resource_view: frame.resource_view,
        graphics_api: match api {
            GraphicsApi::D3d11 => OPENNOW_STREAMER_GRAPHICS_API_D3D11,
            GraphicsApi::Vulkan => OPENNOW_STREAMER_GRAPHICS_API_VULKAN,
            GraphicsApi::Metal => OPENNOW_STREAMER_GRAPHICS_API_METAL,
        },
        texture_format: match frame.texture_format {
            GraphicsTextureFormat::Rgba8 => OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8,
            GraphicsTextureFormat::Rgb10A2 => OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2,
            GraphicsTextureFormat::Rgba16Float => OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F,
        },
        color_space: match frame.color_space {
            opennow_streamer_platform::GraphicsColorSpace::Sdr709 => {
                OPENNOW_STREAMER_COLOR_SPACE_SDR709
            }
            opennow_streamer_platform::GraphicsColorSpace::Pq2020 => {
                OPENNOW_STREAMER_COLOR_SPACE_PQ2020
            }
            opennow_streamer_platform::GraphicsColorSpace::Hlg2020 => {
                OPENNOW_STREAMER_COLOR_SPACE_HLG2020
            }
        },
        width: frame.width,
        height: frame.height,
        frame_slot: frame.frame_slot,
        generation: frame.generation,
        presentation_time_ns: frame.presentation_time_ns,
    })
}

fn graphics_status(error: GraphicsRuntimeError) -> OpenNowStreamerStatus {
    match error {
        GraphicsRuntimeError::InvalidContext(_) | GraphicsRuntimeError::InvalidRenderCommand(_) => {
            OpenNowStreamerStatus::InvalidConfig
        }
        GraphicsRuntimeError::SceneGraphInactive => OpenNowStreamerStatus::GraphicsUnavailable,
        GraphicsRuntimeError::WrongThread => OpenNowStreamerStatus::WrongThread,
        GraphicsRuntimeError::NoFrame => OpenNowStreamerStatus::NoFrame,
        GraphicsRuntimeError::StaleFrame => OpenNowStreamerStatus::StaleFrame,
        GraphicsRuntimeError::FrameAlreadyRecorded => OpenNowStreamerStatus::FrameAlreadyRecorded,
        GraphicsRuntimeError::RecordFailed(_) => OpenNowStreamerStatus::RenderFailed,
    }
}

fn graphics_api_name(api: u32) -> &'static str {
    match api {
        OPENNOW_STREAMER_GRAPHICS_API_D3D11 => "d3d11",
        OPENNOW_STREAMER_GRAPHICS_API_VULKAN => "vulkan",
        OPENNOW_STREAMER_GRAPHICS_API_METAL => "metal",
        _ => "unknown",
    }
}

/// Best-effort command-type label for the log. Payloads may carry session
/// secrets, so only the `type` discriminator is ever extracted.
fn command_kind_label(bytes: &[u8]) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| value.get("type")?.as_str().map(str::to_owned))
        .map(|kind| kind.chars().take(64).collect())
        .unwrap_or_else(|| "<unparsed>".to_owned())
}

fn ffi_status(body: impl FnOnce() -> OpenNowStreamerStatus) -> OpenNowStreamerStatus {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(OpenNowStreamerStatus::Panic)
}

#[unsafe(no_mangle)]
/// Points the embedded file log at `path` (UTF-8, NUL-terminated), creating
/// parent directories and rotating a previous log over 2 MiB to
/// `<path>.previous`. The Qt shell passes its diagnostics
/// `native-streamer.log` here at startup so packaged builds — which never
/// spawn the legacy child streamer — still produce video-pipeline logs.
/// Safe to call more than once; logging never affects streaming results.
///
/// # Safety
///
/// `path` must point to readable NUL-terminated bytes for this call.
pub unsafe extern "C" fn opennow_streamer_set_log_file(
    path: *const c_char,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if path.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        let path = unsafe { CStr::from_ptr(path) };
        let path = match path.to_str() {
            Ok(path) => path,
            Err(_) => return OpenNowStreamerStatus::InvalidConfig,
        };
        match log::set_log_file(path) {
            Ok(()) => {
                log::log_line("INFO", "engine", "file log configured");
                OpenNowStreamerStatus::Ok
            }
            Err(reason) => {
                log::log_line("WARN", "engine", &format!("file log unavailable: {reason}"));
                OpenNowStreamerStatus::InvalidConfig
            }
        }
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Creates one engine handle owned by the caller.
///
/// # Safety
///
/// `config` must point to a readable configuration whose first `struct_size` bytes remain valid
/// for this call. `output` must point to writable storage for one handle pointer. Callback pointers
/// and `user_data` must remain valid until destroy returns. A non-null `vulkan_device` must be
/// live throughout this call; the engine retains its own reference before the worker starts.
pub unsafe extern "C" fn opennow_streamer_create(
    config: *const OpenNowStreamerConfig,
    output: *mut *mut OpenNowStreamer,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if config.is_null() || output.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        unsafe {
            output.write(ptr::null_mut());
        }
        let abi_version = unsafe { ptr::addr_of!((*config).abi_version).read() };
        let struct_size = unsafe { ptr::addr_of!((*config).struct_size).read() };
        if abi_version != OPENNOW_STREAMER_FFI_ABI_VERSION
            || struct_size < size_of::<OpenNowStreamerConfig>()
        {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        let config = unsafe { config.read() };
        let vulkan_device =
            unsafe { config.vulkan_device.as_ref() }.map(|owner| Arc::clone(&owner.device));
        let windows_adapter_luid = WindowsAdapterLuid::new(config.windows_adapter_luid);
        let captured_input = Arc::new(CapturedInputQueue::default());
        let runtime_input = Arc::clone(&captured_input);
        match OpenNowStreamer::create(
            config,
            captured_input,
            move |events, frames, cursor_updates| {
                let cursor_update = Arc::new(move |bytes: Vec<u8>| {
                    let _ = cursor_updates.try_send(bytes);
                });
                Engine::with_embedded_media_runtime(
                    events,
                    create_embedded_runtime_with_config(
                        frames,
                        runtime_input,
                        Some(cursor_update),
                        EmbeddedRuntimeConfig {
                            vulkan_device,
                            windows_adapter_luid,
                        },
                    ),
                )
            },
            || {},
        ) {
            Ok(handle) => {
                unsafe {
                    output.write(Box::into_raw(Box::new(handle)));
                }
                log::log_line(
                    "INFO",
                    "engine",
                    &format!(
                        "engine created (abi={abi_version} cmdq={} rspq={} evtq={} max_bytes={})",
                        config.command_queue_capacity,
                        config.response_queue_capacity,
                        config.event_queue_capacity,
                        config.max_command_bytes,
                    ),
                );
                OpenNowStreamerStatus::Ok
            }
            Err(status) => {
                log::log_line(
                    "WARN",
                    "engine",
                    &format!("engine create failed: {status:?}"),
                );
                status
            }
        }
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Atomically admits one nonempty UTF-8 paste to the bounded captured input queue.
/// Success acknowledges local admission, not remote delivery.
///
/// # Safety
///
/// `handle` must be null or point to a live handle not being destroyed.
/// `text` must be null or readable for `text_len` bytes for the duration of the call.
pub unsafe extern "C" fn opennow_streamer_submit_text(
    handle: *const OpenNowStreamer,
    text: *const u8,
    text_len: usize,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        use opennow_streamer_protocol::text_input::TextInputError;
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        if text_len > OPENNOW_STREAMER_MAX_TEXT_BYTES {
            return OpenNowStreamerStatus::MessageTooLarge;
        }
        if text.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        if text_len == 0 {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        let bytes = unsafe { std::slice::from_raw_parts(text, text_len) };
        if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        match handle.input.submit_text(bytes) {
            Ok(()) => OpenNowStreamerStatus::Ok,
            Err(TextInputError::Invalid) => OpenNowStreamerStatus::InvalidConfig,
            Err(TextInputError::TooLarge) => OpenNowStreamerStatus::MessageTooLarge,
            Err(TextInputError::Busy) => OpenNowStreamerStatus::QueueFull,
            Err(TextInputError::Unavailable) => OpenNowStreamerStatus::Closed,
        }
    })
}

#[unsafe(no_mangle)]
/// Submits one keyboard transition to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_key(
    handle: *const OpenNowStreamer,
    virtual_key: u16,
    modifiers: u16,
    pressed: bool,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        handle.input.submit(CapturedInput::Key {
            virtual_key,
            modifiers,
            pressed,
        });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one relative mouse movement to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_mouse_relative(
    handle: *const OpenNowStreamer,
    delta_x: i16,
    delta_y: i16,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        handle
            .input
            .submit(CapturedInput::MouseMove { delta_x, delta_y });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one absolute mouse position to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_mouse_absolute(
    handle: *const OpenNowStreamer,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        if width == 0 || height == 0 {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        handle.input.submit(CapturedInput::MouseAbsolute {
            x,
            y,
            width,
            height,
        });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one mouse-button transition to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_mouse_button(
    handle: *const OpenNowStreamer,
    button: u8,
    pressed: bool,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        if !(1..=5).contains(&button) {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        handle
            .input
            .submit(CapturedInput::MouseButton { button, pressed });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one mouse-wheel movement to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_mouse_wheel(
    handle: *const OpenNowStreamer,
    delta_x: i16,
    delta_y: i16,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        handle
            .input
            .submit(CapturedInput::MouseWheel { delta_x, delta_y });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one gamepad state snapshot to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_gamepad(
    handle: *const OpenNowStreamer,
    controller_id: u8,
    bitmap: u16,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_stick_x: i16,
    left_stick_y: i16,
    right_stick_x: i16,
    right_stick_y: i16,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        if controller_id >= 4 {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        handle.input.submit(CapturedInput::Gamepad {
            controller_id,
            bitmap,
            buttons,
            left_trigger,
            right_trigger,
            left_stick_x,
            left_stick_y,
            right_stick_x,
            right_stick_y,
        });
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Submits one shell-local action to the embedded input queue.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
pub unsafe extern "C" fn opennow_streamer_submit_local_action(
    handle: *const OpenNowStreamer,
    action: u32,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        let Some(handle) = (unsafe { handle.as_ref() }) else {
            return OpenNowStreamerStatus::NullPointer;
        };
        let action = match action {
            OPENNOW_STREAMER_LOCAL_ACTION_GUIDE => EmbeddedLocalAction::Guide,
            OPENNOW_STREAMER_LOCAL_ACTION_SCREENSHOT => EmbeddedLocalAction::Screenshot,
            OPENNOW_STREAMER_LOCAL_ACTION_RECORDING_TOGGLE => EmbeddedLocalAction::RecordingToggle,
            _ => return OpenNowStreamerStatus::InvalidConfig,
        };
        handle.input.submit_local_action(action);
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Changes native input capture state for the Qt window.
///
/// # Safety
///
/// `handle` must be null or point to a live engine handle that is not being destroyed.
/// `raw_input_active` must be null or point to writable storage for one `bool`.
pub unsafe extern "C" fn opennow_streamer_set_capture_active(
    handle: *const OpenNowStreamer,
    active: bool,
    relative_mouse: bool,
    window_handle: usize,
    raw_input_active: *mut bool,
) -> OpenNowStreamerStatus {
    ffi_status(|| {
        if handle.is_null() || raw_input_active.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        let raw = unsafe { &*handle }
            .input
            .set_active(active, relative_mouse, window_handle);
        unsafe {
            raw_input_active.write(raw);
        }
        OpenNowStreamerStatus::Ok
    })
}

#[unsafe(no_mangle)]
/// Binds the caller's live graphics objects to the current scene-graph render thread.
///
/// Repeating the call with an identical context is idempotent. A changed context invalidates any
/// queued or acquired frame from the previous context.
///
/// # Safety
///
/// `handle` must be live and must not race with destroy. `context` must point to a readable
/// versioned context. Its native graphics objects must remain live until scene-graph shutdown,
/// and this call must run on the thread that will acquire and record frames.
pub unsafe extern "C" fn opennow_streamer_set_graphics_context(
    handle: *const OpenNowStreamer,
    context: *const OpenNowStreamerGraphicsContext,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() || context.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        let version = unsafe { ptr::addr_of!((*context).version).read() };
        let struct_size = unsafe { ptr::addr_of!((*context).struct_size).read() };
        if version != OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION
            || struct_size < size_of::<OpenNowStreamerGraphicsContext>()
        {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        let api_name = graphics_api_name(unsafe { ptr::addr_of!((*context).graphics_api).read() });
        let context = match graphics_context(unsafe { context.read() }) {
            Ok(context) => context,
            Err(status) => {
                log::log_line(
                    "WARN",
                    "graphics",
                    &format!("graphics context rejected (api={api_name}): {status:?}"),
                );
                return status;
            }
        };
        let handle = unsafe { &*handle };
        #[cfg(target_os = "windows")]
        if context.api == GraphicsApi::D3d11
            && let Some(configured) = handle.windows_adapter_luid
        {
            let validation = unsafe {
                opennow_streamer_platform::d3d11_adapter_luid(context.device as *mut c_void)
            }
            .and_then(|actual| validate_windows_adapter_luid(configured, actual));
            if let Err(error) = validation {
                log::log_line(
                    "WARN",
                    "graphics",
                    &format!("graphics context rejected: {error}"),
                );
                return OpenNowStreamerStatus::GraphicsUnavailable;
            }
        }
        let status = handle
            .graphics
            .initialize(context)
            .map_or_else(graphics_status, |()| OpenNowStreamerStatus::Ok);
        log::log_line(
            if status == OpenNowStreamerStatus::Ok {
                "INFO"
            } else {
                "WARN"
            },
            "graphics",
            &format!("graphics context initialized (api={api_name}): {status:?}"),
        );
        status
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Acquires the newest pending GPU frame and transfers one retained reference to the caller.
///
/// # Safety
///
/// `handle` must be live. `output` and `info` must point to writable storage. The call must run on
/// the bound render thread. A successful token must be released exactly once.
pub unsafe extern "C" fn opennow_streamer_acquire_latest_frame(
    handle: *const OpenNowStreamer,
    output: *mut *mut OpenNowStreamerFrame,
    info: *mut OpenNowStreamerFrameInfo,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() || output.is_null() || info.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        unsafe {
            output.write(ptr::null_mut());
            info.write(OpenNowStreamerFrameInfo::default());
        }
        let token = match unsafe { &*handle }.graphics.acquire_latest() {
            Ok(token) => token,
            Err(error) => {
                let status = graphics_status(error);
                if status != OpenNowStreamerStatus::NoFrame {
                    log::log_throttled(
                        "acquire-latest-frame",
                        "WARN",
                        "present",
                        &format!("acquire latest frame failed: {status:?}"),
                    );
                }
                return status;
            }
        };
        let frame_info = token.info();
        if !FIRST_FRAME_LOGGED.swap(true, Ordering::Relaxed) {
            log::log_line(
                "INFO",
                "present",
                &format!(
                    "first frame acquired ({}x{} seq={})",
                    frame_info.width, frame_info.height, frame_info.sequence,
                ),
            );
        }
        unsafe {
            info.write(OpenNowStreamerFrameInfo {
                width: frame_info.width,
                height: frame_info.height,
                sequence: frame_info.sequence,
                presentation_time_ns: frame_info.presentation_time_ns,
            });
            output.write(Box::into_raw(Box::new(OpenNowStreamerFrame { token })));
        }
        OpenNowStreamerStatus::Ok
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Records the acquired frame into the caller's current GPU command stream.
///
/// # Safety
///
/// `handle` and `frame` must be live and belong to the same streamer. `command` must describe a
/// writable command buffer owned by the bound graphics context. This call must happen before the
/// QQuickRhiItem render pass begins. `output` receives textures that can then be sampled inside the
/// item pass. All native objects must remain valid for the call, which must run on the bound render
/// thread.
pub unsafe extern "C" fn opennow_streamer_record_frame(
    handle: *const OpenNowStreamer,
    frame: *const OpenNowStreamerFrame,
    command: *const OpenNowStreamerRecordCommand,
    output: *mut OpenNowStreamerRecordedFrame,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() || frame.is_null() || command.is_null() || output.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        unsafe {
            output.write(OpenNowStreamerRecordedFrame::default());
        }
        let version = unsafe { ptr::addr_of!((*command).version).read() };
        let struct_size = unsafe { ptr::addr_of!((*command).struct_size).read() };
        if version != OPENNOW_STREAMER_RENDER_COMMAND_VERSION
            || struct_size < size_of::<OpenNowStreamerRecordCommand>()
        {
            return OpenNowStreamerStatus::InvalidConfig;
        }
        let command = match render_command(unsafe { command.read() }) {
            Ok(command) => command,
            Err(status) => return status,
        };
        let handle = unsafe { &*handle };
        let recorded = match handle.graphics.record(&unsafe { &*frame }.token, command) {
            Ok(recorded) => recorded,
            Err(error) => {
                // Expected during asynchronous decoder bootstrap. Qt keeps the
                // current surface and waits for the decoder's frame-ready event.
                if error == GraphicsRuntimeError::NoFrame {
                    log::log_throttled(
                        "record-not-ready",
                        "INFO",
                        "present",
                        "decoder output pending; waiting for frame-ready",
                    );
                    return OpenNowStreamerStatus::NoFrame;
                }
                // Preserve the platform failure before collapsing it to an ABI
                // status. Graphics errors contain local stage/device details,
                // not session context; bound the diagnostic on the render path.
                let detail: String = error.to_string().chars().take(512).collect();
                let status = graphics_status(error);
                log::log_throttled(
                    "record-frame",
                    "WARN",
                    "present",
                    &format!("record frame failed: {status:?} detail={detail}"),
                );
                return status;
            }
        };
        let api = match handle.frame_publisher.context() {
            Some(lease) => lease.context().api,
            None => return OpenNowStreamerStatus::GraphicsUnavailable,
        };
        let recorded = match recorded_frame(api, recorded) {
            Ok(recorded) => recorded,
            Err(status) => return status,
        };
        unsafe {
            output.write(recorded);
        }
        OpenNowStreamerStatus::Ok
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Releases one acquired frame token.
///
/// # Safety
///
/// `frame` must be a live token returned by acquire and must be released exactly once. Release may
/// run on any thread and remains valid after scene-graph shutdown.
pub unsafe extern "C" fn opennow_streamer_release_frame(
    frame: *mut OpenNowStreamerFrame,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if frame.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        drop(unsafe { Box::from_raw(frame) });
        OpenNowStreamerStatus::Ok
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Invalidates queued frames and releases all scene-graph-owned graphics state.
///
/// Acquired tokens remain releasable but cannot be recorded. The handle can later bind a new
/// context, including from a replacement render thread.
///
/// # Safety
///
/// `handle` must be live and the call must run on the currently bound render thread. It must not
/// race with another graphics operation or destroy. Submit and drain commands and release host
/// texture wrappers first, while the borrowed device is still live. RenderFailed still invalidates
/// the graphics context; WrongThread does not.
pub unsafe extern "C" fn opennow_streamer_scene_graph_shutdown(
    handle: *const OpenNowStreamer,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        let status = unsafe { &*handle }
            .graphics
            .shutdown()
            .map_or_else(graphics_status, |()| OpenNowStreamerStatus::Ok);
        FIRST_FRAME_LOGGED.store(false, Ordering::Relaxed);
        log::log_line(
            "INFO",
            "graphics",
            &format!("scene graph shutdown: {status:?}"),
        );
        status
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Copies and queues one serialized protocol command.
///
/// # Safety
///
/// `handle` must be a live handle returned by create and must not race with destroy. When `length`
/// is nonzero, `bytes` must identify a readable allocation of at least `length` bytes.
pub unsafe extern "C" fn opennow_streamer_send(
    handle: *const OpenNowStreamer,
    bytes: *const u8,
    length: usize,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() || (bytes.is_null() && length != 0) {
            return OpenNowStreamerStatus::NullPointer;
        }
        let handle = unsafe { &*handle };
        let bytes = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytes, length) }
        };
        let summary = serde_json::from_slice::<Value>(bytes)
            .map(|value| log::message_summary(&value))
            .unwrap_or_else(|_| command_kind_label(bytes));
        let status = handle.send(bytes);
        log::log_line(
            "INFO",
            "qt-to-native",
            &format!("{summary} bytes={length} queue_result={status:?}"),
        );
        status
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[unsafe(no_mangle)]
/// Stops the engine, drains callbacks, and consumes the handle.
///
/// # Safety
///
/// `handle` must be a live handle returned by create. It must be passed to destroy exactly once,
/// with no concurrent or subsequent access through the pointer.
pub unsafe extern "C" fn opennow_streamer_destroy(
    handle: *mut OpenNowStreamer,
) -> OpenNowStreamerStatus {
    match catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return OpenNowStreamerStatus::NullPointer;
        }
        if unsafe { &*handle }.graphics.is_active() {
            return OpenNowStreamerStatus::SceneGraphActive;
        }
        let mut handle = unsafe { Box::from_raw(handle) };
        handle.shutdown();
        FIRST_FRAME_LOGGED.store(false, Ordering::Relaxed);
        log::log_line("INFO", "engine", "engine destroyed");
        OpenNowStreamerStatus::Ok
    })) {
        Ok(status) => status,
        Err(_) => OpenNowStreamerStatus::Panic,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    use opennow_streamer_platform::{GraphicsFrame, GraphicsFrameInfo, create_test_runtime};
    use serde_json::json;

    use super::*;

    #[test]
    fn logging_throttles_only_periodic_snapshots_not_drop_deltas_or_warnings() {
        let now = Instant::now();
        let mut last_telemetry = now;
        for seconds in 0..10 {
            let current = now + Duration::from_secs(seconds);
            assert!(!should_log_event("telemetry", &mut last_telemetry, current));
            assert!(!should_log_event("stats", &mut last_telemetry, current));
            for kind in ["log", "error", "status"] {
                assert!(should_log_event(kind, &mut last_telemetry, current));
            }
            assert_eq!(last_telemetry, now);
        }
        assert!(should_log_event(
            "telemetry",
            &mut last_telemetry,
            now + Duration::from_secs(10)
        ));
        assert_eq!(last_telemetry, now + Duration::from_secs(10));
    }

    #[test]
    fn hdr_recorded_frames_preserve_color_space_and_precision() {
        use opennow_streamer_platform::GraphicsColorSpace;
        for (color_space, wire_color) in [
            (
                GraphicsColorSpace::Sdr709,
                OPENNOW_STREAMER_COLOR_SPACE_SDR709,
            ),
            (
                GraphicsColorSpace::Pq2020,
                OPENNOW_STREAMER_COLOR_SPACE_PQ2020,
            ),
            (
                GraphicsColorSpace::Hlg2020,
                OPENNOW_STREAMER_COLOR_SPACE_HLG2020,
            ),
        ] {
            for (texture_format, wire_format) in [
                (
                    GraphicsTextureFormat::Rgba8,
                    OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8,
                ),
                (
                    GraphicsTextureFormat::Rgb10A2,
                    OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2,
                ),
                (
                    GraphicsTextureFormat::Rgba16Float,
                    OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA16F,
                ),
            ] {
                let result = recorded_frame(
                    GraphicsApi::Vulkan,
                    GraphicsRecordedFrame {
                        resource: 1,
                        resource_view: 2,
                        texture_format,
                        color_space,
                        width: 1920,
                        height: 1080,
                        frame_slot: 0,
                        generation: 7,
                        presentation_time_ns: 123456789,
                    },
                );
                if color_space != GraphicsColorSpace::Sdr709
                    && texture_format == GraphicsTextureFormat::Rgba8
                {
                    assert_eq!(result, Err(OpenNowStreamerStatus::RenderFailed));
                } else {
                    let frame = result.expect("supported color texture");
                    assert_eq!(frame.color_space, wire_color);
                    assert_eq!(frame.texture_format, wire_format);
                    assert_eq!(frame.generation, 7);
                    assert_eq!(frame.presentation_time_ns, 123456789);
                }
            }
        }
    }

    #[test]
    fn hdr_abi_rejects_pre_color_metadata_clients() {
        let messages = CallbackMessages::default();
        let mut config = test_config(&messages);
        config.abi_version = 5;
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        assert_eq!(
            std::mem::offset_of!(OpenNowStreamerRecordedFrame, color_space),
            std::mem::offset_of!(OpenNowStreamerRecordedFrame, texture_format) + size_of::<u32>()
        );
    }

    #[test]
    fn abi_five_appends_the_shared_vulkan_owner() {
        assert_eq!(OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION, 1);
        assert_eq!(
            std::mem::offset_of!(OpenNowStreamerConfig, vulkan_device),
            std::mem::offset_of!(OpenNowStreamerConfig, user_data) + size_of::<*mut c_void>(),
        );
        assert_eq!(
            std::mem::offset_of!(OpenNowStreamerVulkanDeviceInfo, physical_device),
            std::mem::offset_of!(OpenNowStreamerVulkanDeviceInfo, instance)
                + size_of::<*mut c_void>(),
        );
        assert_eq!(
            std::mem::offset_of!(OpenNowStreamerVulkanDeviceInfo, graphics_queue_index),
            std::mem::offset_of!(OpenNowStreamerVulkanDeviceInfo, graphics_queue_family_index)
                + size_of::<u32>(),
        );
        let messages = CallbackMessages::default();
        let mut config = test_config(&messages);
        config.abi_version = 4;
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        config.abi_version = OPENNOW_STREAMER_FFI_ABI_VERSION;
        config.struct_size = std::mem::offset_of!(OpenNowStreamerConfig, vulkan_device);
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
    }

    #[test]
    fn abi_ten_appends_the_windows_adapter_luid() {
        assert_eq!(OPENNOW_STREAMER_FFI_ABI_VERSION, 10);
        assert_eq!(
            std::mem::offset_of!(OpenNowStreamerConfig, windows_adapter_luid),
            std::mem::offset_of!(OpenNowStreamerConfig, vulkan_device)
                + size_of::<*const OpenNowStreamerVulkanDevice>(),
        );
        let messages = CallbackMessages::default();
        let mut config = test_config(&messages);
        config.struct_size = std::mem::offset_of!(OpenNowStreamerConfig, windows_adapter_luid);
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        config.struct_size = size_of::<OpenNowStreamerConfig>();
        config.windows_adapter_luid = 0;
        assert_eq!(WindowsAdapterLuid::new(config.windows_adapter_luid), None);
        config.windows_adapter_luid = 0x1122_3344_ffff_fffe;
        assert_eq!(
            WindowsAdapterLuid::new(config.windows_adapter_luid).map(WindowsAdapterLuid::get),
            Some(0x1122_3344_ffff_fffe)
        );
    }

    #[test]
    fn configured_windows_adapter_must_match_the_adopted_device() {
        let configured = WindowsAdapterLuid::new(0x0000_0001_1122_3344).expect("configured LUID");
        assert_eq!(
            validate_windows_adapter_luid(configured, configured),
            Ok(())
        );
        let actual = WindowsAdapterLuid::new(0x0000_0002_5566_7788).expect("actual LUID");
        assert_eq!(
            validate_windows_adapter_luid(configured, actual),
            Err("configured Windows adapter LUID 0x0000000111223344 does not match Qt D3D11 device LUID 0x0000000255667788".to_owned())
        );
    }

    #[test]
    fn vulkan_owner_exports_reject_null_pointers() {
        assert_eq!(
            unsafe { opennow_streamer_vulkan_device_create(ptr::null_mut()) },
            OpenNowStreamerStatus::NullPointer
        );
        assert_eq!(
            unsafe { opennow_streamer_vulkan_device_info(ptr::null(), ptr::null_mut()) },
            OpenNowStreamerStatus::NullPointer
        );
        assert_eq!(
            unsafe { opennow_streamer_vulkan_device_destroy(ptr::null_mut()) },
            OpenNowStreamerStatus::NullPointer
        );
    }

    #[cfg(not(all(
        target_os = "linux",
        any(feature = "linux-ffmpeg", feature = "linux-ffmpeg-bundled")
    )))]
    #[test]
    fn unsupported_vulkan_creation_clears_the_output() {
        let mut output = ptr::dangling_mut::<OpenNowStreamerVulkanDevice>();
        assert_eq!(
            unsafe { opennow_streamer_vulkan_device_create(&mut output) },
            OpenNowStreamerStatus::GraphicsUnavailable
        );
        assert!(output.is_null());
    }

    #[derive(Default)]
    struct CallbackMessages {
        values: Mutex<Vec<Value>>,
        changed: Condvar,
        frames_available: AtomicUsize,
    }

    impl CallbackMessages {
        fn wait_for_id(&self, id: &str) -> Value {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut values = self.values.lock().expect("callback values");
            loop {
                if let Some(value) = values.iter().find(|value| value["id"] == id) {
                    return value.clone();
                }
                let timeout = deadline.saturating_duration_since(Instant::now());
                assert!(!timeout.is_zero(), "timed out waiting for response {id}");
                let (next, result) = self
                    .changed
                    .wait_timeout(values, timeout)
                    .expect("callback wait");
                values = next;
                assert!(!result.timed_out(), "timed out waiting for response {id}");
            }
        }
    }

    unsafe extern "C" fn collect_response(bytes: *const u8, length: usize, user_data: *mut c_void) {
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
        let Ok(value) = serde_json::from_slice(bytes) else {
            return;
        };
        let messages = unsafe { &*(user_data as *const CallbackMessages) };
        messages.values.lock().expect("callback values").push(value);
        messages.changed.notify_all();
    }

    unsafe extern "C" fn collect_frame_available(user_data: *mut c_void) {
        let messages = unsafe { &*(user_data as *const CallbackMessages) };
        messages.frames_available.fetch_add(1, Ordering::Relaxed);
    }

    fn test_config(messages: &CallbackMessages) -> OpenNowStreamerConfig {
        OpenNowStreamerConfig {
            abi_version: OPENNOW_STREAMER_FFI_ABI_VERSION,
            struct_size: size_of::<OpenNowStreamerConfig>(),
            command_queue_capacity: 4,
            response_queue_capacity: 4,
            event_queue_capacity: 4,
            max_command_bytes: 4096,
            response_callback: Some(collect_response),
            event_callback: None,
            frame_available_callback: Some(collect_frame_available),
            cursor_callback: None,
            user_data: ptr::from_ref(messages).cast_mut().cast(),
            vulkan_device: ptr::null(),
            windows_adapter_luid: 0,
        }
    }

    #[test]
    fn controller_rumble_crosses_the_event_callback_without_per_packet_logging() {
        let messages = CallbackMessages::default();
        let (sender, receiver) = sync_channel(1);
        let dispatcher = spawn_dispatcher(
            "test-rumble-events",
            receiver,
            Callback {
                function: Some(collect_response),
                user_data: ptr::from_ref(&messages) as usize,
            },
        )
        .unwrap();
        let event = json!({"type":"controller-rumble", "startId":"start-7",
            "controllerId":3, "lowFrequency":65535, "highFrequency":12345, "durationMs":65535});
        sender.send(event.clone()).unwrap();
        drop(sender);
        dispatcher.join().unwrap();
        assert_eq!(*messages.values.lock().unwrap(), vec![event]);
        let now = Instant::now();
        let mut last = now;
        assert!(!should_log_event("controller-rumble", &mut last, now));
    }

    fn create_with_test_runtime(
        messages: &CallbackMessages,
    ) -> (
        OpenNowStreamer,
        opennow_streamer_platform::TestMediaRuntimeHost,
    ) {
        let (host, runtime) = create_test_runtime();
        let shutdown_runtime = runtime.clone();
        let handle = OpenNowStreamer::create(
            test_config(messages),
            runtime.captured_input(),
            move |events, _frames, _cursor| Engine::with_embedded_media_runtime(events, runtime),
            move || shutdown_runtime.shutdown(),
        )
        .expect("FFI handle");
        (handle, host)
    }

    #[derive(Default)]
    struct RecordedCommands {
        values: Mutex<Vec<(GraphicsContext, GraphicsRecordCommand)>>,
        drops: AtomicUsize,
    }

    struct TestGraphicsFrame {
        sequence: u64,
        recorded: Arc<RecordedCommands>,
        panic_on_record: bool,
        error: Option<opennow_streamer_platform::GraphicsFrameError>,
    }

    impl GraphicsFrame for TestGraphicsFrame {
        fn info(&self) -> GraphicsFrameInfo {
            GraphicsFrameInfo {
                width: 2560,
                height: 1440,
                sequence: self.sequence,
                presentation_time_ns: 8_333_333 * self.sequence,
            }
        }

        fn record(
            &self,
            context: GraphicsContext,
            command: GraphicsRecordCommand,
        ) -> Result<GraphicsRecordedFrame, opennow_streamer_platform::GraphicsFrameError> {
            assert!(!self.panic_on_record, "injected render panic");
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            self.recorded
                .values
                .lock()
                .expect("recorded commands")
                .push((context, command));
            Ok(GraphicsRecordedFrame {
                resource: 0xfeed,
                resource_view: 0xbeef,
                texture_format: GraphicsTextureFormat::Rgb10A2,
                color_space: opennow_streamer_platform::GraphicsColorSpace::Sdr709,
                width: 2560,
                height: 1440,
                frame_slot: command.frame_slot,
                generation: self.sequence,
                presentation_time_ns: 8_333_333 * self.sequence,
            })
        }
    }

    impl Drop for TestGraphicsFrame {
        fn drop(&mut self) {
            self.recorded.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn ffi_graphics_context() -> OpenNowStreamerGraphicsContext {
        OpenNowStreamerGraphicsContext {
            version: OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION,
            struct_size: size_of::<OpenNowStreamerGraphicsContext>(),
            graphics_api: OPENNOW_STREAMER_GRAPHICS_API_VULKAN,
            instance: ptr::dangling_mut(),
            physical_device: ptr::dangling_mut(),
            device: ptr::dangling_mut(),
            queue: ptr::dangling_mut(),
            queue_family_index: 5,
            enabled_capabilities: 0,
        }
    }

    #[test]
    fn graphics_capabilities_are_explicit_versioned_and_vulkan_only() {
        let mut context = ffi_graphics_context();
        assert!(
            !graphics_context(context)
                .unwrap()
                .vulkan_dmabuf_import_enabled
        );
        context.enabled_capabilities = OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT;
        assert!(
            graphics_context(context)
                .unwrap()
                .vulkan_dmabuf_import_enabled
        );
        assert!(
            !graphics_context(context)
                .unwrap()
                .vulkan_dmabuf_buffer_import_enabled
        );
        context.enabled_capabilities = OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT;
        let buffer_only = graphics_context(context).unwrap();
        assert!(buffer_only.vulkan_dmabuf_buffer_import_enabled);
        assert!(!buffer_only.vulkan_dmabuf_import_enabled);
        context.enabled_capabilities |= OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT;
        let both = graphics_context(context).unwrap();
        assert!(both.vulkan_dmabuf_import_enabled);
        assert!(both.vulkan_dmabuf_buffer_import_enabled);
        context.enabled_capabilities = 4;
        assert_eq!(
            graphics_context(context),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        context.enabled_capabilities = OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT;
        context.graphics_api = OPENNOW_STREAMER_GRAPHICS_API_D3D11;
        assert_eq!(
            graphics_context(context),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        context.enabled_capabilities = OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT;
        assert_eq!(
            graphics_context(context),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        context = ffi_graphics_context();
        for version in [1, 2] {
            context.version = version;
            assert_eq!(
                graphics_context(context),
                Err(OpenNowStreamerStatus::InvalidConfig)
            );
        }
        context = ffi_graphics_context();
        context.struct_size -= 1;
        assert_eq!(
            graphics_context(context),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
    }

    fn ffi_render_command() -> OpenNowStreamerRecordCommand {
        OpenNowStreamerRecordCommand {
            version: OPENNOW_STREAMER_RENDER_COMMAND_VERSION,
            struct_size: size_of::<OpenNowStreamerRecordCommand>(),
            command_buffer: ptr::dangling_mut(),
            frame_slot: 2,
            upscale_width: 0,
            upscale_height: 0,
            upscale_sharpness: 10,
            upscale_denoise: 0,
        }
    }

    #[test]
    fn render_command_three_preserves_controls_and_rejects_previous_layout_and_ranges() {
        let mut command = ffi_render_command();
        let native = render_command(command).unwrap();
        assert_eq!(native.upscale_sharpness, 10);
        assert_eq!(native.upscale_denoise, 0);
        command.upscale_sharpness = 15;
        command.upscale_denoise = 20;
        let native = render_command(command).unwrap();
        assert_eq!(native.upscale_sharpness, 15);
        assert_eq!(native.upscale_denoise, 20);
        command.version = 2;
        assert_eq!(
            render_command(command),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        command.version = OPENNOW_STREAMER_RENDER_COMMAND_VERSION;
        command.struct_size = std::mem::offset_of!(OpenNowStreamerRecordCommand, upscale_sharpness);
        assert_eq!(
            render_command(command),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        for (sharpness, denoise) in [(16, 0), (0, 21), (u32::MAX, u32::MAX)] {
            command = ffi_render_command();
            command.upscale_sharpness = sharpness;
            command.upscale_denoise = denoise;
            assert_eq!(
                render_command(command),
                Err(OpenNowStreamerStatus::InvalidConfig)
            );
        }
        let messages = CallbackMessages::default();
        let mut config = test_config(&messages);
        config.abi_version = 7;
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
    }

    #[test]
    fn render_command_two_preserves_upscaling_and_rejects_old_layouts() {
        let mut command = ffi_render_command();
        assert_eq!(OPENNOW_STREAMER_RENDER_COMMAND_VERSION, 3);
        assert_eq!(render_command(command).unwrap().upscale_width, 0);
        assert_eq!(render_command(command).unwrap().upscale_height, 0);
        command.upscale_width = 2560;
        command.upscale_height = 1440;
        let native = render_command(command).unwrap();
        assert_eq!(native.upscale_width, 2560);
        assert_eq!(native.upscale_height, 1440);
        assert_eq!(native.frame_slot, command.frame_slot);
        command.version = 1;
        assert_eq!(
            render_command(command),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        command.version = OPENNOW_STREAMER_RENDER_COMMAND_VERSION;
        command.struct_size = std::mem::offset_of!(OpenNowStreamerRecordCommand, upscale_width);
        assert_eq!(
            render_command(command),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
        let messages = CallbackMessages::default();
        let mut config = test_config(&messages);
        config.abi_version = 6;
        assert_eq!(
            validate_config(&config),
            Err(OpenNowStreamerStatus::InvalidConfig)
        );
    }

    fn graphics_test_handle(messages: &CallbackMessages) -> OpenNowStreamer {
        OpenNowStreamer::create(
            test_config(messages),
            Arc::new(CapturedInputQueue::default()),
            |events, _frames, _cursor| Engine::embedded(events),
            || {},
        )
        .expect("FFI handle")
    }

    #[test]
    fn typed_submit_api_routes_only_captured_events_to_the_rust_queue() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        handle.input.set_active(true, false, 0);

        assert_eq!(
            unsafe { opennow_streamer_submit_key(&handle, 0x57, 0x02, true) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_relative(&handle, -12, 34) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_absolute(&handle, 20, 30, 640, 360) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_button(&handle, 5, true) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_wheel(&handle, 0, -120) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe {
                opennow_streamer_submit_gamepad(&handle, 2, 0x0404, 0x1001, 4, 5, -6, 7, -8, 9)
            },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe {
                opennow_streamer_submit_local_action(&handle, OPENNOW_STREAMER_LOCAL_ACTION_GUIDE)
            },
            OpenNowStreamerStatus::Ok
        );

        let queue = handle.input.queue();
        assert_eq!(
            queue.take(),
            Some(CapturedInput::Key {
                virtual_key: 0x57,
                modifiers: 0x02,
                pressed: true,
            })
        );
        assert_eq!(
            queue.take(),
            Some(CapturedInput::MouseMove {
                delta_x: -12,
                delta_y: 34,
            })
        );
        assert_eq!(
            queue.take(),
            Some(CapturedInput::MouseAbsolute {
                x: 20,
                y: 30,
                width: 640,
                height: 360,
            })
        );
        assert_eq!(
            queue.take(),
            Some(CapturedInput::MouseButton {
                button: 5,
                pressed: true,
            })
        );
        assert_eq!(
            queue.take(),
            Some(CapturedInput::MouseWheel {
                delta_x: 0,
                delta_y: -120,
            })
        );
        assert_eq!(
            queue.take(),
            Some(CapturedInput::Gamepad {
                controller_id: 2,
                bitmap: 0x0404,
                buttons: 0x1001,
                left_trigger: 4,
                right_trigger: 5,
                left_stick_x: -6,
                left_stick_y: 7,
                right_stick_x: -8,
                right_stick_y: 9,
            })
        );
        assert_eq!(queue.take(), Some(CapturedInput::Guide));
        assert_eq!(queue.take(), None);

        handle.shutdown();
    }

    #[test]
    fn typed_text_submission_validates_copies_and_bounds_the_whole_paste() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        let submit = |bytes: &[u8]| unsafe {
            opennow_streamer_submit_text(&handle, bytes.as_ptr(), bytes.len())
        };
        assert_eq!(
            unsafe { opennow_streamer_submit_text(ptr::null(), b"a".as_ptr(), 1) },
            OpenNowStreamerStatus::NullPointer
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_text(&handle, ptr::null(), 1) },
            OpenNowStreamerStatus::NullPointer
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_text(&handle, ptr::null(), usize::MAX) },
            OpenNowStreamerStatus::MessageTooLarge
        );
        for invalid in [
            &b""[..],
            &b"a\0b"[..],
            &[0xff],
            &[0xc0, 0x80],
            &[0xed, 0xa0, 0x80],
        ] {
            assert_eq!(submit(invalid), OpenNowStreamerStatus::InvalidConfig);
        }
        assert_eq!(
            submit(&vec![b'a'; OPENNOW_STREAMER_MAX_TEXT_BYTES + 1]),
            OpenNowStreamerStatus::MessageTooLarge
        );
        assert_eq!(submit(b"paste"), OpenNowStreamerStatus::Closed);
        handle.input.set_active(true, false, 0);
        assert_eq!(submit(b"paste"), OpenNowStreamerStatus::Closed);
        let queue = handle.input.queue();
        queue.set_text_ready(1, true);
        let mut bytes = "café 世界 🦫\r\n".as_bytes().to_vec();
        assert_eq!(submit(&bytes), OpenNowStreamerStatus::Ok);
        bytes.fill(b'x');
        assert_eq!(submit(b"again"), OpenNowStreamerStatus::QueueFull);
        let Some(CapturedInput::Text(text)) = queue.take() else {
            panic!("missing paste")
        };
        assert_eq!(text.as_str(), "café 世界 🦫\r\n");
        assert_eq!(submit(b"again"), OpenNowStreamerStatus::QueueFull);
        drop(text);
        assert_eq!(
            submit(&vec![b'a'; OPENNOW_STREAMER_MAX_TEXT_BYTES]),
            OpenNowStreamerStatus::Ok
        );
        handle.input.set_active(false, false, 0);
        assert!(queue.take().is_none());
        handle.shutdown();
    }

    #[test]
    fn abi_10_header_and_text_bound_match_rust() {
        let header = include_str!("../include/opennow_streamer_ffi.h");
        assert!(header.contains("#define OPENNOW_STREAMER_FFI_ABI_VERSION 10u"));
        assert!(header.contains("uint64_t windows_adapter_luid;"));
        assert!(header.contains("#define OPENNOW_STREAMER_MAX_TEXT_BYTES 65536u"));
        assert_eq!(OPENNOW_STREAMER_MAX_TEXT_BYTES, 65_536);
        assert!(header.contains("opennow_streamer_submit_text("));
    }

    #[test]
    fn typed_submit_api_validates_boundaries_without_crossing_the_ffi() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_absolute(&handle, 0, 0, 0, 1) },
            OpenNowStreamerStatus::InvalidConfig
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_mouse_button(&handle, 6, true) },
            OpenNowStreamerStatus::InvalidConfig
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_gamepad(&handle, 4, 0, 0, 0, 0, 0, 0, 0, 0) },
            OpenNowStreamerStatus::InvalidConfig
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_local_action(&handle, u32::MAX) },
            OpenNowStreamerStatus::InvalidConfig
        );
        assert_eq!(
            unsafe { opennow_streamer_submit_key(ptr::null(), 0, 0, false) },
            OpenNowStreamerStatus::NullPointer
        );
        handle.shutdown();
    }

    #[test]
    fn graphics_ffi_waits_for_decoder_without_losing_context_or_masking_errors() {
        use opennow_streamer_platform::GraphicsFrameError;
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        let context = ffi_graphics_context();
        assert_eq!(
            unsafe { opennow_streamer_set_graphics_context(&handle, &context) },
            OpenNowStreamerStatus::Ok
        );
        let publisher = handle.frame_publisher();
        let lease = publisher.context().unwrap();
        let recorded = Arc::new(RecordedCommands::default());
        let cases = [
            (
                Some(GraphicsFrameError::NotReady),
                OpenNowStreamerStatus::NoFrame,
            ),
            (
                Some(GraphicsFrameError::NotReady),
                OpenNowStreamerStatus::NoFrame,
            ),
            (None, OpenNowStreamerStatus::Ok),
            (
                Some(GraphicsFrameError::Failed("device removed".into())),
                OpenNowStreamerStatus::RenderFailed,
            ),
        ];
        for (index, (error, expected)) in cases.into_iter().enumerate() {
            publisher
                .publish(
                    lease,
                    Arc::new(TestGraphicsFrame {
                        sequence: index as u64,
                        recorded: Arc::clone(&recorded),
                        panic_on_record: false,
                        error,
                    }),
                )
                .unwrap();
            let mut token = ptr::null_mut();
            let mut info = OpenNowStreamerFrameInfo::default();
            assert_eq!(
                unsafe { opennow_streamer_acquire_latest_frame(&handle, &mut token, &mut info) },
                OpenNowStreamerStatus::Ok
            );
            let mut output = OpenNowStreamerRecordedFrame::default();
            assert_eq!(
                unsafe {
                    opennow_streamer_record_frame(
                        &handle,
                        token,
                        &ffi_render_command(),
                        &mut output,
                    )
                },
                expected
            );
            assert_eq!(output.resource != 0, expected == OpenNowStreamerStatus::Ok);
            assert_eq!(
                unsafe { opennow_streamer_release_frame(token) },
                OpenNowStreamerStatus::Ok
            );
            assert_eq!(recorded.drops.load(Ordering::Relaxed), index + 1);
        }
        assert_eq!(messages.frames_available.load(Ordering::Relaxed), 4);
        assert_eq!(recorded.values.lock().unwrap().len(), 1);
        assert_eq!(
            unsafe { opennow_streamer_scene_graph_shutdown(&handle) },
            OpenNowStreamerStatus::Ok
        );
        handle.shutdown();
    }

    #[test]
    fn graphics_ffi_acquires_records_and_releases_the_latest_frame() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        let context = ffi_graphics_context();
        assert_eq!(
            unsafe { opennow_streamer_set_graphics_context(&handle, &context) },
            OpenNowStreamerStatus::Ok
        );
        let publisher = handle.frame_publisher();
        let lease = publisher.context().expect("graphics context lease");
        let recorded = Arc::new(RecordedCommands::default());
        publisher
            .publish(
                lease,
                Arc::new(TestGraphicsFrame {
                    sequence: 42,
                    recorded: Arc::clone(&recorded),
                    panic_on_record: false,
                    error: None,
                }),
            )
            .expect("publish frame");
        assert_eq!(messages.frames_available.load(Ordering::Relaxed), 1);

        let mut token = ptr::null_mut();
        let mut info = OpenNowStreamerFrameInfo::default();
        assert_eq!(
            unsafe { opennow_streamer_acquire_latest_frame(&handle, &mut token, &mut info) },
            OpenNowStreamerStatus::Ok
        );
        assert!(!token.is_null());
        assert_eq!(
            info,
            OpenNowStreamerFrameInfo {
                width: 2560,
                height: 1440,
                sequence: 42,
                presentation_time_ns: 349_999_986,
            }
        );
        let command = ffi_render_command();
        let mut output = OpenNowStreamerRecordedFrame::default();
        assert_eq!(
            unsafe { opennow_streamer_record_frame(&handle, token, &command, &mut output) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(output.graphics_api, OPENNOW_STREAMER_GRAPHICS_API_VULKAN);
        assert_eq!(
            output.texture_format,
            OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2
        );
        assert_eq!(output.width, 2560);
        assert_eq!(output.height, 1440);
        assert_eq!(output.frame_slot, 2);
        assert_eq!(output.generation, 42);
        assert_eq!(output.resource, 0xfeed);
        assert_eq!(output.resource_view, 0xbeef);
        assert_eq!(output.presentation_time_ns, 349_999_986);
        assert_eq!(
            unsafe { opennow_streamer_record_frame(&handle, token, &command, &mut output) },
            OpenNowStreamerStatus::FrameAlreadyRecorded
        );
        assert_eq!(recorded.drops.load(Ordering::Relaxed), 0);
        assert_eq!(
            unsafe { opennow_streamer_release_frame(token) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(recorded.drops.load(Ordering::Relaxed), 1);

        let values = recorded.values.lock().expect("recorded commands");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].0.api, GraphicsApi::Vulkan);
        assert_eq!(values[0].0.device, context.device as usize);
        assert_eq!(values[0].0.queue, context.queue as usize);
        assert_eq!(values[0].1.command_buffer, command.command_buffer as usize);
        assert_eq!(values[0].1.frame_slot, 2);
        drop(values);

        assert_eq!(
            unsafe { opennow_streamer_scene_graph_shutdown(&handle) },
            OpenNowStreamerStatus::Ok
        );
        handle.shutdown();
    }

    #[test]
    fn scene_graph_shutdown_invalidates_tokens_without_leaking_them() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        let context = ffi_graphics_context();
        assert_eq!(
            unsafe { opennow_streamer_set_graphics_context(&handle, &context) },
            OpenNowStreamerStatus::Ok
        );
        let publisher = handle.frame_publisher();
        let recorded = Arc::new(RecordedCommands::default());
        publisher
            .publish(
                publisher.context().expect("graphics context lease"),
                Arc::new(TestGraphicsFrame {
                    sequence: 1,
                    recorded: Arc::clone(&recorded),
                    panic_on_record: false,
                    error: None,
                }),
            )
            .expect("publish frame");
        let mut token = ptr::null_mut();
        let mut info = OpenNowStreamerFrameInfo::default();
        assert_eq!(
            unsafe { opennow_streamer_acquire_latest_frame(&handle, &mut token, &mut info) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_scene_graph_shutdown(&handle) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe {
                opennow_streamer_record_frame(
                    &handle,
                    token,
                    &ffi_render_command(),
                    &mut OpenNowStreamerRecordedFrame::default(),
                )
            },
            OpenNowStreamerStatus::GraphicsUnavailable
        );
        assert_eq!(recorded.drops.load(Ordering::Relaxed), 0);
        assert_eq!(
            unsafe { opennow_streamer_release_frame(token) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(recorded.drops.load(Ordering::Relaxed), 1);
        handle.shutdown();
    }

    #[test]
    fn graphics_ffi_contains_panics_from_frame_recording() {
        let messages = Box::new(CallbackMessages::default());
        let mut handle = graphics_test_handle(&messages);
        assert_eq!(
            unsafe { opennow_streamer_set_graphics_context(&handle, &ffi_graphics_context()) },
            OpenNowStreamerStatus::Ok
        );
        let publisher = handle.frame_publisher();
        let recorded = Arc::new(RecordedCommands::default());
        publisher
            .publish(
                publisher.context().expect("graphics context lease"),
                Arc::new(TestGraphicsFrame {
                    sequence: 1,
                    recorded,
                    panic_on_record: true,
                    error: None,
                }),
            )
            .expect("publish frame");
        let mut token = ptr::null_mut();
        let mut info = OpenNowStreamerFrameInfo::default();
        assert_eq!(
            unsafe { opennow_streamer_acquire_latest_frame(&handle, &mut token, &mut info) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe {
                opennow_streamer_record_frame(
                    &handle,
                    token,
                    &ffi_render_command(),
                    &mut OpenNowStreamerRecordedFrame::default(),
                )
            },
            OpenNowStreamerStatus::Panic
        );
        assert_eq!(
            unsafe { opennow_streamer_release_frame(token) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_scene_graph_shutdown(&handle) },
            OpenNowStreamerStatus::Ok
        );
        handle.shutdown();
    }

    #[test]
    fn destroy_rejects_an_active_scene_graph_without_consuming_the_handle() {
        let messages = Box::new(CallbackMessages::default());
        let handle = Box::into_raw(Box::new(graphics_test_handle(&messages)));
        assert_eq!(
            unsafe { opennow_streamer_set_graphics_context(handle, &ffi_graphics_context()) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_destroy(handle) },
            OpenNowStreamerStatus::SceneGraphActive
        );
        assert_eq!(
            unsafe { opennow_streamer_scene_graph_shutdown(handle) },
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            unsafe { opennow_streamer_destroy(handle) },
            OpenNowStreamerStatus::Ok
        );
    }

    #[test]
    fn hello_round_trips_through_the_response_callback_with_test_media_runtime() {
        let messages = Box::new(CallbackMessages::default());
        let (mut handle, host) = create_with_test_runtime(&messages);
        let command = serde_json::to_vec(&json!({
            "id": "hello-1",
            "type": "hello",
            "protocolVersion": 7
        }))
        .expect("hello command");

        assert_eq!(handle.send(&command), OpenNowStreamerStatus::Ok);
        let response = messages.wait_for_id("hello-1");
        assert_eq!(response["type"], "ready");
        assert!(
            response["capabilities"]
                .get("supportsOfferAnswer")
                .is_none()
        );
        assert!(response["capabilities"].get("microphoneDevices").is_none());

        handle.shutdown();
        host.join().expect("test media runtime");
    }

    #[test]
    fn audio_mute_round_trips_before_start_and_after_stop() {
        let messages = Box::new(CallbackMessages::default());
        let (mut handle, host) = create_with_test_runtime(&messages);
        for (id, command) in [
            (
                "mute-before-start",
                json!({"type": "setAudioMuted", "muted": true}),
            ),
            ("stop-muted", json!({"type": "stop"})),
            (
                "unmute-after-stop",
                json!({"type": "setAudioMuted", "muted": false}),
            ),
        ] {
            let mut command = command;
            command["id"] = json!(id);
            assert_eq!(
                handle.send(&serde_json::to_vec(&command).unwrap()),
                OpenNowStreamerStatus::Ok
            );
            assert_eq!(messages.wait_for_id(id)["type"], "ok");
        }
        assert_eq!(
            handle.send(br#"{"id":"missing-muted","type":"setAudioMuted"}"#),
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            messages.wait_for_id("missing-muted")["code"],
            "missing-muted"
        );
        handle.shutdown();
        host.join().expect("test media runtime");
    }

    #[test]
    fn audio_devices_round_trips_without_changing_the_media_runtime() {
        let messages = Box::new(CallbackMessages::default());
        let (mut handle, host) = create_with_test_runtime(&messages);
        assert_eq!(
            handle.send(br#"{"id":"audio-1","type":"audioDevices"}"#),
            OpenNowStreamerStatus::Ok
        );
        let response = messages.wait_for_id("audio-1");
        assert_eq!(response["type"], "audioDevices");
        assert_eq!(response["devices"], json!([]));
        assert_eq!(
            handle.send(br#"{"id":"hello-after-audio","type":"hello","protocolVersion":7}"#),
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(messages.wait_for_id("hello-after-audio")["type"], "ready");
        handle.shutdown();
        host.join().expect("test media runtime");
    }

    #[test]
    fn public_constructor_installs_the_production_in_process_media_runtime() {
        let messages = Box::new(CallbackMessages::default());
        let config = test_config(&messages);
        let mut handle = ptr::null_mut();
        assert_eq!(
            unsafe { opennow_streamer_create(&config, &mut handle) },
            OpenNowStreamerStatus::Ok
        );
        assert!(!handle.is_null());
        let command = serde_json::to_vec(&json!({
            "id": "hello-production",
            "type": "hello",
            "protocolVersion": 7
        }))
        .expect("hello command");
        assert_eq!(
            unsafe { &*handle }.send(&command),
            OpenNowStreamerStatus::Ok
        );
        let response = messages.wait_for_id("hello-production");
        assert!(
            response["capabilities"]
                .get("supportsOfferAnswer")
                .is_none()
        );
        assert_eq!(response["capabilities"]["supportsAudioDecode"], true);
        assert_eq!(response["capabilities"]["supportsAudioOutput"], true);
        assert_eq!(
            unsafe { opennow_streamer_destroy(handle) },
            OpenNowStreamerStatus::Ok
        );
    }

    #[test]
    fn replay_commands_cross_ffi_without_activating_disabled_capture() {
        let messages = Box::new(CallbackMessages::default());
        let (mut handle, host) = create_with_test_runtime(&messages);
        let path = std::env::temp_dir().join("opennow-ffi-clip.mkv");
        let command =
            serde_json::to_vec(&json!({"id":"clip-disabled","type":"clip-save","outputPath":path}))
                .unwrap();
        assert_eq!(handle.send(&command), OpenNowStreamerStatus::Ok);
        let response = messages.wait_for_id("clip-disabled");
        assert_eq!(response["code"], "replay-not-enabled");
        assert_eq!(
            handle.send(br#"{"id":"replay-stop","type":"replay-stop"}"#),
            OpenNowStreamerStatus::Ok
        );
        assert_eq!(
            messages.wait_for_id("replay-stop")["type"],
            "replay-stopped"
        );
        handle.shutdown();
        host.join().unwrap();
    }

    #[test]
    fn shutdown_responds_and_closes_the_command_queue() {
        let messages = Box::new(CallbackMessages::default());
        let (mut handle, host) = create_with_test_runtime(&messages);
        let command = br#"{"id":"shutdown-1","type":"shutdown","reason":"ffi test"}"#;

        assert_eq!(handle.send(command), OpenNowStreamerStatus::Ok);
        let response = messages.wait_for_id("shutdown-1");
        assert_eq!(response["type"], "ok");
        let deadline = Instant::now() + Duration::from_secs(5);
        while handle.send(br#"{"id":"late","type":"hello"}"#) != OpenNowStreamerStatus::Closed {
            assert!(Instant::now() < deadline, "command queue did not close");
            thread::yield_now();
        }

        handle.shutdown();
        host.join().expect("test media runtime");
    }
}
