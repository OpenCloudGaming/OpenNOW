use opennow_streamer_protocol::text_input::UnicodeText;
use std::collections::VecDeque;

use crate::nvst_budget::TEXT_BATCH_GUARD;
use crate::nvst_input::{
    NvstEncodedInput, NvstInputCodec, NvstInputCodecError, NvstInputHeldState, NvstInputWrite,
    native_input_type_is_motion, native_input_types, text_batch_fits, unicode_text_messages,
};

const PENDING_INPUT_MESSAGES: usize = 256;
const PENDING_INPUT_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) enum NvstInputDeliveryError {
    Encoding(NvstInputCodecError),
    Exhausted,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pressed: bool) -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes[..4].copy_from_slice(&(if pressed { 3_u32 } else { 4_u32 }).to_le_bytes());
        bytes
    }

    #[test]
    fn shutdown_neutralizes_sent_gamepad_state_over_reliable_control() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let mut gamepad = vec![0; 38];
        gamepad[..4].copy_from_slice(&12_u32.to_le_bytes());
        gamepad[8..10].copy_from_slice(&1_u16.to_le_bytes());
        gamepad[12..14].copy_from_slice(&1_u16.to_le_bytes());
        delivery
            .submit(&mut codec, &gamepad, 0, |_| NvstInputWrite::Sent)
            .unwrap();
        let neutral = delivery.take_neutral_input(42);
        assert_eq!(neutral.len(), 2);
        assert!(
            neutral
                .iter()
                .all(|message| message.route == crate::nvst_input::NvstInputRoute::ControlReliable)
        );
        assert_eq!(neutral[0].bytes[13], 0x22);
        assert_eq!(&neutral[0].bytes[26..38], &[0; 12]);
        assert_eq!(&neutral[1].bytes[22..24], &[0; 2]);
        assert!(delivery.take_neutral_input(42).is_empty());
    }

    #[test]
    fn neutralization_uses_successful_writes_not_deferred_releases_or_presses() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let mut press = key(true);
        press[4..6].copy_from_slice(&65_u16.to_be_bytes());
        let mut release = press.clone();
        release[..4].copy_from_slice(&4_u32.to_le_bytes());
        delivery
            .submit(&mut codec, &press, 0, |_| NvstInputWrite::Sent)
            .unwrap();
        delivery
            .submit(&mut codec, &release, 0, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(
            delivery.take_neutral_input(0),
            codec.encode(&release, 0).unwrap()
        );
        assert!(delivery.is_empty());
        assert!(delivery.take_neutral_input(0).is_empty());
        delivery
            .submit(&mut codec, &press, 0, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert!(delivery.take_neutral_input(0).is_empty());
        delivery
            .submit(&mut codec, &press, 0, |_| NvstInputWrite::Sent)
            .unwrap();
        delivery
            .submit(&mut codec, &release, 0, |_| NvstInputWrite::Sent)
            .unwrap();
        assert!(delivery.take_neutral_input(0).is_empty());
    }

    #[test]
    fn releases_wait_behind_deferred_presses_and_successful_prefixes_are_not_replayed() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let press = key(true);
        let release = key(false);
        let mut expected = codec.clone().encode(&press, 1).unwrap();
        expected.extend(codec.clone().encode(&release, 2).unwrap());
        delivery
            .submit(&mut codec, &press, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit(&mut codec, &release, 2, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(delivery.pending.len(), 2);
        let mut delivered = Vec::new();
        delivery
            .flush(|message| {
                if delivered.is_empty() {
                    delivered.push(message.clone());
                    NvstInputWrite::Sent
                } else {
                    NvstInputWrite::Deferred
                }
            })
            .unwrap();
        assert_eq!(delivery.pending.len(), 1);
        delivery
            .flush(|message| {
                delivered.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        delivery
            .flush(|_| panic!("delivered input must not be retried"))
            .unwrap();
        assert_eq!(delivered, expected);
        assert_eq!(delivery.pending_bytes, 0);
    }

    #[test]
    fn repeated_rejections_keep_streaming_until_the_bounded_queue_is_exhausted() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        for _ in 0..PENDING_INPUT_MESSAGES {
            delivery
                .submit(&mut codec, &key(false), 1, |_| NvstInputWrite::Deferred)
                .unwrap();
        }
        assert!(matches!(
            delivery.submit(&mut codec, &key(false), 1, |_| NvstInputWrite::Deferred),
            Err(NvstInputDeliveryError::Exhausted)
        ));
        assert_eq!(delivery.pending.len(), PENDING_INPUT_MESSAGES);
        assert!(delivery.pending_bytes <= PENDING_INPUT_BYTES);
    }

    #[test]
    fn encoded_batch_admission_is_atomic_and_rejected_codec_state_is_restored() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        for _ in 0..PENDING_INPUT_MESSAGES - 1 {
            delivery
                .submit(&mut codec, &key(false), 1, |_| NvstInputWrite::Deferred)
                .unwrap();
        }
        let mut gamepad = vec![0; 38];
        gamepad[..4].copy_from_slice(&12_u32.to_le_bytes());
        gamepad[8..10].copy_from_slice(&1_u16.to_le_bytes());
        let expected = codec.clone().encode(&gamepad, 1).unwrap();
        assert_eq!(expected.len(), 2);
        assert!(matches!(
            delivery.submit(&mut codec, &gamepad, 1, |_| NvstInputWrite::Deferred),
            Err(NvstInputDeliveryError::Exhausted)
        ));
        assert_eq!(delivery.pending.len(), PENDING_INPUT_MESSAGES - 1);
        assert_eq!(codec.encode(&gamepad, 1).unwrap(), expected);
    }

    #[test]
    fn byte_capacity_rejects_a_batch_before_any_write() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        delivery
            .pending
            .push_back(PendingInput::Discrete(NvstEncodedInput {
                route: crate::nvst_input::NvstInputRoute::ControlReliable,
                bytes: vec![0; PENDING_INPUT_BYTES - 1],
                meaningful: false,
            }));
        delivery.pending_messages = 1;
        delivery.pending_bytes = PENDING_INPUT_BYTES - 1;
        let mut writes = 0;
        assert!(matches!(
            delivery.submit(&mut codec, &key(false), 1, |message| {
                writes += 1;
                assert_eq!(message.bytes.len(), PENDING_INPUT_BYTES - 1);
                NvstInputWrite::Deferred
            }),
            Err(NvstInputDeliveryError::Exhausted)
        ));
        assert_eq!(writes, 1);
        assert_eq!(delivery.pending.len(), 1);
        assert_eq!(delivery.pending_bytes, PENDING_INPUT_BYTES - 1);
    }

    #[test]
    fn motion_is_dropped_instead_of_accumulating_or_overtaking_pending_discrete_input() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let mut motion = vec![0; 22];
        motion[..4].copy_from_slice(&7_u32.to_le_bytes());
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert!(delivery.pending.is_empty());
        delivery
            .submit(&mut codec, &key(true), 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        for _ in 0..1000 {
            delivery
                .submit(&mut codec, &motion, 1, |message| {
                    assert_eq!(
                        message.route,
                        crate::nvst_input::NvstInputRoute::ControlReliable
                    );
                    NvstInputWrite::Deferred
                })
                .unwrap();
        }
        assert_eq!(delivery.pending.len(), 1);
    }

    #[test]
    fn pending_release_reports_a_closed_transport_instead_of_retrying_forever() {
        let mut delivery = NvstInputDelivery::default();
        delivery
            .submit(&mut NvstInputCodec::default(), &key(false), 1, |_| {
                NvstInputWrite::Deferred
            })
            .unwrap();
        assert!(matches!(
            delivery.flush(|_| NvstInputWrite::Closed),
            Err(NvstInputDeliveryError::Closed)
        ));
        assert_eq!(delivery.pending.len(), 1);
    }

    #[test]
    fn diagnostics_track_pending_bytes_deferrals_and_motion_drops_without_resetting_history() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let packet = key(false);
        let bytes = codec.clone().encode(&packet, 1).unwrap()[0].bytes.len();
        delivery
            .submit(&mut codec, &packet, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(
            delivery.snapshot(),
            NvstInputDeliverySnapshot {
                pending_messages: 1,
                pending_bytes: bytes,
                pending_text_messages: 0,
                pending_text_bytes: 0,
                deferred_writes: 1,
                dropped_motion: 0,
            }
        );
        let mut motion = vec![0; 22];
        motion[..4].copy_from_slice(&7_u32.to_le_bytes());
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(
            delivery.snapshot(),
            NvstInputDeliverySnapshot {
                pending_messages: 1,
                pending_bytes: bytes,
                pending_text_messages: 0,
                pending_text_bytes: 0,
                deferred_writes: 2,
                dropped_motion: 1,
            }
        );
        delivery.flush(|_| NvstInputWrite::Sent).unwrap();
        assert_eq!(
            delivery.snapshot(),
            NvstInputDeliverySnapshot {
                pending_messages: 0,
                pending_bytes: 0,
                pending_text_messages: 0,
                pending_text_bytes: 0,
                deferred_writes: 2,
                dropped_motion: 1,
            }
        );
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(
            delivery.snapshot(),
            NvstInputDeliverySnapshot {
                pending_messages: 0,
                pending_bytes: 0,
                pending_text_messages: 0,
                pending_text_bytes: 0,
                deferred_writes: 3,
                dropped_motion: 2,
            }
        );
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Sent)
            .unwrap();
        assert_eq!(delivery.snapshot().deferred_writes, 3);
        assert_eq!(delivery.snapshot().dropped_motion, 2);
    }

    #[test]
    fn diagnostic_counters_saturate_under_prolonged_pressure() {
        let mut delivery = NvstInputDelivery {
            deferred_writes: u64::MAX,
            dropped_motion: u64::MAX,
            ..NvstInputDelivery::default()
        };
        let mut codec = NvstInputCodec::default();
        let mut motion = vec![0; 22];
        motion[..4].copy_from_slice(&7_u32.to_le_bytes());
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit(&mut codec, &key(false), 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit(&mut codec, &motion, 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(delivery.snapshot().deferred_writes, u64::MAX);
        assert_eq!(delivery.snapshot().dropped_motion, u64::MAX);
    }

    #[test]
    fn paste_waits_behind_deferred_press_and_before_subsequent_release() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        let text = slot.submit("café 世界".as_bytes()).unwrap();
        let mut expected = codec.clone().encode(&key(true), 1).unwrap();
        expected.extend(unicode_text_messages(&text, 2));
        expected.extend(codec.clone().encode(&key(false), 3).unwrap());
        delivery
            .submit(&mut codec, &key(true), 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit_text(text, 2, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit(&mut codec, &key(false), 3, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(delivery.snapshot().pending_messages, 2);
        assert_eq!(delivery.snapshot().pending_text_messages, 1);
        let mut sent = Vec::new();
        delivery
            .flush(|message| {
                sent.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        assert_eq!(sent, expected);
        assert!(delivery.is_empty());
        assert_eq!(delivery.snapshot().pending_bytes, 0);
        assert_eq!(delivery.snapshot().pending_text_bytes, 0);
    }

    #[test]
    fn partial_paste_keeps_single_flight_reservation_and_never_replays_sent_prefix() {
        let mut delivery = NvstInputDelivery::default();
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        let text = slot.submit("é世界🦫\r\n".repeat(400).as_bytes()).unwrap();
        let expected = unicode_text_messages(&text, 7);
        assert!(expected.len() > 2);
        let mut sent = Vec::new();
        delivery
            .submit_text(text, 7, |message| {
                if sent.is_empty() {
                    sent.push(message.clone());
                    NvstInputWrite::Sent
                } else {
                    NvstInputWrite::Deferred
                }
            })
            .unwrap();
        assert!(matches!(
            slot.submit(b"next"),
            Err(opennow_streamer_protocol::text_input::TextInputError::Busy)
        ));
        assert_eq!(
            delivery.snapshot().pending_text_messages,
            expected.len() - 1
        );
        delivery
            .flush(|message| {
                sent.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        assert_eq!(sent, expected);
        assert_eq!(delivery.snapshot().pending_text_messages, 0);
        assert!(slot.submit(b"next").is_ok());
    }

    #[test]
    fn cancellation_discards_remaining_text_without_discarding_key_releases() {
        let mut delivery = NvstInputDelivery::default();
        let mut codec = NvstInputCodec::default();
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        let text = slot.submit(b"paste").unwrap();
        let mut expected = codec.clone().encode(&key(true), 1).unwrap();
        expected.extend(codec.clone().encode(&key(false), 3).unwrap());
        delivery
            .submit(&mut codec, &key(true), 1, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit_text(text, 2, |_| NvstInputWrite::Deferred)
            .unwrap();
        delivery
            .submit(&mut codec, &key(false), 3, |_| NvstInputWrite::Deferred)
            .unwrap();
        slot.cancel();
        let mut sent = Vec::new();
        delivery
            .flush(|message| {
                sent.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        assert_eq!(sent, expected);
        assert_eq!(delivery.snapshot().pending_text_bytes, 0);
        assert_eq!(delivery.snapshot().pending_text_messages, 0);
        assert!(slot.submit(b"next").is_ok());
    }

    #[test]
    fn maximum_paste_is_bounded_and_survives_a_full_sctp_normal_budget() {
        let mut delivery = NvstInputDelivery::default();
        let slot = opennow_streamer_protocol::text_input::TextInputSlot::default();
        let text = slot
            .submit(&vec![
                b'a';
                opennow_streamer_protocol::text_input::MAX_TEXT_BYTES
            ])
            .unwrap();
        let expected = unicode_text_messages(&text, 9);
        let total: usize = expected.iter().map(|message| message.bytes.len()).sum();
        assert!(total > crate::nvst_budget::NORMAL_BUDGET);
        assert!(total <= TEXT_BATCH_GUARD);
        delivery
            .submit_text(text, 9, |_| NvstInputWrite::Deferred)
            .unwrap();
        assert_eq!(delivery.snapshot().pending_text_bytes, total);
        let mut budget = crate::nvst_budget::NORMAL_BUDGET;
        let mut sent = Vec::new();
        delivery
            .flush(|message| {
                if message.bytes.len() > budget {
                    return NvstInputWrite::Deferred;
                }
                budget -= message.bytes.len();
                sent.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        assert!(!delivery.is_empty());
        delivery
            .flush(|message| {
                sent.push(message.clone());
                NvstInputWrite::Sent
            })
            .unwrap();
        assert_eq!(sent, expected);
        assert!(delivery.is_empty());
    }

    #[test]
    fn only_one_bounded_paste_batch_can_wait_at_a_time() {
        let mut delivery = NvstInputDelivery::default();
        let first = opennow_streamer_protocol::text_input::TextInputSlot::default();
        let second = opennow_streamer_protocol::text_input::TextInputSlot::default();
        delivery
            .submit_text(first.submit(b"first").unwrap(), 1, |_| {
                NvstInputWrite::Deferred
            })
            .unwrap();
        let snapshot = delivery.snapshot();
        assert!(matches!(
            delivery.submit_text(second.submit(b"second").unwrap(), 2, |_| {
                NvstInputWrite::Deferred
            }),
            Err(NvstInputDeliveryError::Exhausted)
        ));
        assert_eq!(
            delivery.snapshot().pending_text_messages,
            snapshot.pending_text_messages
        );
        assert_eq!(
            delivery.snapshot().pending_text_bytes,
            snapshot.pending_text_bytes
        );
        assert!(second.submit(b"retry").is_ok());
    }
}

#[derive(Default)]
pub(crate) struct NvstInputDelivery {
    held: NvstInputHeldState,
    pending: VecDeque<PendingInput>,
    pending_messages: usize,
    pending_bytes: usize,
    pending_text_messages: usize,
    pending_text_bytes: usize,
    deferred_writes: u64,
    dropped_motion: u64,
}

enum PendingInput {
    Discrete(NvstEncodedInput),
    Text {
        reservation: UnicodeText,
        messages: VecDeque<NvstEncodedInput>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NvstInputDeliverySnapshot {
    pub(crate) pending_messages: usize,
    pub(crate) pending_bytes: usize,
    pub(crate) pending_text_messages: usize,
    pub(crate) pending_text_bytes: usize,
    pub(crate) deferred_writes: u64,
    pub(crate) dropped_motion: u64,
}

impl NvstInputDelivery {
    pub(crate) fn take_neutral_input(&mut self, timestamp_us: u64) -> Vec<NvstEncodedInput> {
        std::mem::take(self).held.neutralize(timestamp_us)
    }

    pub(crate) fn snapshot(&self) -> NvstInputDeliverySnapshot {
        NvstInputDeliverySnapshot {
            pending_messages: self.pending_messages,
            pending_bytes: self.pending_bytes,
            pending_text_messages: self.pending_text_messages,
            pending_text_bytes: self.pending_text_bytes,
            deferred_writes: self.deferred_writes,
            dropped_motion: self.dropped_motion,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn submit(
        &mut self,
        codec: &mut NvstInputCodec,
        bytes: &[u8],
        timestamp_us: u64,
        mut send: impl FnMut(&NvstEncodedInput) -> NvstInputWrite,
    ) -> Result<(), NvstInputDeliveryError> {
        self.flush(&mut send)?;
        let previous_codec = codec.clone();
        let messages = match codec.encode(bytes, timestamp_us) {
            Ok(messages) => messages,
            Err(error) => {
                *codec = previous_codec;
                return Err(NvstInputDeliveryError::Encoding(error));
            }
        };
        let only_motion = native_input_types(bytes).is_ok_and(|types| {
            !types.is_empty() && types.into_iter().all(native_input_type_is_motion)
        });
        if only_motion {
            if self.pending.is_empty() {
                for message in messages {
                    match send(&message) {
                        NvstInputWrite::Sent => {}
                        NvstInputWrite::Deferred => {
                            self.deferred_writes = self.deferred_writes.saturating_add(1);
                            self.dropped_motion = self.dropped_motion.saturating_add(1);
                        }
                        NvstInputWrite::Closed => return Err(NvstInputDeliveryError::Closed),
                    }
                }
            } else {
                self.dropped_motion = self.dropped_motion.saturating_add(messages.len() as u64);
            }
            return Ok(());
        }
        let total_bytes: usize = messages.iter().map(|message| message.bytes.len()).sum();
        if self.pending_messages.saturating_add(messages.len()) > PENDING_INPUT_MESSAGES
            || self.pending_bytes.saturating_add(total_bytes) > PENDING_INPUT_BYTES
        {
            *codec = previous_codec;
            return Err(NvstInputDeliveryError::Exhausted);
        }
        self.pending_bytes += total_bytes;
        self.pending_messages += messages.len();
        self.pending
            .extend(messages.into_iter().map(PendingInput::Discrete));
        self.flush(send)
    }

    pub(crate) fn submit_text(
        &mut self,
        text: UnicodeText,
        timestamp_us: u64,
        mut send: impl FnMut(&NvstEncodedInput) -> NvstInputWrite,
    ) -> Result<(), NvstInputDeliveryError> {
        self.flush(&mut send)?;
        if text.is_cancelled() {
            return Ok(());
        }
        if self.pending_text_messages != 0 {
            return Err(NvstInputDeliveryError::Exhausted);
        }
        let messages = unicode_text_messages(&text, timestamp_us);
        let bytes = messages.iter().map(|message| message.bytes.len()).sum();
        if !text_batch_fits(0, bytes, TEXT_BATCH_GUARD) {
            return Err(NvstInputDeliveryError::Exhausted);
        }
        self.pending_text_messages = messages.len();
        self.pending_text_bytes = bytes;
        self.pending.push_back(PendingInput::Text {
            reservation: text,
            messages: messages.into(),
        });
        self.flush(send)
    }

    pub(crate) fn flush(
        &mut self,
        mut send: impl FnMut(&NvstEncodedInput) -> NvstInputWrite,
    ) -> Result<(), NvstInputDeliveryError> {
        while let Some(pending) = self.pending.front_mut() {
            if let PendingInput::Text { reservation, .. } = pending
                && reservation.is_cancelled()
            {
                self.pending_text_messages = 0;
                self.pending_text_bytes = 0;
                self.pending.pop_front();
                continue;
            }
            let message = match pending {
                PendingInput::Discrete(message) => message,
                PendingInput::Text { messages, .. } => {
                    messages.front().expect("pending text message")
                }
            };
            match send(message) {
                NvstInputWrite::Sent => {
                    self.held.record_sent(message);
                    match pending {
                        PendingInput::Discrete(message) => {
                            self.pending_bytes -= message.bytes.len();
                            self.pending_messages -= 1;
                            self.pending.pop_front();
                        }
                        PendingInput::Text { messages, .. } => {
                            self.pending_text_bytes -=
                                messages.pop_front().expect("sent text message").bytes.len();
                            self.pending_text_messages -= 1;
                            if messages.is_empty() {
                                self.pending.pop_front();
                            }
                        }
                    }
                }
                NvstInputWrite::Deferred => {
                    self.deferred_writes = self.deferred_writes.saturating_add(1);
                    return Ok(());
                }
                NvstInputWrite::Closed => return Err(NvstInputDeliveryError::Closed),
            }
        }
        Ok(())
    }
}
