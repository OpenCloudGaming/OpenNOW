//! Input Protocol Constants
//!
//! GFN input protocol definitions.

/// Input event types
pub mod event_types {
    pub const HEARTBEAT: u32 = 2;
    pub const KEY_UP: u32 = 3;
    pub const KEY_DOWN: u32 = 4;
    pub const MOUSE_ABS: u32 = 5;
    pub const MOUSE_REL: u32 = 7;
    pub const MOUSE_BUTTON_DOWN: u32 = 8;
    pub const MOUSE_BUTTON_UP: u32 = 9;
    pub const MOUSE_WHEEL: u32 = 10;
}

/// Mouse button indices
pub mod mouse_buttons {
    pub const LEFT: u8 = 0;
    pub const RIGHT: u8 = 1;
    pub const MIDDLE: u8 = 2;
    pub const BACK: u8 = 3;
    pub const FORWARD: u8 = 4;
}

/// Keyboard modifier flags
pub mod modifiers {
    pub const SHIFT: u16 = 0x01;
    pub const CTRL: u16 = 0x02;
    pub const ALT: u16 = 0x04;
    pub const META: u16 = 0x08;
    pub const CAPS_LOCK: u16 = 0x10;
    pub const NUM_LOCK: u16 = 0x20;
}

/// Common scancodes (USB HID)
pub mod scancodes {
    pub const A: u16 = 0x04;
    pub const B: u16 = 0x05;
    pub const C: u16 = 0x06;
    pub const D: u16 = 0x07;
    pub const E: u16 = 0x08;
    pub const F: u16 = 0x09;
    pub const G: u16 = 0x0A;
    pub const H: u16 = 0x0B;
    pub const I: u16 = 0x0C;
    pub const J: u16 = 0x0D;
    pub const K: u16 = 0x0E;
    pub const L: u16 = 0x0F;
    pub const M: u16 = 0x10;
    pub const N: u16 = 0x11;
    pub const O: u16 = 0x12;
    pub const P: u16 = 0x13;
    pub const Q: u16 = 0x14;
    pub const R: u16 = 0x15;
    pub const S: u16 = 0x16;
    pub const T: u16 = 0x17;
    pub const U: u16 = 0x18;
    pub const V: u16 = 0x19;
    pub const W: u16 = 0x1A;
    pub const X: u16 = 0x1B;
    pub const Y: u16 = 0x1C;
    pub const Z: u16 = 0x1D;

    pub const NUM_1: u16 = 0x1E;
    pub const NUM_2: u16 = 0x1F;
    pub const NUM_3: u16 = 0x20;
    pub const NUM_4: u16 = 0x21;
    pub const NUM_5: u16 = 0x22;
    pub const NUM_6: u16 = 0x23;
    pub const NUM_7: u16 = 0x24;
    pub const NUM_8: u16 = 0x25;
    pub const NUM_9: u16 = 0x26;
    pub const NUM_0: u16 = 0x27;

    pub const ENTER: u16 = 0x28;
    pub const ESCAPE: u16 = 0x29;
    pub const BACKSPACE: u16 = 0x2A;
    pub const TAB: u16 = 0x2B;
    pub const SPACE: u16 = 0x2C;

    pub const F1: u16 = 0x3A;
    pub const F2: u16 = 0x3B;
    pub const F3: u16 = 0x3C;
    pub const F4: u16 = 0x3D;
    pub const F5: u16 = 0x3E;
    pub const F6: u16 = 0x3F;
    pub const F7: u16 = 0x40;
    pub const F8: u16 = 0x41;
    pub const F9: u16 = 0x42;
    pub const F10: u16 = 0x43;
    pub const F11: u16 = 0x44;
    pub const F12: u16 = 0x45;

    pub const LEFT_CTRL: u16 = 0xE0;
    pub const LEFT_SHIFT: u16 = 0xE1;
    pub const LEFT_ALT: u16 = 0xE2;
    pub const LEFT_META: u16 = 0xE3;
    pub const RIGHT_CTRL: u16 = 0xE4;
    pub const RIGHT_SHIFT: u16 = 0xE5;
    pub const RIGHT_ALT: u16 = 0xE6;
    pub const RIGHT_META: u16 = 0xE7;
}
