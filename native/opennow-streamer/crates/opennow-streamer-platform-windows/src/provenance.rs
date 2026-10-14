use crate::{BackendEvent, event_queue::EventQueue};
use opennow_media_protocol::FrameProvenance;

const MAX_PENDING_FRAMES: usize = 256;

#[derive(Default)]
pub(crate) struct DecoderProvenance {
    next_token: i64,
    pending: Vec<(i64, i64, FrameProvenance)>,
}

impl DecoderProvenance {
    pub(crate) fn insert(
        &mut self,
        timestamp_100ns: i64,
        provenance: FrameProvenance,
    ) -> Result<i64, &'static str> {
        if self.pending.len() == MAX_PENDING_FRAMES {
            return Err("decoder provenance capacity exhausted");
        }
        let token = self
            .next_token
            .checked_add(1)
            .ok_or("decoder provenance tokens exhausted")?;
        self.next_token = token;
        self.pending.push((token, timestamp_100ns, provenance));
        Ok(token)
    }

    pub(crate) fn take(&mut self, token: Option<i64>) -> Option<(i64, FrameProvenance)> {
        let token = token?;
        let index = self.pending.iter().position(|entry| entry.0 == token)?;
        let (_, timestamp, provenance) = self.pending.swap_remove(index);
        Some((timestamp, provenance))
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(crate) fn decoded_output(
        &mut self,
        token: Option<i64>,
        events: &EventQueue,
    ) -> (i64, FrameProvenance) {
        let (timestamp, provenance) = self.take(token).unwrap_or_default();
        let _ = events.push(BackendEvent::VideoFrameDecoded { provenance });
        (timestamp, provenance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::SourceStamp;

    fn provenance(sender_frame_id: Option<u64>) -> FrameProvenance {
        FrameProvenance {
            attempt_generation: 9,
            track_id: 3,
            source: Some(SourceStamp {
                sender_frame_id,
                timestamp: u64::MAX,
                clock_rate_hz: 90_000,
                ssrc: Some(0),
            }),
        }
    }

    #[test]
    fn reordered_outputs_preserve_full_sender_metadata_and_original_timing() {
        let mut pending = DecoderProvenance::default();
        let ids = [Some(u64::MAX), Some(0), None];
        let tokens = ids.map(|id| pending.insert(42, provenance(id)).unwrap());
        for index in [2, 0, 1] {
            assert_eq!(
                pending.take(Some(tokens[index])),
                Some((42, provenance(ids[index])))
            );
            assert_eq!(pending.take(Some(tokens[index])), None);
        }
        assert_eq!(pending.take(None), None);
        assert_eq!(pending.take(Some(0)), None);
    }

    #[test]
    fn unknown_and_reset_outputs_never_acquire_other_frames_metadata() {
        let mut pending = DecoderProvenance::default();
        let old = pending.insert(10, provenance(Some(3))).unwrap();
        pending.clear();
        let new = pending.insert(10, FrameProvenance::default()).unwrap();
        assert_ne!(old, new);
        assert_eq!(pending.take(Some(old)), None);
        assert_eq!(
            pending.take(Some(new)),
            Some((10, FrameProvenance::default()))
        );
    }

    #[test]
    fn only_actual_output_correlation_emits_one_decoded_event_per_sample() {
        let mut pending = DecoderProvenance::default();
        let events = EventQueue::new(8);
        let first = pending.insert(10, provenance(Some(u64::MAX))).unwrap();
        let second = pending.insert(20, provenance(None)).unwrap();
        let rejected = pending.insert(30, provenance(Some(4))).unwrap();
        pending.take(Some(rejected));
        assert_eq!(events.try_pop(), None);

        for (token, expected) in [
            (Some(second), (20, provenance(None))),
            (Some(first), (10, provenance(Some(u64::MAX)))),
            (Some(first), (0, FrameProvenance::default())),
            (Some(rejected), (0, FrameProvenance::default())),
            (None, (0, FrameProvenance::default())),
        ] {
            assert_eq!(pending.decoded_output(token, &events), expected);
            assert_eq!(
                events.try_pop(),
                Some(BackendEvent::VideoFrameDecoded {
                    provenance: expected.1
                })
            );
            assert_eq!(events.try_pop(), None);
        }

        let stale = pending.insert(40, provenance(Some(5))).unwrap();
        pending.clear();
        assert_eq!(events.try_pop(), None);
        assert_eq!(
            pending.decoded_output(Some(stale), &events),
            (0, FrameProvenance::default())
        );
        assert_eq!(
            events.try_pop(),
            Some(BackendEvent::VideoFrameDecoded {
                provenance: FrameProvenance::default()
            })
        );
        assert_eq!(events.try_pop(), None);
    }

    #[test]
    fn capacity_and_token_exhaustion_fail_without_overwriting_pending_frames() {
        let mut pending = DecoderProvenance::default();
        let first = pending.insert(0, provenance(Some(0))).unwrap();
        for _ in 1..MAX_PENDING_FRAMES {
            pending.insert(0, FrameProvenance::default()).unwrap();
        }
        assert!(pending.insert(0, FrameProvenance::default()).is_err());
        assert_eq!(pending.take(Some(first)), Some((0, provenance(Some(0)))));
        pending.clear();
        pending.next_token = i64::MAX;
        assert!(pending.insert(0, FrameProvenance::default()).is_err());
    }
}
