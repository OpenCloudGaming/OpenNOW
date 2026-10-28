use super::*;
use std::collections::VecDeque;

pub(super) const TELEMETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct MouseInputTimings {
    relative: InputTimingWindow,
    absolute: InputTimingWindow,
}

impl MouseInputTimings {
    pub(super) fn record(&mut self, sample: &CapturedInputSample, submitted_at: Instant) {
        let window = match sample.input {
            CapturedInput::MouseMove { .. } => &mut self.relative,
            CapturedInput::MouseAbsolute { .. } => &mut self.absolute,
            _ => return,
        };
        let Some(elapsed) = submitted_at.checked_duration_since(sample.captured_at) else {
            return;
        };
        if window.samples_us.len() == 256 {
            window.samples_us.pop_front();
        }
        window
            .samples_us
            .push_back(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX));
        window.submitted = window.submitted.saturating_add(1);
    }

    pub(super) fn take_report(&mut self) -> Option<Value> {
        if self.relative.submitted == 0 && self.absolute.submitted == 0 {
            return None;
        }
        Some(json!({
            "relative": self.relative.take_report(),
            "absolute": self.absolute.take_report(),
        }))
    }
}

#[derive(Default)]
struct InputTimingWindow {
    samples_us: VecDeque<u64>,
    submitted: u64,
}

impl InputTimingWindow {
    fn take_report(&mut self) -> Value {
        if self.samples_us.is_empty() {
            return Value::Null;
        }
        let mut samples: Vec<_> = self.samples_us.iter().copied().collect();
        self.samples_us.clear();
        samples.sort_unstable();
        let percentile =
            |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1] as f64 / 1_000.0;
        json!({
            "submitted": std::mem::take(&mut self.submitted),
            "windowSamples": samples.len(),
            "captureToTransportSubmitMs": {
                "p50": percentile(50),
                "p95": percentile(95),
                "max": samples[samples.len() - 1] as f64 / 1_000.0,
            },
        })
    }
}

pub(super) fn media_telemetry(
    elapsed: Duration,
    frames: u64,
    bytes: u64,
    socket_bytes: Option<u64>,
    previous_socket_bytes: &mut u64,
    peak_bitrate_mbps: &mut f64,
) -> Value {
    let seconds = elapsed.as_secs_f64();
    let bitrate_mbps = bytes as f64 * 8.0 / seconds / 1_000_000.0;
    let receive_bitrate_mbps = socket_bytes.and_then(|bytes| {
        bytes
            .checked_sub(*previous_socket_bytes)
            .map(|delta| delta as f64 * 8.0 / seconds / 1_000_000.0)
    });
    if let Some(bytes) = socket_bytes {
        *previous_socket_bytes = bytes;
    }
    *peak_bitrate_mbps = peak_bitrate_mbps.max(bitrate_mbps);
    json!({
        "framesPerSecond": frames as f64 / seconds,
        "bitrateMbps": bitrate_mbps,
        "receiveBitrateMbps": receive_bitrate_mbps,
        "peakBitrateMbps": peak_bitrate_mbps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_input_timings_distinguish_modes_and_measure_queue_delay() {
        let captured_at = Instant::now();
        let mut timings = MouseInputTimings::default();
        assert!(timings.take_report().is_none());
        let mut sample = CapturedInputSample {
            input: CapturedInput::MouseMove {
                delta_x: 123,
                delta_y: -456,
            },
            captured_at,
        };
        for micros in [250, 750, 16_750] {
            timings.record(&sample, captured_at + Duration::from_micros(micros));
        }
        sample.input = CapturedInput::MouseAbsolute {
            x: 123,
            y: 456,
            width: 3840,
            height: 2160,
        };
        timings.record(&sample, captured_at + Duration::from_millis(2));
        assert_eq!(
            timings.take_report().unwrap(),
            json!({
                "relative": {
                    "submitted": 3,
                    "windowSamples": 3,
                    "captureToTransportSubmitMs": {"p50": 0.75, "p95": 16.75, "max": 16.75},
                },
                "absolute": {
                    "submitted": 1,
                    "windowSamples": 1,
                    "captureToTransportSubmitMs": {"p50": 2.0, "p95": 2.0, "max": 2.0},
                },
            })
        );
        assert!(timings.take_report().is_none());
    }

    #[test]
    fn mouse_input_timing_storage_is_bounded_and_never_invents_measurements() {
        let captured_at = Instant::now();
        let mut timings = MouseInputTimings::default();
        let mut sample = CapturedInputSample {
            input: CapturedInput::MouseMove {
                delta_x: 1,
                delta_y: 0,
            },
            captured_at,
        };
        timings.record(&sample, captured_at - Duration::from_millis(1));
        assert!(timings.take_report().is_none());
        for micros in 1..=1_000 {
            timings.record(&sample, captured_at + Duration::from_micros(micros));
        }
        assert_eq!(timings.relative.samples_us.len(), 256);
        let report = timings.take_report().unwrap();
        assert_eq!(report["relative"]["submitted"], 1_000);
        assert_eq!(report["relative"]["windowSamples"], 256);
        assert_eq!(
            report["relative"]["captureToTransportSubmitMs"]["p50"],
            0.872
        );
        assert_eq!(
            report["relative"]["captureToTransportSubmitMs"]["p95"],
            0.988
        );
        assert_eq!(report["relative"]["captureToTransportSubmitMs"]["max"], 1.0);
        assert_eq!(report["absolute"], Value::Null);
        sample.input = CapturedInput::Key {
            virtual_key: 0x41,
            modifiers: 0,
            pressed: true,
        };
        timings.record(&sample, captured_at + Duration::from_millis(5));
        assert!(timings.take_report().is_none());
        assert!(MouseInputTimings::default().take_report().is_none());
    }
}
