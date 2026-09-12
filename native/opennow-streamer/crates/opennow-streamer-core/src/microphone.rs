use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use opennow_streamer_platform::{MediaRuntime, MicrophoneReceiver, MicrophoneSession};
use opennow_streamer_protocol::event;
use opennow_streamer_transport::NvstUdpReceiverControl;
use serde_json::json;

use crate::{EventSender, Lifecycle, State, lock_lifecycle};

pub(super) struct MicrophoneController {
    runtime: MediaRuntime,
    device_id: String,
    events: EventSender,
    lifecycle: Arc<Mutex<Lifecycle>>,
    generation: u64,
    enabled: Arc<AtomicBool>,
    transport: NvstUdpReceiverControl,
    capture: Option<MicrophoneSession>,
    worker: Option<JoinHandle<()>>,
}

impl MicrophoneController {
    pub fn new(
        runtime: MediaRuntime,
        transport: NvstUdpReceiverControl,
        device_id: String,
        events: EventSender,
        lifecycle: Arc<Mutex<Lifecycle>>,
        generation: u64,
    ) -> Self {
        Self {
            runtime,
            device_id,
            events,
            lifecycle,
            generation,
            enabled: Arc::new(AtomicBool::new(false)),
            transport,
            capture: None,
            worker: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled && self.enabled() {
            return Ok(());
        }
        self.stop_capture();
        if !enabled {
            publish(
                &self.events,
                &self.lifecycle,
                self.generation,
                "muted",
                false,
                None,
            );
            return Ok(());
        }
        let result = self.start_capture();
        if let Err(message) = &result {
            self.stop_capture();
            publish(
                &self.events,
                &self.lifecycle,
                self.generation,
                "error",
                false,
                Some(message),
            );
        }
        result
    }

    fn start_capture(&mut self) -> Result<(), String> {
        let capture = self.runtime.start_microphone(&self.device_id)?;
        let lifecycle = lock_lifecycle(&self.lifecycle);
        if lifecycle.generation != self.generation || lifecycle.state != State::Connected {
            return Err("Microphone session ended during capture setup".to_owned());
        }
        self.transport
            .set_microphone_enabled(true)
            .map_err(|error| error.to_string())?;
        self.enabled.store(true, Ordering::Release);
        let receiver = capture.receiver();
        self.capture = Some(capture);
        let pump = MicrophonePump {
            transport: self.transport.clone(),
            events: self.events.clone(),
            lifecycle: self.lifecycle.clone(),
            generation: self.generation,
            enabled: self.enabled.clone(),
            receiver,
        };
        self.worker = Some(
            thread::Builder::new()
                .name("opennow-microphone-uplink".to_owned())
                .spawn(move || pump.run())
                .map_err(|error| format!("Microphone worker could not start: {error}"))?,
        );
        let _ = self.events.send(event(
            "microphone-state",
            json!({"state":"ready", "enabled":true}),
        ));
        drop(lifecycle);
        Ok(())
    }

    fn stop_capture(&mut self) {
        self.enabled.store(false, Ordering::Release);
        let _ = self.transport.set_microphone_enabled(false);
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for MicrophoneController {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

fn publish(
    events: &EventSender,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    state: &str,
    enabled: bool,
    message: Option<&str>,
) {
    let lifecycle = lock_lifecycle(lifecycle);
    if lifecycle.generation == generation && lifecycle.state == State::Connected {
        let _ = events.send(event(
            "microphone-state",
            json!({"state":state, "enabled":enabled, "message":message}),
        ));
    }
}

struct MicrophonePump {
    transport: NvstUdpReceiverControl,
    events: EventSender,
    lifecycle: Arc<Mutex<Lifecycle>>,
    generation: u64,
    enabled: Arc<AtomicBool>,
    receiver: MicrophoneReceiver,
}

impl MicrophonePump {
    fn active(&self) -> bool {
        let lifecycle = lock_lifecycle(&self.lifecycle);
        self.enabled.load(Ordering::Acquire)
            && lifecycle.generation == self.generation
            && lifecycle.state == State::Connected
    }

    fn run(self) {
        let mut last_drops = (0, 0, 0);
        let mut last_report = Instant::now();
        while self.active() {
            let mut failure = None;
            for _ in 0..5 {
                match self.receiver.try_recv() {
                    Ok(Some(frame)) => {
                        if let Err(error) = self
                            .transport
                            .send_microphone_opus(frame.opus, frame.timestamp)
                        {
                            failure = Some(error.to_string());
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            let status = self.receiver.status();
            if !status.enabled && failure.is_none() {
                failure = Some(
                    status
                        .error
                        .unwrap_or_else(|| "Microphone capture stopped".to_owned()),
                );
            }
            if let Some(message) = failure {
                if self.enabled.swap(false, Ordering::AcqRel) {
                    publish(
                        &self.events,
                        &self.lifecycle,
                        self.generation,
                        "error",
                        false,
                        Some(&message),
                    );
                }
                break;
            }
            if last_report.elapsed() >= Duration::from_secs(1) {
                let drops = (
                    status.dropped_pcm_frames,
                    status.dropped_encoded_frames,
                    self.transport.microphone_dropped_frames(),
                );
                if drops != last_drops {
                    opennow_streamer_protocol::log::log_line(
                        "WARN",
                        "microphone",
                        &format!(
                            "dropped_pcm={} dropped_encoded={} dropped_transport={}",
                            drops.0, drops.1, drops.2
                        ),
                    );
                    last_drops = drops;
                }
                last_report = Instant::now();
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.enabled.store(false, Ordering::Release);
        self.receiver.stop();
        let _ = self.transport.set_microphone_enabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_status_cannot_escape_its_session_generation() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let events = EventSender::unbounded(sender);
        let lifecycle = Mutex::new(Lifecycle {
            state: State::Connected,
            context: None,
            generation: 2,
        });
        publish(&events, &lifecycle, 1, "ready", true, None);
        assert!(receiver.try_recv().is_err());
        publish(&events, &lifecycle, 2, "ready", true, None);
        let value = receiver.try_recv().unwrap();
        assert_eq!(value["type"], "microphone-state");
        assert_eq!(value["enabled"], true);
        lock_lifecycle(&lifecycle).state = State::Idle;
        publish(&events, &lifecycle, 2, "error", false, Some("device lost"));
        assert!(receiver.try_recv().is_err());
    }
}
