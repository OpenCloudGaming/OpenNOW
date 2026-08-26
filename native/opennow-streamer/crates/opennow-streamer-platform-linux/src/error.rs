use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    VaApi,
    V4l2,
    Vulkan,
    Ffmpeg,
    Opus,
    PipeWire,
    Alsa,
    Session,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{subsystem:?} is unavailable: {reason}")]
    Unavailable {
        subsystem: Subsystem,
        reason: String,
    },
    #[error("{subsystem:?} device was lost: {reason}")]
    DeviceLost {
        subsystem: Subsystem,
        reason: String,
    },
    #[error("invalid media format: {0}")]
    InvalidFormat(String),
    #[error("queue is closed")]
    QueueClosed,
    #[error("session is not running")]
    NotRunning,
    #[error("{subsystem:?} I/O failed: {source}")]
    Io {
        subsystem: Subsystem,
        #[source]
        source: io::Error,
    },
    #[error("{subsystem:?} operation failed: {reason}")]
    Backend {
        subsystem: Subsystem,
        reason: String,
    },
    #[error("worker thread panicked: {0}")]
    WorkerPanic(&'static str),
}

impl Error {
    pub(crate) fn unavailable(subsystem: Subsystem, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            subsystem,
            reason: reason.into(),
        }
    }

    pub(crate) fn backend(subsystem: Subsystem, reason: impl Into<String>) -> Self {
        Self::Backend {
            subsystem,
            reason: reason.into(),
        }
    }

    pub(crate) fn io(subsystem: Subsystem, source: io::Error) -> Self {
        if matches!(
            source.raw_os_error(),
            Some(libc::ENODEV | libc::EIO | libc::ENXIO)
        ) {
            return Self::DeviceLost {
                subsystem,
                reason: source.to_string(),
            };
        }
        Self::Io { subsystem, source }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
