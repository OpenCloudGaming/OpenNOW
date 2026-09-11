use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BackendState {
    Running = 1,
    Stopping = 2,
    Stopped = 3,
}

pub(crate) struct Lifecycle(AtomicU8);

pub(crate) struct AttachmentLifecycle {
    attached: bool,
}

impl AttachmentLifecycle {
    pub(crate) const fn attached() -> Self {
        Self { attached: true }
    }

    pub(crate) fn begin_detach(&mut self) -> bool {
        std::mem::replace(&mut self.attached, false)
    }
}

impl Lifecycle {
    pub(crate) const fn running() -> Self {
        Self(AtomicU8::new(BackendState::Running as u8))
    }

    pub(crate) fn state(&self) -> BackendState {
        match self.0.load(Ordering::Acquire) {
            1 => BackendState::Running,
            2 => BackendState::Stopping,
            _ => BackendState::Stopped,
        }
    }

    pub(crate) fn begin_stop(&self) -> bool {
        self.0
            .compare_exchange(
                BackendState::Running as u8,
                BackendState::Stopping as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(crate) fn finish_stop(&self) {
        self.0.store(BackendState::Stopped as u8, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_transition_is_idempotent() {
        let lifecycle = Lifecycle::running();
        assert_eq!(lifecycle.state(), BackendState::Running);
        assert!(lifecycle.begin_stop());
        assert!(!lifecycle.begin_stop());
        assert_eq!(lifecycle.state(), BackendState::Stopping);
        lifecycle.finish_stop();
        assert_eq!(lifecycle.state(), BackendState::Stopped);
        assert!(!lifecycle.begin_stop());
    }

    #[test]
    fn surface_attachment_detaches_exactly_once() {
        let mut lifecycle = AttachmentLifecycle::attached();
        assert!(lifecycle.begin_detach());
        assert!(!lifecycle.begin_detach());
    }
}
