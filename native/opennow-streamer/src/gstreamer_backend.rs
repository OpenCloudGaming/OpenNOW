use crate::backend::{
    prepare_native_offer, prepared_offer_events, BackendReply, NativeStreamerBackend,
};
use crate::input::InputEncoder;
use crate::protocol::{
    missing_field, CommandEnvelope, Event, IceCandidatePayload, NativeRenderRect,
    NativeRenderSurface, NativeStreamerCapabilities, NativeStreamerSessionContext, Response,
    SendAnswerRequest, PROTOCOL_VERSION,
};
use crate::sdp::{build_nvst_sdp_for_answer, munge_answer_sdp, IceCredentials};
use gst::glib;
use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_video as gst_video;
use gstreamer_webrtc as gst_webrtc;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_v1";
const PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_partially_reliable";
const DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS: u32 = 300;
const WEBRTC_LATENCY_MS: u32 = 0;
const VIDEO_QUEUE_MAX_BUFFERS: u32 = 1;
const AUDIO_QUEUE_MAX_BUFFERS: u32 = 2;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);
const EXTERNAL_RENDERER_ENV: &str = "OPENNOW_NATIVE_EXTERNAL_RENDERER";

// gstreamer-rs exposes the generic ICE transport but not the NICE stream that
// owns remote credentials. GFN uses UUID ICE passwords, so we need the actual
// NICE stream after GStreamer's SDP parser validates a sanitized copy.
#[repr(C)]
struct GstWebRTCNiceTransportCompat {
    parent: gst_webrtc::ffi::GstWebRTCICETransport,
    stream: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    _priv: glib::ffi::gpointer,
}

#[derive(Debug, Clone, Copy)]
struct ActualNiceIceStream {
    ptr: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    stream_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodedMediaKind {
    Audio,
    Video,
    Unknown,
}

#[derive(Clone)]
struct GstreamerInputState {
    encoder: Arc<Mutex<InputEncoder>>,
    ready: Arc<AtomicBool>,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for GstreamerInputState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GstreamerInputState")
            .field("ready", &self.ready.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Default for GstreamerInputState {
    fn default() -> Self {
        Self {
            encoder: Arc::new(Mutex::new(InputEncoder::default())),
            ready: Arc::new(AtomicBool::new(false)),
            heartbeat_stop: Arc::new(AtomicBool::new(false)),
            heartbeat_thread: Arc::new(Mutex::new(None)),
        }
    }
}

impl GstreamerInputState {
    fn reset(&self) {
        self.ready.store(false, Ordering::SeqCst);
        if let Ok(mut encoder) = self.encoder.lock() {
            encoder.set_protocol_version(2);
            encoder.reset_gamepad_sequences();
        }
    }

    fn stop_heartbeat(&self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        let Some(handle) = self
            .heartbeat_thread
            .lock()
            .ok()
            .and_then(|mut thread| thread.take())
        else {
            return;
        };

        if let Err(error) = handle.join() {
            eprintln!("[NativeStreamer] Input heartbeat thread panicked: {error:?}");
        }
    }
}

#[derive(Debug)]
struct GstreamerInputChannels {
    reliable: gst_webrtc::WebRTCDataChannel,
    partially_reliable: gst_webrtc::WebRTCDataChannel,
}

impl GstreamerInputChannels {
    fn labels(&self) -> (String, String) {
        (
            channel_label(&self.reliable),
            channel_label(&self.partially_reliable),
        )
    }

    fn send_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if payload.is_empty() {
            return false;
        }

        let channel = if partially_reliable
            && self.partially_reliable.ready_state() == gst_webrtc::WebRTCDataChannelState::Open
        {
            &self.partially_reliable
        } else {
            &self.reliable
        };

        if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
            return false;
        }

        let bytes = glib::Bytes::from_owned(payload.to_vec());
        channel.send_data(Some(&bytes));
        true
    }
}

#[derive(Clone, Debug, Default)]
struct GstreamerRenderState {
    surface: Arc<Mutex<Option<NativeRenderSurface>>>,
    video_sink: Arc<Mutex<Option<gst::Element>>>,
    external_renderer_logged: Arc<AtomicBool>,
    external_window_guard_started: Arc<AtomicBool>,
}

impl GstreamerRenderState {
    fn set_surface(&self, surface: NativeRenderSurface, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.surface.lock() {
            *current = Some(surface);
        }
        self.apply(event_sender);
    }

    fn set_video_sink(&self, sink: gst::Element, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.video_sink.lock() {
            *current = Some(sink);
        }
        self.apply(event_sender);
    }

    fn apply(&self, event_sender: &Option<Sender<Event>>) {
        let sink = self.video_sink.lock().ok().and_then(|sink| sink.clone());
        let Some(sink) = sink else {
            return;
        };

        if use_external_renderer_window() {
            if !self
                .external_window_guard_started
                .swap(true, Ordering::SeqCst)
            {
                start_external_renderer_window_guard(event_sender.clone());
            }
            if !self.external_renderer_logged.swap(true, Ordering::SeqCst) {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Using external native GStreamer renderer window; set {EXTERNAL_RENDERER_ENV}=0 to retry Electron HWND embedding."
                    ),
                );
            }
            return;
        }

        let surface = self.surface.lock().ok().and_then(|surface| surface.clone());
        let Some(surface) = surface else {
            return;
        };

        if let Err(message) = apply_render_surface_to_video_sink(&sink, &surface) {
            send_log(event_sender, "warn", message);
        }
    }
}

#[derive(Debug)]
struct GstreamerPipeline {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    input_state: GstreamerInputState,
    input_channels: Option<GstreamerInputChannels>,
    render_state: GstreamerRenderState,
    event_sender: Option<Sender<Event>>,
    original_remote_ice_credentials: Option<IceCredentials>,
    original_remote_ice_credentials_restored: bool,
}

impl GstreamerPipeline {
    fn build(event_sender: Option<Sender<Event>>) -> Result<Self, String> {
        init_gstreamer()?;

        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("opennow-webrtcbin")
            .property_from_str("bundle-policy", "max-bundle")
            .build()
            .map_err(|error| format!("Failed to create webrtcbin: {error}"))?;
        configure_webrtc_low_latency(&webrtc);

        let input_state = GstreamerInputState::default();
        let render_state = GstreamerRenderState::default();
        wire_local_ice_events(&webrtc, event_sender.clone())?;
        wire_webrtc_state_events(&webrtc, event_sender.clone());
        wire_remote_data_channels(&webrtc, event_sender.clone());
        wire_incoming_media_sink(
            &pipeline,
            &webrtc,
            event_sender.clone(),
            render_state.clone(),
        );

        pipeline
            .add(&webrtc)
            .map_err(|error| format!("Failed to add webrtcbin to pipeline: {error}"))?;
        pipeline
            .set_state(gst::State::Ready)
            .map_err(|error| format!("Failed to set GStreamer pipeline to Ready: {error:?}"))?;

        Ok(Self {
            pipeline,
            webrtc,
            input_state,
            input_channels: None,
            render_state,
            event_sender,
            original_remote_ice_credentials: None,
            original_remote_ice_credentials_restored: false,
        })
    }

    fn parse_offer_sdp(sdp: &str) -> Result<gst_sdp::SDPMessage, String> {
        init_gstreamer()?;
        gst_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
            .map_err(|error| format!("GStreamer rejected the remote SDP offer: {error:?}"))
    }

    fn webrtc_name(&self) -> String {
        self.webrtc.name().to_string()
    }

    fn ensure_input_data_channels(
        &mut self,
        partial_reliable_threshold_ms: u32,
    ) -> Result<(), String> {
        if self.input_channels.is_some() {
            return Ok(());
        }

        self.input_state.reset();
        let channels = create_input_data_channels(
            &self.webrtc,
            self.input_state.clone(),
            self.event_sender.clone(),
            partial_reliable_threshold_ms,
        )?;
        let _ = channels.labels();
        self.input_channels = Some(channels);
        Ok(())
    }

    fn negotiate_answer(
        &mut self,
        offer_sdp: gst_sdp::SDPMessage,
        original_remote_credentials: Option<&IceCredentials>,
        partial_reliable_threshold_ms: u32,
    ) -> Result<String, String> {
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| {
                format!("Failed to set GStreamer pipeline to Playing before negotiation: {error:?}")
            })?;
        self.set_description("set-remote-description", &offer)?;
        if let Some(credentials) = original_remote_credentials {
            self.original_remote_ice_credentials = Some(credentials.clone());
            self.try_restore_original_remote_ice_credentials("after remote description")?;
        }
        self.ensure_input_data_channels(partial_reliable_threshold_ms)?;
        let answer = self.create_answer()?;
        let answer_sdp = answer
            .sdp()
            .as_text()
            .map_err(|error| format!("Failed to serialize GStreamer answer SDP: {error}"))?;
        self.set_description("set-local-description", &answer)?;
        self.try_restore_original_remote_ice_credentials("after local description")?;
        Ok(answer_sdp)
    }

    fn try_restore_original_remote_ice_credentials(&mut self, stage: &str) -> Result<bool, String> {
        if self.original_remote_ice_credentials_restored {
            return Ok(true);
        }

        let Some(credentials) = self.original_remote_ice_credentials.clone() else {
            return Ok(false);
        };

        if credentials.ufrag.is_empty() || credentials.pwd.is_empty() {
            return Err(
                "Cannot restore original remote ICE credentials: offer credentials are empty."
                    .to_owned(),
            );
        }

        let Some(ice_agent) = self
            .webrtc
            .property::<Option<gst_webrtc::WebRTCICE>>("ice-agent")
        else {
            return Err(
                "Cannot restore original remote ICE credentials: webrtcbin has no ICE agent."
                    .to_owned(),
            );
        };
        let ice_agent_ptr = ice_agent.as_ptr() as *mut gst_webrtc::ffi::GstWebRTCICE;
        let ufrag = CString::new(credentials.ufrag.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: ufrag contains NUL.")?;
        let pwd = CString::new(credentials.pwd.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: pwd contains NUL.")?;

        let streams = self.negotiated_nice_streams();
        if streams.is_empty() {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer has not exposed actual NICE ICE streams {stage}; deferring GFN remote ICE credential restoration."
                ),
            );
            return Ok(false);
        }

        let mut restored = 0usize;
        let stream_ids = streams
            .iter()
            .map(|stream| stream.stream_id)
            .collect::<Vec<_>>();
        for stream in &streams {
            let accepted = unsafe {
                gst_webrtc::ffi::gst_webrtc_ice_set_remote_credentials(
                    ice_agent_ptr,
                    stream.ptr,
                    ufrag.as_ptr(),
                    pwd.as_ptr(),
                ) != glib::ffi::GFALSE
            };
            if accepted {
                restored += 1;
            } else {
                send_log(
                    &self.event_sender,
                    "warn",
                    format!(
                        "GStreamer ICE agent rejected original remote credentials for actual stream {}.",
                        stream.stream_id
                    ),
                );
            }
        }

        if restored == 0 {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer rejected original GFN remote ICE credentials on all actual streams {stage}; ICE may fail."
                ),
            );
            return Ok(false);
        }

        self.original_remote_ice_credentials_restored = true;
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Restored original GFN remote ICE credentials on {restored}/{} actual GStreamer NICE ICE stream(s) {stage}; streamIds={stream_ids:?}.",
                streams.len()
            ),
        );
        Ok(true)
    }

    fn negotiated_nice_streams(&self) -> Vec<ActualNiceIceStream> {
        let mut streams = Vec::new();
        let mut seen_stream_pointers = HashSet::new();
        let mut seen_transport_summaries = Vec::new();
        for index in 0..8 {
            let transceiver = self
                .webrtc
                .emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
                    "get-transceiver",
                    &[&(index as i32)],
                );
            let Some(transceiver) = transceiver else {
                continue;
            };

            if let Some(receiver) = transceiver.receiver() {
                if let Some(transport) = receiver.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "receiver",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
            if let Some(sender) = transceiver.sender() {
                if let Some(transport) = sender.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "sender",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
        }

        if !seen_transport_summaries.is_empty() {
            send_log(
                &self.event_sender,
                "debug",
                format!(
                    "GStreamer negotiated ICE transports: {}.",
                    seen_transport_summaries.join(", ")
                ),
            );
        }
        streams
    }

    fn collect_nice_stream_from_dtls_transport(
        &self,
        dtls_transport: &gst_webrtc::WebRTCDTLSTransport,
        transceiver_index: u32,
        direction: &str,
        streams: &mut Vec<ActualNiceIceStream>,
        seen_stream_pointers: &mut HashSet<usize>,
        seen_transport_summaries: &mut Vec<String>,
    ) {
        let session_id = dtls_transport.session_id();
        let Some(ice_transport) = dtls_transport.transport() else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransport=none"
            ));
            return;
        };

        let transport_type = ice_transport.type_().name().to_owned();
        let component = ice_transport.component();
        let state = ice_transport.state();
        let Some(stream) = nice_stream_from_ice_transport(&ice_transport) else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} stream=none"
            ));
            return;
        };

        seen_transport_summaries.push(format!(
            "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} streamId={}",
            stream.stream_id
        ));

        let stream_pointer = stream.ptr as usize;
        if seen_stream_pointers.insert(stream_pointer) {
            streams.push(stream);
        }
    }

    fn set_description(
        &self,
        signal_name: &'static str,
        description: &gst_webrtc::WebRTCSessionDescription,
    ) -> Result<(), String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>(signal_name, &[description, &promise]);
        wait_for_promise(&promise, signal_name)
    }

    fn create_answer(&self) -> Result<gst_webrtc::WebRTCSessionDescription, String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
        wait_for_promise(&promise, "create-answer")?;
        let reply = promise
            .get_reply()
            .ok_or_else(|| "GStreamer create-answer resolved without a reply.".to_owned())?;
        reply
            .get::<gst_webrtc::WebRTCSessionDescription>("answer")
            .map_err(|error| {
                format!(
                    "GStreamer create-answer reply did not contain an answer: {error}; reply={}",
                    describe_structure(reply)
                )
            })
    }

    fn add_remote_ice(&mut self, candidate: &IceCandidatePayload) -> Result<(), String> {
        if candidate.candidate.trim().is_empty() {
            return Err("Remote ICE candidate is empty.".to_owned());
        }
        self.try_restore_original_remote_ice_credentials("before adding remote ICE candidate")?;
        let sdp_m_line_index = candidate.sdp_m_line_index.unwrap_or(0);
        self.webrtc.emit_by_name::<()>(
            "add-ice-candidate",
            &[&sdp_m_line_index, &candidate.candidate],
        );
        Ok(())
    }

    fn send_input_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if !self.input_state.ready.load(Ordering::SeqCst) {
            return false;
        }

        let Some(input_channels) = &self.input_channels else {
            return false;
        };

        input_channels.send_packet(payload, partially_reliable)
    }

    fn update_render_surface(&self, surface: NativeRenderSurface) {
        self.render_state.set_surface(surface, &self.event_sender);
    }

    fn stop(&self) -> Result<(), String> {
        self.input_state.stop_heartbeat();
        self.pipeline
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|error| format!("Failed to stop GStreamer pipeline: {error:?}"))
    }
}

fn nice_stream_from_ice_transport(
    transport: &gst_webrtc::WebRTCICETransport,
) -> Option<ActualNiceIceStream> {
    if transport.type_().name() != "GstWebRTCNiceTransport" {
        return None;
    }

    unsafe {
        let transport_ptr = transport.as_ptr() as *mut GstWebRTCNiceTransportCompat;
        if transport_ptr.is_null() {
            return None;
        }

        let stream_ptr = (*transport_ptr).stream;
        if stream_ptr.is_null() {
            return None;
        }

        Some(ActualNiceIceStream {
            ptr: stream_ptr,
            stream_id: (*stream_ptr).stream_id,
        })
    }
}

fn init_gstreamer() -> Result<(), String> {
    gst::init().map_err(|error| format!("Failed to initialize GStreamer: {error}"))
}

fn set_property_if_supported<T: Into<glib::Value>>(element: &gst::Element, name: &str, value: T) {
    if element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

fn set_property_from_str_if_supported(element: &gst::Element, name: &str, value: &str) {
    if element.find_property(name).is_some() {
        element.set_property_from_str(name, value);
    }
}

fn configure_webrtc_low_latency(webrtc: &gst::Element) {
    set_property_if_supported(webrtc, "latency", WEBRTC_LATENCY_MS);
}

fn configure_queue_for_low_latency(element: &gst::Element, media_label: &str) {
    let max_buffers = if media_label == "video" {
        VIDEO_QUEUE_MAX_BUFFERS
    } else {
        AUDIO_QUEUE_MAX_BUFFERS
    };

    set_property_if_supported(element, "max-size-buffers", max_buffers);
    set_property_if_supported(element, "max-size-bytes", 0u32);
    set_property_if_supported(element, "max-size-time", 0u64);
    set_property_from_str_if_supported(element, "leaky", "downstream");
}

fn configure_sink_for_low_latency(element: &gst::Element) {
    set_property_if_supported(element, "sync", false);
    set_property_if_supported(element, "async", false);
    set_property_if_supported(element, "qos", true);
    set_property_if_supported(element, "max-lateness", 0i64);
    set_property_if_supported(element, "processing-deadline", 0u64);
    set_property_if_supported(element, "render-delay", 0u64);
    set_property_if_supported(element, "redraw-on-update", true);
    set_property_if_supported(element, "force-aspect-ratio", true);
}

#[cfg(target_os = "windows")]
fn parse_window_handle(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"));
    let parsed = if let Some(hex) = hex {
        usize::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<usize>()
    }
    .map_err(|error| format!("Invalid native render window handle {value:?}: {error}"))?;

    if parsed == 0 {
        return Err("Native render window handle is zero.".to_owned());
    }

    Ok(parsed)
}

#[cfg(target_os = "windows")]
fn normalized_render_rect(rect: Option<&NativeRenderRect>) -> NativeRenderRect {
    let Some(rect) = rect else {
        return NativeRenderRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
    };

    NativeRenderRect {
        x: rect.x.max(0),
        y: rect.y.max(0),
        width: rect.width.max(2),
        height: rect.height.max(2),
    }
}

fn use_external_renderer_window() -> bool {
    std::env::var(EXTERNAL_RENDERER_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn start_external_renderer_window_guard(event_sender: Option<Sender<Event>>) {
    thread::spawn(move || {
        let mut logged = false;
        for _ in 0..200 {
            let updated = unsafe { win32_renderer_window::protect_process_renderer_windows() };
            if updated > 0 && !logged {
                send_log(
                    &event_sender,
                    "info",
                    format!(
                        "Configured {updated} external native renderer window(s) to avoid stealing OpenNOW input focus."
                    ),
                );
                logged = true;
            }
            thread::sleep(if logged {
                Duration::from_millis(500)
            } else {
                Duration::from_millis(100)
            });
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_external_renderer_window_guard(_event_sender: Option<Sender<Event>>) {}

#[cfg(target_os = "windows")]
mod win32_renderer_window {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type Bool = i32;
    type Hwnd = *mut c_void;
    type Lparam = isize;

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    struct EnumState {
        process_id: u32,
        updated: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>,
            lparam: Lparam,
        ) -> Bool;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: Hwnd) -> Bool;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> Bool;
    }

    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    pub unsafe fn protect_process_renderer_windows() -> u32 {
        let mut state = EnumState {
            process_id: GetCurrentProcessId(),
            updated: 0,
        };
        EnumWindows(Some(protect_window), &mut state as *mut EnumState as Lparam);
        state.updated
    }

    unsafe extern "system" fn protect_window(hwnd: Hwnd, lparam: Lparam) -> Bool {
        let state = &mut *(lparam as *mut EnumState);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != state.process_id || IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired = current | WS_EX_NOACTIVATE;
        if desired != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            state.updated += 1;
        }

        1
    }
}

#[cfg(target_os = "windows")]
fn apply_render_surface_to_video_sink(
    sink: &gst::Element,
    surface: &NativeRenderSurface,
) -> Result<(), String> {
    let Some(window_handle) = surface.window_handle.as_deref() else {
        return Ok(());
    };

    let handle = parse_window_handle(window_handle)?;
    let overlay = sink
        .clone()
        .dynamic_cast::<gst_video::VideoOverlay>()
        .map_err(|_| {
            format!(
                "Native render sink {} does not implement GstVideoOverlay.",
                sink.name()
            )
        })?;
    let rect = normalized_render_rect(surface.visible.then_some(()).and(surface.rect.as_ref()));

    unsafe {
        overlay.set_window_handle(handle);
    }
    overlay.handle_events(false);
    overlay
        .set_render_rectangle(rect.x, rect.y, rect.width, rect.height)
        .map_err(|error| format!("Failed to set native render rectangle: {error}"))?;
    overlay.expose();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn apply_render_surface_to_video_sink(
    _sink: &gst::Element,
    _surface: &NativeRenderSurface,
) -> Result<(), String> {
    Ok(())
}

fn wait_for_promise(promise: &gst::Promise, operation: &str) -> Result<(), String> {
    match promise.wait() {
        gst::PromiseResult::Replied => {
            if let Some(reply) = promise.get_reply() {
                if reply.has_field("error") {
                    return Err(format!(
                        "GStreamer promise returned an error during {operation}: {}",
                        describe_structure(reply)
                    ));
                }
            }
            Ok(())
        }
        gst::PromiseResult::Interrupted => {
            Err(format!("GStreamer promise interrupted during {operation}."))
        }
        gst::PromiseResult::Expired => {
            Err(format!("GStreamer promise expired during {operation}."))
        }
        gst::PromiseResult::Pending => Err(format!(
            "GStreamer promise still pending during {operation}."
        )),
        other => Err(format!(
            "GStreamer promise failed during {operation}: {other:?}"
        )),
    }
}

fn describe_structure(structure: &gst::StructureRef) -> String {
    let fields = structure
        .iter()
        .map(|(name, value)| {
            let rendered = value
                .get::<&glib::Error>()
                .map(|error| format!("{error:?}"))
                .unwrap_or_else(|_| format!("{value:?}"));
            format!("{}={rendered}", name.as_str())
        })
        .collect::<Vec<_>>();

    format!("{} {{{}}}", structure.name().as_str(), fields.join(", "))
}

fn wire_local_ice_events(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) -> Result<(), String> {
    let Some(event_sender) = event_sender else {
        return Ok(());
    };

    webrtc.connect("on-ice-candidate", false, move |values| {
        let sdp_m_line_index = values
            .get(1)
            .and_then(|value| value.get::<u32>().ok())
            .unwrap_or(0);
        let candidate = values
            .get(2)
            .and_then(|value| value.get::<String>().ok())
            .unwrap_or_default();

        if !candidate.trim().is_empty() {
            let _ = event_sender.send(Event::LocalIce {
                candidate: IceCandidatePayload {
                    candidate,
                    sdp_mid: Some(sdp_m_line_index.to_string()),
                    sdp_m_line_index: Some(sdp_m_line_index),
                    username_fragment: None,
                },
            });
        }

        None
    });
    Ok(())
}

fn wire_webrtc_state_events(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-connection-state",
        "ICE connection state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-gathering-state",
        "ICE gathering state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender,
        "connection-state",
        "peer connection state",
    );
}

fn wire_webrtc_property_event(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    property_name: &'static str,
    label: &'static str,
) {
    if event_sender.is_none() || webrtc.find_property(property_name).is_none() {
        return;
    }

    webrtc.connect_notify(Some(property_name), move |element, _| {
        let value = element.property_value(property_name);
        send_log(
            &event_sender,
            "debug",
            format!("GStreamer WebRTC {label}: {value:?}."),
        );
    });
}

fn wire_remote_data_channels(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    webrtc.connect("on-data-channel", false, move |values| {
        let Some(channel) = values
            .get(1)
            .and_then(|value| value.get::<gst_webrtc::WebRTCDataChannel>().ok())
        else {
            send_log(
                &event_sender,
                "warn",
                "GStreamer emitted on-data-channel without a channel.".to_owned(),
            );
            return None;
        };

        let label = channel_label(&channel);
        send_log(
            &event_sender,
            "info",
            format!(
                "Remote WebRTC data channel received: label={}, ordered={}.",
                label,
                channel.is_ordered()
            ),
        );
        connect_remote_data_channel_callbacks(&label, &channel, event_sender.clone());
        None
    });
}

fn create_input_data_channels(
    webrtc: &gst::Element,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
    partial_reliable_threshold_ms: u32,
) -> Result<GstreamerInputChannels, String> {
    let reliable = create_data_channel(webrtc, RELIABLE_INPUT_CHANNEL_LABEL, None)?;
    connect_input_channel_callbacks(
        RELIABLE_INPUT_CHANNEL_LABEL,
        &reliable,
        input_state.clone(),
        event_sender.clone(),
    );

    let clamped_threshold_ms = if partial_reliable_threshold_ms == 0 {
        DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS
    } else {
        partial_reliable_threshold_ms.clamp(1, 5000)
    };
    let options = gst::Structure::builder("data-channel-options")
        .field("ordered", false)
        .field("max-packet-lifetime", clamped_threshold_ms as i32)
        .build();
    let partially_reliable = create_data_channel(
        webrtc,
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        Some(options),
    )?;
    connect_input_channel_callbacks(
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        &partially_reliable,
        input_state,
        event_sender.clone(),
    );

    send_log(
        &event_sender,
        "info",
        format!(
            "Created WebRTC input data channels ({}, {} maxPacketLifeTime={}ms).",
            RELIABLE_INPUT_CHANNEL_LABEL,
            PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
            clamped_threshold_ms
        ),
    );

    Ok(GstreamerInputChannels {
        reliable,
        partially_reliable,
    })
}

fn create_data_channel(
    webrtc: &gst::Element,
    label: &'static str,
    options: Option<gst::Structure>,
) -> Result<gst_webrtc::WebRTCDataChannel, String> {
    let channel = match options {
        Some(options) => {
            let options = Some(options);
            webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
                "create-data-channel",
                &[&label, &options],
            )
        }
        None => webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
            "create-data-channel",
            &[&label, &None::<gst::Structure>],
        ),
    };

    let actual_label = channel_label(&channel);
    if actual_label != label {
        return Err(format!(
            "GStreamer created data channel with unexpected label: expected {label}, got {actual_label}."
        ));
    }

    Ok(channel)
}

fn connect_input_channel_callbacks(
    label: &'static str,
    channel: &gst_webrtc::WebRTCDataChannel,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let open_sender = event_sender.clone();
    channel.connect_on_open(move |channel| {
        send_log(
            &open_sender,
            "info",
            format!(
                "Input data channel open: label={}, id={}, ordered={}, maxPacketLifeTime={}.",
                label,
                channel.id(),
                channel.is_ordered(),
                channel.max_packet_lifetime()
            ),
        );
    });

    let close_sender = event_sender.clone();
    let close_state = input_state.clone();
    channel.connect_on_close(move |_| {
        if label == RELIABLE_INPUT_CHANNEL_LABEL {
            close_state.ready.store(false, Ordering::SeqCst);
            close_state.heartbeat_stop.store(true, Ordering::SeqCst);
        }
        send_log(
            &close_sender,
            "info",
            format!("Input data channel closed: label={label}."),
        );
    });

    let error_sender = event_sender.clone();
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Input data channel error on {label}: {error}."),
        );
    });

    if label == RELIABLE_INPUT_CHANNEL_LABEL {
        let data_sender = event_sender.clone();
        let data_state = input_state.clone();
        channel.connect_on_message_data(move |channel, data| {
            let Some(bytes) = data else {
                return;
            };
            handle_input_handshake_message(
                channel,
                bytes.as_ref(),
                data_state.clone(),
                data_sender.clone(),
            );
        });

        let string_sender = event_sender.clone();
        let string_state = input_state;
        channel.connect_on_message_string(move |channel, message| {
            let Some(message) = message else {
                return;
            };
            handle_input_handshake_message(
                channel,
                message.as_bytes(),
                string_state.clone(),
                string_sender.clone(),
            );
        });
    }
}

fn connect_remote_data_channel_callbacks(
    label: &str,
    channel: &gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let label = label.to_owned();
    let open_sender = event_sender.clone();
    let open_label = label.clone();
    channel.connect_on_open(move |_| {
        send_log(
            &open_sender,
            "info",
            format!("Remote data channel open: label={open_label}."),
        );
    });

    let close_sender = event_sender.clone();
    let close_label = label.clone();
    channel.connect_on_close(move |_| {
        send_log(
            &close_sender,
            "info",
            format!("Remote data channel closed: label={close_label}."),
        );
    });

    let error_sender = event_sender;
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Remote data channel error on {label}: {error}."),
        );
    });
}

fn handle_input_handshake_message(
    channel: &gst_webrtc::WebRTCDataChannel,
    bytes: &[u8],
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let Some(protocol_version) = parse_input_handshake_version(bytes) else {
        return;
    };

    let encoder_version = protocol_version.min(u8::MAX as u16) as u8;
    if let Ok(mut encoder) = input_state.encoder.lock() {
        encoder.set_protocol_version(encoder_version);
    }
    let was_ready = input_state.ready.swap(true, Ordering::SeqCst);
    if was_ready {
        return;
    }

    send_log(
        &event_sender,
        "info",
        format!(
            "Input handshake complete on {} (protocol v{}).",
            channel_label(channel),
            protocol_version
        ),
    );
    if let Some(sender) = event_sender.as_ref() {
        let _ = sender.send(Event::InputReady { protocol_version });
    }
    start_input_heartbeat(input_state, channel.clone(), event_sender);
}

fn parse_input_handshake_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }

    let first_word = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first_word == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }

    if bytes[0] == 0x0e {
        return Some(first_word);
    }

    None
}

fn start_input_heartbeat(
    input_state: GstreamerInputState,
    channel: gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let Ok(mut heartbeat_thread) = input_state.heartbeat_thread.lock() else {
        send_log(
            &event_sender,
            "warn",
            "Failed to acquire input heartbeat thread lock.".to_owned(),
        );
        return;
    };
    if heartbeat_thread
        .as_ref()
        .is_some_and(|thread| !thread.is_finished())
    {
        return;
    }
    if let Some(thread) = heartbeat_thread.take() {
        let _ = thread.join();
    }

    input_state.heartbeat_stop.store(false, Ordering::SeqCst);
    let encoder = input_state.encoder.clone();
    let stop = input_state.heartbeat_stop.clone();
    let thread_sender = event_sender.clone();
    *heartbeat_thread = Some(thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            send_input_heartbeat(&channel, &encoder, &thread_sender);

            let mut slept = Duration::ZERO;
            while slept < HEARTBEAT_INTERVAL {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let remaining = HEARTBEAT_INTERVAL.saturating_sub(slept);
                let interval = remaining.min(HEARTBEAT_STOP_POLL_INTERVAL);
                thread::sleep(interval);
                slept += interval;
            }
        }
    }));
}

fn send_input_heartbeat(
    channel: &gst_webrtc::WebRTCDataChannel,
    encoder: &Arc<Mutex<InputEncoder>>,
    event_sender: &Option<Sender<Event>>,
) {
    if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
        return;
    }

    let Ok(encoder) = encoder.lock() else {
        send_log(
            event_sender,
            "warn",
            "Failed to acquire input encoder for heartbeat.".to_owned(),
        );
        return;
    };
    let bytes = glib::Bytes::from_owned(encoder.encode_heartbeat());
    channel.send_data(Some(&bytes));
}

fn channel_label(channel: &gst_webrtc::WebRTCDataChannel) -> String {
    channel
        .label()
        .map(|label| label.to_string())
        .unwrap_or_else(|| "<unlabeled>".to_owned())
}

fn send_log(event_sender: &Option<Sender<Event>>, level: &'static str, message: String) {
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::Log { level, message });
    } else {
        eprintln!("[NativeStreamer] {message}");
    }
}

fn wire_incoming_media_sink(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    render_state: GstreamerRenderState,
) {
    let pipeline = pipeline.downgrade();
    let streaming_reported = Arc::new(AtomicBool::new(false));
    webrtc.connect_pad_added(move |_webrtc, src_pad| {
        let Some(pipeline) = pipeline.upgrade() else {
            return;
        };
        let event_sender = event_sender.clone();

        if !is_rtp_pad(src_pad) {
            send_log(
                &event_sender,
                "debug",
                format!(
                    "Ignoring non-RTP WebRTC pad with caps {:?}.",
                    pad_caps_name(src_pad)
                ),
            );
            return;
        }

        let decodebin = match make_element("decodebin") {
            Ok(decodebin) => decodebin,
            Err(error) => {
                send_log(&event_sender, "warn", error);
                return;
            }
        };

        let decode_pipeline = pipeline.downgrade();
        let decode_sender = event_sender.clone();
        let decode_render_state = render_state.clone();
        let decode_streaming_reported = streaming_reported.clone();
        decodebin.connect_pad_added(move |_decodebin, decoded_pad| {
            let Some(pipeline) = decode_pipeline.upgrade() else {
                return;
            };
            let media_kind = decoded_media_kind(decoded_pad);
            if let Err(error) = link_decoded_media_pad(
                &pipeline,
                decoded_pad,
                &decode_render_state,
                &decode_sender,
                &decode_streaming_reported,
            ) {
                send_log(&decode_sender, "warn", error);
                if let Err(fallback_error) =
                    link_decoded_media_to_fakesink(&pipeline, decoded_pad, "decoded media fallback")
                {
                    send_log(&decode_sender, "warn", fallback_error);
                }
                return;
            }

            send_log(
                &decode_sender,
                "info",
                format!(
                    "Linked decoded {} stream to native sink chain.",
                    media_kind.label()
                ),
            );
        });

        if let Err(error) = pipeline.add(&decodebin) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to add decodebin: {error}"),
            );
            return;
        }
        if let Err(error) = decodebin.sync_state_with_parent() {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to sync decodebin state: {error}"),
            );
            return;
        }

        let Some(sink_pad) = decodebin.static_pad("sink") else {
            send_log(
                &event_sender,
                "warn",
                "decodebin has no sink pad.".to_owned(),
            );
            return;
        };
        if let Err(error) = src_pad.link(&sink_pad) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to link WebRTC RTP pad to decodebin: {error:?}"),
            );
        }
    });
}

impl DecodedMediaKind {
    fn label(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Unknown => "unknown",
        }
    }
}

fn is_rtp_pad(pad: &gst::Pad) -> bool {
    pad_caps_name(pad)
        .as_deref()
        .is_some_and(|name| name == "application/x-rtp")
}

fn pad_caps_name(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0)
        .map(|structure| structure.name().to_string())
}

fn decoded_media_kind(pad: &gst::Pad) -> DecodedMediaKind {
    match pad_caps_name(pad).as_deref() {
        Some(name) if name.starts_with("video/") => DecodedMediaKind::Video,
        Some(name) if name.starts_with("audio/") => DecodedMediaKind::Audio,
        _ => DecodedMediaKind::Unknown,
    }
}

fn link_decoded_media_pad(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    match decoded_media_kind(src_pad) {
        DecodedMediaKind::Video => link_media_chain(
            pipeline,
            src_pad,
            &video_sink_factories(),
            "video",
            Some(render_state),
            event_sender,
            streaming_reported,
        ),
        DecodedMediaKind::Audio => link_media_chain(
            pipeline,
            src_pad,
            &[
                ("queue", None),
                ("audioconvert", None),
                ("audioresample", None),
                ("autoaudiosink", Some(false)),
            ],
            "audio",
            None,
            event_sender,
            streaming_reported,
        ),
        DecodedMediaKind::Unknown => Err(format!(
            "Unsupported decoded media caps {:?}; routing to fallback sink.",
            pad_caps_name(src_pad)
        )),
    }
}

fn video_sink_factories() -> Vec<(&'static str, Option<bool>)> {
    #[cfg(target_os = "windows")]
    {
        if gst::ElementFactory::find("d3d11videosink").is_some() {
            return vec![("queue", None), ("d3d11videosink", Some(false))];
        }
    }

    vec![
        ("queue", None),
        ("videoconvert", None),
        ("autovideosink", Some(false)),
    ]
}

fn link_media_chain(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    factories: &[(&str, Option<bool>)],
    media_label: &str,
    render_state: Option<&GstreamerRenderState>,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut elements = Vec::with_capacity(factories.len());
    for (factory, sync_property) in factories {
        let factory = *factory;
        let element = make_element(factory)?;
        if factory == "queue" {
            configure_queue_for_low_latency(&element, media_label);
        }
        if sync_property.is_some() || factory.ends_with("sink") {
            configure_sink_for_low_latency(&element);
        }
        pipeline
            .add(&element)
            .map_err(|error| format!("Failed to add {factory} for {media_label}: {error}"))?;
        elements.push(element);
    }

    for pair in elements.windows(2) {
        pair[0].link(&pair[1]).map_err(|error| {
            format!(
                "Failed to link {} -> {} for {media_label}: {error:?}",
                pair[0]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default(),
                pair[1]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default()
            )
        })?;
    }

    let first = elements
        .first()
        .ok_or_else(|| format!("No elements created for {media_label} sink chain."))?;
    let Some(first_sink_pad) = first.static_pad("sink") else {
        return Err(format!(
            "First {media_label} sink-chain element has no sink pad."
        ));
    };
    src_pad
        .link(&first_sink_pad)
        .map_err(|error| format!("Failed to link decoded {media_label} pad: {error:?}"))?;

    if let Some(sink) = elements.last() {
        if media_label == "video" {
            if let Some(render_state) = render_state {
                render_state.set_video_sink(sink.clone(), event_sender);
            }
        }
        watch_first_sink_buffer(sink, media_label, event_sender, streaming_reported);
    }

    for element in &elements {
        element.sync_state_with_parent().map_err(|error| {
            format!("Failed to sync {media_label} sink-chain element state: {error}")
        })?;
    }

    Ok(())
}

fn watch_first_sink_buffer(
    sink: &gst::Element,
    media_label: &str,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sender = event_sender.clone();
    let label = media_label.to_owned();
    let reported = streaming_reported.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .and_then(|caps| caps.structure(0).map(|structure| structure.to_string()))
            .unwrap_or_else(|| "unknown caps".to_owned());
        send_log(
            &sender,
            "info",
            format!("First decoded {label} buffer reached native sink; caps={caps}."),
        );

        if label == "video" && !reported.swap(true, Ordering::SeqCst) {
            if let Some(event_sender) = &sender {
                let message = if use_external_renderer_window() {
                    "Native video frames reached the external low-latency GStreamer renderer window."
                } else {
                    "Native video frames reached the embedded low-latency GStreamer sink."
                };
                let _ = event_sender.send(Event::Status {
                    status: "streaming",
                    message: Some(message.to_owned()),
                });
            }
        }

        gst::PadProbeReturn::Remove
    });
}

fn link_decoded_media_to_fakesink(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    label: &str,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .map_err(|error| format!("Failed to create {label}: {error}"))?;
    configure_sink_for_low_latency(&sink);
    pipeline
        .add(&sink)
        .map_err(|error| format!("Failed to add {label}: {error}"))?;
    sink.sync_state_with_parent()
        .map_err(|error| format!("Failed to sync {label} state: {error}"))?;

    let Some(sink_pad) = sink.static_pad("sink") else {
        return Err(format!("{label} has no sink pad."));
    };
    src_pad
        .link(&sink_pad)
        .map(|_| ())
        .map_err(|error| format!("Failed to link {label}: {error:?}"))
}

fn make_element(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|error| format!("Failed to create GStreamer element {factory}: {error}"))
}

#[derive(Debug)]
pub struct GstreamerBackend {
    active_context: Option<NativeStreamerSessionContext>,
    pending_remote_ice: Vec<IceCandidatePayload>,
    pipeline: Option<GstreamerPipeline>,
    event_sender: Option<Sender<Event>>,
    remote_description_set: bool,
    render_surface: Option<NativeRenderSurface>,
}

impl GstreamerBackend {
    pub fn new(event_sender: Option<Sender<Event>>) -> Self {
        Self {
            active_context: None,
            pending_remote_ice: Vec::new(),
            pipeline: None,
            event_sender,
            remote_description_set: false,
            render_surface: None,
        }
    }

    fn replay_pending_remote_ice(&mut self) -> Vec<Event> {
        let candidates = std::mem::take(&mut self.pending_remote_ice);
        let Some(pipeline) = self.pipeline.as_mut() else {
            self.pending_remote_ice = candidates;
            return Vec::new();
        };

        let mut events = Vec::new();
        for candidate in candidates {
            if let Err(message) = pipeline.add_remote_ice(&candidate) {
                events.push(Event::Error {
                    code: "remote-ice-failed".to_owned(),
                    message,
                });
            }
        }
        events
    }
}

impl NativeStreamerBackend for GstreamerBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities {
        NativeStreamerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "gstreamer",
            requested_backend: None,
            fallback_reason: None,
            supports_offer_answer: true,
            supports_remote_ice: true,
            supports_local_ice: true,
            supports_input: true,
        }
    }

    fn start(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };

        let session_id = context.session.session_id.clone();
        let pipeline = match GstreamerPipeline::build(self.event_sender.clone()) {
            Ok(pipeline) => pipeline,
            Err(message) => {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-start-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-start-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        if let Some(old_pipeline) = self.pipeline.take() {
            if let Err(message) = old_pipeline.stop() {
                eprintln!("[NativeStreamer] {message}");
            }
        }

        self.active_context = Some(context);
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        let webrtc_name = pipeline.webrtc_name();
        self.pipeline = Some(pipeline);
        if let (Some(surface), Some(pipeline)) =
            (self.render_surface.clone(), self.pipeline.as_ref())
        {
            pipeline.update_render_surface(surface);
        }

        BackendReply {
            events: vec![Event::Status {
                status: "ready",
                message: Some(format!(
                    "GStreamer backend selected for session {session_id}; {} pipeline is ready.",
                    webrtc_name
                )),
            }],
            response: Some(Response::Ok { id }),
            should_continue: true,
        }
    }

    fn handle_offer(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id.clone();
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };
        let Some(offer_sdp) = command.sdp else {
            return BackendReply::response(missing_field(&id, "sdp"));
        };

        let prepared = match prepare_native_offer(&context, &offer_sdp) {
            Ok(prepared) => prepared,
            Err(error) => return BackendReply::response(error.into_response(id)),
        };

        let mut events = prepared_offer_events(&prepared);
        let parsed_offer = match GstreamerPipeline::parse_offer_sdp(&prepared.gstreamer_offer_sdp) {
            Ok(offer) => offer,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-remote-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        let Some(pipeline) = self.pipeline.as_mut() else {
            return BackendReply {
                events,
                response: Some(Response::Error {
                    id: Some(id),
                    code: "gstreamer-not-started".to_owned(),
                    message: "GStreamer pipeline is not started.".to_owned(),
                }),
                should_continue: true,
            };
        };

        let answer_sdp = match pipeline.negotiate_answer(
            parsed_offer,
            (prepared.gstreamer_ice_pwd_replacements > 0)
                .then_some(&prepared.nvst_params.credentials),
            prepared.nvst_params.partial_reliable_threshold_ms,
        ) {
            Ok(answer_sdp) => munge_answer_sdp(&answer_sdp, prepared.nvst_params.max_bitrate_kbps),
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-negotiation-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };
        self.remote_description_set = true;
        events.extend(self.replay_pending_remote_ice());

        events.push(Event::Log {
            level: "info",
            message:
                "GStreamer created a local WebRTC answer and replayed queued remote ICE candidates."
                    .to_owned(),
        });

        let nvst_sdp = match build_nvst_sdp_for_answer(&prepared.nvst_params, &answer_sdp) {
            Ok(nvst_sdp) => nvst_sdp,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-local-answer-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        events.push(Event::Log {
            level: "debug",
            message: "Built native NVST SDP from the local WebRTC answer transport credentials."
                .to_owned(),
        });

        BackendReply {
            events,
            response: Some(Response::Answer {
                id,
                answer: SendAnswerRequest {
                    sdp: answer_sdp,
                    nvst_sdp: Some(nvst_sdp),
                },
            }),
            should_continue: true,
        }
    }

    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(candidate) = command.candidate else {
            return BackendReply::response(missing_field(&command.id, "candidate"));
        };

        if self.remote_description_set {
            if let Some(pipeline) = self.pipeline.as_mut() {
                if let Err(message) = pipeline.add_remote_ice(&candidate) {
                    return BackendReply::response(Response::Error {
                        id: Some(command.id),
                        code: "remote-ice-failed".to_owned(),
                        message,
                    });
                }
            } else {
                self.pending_remote_ice.push(candidate);
            }
        } else {
            self.pending_remote_ice.push(candidate);
        }
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(packet) = command.input else {
            return BackendReply::continue_without_response();
        };

        let Ok(payload) = packet.payload_bytes() else {
            return BackendReply::continue_without_response();
        };

        if payload.is_empty() || payload.len() > 4096 {
            return BackendReply::continue_without_response();
        }

        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.send_input_packet(&payload, packet.partially_reliable);
        }

        BackendReply::continue_without_response()
    }

    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(surface) = command.surface else {
            return BackendReply::response(missing_field(&command.id, "surface"));
        };

        self.render_surface = Some(surface.clone());
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.update_render_surface(surface);
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn stop(&mut self, command: CommandEnvelope) -> BackendReply {
        self.active_context = None;
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        if let Some(pipeline) = self.pipeline.take() {
            if let Err(message) = pipeline.stop() {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-stop-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(command.id),
                        code: "gstreamer-stop-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        }
        let message = command
            .reason
            .unwrap_or_else(|| "stop requested".to_owned());
        BackendReply::stop(command.id, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_stops_webrtc_pipeline() {
        let pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        assert_eq!(pipeline.webrtc.name(), "opennow-webrtcbin");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn parses_basic_remote_offer_sdp() {
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 127.0.0.1\r\na=mid:0\r\na=sctp-port:5000\r\n";
        let parsed = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        assert_eq!(parsed.medias_len(), 1);
    }

    #[test]
    fn defers_gfn_uuid_ice_password_until_actual_ice_stream_exists() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-6cb2-4c18-96d0-963e5e586c9a".to_owned(),
            fingerprint: String::new(),
        };

        pipeline.original_remote_ice_credentials = Some(credentials);
        assert!(!pipeline
            .try_restore_original_remote_ice_credentials("without negotiated streams")
            .expect("remote ICE credential restoration can be deferred"));
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn remote_ice_credential_restore_after_remote_description_does_not_probe_fake_streams() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let sdp = concat!(
            "v=0\r\n",
            "o=- 4373647202393833435 2 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=group:BUNDLE 0 1 2 3\r\n",
            "a=ice-options:trickle\r\n",
            "a=ice-lite\r\n",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:0\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:111 OPUS/48000/2\r\n",
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:1\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:96 H264/90000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:2\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:3\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
        );
        let offer_sdp = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        pipeline
            .pipeline
            .set_state(gst::State::Playing)
            .expect("pipeline plays");
        pipeline
            .set_description("set-remote-description", &offer)
            .expect("remote description");

        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-99a8-4ffa-b9aa-f38ddad1e2b4".to_owned(),
            fingerprint: String::new(),
        };
        pipeline.original_remote_ice_credentials = Some(credentials);
        pipeline
            .try_restore_original_remote_ice_credentials("after remote description")
            .expect("remote ICE credential restoration does not fail without actual streams");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn reports_offer_answer_and_local_ice_capabilities() {
        let backend = GstreamerBackend::new(None);
        let capabilities = backend.capabilities();
        assert!(capabilities.supports_offer_answer);
        assert!(capabilities.supports_local_ice);
        assert!(capabilities.supports_input);
    }

    #[test]
    fn parses_input_handshake_versions() {
        assert_eq!(
            parse_input_handshake_version(&[0x0e, 0x02, 0x03, 0x00]),
            Some(3)
        );
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x02]), Some(2));
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(parse_input_handshake_version(&[0x01, 0x02, 0x03]), None);
        assert_eq!(parse_input_handshake_version(&[0x0e]), None);
    }
}
