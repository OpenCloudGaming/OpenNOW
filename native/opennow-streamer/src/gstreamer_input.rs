use crate::gstreamer_backend::send_log;
#[cfg(target_os = "linux")]
use crate::gstreamer_platform::{
    linux_fullscreen_renderer_window_focused, linux_renderer_capture_focus_lost,
    linux_x11_fullscreen_overlay_active,
};
#[cfg(all(feature = "gstreamer", target_os = "linux"))]
use crate::linux_wayland_renderer;
#[cfg(target_os = "linux")]
use crate::linux_cursor::{linux_hide_stream_cursor, linux_show_stream_cursor};
#[cfg(target_os = "linux")]
use crate::linux_display_session::{
    detect_linux_display_session, linux_display_session_label, linux_uses_evdev_mouse_grab,
    linux_uses_wayland_native_input_refocus, LinuxDisplaySession,
};
#[cfg(target_os = "windows")]
use crate::gstreamer_platform::win32_renderer_window;
use crate::input::InputEncoder;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use crate::input::{
    GamepadInput, KeyboardPayload, MouseButtonPayload, MouseMovePayload, MouseWheelPayload,
    GAMEPAD_MAX_CONTROLLERS, PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
};
use crate::protocol::Event;
use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_webrtc as gst_webrtc;
#[cfg(target_os = "linux")]
use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::fs::{canonicalize, read_dir, File, OpenOptions};
#[cfg(target_os = "linux")]
use std::io::{ErrorKind, Read};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "windows")]
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
#[cfg(any(target_os = "windows", target_os = "linux"))]
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
#[cfg(any(target_os = "windows", target_os = "linux"))]
const NATIVE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(4);
#[cfg(any(target_os = "windows", target_os = "linux"))]
const NATIVE_GAMEPAD_KEEPALIVE_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(any(target_os = "windows", target_os = "linux"))]
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
#[derive(Debug, Clone, Copy)]
pub(crate) enum NativeWindowInputEvent {
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

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct NativeWindowInputBridge {
    stop: Arc<AtomicBool>,
    input_thread: Option<JoinHandle<()>>,
    gamepad_thread: Option<JoinHandle<()>>,
}

#[cfg(target_os = "linux")]
impl NativeWindowInputBridge {
    pub(crate) fn start(
        input_state: GstreamerInputState,
        input_channels: GstreamerInputChannels,
        event_sender: Option<Sender<Event>>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let input_thread = Some(spawn_linux_keyboard_mouse_thread(
            input_state.clone(),
            input_channels.clone(),
            event_sender.clone(),
            stop.clone(),
        ));
        let gamepad_thread = Some(spawn_linux_gamepad_thread(
            input_state,
            input_channels,
            event_sender,
            stop.clone(),
        ));

        Self {
            stop,
            input_thread,
            gamepad_thread,
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.input_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!(
                    "[NativeStreamer] Native Linux keyboard/mouse thread panicked: {error:?}"
                );
            }
        }
        if let Some(thread) = self.gamepad_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native Linux gamepad thread panicked: {error:?}");
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for NativeWindowInputBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
#[derive(Debug)]
pub(crate) struct NativeWindowInputBridge;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
impl NativeWindowInputBridge {
    pub(crate) fn start(
        _input_state: GstreamerInputState,
        _input_channels: GstreamerInputChannels,
        event_sender: Option<Sender<Event>>,
    ) -> Self {
        send_log(
            &event_sender,
            "warn",
            format!(
                "Native OS-level input capture is not implemented for {}; Electron input forwarding remains active.",
                std::env::consts::OS
            ),
        );
        Self
    }

    pub(crate) fn stop(&mut self) {}
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn send_native_window_input_events(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    events: &[NativeWindowInputEvent],
) {
    if events.is_empty() || !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let Ok(encoder) = input_state.encoder.lock() else {
        return;
    };

    let mut pending_mouse_move: Option<(i32, i32, u64)> = None;
    for event in events.iter().copied() {
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
        send_encoded_native_window_input_event(&encoder, input_channels, event);
    }
    flush_pending_mouse_move(&encoder, input_channels, &mut pending_mouse_move);
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn send_encoded_native_window_input_event(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    event: NativeWindowInputEvent,
) {
    let (payload, partially_reliable) = match event {
        NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us,
        } => {
            let payload = KeyboardPayload {
                keycode,
                scancode,
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
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

impl NativeGamepadSnapshot {
    #[cfg(target_os = "windows")]
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
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

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn native_input_timestamp_us() -> u64 {
    NATIVE_INPUT_STARTED_AT
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct LinuxInputDevice {
    path: PathBuf,
    file: File,
    grabbed: bool,
}

#[cfg(target_os = "linux")]
impl Drop for LinuxInputDevice {
    fn drop(&mut self) {
        if self.grabbed {
            linux_evdev_grab(&self.file, false);
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct LinuxGamepadDevice {
    path: PathBuf,
    file: File,
    controller_id: u8,
    snapshot: NativeGamepadSnapshot,
    last_snapshot_sent: NativeGamepadSnapshot,
    last_sent: Instant,
}

#[cfg(target_os = "linux")]
fn spawn_linux_keyboard_mouse_thread(
    input_state: GstreamerInputState,
    input_channels: GstreamerInputChannels,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        log_linux_native_input_status(&event_sender);
        let mut devices = discover_linux_keyboard_mouse_devices(&event_sender);
        if devices.is_empty() {
            send_log(
                &event_sender,
                "error",
                "Native Linux keyboard/mouse capture could not open any evdev keyboard or mouse devices. Native mode requires /dev/input permissions.".to_owned(),
            );
            return;
        }

        let uses_evdev_grab = linux_uses_evdev_mouse_grab();
        send_log(
            &event_sender,
            "info",
            format!(
                "Native Linux evdev capture armed with {} device(s); {} keyboard stays compositor-aware until forwarded.",
                devices.len(),
                if uses_evdev_grab {
                    "mouse uses exclusive kernel grab and"
                } else {
                    "Wayland session uses compositor-aware mouse forwarding without kernel grab and"
                }
            ),
        );

        let mut pending_events = Vec::with_capacity(512);
        let mut pressed_keys = HashSet::<(u16, u16, u16)>::new();
        let mut pressed_buttons = HashSet::<u8>::new();
        let mut forwarded_keys = HashSet::<(u16, u16, u16)>::new();
        let mut captured = !linux_uses_wayland_native_input_refocus();
        if captured {
            linux_begin_local_capture(&mut devices, &event_sender);
        } else {
            send_log(
                &event_sender,
                "info",
                "Native Linux capture waiting for renderer focus; click the stream to capture mouse and keyboard like a fullscreen game.".to_owned(),
            );
        }
        while !stop.load(Ordering::SeqCst) {
            pending_events.clear();
            poll_linux_keyboard_mouse_devices(&mut devices, &mut pending_events);
            if !captured {
                let wayland_refocus = linux_wayland_renderer_refocus_signal(&pending_events);
                let renderer_refocused =
                    linux_fullscreen_renderer_window_focused() || wayland_refocus;
                if renderer_refocused {
                    if wayland_refocus {
                        pending_events.clear();
                    }
                    linux_begin_local_capture(&mut devices, &event_sender);
                    captured = true;
                    send_log(
                        &event_sender,
                        "info",
                        "Native Linux keyboard/mouse capture restored after renderer focus returned."
                            .to_owned(),
                    );
                }
                thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
                continue;
            }

            if captured {
                if linux_renderer_capture_focus_lost() {
                    linux_release_local_capture(
                        &input_state,
                        &input_channels,
                        &mut devices,
                        &mut pressed_keys,
                        &mut pressed_buttons,
                        &mut forwarded_keys,
                        LinuxCaptureReleaseReason::FocusLoss,
                        &event_sender,
                    );
                    captured = false;
                    thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
                    continue;
                }
            }

            if pending_events.is_empty() {
                thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
                continue;
            }

            let mut shortcut_released = false;
            for event in &pending_events {
                record_linux_pressed_input(
                    std::slice::from_ref(event),
                    &mut pressed_keys,
                    &mut pressed_buttons,
                );

                if let Some(reason) = linux_capture_release_reason(&pressed_keys) {
                    linux_release_local_capture(
                        &input_state,
                        &input_channels,
                        &mut devices,
                        &mut pressed_keys,
                        &mut pressed_buttons,
                        &mut forwarded_keys,
                        reason,
                        &event_sender,
                    );
                    captured = false;
                    shortcut_released = true;
                    break;
                }

                if linux_should_block_stream_keyboard_event(event, &pressed_keys) {
                    continue;
                }

                if let NativeWindowInputEvent::Key {
                    pressed: true,
                    keycode,
                    scancode,
                    modifiers,
                    ..
                } = event
                {
                    forwarded_keys.insert((*keycode, *scancode, *modifiers));
                } else if let NativeWindowInputEvent::Key {
                    pressed: false,
                    keycode,
                    scancode,
                    modifiers,
                    ..
                } = event
                {
                    forwarded_keys.remove(&(*keycode, *scancode, *modifiers));
                }

                send_native_window_input_events(
                    &input_state,
                    &input_channels,
                    std::slice::from_ref(event),
                );
            }

            if shortcut_released {
                thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
                continue;
            }
            thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
        }

        linux_show_stream_cursor();
        linux_send_stream_release_for_forwarded_keys(
            &input_state,
            &input_channels,
            &forwarded_keys,
        );
    })
}

#[cfg(target_os = "linux")]
fn log_linux_native_input_status(event_sender: &Option<Sender<Event>>) {
    let session = detect_linux_display_session();
    match session {
        LinuxDisplaySession::Wayland => {
            send_log(
                event_sender,
                "info",
                "Linux Wayland session detected; native video uses Wayland sinks/fullscreen and compositor-aware click-to-capture input without kernel mouse grab.".to_owned(),
            );
        }
        LinuxDisplaySession::X11 => {
            send_log(
                event_sender,
                "info",
                "Linux X11 session detected; native video uses X11 fullscreen overlay when available and game-style mouse grab with compositor-aware keyboard forwarding.".to_owned(),
            );
        }
        LinuxDisplaySession::Unknown => {
            send_log(
                event_sender,
                "warn",
                format!(
                    "Linux display server could not be identified (session={}); native input will still attempt evdev device grabs.",
                    linux_display_session_label(session)
                ),
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_wayland_renderer_refocus_signal(
    pending_events: &[NativeWindowInputEvent],
) -> bool {
    if !linux_uses_wayland_native_input_refocus() || linux_x11_fullscreen_overlay_active() {
        return false;
    }

    if linux_wayland_renderer::linux_wayland_renderer_consume_capture_click() {
        return true;
    }

    if !linux_wayland_renderer::linux_wayland_renderer_video_sink_attached() {
        return pending_events.iter().any(|event| {
            matches!(
                event,
                NativeWindowInputEvent::MouseButton {
                    pressed: true,
                    ..
                }
            )
        });
    }

    false
}

#[cfg(target_os = "linux")]
fn discover_linux_keyboard_mouse_devices(
    event_sender: &Option<Sender<Event>>,
) -> Vec<LinuxInputDevice> {
    let mut devices = Vec::new();
    for path in linux_keyboard_mouse_event_candidates() {
        match OpenOptions::new()
            .read(true)
            .custom_flags(0x800)
            .open(&path)
        {
            Ok(file) => {
                send_log(
                    event_sender,
                    "info",
                    format!("Native Linux evdev capture opened {}.", path.display()),
                );
                devices.push(LinuxInputDevice {
                    path,
                    file,
                    grabbed: false,
                });
            }
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                send_log(
                    event_sender,
                    "error",
                    format!(
                        "Native Linux evdev capture cannot open {}; grant input device permissions.",
                        path.display()
                    ),
                );
            }
            Err(error) => {
                send_log(
                    event_sender,
                    "debug",
                    format!(
                        "Native Linux evdev capture skipped {}: {error}.",
                        path.display()
                    ),
                );
            }
        }
    }
    devices
}

#[cfg(target_os = "linux")]
fn release_linux_keyboard_mouse_devices(devices: &mut [LinuxInputDevice]) {
    if !linux_uses_evdev_mouse_grab() {
        return;
    }

    for device in devices {
        if device.grabbed {
            linux_evdev_grab(&device.file, false);
            device.grabbed = false;
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_begin_local_capture(
    devices: &mut [LinuxInputDevice],
    event_sender: &Option<Sender<Event>>,
) {
    if linux_uses_evdev_mouse_grab() {
        grab_linux_mouse_devices(devices);
    } else if crate::gstreamer_config::use_wayland_owned_renderer()
        && linux_wayland_renderer::linux_wayland_renderer_is_active()
    {
        if let Err(error) = linux_wayland_renderer::linux_wayland_renderer_lock_pointer() {
            send_log(
                event_sender,
                "warn",
                format!(
                    "Native Wayland compositor pointer lock could not be activated yet: {error}"
                ),
            );
        }
        linux_wayland_renderer::linux_wayland_renderer_set_capture_active(true);
    }
    if linux_should_hide_stream_cursor() {
        linux_hide_stream_cursor();
    }
}

#[cfg(target_os = "linux")]
fn linux_should_hide_stream_cursor() -> bool {
    linux_uses_evdev_mouse_grab()
        || linux_x11_fullscreen_overlay_active()
        || (linux_wayland_renderer::linux_wayland_renderer_is_active()
            && linux_uses_wayland_native_input_refocus())
}

#[cfg(target_os = "linux")]
fn linux_input_device_is_mouse(path: &Path) -> bool {
    path.to_string_lossy().contains("event-mouse")
}

#[cfg(target_os = "linux")]
fn grab_linux_mouse_devices(devices: &mut [LinuxInputDevice]) {
    if !linux_uses_evdev_mouse_grab() {
        return;
    }

    for device in devices {
        if device.grabbed || !linux_input_device_is_mouse(&device.path) {
            continue;
        }
        linux_evdev_grab(&device.file, true);
        device.grabbed = true;
    }
}

#[cfg(target_os = "linux")]
fn linux_keyboard_mouse_event_candidates() -> Vec<PathBuf> {
    linux_event_candidates_by_name(|name| {
        (name.contains("event-kbd") || name.contains("event-mouse"))
            && !name.contains("event-joystick")
    })
}

#[cfg(target_os = "linux")]
fn poll_linux_keyboard_mouse_devices(
    devices: &mut [LinuxInputDevice],
    output: &mut Vec<NativeWindowInputEvent>,
) {
    let mut buffer = [0u8; 24 * 64];
    for device in devices {
        loop {
            match device.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    for event in buffer[..bytes_read].chunks_exact(24) {
                        if let Some(input_event) = linux_evdev_event_to_native_window_event(event) {
                            output.push(input_event);
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_evdev_event_to_native_window_event(event: &[u8]) -> Option<NativeWindowInputEvent> {
    if event.len() < 24 {
        return None;
    }
    let event_type = u16::from_ne_bytes([event[16], event[17]]);
    let code = u16::from_ne_bytes([event[18], event[19]]);
    let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);
    let timestamp_us = native_input_timestamp_us();

    match event_type {
        0x01 if (0x110..=0x117).contains(&code) => Some(NativeWindowInputEvent::MouseButton {
            pressed: value != 0,
            button: linux_mouse_button_to_gfn_button(code)?,
            timestamp_us,
        }),
        0x01 if code < 0x110 && value != 2 => {
            let (keycode, scancode, modifiers) = linux_keyboard_code_to_gfn_key(code)?;
            Some(NativeWindowInputEvent::Key {
                pressed: value != 0,
                keycode,
                scancode,
                modifiers,
                timestamp_us,
            })
        }
        0x02 => match code {
            0x00 => Some(NativeWindowInputEvent::MouseMove {
                dx: clamp_linux_rel_i16(value),
                dy: 0,
                timestamp_us,
            }),
            0x01 => Some(NativeWindowInputEvent::MouseMove {
                dx: 0,
                dy: clamp_linux_rel_i16(value),
                timestamp_us,
            }),
            0x08 => Some(NativeWindowInputEvent::MouseWheel {
                delta: clamp_linux_rel_i16(value.saturating_mul(120)),
                timestamp_us,
            }),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn linux_mouse_button_to_gfn_button(code: u16) -> Option<u8> {
    match code {
        0x110 => Some(1),
        0x111 => Some(3),
        0x112 => Some(2),
        0x113 => Some(4),
        0x114 => Some(5),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn linux_keyboard_code_to_gfn_key(code: u16) -> Option<(u16, u16, u16)> {
    let keycode = match code {
        1 => 0x1b,
        2 => 0x31,
        3 => 0x32,
        4 => 0x33,
        5 => 0x34,
        6 => 0x35,
        7 => 0x36,
        8 => 0x37,
        9 => 0x38,
        10 => 0x39,
        11 => 0x30,
        12 => 0xbd,
        13 => 0xbb,
        14 => 0x08,
        15 => 0x09,
        16 => 0x51,
        17 => 0x57,
        18 => 0x45,
        19 => 0x52,
        20 => 0x54,
        21 => 0x59,
        22 => 0x55,
        23 => 0x49,
        24 => 0x4f,
        25 => 0x50,
        26 => 0xdb,
        27 => 0xdd,
        28 => 0x0d,
        29 | 97 => 0x11,
        30 => 0x41,
        31 => 0x53,
        32 => 0x44,
        33 => 0x46,
        34 => 0x47,
        35 => 0x48,
        36 => 0x4a,
        37 => 0x4b,
        38 => 0x4c,
        39 => 0xba,
        40 => 0xde,
        41 => 0xc0,
        42 | 54 => 0x10,
        43 => 0xdc,
        44 => 0x5a,
        45 => 0x58,
        46 => 0x43,
        47 => 0x56,
        48 => 0x42,
        49 => 0x4e,
        50 => 0x4d,
        51 => 0xbc,
        52 => 0xbe,
        53 => 0xbf,
        56 | 100 => 0x12,
        57 => 0x20,
        58 => 0x14,
        59..=68 => 0x70 + code - 59,
        87 => 0x79,
        88 => 0x7a,
        69 => 0x90,
        71..=73 => 0x67 + code - 71,
        74 => 0x6d,
        75..=77 => 0x64 + code - 75,
        78 => 0x6b,
        79..=81 => 0x61 + code - 79,
        82 => 0x60,
        83 => 0x6e,
        96 => 0x0d,
        102 => 0x24,
        103 => 0x26,
        105 => 0x25,
        106 => 0x27,
        107 => 0x23,
        108 => 0x28,
        110 => 0x2d,
        111 => 0x2e,
        119 => 0x13,
        125 | 126 => 0x5b,
        _ => return None,
    };
    let modifiers = match code {
        42 | 54 => 0x0001,
        29 | 97 => 0x0002,
        56 | 100 => 0x0004,
        125 | 126 => 0x0008,
        _ => 0,
    };
    Some((keycode, code, modifiers))
}

#[cfg(target_os = "linux")]
fn linux_evdev_grab(file: &File, grab: bool) {
    const EVIOCGRAB: u64 = 0x4004_4590;
    unsafe extern "C" {
        fn ioctl(fd: i32, request: u64, ...) -> i32;
    }
    let value: i32 = if grab { 1 } else { 0 };
    unsafe {
        let _ = ioctl(file.as_raw_fd(), EVIOCGRAB, &value);
    }
}

#[cfg(target_os = "linux")]
fn record_linux_pressed_input(
    events: &[NativeWindowInputEvent],
    pressed_keys: &mut HashSet<(u16, u16, u16)>,
    pressed_buttons: &mut HashSet<u8>,
) {
    for event in events {
        match *event {
            NativeWindowInputEvent::Key {
                pressed,
                keycode,
                scancode,
                modifiers,
                ..
            } => {
                if pressed {
                    pressed_keys.insert((keycode, scancode, modifiers));
                } else {
                    pressed_keys.remove(&(keycode, scancode, modifiers));
                }
            }
            NativeWindowInputEvent::MouseButton {
                pressed, button, ..
            } => {
                if pressed {
                    pressed_buttons.insert(button);
                } else {
                    pressed_buttons.remove(&button);
                }
            }
            NativeWindowInputEvent::MouseMove { .. }
            | NativeWindowInputEvent::MouseWheel { .. } => {}
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxCaptureReleaseReason {
    AltTab,
    CtrlAltEsc,
    FocusLoss,
    WindowSwitch,
}

#[cfg(target_os = "linux")]
fn linux_capture_release_reason(
    pressed_keys: &HashSet<(u16, u16, u16)>,
) -> Option<LinuxCaptureReleaseReason> {
    let alt = linux_alt_modifier_pressed(pressed_keys);
    let ctrl = pressed_keys
        .iter()
        .any(|(_, scancode, _)| *scancode == 29 || *scancode == 97);
    let tab = pressed_keys.iter().any(|(_, scancode, _)| *scancode == 15);
    let escape = pressed_keys.iter().any(|(_, scancode, _)| *scancode == 1);
    if alt && tab {
        Some(LinuxCaptureReleaseReason::AltTab)
    } else if ctrl && alt && escape {
        Some(LinuxCaptureReleaseReason::CtrlAltEsc)
    } else if linux_window_switch_modifier_pressed(pressed_keys) {
        Some(LinuxCaptureReleaseReason::WindowSwitch)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_window_switch_modifier_pressed(pressed_keys: &HashSet<(u16, u16, u16)>) -> bool {
    pressed_keys.iter().any(|(_, scancode, _)| {
        matches!(*scancode, 125 | 126 | 367 | 368)
    })
}

#[cfg(target_os = "linux")]
fn linux_alt_modifier_pressed(pressed_keys: &HashSet<(u16, u16, u16)>) -> bool {
    pressed_keys
        .iter()
        .any(|(_, scancode, _)| *scancode == 56 || *scancode == 100)
}

#[cfg(target_os = "linux")]
fn linux_should_block_stream_keyboard_event(
    event: &NativeWindowInputEvent,
    pressed_keys: &HashSet<(u16, u16, u16)>,
) -> bool {
    let NativeWindowInputEvent::Key {
        pressed: true,
        scancode,
        ..
    } = event
    else {
        return false;
    };

    if matches!(*scancode, 56 | 100) {
        return true;
    }

    if linux_window_switch_modifier_pressed(pressed_keys) {
        return true;
    }

    if *scancode == 15 && linux_alt_modifier_pressed(pressed_keys) {
        return true;
    }

    if linux_ctrl_alt_esc_shortcut_forming(pressed_keys, *scancode) {
        return true;
    }

    false
}

#[cfg(target_os = "linux")]
fn linux_ctrl_alt_esc_shortcut_forming(
    pressed_keys: &HashSet<(u16, u16, u16)>,
    scancode: u16,
) -> bool {
    let ctrl = pressed_keys
        .iter()
        .any(|(_, pressed_scancode, _)| *pressed_scancode == 29 || *pressed_scancode == 97)
        || matches!(scancode, 29 | 97);
    let alt =
        linux_alt_modifier_pressed(pressed_keys) || matches!(scancode, 56 | 100);
    let escape = pressed_keys
        .iter()
        .any(|(_, pressed_scancode, _)| *pressed_scancode == 1)
        || scancode == 1;
    ctrl
        && alt
        && escape
        && matches!(scancode, 1 | 29 | 97 | 56 | 100)
}

#[cfg(target_os = "linux")]
fn linux_release_local_capture(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    devices: &mut [LinuxInputDevice],
    pressed_keys: &mut HashSet<(u16, u16, u16)>,
    pressed_buttons: &mut HashSet<u8>,
    forwarded_keys: &mut HashSet<(u16, u16, u16)>,
    reason: LinuxCaptureReleaseReason,
    event_sender: &Option<Sender<Event>>,
) {
    linux_send_stream_release_for_forwarded_keys(input_state, input_channels, forwarded_keys);
    forwarded_keys.clear();
    release_linux_keyboard_mouse_devices(devices);
    if crate::gstreamer_config::use_wayland_owned_renderer()
        && linux_wayland_renderer::linux_wayland_renderer_is_active()
    {
        linux_wayland_renderer::linux_wayland_renderer_set_capture_active(false);
        linux_wayland_renderer::linux_wayland_renderer_unlock_pointer();
    }
    linux_show_stream_cursor();
    pressed_keys.clear();
    pressed_buttons.clear();

    let message = match reason {
        LinuxCaptureReleaseReason::AltTab => {
            "Native Linux capture released by Alt+Tab; mouse and keyboard returned to the desktop while streaming continues."
        }
        LinuxCaptureReleaseReason::CtrlAltEsc => {
            "Native Linux capture released by Ctrl+Alt+Esc; mouse and keyboard returned to the desktop while streaming continues."
        }
        LinuxCaptureReleaseReason::FocusLoss => {
            "Native Linux capture released because the renderer lost focus; mouse and keyboard returned to the desktop while streaming continues."
        }
        LinuxCaptureReleaseReason::WindowSwitch => {
            "Native Linux capture released by a desktop window-switch key; mouse and keyboard returned to the desktop while streaming continues."
        }
    };
    send_log(event_sender, "info", message.to_owned());
}

#[cfg(target_os = "linux")]
fn linux_send_stream_release_for_forwarded_keys(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    forwarded_keys: &HashSet<(u16, u16, u16)>,
) {
    if forwarded_keys.is_empty() {
        return;
    }

    let timestamp_us = native_input_timestamp_us();
    let release_events = forwarded_keys
        .iter()
        .map(
            |(keycode, scancode, modifiers)| NativeWindowInputEvent::Key {
                pressed: false,
                keycode: *keycode,
                scancode: *scancode,
                modifiers: *modifiers,
                timestamp_us,
            },
        )
        .collect::<Vec<_>>();
    send_native_window_input_events(input_state, input_channels, &release_events);
}

#[cfg(target_os = "linux")]
fn spawn_linux_gamepad_thread(
    input_state: GstreamerInputState,
    input_channels: GstreamerInputChannels,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        send_log(
            &event_sender,
            "info",
            "Native Linux evdev gamepad polling armed.".to_owned(),
        );

        let mut devices: Vec<LinuxGamepadDevice> = Vec::new();
        let mut permission_warning_logged = false;
        let mut last_rescan = Instant::now() - Duration::from_secs(1);

        while !stop.load(Ordering::SeqCst) {
            if last_rescan.elapsed() >= Duration::from_secs(1) {
                discover_linux_gamepads(
                    &mut devices,
                    &event_sender,
                    &mut permission_warning_logged,
                );
                last_rescan = Instant::now();
            }

            if input_state.ready.load(Ordering::SeqCst) {
                poll_linux_gamepads(&mut devices);
                let bitmap = devices
                    .iter()
                    .filter(|device| device.snapshot.connected)
                    .fold(0u16, |bitmap, device| bitmap | (1 << device.controller_id));

                for device in &mut devices {
                    let state_changed = device.snapshot != device.last_snapshot_sent;
                    let keepalive_due = device.snapshot.connected
                        && device.last_sent.elapsed() >= NATIVE_GAMEPAD_KEEPALIVE_INTERVAL;
                    if state_changed || keepalive_due {
                        send_native_gamepad_snapshot(
                            &input_state,
                            &input_channels,
                            device.controller_id,
                            bitmap,
                            device.snapshot,
                        );
                        device.last_snapshot_sent = device.snapshot;
                        device.last_sent = Instant::now();
                    }
                }
            }

            thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
        }
    })
}

#[cfg(target_os = "linux")]
fn discover_linux_gamepads(
    devices: &mut Vec<LinuxGamepadDevice>,
    event_sender: &Option<Sender<Event>>,
    permission_warning_logged: &mut bool,
) {
    let candidates = linux_gamepad_event_candidates();
    if candidates.is_empty() {
        if !*permission_warning_logged {
            send_log(
                event_sender,
                "info",
                "No Linux evdev gamepad devices were found under /dev/input/by-id or /dev/input/by-path.".to_owned(),
            );
            *permission_warning_logged = true;
        }
        return;
    }

    for path in candidates {
        if devices.iter().any(|device| device.path == path) {
            continue;
        }
        if devices.len() >= GAMEPAD_MAX_CONTROLLERS as usize {
            break;
        }

        match OpenOptions::new()
            .read(true)
            .custom_flags(0x800)
            .open(&path)
        {
            Ok(file) => {
                let controller_id = devices.len() as u8;
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Native Linux evdev input device {} opened as controller {controller_id}.",
                        path.display()
                    ),
                );
                devices.push(LinuxGamepadDevice {
                    path,
                    file,
                    controller_id,
                    snapshot: NativeGamepadSnapshot::default(),
                    last_snapshot_sent: NativeGamepadSnapshot::default(),
                    last_sent: Instant::now(),
                });
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::PermissionDenied | ErrorKind::NotFound | ErrorKind::WouldBlock
                ) =>
            {
                if !*permission_warning_logged && error.kind() == ErrorKind::PermissionDenied {
                    send_log(
                        event_sender,
                        "warn",
                        "Native Linux gamepad polling cannot open one or more /dev/input/event* devices; add the user to the input group or grant udev ACLs.".to_owned(),
                    );
                    *permission_warning_logged = true;
                }
            }
            Err(_) => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_gamepad_event_candidates() -> Vec<PathBuf> {
    linux_event_candidates_by_name(|name| name.contains("event-joystick"))
}

#[cfg(target_os = "linux")]
fn linux_event_candidates_by_name(matches_name: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut real_paths = Vec::new();
    for directory in ["/dev/input/by-id", "/dev/input/by-path"] {
        let Ok(entries) = read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let real_path = canonicalize(&path).unwrap_or_else(|_| path.clone());
            if matches_name(name) && !real_paths.iter().any(|candidate| candidate == &real_path) {
                real_paths.push(real_path);
                candidates.push(path);
            }
        }
    }
    candidates
}

#[cfg(target_os = "linux")]
fn poll_linux_gamepads(devices: &mut [LinuxGamepadDevice]) {
    let mut buffer = [0u8; 24 * 32];
    for device in devices {
        loop {
            match device.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    for event in buffer[..bytes_read].chunks_exact(24) {
                        apply_linux_evdev_event(&mut device.snapshot, event);
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn apply_linux_evdev_event(snapshot: &mut NativeGamepadSnapshot, event: &[u8]) {
    if event.len() < 24 {
        return;
    }
    let event_type = u16::from_ne_bytes([event[16], event[17]]);
    let code = u16::from_ne_bytes([event[18], event[19]]);
    let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);

    match event_type {
        0x01 => apply_linux_evdev_key(snapshot, code, value != 0),
        0x03 => apply_linux_evdev_axis(snapshot, code, value),
        _ => {}
    }
}

#[cfg(target_os = "linux")]
fn apply_linux_evdev_key(snapshot: &mut NativeGamepadSnapshot, code: u16, pressed: bool) {
    let Some(mask) = linux_evdev_button_mask(code) else {
        return;
    };
    snapshot.connected = true;
    if pressed {
        snapshot.buttons |= mask;
    } else {
        snapshot.buttons &= !mask;
    }
}

#[cfg(target_os = "linux")]
fn apply_linux_evdev_axis(snapshot: &mut NativeGamepadSnapshot, code: u16, value: i32) {
    snapshot.connected = true;
    match code {
        0x00 => snapshot.left_stick_x = clamp_linux_axis_i16(value),
        0x01 => snapshot.left_stick_y = clamp_linux_axis_i16(value.saturating_neg()),
        0x02 => snapshot.left_trigger = normalize_linux_trigger(value),
        0x03 => snapshot.right_stick_x = clamp_linux_axis_i16(value),
        0x04 => snapshot.right_stick_y = clamp_linux_axis_i16(value.saturating_neg()),
        0x05 => snapshot.right_trigger = normalize_linux_trigger(value),
        0x10 => {
            snapshot.buttons &= !(0x0004 | 0x0008);
            if value < 0 {
                snapshot.buttons |= 0x0004;
            } else if value > 0 {
                snapshot.buttons |= 0x0008;
            }
        }
        0x11 => {
            snapshot.buttons &= !(0x0001 | 0x0002);
            if value < 0 {
                snapshot.buttons |= 0x0001;
            } else if value > 0 {
                snapshot.buttons |= 0x0002;
            }
        }
        _ => {}
    }
}

#[cfg(target_os = "linux")]
fn linux_evdev_button_mask(code: u16) -> Option<u16> {
    match code {
        0x130 => Some(0x1000),
        0x131 => Some(0x2000),
        0x133 => Some(0x4000),
        0x134 => Some(0x8000),
        0x136 => Some(0x0020),
        0x137 => Some(0x0010),
        0x138 => Some(0x0200),
        0x139 => Some(0x0100),
        0x13a => Some(0x0040),
        0x13b => Some(0x0080),
        0x13c => Some(0x0400),
        0x13d => Some(0x0800),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn clamp_linux_axis_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(target_os = "linux")]
fn clamp_linux_rel_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(target_os = "linux")]
fn normalize_linux_trigger(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;

    fn evdev_event(event_type: u16, code: u16, value: i32) -> [u8; 24] {
        let mut event = [0u8; 24];
        event[16..18].copy_from_slice(&event_type.to_ne_bytes());
        event[18..20].copy_from_slice(&code.to_ne_bytes());
        event[20..24].copy_from_slice(&value.to_ne_bytes());
        event
    }

    #[test]
    fn maps_linux_evdev_buttons_and_axes_to_gamepad_snapshot() {
        let mut snapshot = NativeGamepadSnapshot::default();

        apply_linux_evdev_event(&mut snapshot, &evdev_event(0x01, 0x130, 1));
        apply_linux_evdev_event(&mut snapshot, &evdev_event(0x03, 0x00, 32767));
        apply_linux_evdev_event(&mut snapshot, &evdev_event(0x03, 0x01, -32768));
        apply_linux_evdev_event(&mut snapshot, &evdev_event(0x03, 0x02, 128));
        apply_linux_evdev_event(&mut snapshot, &evdev_event(0x03, 0x11, -1));

        assert!(snapshot.connected);
        assert_eq!(snapshot.buttons & 0x1000, 0x1000);
        assert_eq!(snapshot.buttons & 0x0001, 0x0001);
        assert_eq!(snapshot.left_stick_x, 32767);
        assert_eq!(snapshot.left_stick_y, 32767);
        assert_eq!(snapshot.left_trigger, 128);
    }

    #[test]
    fn detects_linux_capture_release_reasons() {
        let mut pressed = HashSet::<(u16, u16, u16)>::new();

        pressed.insert((0x12, 56, 0x0004));
        assert_eq!(linux_capture_release_reason(&pressed), None);

        pressed.insert((0x09, 15, 0));
        assert_eq!(
            linux_capture_release_reason(&pressed),
            Some(LinuxCaptureReleaseReason::AltTab)
        );

        pressed.clear();
        pressed.insert((0x11, 29, 0x0002));
        pressed.insert((0x12, 56, 0x0004));
        pressed.insert((0x1b, 1, 0));
        assert_eq!(
            linux_capture_release_reason(&pressed),
            Some(LinuxCaptureReleaseReason::CtrlAltEsc)
        );

        pressed.clear();
        pressed.insert((0x7d, 125, 0x0040));
        assert_eq!(
            linux_capture_release_reason(&pressed),
            Some(LinuxCaptureReleaseReason::WindowSwitch)
        );
    }

    #[test]
    fn wayland_sessions_skip_kernel_mouse_grab() {
        if detect_linux_display_session() == LinuxDisplaySession::Wayland {
            assert!(!linux_uses_evdev_mouse_grab());
        }
    }

    #[test]
    fn blocks_local_shortcut_keys_from_stream() {
        let mut pressed = HashSet::<(u16, u16, u16)>::new();
        let alt_down = NativeWindowInputEvent::Key {
            pressed: true,
            keycode: 0x12,
            scancode: 56,
            modifiers: 0x0004,
            timestamp_us: 0,
        };
        assert!(linux_should_block_stream_keyboard_event(&alt_down, &pressed));
        record_linux_pressed_input(&[alt_down], &mut pressed, &mut HashSet::new());

        let tab_down = NativeWindowInputEvent::Key {
            pressed: true,
            keycode: 0x09,
            scancode: 15,
            modifiers: 0,
            timestamp_us: 0,
        };
        assert!(linux_should_block_stream_keyboard_event(&tab_down, &pressed));
    }
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
