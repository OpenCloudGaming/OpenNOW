//! GFN Input Protocol Encoder
//!
//! Binary protocol format discovered from vendor.js:
//! - Event type: 4 bytes, Little Endian
//! - Data fields: Big Endian
//! - Timestamp: 8 bytes, Big Endian, in microseconds

use bytes::{BytesMut, BufMut};

/// Input event type constants
pub const INPUT_HEARTBEAT: u32 = 2;
pub const INPUT_KEY_UP: u32 = 3;
pub const INPUT_KEY_DOWN: u32 = 4;
pub const INPUT_MOUSE_ABS: u32 = 5;
pub const INPUT_MOUSE_REL: u32 = 7;
pub const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
pub const INPUT_MOUSE_BUTTON_UP: u32 = 9;
pub const INPUT_MOUSE_WHEEL: u32 = 10;

/// Input events that can be sent to the server
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Keyboard key pressed
    KeyDown {
        keycode: u16,
        scancode: u16,
        modifiers: u16,
        timestamp_us: u64,
    },
    /// Keyboard key released
    KeyUp {
        keycode: u16,
        scancode: u16,
        modifiers: u16,
        timestamp_us: u64,
    },
    /// Mouse moved (relative)
    MouseMove {
        dx: i16,
        dy: i16,
        timestamp_us: u64,
    },
    /// Mouse button pressed
    MouseButtonDown {
        button: u8,
        timestamp_us: u64,
    },
    /// Mouse button released
    MouseButtonUp {
        button: u8,
        timestamp_us: u64,
    },
    /// Mouse wheel scrolled
    MouseWheel {
        delta: i16,
        timestamp_us: u64,
    },
    /// Heartbeat (keep-alive)
    Heartbeat,
}

/// Encoder for GFN input protocol
pub struct InputEncoder {
    buffer: BytesMut,
}

impl InputEncoder {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(256),
        }
    }

    /// Encode an input event to binary format
    ///
    /// Format from vendor.js analysis:
    /// - Type: 4 bytes LE
    /// - Data: varies by type, Big Endian for multi-byte values
    /// - Timestamp: 8 bytes BE, microseconds
    pub fn encode(&mut self, event: &InputEvent) -> Vec<u8> {
        self.buffer.clear();

        match event {
            InputEvent::KeyDown { keycode, scancode, modifiers, timestamp_us } => {
                // Keyboard (Yc): 18 bytes
                // [type 4B LE][keycode 2B BE][modifiers 2B BE][scancode 2B BE][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_KEY_DOWN);
                self.buffer.put_u16(*keycode);      // BE (default)
                self.buffer.put_u16(*modifiers);    // BE
                self.buffer.put_u16(*scancode);     // BE
                self.buffer.put_u64(*timestamp_us); // BE
            }

            InputEvent::KeyUp { keycode, scancode, modifiers, timestamp_us } => {
                // Same format as KeyDown but with type 3
                self.buffer.put_u32_le(INPUT_KEY_UP);
                self.buffer.put_u16(*keycode);
                self.buffer.put_u16(*modifiers);
                self.buffer.put_u16(*scancode);
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseMove { dx, dy, timestamp_us } => {
                // Mouse Relative (Gc): 22 bytes
                // [type 4B LE][dx 2B BE][dy 2B BE][reserved 2B][reserved 4B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_REL);
                self.buffer.put_i16(*dx);           // BE
                self.buffer.put_i16(*dy);           // BE
                self.buffer.put_u16(0);             // Reserved
                self.buffer.put_u32(0);             // Reserved
                self.buffer.put_u64(*timestamp_us); // BE
            }

            InputEvent::MouseButtonDown { button, timestamp_us } => {
                // Mouse Button (xc): 18 bytes
                // [type 4B LE][button 1B][pad 1B][reserved 4B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_BUTTON_DOWN);
                self.buffer.put_u8(*button);
                self.buffer.put_u8(0);              // Padding
                self.buffer.put_u32(0);             // Reserved
                self.buffer.put_u64(*timestamp_us); // BE
            }

            InputEvent::MouseButtonUp { button, timestamp_us } => {
                // Same format as MouseButtonDown but with type 9
                self.buffer.put_u32_le(INPUT_MOUSE_BUTTON_UP);
                self.buffer.put_u8(*button);
                self.buffer.put_u8(0);
                self.buffer.put_u32(0);
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseWheel { delta, timestamp_us } => {
                // Mouse Wheel (Lc): 22 bytes
                // [type 4B LE][horiz 2B BE][vert 2B BE][reserved 2B BE][reserved 4B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_WHEEL);
                self.buffer.put_i16(0);             // Horizontal (unused)
                self.buffer.put_i16(-*delta);       // Vertical (negated per vendor.js)
                self.buffer.put_u16(0);             // Reserved
                self.buffer.put_u32(0);             // Reserved
                self.buffer.put_u64(*timestamp_us); // BE
            }

            InputEvent::Heartbeat => {
                // Heartbeat (Jc): 4 bytes
                // [type 4B LE]
                self.buffer.put_u32_le(INPUT_HEARTBEAT);
            }
        }

        self.buffer.to_vec()
    }

    /// Encode the protocol handshake response
    ///
    /// When server sends [0x0e, major, minor, flags], we echo it back
    pub fn encode_handshake_response(major: u8, minor: u8, flags: u8) -> Vec<u8> {
        vec![0x0e, major, minor, flags]
    }
}

impl Default for InputEncoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_move_encoding() {
        let mut encoder = InputEncoder::new();
        let event = InputEvent::MouseMove {
            dx: -1,
            dy: 5,
            timestamp_us: 1000000, // 1 second
        };

        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 22);
        // Type 7 in LE
        assert_eq!(&encoded[0..4], &[0x07, 0x00, 0x00, 0x00]);
        // dx = -1 in BE = 0xFFFF
        assert_eq!(&encoded[4..6], &[0xFF, 0xFF]);
        // dy = 5 in BE
        assert_eq!(&encoded[6..8], &[0x00, 0x05]);
    }

    #[test]
    fn test_mouse_button_encoding() {
        let mut encoder = InputEncoder::new();
        let event = InputEvent::MouseButtonDown {
            button: 0,
            timestamp_us: 500000,
        };

        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 18);
        // Type 8 in LE
        assert_eq!(&encoded[0..4], &[0x08, 0x00, 0x00, 0x00]);
        // Button 0
        assert_eq!(encoded[4], 0x00);
    }

    #[test]
    fn test_keyboard_encoding() {
        let mut encoder = InputEncoder::new();
        let event = InputEvent::KeyDown {
            keycode: 68, // 'D'
            scancode: 0,
            modifiers: 0,
            timestamp_us: 750000,
        };

        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 18);
        // Type 4 in LE (keydown)
        assert_eq!(&encoded[0..4], &[0x04, 0x00, 0x00, 0x00]);
        // Keycode 68 in BE
        assert_eq!(&encoded[4..6], &[0x00, 0x44]);
    }

    #[test]
    fn test_heartbeat_encoding() {
        let mut encoder = InputEncoder::new();
        let event = InputEvent::Heartbeat;

        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 4);
        // Type 2 in LE
        assert_eq!(&encoded[0..4], &[0x02, 0x00, 0x00, 0x00]);
    }
}
