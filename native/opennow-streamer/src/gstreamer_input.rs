use crate::gstreamer_backend::send_log;
#[cfg(target_os = "windows")]
use crate::gstreamer_platform::win32_renderer_window;
use crate::input::InputEncoder;
#[cfg(target_os = "windows")]
use crate::input::{
    layout_mapped_keyboard_scancode, GamepadInput, KeyboardPayload, MouseButtonPayload,
    MouseMovePayload, MouseWheelPayload, GAMEPAD_MAX_CONTROLLERS,
    PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
};
use crate::protocol::Event;
#[cfg(target_os = "windows")]
use crate::protocol::NativeStreamerShortcutAction;
use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_webrtc as gst_webrtc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "windows")]
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
#[cfg(target_os = "windows")]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

const RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_v1";
const PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_partially_reliable";
const DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS: u32 = 300;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(target_os = "windows")]
const NATIVE_INPUT_BRIDGE_POLL_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(target_os = "windows")]
const NATIVE_INPUT_DRAIN_MAX_EVENTS: usize = 512;
#[cfg(target_os = "windows")]
const NATIVE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(4);
#[cfg(target_os = "windows")]
const NATIVE_GAMEPAD_KEEPALIVE_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(target_os = "windows")]
static NATIVE_INPUT_STARTED_AT: OnceLock<Instant> = OnceLock::new();

#[derive(Clone)]
pub(crate) struct GstreamerInputState {
    encoder: Arc<Mutex<InputEncoder>>,
    pub(crate) ready: Arc<AtomicBool>,
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
    pub(crate) fn reset(&self) {
        self.ready.store(false, Ordering::SeqCst);
        if let Ok(mut encoder) = self.encoder.lock() {
            encoder.set_protocol_version(2);
            encoder.reset_gamepad_sequences();
        }
    }

    pub(crate) fn stop_heartbeat(&self) {
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

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum NativeWindowInputEvent {
    Shortcut {
        action: NativeStreamerShortcutAction,
    },
    Key {
        pressed: bool,
        keycode: u16,
        scancode: u16,
        modifiers: u16,
        timestamp_us: u64,
    },
    MouseMove {
        dx: i16,
        dy: i16,
        timestamp_us: u64,
    },
    MouseButton {
        pressed: bool,
        button: u8,
        timestamp_us: u64,
    },
    MouseWheel {
        delta: i16,
        timestamp_us: u64,
    },
}

#[cfg(target_os = "windows")]
mod win32_xinput {
    use std::ffi::{c_char, c_void};

    type Dword = u32;
    type Hmodule = *mut c_void;
    type XInputGetStateFn = unsafe extern "system" fn(Dword, *mut XInputStateRaw) -> Dword;

    const ERROR_SUCCESS: Dword = 0;
    const XINPUT_DLLS: [&str; 3] = ["xinput1_4.dll", "xinput9_1_0.dll", "xinput1_3.dll"];

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputGamepadRaw {
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        thumb_lx: i16,
        thumb_ly: i16,
        thumb_rx: i16,
        thumb_ry: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputStateRaw {
        packet_number: Dword,
        gamepad: XInputGamepadRaw,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct XInputGamepadSnapshot {
        pub buttons: u16,
        pub left_trigger: u8,
        pub right_trigger: u8,
        pub left_stick_x: i16,
        pub left_stick_y: i16,
        pub right_stick_x: i16,
        pub right_stick_y: i16,
    }

    #[derive(Clone, Copy)]
    pub struct XInput {
        get_state: XInputGetStateFn,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcAddress(module: Hmodule, proc_name: *const c_char) -> *mut c_void;
        fn LoadLibraryW(filename: *const u16) -> Hmodule;
    }

    impl XInput {
        pub unsafe fn load() -> Option<Self> {
            for dll in XINPUT_DLLS {
                let wide = wide_null(dll);
                let module = LoadLibraryW(wide.as_ptr());
                if module.is_null() {
                    continue;
                }

                let address = GetProcAddress(module, b"XInputGetState\0".as_ptr() as *const c_char);
                if !address.is_null() {
                    return Some(Self {
                        get_state: std::mem::transmute::<*mut c_void, XInputGetStateFn>(address),
                    });
                }
            }

            None
        }

        pub unsafe fn get_state(self, controller_id: u32) -> Option<XInputGamepadSnapshot> {
            let mut state = XInputStateRaw::default();
            if (self.get_state)(controller_id, &mut state) != ERROR_SUCCESS {
                return None;
            }

            Some(XInputGamepadSnapshot {
                buttons: state.gamepad.buttons,
                left_trigger: apply_trigger_deadzone(state.gamepad.left_trigger),
                right_trigger: apply_trigger_deadzone(state.gamepad.right_trigger),
                left_stick_x: apply_stick_deadzone(state.gamepad.thumb_lx, 7849),
                left_stick_y: apply_stick_deadzone(state.gamepad.thumb_ly, 7849),
                right_stick_x: apply_stick_deadzone(state.gamepad.thumb_rx, 8689),
                right_stick_y: apply_stick_deadzone(state.gamepad.thumb_ry, 8689),
            })
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn apply_trigger_deadzone(value: u8) -> u8 {
        if value <= 30 {
            0
        } else {
            value
        }
    }

    fn apply_stick_deadzone(value: i16, deadzone: i16) -> i16 {
        if (value as i32).abs() <= deadzone as i32 {
            0
        } else {
            value
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GstreamerInputChannels {
    reliable: gst_webrtc::WebRTCDataChannel,
    partially_reliable: gst_webrtc::WebRTCDataChannel,
}

impl GstreamerInputChannels {
    pub(crate) fn labels(&self) -> (String, String) {
        (
            channel_label(&self.reliable),
            channel_label(&self.partially_reliable),
        )
    }

    pub(crate) fn send_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if payload.is_empty() {
            return false;
        }

        let channel = if partially_reliable {
            if self.partially_reliable.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
                return false;
            }
            &self.partially_reliable
        } else {
            &self.reliable
        };

        if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
            return false;
        }

        let bytes = glib::Bytes::from_owned(payload.to_vec());
        channel.send_data_full(Some(&bytes)).is_ok()
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub(crate) struct NativeWindowInputBridge {
    stop: Arc<AtomicBool>,
    input_thread: Option<JoinHandle<()>>,
    gamepad_thread: Option<JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl NativeWindowInputBridge {
    pub(crate) fn start(
        input_state: GstreamerInputState,
        input_channels: GstreamerInputChannels,
        event_sender: Option<Sender<Event>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<NativeWindowInputEvent>();
        unsafe {
            win32_renderer_window::set_input_event_sender(Some(sender));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_sender = event_sender.clone();
        let input_thread_state = input_state.clone();
        let input_thread_channels = input_channels.clone();
        let input_thread = thread::spawn(move || {
            let mut pending_events = Vec::with_capacity(NATIVE_INPUT_DRAIN_MAX_EVENTS);
            send_log(
                &thread_sender,
                "info",
                "Native DX11 window input capture bridge armed.".to_owned(),
            );

            while !thread_stop.load(Ordering::SeqCst) {
                match receiver.recv_timeout(NATIVE_INPUT_BRIDGE_POLL_INTERVAL) {
                    Ok(event) => {
                        pending_events.clear();
                        pending_events.push(event);
                        let mut disconnected = false;
                        while pending_events.len() < NATIVE_INPUT_DRAIN_MAX_EVENTS {
                            match receiver.try_recv() {
                                Ok(event) => pending_events.push(event),
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    disconnected = true;
                                    break;
                                }
                            }
                        }
                        send_native_window_input_events(
                            &input_thread_state,
                            &input_thread_channels,
                            &thread_sender,
                            &pending_events,
                        );
                        if disconnected {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        let gamepad_thread = Some(spawn_native_gamepad_thread(
            input_state,
            input_channels,
            event_sender,
            stop.clone(),
        ));

        Self {
            stop,
            input_thread: Some(input_thread),
            gamepad_thread,
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        unsafe {
            win32_renderer_window::release_current_input_capture();
            win32_renderer_window::set_input_event_sender(None);
        }

        if let Some(thread) = self.input_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native window input bridge thread panicked: {error:?}");
            }
        }
        if let Some(thread) = self.gamepad_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native XInput gamepad thread panicked: {error:?}");
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowInputBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "windows")]
fn send_native_window_input_events(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    event_sender: &Option<Sender<Event>>,
    events: &[NativeWindowInputEvent],
) {
    if events.is_empty() {
        return;
    }

    // Forward shortcuts immediately (before input readiness check)
    // Shortcuts are local control and don't need the stream channel
    let mut other_events = Vec::new();
    for event in events.iter().copied() {
        match event {
            NativeWindowInputEvent::Shortcut { action } => {
                if let Some(sender) = event_sender.as_ref() {
                    let _ = sender.send(Event::Shortcut { action });
                }
            }
            _ => {
                other_events.push(event);
            }
        }
    }

    // Only process non-shortcut events if input is ready
    if other_events.is_empty() || !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let Ok(encoder) = input_state.encoder.lock() else {
        return;
    };

    let mut pending_mouse_move: Option<(i32, i32, u64)> = None;
    for event in other_events.iter().copied() {
        if let NativeWindowInputEvent::MouseMove {
            dx,
            dy,
            timestamp_us,
        } = event
        {
            let (pending_dx, pending_dy, pending_timestamp_us) =
                pending_mouse_move.get_or_insert((0, 0, timestamp_us));
            *pending_dx = pending_dx.saturating_add(i32::from(dx));
            *pending_dy = pending_dy.saturating_add(i32::from(dy));
            *pending_timestamp_us = timestamp_us;
            continue;
        }

        flush_pending_mouse_move(&encoder, input_channels, &mut pending_mouse_move);
        send_encoded_native_window_input_event(&encoder, input_channels, event_sender, event);
    }
    flush_pending_mouse_move(&encoder, input_channels, &mut pending_mouse_move);
}

#[cfg(target_os = "windows")]
fn flush_pending_mouse_move(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    pending_mouse_move: &mut Option<(i32, i32, u64)>,
) {
    let Some((mut dx, mut dy, timestamp_us)) = pending_mouse_move.take() else {
        return;
    };

    while dx != 0 || dy != 0 {
        let chunk_dx = dx.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let chunk_dy = dy.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let payload = encoder.encode_mouse_move(MouseMovePayload {
            dx: chunk_dx,
            dy: chunk_dy,
            timestamp_us,
        });
        let _ = input_channels.send_packet(&payload, true);
        dx = dx.saturating_sub(i32::from(chunk_dx));
        dy = dy.saturating_sub(i32::from(chunk_dy));
    }
}

#[cfg(target_os = "windows")]
fn send_encoded_native_window_input_event(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    event_sender: &Option<Sender<Event>>,
    event: NativeWindowInputEvent,
) {
    let (payload, partially_reliable) = match event {
        NativeWindowInputEvent::Shortcut { action } => {
            if let Some(sender) = event_sender.as_ref() {
                let _ = sender.send(Event::Shortcut { action });
            }
            return;
        }
        NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us,
        } => {
            let payload = KeyboardPayload {
                keycode,
                scancode: layout_mapped_keyboard_scancode(scancode),
                modifiers,
                timestamp_us,
            };
            let bytes = if pressed {
                encoder.encode_key_down(payload)
            } else {
                encoder.encode_key_up(payload)
            };
            (bytes, false)
        }
        NativeWindowInputEvent::MouseMove {
            dx,
            dy,
            timestamp_us,
        } => (
            encoder.encode_mouse_move(MouseMovePayload {
                dx,
                dy,
                timestamp_us,
            }),
            true,
        ),
        NativeWindowInputEvent::MouseButton {
            pressed,
            button,
            timestamp_us,
        } => {
            let payload = MouseButtonPayload {
                button,
                timestamp_us,
            };
            let bytes = if pressed {
                encoder.encode_mouse_button_down(payload)
            } else {
                encoder.encode_mouse_button_up(payload)
            };
            (bytes, false)
        }
        NativeWindowInputEvent::MouseWheel {
            delta,
            timestamp_us,
        } => (
            encoder.encode_mouse_wheel(MouseWheelPayload {
                delta,
                timestamp_us,
            }),
            false,
        ),
    };

    let _ = input_channels.send_packet(&payload, partially_reliable);
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NativeGamepadSnapshot {
    connected: bool,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_stick_x: i16,
    left_stick_y: i16,
    right_stick_x: i16,
    right_stick_y: i16,
}

#[cfg(target_os = "windows")]
impl NativeGamepadSnapshot {
    fn from_xinput(snapshot: win32_xinput::XInputGamepadSnapshot) -> Self {
        Self {
            connected: true,
            buttons: snapshot.buttons,
            left_trigger: snapshot.left_trigger,
            right_trigger: snapshot.right_trigger,
            left_stick_x: snapshot.left_stick_x,
            left_stick_y: snapshot.left_stick_y,
            right_stick_x: snapshot.right_stick_x,
            right_stick_y: snapshot.right_stick_y,
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_native_gamepad_thread(
    input_state: GstreamerInputState,
    input_channels: GstreamerInputChannels,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let Some(xinput) = (unsafe { win32_xinput::XInput::load() }) else {
            send_log(
                &event_sender,
                "warn",
                "Native XInput gamepad bridge unavailable; controller input will require the web renderer fallback.".to_owned(),
            );
            return;
        };

        send_log(
            &event_sender,
            "info",
            "Native XInput gamepad bridge armed.".to_owned(),
        );

        let mut previous = [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
        let mut last_sent = [Instant::now(); GAMEPAD_MAX_CONTROLLERS as usize];

        while !stop.load(Ordering::SeqCst) {
            if input_state.ready.load(Ordering::SeqCst) {
                let mut snapshots =
                    [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
                let mut bitmap = 0u16;

                for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                    if let Some(snapshot) = unsafe { xinput.get_state(controller_id as u32) } {
                        snapshots[controller_id] = NativeGamepadSnapshot::from_xinput(snapshot);
                        bitmap |= 1 << controller_id;
                    }
                }

                for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                    let snapshot = snapshots[controller_id];
                    let state_changed = snapshot != previous[controller_id];
                    let keepalive_due = snapshot.connected
                        && last_sent[controller_id].elapsed() >= NATIVE_GAMEPAD_KEEPALIVE_INTERVAL;

                    if state_changed || keepalive_due {
                        send_native_gamepad_snapshot(
                            &input_state,
                            &input_channels,
                            controller_id as u8,
                            bitmap,
                            snapshot,
                        );
                        last_sent[controller_id] = Instant::now();

                        if snapshot.connected != previous[controller_id].connected {
                            send_log(
                                &event_sender,
                                "info",
                                format!(
                                    "Native XInput controller {controller_id} {}.",
                                    if snapshot.connected {
                                        "connected"
                                    } else {
                                        "disconnected"
                                    }
                                ),
                            );
                        }
                    }

                    previous[controller_id] = snapshot;
                }
            }

            thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
        }
    })
}

#[cfg(target_os = "windows")]
fn send_native_gamepad_snapshot(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    controller_id: u8,
    bitmap: u16,
    snapshot: NativeGamepadSnapshot,
) {
    if !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let use_partially_reliable =
        (PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL & (1_u32 << u32::from(controller_id))) != 0;
    let input = GamepadInput {
        controller_id,
        buttons: snapshot.buttons,
        left_trigger: snapshot.left_trigger,
        right_trigger: snapshot.right_trigger,
        left_stick_x: snapshot.left_stick_x,
        left_stick_y: snapshot.left_stick_y,
        right_stick_x: snapshot.right_stick_x,
        right_stick_y: snapshot.right_stick_y,
        connected: snapshot.connected,
        timestamp_us: native_input_timestamp_us(),
    };

    let Ok(mut encoder) = input_state.encoder.lock() else {
        return;
    };
    let payload = encoder.encode_gamepad_state(bitmap, input, use_partially_reliable);
    drop(encoder);

    let _ = input_channels.send_packet(&payload, use_partially_reliable);
}

#[cfg(target_os = "windows")]
fn native_input_timestamp_us() -> u64 {
    NATIVE_INPUT_STARTED_AT
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

pub(crate) fn wire_remote_data_channels(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) {
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

pub(crate) fn create_input_data_channels(
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

pub(crate) fn parse_input_handshake_version(bytes: &[u8]) -> Option<u16> {
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
    if let Err(error) = channel.send_data_full(Some(&bytes)) {
        send_log(
            event_sender,
            "warn",
            format!("Failed to send input heartbeat: {error}."),
        );
    }
}

pub(crate) fn channel_label(channel: &gst_webrtc::WebRTCDataChannel) -> String {
    channel
        .label()
        .map(|label| label.to_string())
        .unwrap_or_else(|| "<unlabeled>".to_owned())
}
