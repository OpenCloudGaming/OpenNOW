use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use opennow_media_protocol::FrameProvenance;

use super::VideoDecoder;
use crate::{DecodedVideoFrame, EncodedVideoFrame, Error, Result, StreamFormat, Subsystem};

const MAX_PENDING: usize = 256;
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

pub(super) struct CorrelatedDecoder {
    decoder: Box<dyn VideoDecoder>,
    pending: VecDeque<(u64, u64, FrameProvenance)>,
    last_timestamp_us: u64,
}

impl CorrelatedDecoder {
    pub(super) fn new(decoder: Box<dyn VideoDecoder>) -> Self {
        Self {
            decoder,
            pending: VecDeque::new(),
            last_timestamp_us: 0,
        }
    }

    fn correlate(&mut self, mut frames: Vec<DecodedVideoFrame>) -> Vec<DecodedVideoFrame> {
        for frame in &mut frames {
            let index = frame
                .correlation_timestamp_us
                .and_then(|token| self.pending.iter().position(|entry| entry.0 == token));
            frame.provenance = FrameProvenance::default();
            frame.timestamp_us = self.last_timestamp_us;
            if let Some(index) = index {
                let (_, timestamp, provenance) = self.pending.remove(index).unwrap();
                frame.timestamp_us = timestamp;
                frame.provenance = provenance;
            }
            frame.correlation_timestamp_us = None;
        }
        frames
    }
}

impl VideoDecoder for CorrelatedDecoder {
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        let token = NEXT_TOKEN
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |token| {
                (token < i64::MAX as u64).then_some(token + 1)
            })
            .map_err(|_| Error::backend(Subsystem::Session, "decoder token space exhausted"))?;
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending
            .push_back((token, frame.timestamp_us, frame.provenance));
        self.last_timestamp_us = frame.timestamp_us;
        let mut submitted = frame.clone();
        submitted.timestamp_us = token;
        match self.decoder.decode(&submitted) {
            Ok(frames) => Ok(self.correlate(frames)),
            Err(error) => {
                self.pending.clear();
                Err(error)
            }
        }
    }

    fn poll(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        let frames = self.decoder.poll()?;
        Ok(self.correlate(frames))
    }

    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        self.pending.clear();
        let result = self.decoder.flush();
        result.map(|frames| self.correlate(frames))
    }

    fn take_format_change(&mut self) -> Option<StreamFormat> {
        self.decoder.take_format_change()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::SourceStamp;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct DecoderState {
        submitted: Vec<u64>,
        output: Vec<DecodedVideoFrame>,
    }

    struct Decoder(Arc<Mutex<DecoderState>>);

    impl VideoDecoder for Decoder {
        fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
            self.0.lock().unwrap().submitted.push(frame.timestamp_us);
            self.poll()
        }

        fn poll(&mut self) -> Result<Vec<DecodedVideoFrame>> {
            Ok(std::mem::take(&mut self.0.lock().unwrap().output))
        }

        fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
            self.poll()
        }

        fn take_format_change(&mut self) -> Option<StreamFormat> {
            None
        }
    }

    fn provenance(id: Option<u64>) -> FrameProvenance {
        FrameProvenance {
            attempt_generation: 9,
            track_id: 2,
            source: Some(SourceStamp {
                sender_frame_id: id,
                timestamp: u64::MAX - 2,
                clock_rate_hz: 90_000,
                ssrc: None,
            }),
        }
    }

    fn output(token: Option<u64>) -> DecodedVideoFrame {
        DecodedVideoFrame {
            provenance: FrameProvenance::default(),
            correlation_timestamp_us: token,
            format: StreamFormat::video_default(2, 2).unwrap(),
            timestamp_us: token.unwrap_or_default(),
            planes: Vec::new(),
            dmabuf: None,
            vulkan: None,
        }
    }

    fn frame(id: Option<u64>) -> EncodedVideoFrame {
        EncodedVideoFrame::new(vec![1], 42, true)
            .unwrap()
            .with_provenance(provenance(id))
    }

    #[test]
    fn reordered_decoder_outputs_keep_exact_optional_sender_ids() {
        let state = Arc::new(Mutex::new(DecoderState::default()));
        let mut decoder = CorrelatedDecoder::new(Box::new(Decoder(Arc::clone(&state))));
        for id in [Some(u64::MAX), Some(0), None] {
            assert!(decoder.decode(&frame(id)).unwrap().is_empty());
        }
        let tokens = state.lock().unwrap().submitted.clone();
        state.lock().unwrap().output = vec![
            output(Some(tokens[2])),
            output(Some(tokens[0])),
            output(Some(tokens[1])),
        ];
        let decoded = decoder.poll().unwrap();
        assert_eq!(
            decoded
                .iter()
                .map(|frame| frame.provenance)
                .collect::<Vec<_>>(),
            [
                provenance(None),
                provenance(Some(u64::MAX)),
                provenance(Some(0))
            ]
        );
        assert!(decoded.iter().all(|frame| frame.timestamp_us == 42));
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn absent_decoder_timestamp_never_uses_latest_submission_provenance() {
        let state = Arc::new(Mutex::new(DecoderState::default()));
        let mut decoder = CorrelatedDecoder::new(Box::new(Decoder(Arc::clone(&state))));
        decoder.decode(&frame(Some(99))).unwrap();
        state.lock().unwrap().output = vec![output(None)];
        assert_eq!(
            decoder.poll().unwrap()[0].provenance,
            FrameProvenance::default()
        );
    }

    #[test]
    fn stale_and_evicted_decoder_tokens_remain_unknown_after_drain_and_reset() {
        let state = Arc::new(Mutex::new(DecoderState::default()));
        let mut decoder = CorrelatedDecoder::new(Box::new(Decoder(Arc::clone(&state))));
        for _ in 0..=MAX_PENDING {
            decoder.decode(&frame(Some(1))).unwrap();
        }
        assert_eq!(decoder.pending.len(), MAX_PENDING);
        let tokens = state.lock().unwrap().submitted.clone();
        state.lock().unwrap().output = vec![output(Some(tokens[0]))];
        assert_eq!(
            decoder.poll().unwrap()[0].provenance,
            FrameProvenance::default()
        );
        state.lock().unwrap().output = vec![output(Some(tokens[2]))];
        assert_eq!(
            decoder.flush().unwrap()[0].provenance,
            FrameProvenance::default()
        );
        assert!(decoder.pending.is_empty());
        state.lock().unwrap().output = vec![output(Some(tokens[1]))];
        assert_eq!(
            decoder.poll().unwrap()[0].provenance,
            FrameProvenance::default()
        );
        let mut replacement = CorrelatedDecoder::new(Box::new(Decoder(Arc::clone(&state))));
        replacement.decode(&frame(Some(2))).unwrap();
        state.lock().unwrap().output = vec![output(Some(*tokens.last().unwrap()))];
        assert_eq!(
            replacement.poll().unwrap()[0].provenance,
            FrameProvenance::default()
        );
    }
}
