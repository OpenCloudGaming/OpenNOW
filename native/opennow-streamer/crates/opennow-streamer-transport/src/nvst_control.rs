use std::time::Duration;

pub(crate) const FRAME_ACK_CODE: u16 = 0x204;
pub(crate) const FRAME_PACING_CODE: u16 = 0x203;
pub(crate) const QOS_REPORT_CODE: u16 = 0x207;
pub(crate) const IDR_REQUEST_CODE: u16 = 0x302;
pub(crate) const NACK_V2_CODE: u16 = 0x317;
pub(crate) const MAX_NACK_PACKET_COUNT: usize = 64;

pub(crate) const FRAME_ACK_PAYLOAD_LEN: usize = 102;
pub(crate) const FRAME_PACING_PAYLOAD_LEN: usize = 28;
pub(crate) const QOS_REPORT_PAYLOAD_LEN: usize = 52;
// The official GFN client keeps the frame-pacing PID target at 16.666 ms even for a
// 120 FPS encoded stream. This is a renderer/feedback target, not the encoded-frame interval.
pub(crate) const DEFAULT_FRAME_TIME_US: u32 = 16_666;
pub(crate) const FRAME_PACING_INTERVAL: Duration = Duration::from_micros(55_556);
pub(crate) const QOS_REPORT_INTERVAL: Duration = Duration::from_millis(50);

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

#[derive(Debug, Default)]
pub(crate) struct QosReport {
    pub(crate) sequence: u32,
    pub(crate) sender_frame_number: u32,
    pub(crate) bytes_received: u32,
    pub(crate) loss_per_ten_thousand: u16,
    pub(crate) client_time_90khz: u32,
    pub(crate) packet_snapshot: Option<QosPacketSnapshot>,
    pub(crate) bandwidth: crate::nvst_bandwidth::BandwidthFeedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QosPacketSnapshot {
    pub(crate) ssrc: u32,
    pub(crate) base: u32,
    pub(crate) highest: u32,
    pub(crate) received: u32,
}

impl QosReport {
    pub(crate) fn command(&self) -> NvstControlCommand {
        let mut payload = vec![0; QOS_REPORT_PAYLOAD_LEN];
        put_u32(&mut payload, 0, 7);
        put_u32(&mut payload, 8, self.sequence);
        put_u32(&mut payload, 12, self.sender_frame_number);
        put_u32(&mut payload, 16, self.bandwidth.minimum_server_time);
        put_u32(&mut payload, 20, self.bandwidth.queue_delay_us);
        put_u16(&mut payload, 24, self.bandwidth.jitter_us);
        put_u16(&mut payload, 26, self.loss_per_ten_thousand);
        payload[28] = self.bandwidth.utilization_percent;
        put_u16(&mut payload, 30, 1_000);
        put_u16(&mut payload, 32, 1_000);
        put_u32(&mut payload, 36, self.client_time_90khz);
        put_u32(&mut payload, 40, self.bandwidth.lossy_frames);
        put_u32(&mut payload, 44, self.bandwidth.estimate_kbps);
        put_u32(&mut payload, 48, self.bandwidth.median_server_time);
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
    fn nack_v2_encodes_the_implicit_base_without_a_bitmap_bit() {
        let command = nack_v2(0, &[0x1234]).unwrap();
        assert_eq!(command.code, NACK_V2_CODE);
        assert_eq!(command.encoded(), hex("17030d0002000134120000000000000000"));
    }

    #[test]
    fn nack_v2_uses_all_64_bitmap_bits_before_starting_another_record() {
        let command = nack_v2(7, &[0x1234, 0x1235, 0x1274, 0x1275]).unwrap();
        assert_eq!(
            command.encoded(),
            hex("170317000207023412010000000000008075120000000000000000")
        );
    }

    #[test]
    fn nack_v2_groups_across_sequence_wrap_without_requesting_the_holes() {
        let command = nack_v2(0, &[65534, 65535, 0, 62, 63]).unwrap();
        assert_eq!(
            command.encoded(),
            hex("17031700020002feff03000000000000803f000000000000000000")
        );
    }

    #[test]
    fn nack_v2_bounds_missing_packets_not_sequence_span_or_record_count() {
        assert!(nack_v2(0, &[]).is_none());
        assert!(nack_v2(0, &[0; MAX_NACK_PACKET_COUNT + 1]).is_none());
        let contiguous: Vec<u16> = (0..64).collect();
        assert_eq!(
            nack_v2(0, &contiguous).unwrap().encoded(),
            hex("17030d000200010000ffffffffffffff7f")
        );
        let sparse: Vec<u16> = (0..64).map(|index| index * 65).collect();
        let command = nack_v2(0, &sparse).unwrap();
        assert_eq!(command.payload.len(), 643);
        assert_eq!(&command.encoded()[..7], &[0x17, 3, 0x83, 2, 2, 0, 64]);
        for (record, sequence) in command.payload[3..].chunks_exact(10).zip(sparse) {
            assert_eq!(&record[..2], &sequence.to_le_bytes());
            assert_eq!(&record[2..], &[0; 8]);
        }
    }

    #[test]
    fn nack_v2_round_trip_preserves_only_requested_sequences() {
        use std::collections::BTreeSet;

        for base in [0_u16, 1, 32767, 65534, 65535] {
            for step in [0_u16, 1, 2, 64, 65, 127, 4095, 65535] {
                for count in 1..=MAX_NACK_PACKET_COUNT {
                    let missing: Vec<u16> = (0..count)
                        .map(|index| base.wrapping_add((index as u16).wrapping_mul(step)))
                        .collect();
                    let command = nack_v2(3, &missing).unwrap();
                    assert_eq!(
                        command.payload.len(),
                        3 + usize::from(command.payload[2]) * 10
                    );
                    let mut decoded = BTreeSet::new();
                    for record in command.payload[3..].chunks_exact(10) {
                        let base = u16::from_le_bytes(record[..2].try_into().unwrap());
                        let bitmap = u64::from_le_bytes(record[2..].try_into().unwrap());
                        decoded.insert(base);
                        for bit in 0..64 {
                            if bitmap & (1_u64 << bit) != 0 {
                                decoded.insert(base.wrapping_add(bit + 1));
                            }
                        }
                    }
                    assert_eq!(decoded, missing.into_iter().collect());
                }
            }
        }
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
                &command.payload[offset..offset + 4],
                &(-1.0_f32).to_le_bytes()
            );
        }
        assert_eq!(&command.payload[56..60], &[0; 4]);
        assert_eq!(
            u32::from_le_bytes(command.payload[72..76].try_into().unwrap()),
            15_168
        );
        assert_eq!(&command.payload[84..88], &[0; 4]);
        assert_eq!(&command.payload[94..98], &(-1.0_f32).to_le_bytes());
        assert_eq!(&command.payload[98..102], &(-1.0_f32).to_le_bytes());
    }

    #[test]
    fn frame_ack_never_writes_a_measured_value_into_unpinned_stage_slots() {
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
            sender_frame_number: 2,
            bytes_received: 244_808,
            client_time_90khz: 1_818_674,
            ..QosReport::default()
        }
        .command();
        assert_eq!(command.code, QOS_REPORT_CODE);
        assert_eq!(
            command.payload,
            hex(
                "070000000000000006000000020000000000000000000000000000000000e803e803000032c01b00000000000000000000000000"
            )
        );
    }

    #[test]
    fn qos_report_writes_sender_frame_loss_and_client_clock_at_their_wire_offsets() {
        let report = QosReport {
            sender_frame_number: 0x1234_5678,
            loss_per_ten_thousand: 2_500,
            client_time_90khz: 0x8765_4321,
            ..QosReport::default()
        };
        let payload = report.command().payload;
        assert_eq!(&payload[12..16], &0x1234_5678_u32.to_le_bytes());
        assert_eq!(&payload[26..28], &2_500_u16.to_le_bytes());
        assert_eq!(&payload[34..36], &[0, 0]);
        assert_eq!(&payload[36..40], &0x8765_4321_u32.to_le_bytes());
        for range in [16..26, 28..30, 40..52] {
            assert!(payload[range].iter().all(|byte| *byte == 0));
        }
        assert_eq!(QOS_REPORT_INTERVAL, Duration::from_millis(50));
        assert_eq!(FRAME_PACING_INTERVAL, Duration::from_micros(55_556));
    }

    #[test]
    fn idr_request_matches_the_source_layout() {
        assert_eq!(idr_request().encoded(), [0x02, 0x03, 0x02, 0x00, 0, 0]);
    }
}
