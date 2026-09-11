//! 20 ms, stereo 48 kHz Opus voice with RFC 2198 redundancy on the DTLS bundle.
use opus::{Application, Bitrate, Channels, Encoder};
use std::collections::VecDeque;

pub(crate) const FRAME_SAMPLES: usize = 960;
const OPUS_PT: u8 = 111;

pub(crate) struct MicrophoneEncoder {
    encoder: Encoder,
    history: VecDeque<(u32, Vec<u8>)>,
}
impl MicrophoneEncoder {
    pub(crate) fn new() -> Result<Self, opus::Error> {
        let mut encoder = Encoder::new(48_000, Channels::Stereo, Application::Voip)?;
        encoder.set_bitrate(Bitrate::Bits(16_000))?;
        encoder.set_inband_fec(false)?;
        Ok(Self {
            encoder,
            history: VecDeque::with_capacity(3),
        })
    }

    pub(crate) fn encode(
        &mut self,
        mono: &[i16; FRAME_SAMPLES],
        timestamp: u32,
        restart: bool,
    ) -> Result<Vec<u8>, opus::Error> {
        if restart {
            self.encoder.reset_state()?;
            // Muted audio must never reappear through redundant blocks on unmute.
            self.history.clear();
        }
        let mut stereo = [0_i16; FRAME_SAMPLES * 2];
        for (frame, sample) in stereo.chunks_exact_mut(2).zip(mono) {
            frame.fill(*sample);
        }
        let mut encoded = vec![0; 1023];
        let length = self.encoder.encode(&stereo, &mut encoded)?;
        encoded.truncate(length);
        self.history
            .retain(|(time, _)| timestamp.wrapping_sub(*time) < 16_384);
        let mut payload = Vec::with_capacity(
            1 + length + self.history.iter().map(|(_, b)| 4 + b.len()).sum::<usize>(),
        );
        for (time, data) in &self.history {
            let offset = timestamp.wrapping_sub(*time);
            // F=1, 7-bit PT, 14-bit timestamp offset, 10-bit block length.
            payload.extend_from_slice(&[
                0x80 | OPUS_PT,
                (offset >> 6) as u8,
                ((offset << 2) as u8) | ((data.len() >> 8) as u8),
                data.len() as u8,
            ]);
        }
        payload.push(OPUS_PT);
        for (_, data) in &self.history {
            payload.extend_from_slice(data);
        }
        payload.extend_from_slice(&encoded);
        if self.history.len() == 3 {
            self.history.pop_front();
        }
        self.history.push_back((timestamp, encoded));
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opus_roundtrip_and_redundancy_reset_on_unmute() {
        let mut encoder = MicrophoneEncoder::new().unwrap();
        let pcm = std::array::from_fn(|i| ((i as f32 * 0.06).sin() * 8000.0) as i16);
        let mut decoder = opus::Decoder::new(48_000, Channels::Stereo).unwrap();
        for frame in 0..6 {
            let timestamp = u32::MAX.wrapping_add(frame * 960);
            let packet = encoder.encode(&pcm, timestamp, frame == 0).unwrap();
            let count = (frame as usize).min(3);
            let mut data_start = 4 * count + 1;
            for header in packet[..count * 4].chunks_exact(4) {
                assert_eq!(header[0], 0x80 | OPUS_PT);
                let offset = (u32::from(header[1]) << 6) | u32::from(header[2] >> 2);
                assert!(offset > 0 && offset <= 2880);
                data_start += (usize::from(header[2] & 3) << 8) | usize::from(header[3]);
            }
            assert_eq!(packet[4 * count], OPUS_PT);
            let mut output = [0; FRAME_SAMPLES * 2];
            assert_eq!(
                decoder
                    .decode(&packet[data_start..], &mut output, false)
                    .unwrap(),
                FRAME_SAMPLES
            );
        }
        let resumed = encoder.encode(&pcm, 100_000, true).unwrap();
        assert_eq!(resumed[0], OPUS_PT);
    }
}
