use std::time::Duration;

pub(crate) const FRAME_ACK_CODE: u16 = 0x204;
pub(crate) const QOS_REPORT_CODE: u16 = 0x207;
pub(crate) const NACK_V2_CODE: u16 = 0x317;
pub(crate) const MAX_NACK_PACKET_COUNT: usize = 64;
pub(crate) const IDR_REQUEST_CODE: u16 = 0x302;

pub(crate) const FRAME_ACK_PAYLOAD_LEN: usize = 102;
pub(crate) const QOS_REPORT_PAYLOAD_LEN: usize = 52;
pub(crate) const QOS_REPORT_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const MAX_CONTROL_BATCH_BYTES: usize = 1071;

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

/// Keep record boundaries intact; a server parses consecutive code/length records.
pub(crate) fn batch_control_records(
    records: impl IntoIterator<Item = NvstControlCommand>,
) -> Vec<Vec<u8>> {
    let mut batches = Vec::new();
    let mut batch = Vec::with_capacity(MAX_CONTROL_BATCH_BYTES);
    for record in records {
        let encoded = record.encoded();
        if encoded.is_empty() || encoded.len() > MAX_CONTROL_BATCH_BYTES {
            continue;
        }
        if batch.len() + encoded.len() > MAX_CONTROL_BATCH_BYTES {
            batches.push(std::mem::replace(
                &mut batch,
                Vec::with_capacity(MAX_CONTROL_BATCH_BYTES),
            ));
        }
        batch.extend_from_slice(&encoded);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

pub(crate) fn nack_v2(stream_index: u8, missing: &[u16]) -> Option<NvstControlCommand> {
    if missing.is_empty() || missing.len() > MAX_NACK_PACKET_COUNT {
        return None;
    }
    let mut payload = Vec::with_capacity(3 + missing.len() * 10);
    payload.extend_from_slice(&[2, stream_index, 0]);
    let mut base = missing[0];
    let mut bitmap = 0_u64;
    for &sequence in &missing[1..] {
        let distance = sequence.wrapping_sub(base);
        if distance == 0 {
            continue;
        }
        if distance <= 64 {
            bitmap |= 1_u64 << (distance - 1);
        } else {
            payload.extend_from_slice(&base.to_le_bytes());
            payload.extend_from_slice(&bitmap.to_le_bytes());
            payload[2] += 1;
            base = sequence;
            bitmap = 0;
        }
    }
    payload.extend_from_slice(&base.to_le_bytes());
    payload.extend_from_slice(&bitmap.to_le_bytes());
    payload[2] += 1;
    Some(NvstControlCommand {
        code: NACK_V2_CODE,
        payload,
    })
}

pub(crate) fn frame_ack(
    frame_number: u32,
    first_packet_time_ms: Option<f64>,
    frame_bytes: u32,
) -> NvstControlCommand {
    let mut payload = vec![0; FRAME_ACK_PAYLOAD_LEN];
    put_u16(&mut payload, 0, 1);
    put_u16(&mut payload, 2, 9);
    put_u32(&mut payload, 4, frame_number);
    if let Some(first_packet_time_ms) = first_packet_time_ms {
        put_u64(&mut payload, 12, first_packet_time_ms.to_bits());
    }
    for offset in (20..=48).step_by(4) {
        put_u32(&mut payload, offset, (-1.0_f32).to_bits());
    }
    put_u32(&mut payload, 72, frame_bytes);
    put_u32(&mut payload, 94, (-1.0_f32).to_bits());
    put_u32(&mut payload, 98, (-1.0_f32).to_bits());
    NvstControlCommand {
        code: FRAME_ACK_CODE,
        payload,
    }
}

pub(crate) struct QosReport {
    pub(crate) sequence: u32,
    pub(crate) last_frame_number: u32,
    pub(crate) client_time_90khz: u32,
    pub(crate) loss_per_10000: u16,
}

impl QosReport {
    pub(crate) fn command(&self) -> NvstControlCommand {
        let mut payload = vec![0; QOS_REPORT_PAYLOAD_LEN];
        put_u32(&mut payload, 0, 7);
        put_u32(&mut payload, 8, self.sequence);
        put_u32(&mut payload, 12, self.last_frame_number);
        put_u16(&mut payload, 26, self.loss_per_10000.min(10_000));
        put_u16(&mut payload, 30, 1_000);
        put_u16(&mut payload, 32, 1_000);
        put_u32(&mut payload, 36, self.client_time_90khz);
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

    #[test]
    fn batching_preserves_records_and_bounds_each_sctp_message() {
        let records: Vec<_> = (0..25).map(|frame| frame_ack(frame, None, 1)).collect();
        let expected: Vec<_> = records
            .iter()
            .flat_map(NvstControlCommand::encoded)
            .collect();
        let batches = batch_control_records(records);
        assert_eq!(
            batches.iter().map(Vec::len).collect::<Vec<_>>(),
            [1060, 1060, 530]
        );
        assert_eq!(batches.concat(), expected);
        assert!(batch_control_records([]).is_empty());
    }

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
    fn frame_ack_places_only_source_pinned_fields() {
        let command = frame_ack(42, Some(20_320.16), 15_168);
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
        for offset in (20..=48).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(command.payload[offset..offset + 4].try_into().unwrap()),
                (-1.0_f32).to_bits()
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
            0
        );
        assert_eq!(
            u32::from_le_bytes(command.payload[94..98].try_into().unwrap()),
            (-1.0_f32).to_bits()
        );
        assert_eq!(&command.payload[98..102], &(-1.0_f32).to_le_bytes());
        assert_eq!(
            command.payload,
            hex(
                "010009002a00000000000000d7a3703d0ad8d340000080bf000080bf000080bf000080bf000080bf000080bf000080bf000080bf0000000000000000000000000000000000000000403b0000000000000000000000000000000000000000000080bf000080bf"
            )
        );
    }

    #[test]
    fn frame_ack_leaves_unavailable_stage_metrics_zero() {
        let command = frame_ack(1, None, 7);
        assert_eq!(&command.payload[12..20], &[0; 8]);
        assert_eq!(&command.payload[56..60], &[0; 4]);
        for offset in (20..=48).step_by(4) {
            assert_eq!(
                &command.payload[offset..offset + 4],
                &(-1.0_f32).to_le_bytes()
            );
        }
    }

    #[test]
    fn qos_report_matches_the_source_test_layout() {
        let command = QosReport {
            sequence: 6,
            last_frame_number: 42,
            client_time_90khz: 90_000,
            loss_per_10000: 2500,
        }
        .command();
        assert_eq!(command.code, QOS_REPORT_CODE);
        assert_eq!(
            command.payload,
            hex(
                "0700000000000000060000002a00000000000000000000000000c4090000e803e8030000905f0100000000000000000000000000"
            )
        );
    }

    #[test]
    fn idr_request_matches_the_source_layout() {
        assert_eq!(idr_request().encoded(), [0x02, 0x03, 0x02, 0x00, 0, 0]);
    }
}

#[cfg(test)]
mod alliance_tests {
    use super::*;
    #[test]
    fn private_nack_v2_keeps_implicit_base_wrap_and_bounds() {
        let packet = nack_v2(0, &[0x1234]).unwrap().encoded();
        assert_eq!(
            packet,
            [0x17, 3, 13, 0, 2, 0, 1, 0x34, 0x12, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let packet = nack_v2(0, &[65534, 65535, 0, 62, 63]).unwrap().payload;
        assert_eq!(packet[2], 2);
        assert_eq!(&packet[3..5], &65534u16.to_le_bytes());
        assert_eq!(&packet[5..13], &(3u64 | (1 << 63)).to_le_bytes());
        assert_eq!(&packet[13..15], &63u16.to_le_bytes());
        assert!(nack_v2(0, &[]).is_none());
        assert!(nack_v2(0, &[0; 65]).is_none());
    }
}
