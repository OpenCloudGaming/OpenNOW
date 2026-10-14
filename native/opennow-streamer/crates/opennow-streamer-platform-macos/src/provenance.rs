use opennow_media_protocol::FrameProvenance;

pub(crate) struct DecoderProvenance {
    next_token: usize,
    maximum: usize,
    invalidated: bool,
    pending: Vec<(usize, FrameProvenance)>,
}

impl DecoderProvenance {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            next_token: 0,
            maximum,
            invalidated: false,
            pending: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, provenance: FrameProvenance) -> Option<usize> {
        if self.invalidated || self.pending.len() == self.maximum {
            return None;
        }
        let token = self.next_token.checked_add(1)?;
        self.next_token = token;
        self.pending.push((token, provenance));
        Some(token)
    }

    pub(crate) fn take(&mut self, token: usize) -> Option<FrameProvenance> {
        let index = self.pending.iter().position(|entry| entry.0 == token)?;
        Some(self.pending.swap_remove(index).1)
    }

    pub(crate) fn invalidate(&mut self) {
        self.invalidated = true;
        for (_, provenance) in &mut self.pending {
            *provenance = FrameProvenance::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::SourceStamp;

    fn provenance(sender_frame_id: Option<u64>) -> FrameProvenance {
        FrameProvenance {
            attempt_generation: 8,
            track_id: 7,
            source: Some(SourceStamp {
                sender_frame_id,
                timestamp: u64::MAX,
                clock_rate_hz: 90_000,
                ssrc: None,
            }),
        }
    }

    #[test]
    fn callback_tokens_correlate_reordered_optional_full_width_ids() {
        let mut pending = DecoderProvenance::new(3);
        let ids = [Some(u64::MAX), Some(0), None];
        let tokens = ids.map(|id| pending.insert(provenance(id)).unwrap());
        for index in [2, 0, 1] {
            assert_eq!(pending.take(tokens[index]), Some(provenance(ids[index])));
            assert_eq!(pending.take(tokens[index]), None);
        }
        assert_eq!(pending.take(0), None);
        assert_eq!(pending.take(999), None);
    }

    #[test]
    fn unknown_input_and_drain_callbacks_never_fabricate_source_metadata() {
        let mut pending = DecoderProvenance::new(3);
        let unknown = pending.insert(FrameProvenance::default()).unwrap();
        assert_eq!(pending.take(unknown), Some(FrameProvenance::default()));
        let old = pending.insert(provenance(Some(5))).unwrap();
        pending.invalidate();
        assert_eq!(pending.take(old), Some(FrameProvenance::default()));
        assert_eq!(pending.insert(provenance(Some(6))), None);
    }

    #[test]
    fn admission_is_bounded_and_failure_removal_releases_capacity_without_token_reuse() {
        assert_eq!(
            DecoderProvenance::new(0).insert(FrameProvenance::default()),
            None
        );
        let mut pending = DecoderProvenance::new(1);
        let rejected = pending.insert(provenance(Some(1))).unwrap();
        assert_eq!(pending.insert(FrameProvenance::default()), None);
        pending.take(rejected);
        let accepted = pending.insert(provenance(Some(2))).unwrap();
        assert_ne!(rejected, accepted);
        assert_eq!(pending.take(rejected), None);
        assert_eq!(pending.take(accepted), Some(provenance(Some(2))));
        pending.next_token = usize::MAX;
        assert_eq!(pending.insert(FrameProvenance::default()), None);
    }

    #[test]
    fn timing_constructor_defaults_to_unknown_and_builder_preserves_all_fields() {
        let timing = crate::FrameTiming::from_90khz(15, 3);
        assert_eq!(timing.provenance, FrameProvenance::default());
        let stamped = timing.with_provenance(provenance(Some(u64::MAX)));
        assert_eq!(stamped.provenance, provenance(Some(u64::MAX)));
        assert_eq!(
            (
                stamped.presentation_value,
                stamped.duration_value,
                stamped.timescale
            ),
            (15, 3, 90_000)
        );
    }
}
