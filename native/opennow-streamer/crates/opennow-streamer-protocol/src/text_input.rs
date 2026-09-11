use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub const MAX_TEXT_BYTES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputError {
    Invalid,
    TooLarge,
    Busy,
    Unavailable,
}

#[derive(Debug, Default)]
pub struct TextInputSlot(Mutex<Weak<TextPayload>>);

impl TextInputSlot {
    pub fn submit(&self, bytes: &[u8]) -> Result<UnicodeText, TextInputError> {
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(TextInputError::TooLarge);
        }
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(TextInputError::Invalid);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| TextInputError::Invalid)?;
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.upgrade().is_some() {
            return Err(TextInputError::Busy);
        }
        let payload = Arc::new(TextPayload {
            text: text.to_owned(),
            cancelled: AtomicBool::new(false),
        });
        *slot = Arc::downgrade(&payload);
        Ok(UnicodeText(payload))
    }

    pub fn cancel(&self) {
        if let Some(payload) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .upgrade()
        {
            payload.cancelled.store(true, Ordering::Release);
        }
    }
}

struct TextPayload {
    text: String,
    cancelled: AtomicBool,
}

#[derive(Clone)]
pub struct UnicodeText(Arc<TextPayload>);

impl UnicodeText {
    pub fn as_str(&self) -> &str {
        &self.0.text
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for UnicodeText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnicodeText")
            .field("bytes", &self.0.text.len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for UnicodeText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for UnicodeText {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_text_without_truncation_or_payload_diagnostics() {
        let slot = TextInputSlot::default();
        for invalid in [
            &b""[..],
            &b"a\0b"[..],
            &[0xff],
            &[0xc0, 0x80],
            &[0xed, 0xa0, 0x80],
        ] {
            assert_eq!(slot.submit(invalid), Err(TextInputError::Invalid));
        }
        assert_eq!(
            slot.submit(&vec![b'a'; MAX_TEXT_BYTES + 1]),
            Err(TextInputError::TooLarge)
        );
        let text = slot.submit("secret café 世界 🦫\r\n".as_bytes()).unwrap();
        assert_eq!(text.as_str(), "secret café 世界 🦫\r\n");
        assert!(!format!("{text:?}").contains("secret"));
        drop(text);
        assert_eq!(
            slot.submit(&vec![b'a'; MAX_TEXT_BYTES])
                .unwrap()
                .as_str()
                .len(),
            MAX_TEXT_BYTES
        );
    }

    #[test]
    fn one_paste_remains_reserved_through_clones_and_cancellation() {
        let slot = TextInputSlot::default();
        let text = slot.submit(b"first").unwrap();
        let transport = text.clone();
        drop(text);
        assert_eq!(slot.submit(b"second"), Err(TextInputError::Busy));
        slot.cancel();
        assert!(transport.is_cancelled());
        assert_eq!(slot.submit(b"second"), Err(TextInputError::Busy));
        drop(transport);
        let next = slot.submit(b"second").unwrap();
        assert!(!next.is_cancelled());
    }
}
