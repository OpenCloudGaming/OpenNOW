//! GFN Input Protocol Encoder
//!
//! Binary protocol for sending input events over WebRTC data channel.

use bytes::{BytesMut, BufMut};
use std::time::Instant;

/// Input event type constants
pub const INPUT_HEARTBEAT: u32 = 2;
pub const INPUT_KEY_DOWN: u32 = 3;  // Type 3 = Key pressed
pub const INPUT_KEY_UP: u32 = 4;    // Type 4 = Key released
pub const INPUT_MOUSE_ABS: u32 = 5;
pub const INPUT_MOUSE_REL: u32 = 7;
pub const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
pub const INPUT_MOUSE_BUTTON_UP: u32 = 9;
pub const INPUT_MOUSE_WHEEL: u32 = 10;

/// Mouse buttons
pub const MOUSE_BUTTON_LEFT: u8 = 0;
pub const MOUSE_BUTTON_RIGHT: u8 = 1;
pub const MOUSE_BUTTON_MIDDLE: u8 = 2;

/// Input events that can be sent to the server
/// Each event carries its own timestamp_us (microseconds since app start)
/// for accurate timing even when events are queued.
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
    start_time: Instant,
    protocol_version: u8,
}

impl InputEncoder {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(256),
            start_time: Instant::now(),
            protocol_version: 2,
        }
    }

    /// Set protocol version (received from handshake)
    pub fn set_protocol_version(&mut self, version: u8) {
        self.protocol_version = version;
    }

    /// Get timestamp in microseconds since start
    fn timestamp_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Encode an input event to binary format
    /// Uses the timestamp embedded in each event (captured at creation time)
    pub fn encode(&mut self, event: &InputEvent) -> Vec<u8> {
        self.buffer.clear();

        match event {
            InputEvent::KeyDown { keycode, scancode, modifiers, timestamp_us } => {
                // Type 3 (Key Down): 18 bytes
                // [type 4B LE][keycode 2B BE][modifiers 2B BE][scancode 2B BE][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_KEY_DOWN);
                self.buffer.put_u16(*keycode);
                self.buffer.put_u16(*modifiers);
                self.buffer.put_u16(*scancode);
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::KeyUp { keycode, scancode, modifiers, timestamp_us } => {
                self.buffer.put_u32_le(INPUT_KEY_UP);
                self.buffer.put_u16(*keycode);
                self.buffer.put_u16(*modifiers);
                self.buffer.put_u16(*scancode);
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseMove { dx, dy, timestamp_us } => {
                // Type 7 (Mouse Relative): 22 bytes
                // [type 4B LE][dx 2B BE][dy 2B BE][reserved 6B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_REL);
                self.buffer.put_i16(*dx);
                self.buffer.put_i16(*dy);
                self.buffer.put_u16(0); // Reserved
                self.buffer.put_u32(0); // Reserved
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseButtonDown { button, timestamp_us } => {
                // Type 8 (Mouse Button Down): 18 bytes
                // [type 4B LE][button 1B][pad 1B][reserved 4B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_BUTTON_DOWN);
                self.buffer.put_u8(*button);
                self.buffer.put_u8(0); // Padding
                self.buffer.put_u32(0); // Reserved
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseButtonUp { button, timestamp_us } => {
                self.buffer.put_u32_le(INPUT_MOUSE_BUTTON_UP);
                self.buffer.put_u8(*button);
                self.buffer.put_u8(0);
                self.buffer.put_u32(0);
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::MouseWheel { delta, timestamp_us } => {
                // Type 10 (Mouse Wheel): 22 bytes
                // [type 4B LE][horiz 2B BE][vert 2B BE][reserved 6B][timestamp 8B BE]
                self.buffer.put_u32_le(INPUT_MOUSE_WHEEL);
                self.buffer.put_i16(0);        // Horizontal (unused)
                self.buffer.put_i16(*delta);   // Vertical (positive = scroll up)
                self.buffer.put_u16(0);        // Reserved
                self.buffer.put_u32(0);        // Reserved
                self.buffer.put_u64(*timestamp_us);
            }

            InputEvent::Heartbeat => {
                // Type 2 (Heartbeat): 4 bytes
                self.buffer.put_u32_le(INPUT_HEARTBEAT);
            }
        }

        // Protocol v3+ requires single event wrapper
        // Official client uses: [0x22][payload] for single events
        if self.protocol_version > 2 {
            let payload = self.buffer.to_vec();
            let mut final_buf = BytesMut::with_capacity(1 + payload.len());

            // Single event wrapper marker (34 = 0x22)
            final_buf.put_u8(0x22);
            // Payload (already contains timestamp)
            final_buf.extend_from_slice(&payload);

            final_buf.to_vec()
        } else {
            self.buffer.to_vec()
        }
    }

    /// Encode handshake response
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
        let event = InputEvent::MouseMove { dx: -1, dy: 5, timestamp_us: 12345 };
        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 22);
        // Type 7 in LE
        assert_eq!(&encoded[0..4], &[0x07, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_heartbeat_encoding() {
        let mut encoder = InputEncoder::new();
        let event = InputEvent::Heartbeat;
        let encoded = encoder.encode(&event);

        assert_eq!(encoded.len(), 4);
        assert_eq!(&encoded[0..4], &[0x02, 0x00, 0x00, 0x00]);
    }
}
