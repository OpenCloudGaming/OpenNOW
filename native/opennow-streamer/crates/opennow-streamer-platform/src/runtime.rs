use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use opennow_streamer_protocol::RenderSurface;

use crate::media::{MediaFeedback, MediaSession, MediaStreamConfig};
#[cfg(target_os = "windows")]
use crate::output::WindowsBridge;
use crate::output::{ActiveOutput, OutputBuffers, OutputEvent};

#[cfg(target_os = "linux")]
use crate::linux_backend::{LinuxVideoPath, LinuxVideoSelection};

// Keep native input sampling below one rendered frame even at 120 Hz. SDL can
// coalesce the individual raw-input events, so this improves delivery cadence
// without allowing the queue to grow with redundant motion packets.
const HOST_POLL_INTERVAL: Duration = Duration::from_micros(250);
const HOST_START_TIMEOUT: Duration = Duration::from_secs(10);
const HOST_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "macos")]
pub(crate) enum MacH264Configuration {
    Hardware(opennow_streamer_platform_macos::StreamSink),
    SoftwareFallback { reason: String },
}

pub(crate) enum HostCommand {
    Start {
        reply: Sender<Result<(), String>>,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
    },
    Pause {
        paused: bool,
        reply: Option<Sender<Result<(), String>>>,
    },
    Surface {
        surface: RenderSurface,
        reply: Sender<Result<(), String>>,
    },
    Cursor(Vec<u8>),
    #[cfg(target_os = "macos")]
    ConfigureMacH264 {
        parameter_sets: opennow_streamer_platform_macos::H264ParameterSets,
        reply: Sender<Result<MacH264Configuration, String>>,
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
    output: Arc<OutputBuffers>,
    paused: Arc<AtomicBool>,
    use_hardware: Arc<AtomicBool>,
    #[cfg(target_os = "windows")]
    windows_bridge: Arc<WindowsBridge>,
    #[cfg(target_os = "linux")]
    linux_selection: LinuxVideoSelection,
    #[cfg(target_os = "linux")]
    linux_software_fallback: Arc<AtomicBool>,
}

impl MediaRuntime {
    pub fn start(
        &self,
        feedback: Sender<MediaFeedback>,
        stream: MediaStreamConfig,
    ) -> Result<MediaSession, String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Start {
                reply,
                feedback: feedback.clone(),
                stream,
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

    pub fn update_surface(&self, surface: RenderSurface) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(HostCommand::Surface { surface, reply })
            .map_err(|_| "native media host is no longer running".to_owned())?;
        response
            .recv_timeout(HOST_CONTROL_TIMEOUT)
            .map_err(|_| "native media host did not apply the Electron surface update".to_owned())?
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.paused.store(paused, Ordering::Release);
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

    pub fn shutdown(&self) {
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
        let mut surface: Option<RenderSurface> = None;
        let mut paused = false;
        let mut feedback: Option<Sender<MediaFeedback>> = None;
        let mut software_playback_started = false;
        #[cfg(target_os = "linux")]
        let mut active_stream = MediaStreamConfig::default();
        loop {
            match self.commands.recv_timeout(HOST_POLL_INTERVAL) {
                Ok(HostCommand::Start {
                    reply,
                    feedback: session_feedback,
                    stream,
                }) => {
                    #[cfg(target_os = "linux")]
                    {
                        active_stream = stream;
                    }
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
                        stream,
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
                                stream,
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
                            active_stream,
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
                                MediaStreamConfig::default(),
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
                        active_stream,
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
                    if let Some(output) = active.as_mut() {
                        output.stop();
                    }
                    active = None;
                    feedback = None;
                    software_playback_started = false;
                }
                Ok(HostCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(output) = active.as_mut() {
                        output.stop();
                    }
                    return;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
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
                        let fallback = (|| {
                            output.stop();
                            self.use_macos_hardware.store(false, Ordering::Release);
                            #[cfg(target_os = "windows")]
                            self.windows_bridge.fall_back_to_software();
                            let mut replacement = ActiveOutput::initialize(
                                Arc::clone(&self.output),
                                false,
                                MediaStreamConfig::default(),
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
                                        to: "OpenH264/SDL",
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
                                MediaStreamConfig::default(),
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
                                active_stream,
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
            // overlay window never finishes ordering in and its controls stay dead.
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
            output,
            paused,
            use_hardware: use_macos_hardware,
            #[cfg(target_os = "windows")]
            windows_bridge,
            #[cfg(target_os = "linux")]
            linux_selection,
            #[cfg(target_os = "linux")]
            linux_software_fallback,
        },
    ))
}

fn initialize_output(
    output: Arc<OutputBuffers>,
    surface: Option<&RenderSurface>,
    paused: bool,
    use_hardware: bool,
    stream: MediaStreamConfig,
    use_linux_hardware: bool,
    #[cfg(target_os = "windows")] windows_bridge: Arc<WindowsBridge>,
) -> Result<ActiveOutput, String> {
    let mut output = ActiveOutput::initialize(
        output,
        use_hardware,
        stream,
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
            value == "auto" || value == hardware_backend
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
    use super::backend_preference_allows_value;

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
}
