use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use opennow_streamer_protocol::{AudioDevice, AudioOutputDevice, RenderSurface};

use crate::GraphicsFramePublisher;
use crate::media::{MediaFeedback, MediaSession, MediaSessionConfig, MediaStreamConfig};
#[cfg(target_os = "windows")]
use crate::output::WindowsBridge;
use crate::output::{ActiveOutput, OutputBuffers, OutputControl, OutputEvent};

#[cfg(target_os = "linux")]
use crate::linux_backend::{LinuxVideoPath, LinuxVideoSelection};

// Keep native input sampling below one rendered frame even at 120 Hz. SDL can
// coalesce the individual raw-input events, so this improves delivery cadence
// without allowing the queue to grow with redundant motion packets.
const HOST_POLL_INTERVAL: Duration = Duration::from_micros(250);
const HOST_START_TIMEOUT: Duration = Duration::from_secs(10);
const HOST_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const MICROPHONE_HOST_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRuntimeControl {
    PointerLock,
}

#[cfg(target_os = "macos")]
pub(crate) enum MacH264Configuration {
    Hardware(opennow_streamer_platform_macos::StreamSink),
    SoftwareFallback { reason: String },
}

pub(crate) struct AudioDeviceRequest {
    pub(crate) reply: Sender<Result<Vec<AudioDevice>, String>>,
    pending: Arc<AtomicBool>,
}

impl Drop for AudioDeviceRequest {
    fn drop(&mut self) {
        self.pending.store(false, Ordering::Release);
    }
}

pub(crate) enum HostCommand {
    StartMicrophone {
        device_id: String,
        shared: Arc<crate::microphone::MicrophoneShared>,
        reply: Sender<Result<(), String>>,
    },
    AudioDevices(AudioDeviceRequest),
    Start {
        reply: Sender<Result<(), String>>,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
        audio_device: AudioOutputDevice,
    },
    Pause {
        paused: bool,
        reply: Option<Sender<Result<(), String>>>,
    },
    Surface {
        surface: RenderSurface,
        reply: Sender<Result<(), String>>,
    },
    Control {
        control: MediaRuntimeControl,
        reply: Sender<Result<(), String>>,
    },
    Cursor(Vec<u8>),
    #[cfg(target_os = "macos")]
    ConfigureMacH264 {
        parameter_sets: opennow_streamer_platform_macos::H264ParameterSets,
        reply: Sender<Result<MacH264Configuration, String>>,
    },
    #[cfg(target_os = "macos")]
    ConfigureMacH265 {
        parameter_sets: opennow_streamer_platform_macos::H265ParameterSets,
        reply: Sender<Result<opennow_streamer_platform_macos::StreamSink, String>>,
    },
    #[cfg(target_os = "macos")]
    ConfigureMacAv1 {
        format: opennow_streamer_platform_macos::Av1Format,
        reply: Sender<Result<opennow_streamer_platform_macos::StreamSink, String>>,
    },
    #[cfg(target_os = "linux")]
    FallbackLinux {
        reason: String,
    },
    Stop,
    Shutdown,
}

#[derive(Clone)]
pub struct MediaRuntime {
    commands: Sender<HostCommand>,
    audio_query_pending: Arc<AtomicBool>,
    output: Arc<OutputBuffers>,
    paused: Arc<AtomicBool>,
    use_hardware: Arc<AtomicBool>,
    #[cfg(target_os = "windows")]
    windows_bridge: Arc<WindowsBridge>,
    #[cfg(target_os = "linux")]
    linux_selection: LinuxVideoSelection,
    #[cfg(target_os = "linux")]
    linux_software_fallback: Arc<AtomicBool>,
    mode: MediaRuntimeMode,
}

#[derive(Clone)]
enum MediaRuntimeMode {
    Standalone,
    Embedded {
        frames: GraphicsFramePublisher,
        config: EmbeddedRuntimeConfig,
    },
    #[cfg(feature = "test-runtime")]
    Test,
}

#[derive(Clone, Default)]
pub struct EmbeddedRuntimeConfig {
    pub vulkan_device: Option<Arc<crate::SharedVulkanDevice>>,
    pub windows_adapter_luid: Option<crate::WindowsAdapterLuid>,
}

impl MediaRuntime {
    pub fn set_audio_muted(&self, muted: bool) {
        self.output.audio_muted.store(muted, Ordering::Release);
    }

    pub const fn is_embedded(&self) -> bool {
        matches!(self.mode, MediaRuntimeMode::Embedded { .. })
    }

    pub fn video_backends(&self) -> Vec<opennow_streamer_protocol::VideoBackendCapability> {
        match &self.mode {
            MediaRuntimeMode::Embedded { config, .. } => {
                crate::embedded_video_backends_with_config(
                    config.vulkan_device.as_deref(),
                    self.windows_adapter_luid(),
                )
            }
            _ => crate::video_backends(),
        }
    }

    fn windows_adapter_luid(&self) -> Option<crate::WindowsAdapterLuid> {
        match &self.mode {
            MediaRuntimeMode::Embedded { config, .. } => config.windows_adapter_luid,
            _ => None,
        }
    }

    pub fn captured_input(&self) -> Arc<crate::CapturedInputQueue> {
        self.output.captured_input()
    }

    pub fn start_microphone(&self, device_id: &str) -> Result<crate::MicrophoneSession, String> {
        self.output.stop_microphone();
        let shared = crate::microphone::MicrophoneShared::new();
        let session = crate::MicrophoneSession::from_shared(Arc::clone(&shared));
        self.output.set_microphone(&shared);
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::StartMicrophone {
                device_id: device_id.to_owned(),
                shared,
                reply,
            })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_START_TIMEOUT)
            .map_err(|_| "native media host did not start microphone capture".to_owned())??;
        Ok(session)
    }

    pub fn audio_devices(&self) -> Result<Vec<AudioDevice>, String> {
        if self.audio_query_pending.swap(true, Ordering::AcqRel) {
            return Err("native audio device enumeration is already in progress".to_owned());
        }
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::AudioDevices(AudioDeviceRequest {
                reply,
                pending: Arc::clone(&self.audio_query_pending),
            }))
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_CONTROL_TIMEOUT)
            .map_err(|_| "native audio device enumeration timed out".to_owned())?
    }

    pub fn start(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
    ) -> Result<MediaSession, String> {
        self.start_with_backend(feedback, stream, "auto")
    }

    pub fn validate_backend(&self, requested: &str) -> Result<(), String> {
        if !self.is_embedded() {
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        let supported = matches!(requested, "auto" | "d3d11");
        #[cfg(target_os = "macos")]
        let supported = matches!(requested, "auto" | "videotoolbox");
        #[cfg(target_os = "linux")]
        let supported = requested == "auto"
            || (matches!(requested, "vulkan" | "cuda" | "nvdec" | "vaapi" | "v4l2")
                && self.video_backends().iter().any(|backend| {
                    backend.available
                        && (backend.backend == requested
                            || (requested == "nvdec" && backend.backend == "cuda"))
                }));
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        let supported = requested == "auto";
        if supported {
            Ok(())
        } else {
            Err(format!(
                "Backend {requested} is not supported by this embedded stream view. Select Auto in Stream settings."
            ))
        }
    }

    pub fn start_with_backend(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
        requested: &str,
    ) -> Result<MediaSession, String> {
        self.start_with_audio_device(feedback, stream, requested, AudioOutputDevice::default())
    }

    pub fn start_with_audio_device(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
        requested: &str,
        audio_device: AudioOutputDevice,
    ) -> Result<MediaSession, String> {
        self.validate_backend(requested)?;
        if stream.hdr {
            if stream.codec == crate::MediaVideoCodec::H264
                || stream.color_quality.bit_depth() != 10
            {
                return Err("HDR requires a negotiated 10-bit HEVC or AV1 stream".to_owned());
            }
            if stream.color_quality.is_444() && !self.supports_hdr_444(stream) {
                return Err("HDR requires a negotiated 10-bit 4:2:0 stream unless HEVC 10-bit 4:4:4 hardware decode and GPU output are supported".to_owned());
            }
            if !cfg!(any(
                target_os = "windows",
                target_os = "linux",
                target_os = "macos"
            )) {
                return Err(
                    "HDR is supported only by the Windows, Linux, and macOS embedded stream view"
                        .to_owned(),
                );
            }
            if matches!(self.mode, MediaRuntimeMode::Standalone) {
                return Err("HDR requires the color-managed embedded Qt stream view".to_owned());
            }
            #[cfg(target_os = "macos")]
            if stream.codec != crate::MediaVideoCodec::H265
                || !(if stream.color_quality.is_444() {
                    opennow_streamer_platform_macos::probe_h265_hdr_444_hardware()
                } else {
                    opennow_streamer_platform_macos::probe_h265_hdr_hardware()
                })
            {
                return Err(
                    "HDR requires a verified HEVC VideoToolbox and Metal output path".to_owned(),
                );
            }
        }
        if (stream.codec == crate::MediaVideoCodec::H264
            && stream.color_quality != crate::MediaColorQuality::EightBit420)
            || (stream.codec == crate::MediaVideoCodec::Av1 && stream.color_quality.is_444())
        {
            return Err("The negotiated codec and color profile cannot be preserved by the embedded decoder".to_owned());
        }
        self.output.stop_microphone();
        self.output.reset_microphone_clock();
        if let MediaRuntimeMode::Embedded {
            frames,
            config: _config,
        } = &self.mode
        {
            #[cfg(target_os = "macos")]
            if let Some(id) = audio_device.device_name() {
                if self
                    .audio_devices()?
                    .iter()
                    .filter(|device| device.id == id)
                    .count()
                    != 1
                {
                    return Err("The selected audio output device is unavailable or ambiguous. Select another device or System default.".to_owned());
                }
            }
            #[cfg(target_os = "windows")]
            {
                let (reply, response) = mpsc::channel();
                self.commands
                    .send(HostCommand::Start {
                        reply,
                        feedback: feedback.clone(),
                        stream,
                        audio_device: audio_device.clone(),
                    })
                    .map_err(|_| "native media host is no longer running".to_owned())?;
                response
                    .recv_timeout(HOST_START_TIMEOUT)
                    .map_err(|_| "native audio output startup timed out".to_owned())??;
            }
            let session = MediaSession::spawn_embedded(
                Arc::clone(&self.output),
                feedback,
                self.commands.clone(),
                crate::media::MediaSessionConfig {
                    stream,
                    audio_device: &audio_device,
                },
                frames.clone(),
                #[cfg(target_os = "linux")]
                crate::linux_backend::select_embedded_video_path(
                    requested,
                    _config.vulkan_device.as_deref(),
                    stream,
                ),
                #[cfg(target_os = "linux")]
                _config.vulkan_device.clone(),
            )
            .inspect_err(|_| {
                let _ = self.commands.send(HostCommand::Stop);
            })?;
            if self.paused.load(Ordering::Acquire) {
                session.set_paused(true);
            }
            return Ok(session);
        }
        #[cfg(feature = "test-runtime")]
        if matches!(self.mode, MediaRuntimeMode::Test) {
            let session = MediaSession::spawn_test(
                Arc::clone(&self.output),
                feedback,
                self.commands.clone(),
                stream,
                #[cfg(target_os = "windows")]
                Arc::clone(&self.windows_bridge),
                #[cfg(target_os = "linux")]
                Arc::clone(&self.linux_software_fallback),
            )?;
            if self.paused.load(Ordering::Acquire) {
                session.set_paused(true);
            }
            return Ok(session);
        }
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Start {
                reply,
                feedback: feedback.clone(),
                stream,
                audio_device,
            })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_START_TIMEOUT)
            .map_err(|_| "native media host did not start on the UI thread".to_owned())??;
        #[cfg(target_os = "linux")]
        if let Some(reason) = self.linux_selection.fallback_reason.clone() {
            let _ = feedback.send(MediaFeedback::BackendFallback {
                from: "Linux Vulkan presentation",
                to: if matches!(self.linux_selection.path, LinuxVideoPath::Hardware(_)) {
                    "Linux decoder/SDL NV12"
                } else {
                    "OpenH264/SDL"
                },
                reason,
            });
        }
        match MediaSession::spawn(
            Arc::clone(&self.output),
            feedback,
            self.commands.clone(),
            self.use_hardware.load(Ordering::Acquire),
            stream,
            #[cfg(target_os = "windows")]
            Arc::clone(&self.windows_bridge),
            #[cfg(target_os = "linux")]
            self.linux_selection.clone(),
            #[cfg(target_os = "linux")]
            Arc::clone(&self.linux_software_fallback),
        ) {
            Ok(session) => {
                if self.paused.load(Ordering::Acquire) {
                    session.set_paused(true);
                }
                Ok(session)
            }
            Err(error) => {
                let _ = self.commands.send(HostCommand::Stop);
                Err(error)
            }
        }
    }

    fn supports_hdr_444(&self, stream: MediaStreamConfig) -> bool {
        if !self.is_embedded() || stream.codec != crate::MediaVideoCodec::H265 {
            return false;
        }
        #[cfg(target_os = "windows")]
        {
            use opennow_streamer_platform_windows::{VideoCodec, VideoPixelFormat, WindowsBackend};
            WindowsBackend::probe_for(
                opennow_streamer_platform_windows::WindowsGraphicsApi::D3d11,
                self.windows_adapter_luid(),
            )
            .supports_format(VideoCodec::H265, VideoPixelFormat::Y410, true)
        }
        #[cfg(target_os = "linux")]
        {
            use opennow_streamer_platform_linux::{PixelFormat, VideoCodec};
            matches!(&self.mode, MediaRuntimeMode::Embedded { config, .. }
                if config.vulkan_device.as_ref().is_some_and(|device| device.supports_format(VideoCodec::H265, PixelFormat::P410, stream.width, stream.height)))
        }
        #[cfg(target_os = "macos")]
        {
            opennow_streamer_platform_macos::probe_h265_hdr_444_hardware()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        false
    }

    pub fn update_surface(&self, surface: RenderSurface) -> Result<(), String> {
        if !matches!(self.mode, MediaRuntimeMode::Standalone) {
            return Ok(());
        }
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Surface { surface, reply })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_CONTROL_TIMEOUT)
            .map_err(|_| "native media host did not apply the shell surface update".to_owned())?
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.paused.store(paused, Ordering::Release);
        if !matches!(self.mode, MediaRuntimeMode::Standalone) {
            return Ok(());
        }
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Pause {
                paused,
                reply: Some(reply),
            })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_CONTROL_TIMEOUT)
            .map_err(|_| "native media host did not apply the pause update".to_owned())?
    }

    pub fn control(&self, control: MediaRuntimeControl) -> Result<(), String> {
        if !matches!(self.mode, MediaRuntimeMode::Standalone) {
            return Ok(());
        }
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Control { control, reply })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_CONTROL_TIMEOUT)
            .map_err(|_| "native media host did not apply the runtime control".to_owned())?
    }

    pub fn shutdown(&self) {
        self.output.stop_microphone();
        let _ = self.commands.send(HostCommand::Shutdown);
    }
}

pub struct MainThreadHost {
    commands: Receiver<HostCommand>,
    output: Arc<OutputBuffers>,
    _not_send: PhantomData<Rc<()>>,
    use_macos_hardware: Arc<AtomicBool>,
    #[cfg(target_os = "windows")]
    windows_bridge: Arc<WindowsBridge>,
    #[cfg(target_os = "linux")]
    linux_selection: LinuxVideoSelection,
    #[cfg(target_os = "linux")]
    linux_software_fallback: Arc<AtomicBool>,
}

impl MainThreadHost {
    pub fn run(self) {
        let mut active: Option<ActiveOutput> = None;
        let mut microphone: Option<crate::microphone::MicrophoneCapture> = None;
        let mut surface: Option<RenderSurface> = None;
        let mut paused = false;
        let mut feedback: Option<Sender<MediaFeedback>> = None;
        let mut software_playback_started = false;
        let mut active_stream = MediaStreamConfig::default();
        let mut active_audio_device = AudioOutputDevice::default();
        loop {
            match self.commands.recv_timeout(HOST_POLL_INTERVAL) {
                Ok(HostCommand::StartMicrophone {
                    device_id,
                    shared,
                    reply,
                }) => {
                    microphone = None;
                    let result = crate::microphone::MicrophoneCapture::start(
                        &device_id,
                        shared,
                        self.output.microphone_clock(),
                    )
                    .map(|capture| microphone = Some(capture));
                    if reply.send(result).is_err() {
                        microphone = None;
                    }
                }
                Ok(HostCommand::Start {
                    reply,
                    feedback: session_feedback,
                    stream,
                    audio_device,
                }) => {
                    active_stream = stream;
                    self.output.stop_microphone();
                    microphone = None;
                    active_audio_device = audio_device;
                    if let Some(output) = active.as_mut() {
                        output.stop();
                    }
                    let requested_hardware = self.use_macos_hardware.load(Ordering::Acquire);
                    #[cfg(target_os = "linux")]
                    let requested_linux_hardware = self.linux_selection.use_vulkan_output
                        && matches!(self.linux_selection.path, LinuxVideoPath::Hardware(_))
                        && !self.linux_software_fallback.load(Ordering::Acquire);
                    #[cfg(not(target_os = "linux"))]
                    let requested_linux_hardware = false;
                    let mut startup_fallback = None;
                    let initialized = match initialize_output(
                        Arc::clone(&self.output),
                        surface.as_ref(),
                        paused,
                        requested_hardware,
                        MediaSessionConfig {
                            stream,
                            audio_device: &active_audio_device,
                        },
                        requested_linux_hardware,
                        #[cfg(target_os = "windows")]
                        Arc::clone(&self.windows_bridge),
                    ) {
                        Ok(output) => Ok(output),
                        Err(hardware_error) if requested_hardware || requested_linux_hardware => {
                            self.use_macos_hardware.store(false, Ordering::Release);
                            #[cfg(target_os = "windows")]
                            self.windows_bridge.fall_back_to_software();
                            startup_fallback = Some(hardware_error);
                            initialize_output(
                                Arc::clone(&self.output),
                                surface.as_ref(),
                                paused,
                                false,
                                MediaSessionConfig {
                                    stream,
                                    audio_device: &active_audio_device,
                                },
                                false,
                                #[cfg(target_os = "windows")]
                                Arc::clone(&self.windows_bridge),
                            )
                        }
                        Err(error) => Err(error),
                    };
                    match initialized {
                        Ok(output) => {
                            active = Some(output);
                            feedback = Some(session_feedback);
                            software_playback_started = false;
                            if let (Some(reason), Some(feedback)) =
                                (startup_fallback, feedback.as_ref())
                            {
                                let _ = feedback.send(MediaFeedback::BackendFallback {
                                    from: hardware_backend_label(),
                                    to: if requested_linux_hardware {
                                        "Linux decoder/SDL NV12"
                                    } else if cfg!(target_os = "windows")
                                        && stream.codec != crate::media::MediaVideoCodec::H264
                                    {
                                        "Media Foundation software/D3D11/WASAPI"
                                    } else {
                                        "OpenH264/SDL"
                                    },
                                    reason,
                                });
                            }
                            let _ = reply.send(Ok(()));
                        }
                        Err(error) => {
                            active = None;
                            feedback = None;
                            let _ = reply.send(Err(error));
                        }
                    }
                }
                Ok(HostCommand::AudioDevices(request)) => {
                    let reply = &request.reply;
                    #[cfg(target_os = "windows")]
                    if self.use_macos_hardware.load(Ordering::Acquire)
                        || active
                            .as_ref()
                            .is_some_and(|output| matches!(output, ActiveOutput::Windows(_)))
                    {
                        let _ = reply.send(Err("Fixed audio output selection is unavailable in the standalone Windows native output. Use the Qt embedded stream view.".to_owned()));
                        continue;
                    }
                    #[cfg(target_os = "macos")]
                    if self.use_macos_hardware.load(Ordering::Acquire) {
                        let devices = opennow_streamer_platform_macos::audio_output_devices()
                            .map(|devices| {
                                devices
                                    .into_iter()
                                    .map(|device| AudioDevice {
                                        id: device.id,
                                        name: device.name,
                                    })
                                    .collect()
                            })
                            .map_err(|error| error.to_string());
                        let _ = reply.send(devices);
                        continue;
                    }
                    let _ = reply.send(crate::output::audio_devices());
                }
                Ok(HostCommand::Pause {
                    paused: new_paused,
                    reply,
                }) => {
                    let result = active
                        .as_mut()
                        .map_or(Ok(()), |output| output.set_paused(new_paused));
                    if result.is_ok() {
                        paused = new_paused;
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(result);
                    }
                }
                Ok(HostCommand::Surface {
                    surface: new_surface,
                    reply,
                }) => {
                    static SURFACE_LOG_REMAINING: AtomicU64 = AtomicU64::new(12);
                    if SURFACE_LOG_REMAINING
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                            remaining.checked_sub(1)
                        })
                        .is_ok()
                    {
                        eprintln!(
                            "NVST surface-update visible={} rect={:?}",
                            new_surface.visible, new_surface.screen_rect
                        );
                    }
                    #[cfg(target_os = "linux")]
                    let linux_hardware =
                        active.as_ref().is_some_and(ActiveOutput::is_linux_hardware);
                    let result = if let Some(output) = active.as_mut() {
                        output.update_surface(&new_surface)
                    } else {
                        Ok(())
                    };
                    #[cfg(target_os = "linux")]
                    let mut result = result;
                    #[cfg(target_os = "linux")]
                    if linux_hardware && let Err(reason) = &result {
                        match initialize_output(
                            Arc::clone(&self.output),
                            Some(&new_surface),
                            paused,
                            false,
                            MediaSessionConfig {
                                stream: active_stream,
                                audio_device: &active_audio_device,
                            },
                            false,
                        ) {
                            Ok(output) => {
                                if let Some(current) = active.as_mut() {
                                    current.stop();
                                }
                                active = Some(output);
                                software_playback_started = false;
                                if let Some(feedback) = feedback.as_ref() {
                                    let _ = feedback.send(MediaFeedback::BackendFallback {
                                        from: "Linux decoder/Vulkan",
                                        to: "Linux decoder/SDL NV12",
                                        reason: format!(
                                            "Linux surface reconfiguration failed: {reason}"
                                        ),
                                    });
                                }
                                result = Ok(());
                            }
                            Err(error) => result = Err(error),
                        }
                    }
                    if let Err(message) = &result {
                        if let Some(output) = active.as_mut() {
                            if let Some(feedback) = feedback.as_ref() {
                                let _ = feedback.send(MediaFeedback::OutputError {
                                    message: message.clone(),
                                });
                            }
                            output.stop();
                            active = None;
                            feedback = None;
                        }
                    }
                    surface = Some(new_surface);
                    let _ = reply.send(result);
                }
                Ok(HostCommand::Control { control, reply }) => {
                    let output_control = match control {
                        MediaRuntimeControl::PointerLock => OutputControl::PointerLock,
                    };
                    let result = active
                        .as_mut()
                        .ok_or_else(|| "native media output is not active".to_owned())
                        .and_then(|output| output.control(output_control));
                    let _ = reply.send(result);
                }
                #[cfg(target_os = "macos")]
                Ok(HostCommand::ConfigureMacH264 {
                    parameter_sets,
                    reply,
                }) => {
                    let hardware_result = active
                        .as_mut()
                        .ok_or_else(|| "native media output is not active".to_owned())
                        .and_then(|output| output.configure_macos_h264(parameter_sets));
                    match hardware_result {
                        Ok(sink) => {
                            let _ = reply.send(Ok(MacH264Configuration::Hardware(sink)));
                        }
                        Err(hardware_error) => {
                            self.use_macos_hardware.store(false, Ordering::Release);
                            crate::macos_backend::disable();
                            if let Some(output) = active.as_mut() {
                                output.stop();
                            }
                            let fallback = ActiveOutput::initialize(
                                Arc::clone(&self.output),
                                false,
                                MediaSessionConfig {
                                    stream: active_stream,
                                    audio_device: &active_audio_device,
                                },
                                false,
                            )
                            .and_then(|mut output| {
                                output.start(surface.as_ref())?;
                                output.set_paused(paused)?;
                                Ok(output)
                            });
                            match fallback {
                                Ok(output) => {
                                    active = Some(output);
                                    let _ =
                                        reply.send(Ok(MacH264Configuration::SoftwareFallback {
                                            reason: hardware_error,
                                        }));
                                }
                                Err(fallback_error) => {
                                    active = None;
                                    let _ = reply.send(Err(format!(
                                        "VideoToolbox startup failed ({hardware_error}); software output fallback also failed: {fallback_error}"
                                    )));
                                }
                            }
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                Ok(HostCommand::ConfigureMacH265 {
                    parameter_sets,
                    reply,
                }) => {
                    let result = active
                        .as_mut()
                        .ok_or_else(|| "native media output is not active".to_owned())
                        .and_then(|output| output.configure_macos_h265(parameter_sets));
                    if result.is_err() {
                        crate::macos_backend::disable_h265();
                    }
                    let _ = reply.send(result);
                }
                #[cfg(target_os = "macos")]
                Ok(HostCommand::ConfigureMacAv1 { format, reply }) => {
                    let result = active
                        .as_mut()
                        .ok_or_else(|| "native media output is not active".to_owned())
                        .and_then(|output| output.configure_macos_av1(format));
                    if result.is_err() {
                        crate::macos_backend::disable_av1();
                    }
                    let _ = reply.send(result);
                }
                Ok(HostCommand::Cursor(bytes)) => {
                    if let Some(output) = active.as_mut() {
                        output.update_cursor(&bytes);
                    }
                }
                #[cfg(target_os = "linux")]
                Ok(HostCommand::FallbackLinux { reason }) => {
                    self.linux_software_fallback.store(true, Ordering::Release);
                    let replacement = initialize_output(
                        Arc::clone(&self.output),
                        surface.as_ref(),
                        paused,
                        false,
                        MediaSessionConfig {
                            stream: active_stream,
                            audio_device: &active_audio_device,
                        },
                        false,
                    );
                    match replacement {
                        Ok(output) => {
                            if let Some(current) = active.as_mut() {
                                current.stop();
                            }
                            active = Some(output);
                            software_playback_started = false;
                            if let Some(feedback) = feedback.as_ref() {
                                let _ = feedback.send(MediaFeedback::BackendFallback {
                                    from: "Linux decoder pipeline",
                                    to: "OpenH264/SDL",
                                    reason,
                                });
                            }
                        }
                        Err(fallback_error) => {
                            if let Some(feedback) = feedback.as_ref() {
                                let _ = feedback.send(MediaFeedback::OutputError {
                                    message: format!(
                                        "Linux hardware fallback failed: {fallback_error}"
                                    ),
                                });
                            }
                            active = None;
                            feedback = None;
                        }
                    }
                }
                Ok(HostCommand::Stop) => {
                    self.output.stop_microphone();
                    microphone = None;
                    if let Some(output) = active.as_mut() {
                        output.stop();
                    }
                    active = None;
                    feedback = None;
                    software_playback_started = false;
                }
                Ok(HostCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.output.stop_microphone();
                    drop(microphone);
                    if let Some(output) = active.as_mut() {
                        output.stop();
                    }
                    return;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(capture) = microphone.as_mut() {
                capture.poll();
                if capture.stopped() {
                    microphone = None;
                }
            }
            if let Some(output) = active.as_mut() {
                let output_event = output.pump();
                let captured_input = self.output.captured_input();
                for input in output.take_captured_input() {
                    captured_input.push(input);
                }
                match output_event {
                    Ok(OutputEvent::Presented(backend)) if !software_playback_started => {
                        software_playback_started = true;
                        if let Some(feedback) = feedback.as_ref() {
                            let _ = feedback.send(MediaFeedback::PlaybackStarted { backend });
                        }
                    }
                    #[cfg(target_os = "windows")]
                    Ok(OutputEvent::RequestKeyframe) => {
                        if let Some(feedback) = feedback.as_ref() {
                            let _ = feedback.send(MediaFeedback::RequestKeyframe {
                                mid: current_video_mid(
                                    #[cfg(target_os = "windows")]
                                    &self.windows_bridge,
                                ),
                                reason: "D3D11 decoder recovery requires a fresh keyframe"
                                    .to_owned(),
                            });
                        }
                    }
                    #[cfg(target_os = "windows")]
                    Ok(OutputEvent::DeviceLost {
                        subsystem,
                        recovered,
                        message,
                    }) => {
                        if let Some(feedback) = feedback.as_ref() {
                            let _ = feedback.send(MediaFeedback::DeviceLost {
                                subsystem,
                                recovered,
                                message,
                            });
                        }
                    }
                    #[cfg(target_os = "windows")]
                    Ok(OutputEvent::QueueDropped(media)) => {
                        if let Some(feedback) = feedback.as_ref() {
                            let _ = feedback.send(MediaFeedback::QueueDropped { media, count: 1 });
                        }
                    }
                    #[cfg(target_os = "windows")]
                    Ok(OutputEvent::Fatal(message)) => {
                        if !output.is_windows_hardware() {
                            if let Some(feedback) = feedback.as_ref() {
                                let _ = feedback.send(MediaFeedback::OutputError { message });
                            }
                            output.stop();
                            active = None;
                            feedback = None;
                            software_playback_started = false;
                            continue;
                        }
                        let fallback = (|| {
                            output.stop();
                            self.use_macos_hardware.store(false, Ordering::Release);
                            #[cfg(target_os = "windows")]
                            self.windows_bridge.fall_back_to_software();
                            let mut replacement = ActiveOutput::initialize(
                                Arc::clone(&self.output),
                                false,
                                MediaSessionConfig {
                                    stream: active_stream,
                                    audio_device: &active_audio_device,
                                },
                                #[cfg(target_os = "windows")]
                                Arc::clone(&self.windows_bridge),
                                false,
                            )?;
                            replacement.start(surface.as_ref())?;
                            replacement.set_paused(paused)?;
                            Ok::<_, String>(replacement)
                        })();
                        match fallback {
                            Ok(replacement) => {
                                *output = replacement;
                                software_playback_started = false;
                                if let Some(feedback) = feedback.as_ref() {
                                    let _ = feedback.send(MediaFeedback::BackendFallback {
                                        from: "Media Foundation/D3D11/WASAPI",
                                        to: if active_stream.codec
                                            == crate::media::MediaVideoCodec::H264
                                        {
                                            "OpenH264/SDL"
                                        } else {
                                            "Media Foundation software/D3D11/WASAPI"
                                        },
                                        reason: message,
                                    });
                                    let _ = feedback.send(MediaFeedback::RequestKeyframe {
                                        mid: current_video_mid(
                                            #[cfg(target_os = "windows")]
                                            &self.windows_bridge,
                                        ),
                                        reason: "D3D11 device recovery failed".to_owned(),
                                    });
                                }
                            }
                            Err(fallback_error) => {
                                if let Some(feedback) = feedback.as_ref() {
                                    let _ = feedback.send(MediaFeedback::OutputError {
                                        message: format!(
                                            "D3D11 output failed ({message}); software fallback failed: {fallback_error}"
                                        ),
                                    });
                                }
                                active = None;
                                feedback = None;
                                software_playback_started = false;
                            }
                        }
                    }
                    Ok(OutputEvent::None | OutputEvent::Presented(_)) => {}
                    Err(message) => {
                        #[cfg(target_os = "macos")]
                        if output.is_macos_hardware() {
                            self.use_macos_hardware.store(false, Ordering::Release);
                            crate::macos_backend::disable();
                            output.stop();
                            let fallback = initialize_output(
                                Arc::clone(&self.output),
                                surface.as_ref(),
                                paused,
                                false,
                                MediaSessionConfig {
                                    stream: active_stream,
                                    audio_device: &active_audio_device,
                                },
                                false,
                            );
                            match fallback {
                                Ok(replacement) => {
                                    *output = replacement;
                                    software_playback_started = false;
                                    if let Some(feedback) = feedback.as_ref() {
                                        let _ = feedback.send(MediaFeedback::BackendFallback {
                                            from: "VideoToolbox/Metal/CoreAudio",
                                            to: "OpenH264/SDL",
                                            reason: message,
                                        });
                                    }
                                    continue;
                                }
                                Err(fallback_error) => {
                                    if let Some(feedback) = feedback.as_ref() {
                                        let _ = feedback.send(MediaFeedback::OutputError {
                                            message: format!(
                                                "macOS hardware output failed ({message}); software fallback failed: {fallback_error}"
                                            ),
                                        });
                                    }
                                    active = None;
                                    feedback = None;
                                    software_playback_started = false;
                                    continue;
                                }
                            }
                        }
                        #[cfg(target_os = "linux")]
                        if output.is_linux_hardware() {
                            output.stop();
                            let fallback = initialize_output(
                                Arc::clone(&self.output),
                                surface.as_ref(),
                                paused,
                                false,
                                MediaSessionConfig {
                                    stream: active_stream,
                                    audio_device: &active_audio_device,
                                },
                                false,
                            );
                            if let Ok(replacement) = fallback {
                                *output = replacement;
                                software_playback_started = false;
                                if let Some(feedback) = feedback.as_ref() {
                                    let _ = feedback.send(MediaFeedback::BackendFallback {
                                        from: "Linux decoder/Vulkan",
                                        to: "Linux decoder/SDL NV12",
                                        reason: format!(
                                            "Linux Vulkan presentation failed: {message}"
                                        ),
                                    });
                                }
                                continue;
                            }
                        }
                        if let Some(feedback) = feedback.as_ref() {
                            let _ = feedback.send(MediaFeedback::OutputError { message });
                        }
                        output.stop();
                        active = None;
                        feedback = None;
                        software_playback_started = false;
                    }
                }
            }
            // The host loop replaces `NSApplication.run()`; without draining AppKit's queue the
            // stream window never finishes ordering in and its controls stay dead.
            #[cfg(target_os = "macos")]
            opennow_streamer_platform_macos::pump_app_events();
        }
    }
}

pub fn create_runtime() -> Result<(MainThreadHost, MediaRuntime), String> {
    ensure_macos_main_thread()?;
    let use_macos_hardware = Arc::new(AtomicBool::new(use_hardware_backend()));
    #[cfg(target_os = "linux")]
    let linux_selection = crate::linux_backend::select_video_path();
    #[cfg(target_os = "linux")]
    let linux_software_fallback = Arc::new(AtomicBool::new(matches!(
        linux_selection.path,
        LinuxVideoPath::Software
    )));
    let (commands, receiver) = mpsc::channel();
    let output = Arc::new(OutputBuffers::new());
    let paused = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "windows")]
    let windows_bridge = Arc::new(WindowsBridge::new());
    Ok((
        MainThreadHost {
            commands: receiver,
            output: Arc::clone(&output),
            _not_send: PhantomData,
            use_macos_hardware: Arc::clone(&use_macos_hardware),
            #[cfg(target_os = "windows")]
            windows_bridge: Arc::clone(&windows_bridge),
            #[cfg(target_os = "linux")]
            linux_selection: linux_selection.clone(),
            #[cfg(target_os = "linux")]
            linux_software_fallback: Arc::clone(&linux_software_fallback),
        },
        MediaRuntime {
            commands,
            audio_query_pending: Arc::new(AtomicBool::new(false)),
            output,
            paused,
            use_hardware: use_macos_hardware,
            #[cfg(target_os = "windows")]
            windows_bridge,
            #[cfg(target_os = "linux")]
            linux_selection,
            #[cfg(target_os = "linux")]
            linux_software_fallback,
            mode: MediaRuntimeMode::Standalone,
        },
    ))
}

pub fn create_embedded_runtime(frames: GraphicsFramePublisher) -> MediaRuntime {
    create_embedded_runtime_with_input(frames, Arc::new(crate::CapturedInputQueue::default()), None)
}

pub fn create_embedded_runtime_with_input(
    frames: GraphicsFramePublisher,
    captured_input: Arc<crate::CapturedInputQueue>,
    cursor_update: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync>>,
) -> MediaRuntime {
    create_embedded_runtime_with_config(
        frames,
        captured_input,
        cursor_update,
        EmbeddedRuntimeConfig::default(),
    )
}

pub fn create_embedded_runtime_with_config(
    frames: GraphicsFramePublisher,
    captured_input: Arc<crate::CapturedInputQueue>,
    cursor_update: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync>>,
    config: EmbeddedRuntimeConfig,
) -> MediaRuntime {
    let (commands, receiver) = mpsc::channel();
    let output = Arc::new(OutputBuffers::with_captured_input(captured_input));
    let host_output = Arc::clone(&output);
    let _ = std::thread::Builder::new()
        .name("opennow-embedded-host".to_owned())
        .spawn(move || {
            #[cfg(any(target_os = "windows", test))]
            let mut audio = None;
            let mut microphone: Option<crate::microphone::MicrophoneCapture> = None;
            loop {
                let command = if microphone.is_some() {
                    receiver.recv_timeout(MICROPHONE_HOST_POLL_INTERVAL)
                } else {
                    receiver
                        .recv()
                        .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                };
                let command = match command {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                if let Some(command) = command {
                    match command {
                        HostCommand::StartMicrophone {
                            device_id,
                            shared,
                            reply,
                        } => {
                            microphone = None;
                            let result = crate::microphone::MicrophoneCapture::start(
                                &device_id,
                                shared,
                                host_output.microphone_clock(),
                            )
                            .map(|capture| microphone = Some(capture));
                            if reply.send(result).is_err() {
                                microphone = None;
                            }
                        }
                        HostCommand::Cursor(bytes) => {
                            if let Some(update) = &cursor_update {
                                update(bytes);
                            }
                        }
                        HostCommand::AudioDevices(request) => {
                            let reply = &request.reply;
                            #[cfg(all(target_os = "linux", not(test)))]
                            let devices = opennow_streamer_platform_linux::audio_output_devices()
                                .map(|devices| {
                                    devices
                                        .into_iter()
                                        .map(|device| AudioDevice {
                                            id: device.id,
                                            name: device.name,
                                        })
                                        .collect()
                                })
                                .map_err(|error| error.to_string());
                            #[cfg(any(target_os = "windows", test))]
                            let devices = crate::output::audio_devices();
                            #[cfg(all(target_os = "macos", not(test)))]
                            let devices = opennow_streamer_platform_macos::audio_output_devices()
                                .map(|devices| {
                                    devices
                                        .into_iter()
                                        .map(|device| AudioDevice {
                                            id: device.id,
                                            name: device.name,
                                        })
                                        .collect()
                                })
                                .map_err(|error| error.to_string());
                            #[cfg(not(any(
                                target_os = "linux",
                                target_os = "windows",
                                target_os = "macos",
                                test
                            )))]
                            let devices =
                                Err("Native audio device enumeration is unavailable".to_owned());
                            let _ = reply.send(devices);
                        }
                        #[cfg(any(target_os = "windows", test))]
                        HostCommand::Start {
                            reply,
                            audio_device,
                            ..
                        } => {
                            host_output.stop_microphone();
                            microphone = None;
                            audio = None;
                            let result = crate::output::HeadlessAudioOutput::start(
                                Arc::clone(&host_output),
                                &audio_device,
                            )
                            .map(|started| {
                                audio = Some(started);
                            });
                            if reply.send(result).is_err() {
                                audio = None;
                            }
                        }
                        #[cfg(any(target_os = "windows", test))]
                        HostCommand::Pause { paused, reply } => {
                            if let Some(audio) = &audio {
                                audio.set_paused(paused);
                            }
                            if let Some(reply) = reply {
                                let _ = reply.send(Ok(()));
                            }
                        }
                        HostCommand::Stop => {
                            host_output.stop_microphone();
                            microphone = None;
                            #[cfg(any(target_os = "windows", test))]
                            {
                                audio = None;
                            }
                        }
                        HostCommand::Shutdown => break,
                        _ => {}
                    }
                }
                if let Some(capture) = microphone.as_mut() {
                    capture.poll();
                    if capture.stopped() {
                        microphone = None;
                    }
                }
            }
            host_output.stop_microphone();
            drop(microphone);
        });
    #[cfg(target_os = "linux")]
    let linux_selection = crate::linux_backend::select_video_path();
    MediaRuntime {
        commands,
        audio_query_pending: Arc::new(AtomicBool::new(false)),
        output,
        paused: Arc::new(AtomicBool::new(false)),
        use_hardware: Arc::new(AtomicBool::new(true)),
        #[cfg(target_os = "windows")]
        windows_bridge: Arc::new(WindowsBridge::new()),
        #[cfg(target_os = "linux")]
        linux_selection,
        #[cfg(target_os = "linux")]
        linux_software_fallback: Arc::new(AtomicBool::new(false)),
        mode: MediaRuntimeMode::Embedded { frames, config },
    }
}

#[cfg(feature = "test-runtime")]
pub struct TestMediaRuntimeHost {
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "test-runtime")]
impl TestMediaRuntimeHost {
    pub fn join(mut self) -> std::thread::Result<()> {
        self.worker.take().expect("test media worker").join()
    }
}

#[cfg(feature = "test-runtime")]
pub fn create_test_runtime() -> (TestMediaRuntimeHost, MediaRuntime) {
    let use_macos_hardware = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "linux")]
    let linux_selection = crate::linux_backend::select_video_path();
    #[cfg(target_os = "linux")]
    let linux_software_fallback = Arc::new(AtomicBool::new(matches!(
        linux_selection.path,
        LinuxVideoPath::Software
    )));
    let (commands, receiver) = mpsc::channel();
    let output = Arc::new(OutputBuffers::new());
    let paused = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "windows")]
    let windows_bridge = Arc::new(WindowsBridge::new());
    let worker = std::thread::Builder::new()
        .name("opennow-test-media-runtime".to_owned())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                match command {
                    HostCommand::StartMicrophone { reply, .. } => {
                        let _ =
                            reply
                                .send(Err("test media runtime has no microphone capture device"
                                    .to_owned()));
                    }
                    HostCommand::AudioDevices(request) => {
                        let reply = &request.reply;
                        let _ = reply.send(Ok(Vec::new()));
                    }
                    HostCommand::Start { reply, .. } => {
                        let _ = reply.send(Err(
                            "test media runtime does not start media sessions".to_owned()
                        ));
                    }
                    HostCommand::Pause { reply, .. } => {
                        if let Some(reply) = reply {
                            let _ = reply.send(Ok(()));
                        }
                    }
                    HostCommand::Surface { reply, .. } | HostCommand::Control { reply, .. } => {
                        let _ = reply.send(Ok(()));
                    }
                    HostCommand::Cursor(_) | HostCommand::Stop => {}
                    #[cfg(target_os = "macos")]
                    HostCommand::ConfigureMacH264 { reply, .. } => {
                        let _ = reply.send(Err("test media runtime has no decoder".to_owned()));
                    }
                    #[cfg(target_os = "macos")]
                    HostCommand::ConfigureMacH265 { reply, .. } => {
                        let _ = reply.send(Err("test media runtime has no decoder".to_owned()));
                    }
                    #[cfg(target_os = "macos")]
                    HostCommand::ConfigureMacAv1 { reply, .. } => {
                        let _ = reply.send(Err("test media runtime has no decoder".to_owned()));
                    }
                    #[cfg(target_os = "linux")]
                    HostCommand::FallbackLinux { .. } => {}
                    HostCommand::Shutdown => break,
                }
            }
        })
        .expect("test media runtime worker");
    (
        TestMediaRuntimeHost {
            worker: Some(worker),
        },
        MediaRuntime {
            commands,
            audio_query_pending: Arc::new(AtomicBool::new(false)),
            output,
            paused,
            use_hardware: use_macos_hardware,
            #[cfg(target_os = "windows")]
            windows_bridge,
            #[cfg(target_os = "linux")]
            linux_selection,
            #[cfg(target_os = "linux")]
            linux_software_fallback,
            mode: MediaRuntimeMode::Test,
        },
    )
}

fn initialize_output(
    output: Arc<OutputBuffers>,
    surface: Option<&RenderSurface>,
    paused: bool,
    use_hardware: bool,
    config: MediaSessionConfig<'_>,
    use_linux_hardware: bool,
    #[cfg(target_os = "windows")] windows_bridge: Arc<WindowsBridge>,
) -> Result<ActiveOutput, String> {
    let mut output = ActiveOutput::initialize(
        output,
        use_hardware,
        config,
        #[cfg(target_os = "windows")]
        windows_bridge,
        use_linux_hardware,
    )?;
    if let Err(error) = output.start(surface) {
        output.stop();
        return Err(error);
    }
    if let Err(error) = output.set_paused(paused) {
        output.stop();
        return Err(error);
    }
    Ok(output)
}

#[cfg(target_os = "macos")]
fn use_hardware_backend() -> bool {
    backend_preference_allows("videotoolbox") && crate::macos_backend::available()
}

#[cfg(target_os = "windows")]
fn use_hardware_backend() -> bool {
    backend_preference_allows("d3d12") || backend_preference_allows("d3d11")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const fn use_hardware_backend() -> bool {
    false
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn backend_preference_allows(hardware_backend: &str) -> bool {
    backend_preference_allows_value(
        std::env::var("OPENNOW_NATIVE_VIDEO_BACKEND")
            .ok()
            .as_deref(),
        hardware_backend,
    )
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn backend_preference_allows_value(value: Option<&str>, hardware_backend: &str) -> bool {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|value| {
            let value = value.to_ascii_lowercase();
            value == "auto" || value == "hardware" || value == hardware_backend
        })
}

#[cfg(target_os = "windows")]
fn current_video_mid(bridge: &WindowsBridge) -> String {
    bridge.last_video_mid()
}

#[cfg(target_os = "windows")]
const fn hardware_backend_label() -> &'static str {
    "Media Foundation/Direct3D/WASAPI"
}

#[cfg(target_os = "macos")]
const fn hardware_backend_label() -> &'static str {
    "VideoToolbox/Metal/CoreAudio"
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const fn hardware_backend_label() -> &'static str {
    "Linux decoder/Vulkan"
}

#[cfg(target_os = "macos")]
fn ensure_macos_main_thread() -> Result<(), String> {
    if unsafe { libc::pthread_main_np() } == 1 {
        Ok(())
    } else {
        Err(
            "the macOS native media host must be created and run on the process main thread"
                .to_owned(),
        )
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos_main_thread() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_runtime_snapshots_windows_adapter_selection() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let luid = crate::WindowsAdapterLuid::new(0xffff_fffe_1122_3344).expect("non-zero LUID");
        let runtime = super::create_embedded_runtime_with_config(
            frames,
            std::sync::Arc::new(crate::CapturedInputQueue::default()),
            None,
            super::EmbeddedRuntimeConfig {
                vulkan_device: None,
                windows_adapter_luid: Some(luid),
            },
        );
        let super::MediaRuntimeMode::Embedded { config, .. } = &runtime.mode else {
            panic!("embedded runtime mode")
        };
        assert_eq!(config.windows_adapter_luid, Some(luid));
        assert_eq!(runtime.windows_adapter_luid(), Some(luid));
    }

    #[test]
    #[ignore = "requires SDL_AUDIODRIVER=dummy in an isolated test process"]
    fn embedded_host_shares_sdl_playback_capture_and_enumeration_lifetimes() {
        use super::{HostCommand, MediaRuntime};
        use std::sync::{Arc, mpsc};
        use std::time::{Duration, Instant};

        fn wait_until(mut condition: impl FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !condition() {
                assert!(
                    Instant::now() < deadline,
                    "embedded audio host did not settle"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn start_playback(
            runtime: &MediaRuntime,
            device: &opennow_streamer_protocol::AudioOutputDevice,
        ) {
            let (reply, response) = mpsc::channel();
            let (feedback, _) = mpsc::channel();
            runtime
                .commands
                .send(HostCommand::Start {
                    reply,
                    feedback,
                    stream: crate::MediaStreamConfig::default(),
                    audio_device: device.clone(),
                })
                .unwrap();
            response
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
        }

        fn playback_consumes(runtime: &MediaRuntime) {
            runtime.output.push_audio(&[0.25; 4096]);
            wait_until(|| !runtime.output.audio_samples_pending());
        }

        fn next_microphone(receiver: &crate::MicrophoneReceiver) -> u32 {
            let mut timestamp = None;
            wait_until(|| {
                timestamp = receiver.try_recv().unwrap().map(|frame| frame.timestamp);
                timestamp.is_some()
            });
            timestamp.unwrap()
        }

        assert_eq!(std::env::var("SDL_AUDIODRIVER").as_deref(), Ok("dummy"));
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let (cursor_sent, cursor_received) = mpsc::channel();
        let runtime = super::create_embedded_runtime_with_input(
            frames,
            Arc::new(crate::CapturedInputQueue::default()),
            Some(Arc::new(move |bytes| {
                let _ = cursor_sent.send(bytes);
            })),
        );
        let devices = runtime.audio_devices().unwrap();
        let selected = opennow_streamer_protocol::AudioOutputDevice::new(
            devices.first().expect("dummy playback device").id.clone(),
        )
        .unwrap();
        start_playback(&runtime, &selected);
        playback_consumes(&runtime);
        assert!(
            sdl2::init().is_err(),
            "the embedded host must remain SDL's sole initialization thread"
        );

        let mut microphone = runtime.start_microphone("").unwrap();
        let receiver = microphone.receiver();
        let first = next_microphone(&receiver);
        assert_eq!(runtime.audio_devices().unwrap(), devices);
        playback_consumes(&runtime);
        for paused in [true, false] {
            let (reply, response) = mpsc::channel();
            runtime
                .commands
                .send(HostCommand::Pause {
                    paused,
                    reply: Some(reply),
                })
                .unwrap();
            response
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
            next_microphone(&receiver);
            assert!(receiver.status().enabled);
        }
        playback_consumes(&runtime);

        let stop_receiver = microphone.receiver();
        std::thread::spawn(move || stop_receiver.stop())
            .join()
            .unwrap();
        wait_until(|| !receiver.capture_device_open());
        assert!(!receiver.status().enabled);
        assert_eq!(receiver.try_recv(), Ok(None));
        assert_eq!(runtime.audio_devices().unwrap(), devices);
        assert!(!receiver.capture_device_open());
        playback_consumes(&runtime);
        microphone.stop();
        assert!(runtime.start_microphone("unsupported-input").is_err());
        playback_consumes(&runtime);

        let restarted = runtime.start_microphone("").unwrap();
        let restarted_receiver = restarted.receiver();
        assert!(
            next_microphone(&restarted_receiver).wrapping_sub(first)
                >= crate::MICROPHONE_FRAME_SAMPLES as u32
        );
        receiver.stop();
        assert!(restarted_receiver.status().enabled);
        playback_consumes(&runtime);

        restarted_receiver.pause_capture_device();
        wait_until(|| !restarted_receiver.capture_device_open());
        assert!(!restarted_receiver.status().enabled);
        assert!(restarted_receiver.status().error.is_some());
        playback_consumes(&runtime);
        let terminal_microphone = runtime.start_microphone("").unwrap();
        let terminal_receiver = terminal_microphone.receiver();
        next_microphone(&terminal_receiver);

        runtime.commands.send(HostCommand::Stop).unwrap();
        runtime.commands.send(HostCommand::Cursor(vec![1])).unwrap();
        assert_eq!(
            cursor_received
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            vec![1]
        );
        assert!(!restarted_receiver.status().enabled);
        assert!(!restarted_receiver.capture_device_open());
        assert!(!terminal_receiver.status().enabled);
        assert!(!terminal_receiver.capture_device_open());
        runtime.output.push_audio(&[0.25; 4096]);
        std::thread::sleep(Duration::from_millis(50));
        assert!(runtime.output.audio_samples_pending());

        start_playback(&runtime, &selected);
        let final_microphone = runtime.start_microphone("").unwrap();
        let final_receiver = final_microphone.receiver();
        next_microphone(&final_receiver);
        runtime.shutdown();
        assert!(!final_receiver.status().enabled);
        assert_eq!(
            cursor_received.recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
        assert!(!final_receiver.capture_device_open());
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn timed_out_audio_query_does_not_queue_more_requests() {
        let (host, mut runtime) = super::create_test_runtime();
        let original = runtime.clone();
        let (commands, receiver) = std::sync::mpsc::channel();
        runtime.commands = commands;
        assert!(runtime.audio_devices().unwrap_err().contains("timed out"));
        assert!(
            runtime
                .audio_devices()
                .unwrap_err()
                .contains("already in progress")
        );
        drop(receiver.try_recv().expect("one queued query"));
        assert!(receiver.try_recv().is_err());
        assert!(
            !runtime
                .audio_query_pending
                .load(std::sync::atomic::Ordering::Acquire)
        );
        drop(receiver);
        assert!(
            runtime
                .audio_devices()
                .unwrap_err()
                .contains("no longer running")
        );
        assert!(
            !runtime
                .audio_query_pending
                .load(std::sync::atomic::Ordering::Acquire)
        );
        original.shutdown();
        host.join().expect("test host");
    }

    use super::backend_preference_allows_value;

    #[test]
    fn hdr_rejects_h264_and_eight_bit_before_decoder_start() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let runtime = super::create_embedded_runtime(frames);
        for (codec, color_quality) in [
            (
                crate::MediaVideoCodec::H264,
                crate::MediaColorQuality::TenBit420,
            ),
            (
                crate::MediaVideoCodec::H265,
                crate::MediaColorQuality::EightBit420,
            ),
            (
                crate::MediaVideoCodec::Av1,
                crate::MediaColorQuality::EightBit444,
            ),
        ] {
            let (feedback, _) = std::sync::mpsc::channel();
            let result = runtime.start(
                feedback,
                crate::MediaStreamConfig {
                    codec,
                    color_quality,
                    hdr: true,
                    ..Default::default()
                },
            );
            assert!(matches!(result, Err(message) if message.contains("10-bit HEVC or AV1")));
        }
        runtime.shutdown();
    }

    #[test]
    fn hdr_444_fails_before_decoder_start() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let mut runtime = super::create_embedded_runtime(frames);
        runtime.mode = super::MediaRuntimeMode::Standalone;
        for codec in [crate::MediaVideoCodec::H265, crate::MediaVideoCodec::Av1] {
            let (feedback, _) = std::sync::mpsc::channel();
            let result = runtime.start(
                feedback,
                crate::MediaStreamConfig {
                    codec,
                    color_quality: crate::MediaColorQuality::TenBit444,
                    hdr: true,
                    ..Default::default()
                },
            );
            assert!(matches!(result, Err(message) if message.contains("10-bit 4:2:0")));
        }
        runtime.shutdown();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hdr_444_without_shared_vulkan_device_never_falls_back_to_cpu() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let runtime = super::create_embedded_runtime(frames);
        let stream = crate::MediaStreamConfig {
            codec: crate::MediaVideoCodec::H265,
            color_quality: crate::MediaColorQuality::TenBit444,
            hdr: true,
            ..Default::default()
        };
        assert!(!runtime.supports_hdr_444(stream));
        let (feedback, _) = std::sync::mpsc::channel();
        assert!(matches!(runtime.start(feedback, stream), Err(message)
            if message.contains("4:4:4 hardware decode and GPU output")));
        runtime.shutdown();
    }

    #[test]
    fn incompatible_color_profiles_fail_before_decoder_start() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let runtime = super::create_embedded_runtime(frames);
        for (codec, color_quality) in [
            (
                crate::MediaVideoCodec::H264,
                crate::MediaColorQuality::EightBit444,
            ),
            (
                crate::MediaVideoCodec::H264,
                crate::MediaColorQuality::TenBit420,
            ),
            (
                crate::MediaVideoCodec::H264,
                crate::MediaColorQuality::TenBit444,
            ),
            (
                crate::MediaVideoCodec::Av1,
                crate::MediaColorQuality::EightBit444,
            ),
            (
                crate::MediaVideoCodec::Av1,
                crate::MediaColorQuality::TenBit444,
            ),
        ] {
            let (feedback, _) = std::sync::mpsc::channel();
            let result = runtime.start(
                feedback,
                crate::MediaStreamConfig {
                    codec,
                    color_quality,
                    ..Default::default()
                },
            );
            assert!(matches!(result, Err(message) if message.contains("cannot be preserved")));
        }
        runtime.shutdown();
    }

    #[test]
    fn standalone_output_never_presents_hdr_as_sdr() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let mut runtime = super::create_embedded_runtime(frames);
        runtime.mode = super::MediaRuntimeMode::Standalone;
        let (feedback, _) = std::sync::mpsc::channel();
        let result = runtime.start(
            feedback,
            crate::MediaStreamConfig {
                codec: crate::MediaVideoCodec::H265,
                color_quality: crate::MediaColorQuality::TenBit420,
                hdr: true,
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(message) if message.contains("embedded")));
        runtime.shutdown();
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn audio_mute_survives_session_restart_without_pausing_media_or_microphone() {
        use std::sync::atomic::Ordering;

        let (host, runtime) = super::create_test_runtime();
        runtime.set_audio_muted(true);
        for _ in 0..2 {
            let (feedback, _) = std::sync::mpsc::channel();
            let media = runtime
                .start(feedback, crate::MediaStreamConfig::default())
                .unwrap();
            assert!(runtime.output.audio_muted.load(Ordering::Acquire));
            assert!(!runtime.paused.load(Ordering::Acquire));
            let shared = crate::microphone::MicrophoneShared::new();
            let microphone = crate::MicrophoneSession::from_shared(std::sync::Arc::clone(&shared));
            runtime.output.set_microphone(&shared);
            let clock = runtime.output.microphone_clock();
            for muted in [false, true] {
                runtime.set_audio_muted(muted);
                assert_eq!(runtime.output.audio_muted.load(Ordering::Acquire), muted);
                assert!(!runtime.paused.load(Ordering::Acquire));
                assert!(microphone.receiver().status().enabled);
                assert_eq!(runtime.output.microphone_clock(), clock);
            }
            media.stop();
            assert!(runtime.output.audio_muted.load(Ordering::Acquire));
        }
        runtime.shutdown();
        host.join().unwrap();
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn audio_device_start_resets_microphone_but_enumeration_does_not() {
        let (host, runtime) = super::create_test_runtime();
        let shared = crate::microphone::MicrophoneShared::new();
        let microphone = crate::MicrophoneSession::from_shared(std::sync::Arc::clone(&shared));
        runtime.output.set_microphone(&shared);
        let receiver = microphone.receiver();
        let previous_clock = runtime.output.microphone_clock();
        assert_eq!(runtime.audio_devices().unwrap(), Vec::new());
        assert!(receiver.status().enabled);
        assert_eq!(runtime.output.microphone_clock(), previous_clock);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let (feedback, _) = std::sync::mpsc::channel();
        let media = runtime
            .start_with_audio_device(
                feedback,
                crate::MediaStreamConfig::default(),
                "auto",
                opennow_streamer_protocol::AudioOutputDevice::default(),
            )
            .unwrap();
        assert!(!receiver.status().enabled);
        assert!(runtime.output.microphone_clock() > previous_clock);
        drop(media);
        runtime.shutdown();
        host.join().unwrap();
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn terminal_media_stop_synchronously_disables_microphone_receiver() {
        let (host, runtime) = super::create_test_runtime();
        let (feedback, _) = std::sync::mpsc::channel();
        let media = runtime
            .start(feedback, crate::MediaStreamConfig::default())
            .unwrap();
        let shared = crate::microphone::MicrophoneShared::new();
        let session = crate::MicrophoneSession::from_shared(std::sync::Arc::clone(&shared));
        runtime.output.set_microphone(&shared);
        let receiver = session.receiver();
        assert!(receiver.status().enabled);
        media.control().stop();
        assert!(!receiver.status().enabled);
        assert_eq!(receiver.try_recv(), Ok(None));
        drop(media);
        runtime.shutdown();
        host.join().unwrap();
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn test_runtime_reports_no_capture_device_instead_of_enabled() {
        let (host, runtime) = super::create_test_runtime();
        let error = runtime
            .start_microphone("")
            .err()
            .expect("test host has no device");
        assert!(error.contains("no microphone capture device"));
        runtime.shutdown();
        host.join().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn embedded_runtime_without_an_owner_disables_vulkan() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let runtime = super::create_embedded_runtime(frames);
        let backends = runtime.video_backends();
        let vulkan = backends
            .iter()
            .find(|backend| backend.backend == "vulkan")
            .unwrap();
        assert!(!vulkan.available);
        assert!(vulkan.codecs.iter().all(|codec| !codec.available));
        assert!(runtime.validate_backend("vulkan").is_err());
        assert!(runtime.validate_backend("auto").is_ok());
        runtime.shutdown();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_windows_rejects_unimplemented_backend_overrides() {
        let (_graphics, frames) = crate::RenderThreadGraphics::new(|| {});
        let runtime = super::create_embedded_runtime(frames);
        assert!(runtime.validate_backend("auto").is_ok());
        assert!(runtime.validate_backend("d3d11").is_ok());
        for backend in ["d3d12", "vulkan", "software", "invalid"] {
            assert!(
                runtime
                    .validate_backend(backend)
                    .unwrap_err()
                    .contains("Select Auto")
            );
        }
        runtime.shutdown();
    }

    #[test]
    fn video_backend_preference_selects_only_auto_or_the_requested_hardware() {
        assert!(backend_preference_allows_value(None, "d3d11"));
        assert!(backend_preference_allows_value(Some(""), "d3d11"));
        assert!(backend_preference_allows_value(Some("AUTO"), "d3d11"));
        assert!(backend_preference_allows_value(Some("d3d11"), "d3d11"));
        assert!(!backend_preference_allows_value(Some("software"), "d3d11"));
        assert!(!backend_preference_allows_value(Some("d3d12"), "d3d11"));
        assert!(backend_preference_allows_value(Some("d3d12"), "d3d12"));
        assert!(!backend_preference_allows_value(Some("d3d11"), "d3d12"));
    }

    #[cfg(feature = "test-runtime")]
    #[test]
    fn embedded_style_start_creates_a_live_media_consumer_without_a_render_surface() {
        use std::sync::Arc;
        use std::sync::mpsc;
        use std::time::Duration;

        use crate::{EncodedFrame, MediaCodec, MediaFeedback, MediaStreamConfig, PushOutcome};

        let (host, runtime) = super::create_test_runtime();
        let (feedback, received) = mpsc::channel();
        let session = runtime
            .start(feedback, MediaStreamConfig::default())
            .expect("headless media session starts without a RenderSurface");
        assert_eq!(
            session.sink().push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from([0_u8, 0, 0, 1, 0x65]),
                frame_index: Some(9),
                timestamp: 90_000,
                clock_rate_hz: 90_000,
                keyframe: true,
                contiguous: true,
            }),
            PushOutcome::Queued
        );
        assert!(matches!(
            received.recv_timeout(Duration::from_secs(2)),
            Ok(MediaFeedback::VideoFrameAccepted { .. })
        ));

        session.stop();
        runtime.shutdown();
        host.join().expect("test media runtime");
    }
}
