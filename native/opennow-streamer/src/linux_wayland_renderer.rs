use crate::gstreamer_backend::send_log;
use crate::protocol::Event;
use gstreamer as gst;
use gstreamer_video::prelude::*;
use gstreamer_video::{is_video_overlay_prepare_window_handle_message, VideoOverlay};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use wayland_client::protocol::{
    wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1, zwp_locked_pointer_v1, zwp_pointer_constraints_v1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1,
};
use wayland_protocols::xdg::activation::v1::client::{
    xdg_activation_token_v1, xdg_activation_v1,
};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_sys::client::wl_display;

static RENDERER: OnceLock<Mutex<Option<Arc<WaylandRendererShared>>>> = OnceLock::new();

const GST_WL_DISPLAY_HANDLE_CONTEXT_TYPE: &str = "GstWlDisplayHandleContextType";

enum RendererCommand {
    LockPointer,
    UnlockPointer,
    SetCaptureActive(bool),
    Shutdown,
}

struct WaylandRendererShared {
    command_tx: Sender<RendererCommand>,
    worker: Mutex<Option<JoinHandle<()>>>,
    display_ptr: usize,
    surface_ptr: AtomicUsize,
    surface_ready: AtomicBool,
    video_sink_attached: AtomicBool,
    video_sink: Mutex<Option<gst::Element>>,
    attach_retry_running: AtomicBool,
    width: AtomicU32,
    height: AtomicU32,
    surface_focused: AtomicBool,
    keyboard_focused: AtomicBool,
    capture_active: AtomicBool,
    capture_click_pending: AtomicBool,
    pointer_locked: AtomicBool,
    pointer_confined: AtomicBool,
    pointer_lock_lost: AtomicBool,
}

#[link(name = "gstwayland-1.0")]
extern "C" {
    fn gst_wl_display_handle_context_new(display: *mut wl_display) -> *mut gst::ffi::GstContext;
}

pub(crate) fn linux_wayland_renderer_is_active() -> bool {
    renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|_| true))
        .unwrap_or(false)
}

pub(crate) fn linux_wayland_renderer_video_sink_attached() -> bool {
    renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|shared| shared.video_sink_attached.load(Ordering::SeqCst))
        })
        .unwrap_or(false)
}

pub(crate) fn linux_wayland_renderer_ensure(
    event_sender: &Option<Sender<Event>>,
) -> Result<Arc<WaylandRendererShared>, String> {
    if let Some(shared) = renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
    {
        return Ok(shared);
    }

    let conn = Connection::connect_to_env()
        .map_err(|error| format!("Failed to connect to Wayland display: {error}"))?;
    let display_ptr = conn.display().id().as_ptr() as usize;
    let (command_tx, command_rx) = mpsc::channel();

    let shared = Arc::new(WaylandRendererShared {
        command_tx,
        worker: Mutex::new(None),
        display_ptr,
        surface_ptr: AtomicUsize::new(0),
        surface_ready: AtomicBool::new(false),
        video_sink_attached: AtomicBool::new(false),
        video_sink: Mutex::new(None),
        attach_retry_running: AtomicBool::new(false),
        width: AtomicU32::new(1920),
        height: AtomicU32::new(1080),
        surface_focused: AtomicBool::new(false),
        keyboard_focused: AtomicBool::new(false),
        capture_active: AtomicBool::new(false),
        capture_click_pending: AtomicBool::new(false),
        pointer_locked: AtomicBool::new(false),
        pointer_confined: AtomicBool::new(false),
        pointer_lock_lost: AtomicBool::new(false),
    });

    let worker_shared = Arc::clone(&shared);
    let handle = thread::Builder::new()
        .name("opennow-wayland-renderer".into())
        .spawn(move || wayland_renderer_worker(conn, command_rx, worker_shared))
        .map_err(|error| format!("Failed to start Wayland renderer thread: {error}"))?;
    *shared.worker.lock().unwrap() = Some(handle);

    if let Ok(mut guard) = renderer_slot().lock() {
        *guard = Some(Arc::clone(&shared));
    }

    send_log(
        event_sender,
        "info",
        "Native Wayland renderer starting with compositor pointer-lock support.".to_owned(),
    );
    Ok(shared)
}

fn renderer_surface_available(shared: &WaylandRendererShared) -> bool {
    shared.surface_ready.load(Ordering::SeqCst)
        && shared.surface_ptr.load(Ordering::SeqCst) != 0
}

fn set_wl_display_context_on_element(
    element: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) -> Result<(), String> {
    let shared = linux_wayland_renderer_ensure(event_sender)?;
    unsafe {
        let context = gst_wl_display_handle_context_new(shared.display_ptr as *mut wl_display);
        if context.is_null() {
            return Err("Failed to create GstWl display context for waylandsink.".to_owned());
        }
        element.set_context(&gst::Context::from_glib_full(context));
    }
    Ok(())
}

fn apply_video_overlay_to_renderer_surface(
    element: &gst::Element,
    overlay: &VideoOverlay,
    shared: &WaylandRendererShared,
    event_sender: &Option<Sender<Event>>,
) -> Result<(), String> {
    if !renderer_surface_available(shared) {
        return Err(
            "Wayland renderer surface is waiting for the first compositor configure event."
                .to_owned(),
        );
    }

    let surface_ptr = shared.surface_ptr.load(Ordering::SeqCst);
    set_wl_display_context_on_element(element, event_sender)?;
    unsafe {
        overlay.set_window_handle(surface_ptr);
    }
    let width = shared.width.load(Ordering::SeqCst).max(1) as i32;
    let height = shared.height.load(Ordering::SeqCst).max(1) as i32;
    overlay
        .set_render_rectangle(0, 0, width, height)
        .map_err(|error| format!("Failed to set Wayland render rectangle: {error}"))?;
    overlay.expose();
    shared.video_sink_attached.store(true, Ordering::SeqCst);
    Ok(())
}

fn refresh_video_sink_geometry(
    shared: &WaylandRendererShared,
    event_sender: &Option<Sender<Event>>,
) {
    if !renderer_surface_available(shared) {
        return;
    }

    let sink = shared
        .video_sink
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    let Some(sink) = sink else {
        return;
    };

    if !shared.video_sink_attached.load(Ordering::SeqCst) {
        let _ = try_attach_video_sink_to_renderer(&sink, event_sender);
        return;
    }

    let Ok(overlay) = sink.dynamic_cast::<VideoOverlay>() else {
        return;
    };
    let width = shared.width.load(Ordering::SeqCst).max(1) as i32;
    let height = shared.height.load(Ordering::SeqCst).max(1) as i32;
    let _ = overlay.set_render_rectangle(0, 0, width, height);
    overlay.expose();
}

fn try_attach_video_sink_to_renderer(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) -> bool {
    let shared = match linux_wayland_renderer_ensure(event_sender) {
        Ok(shared) => shared,
        Err(error) => {
            send_log(event_sender, "warn", error);
            return false;
        }
    };
    if shared.video_sink_attached.load(Ordering::SeqCst) {
        return true;
    }
    if !renderer_surface_available(&shared) {
        return false;
    }

    match attach_video_sink_to_renderer_surface(sink, &shared, event_sender) {
        Ok(()) => true,
        Err(error) => {
            send_log(event_sender, "warn", error);
            false
        }
    }
}

fn schedule_video_sink_attach_retry(sink: gst::Element, event_sender: Option<Sender<Event>>) {
    let shared = match renderer_slot().lock().ok().and_then(|guard| guard.clone()) {
        Some(shared) => shared,
        None => return,
    };
    if shared.attach_retry_running.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::Builder::new()
        .name("opennow-wayland-attach".into())
        .spawn(move || {
            let _guard = AttachRetryGuard(shared);
            for attempt in 0..150 {
                if try_attach_video_sink_to_renderer(&sink, &event_sender) {
                    return;
                }
                if attempt == 0 {
                    send_log(
                        &event_sender,
                        "info",
                        "Waiting for native Wayland renderer surface before embedding the video sink."
                            .to_owned(),
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
            send_log(
                &event_sender,
                "warn",
                "Timed out waiting to embed the native video sink into the Wayland renderer surface; continuing with the sink's own window.".to_owned(),
            );
        })
        .ok();
}

struct AttachRetryGuard(Arc<WaylandRendererShared>);

impl Drop for AttachRetryGuard {
    fn drop(&mut self) {
        self.0
            .attach_retry_running
            .store(false, Ordering::SeqCst);
    }
}

fn attach_video_sink_to_renderer_surface(
    sink: &gst::Element,
    shared: &WaylandRendererShared,
    event_sender: &Option<Sender<Event>>,
) -> Result<(), String> {
    let overlay = sink
        .clone()
        .dynamic_cast::<VideoOverlay>()
        .map_err(|_| "Native Wayland video sink does not implement GstVideoOverlay.".to_owned())?;
    apply_video_overlay_to_renderer_surface(sink, &overlay, shared, event_sender)?;

    let width = shared.width.load(Ordering::SeqCst).max(1);
    let height = shared.height.load(Ordering::SeqCst).max(1);
    send_log(
        event_sender,
        "info",
        format!(
            "Attached native Wayland video sink to renderer surface at {width}x{height}."
        ),
    );
    Ok(())
}

pub(crate) fn linux_wayland_renderer_register_video_sink(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) {
    if let Ok(shared) = linux_wayland_renderer_ensure(event_sender) {
        if let Ok(mut slot) = shared.video_sink.lock() {
            *slot = Some(sink.clone());
        }
    }
}

pub(crate) fn linux_wayland_renderer_provision_display_context(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) -> Result<(), String> {
    set_wl_display_context_on_element(sink, event_sender)
}

pub(crate) fn handle_pipeline_bus_sync_message(msg: &gst::MessageRef) -> gst::BusSyncReply {
    match msg.view() {
        gst::MessageView::NeedContext(need) => {
            if need.context_type() != GST_WL_DISPLAY_HANDLE_CONTEXT_TYPE {
                return gst::BusSyncReply::Pass;
            }
            let Some(element) = msg
                .src()
                .and_then(|src| src.clone().downcast::<gst::Element>().ok())
            else {
                return gst::BusSyncReply::Pass;
            };
            if set_wl_display_context_on_element(&element, &None).is_err() {
                return gst::BusSyncReply::Pass;
            }
            gst::BusSyncReply::Drop
        }
        gst::MessageView::Element(_) if is_video_overlay_prepare_window_handle_message(msg) => {
            let Some(element) = msg
                .src()
                .and_then(|src| src.clone().downcast::<gst::Element>().ok())
            else {
                return gst::BusSyncReply::Pass;
            };
            let Ok(overlay) = element.clone().dynamic_cast::<VideoOverlay>() else {
                return gst::BusSyncReply::Pass;
            };
            let Ok(shared) = linux_wayland_renderer_ensure(&None) else {
                return gst::BusSyncReply::Pass;
            };
            if let Ok(mut slot) = shared.video_sink.lock() {
                *slot = Some(element.clone());
            }
            match apply_video_overlay_to_renderer_surface(&element, &overlay, &shared, &None) {
                Ok(()) => gst::BusSyncReply::Drop,
                Err(_) => gst::BusSyncReply::Pass,
            }
        }
        _ => gst::BusSyncReply::Pass,
    }
}

pub(crate) fn linux_wayland_renderer_attach_video_sink(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) -> Result<(), String> {
    if try_attach_video_sink_to_renderer(sink, event_sender) {
        return Ok(());
    }

    schedule_video_sink_attach_retry(sink.clone(), event_sender.clone());
    Ok(())
}

pub(crate) fn linux_wayland_renderer_retry_video_sink_attach(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
) {
    if !try_attach_video_sink_to_renderer(sink, event_sender) {
        schedule_video_sink_attach_retry(sink.clone(), event_sender.clone());
    }
}

pub(crate) fn linux_wayland_renderer_lock_pointer() -> Result<(), String> {
    send_renderer_command(RendererCommand::LockPointer)
}

pub(crate) fn linux_wayland_renderer_unlock_pointer() {
    let _ = send_renderer_command(RendererCommand::UnlockPointer);
}

pub(crate) fn linux_wayland_renderer_set_capture_active(active: bool) {
    let _ = send_renderer_command(RendererCommand::SetCaptureActive(active));
}

pub(crate) fn linux_wayland_renderer_consume_capture_click() -> bool {
    renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|shared| shared.capture_click_pending.swap(false, Ordering::SeqCst))
        })
        .unwrap_or(false)
}

pub(crate) fn linux_wayland_renderer_surface_focused() -> bool {
    renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|shared| shared.surface_focused()))
        .unwrap_or(false)
}

pub(crate) fn linux_wayland_renderer_pointer_lock_lost() -> bool {
    renderer_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|shared| shared.pointer_lock_lost()))
        .unwrap_or(false)
}

pub(crate) fn linux_wayland_renderer_close() {
    let worker = {
        let mut guard = match renderer_slot().lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let Some(shared) = guard.take() else {
            return;
        };
        let _ = shared.command_tx.send(RendererCommand::Shutdown);
        shared.worker.lock().ok().and_then(|mut slot| slot.take())
    };
    if let Some(handle) = worker {
        let _ = handle.join();
    }
}

fn send_renderer_command(command: RendererCommand) -> Result<(), String> {
    renderer_slot()
        .lock()
        .map_err(|_| "Wayland renderer lock is poisoned.".to_owned())?
        .as_ref()
        .ok_or_else(|| "Wayland renderer is not initialized.".to_owned())?
        .command_tx
        .send(command)
        .map_err(|error| format!("Failed to send Wayland renderer command: {error}"))
}

fn renderer_slot() -> &'static Mutex<Option<Arc<WaylandRendererShared>>> {
    RENDERER.get_or_init(|| Mutex::new(None))
}

impl WaylandRendererShared {
    fn surface_focused(&self) -> bool {
        self.surface_focused.load(Ordering::SeqCst)
            || self.keyboard_focused.load(Ordering::SeqCst)
    }

    fn pointer_lock_lost(&self) -> bool {
        self.pointer_lock_lost.swap(false, Ordering::SeqCst)
    }
}

struct RendererState {
    command_rx: Receiver<RendererCommand>,
    shared: Arc<WaylandRendererShared>,
    running: bool,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    base_surface: Option<wl_surface::WlSurface>,
    xdg_surface: Option<xdg_surface::XdgSurface>,
    xdg_toplevel: Option<xdg_toplevel::XdgToplevel>,
    decoration_manager: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    toplevel_decoration: Option<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1>,
    activation: Option<xdg_activation_v1::XdgActivationV1>,
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    relative_pointer_manager: Option<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1>,
    relative_pointer: Option<zwp_relative_pointer_v1::ZwpRelativePointerV1>,
    pointer_enter_serial: u32,
    pointer_constraints: Option<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1>,
    locked_pointer: Option<zwp_locked_pointer_v1::ZwpLockedPointerV1>,
    confined_pointer: Option<zwp_confined_pointer_v1::ZwpConfinedPointerV1>,
    lock_requested: bool,
    use_confine_fallback: bool,
    pending_activation: Option<xdg_activation_token_v1::XdgActivationTokenV1>,
}

impl RendererState {
    fn ensure_relative_pointer(&mut self, qh: &QueueHandle<Self>) {
        if self.relative_pointer.is_some() {
            return;
        }
        let (Some(manager), Some(pointer)) = (
            self.relative_pointer_manager.as_ref(),
            self.pointer.as_ref(),
        ) else {
            return;
        };
        self.relative_pointer = Some(manager.get_relative_pointer(pointer, qh, ()));
    }

    fn init_xdg_surface(&mut self, qh: &QueueHandle<Self>) {
        let Some(wm_base) = self.wm_base.as_ref() else {
            return;
        };
        let Some(base_surface) = self.base_surface.as_ref() else {
            return;
        };

        let xdg_surface = wm_base.get_xdg_surface(base_surface, qh, ());
        let toplevel = xdg_surface.get_toplevel(qh, ());
        toplevel.set_title("OpenNOW Stream".into());
        toplevel.set_app_id("dev.opennow.stream".into());
        toplevel.set_min_size(640, 360);
        base_surface.commit();

        self.shared.surface_ptr.store(
            base_surface.id().as_ptr() as usize,
            Ordering::SeqCst,
        );
        self.xdg_surface = Some(xdg_surface);
        self.xdg_toplevel = Some(toplevel);
        self.apply_toplevel_decoration(qh);
    }

    fn apply_toplevel_decoration(&mut self, qh: &QueueHandle<Self>) {
        let (Some(manager), Some(toplevel)) = (
            self.decoration_manager.as_ref(),
            self.xdg_toplevel.as_ref(),
        ) else {
            return;
        };
        if self.toplevel_decoration.is_some() {
            return;
        }

        let decoration = manager.get_toplevel_decoration(toplevel, qh, ());
        decoration.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        self.toplevel_decoration = Some(decoration);
    }

    fn request_surface_activation(&mut self, serial: u32, qh: &QueueHandle<Self>) {
        let (Some(activation), Some(seat), Some(surface)) = (
            self.activation.as_ref(),
            self.seat.as_ref(),
            self.base_surface.as_ref(),
        ) else {
            return;
        };
        if self.pending_activation.is_some() {
            return;
        }

        let token = activation.get_activation_token(qh, ());
        token.set_serial(serial, seat);
        token.set_surface(surface);
        token.set_app_id("dev.opennow.stream".into());
        token.commit();
        self.pending_activation = Some(token);
    }

    fn try_apply_pointer_constraint(&mut self, qh: &QueueHandle<Self>) {
        if !self.lock_requested
            || self.locked_pointer.is_some()
            || self.confined_pointer.is_some()
        {
            return;
        }
        let (Some(constraints), Some(pointer), Some(surface)) = (
            self.pointer_constraints.as_ref(),
            self.pointer.as_ref(),
            self.base_surface.as_ref(),
        ) else {
            return;
        };

        if self.use_confine_fallback {
            let confined = constraints.confine_pointer(
                surface,
                pointer,
                None,
                zwp_pointer_constraints_v1::Lifetime::Persistent,
                qh,
                (),
            );
            self.confined_pointer = Some(confined);
        } else {
            let locked = constraints.lock_pointer(
                surface,
                pointer,
                None,
                zwp_pointer_constraints_v1::Lifetime::Persistent,
                qh,
                (),
            );
            self.locked_pointer = Some(locked);
        }
        self.hide_compositor_cursor();
    }

    fn hide_compositor_cursor(&self) {
        let Some(pointer) = self.pointer.as_ref() else {
            return;
        };
        if self.pointer_enter_serial != 0 {
            pointer.set_cursor(self.pointer_enter_serial, None, 0, 0);
        }
    }

    fn begin_capture_from_surface_click(&mut self, serial: u32, qh: &QueueHandle<Self>) {
        self.shared.capture_click_pending.store(true, Ordering::SeqCst);
        self.lock_requested = true;
        self.request_surface_activation(serial, qh);
        self.try_apply_pointer_constraint(qh);
        self.hide_compositor_cursor();
    }

    fn release_pointer_constraint(&mut self) {
        self.lock_requested = false;
        self.use_confine_fallback = false;
        self.shared.pointer_locked.store(false, Ordering::SeqCst);
        self.shared.pointer_confined.store(false, Ordering::SeqCst);
        if let Some(locked) = self.locked_pointer.take() {
            locked.destroy();
        }
        if let Some(confined) = self.confined_pointer.take() {
            confined.destroy();
        }
        if let Some(token) = self.pending_activation.take() {
            token.destroy();
        }
    }

    fn handle_constraint_unlocked(&mut self, qh: &QueueHandle<Self>) {
        self.shared.pointer_locked.store(false, Ordering::SeqCst);
        self.shared.pointer_confined.store(false, Ordering::SeqCst);
        self.locked_pointer = None;
        self.confined_pointer = None;
        if self.lock_requested && !self.use_confine_fallback {
            self.use_confine_fallback = true;
            self.try_apply_pointer_constraint(qh);
            return;
        }
        if self.shared.capture_active.load(Ordering::SeqCst) {
            self.shared.pointer_lock_lost.store(true, Ordering::SeqCst);
        }
        self.lock_requested = false;
    }
}

fn wayland_renderer_worker(
    conn: Connection,
    command_rx: Receiver<RendererCommand>,
    shared: Arc<WaylandRendererShared>,
) {
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    conn.display().get_registry(&qh, ());

    let mut state = RendererState {
        command_rx,
        shared,
        running: true,
        wm_base: None,
        base_surface: None,
        xdg_surface: None,
        xdg_toplevel: None,
        decoration_manager: None,
        toplevel_decoration: None,
        activation: None,
        seat: None,
        keyboard: None,
        pointer: None,
        relative_pointer_manager: None,
        relative_pointer: None,
        pointer_enter_serial: 0,
        pointer_constraints: None,
        locked_pointer: None,
        confined_pointer: None,
        lock_requested: false,
        use_confine_fallback: false,
        pending_activation: None,
    };

    if event_queue.roundtrip(&mut state).is_err() {
        eprintln!("OpenNOW Wayland renderer bootstrap roundtrip failed.");
        return;
    }
    if !renderer_surface_available(&state.shared) {
        let _ = event_queue.roundtrip(&mut state);
    }

    while state.running {
        loop {
            match state.command_rx.recv_timeout(Duration::from_millis(16)) {
                Ok(command) => handle_renderer_command(&mut state, command, &qh),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    state.running = false;
                    break;
                }
            }
        }
        if !state.running {
            break;
        }

        if event_queue.blocking_dispatch(&mut state).is_err() {
            eprintln!("OpenNOW Wayland renderer event dispatch failed.");
            break;
        }
    }
}

fn handle_renderer_command(
    state: &mut RendererState,
    command: RendererCommand,
    qh: &QueueHandle<RendererState>,
) {
    match command {
        RendererCommand::LockPointer => {
            state.lock_requested = true;
            state.try_apply_pointer_constraint(qh);
        }
        RendererCommand::UnlockPointer => state.release_pointer_constraint(),
        RendererCommand::SetCaptureActive(active) => {
            state
                .shared
                .capture_active
                .store(active, Ordering::SeqCst);
            if active {
                state.try_apply_pointer_constraint(qh);
                state.hide_compositor_cursor();
            } else {
                state.release_pointer_constraint();
            }
        }
        RendererCommand::Shutdown => {
            state.release_pointer_constraint();
            state.running = false;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for RendererState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        1.min(version),
                        qh,
                        (),
                    );
                    let surface = compositor.create_surface(qh, ());
                    state.base_surface = Some(surface);
                    if state.wm_base.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, 1.min(version), qh, ());
                    state.seat = Some(seat);
                }
                "xdg_wm_base" => {
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(
                        name,
                        1.min(version),
                        qh,
                        (),
                    );
                    state.wm_base = Some(wm_base);
                    if state.base_surface.is_some() && state.xdg_surface.is_none() {
                        state.init_xdg_surface(qh);
                    }
                }
                "xdg_activation_v1" => {
                    let activation = registry.bind::<xdg_activation_v1::XdgActivationV1, _, _>(
                        name,
                        1.min(version),
                        qh,
                        (),
                    );
                    state.activation = Some(activation);
                }
                "zxdg_decoration_manager_v1" => {
                    let manager = registry
                        .bind::<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, _, _>(
                            name,
                            1.min(version),
                            qh,
                            (),
                        );
                    state.decoration_manager = Some(manager);
                    state.apply_toplevel_decoration(qh);
                }
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, 1.min(version), qh, ());
                    let _ = output;
                }
                "zwp_pointer_constraints_v1" => {
                    let constraints = registry
                        .bind::<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1, _, _>(
                            name,
                            1.min(version),
                            qh,
                            (),
                        );
                    state.pointer_constraints = Some(constraints);
                    state.try_apply_pointer_constraint(qh);
                }
                "zwp_relative_pointer_manager_v1" => {
                    let manager = registry
                        .bind::<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, _, _>(
                            name,
                            1.min(version),
                            qh,
                            (),
                        );
                    state.relative_pointer_manager = Some(manager);
                    state.ensure_relative_pointer(qh);
                }
                _ => {}
            }
        }
    }
}

delegate_noop!(RendererState: ignore wl_compositor::WlCompositor);
delegate_noop!(RendererState: ignore wl_surface::WlSurface);
delegate_noop!(RendererState: ignore wl_output::WlOutput);
delegate_noop!(RendererState: ignore zwp_pointer_constraints_v1::ZwpPointerConstraintsV1);
delegate_noop!(RendererState: ignore zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1);
delegate_noop!(RendererState: ignore zwp_relative_pointer_v1::ZwpRelativePointerV1);
delegate_noop!(RendererState: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(RendererState: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(RendererState: ignore zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for RendererState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for RendererState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            if let Some(base_surface) = state.base_surface.as_ref() {
                state.shared.surface_ptr.store(
                    base_surface.id().as_ptr() as usize,
                    Ordering::SeqCst,
                );
                base_surface.commit();
                state
                    .shared
                    .surface_ready
                    .store(true, Ordering::SeqCst);
                refresh_video_sink_geometry(&state.shared, &None);
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for RendererState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 {
                    state.shared.width.store(width as u32, Ordering::SeqCst);
                }
                if height > 0 {
                    state.shared.height.store(height as u32, Ordering::SeqCst);
                }
                refresh_video_sink_geometry(&state.shared, &None);
            }
            xdg_toplevel::Event::Close => {
                state.shared.pointer_lock_lost.store(true, Ordering::SeqCst);
                state.release_pointer_constraint();
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for RendererState {
    fn event(
        state: &mut Self,
        token: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token: token_str } = event {
            if let (Some(activation), Some(surface)) = (
                state.activation.as_ref(),
                state.base_surface.as_ref(),
            ) {
                activation.activate(token_str, surface);
            }
            token.destroy();
            state.pending_activation = None;
            state.try_apply_pointer_constraint(qh);
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for RendererState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
                state.ensure_relative_pointer(qh);
                state.try_apply_pointer_constraint(qh);
            }
            if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for RendererState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, .. } => {
                let focused = state
                    .base_surface
                    .as_ref()
                    .is_some_and(|base| base.id() == surface.id());
                state
                    .shared
                    .keyboard_focused
                    .store(focused, Ordering::SeqCst);
                if focused {
                    state.try_apply_pointer_constraint(qh);
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                state
                    .shared
                    .keyboard_focused
                    .store(false, Ordering::SeqCst);
                if state.shared.capture_active.load(Ordering::SeqCst) {
                    state.shared.pointer_lock_lost.store(true, Ordering::SeqCst);
                    state.release_pointer_constraint();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for RendererState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, surface, .. } => {
                let focused = state
                    .base_surface
                    .as_ref()
                    .is_some_and(|base| base.id() == surface.id());
                state.shared.surface_focused.store(focused, Ordering::SeqCst);
                state.pointer_enter_serial = serial;
                if focused {
                    if state.lock_requested || state.shared.capture_active.load(Ordering::SeqCst) {
                        state.hide_compositor_cursor();
                    }
                    state.try_apply_pointer_constraint(qh);
                }
            }
            wl_pointer::Event::Leave { .. } => {
                state.shared.surface_focused.store(false, Ordering::SeqCst);
                if state.shared.capture_active.load(Ordering::SeqCst) {
                    state.shared.pointer_lock_lost.store(true, Ordering::SeqCst);
                    state.release_pointer_constraint();
                }
            }
            wl_pointer::Event::Motion { .. } => {
                if state.shared.capture_active.load(Ordering::SeqCst)
                    || state.lock_requested
                    || state.shared.pointer_locked.load(Ordering::SeqCst)
                    || state.shared.pointer_confined.load(Ordering::SeqCst)
                {
                    state.hide_compositor_cursor();
                }
            }
            wl_pointer::Event::Button {
                serial,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => {
                if state.shared.surface_focused.load(Ordering::SeqCst) {
                    state.begin_capture_from_surface_click(serial, qh);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_locked_pointer_v1::ZwpLockedPointerV1, ()> for RendererState {
    fn event(
        state: &mut Self,
        _: &zwp_locked_pointer_v1::ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_locked_pointer_v1::Event::Locked => {
                state.shared.pointer_locked.store(true, Ordering::SeqCst);
                state.shared.pointer_lock_lost.store(false, Ordering::SeqCst);
                state.use_confine_fallback = false;
                state.hide_compositor_cursor();
            }
            zwp_locked_pointer_v1::Event::Unlocked => {
                state.handle_constraint_unlocked(qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_confined_pointer_v1::ZwpConfinedPointerV1, ()> for RendererState {
    fn event(
        state: &mut Self,
        _: &zwp_confined_pointer_v1::ZwpConfinedPointerV1,
        event: zwp_confined_pointer_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_confined_pointer_v1::Event::Confined => {
                state.shared.pointer_confined.store(true, Ordering::SeqCst);
                state.shared.pointer_lock_lost.store(false, Ordering::SeqCst);
                state.hide_compositor_cursor();
            }
            zwp_confined_pointer_v1::Event::Unconfined => {
                state.handle_constraint_unlocked(qh);
            }
            _ => {}
        }
    }
}
