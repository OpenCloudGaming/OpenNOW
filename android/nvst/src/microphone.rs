//! 10 ms, mono 48 kHz Opus voice on the DTLS bundle.
use opus::{Application, Bitrate, Channels, Encoder};

const SAMPLE_RATE: u32 = 48_000;
// GFN's native microphone expects 10 ms frames. See OpenNOW #1014.
pub(crate) const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 100;

pub(crate) struct MicrophoneEncoder {
    encoder: Encoder,
}
impl MicrophoneEncoder {
    pub(crate) fn new() -> Result<Self, opus::Error> {
        let mut encoder = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)?;
        encoder.set_bitrate(Bitrate::Bits(16_000))?;
        encoder.set_inband_fec(false)?;
        Ok(Self { encoder })
    }

    pub(crate) fn encode(
        &mut self,
        mono: &[i16; FRAME_SAMPLES],
        restart: bool,
    ) -> Result<Vec<u8>, opus::Error> {
        if restart {
            self.encoder.reset_state()?;
        }
        let mut encoded = vec![0; 1023];
        let length = self.encoder.encode(mono, &mut encoded)?;
        encoded.truncate(length);
        Ok(encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_packets_are_plain_ten_millisecond_opus() {
        let mut encoder = MicrophoneEncoder::new().unwrap();
        let pcm = std::array::from_fn(|i| ((i as f32 * 0.06).sin() * 8000.0) as i16);
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, Channels::Mono).unwrap();
        let mut output = [0; 480];
        for restart in [true, false, true] {
            let packet = encoder.encode(&pcm, restart).unwrap();
            assert_eq!(decoder.decode(&packet, &mut output, false).unwrap(), 480);
        }
    }
}
