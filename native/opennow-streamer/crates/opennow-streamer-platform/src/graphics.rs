use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsApi {
    D3d11,
    Vulkan,
    Metal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsTextureFormat {
    Rgba8,
    Rgb10A2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsContext {
    pub api: GraphicsApi,
    pub instance: usize,
    pub physical_device: usize,
    pub device: usize,
    pub queue: usize,
    pub queue_family_index: u32,
}

impl GraphicsContext {
    fn validate(self) -> Result<Self, GraphicsRuntimeError> {
        if self.device == 0 || self.queue == 0 {
            return Err(GraphicsRuntimeError::InvalidContext(
                "the graphics device and queue must be non-null",
            ));
        }
        if self.api == GraphicsApi::Vulkan && (self.instance == 0 || self.physical_device == 0) {
            return Err(GraphicsRuntimeError::InvalidContext(
                "Vulkan requires non-null instance and physical-device handles",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsRecordCommand {
    pub command_buffer: usize,
    pub frame_slot: u32,
}

impl GraphicsRecordCommand {
    fn validate(self) -> Result<Self, GraphicsRuntimeError> {
        if self.command_buffer == 0 {
            return Err(GraphicsRuntimeError::InvalidRenderCommand(
                "the command buffer must be non-null",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsRecordedFrame {
    pub resource: u64,
    pub resource_view: u64,
    pub texture_format: GraphicsTextureFormat,
    pub width: u32,
    pub height: u32,
    pub frame_slot: u32,
    pub generation: u64,
    pub presentation_time_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsFrameInfo {
    pub width: u32,
    pub height: u32,
    pub sequence: u64,
    pub presentation_time_ns: u64,
}

pub trait GraphicsFrame: Send + Sync + 'static {
    fn info(&self) -> GraphicsFrameInfo;

    fn record(
        &self,
        context: GraphicsContext,
        command: GraphicsRecordCommand,
    ) -> Result<GraphicsRecordedFrame, String>;
}

#[cfg(target_os = "linux")]
impl GraphicsFrame for opennow_streamer_platform_linux::LinuxGpuFrame {
    fn info(&self) -> GraphicsFrameInfo {
        GraphicsFrameInfo {
            width: self.width(),
            height: self.height(),
            sequence: self.sequence(),
            presentation_time_ns: self.presentation_time_ns(),
        }
    }

    fn record(
        &self,
        context: GraphicsContext,
        command: GraphicsRecordCommand,
    ) -> Result<GraphicsRecordedFrame, String> {
        if context.api != GraphicsApi::Vulkan {
            return Err("a Linux decoded frame requires a Vulkan graphics context".to_owned());
        }
        let frame = unsafe {
            self.record(
                opennow_streamer_platform_linux::VulkanRenderDevice {
                    instance: context.instance,
                    physical_device: context.physical_device,
                    device: context.device,
                    queue_family: context.queue_family_index,
                },
                command.command_buffer,
                command.frame_slot,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(GraphicsRecordedFrame {
            resource: frame.image,
            resource_view: frame.image_view,
            texture_format: GraphicsTextureFormat::Rgba8,
            width: frame.width,
            height: frame.height,
            frame_slot: frame.slot,
            generation: frame.generation,
            presentation_time_ns: frame.timestamp_us.saturating_mul(1_000),
        })
    }
}

#[cfg(target_os = "macos")]
impl GraphicsFrame for opennow_streamer_platform_macos::MetalFrame {
    fn info(&self) -> GraphicsFrameInfo {
        GraphicsFrameInfo {
            width: self.width(),
            height: self.height(),
            sequence: self.sequence(),
            presentation_time_ns: self.presentation_time_ns(),
        }
    }

    fn record(
        &self,
        context: GraphicsContext,
        command: GraphicsRecordCommand,
    ) -> Result<GraphicsRecordedFrame, String> {
        if context.api != GraphicsApi::Metal {
            return Err("a macOS VideoToolbox frame requires a Metal graphics context".to_owned());
        }
        let frame = unsafe {
            self.record(
                opennow_streamer_platform_macos::AdoptedMetalContext {
                    device: context.device as *mut std::ffi::c_void,
                    command_buffer: command.command_buffer as *mut std::ffi::c_void,
                },
                command.frame_slot,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(GraphicsRecordedFrame {
            resource: frame.texture as usize as u64,
            resource_view: 0,
            texture_format: GraphicsTextureFormat::Rgba8,
            width: frame.width,
            height: frame.height,
            frame_slot: frame.frame_slot,
            generation: frame.generation,
            presentation_time_ns: frame.presentation_time_ns,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsRuntimeError {
    InvalidContext(&'static str),
    InvalidRenderCommand(&'static str),
    SceneGraphInactive,
    WrongThread,
    NoFrame,
    StaleFrame,
    FrameAlreadyRecorded,
    RecordFailed(String),
}

impl fmt::Display for GraphicsRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContext(message) | Self::InvalidRenderCommand(message) => {
                formatter.write_str(message)
            }
            Self::SceneGraphInactive => formatter.write_str("the scene graph is not initialized"),
            Self::WrongThread => {
                formatter.write_str("graphics access must stay on the render thread")
            }
            Self::NoFrame => formatter.write_str("no decoded GPU frame is pending"),
            Self::StaleFrame => {
                formatter.write_str("the GPU frame belongs to an older scene graph")
            }
            Self::FrameAlreadyRecorded => {
                formatter.write_str("the GPU frame token was already recorded")
            }
            Self::RecordFailed(message) => {
                write!(formatter, "GPU frame recording failed: {message}")
            }
        }
    }
}

impl std::error::Error for GraphicsRuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsPublishOutcome {
    Published,
    Replaced,
}

pub struct GraphicsFrameToken {
    epoch: u64,
    frame: Arc<dyn GraphicsFrame>,
    owner: Arc<GraphicsRuntimeInner>,
    recorded: AtomicBool,
}

impl GraphicsFrameToken {
    pub fn info(&self) -> GraphicsFrameInfo {
        self.frame.info()
    }
}

#[derive(Clone)]
pub struct GraphicsFramePublisher {
    inner: Arc<GraphicsRuntimeInner>,
}

impl GraphicsFramePublisher {
    pub fn clear(&self) {
        lock_scene(&self.inner.scene).latest = None;
    }

    pub fn context(&self) -> Option<GraphicsContextLease> {
        let scene = lock_scene(&self.inner.scene);
        scene.context.map(|context| GraphicsContextLease {
            context,
            epoch: scene.epoch,
        })
    }

    pub fn publish(
        &self,
        lease: GraphicsContextLease,
        frame: Arc<dyn GraphicsFrame>,
    ) -> Result<GraphicsPublishOutcome, GraphicsRuntimeError> {
        let outcome = {
            let mut scene = lock_scene(&self.inner.scene);
            if scene.context.is_none() {
                return Err(GraphicsRuntimeError::SceneGraphInactive);
            }
            if scene.epoch != lease.epoch || scene.context != Some(lease.context) {
                return Err(GraphicsRuntimeError::StaleFrame);
            }
            let replaced = scene.latest.replace(FrameSlot {
                epoch: lease.epoch,
                frame,
            });
            if replaced.is_some() {
                GraphicsPublishOutcome::Replaced
            } else {
                GraphicsPublishOutcome::Published
            }
        };
        (self.inner.frame_available)();
        Ok(outcome)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsContextLease {
    context: GraphicsContext,
    epoch: u64,
}

impl GraphicsContextLease {
    pub fn context(self) -> GraphicsContext {
        self.context
    }
}

#[derive(Clone)]
pub struct RenderThreadGraphics {
    inner: Arc<GraphicsRuntimeInner>,
}

impl RenderThreadGraphics {
    pub fn new(
        frame_available: impl Fn() + Send + Sync + 'static,
    ) -> (Self, GraphicsFramePublisher) {
        let inner = Arc::new(GraphicsRuntimeInner {
            scene: Mutex::new(SceneGraphState::default()),
            frame_available: Box::new(frame_available),
        });
        (
            Self {
                inner: Arc::clone(&inner),
            },
            GraphicsFramePublisher { inner },
        )
    }

    pub fn initialize(&self, context: GraphicsContext) -> Result<(), GraphicsRuntimeError> {
        let context = context.validate()?;
        let current_thread = thread::current().id();
        let mut scene = lock_scene(&self.inner.scene);
        if let Some(render_thread) = scene.render_thread {
            if render_thread != current_thread {
                return Err(GraphicsRuntimeError::WrongThread);
            }
            if scene.context == Some(context) {
                return Ok(());
            }
        }
        scene.latest = None;
        scene.epoch = scene.epoch.wrapping_add(1);
        scene.context = Some(context);
        scene.render_thread = Some(current_thread);
        Ok(())
    }

    pub fn acquire_latest(&self) -> Result<GraphicsFrameToken, GraphicsRuntimeError> {
        let mut scene = lock_scene(&self.inner.scene);
        require_render_thread(&scene)?;
        let slot = scene.latest.take().ok_or(GraphicsRuntimeError::NoFrame)?;
        if slot.epoch != scene.epoch {
            return Err(GraphicsRuntimeError::StaleFrame);
        }
        Ok(GraphicsFrameToken {
            epoch: slot.epoch,
            frame: slot.frame,
            owner: Arc::clone(&self.inner),
            recorded: AtomicBool::new(false),
        })
    }

    pub fn record(
        &self,
        frame: &GraphicsFrameToken,
        command: GraphicsRecordCommand,
    ) -> Result<GraphicsRecordedFrame, GraphicsRuntimeError> {
        let command = command.validate()?;
        if !Arc::ptr_eq(&self.inner, &frame.owner) {
            return Err(GraphicsRuntimeError::StaleFrame);
        }
        let context = {
            let scene = lock_scene(&self.inner.scene);
            require_render_thread(&scene)?;
            if frame.epoch != scene.epoch {
                return Err(GraphicsRuntimeError::StaleFrame);
            }
            scene
                .context
                .ok_or(GraphicsRuntimeError::SceneGraphInactive)?
        };
        if frame.recorded.swap(true, Ordering::AcqRel) {
            return Err(GraphicsRuntimeError::FrameAlreadyRecorded);
        }
        frame
            .frame
            .record(context, command)
            .map_err(GraphicsRuntimeError::RecordFailed)
    }

    pub fn shutdown(&self) -> Result<(), GraphicsRuntimeError> {
        let mut scene = lock_scene(&self.inner.scene);
        if scene.context.is_none() {
            return Ok(());
        }
        require_render_thread(&scene)?;
        scene.latest = None;
        scene.context = None;
        scene.render_thread = None;
        scene.epoch = scene.epoch.wrapping_add(1);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        lock_scene(&self.inner.scene).context.is_some()
    }
}

struct GraphicsRuntimeInner {
    scene: Mutex<SceneGraphState>,
    frame_available: Box<dyn Fn() + Send + Sync>,
}

#[derive(Default)]
struct SceneGraphState {
    context: Option<GraphicsContext>,
    render_thread: Option<ThreadId>,
    epoch: u64,
    latest: Option<FrameSlot>,
}

struct FrameSlot {
    epoch: u64,
    frame: Arc<dyn GraphicsFrame>,
}

fn require_render_thread(scene: &SceneGraphState) -> Result<(), GraphicsRuntimeError> {
    let render_thread = scene
        .render_thread
        .ok_or(GraphicsRuntimeError::SceneGraphInactive)?;
    if render_thread != thread::current().id() {
        return Err(GraphicsRuntimeError::WrongThread);
    }
    Ok(())
}

fn lock_scene(scene: &Mutex<SceneGraphState>) -> std::sync::MutexGuard<'_, SceneGraphState> {
    scene
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_frame_implements_shared_graphics_frame_contract() {
        fn assert_graphics_frame<T: GraphicsFrame>() {}
        assert_graphics_frame::<opennow_streamer_platform_macos::MetalFrame>();
    }

    struct TestFrame {
        sequence: u64,
        drops: Arc<AtomicUsize>,
        records: Arc<Mutex<Vec<(GraphicsContext, GraphicsRecordCommand)>>>,
    }

    impl GraphicsFrame for TestFrame {
        fn info(&self) -> GraphicsFrameInfo {
            GraphicsFrameInfo {
                width: 1920,
                height: 1080,
                sequence: self.sequence,
                presentation_time_ns: self.sequence * 1_000,
            }
        }

        fn record(
            &self,
            context: GraphicsContext,
            command: GraphicsRecordCommand,
        ) -> Result<GraphicsRecordedFrame, String> {
            self.records
                .lock()
                .expect("record calls")
                .push((context, command));
            Ok(GraphicsRecordedFrame {
                resource: 10,
                resource_view: 11,
                texture_format: GraphicsTextureFormat::Rgba8,
                width: 1920,
                height: 1080,
                frame_slot: command.frame_slot,
                generation: self.sequence,
                presentation_time_ns: self.sequence * 1_000,
            })
        }
    }

    impl Drop for TestFrame {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn context() -> GraphicsContext {
        GraphicsContext {
            api: GraphicsApi::Vulkan,
            instance: 1,
            physical_device: 2,
            device: 3,
            queue: 4,
            queue_family_index: 5,
        }
    }

    fn command() -> GraphicsRecordCommand {
        GraphicsRecordCommand {
            command_buffer: 6,
            frame_slot: 7,
        }
    }

    fn frame(
        sequence: u64,
        drops: &Arc<AtomicUsize>,
        records: &Arc<Mutex<Vec<(GraphicsContext, GraphicsRecordCommand)>>>,
    ) -> Arc<dyn GraphicsFrame> {
        Arc::new(TestFrame {
            sequence,
            drops: Arc::clone(drops),
            records: Arc::clone(records),
        })
    }

    #[test]
    fn mailbox_keeps_only_the_latest_frame_and_transfers_ownership_to_the_token() {
        let notifications = Arc::new(AtomicUsize::new(0));
        let callback_notifications = Arc::clone(&notifications);
        let (graphics, publisher) = RenderThreadGraphics::new(move || {
            callback_notifications.fetch_add(1, Ordering::Relaxed);
        });
        graphics.initialize(context()).expect("graphics context");
        let lease = publisher.context().expect("context lease");
        let drops = Arc::new(AtomicUsize::new(0));
        let records = Arc::new(Mutex::new(Vec::new()));

        assert_eq!(
            publisher
                .publish(lease, frame(1, &drops, &records))
                .expect("first frame"),
            GraphicsPublishOutcome::Published
        );
        assert_eq!(
            publisher
                .publish(lease, frame(2, &drops, &records))
                .expect("replacement frame"),
            GraphicsPublishOutcome::Replaced
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(notifications.load(Ordering::Relaxed), 2);

        let token = graphics.acquire_latest().expect("latest frame");
        assert_eq!(token.info().sequence, 2);
        assert_eq!(
            graphics.acquire_latest().err(),
            Some(GraphicsRuntimeError::NoFrame)
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(token);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn record_uses_the_active_context_and_callers_command_stream() {
        let (graphics, publisher) = RenderThreadGraphics::new(|| {});
        graphics.initialize(context()).expect("graphics context");
        let lease = publisher.context().expect("context lease");
        let drops = Arc::new(AtomicUsize::new(0));
        let records = Arc::new(Mutex::new(Vec::new()));
        publisher
            .publish(lease, frame(1, &drops, &records))
            .expect("frame");
        let token = graphics.acquire_latest().expect("latest frame");

        let output = graphics.record(&token, command()).expect("record frame");

        assert_eq!(
            &*records.lock().expect("record calls"),
            &[(context(), command())]
        );
        assert_eq!(output.resource, 10);
        assert_eq!(output.resource_view, 11);
    }

    #[test]
    fn shutdown_drops_pending_frames_and_invalidates_tokens_and_leases() {
        let (graphics, publisher) = RenderThreadGraphics::new(|| {});
        graphics.initialize(context()).expect("graphics context");
        let old_lease = publisher.context().expect("context lease");
        let drops = Arc::new(AtomicUsize::new(0));
        let records = Arc::new(Mutex::new(Vec::new()));
        publisher
            .publish(old_lease, frame(1, &drops, &records))
            .expect("first frame");
        let token = graphics.acquire_latest().expect("latest frame");
        publisher
            .publish(old_lease, frame(2, &drops, &records))
            .expect("pending frame");

        graphics.shutdown().expect("scene graph shutdown");
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(
            graphics.record(&token, command()).err(),
            Some(GraphicsRuntimeError::SceneGraphInactive)
        );
        assert_eq!(
            publisher
                .publish(old_lease, frame(3, &drops, &records))
                .err(),
            Some(GraphicsRuntimeError::SceneGraphInactive)
        );

        graphics.initialize(context()).expect("new scene graph");
        assert_eq!(
            graphics.record(&token, command()).err(),
            Some(GraphicsRuntimeError::StaleFrame)
        );
        assert_eq!(
            publisher
                .publish(old_lease, frame(4, &drops, &records))
                .err(),
            Some(GraphicsRuntimeError::StaleFrame)
        );
    }

    #[test]
    fn graphics_calls_are_bound_to_the_initializing_render_thread() {
        let (graphics, _publisher) = RenderThreadGraphics::new(|| {});
        graphics.initialize(context()).expect("graphics context");
        let other_thread = std::thread::spawn(move || graphics.acquire_latest().err())
            .join()
            .expect("render-thread check");
        assert_eq!(other_thread, Some(GraphicsRuntimeError::WrongThread));
    }
}
