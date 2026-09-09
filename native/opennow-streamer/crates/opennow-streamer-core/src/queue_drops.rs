use std::collections::HashMap;
use std::time::{Duration, Instant};

use opennow_streamer_protocol::event;
use serde_json::{Value, json};

use crate::EventSender;

pub(crate) struct QueueDropReports {
    pending: HashMap<&'static str, usize>,
    last_flush: Instant,
}

impl QueueDropReports {
    pub(crate) fn new() -> Self {
        Self {
            pending: HashMap::new(),
            last_flush: Instant::now(),
        }
    }

    pub(crate) fn record(&mut self, media: &'static str, count: usize) {
        if count != 0 {
            let pending = self.pending.entry(media).or_default();
            *pending = pending.saturating_add(count);
        }
    }

    pub(crate) fn flush(&mut self, output: &EventSender, now: Instant, final_flush: bool) {
        if !final_flush && now.duration_since(self.last_flush) < Duration::from_secs(1) {
            return;
        }
        self.last_flush = now;
        self.pending.retain(|media, count| {
            let report = queue_drop_event(media, *count);
            let summary =
                final_flush.then(|| opennow_streamer_protocol::log::message_summary(&report));
            if output.send(report).is_ok() {
                return false;
            }
            if let Some(summary) = summary {
                opennow_streamer_protocol::log::log_line(
                    "WARN",
                    "native-queue-drop",
                    &format!("{summary} delivery=event-queue-unavailable"),
                );
                return false;
            }
            true
        });
    }
}

fn queue_drop_event(media: &str, count: usize) -> Value {
    let unit = match media {
        "video" | "d3d11-video" | "d3d11-decode" | "present" | "linux-present" | "videotoolbox"
        | "videotoolbox-hevc" | "videotoolbox-av1" | "video-decode" | "video-presentation" => {
            "frames"
        }
        "audio-output" => "samples",
        "audio" | "wasapi" | "linux-audio" | "coreaudio" => "packets",
        _ => "items",
    };
    let mut report = event(
        "log",
        json!({
            "event": "queue-dropped",
            "media": media,
            "count": count,
            "unit": unit,
            "level": "debug",
            "message": format!("Low-latency {media} queues dropped {count} stale {unit}")
        }),
    );
    if unit == "samples" {
        report["sampleRate"] = json!(48_000);
        report["channels"] = json!(2);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_units_preserve_count_and_only_describe_known_sample_duration() {
        for (sources, unit) in [
            (
                &[
                    "video",
                    "d3d11-video",
                    "d3d11-decode",
                    "present",
                    "linux-present",
                    "videotoolbox",
                    "videotoolbox-hevc",
                    "videotoolbox-av1",
                    "video-decode",
                    "video-presentation",
                ][..],
                "frames",
            ),
            (
                &["audio", "wasapi", "linux-audio", "coreaudio"][..],
                "packets",
            ),
            (&["audio-output"][..], "samples"),
            (&["unknown", "future-audio", "future-video"][..], "items"),
        ] {
            for source in sources {
                let report = queue_drop_event(source, 37);
                assert_eq!(report["event"], "queue-dropped");
                assert_eq!(report["media"], *source);
                assert_eq!(report["count"], 37);
                assert_eq!(report["unit"], unit);
                if unit == "samples" {
                    assert_eq!(report["sampleRate"], 48_000);
                    assert_eq!(report["channels"], 2);
                } else {
                    assert!(report.get("sampleRate").is_none());
                    assert!(report.get("channels").is_none());
                }
            }
        }
    }

    #[test]
    fn periodic_flush_delivers_idle_sources_and_final_flush_is_exact() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let sender = EventSender::unbounded(sender);
        let mut reports = QueueDropReports::new();
        let now = reports.last_flush;
        reports.record("video", 3);
        reports.record("video", 4);
        reports.record("audio-output", 96_000);
        reports.record("audio", 2);
        reports.record("unknown", 0);
        reports.flush(&sender, now, false);
        assert!(receiver.try_recv().is_err());
        reports.flush(&sender, now + Duration::from_secs(1), false);
        let delivered: HashMap<_, _> = receiver
            .try_iter()
            .map(|value| {
                (
                    value["media"].as_str().unwrap().to_owned(),
                    value["count"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            delivered,
            HashMap::from([
                ("video".to_owned(), 7),
                ("audio-output".to_owned(), 96_000),
                ("audio".to_owned(), 2),
            ])
        );
        reports.record("video", 5);
        reports.flush(&sender, now + Duration::from_secs(1), true);
        assert_eq!(receiver.try_recv().unwrap()["count"], 5);
        reports.flush(&sender, now + Duration::from_secs(2), true);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn full_event_queue_retains_pending_counts_for_retry() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let sender = EventSender::bounded(sender);
        sender.send(json!({"type": "telemetry"})).unwrap();
        let mut reports = QueueDropReports::new();
        let now = reports.last_flush;
        reports.record("video", 3);
        reports.flush(&sender, now + Duration::from_secs(1), false);
        receiver.recv().unwrap();
        reports.record("video", 4);
        reports.flush(&sender, now + Duration::from_secs(2), false);
        assert_eq!(receiver.recv().unwrap()["count"], 7);
        reports.flush(&sender, now + Duration::from_secs(3), false);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn final_flush_does_not_wait_for_dispatcher_capacity() {
        let path = std::env::temp_dir().join(format!(
            "opennow-final-queue-drops-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        opennow_streamer_protocol::log::set_log_file(path.to_str().unwrap()).unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let sender = EventSender::bounded(sender);
        sender.send(json!({"type": "telemetry"})).unwrap();
        let (finished, completion) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut reports = QueueDropReports::new();
            reports.record("audio-output", 96_000);
            reports.flush(&sender, Instant::now(), true);
            assert!(reports.pending.is_empty());
            reports.flush(&sender, Instant::now(), true);
            finished.send(()).unwrap();
        });
        completion.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(receiver.recv().unwrap()["type"], "telemetry");
        worker.join().unwrap();
        assert!(receiver.try_recv().is_err());
        let log = std::fs::read_to_string(&path).unwrap();
        let reports: Vec<_> = log
            .lines()
            .filter(|line| line.contains("delivery=event-queue-unavailable"))
            .collect();
        assert_eq!(reports.len(), 1);
        for field in [
            "event=queue-dropped",
            "media=audio-output",
            "unit=samples",
            "count=96000",
            "sampleRate=48000",
            "channels=2",
        ] {
            assert!(reports[0].contains(field), "{}", reports[0]);
        }
        let _ = std::fs::remove_file(path);
    }
}
