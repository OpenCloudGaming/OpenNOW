#![allow(dead_code)]

use std::collections::HashMap;

pub const INPUT_HEARTBEAT: u32 = 2;
pub const INPUT_KEY_DOWN: u32 = 3;
pub const INPUT_KEY_UP: u32 = 4;
pub const INPUT_MOUSE_REL: u32 = 7;
pub const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
pub const INPUT_MOUSE_BUTTON_UP: u32 = 9;
pub const INPUT_MOUSE_WHEEL: u32 = 10;
pub const INPUT_GAMEPAD: u32 = 12;

pub const GAMEPAD_MAX_CONTROLLERS: u8 = 4;
pub const GAMEPAD_PACKET_SIZE: usize = 38;
pub const PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL: u32 = (1 << GAMEPAD_MAX_CONTROLLERS) - 1;
pub const PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL: u32 = 0xFFFF_FFFF;

const WRAPPER_LEGACY_INPUT: u8 = 0x21;
const WRAPPER_SINGLE_INPUT: u8 = 0x22;
const WRAPPER_PARTIALLY_RELIABLE_INPUT: u8 = 0x26;
const WRAPPER_VERSION_MARKER: u8 = 0x23;
const GAMEPAD_PAYLOAD_SIZE: u16 = 26;
const GAMEPAD_INNER_SIZE: u16 = 20;
const GAMEPAD_RESERVED_MARKER: u16 = 85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardPayload {
    pub keycode: u16,
    pub scancode: u16,
    pub modifiers: u16,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseMovePayload {
    pub dx: i16,
    pub dy: i16,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseButtonPayload {
    pub button: u8,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseWheelPayload {
    pub delta: i16,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamepadInput {
    pub controller_id: u8,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
    pub connected: bool,
    pub timestamp_us: u64,
}

#[derive(Debug, Clone)]
pub struct InputEncoder {
    protocol_version: u8,
    gamepad_sequences: HashMap<u8, u16>,
}

impl Default for InputEncoder {
    fn default() -> Self {
        Self {
            protocol_version: 2,
            gamepad_sequences: HashMap::new(),
        }
    }
}

impl InputEncoder {
    pub fn new(protocol_version: u8) -> Self {
        Self {
            protocol_version,
            gamepad_sequences: HashMap::new(),
        }
    }

    pub fn protocol_version(&self) -> u8 {
        self.protocol_version
    }

    pub fn set_protocol_version(&mut self, protocol_version: u8) {
        self.protocol_version = protocol_version;
    }

    pub fn reset_gamepad_sequences(&mut self) {
        self.gamepad_sequences.clear();
    }

    pub fn encode_heartbeat(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(4);
        put_u32_le(&mut payload, INPUT_HEARTBEAT);
        payload
    }

    pub fn encode_key_down(&self, payload: KeyboardPayload) -> Vec<u8> {
        self.encode_keyboard(INPUT_KEY_DOWN, payload)
    }

    pub fn encode_key_up(&self, payload: KeyboardPayload) -> Vec<u8> {
        self.encode_keyboard(INPUT_KEY_UP, payload)
    }

    pub fn encode_mouse_move(&self, payload: MouseMovePayload) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(22);
        put_u32_le(&mut bytes, INPUT_MOUSE_REL);
        put_i16_be(&mut bytes, payload.dx);
        put_i16_be(&mut bytes, payload.dy);
        put_u16_be(&mut bytes, 0);
        put_u32_be(&mut bytes, 0);
        put_u64_be(&mut bytes, payload.timestamp_us);
        wrap_legacy_input(self.protocol_version, payload.timestamp_us, &bytes)
    }

    pub fn encode_mouse_button_down(&self, payload: MouseButtonPayload) -> Vec<u8> {
        self.encode_mouse_button(INPUT_MOUSE_BUTTON_DOWN, payload)
    }

    pub fn encode_mouse_button_up(&self, payload: MouseButtonPayload) -> Vec<u8> {
        self.encode_mouse_button(INPUT_MOUSE_BUTTON_UP, payload)
    }

    pub fn encode_mouse_wheel(&self, payload: MouseWheelPayload) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(22);
        put_u32_le(&mut bytes, INPUT_MOUSE_WHEEL);
        put_i16_be(&mut bytes, 0);
        put_i16_be(&mut bytes, payload.delta);
        put_u16_be(&mut bytes, 0);
        put_u32_be(&mut bytes, 0);
        put_u64_be(&mut bytes, payload.timestamp_us);
        wrap_single_input(self.protocol_version, payload.timestamp_us, &bytes)
    }

    pub fn encode_gamepad_state(
        &mut self,
        bitmap: u16,
        input: GamepadInput,
        use_partially_reliable: bool,
    ) -> Vec<u8> {
        let payload = encode_gamepad_payload(bitmap, input);
        if !use_partially_reliable {
            return wrap_legacy_input(self.protocol_version, input.timestamp_us, &payload);
        }

        let sequence = self.next_gamepad_sequence(input.controller_id);
        wrap_partially_reliable_input(
            self.protocol_version,
            input.timestamp_us,
            input.controller_id,
            sequence,
            &payload,
        )
    }

    fn encode_keyboard(&self, input_type: u32, payload: KeyboardPayload) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(18);
        put_u32_le(&mut bytes, input_type);
        put_u16_be(&mut bytes, payload.keycode);
        put_u16_be(&mut bytes, payload.modifiers);
        put_u16_be(&mut bytes, payload.scancode);
        put_u64_be(&mut bytes, payload.timestamp_us);
        wrap_single_input(self.protocol_version, payload.timestamp_us, &bytes)
    }

    fn encode_mouse_button(&self, input_type: u32, payload: MouseButtonPayload) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(18);
        put_u32_le(&mut bytes, input_type);
        bytes.push(payload.button);
        bytes.push(0);
        put_u32_be(&mut bytes, 0);
        put_u64_be(&mut bytes, payload.timestamp_us);
        wrap_single_input(self.protocol_version, payload.timestamp_us, &bytes)
    }

    fn next_gamepad_sequence(&mut self, controller_id: u8) -> u16 {
        let current = *self.gamepad_sequences.get(&controller_id).unwrap_or(&1);
        self.gamepad_sequences
            .insert(controller_id, current.wrapping_add(1));
        current
    }
}

pub fn partially_reliable_hid_mask_for_input_type(input_type: u32) -> u32 {
    if input_type > 31 {
        return 0;
    }
    1_u32 << input_type
}

pub fn is_partially_reliable_hid_transfer_eligible(input_type: u32) -> bool {
    input_type == INPUT_MOUSE_REL
}

fn encode_gamepad_payload(bitmap: u16, input: GamepadInput) -> Vec<u8> {
    let mut payload = Vec::with_capacity(GAMEPAD_PACKET_SIZE);
    put_u32_le(&mut payload, INPUT_GAMEPAD);
    put_u16_le(&mut payload, GAMEPAD_PAYLOAD_SIZE);
    put_u16_le(&mut payload, input.controller_id as u16);
    put_u16_le(&mut payload, bitmap);
    put_u16_le(&mut payload, GAMEPAD_INNER_SIZE);
    put_u16_le(&mut payload, input.buttons);
    put_u16_le(
        &mut payload,
        input.left_trigger as u16 | ((input.right_trigger as u16) << 8),
    );
    put_i16_le(&mut payload, input.left_stick_x);
    put_i16_le(&mut payload, input.left_stick_y);
    put_i16_le(&mut payload, input.right_stick_x);
    put_i16_le(&mut payload, input.right_stick_y);
    put_u16_le(&mut payload, 0);
    put_u16_le(&mut payload, GAMEPAD_RESERVED_MARKER);
    put_u16_le(&mut payload, 0);
    put_u64_le(&mut payload, input.timestamp_us);
    payload
}

fn wrap_single_input(protocol_version: u8, timestamp_us: u64, payload: &[u8]) -> Vec<u8> {
    if protocol_version < 3 {
        return payload.to_vec();
    }

    let mut bytes = Vec::with_capacity(10 + payload.len());
    bytes.push(WRAPPER_VERSION_MARKER);
    put_u64_be(&mut bytes, timestamp_us);
    bytes.push(WRAPPER_SINGLE_INPUT);
    bytes.extend_from_slice(payload);
    bytes
}

fn wrap_legacy_input(protocol_version: u8, timestamp_us: u64, payload: &[u8]) -> Vec<u8> {
    if protocol_version < 3 {
        return payload.to_vec();
    }

    let mut bytes = Vec::with_capacity(12 + payload.len());
    bytes.push(WRAPPER_VERSION_MARKER);
    put_u64_be(&mut bytes, timestamp_us);
    bytes.push(WRAPPER_LEGACY_INPUT);
    put_u16_be(&mut bytes, payload.len() as u16);
    bytes.extend_from_slice(payload);
    bytes
}

fn wrap_partially_reliable_input(
    protocol_version: u8,
    timestamp_us: u64,
    controller_id: u8,
    sequence: u16,
    payload: &[u8],
) -> Vec<u8> {
    if protocol_version < 3 {
        return payload.to_vec();
    }

    let mut bytes = Vec::with_capacity(15 + payload.len());
    bytes.push(WRAPPER_VERSION_MARKER);
    put_u64_be(&mut bytes, timestamp_us);
    bytes.push(WRAPPER_PARTIALLY_RELIABLE_INPUT);
    bytes.push(controller_id);
    put_u16_be(&mut bytes, sequence);
    bytes.push(WRAPPER_LEGACY_INPUT);
    put_u16_be(&mut bytes, payload.len() as u16);
    bytes.extend_from_slice(payload);
    bytes
}

fn put_u16_be(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_u16_le(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_i16_be(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_i16_le(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u32_be(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_u32_le(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u64_be(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_u64_le(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_heartbeat_as_raw_little_endian_type() {
        let encoder = InputEncoder::default();
        assert_eq!(encoder.encode_heartbeat(), vec![2, 0, 0, 0]);
    }

    #[test]
    fn encodes_protocol_v2_keyboard_without_wrapper() {
        let encoder = InputEncoder::new(2);
        let payload = encoder.encode_key_down(KeyboardPayload {
            keycode: 0x0041,
            scancode: 0x001e,
            modifiers: 0x0002,
            timestamp_us: 0x0102_0304_0506_0708,
        });

        assert_eq!(payload.len(), 18);
        assert_eq!(
            payload,
            vec![
                0x03, 0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x02, 0x00, 0x1e, 0x01, 0x02, 0x03, 0x04,
                0x05, 0x06, 0x07, 0x08,
            ],
        );
    }

    #[test]
    fn wraps_protocol_v3_keyboard_as_single_input() {
        let encoder = InputEncoder::new(3);
        let payload = encoder.encode_key_up(KeyboardPayload {
            keycode: 0x0041,
            scancode: 0x001e,
            modifiers: 0,
            timestamp_us: 7,
        });

        assert_eq!(payload.len(), 28);
        assert_eq!(payload[0], WRAPPER_VERSION_MARKER);
        assert_eq!(&payload[1..9], &[0, 0, 0, 0, 0, 0, 0, 7]);
        assert_eq!(payload[9], WRAPPER_SINGLE_INPUT);
        assert_eq!(&payload[10..14], &[0x04, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn wraps_protocol_v3_mouse_move_with_payload_size() {
        let encoder = InputEncoder::new(3);
        let payload = encoder.encode_mouse_move(MouseMovePayload {
            dx: -2,
            dy: 300,
            timestamp_us: 9,
        });

        assert_eq!(payload.len(), 34);
        assert_eq!(payload[0], WRAPPER_VERSION_MARKER);
        assert_eq!(payload[9], WRAPPER_LEGACY_INPUT);
        assert_eq!(&payload[10..12], &[0, 22]);
        assert_eq!(&payload[12..16], &[0x07, 0x00, 0x00, 0x00]);
        assert_eq!(&payload[16..18], &(-2_i16).to_be_bytes());
        assert_eq!(&payload[18..20], &300_i16.to_be_bytes());
    }

    #[test]
    fn encodes_mouse_button_and_wheel_payloads() {
        let encoder = InputEncoder::new(2);
        let button = encoder.encode_mouse_button_down(MouseButtonPayload {
            button: 1,
            timestamp_us: 5,
        });
        assert_eq!(button.len(), 18);
        assert_eq!(&button[0..4], &[0x08, 0, 0, 0]);
        assert_eq!(button[4], 1);
        assert_eq!(&button[10..18], &[0, 0, 0, 0, 0, 0, 0, 5]);

        let wheel = encoder.encode_mouse_wheel(MouseWheelPayload {
            delta: -120,
            timestamp_us: 6,
        });
        assert_eq!(wheel.len(), 22);
        assert_eq!(&wheel[0..4], &[0x0a, 0, 0, 0]);
        assert_eq!(&wheel[6..8], &(-120_i16).to_be_bytes());
        assert_eq!(&wheel[14..22], &[0, 0, 0, 0, 0, 0, 0, 6]);
    }

    #[test]
    fn encodes_gamepad_payload_and_wrappers() {
        let mut encoder = InputEncoder::new(3);
        let input = GamepadInput {
            controller_id: 2,
            buttons: 0x1001,
            left_trigger: 12,
            right_trigger: 34,
            left_stick_x: -100,
            left_stick_y: 100,
            right_stick_x: -200,
            right_stick_y: 200,
            connected: true,
            timestamp_us: 11,
        };

        let reliable = encoder.encode_gamepad_state(0x00ff, input, false);
        assert_eq!(reliable.len(), 50);
        assert_eq!(reliable[9], WRAPPER_LEGACY_INPUT);
        assert_eq!(&reliable[10..12], &[0, GAMEPAD_PACKET_SIZE as u8]);
        let raw = &reliable[12..];
        assert_eq!(&raw[0..4], &[0x0c, 0, 0, 0]);
        assert_eq!(&raw[4..6], &GAMEPAD_PAYLOAD_SIZE.to_le_bytes());
        assert_eq!(&raw[6..8], &2_u16.to_le_bytes());
        assert_eq!(&raw[8..10], &0x00ff_u16.to_le_bytes());
        assert_eq!(&raw[12..14], &0x1001_u16.to_le_bytes());
        assert_eq!(&raw[14..16], &(12_u16 | (34_u16 << 8)).to_le_bytes());
        assert_eq!(&raw[16..18], &(-100_i16).to_le_bytes());
        assert_eq!(&raw[24..26], &0_u16.to_le_bytes());
        assert_eq!(&raw[26..28], &GAMEPAD_RESERVED_MARKER.to_le_bytes());
        assert_eq!(&raw[30..38], &11_u64.to_le_bytes());

        let partially_reliable = encoder.encode_gamepad_state(0x00ff, input, true);
        assert_eq!(partially_reliable.len(), 54);
        assert_eq!(partially_reliable[9], WRAPPER_PARTIALLY_RELIABLE_INPUT);
        assert_eq!(partially_reliable[10], 2);
        assert_eq!(&partially_reliable[11..13], &1_u16.to_be_bytes());
        assert_eq!(partially_reliable[13], WRAPPER_LEGACY_INPUT);
        assert_eq!(&partially_reliable[14..16], &[0, GAMEPAD_PACKET_SIZE as u8]);

        let next = encoder.encode_gamepad_state(0x00ff, input, true);
        assert_eq!(&next[11..13], &2_u16.to_be_bytes());

        encoder.reset_gamepad_sequences();
        let reset = encoder.encode_gamepad_state(0x00ff, input, true);
        assert_eq!(&reset[11..13], &1_u16.to_be_bytes());
    }

    #[test]
    fn computes_partially_reliable_hid_masks() {
        assert_eq!(
            partially_reliable_hid_mask_for_input_type(INPUT_MOUSE_REL),
            1 << 7
        );
        assert_eq!(partially_reliable_hid_mask_for_input_type(32), 0);
        assert!(is_partially_reliable_hid_transfer_eligible(INPUT_MOUSE_REL));
        assert!(!is_partially_reliable_hid_transfer_eligible(INPUT_KEY_DOWN));
    }
}
