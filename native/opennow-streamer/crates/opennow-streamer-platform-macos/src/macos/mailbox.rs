use std::sync::Mutex;

pub(super) struct LatestMailbox<T> {
    value: Mutex<Option<T>>,
}

impl<T> LatestMailbox<T> {
    pub(super) fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    pub(super) fn replace(&self, value: T) -> bool {
        self.value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .replace(value)
            .is_some()
    }

    pub(super) fn take(&self) -> Option<T> {
        self.value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    pub(super) fn clear(&self) -> bool {
        self.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::LatestMailbox;

    #[test]
    fn replaces_stale_value_and_consumes_latest_once() {
        let mailbox = LatestMailbox::new();
        assert!(!mailbox.replace(1));
        assert!(mailbox.replace(2));
        assert_eq!(mailbox.take(), Some(2));
        assert_eq!(mailbox.take(), None);
    }

    #[test]
    fn clear_reports_whether_a_value_was_discarded() {
        let mailbox = LatestMailbox::new();
        assert!(!mailbox.clear());
        mailbox.replace(3);
        assert!(mailbox.clear());
        assert!(!mailbox.clear());
    }
}
