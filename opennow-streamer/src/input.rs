use std::sync::OnceLock;

use winit::{event::MouseButton, keyboard::KeyCode};

pub const INPUT_HEARTBEAT: u32 = 2;
pub const INPUT_KEY_DOWN: u32 = 3;
pub const INPUT_KEY_UP: u32 = 4;
pub const INPUT_MOUSE_REL: u32 = 7;
pub const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
pub const INPUT_MOUSE_BUTTON_UP: u32 = 9;
pub const INPUT_MOUSE_WHEEL: u32 = 10;
pub const INPUT_GAMEPAD: u32 = 12;

fn now_micros() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_micros() as u64
}

fn wrap_single_event(payload: &[u8], protocol_version: u16) -> Vec<u8> {
    if protocol_version <= 2 {
        return payload.to_vec();
    }
    let mut wrapped = Vec::with_capacity(10 + payload.len());
    wrapped.push(0x23);
    wrapped.extend_from_slice(&now_micros().to_be_bytes());
    wrapped.push(0x22);
    wrapped.extend_from_slice(payload);
    wrapped
}

fn wrap_batched_event(payload: &[u8], protocol_version: u16) -> Vec<u8> {
    if protocol_version <= 2 {
        return payload.to_vec();
    }
    let mut wrapped = Vec::with_capacity(12 + payload.len());
    wrapped.push(0x23);
    wrapped.extend_from_slice(&now_micros().to_be_bytes());
    wrapped.push(0x21);
    wrapped.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    wrapped.extend_from_slice(payload);
    wrapped
}

pub fn encode_heartbeat() -> Vec<u8> {
    INPUT_HEARTBEAT.to_le_bytes().to_vec()
}

pub fn encode_key(key_code: u16, scancode: u16, modifiers: u16, down: bool, protocol_version: u16) -> Vec<u8> {
    let mut payload = vec![0_u8; 18];
    payload[0..4].copy_from_slice(&(if down { INPUT_KEY_DOWN } else { INPUT_KEY_UP }).to_le_bytes());
    payload[4..6].copy_from_slice(&key_code.to_be_bytes());
    payload[6..8].copy_from_slice(&modifiers.to_be_bytes());
    payload[8..10].copy_from_slice(&scancode.to_be_bytes());
    payload[10..18].copy_from_slice(&now_micros().to_be_bytes());
    wrap_single_event(&payload, protocol_version)
}

pub fn encode_mouse_move(dx: i16, dy: i16, protocol_version: u16) -> Vec<u8> {
    let mut payload = vec![0_u8; 22];
    payload[0..4].copy_from_slice(&INPUT_MOUSE_REL.to_le_bytes());
    payload[4..6].copy_from_slice(&dx.to_be_bytes());
    payload[6..8].copy_from_slice(&dy.to_be_bytes());
    payload[14..22].copy_from_slice(&now_micros().to_be_bytes());
    wrap_batched_event(&payload, protocol_version)
}

pub fn encode_mouse_button(button: u8, down: bool, protocol_version: u16) -> Vec<u8> {
    let mut payload = vec![0_u8; 18];
    payload[0..4].copy_from_slice(&(if down { INPUT_MOUSE_BUTTON_DOWN } else { INPUT_MOUSE_BUTTON_UP }).to_le_bytes());
    payload[4] = button;
    payload[10..18].copy_from_slice(&now_micros().to_be_bytes());
    wrap_single_event(&payload, protocol_version)
}

pub fn encode_mouse_wheel(delta: i16, protocol_version: u16) -> Vec<u8> {
    let mut payload = vec![0_u8; 22];
    payload[0..4].copy_from_slice(&INPUT_MOUSE_WHEEL.to_le_bytes());
    payload[6..8].copy_from_slice(&delta.to_be_bytes());
    payload[14..22].copy_from_slice(&now_micros().to_be_bytes());
    wrap_single_event(&payload, protocol_version)
}

fn wrap_gamepad_reliable(payload: &[u8], protocol_version: u16) -> Vec<u8> {
    wrap_batched_event(payload, protocol_version)
}

fn wrap_gamepad_partially_reliable(payload: &[u8], controller_id: u8, sequence: u16, protocol_version: u16) -> Vec<u8> {
    if protocol_version <= 2 {
        return payload.to_vec();
    }
    let mut wrapped = Vec::with_capacity(16 + payload.len());
    wrapped.push(0x23);
    wrapped.extend_from_slice(&now_micros().to_be_bytes());
    wrapped.push(0x26);
    wrapped.push(controller_id);
    wrapped.extend_from_slice(&sequence.to_be_bytes());
    wrapped.push(0x21);
    wrapped.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    wrapped.extend_from_slice(payload);
    wrapped
}

pub fn parse_input_handshake(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }
    let first_word = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first_word == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }
    if bytes[0] == 0x0e {
        return Some(first_word);
    }
    None
}

pub struct GamepadState {
    pub controller_id: u8,
    pub bitmap: u16,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
}

pub fn encode_gamepad(state: &GamepadState, use_partially_reliable: bool, sequence: u16, protocol_version: u16) -> Vec<u8> {
    let mut payload = vec![0_u8; 38];
    payload[0..4].copy_from_slice(&INPUT_GAMEPAD.to_le_bytes());
    payload[4..6].copy_from_slice(&(26_u16).to_le_bytes());
    payload[6..8].copy_from_slice(&(state.controller_id as u16).to_le_bytes());
    payload[8..10].copy_from_slice(&state.bitmap.to_le_bytes());
    payload[10..12].copy_from_slice(&(20_u16).to_le_bytes());
    payload[12..14].copy_from_slice(&state.buttons.to_le_bytes());
    payload[14..16].copy_from_slice(&u16::from_le_bytes([state.left_trigger, state.right_trigger]).to_le_bytes());
    payload[16..18].copy_from_slice(&state.left_x.to_le_bytes());
    payload[18..20].copy_from_slice(&state.left_y.to_le_bytes());
    payload[20..22].copy_from_slice(&state.right_x.to_le_bytes());
    payload[22..24].copy_from_slice(&state.right_y.to_le_bytes());
    payload[26..28].copy_from_slice(&(85_u16).to_le_bytes());
    payload[30..38].copy_from_slice(&now_micros().to_le_bytes());
    if use_partially_reliable {
        wrap_gamepad_partially_reliable(&payload, state.controller_id, sequence, protocol_version)
    } else {
        wrap_gamepad_reliable(&payload, protocol_version)
    }
}

pub fn key_mapping(code: KeyCode) -> Option<(u16, u16)> {
    Some(match code {
        KeyCode::KeyA => (0x41, 0x04),
        KeyCode::KeyB => (0x42, 0x05),
        KeyCode::KeyC => (0x43, 0x06),
        KeyCode::KeyD => (0x44, 0x07),
        KeyCode::KeyE => (0x45, 0x08),
        KeyCode::KeyF => (0x46, 0x09),
        KeyCode::KeyG => (0x47, 0x0A),
        KeyCode::KeyH => (0x48, 0x0B),
        KeyCode::KeyI => (0x49, 0x0C),
        KeyCode::KeyJ => (0x4A, 0x0D),
        KeyCode::KeyK => (0x4B, 0x0E),
        KeyCode::KeyL => (0x4C, 0x0F),
        KeyCode::KeyM => (0x4D, 0x10),
        KeyCode::KeyN => (0x4E, 0x11),
        KeyCode::KeyO => (0x4F, 0x12),
        KeyCode::KeyP => (0x50, 0x13),
        KeyCode::KeyQ => (0x51, 0x14),
        KeyCode::KeyR => (0x52, 0x15),
        KeyCode::KeyS => (0x53, 0x16),
        KeyCode::KeyT => (0x54, 0x17),
        KeyCode::KeyU => (0x55, 0x18),
        KeyCode::KeyV => (0x56, 0x19),
        KeyCode::KeyW => (0x57, 0x1A),
        KeyCode::KeyX => (0x58, 0x1B),
        KeyCode::KeyY => (0x59, 0x1C),
        KeyCode::KeyZ => (0x5A, 0x1D),
        KeyCode::Digit0 => (0x30, 0x27),
        KeyCode::Digit1 => (0x31, 0x1E),
        KeyCode::Digit2 => (0x32, 0x1F),
        KeyCode::Digit3 => (0x33, 0x20),
        KeyCode::Digit4 => (0x34, 0x21),
        KeyCode::Digit5 => (0x35, 0x22),
        KeyCode::Digit6 => (0x36, 0x23),
        KeyCode::Digit7 => (0x37, 0x24),
        KeyCode::Digit8 => (0x38, 0x25),
        KeyCode::Digit9 => (0x39, 0x26),
        KeyCode::Enter => (0x0D, 0x28),
        KeyCode::Escape => (0x1B, 0x29),
        KeyCode::Backspace => (0x08, 0x2A),
        KeyCode::Tab => (0x09, 0x2B),
        KeyCode::Space => (0x20, 0x2C),
        KeyCode::ArrowLeft => (0x25, 0x50),
        KeyCode::ArrowRight => (0x27, 0x4F),
        KeyCode::ArrowUp => (0x26, 0x52),
        KeyCode::ArrowDown => (0x28, 0x51),
        KeyCode::ShiftLeft => (0xA0, 0xE1),
        KeyCode::ShiftRight => (0xA1, 0xE5),
        KeyCode::ControlLeft => (0xA2, 0xE0),
        KeyCode::ControlRight => (0xA3, 0xE4),
        KeyCode::AltLeft => (0xA4, 0xE2),
        KeyCode::AltRight => (0xA5, 0xE6),
        KeyCode::SuperLeft => (0x5B, 0xE3),
        KeyCode::SuperRight => (0x5C, 0xE7),
        KeyCode::F1 => (0x70, 0x3A),
        KeyCode::F2 => (0x71, 0x3B),
        KeyCode::F3 => (0x72, 0x3C),
        KeyCode::F4 => (0x73, 0x3D),
        KeyCode::F5 => (0x74, 0x3E),
        KeyCode::F6 => (0x75, 0x3F),
        KeyCode::F7 => (0x76, 0x40),
        KeyCode::F8 => (0x77, 0x41),
        KeyCode::F9 => (0x78, 0x42),
        KeyCode::F10 => (0x79, 0x43),
        KeyCode::F11 => (0x7A, 0x44),
        KeyCode::F12 => (0x7B, 0x45),
        _ => return None,
    })
}

pub fn modifier_flags(shift: bool, ctrl: bool, alt: bool, meta: bool) -> u16 {
    let mut flags = 0_u16;
    if shift { flags |= 0x01; }
    if ctrl { flags |= 0x02; }
    if alt { flags |= 0x04; }
    if meta { flags |= 0x08; }
    flags
}

pub fn mouse_button(button: MouseButton) -> Option<u8> {
    Some(match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
        _ => return None,
    })
}
