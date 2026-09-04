#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
compile_error!("the Linux backend currently supports x86_64 and aarch64");

#[cfg(target_os = "linux")]
mod audio;
#[cfg(target_os = "linux")]
mod capability;
#[cfg(target_os = "linux")]
mod error;
#[cfg(target_os = "linux")]
mod format;
#[cfg(all(target_os = "linux", feature = "vulkan"))]
mod frame_producer;
#[cfg(all(target_os = "linux", feature = "vulkan"))]
mod presentation;
#[cfg(target_os = "linux")]
mod queue;
#[cfg(target_os = "linux")]
mod session;
#[cfg(target_os = "linux")]
mod video;

#[cfg(target_os = "linux")]
pub use audio::{AudioBackend, AudioBackendPreference, AudioConfig, AudioPacket};
#[cfg(target_os = "linux")]
pub use capability::{
    BackendCapability, CapabilityReport, PresentationCapability, probe_capabilities,
    probe_video_capabilities,
};
#[cfg(target_os = "linux")]
pub use error::{Error, Result, Subsystem};
#[cfg(target_os = "linux")]
pub use format::{
    ChromaLocation, ColorMatrix, ColorRange, DecodedVideoFrame, DmaBufFrame, DmaBufLayer,
    DmaBufObject, DmaBufPlane, EncodedVideoFrame, FramePlane, PixelFormat, StreamFormat,
    VideoCodec, VulkanImage, VulkanVideoFrame,
};
#[cfg(all(target_os = "linux", feature = "vulkan"))]
pub use frame_producer::{
    CpuNv12Frame, ImportedNv12Frame, LinuxFrameProducer, LinuxGpuFrame, LinuxGpuFrameProducer,
    PreparedLinuxFrame, PreparedVulkanFrame, PreparedVulkanImage, RecordedGpuFrame,
    VulkanRenderDevice,
};
#[cfg(all(target_os = "linux", feature = "vulkan"))]
pub use presentation::{NativeSurface, VulkanPresenter};
#[cfg(target_os = "linux")]
pub use session::{
    BackendEvent, DecoderBackend, DecoderPreference, LifecycleState, LinuxSession, PushOutcome,
    SessionConfig,
};
