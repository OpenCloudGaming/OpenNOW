use std::time::Duration;

pub(crate) const FRAME_ACK_CODE: u16 = 0x204;
pub(crate) const FRAME_PACING_CODE: u16 = 0x203;
pub(crate) const QOS_REPORT_CODE: u16 = 0x207;
pub(crate) const IDR_REQUEST_CODE: u16 = 0x302;

pub(crate) const FRAME_ACK_PAYLOAD_LEN: usize = 102;
pub(crate) const FRAME_PACING_PAYLOAD_LEN: usize = 28;
pub(crate) const QOS_REPORT_PAYLOAD_LEN: usize = 52;
// The official GFN client keeps the frame-pacing PID target at 16.666 ms even for a
// 120 FPS encoded stream. This is a renderer/feedback target, not the encoded-frame interval.
pub(crate) const DEFAULT_FRAME_TIME_US: u32 = 16_666;
pub(crate) const QOS_REPORT_INTERVAL: Duration = Duration::from_micros(55_556);
pub(crate) const QOS_WARM_UP: Duration = Duration::from_millis(1_900);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NvstControlCommand {
    pub(crate) code: u16,
    pub(crate) payload: Vec<u8>,
}

impl NvstControlCommand {
    pub(crate) fn encoded(&self) -> Vec<u8> {
        let Ok(payload_len) = u16::try_from(self.payload.len()) else {
            return Vec::new();
        };
        let mut encoded = Vec::with_capacity(4 + self.payload.len());
        encoded.extend_from_slice(&self.code.to_le_bytes());
        encoded.extend_from_slice(&payload_len.to_le_bytes());
        encoded.extend_from_slice(&self.payload);
        encoded
    }
}

pub(crate) fn frame_ack(
    frame_number: u32,
    client_time_ms: f64,
    frame_bytes: u32,
    frame_time_us: u32,
    measured_stage_ms: Option<f32>,
) -> NvstControlCommand {
    let mut payload = vec![0; FRAME_ACK_PAYLOAD_LEN];
    put_u16(&mut payload, 0, 1);
    put_u16(&mut payload, 2, 9);
    put_u32(&mut payload, 4, frame_number);
    put_u64(&mut payload, 12, client_time_ms.to_bits());
    if let Some(stage_ms) = measured_stage_ms.filter(|value| value.is_finite() && *value >= 0.0) {
        for offset in (28..=44).step_by(4) {
            put_u32(&mut payload, offset, stage_ms.to_bits());
        }
    }
    put_u32(&mut payload, 48, (-1.0_f32).to_bits());
    put_u32(&mut payload, 72, frame_bytes);
    put_u32(&mut payload, 84, 16_384);
    put_u32(&mut payload, 96, frame_time_us);
    NvstControlCommand {
        code: FRAME_ACK_CODE,
        payload,
    }
}

pub(crate) fn frame_pacing_report(
    frame_number: u32,
    target_frame_time_us: u32,
    pacing_error_us: u32,
) -> NvstControlCommand {
    let mut payload = vec![0; FRAME_PACING_PAYLOAD_LEN];
    put_u32(&mut payload, 0, 5);
    put_u32(&mut payload, 8, 2);
    put_u32(&mut payload, 12, frame_number);
    put_u32(&mut payload, 16, target_frame_time_us);
    put_u32(&mut payload, 20, pacing_error_us.min(target_frame_time_us));
    put_u32(&mut payload, 24, 0x341a);
    NvstControlCommand {
        code: FRAME_PACING_CODE,
        payload,
    }
}

pub(crate) struct QosReport {
    pub(crate) sequence: u32,
    pub(crate) frames_received: u32,
    pub(crate) bytes_received: u32,
    pub(crate) rtp_timestamp: u32,
    pub(crate) previous_bytes_received: u32,
    pub(crate) warmed_up: bool,
}

impl QosReport {
    pub(crate) fn command(&self) -> NvstControlCommand {
        let mut payload = vec![0; QOS_REPORT_PAYLOAD_LEN];
        put_u32(&mut payload, 0, 7);
        put_u32(&mut payload, 8, self.sequence);
        put_u32(&mut payload, 12, self.frames_received);
        put_u32(&mut payload, 16, self.bytes_received);
        put_u16(&mut payload, 28, if self.warmed_up { 2 } else { 0 });
        put_u16(&mut payload, 30, 1_000);
        put_u16(&mut payload, 32, 1_000);
        put_u16(&mut payload, 34, 12_708);
        put_u32(&mut payload, 36, self.rtp_timestamp);
        put_u32(&mut payload, 48, self.previous_bytes_received);
        NvstControlCommand {
            code: QOS_REPORT_CODE,
            payload,
        }
    }
}

pub(crate) fn idr_request() -> NvstControlCommand {
    NvstControlCommand {
        code: IDR_REQUEST_CODE,
        payload: 0_u16.to_le_bytes().to_vec(),
    }
}

fn put_u16(payload: &mut [u8], offset: usize, value: u16) {
    payload[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(payload: &mut [u8], offset: usize, value: u32) {
    payload[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(payload: &mut [u8], offset: usize, value: u64) {
    payload[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16)
                    .expect("valid hex")
            })
            .collect()
    }

    #[test]
    fn command_header_is_little_endian_and_byte_exact() {
        let command = NvstControlCommand {
            code: 0x207,
            payload: vec![1, 2, 3],
        };
        assert_eq!(command.encoded(), [0x07, 0x02, 0x03, 0x00, 1, 2, 3]);
    }

    #[test]
    fn frame_pacing_report_matches_the_source_test_vector() {
        let command = frame_pacing_report(1, 16_000, 16_000);
        assert_eq!(command.code, FRAME_PACING_CODE);
        assert_eq!(
            command.payload,
            hex("05000000000000000200000001000000803e0000803e00001a340000")
        );
        assert_eq!(command.encoded().len(), 4 + FRAME_PACING_PAYLOAD_LEN);
    }

    #[test]
    fn frame_ack_places_only_source_pinned_fields() {
        let command = frame_ack(42, 20_320.16, 15_168, 16_667, Some(4.5));
        assert_eq!(command.code, FRAME_ACK_CODE);
        assert_eq!(command.payload.len(), FRAME_ACK_PAYLOAD_LEN);
        assert_eq!(&command.payload[0..4], &[1, 0, 9, 0]);
        assert_eq!(
            u32::from_le_bytes(command.payload[4..8].try_into().unwrap()),
            42
        );
        assert_eq!(
            u64::from_le_bytes(command.payload[12..20].try_into().unwrap()),
            20_320.16_f64.to_bits()
        );
        for offset in (28..=44).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(command.payload[offset..offset + 4].try_into().unwrap()),
                4.5_f32.to_bits()
            );
        }
        assert_eq!(
            u32::from_le_bytes(command.payload[48..52].try_into().unwrap()),
            (-1.0_f32).to_bits()
        );
        assert_eq!(
            u32::from_le_bytes(command.payload[72..76].try_into().unwrap()),
            15_168
        );
        assert_eq!(
            u32::from_le_bytes(command.payload[84..88].try_into().unwrap()),
            16_384
        );
        assert_eq!(
            u32::from_le_bytes(command.payload[96..100].try_into().unwrap()),
            16_667
        );
        assert_eq!(&command.payload[100..102], &[0, 0]);
        assert_eq!(
            command.payload,
            hex(
                "010009002a00000000000000d7a3703d0ad8d34000000000000000000000904000009040000090400000904000009040000080bf0000000000000000000000000000000000000000403b000000000000000000000040000000000000000000001b4100000000"
            )
        );
    }

    #[test]
    fn frame_ack_leaves_unavailable_stage_metrics_zero() {
        let command = frame_ack(1, 0.0, 7, DEFAULT_FRAME_TIME_US, None);
        assert!(command.payload[28..48].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn qos_report_matches_the_source_test_layout() {
        let command = QosReport {
            sequence: 6,
            frames_received: 2,
            bytes_received: 244_808,
            rtp_timestamp: 1_818_674,
            previous_bytes_received: 244_808,
            warmed_up: false,
        }
        .command();
        assert_eq!(command.code, QOS_REPORT_CODE);
        assert_eq!(
            command.payload,
            hex(
                "0700000000000000060000000200000048bc030000000000000000000000e803e803a43132c01b00000000000000000048bc0300"
            )
        );
    }

    #[test]
    fn idr_request_matches_the_source_layout() {
        assert_eq!(idr_request().encoded(), [0x02, 0x03, 0x02, 0x00, 0, 0]);
    }
}
