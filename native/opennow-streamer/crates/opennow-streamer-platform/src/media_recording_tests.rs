use super::*;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::sync::mpsc;
use std::time::Duration;

const DEADLOCK_TIMEOUT: Duration = Duration::from_secs(5);

fn frame(index: u32) -> EncodedFrame {
    EncodedFrame {
        mid: "video".to_owned(),
        codec: MediaCodec::H264,
        data: Arc::from([0, 0, 0, 1, 0x65, 0x88]),
        frame_index: Some(index),
        timestamp: 90_000 + u64::from(index) * 3_000,
        clock_rate_hz: 90_000,
        keyframe: true,
        contiguous: true,
        ssrc: Some(9),
    }
}

fn cut(reason: RecordingCutReason) -> RecordingCompletion {
    RecordingCompletion::Cut { reason }
}

fn terminal(receiver: &EncodedRecordingReceiver) -> RecordingCompletion {
    receiver
        .recv()
        .expect_err("recording must not admit another frame")
}

fn waiting_receiver(
    mut receiver: EncodedRecordingReceiver,
) -> (
    mpsc::Receiver<()>,
    mpsc::Receiver<Result<EncodedFrame, RecordingCompletion>>,
    JoinHandle<()>,
) {
    let (waiting, entered_wait) = mpsc::sync_channel(0);
    receiver.before_wait = Some(waiting);
    let (result, completed) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let _ = result.send(receiver.recv());
    });
    (entered_wait, completed, worker)
}

#[test]
fn close_before_receive_needs_no_later_frame() {
    for completion in [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::Discontinuity),
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ] {
        let tap = RecordingTap::default();
        let receiver = tap.subscribe().unwrap();
        tap.close(completion);
        assert_eq!(terminal(&receiver), completion);
        assert_eq!(terminal(&receiver), completion);
    }
}

#[test]
fn signal_between_empty_check_and_wait_is_not_lost() {
    for completion in [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ] {
        let tap = RecordingTap::default();
        let mut receiver = tap.subscribe().unwrap();
        let (release, wait_release) = mpsc::sync_channel(0);
        receiver.wait_release = Some(wait_release);
        let (entered_wait, completed, worker) = waiting_receiver(receiver);
        entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        drop(entered_wait);
        tap.close(completion);
        release.send(()).unwrap();
        drop(release);
        assert_eq!(
            completed
                .recv_timeout(DEADLOCK_TIMEOUT)
                .unwrap()
                .unwrap_err(),
            completion
        );
        worker.join().unwrap();
    }
}

#[test]
fn close_wakes_receiver_after_it_enters_wait_without_more_frames() {
    for completion in [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::Discontinuity),
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ] {
        let tap = RecordingTap::default();
        let receiver = tap.subscribe().unwrap();
        let (entered_wait, completed, worker) = waiting_receiver(receiver);
        entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        drop(entered_wait);
        assert!(matches!(
            tap.wake_receiver.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        assert!(matches!(
            completed.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        tap.close(completion);
        assert_eq!(
            completed
                .recv_timeout(DEADLOCK_TIMEOUT)
                .unwrap()
                .unwrap_err(),
            completion
        );
        worker.join().unwrap();
    }
}

#[test]
fn graceful_signal_before_sender_drop_can_wake_and_wait_again() {
    let tap = RecordingTap::default();
    let receiver = tap.subscribe().unwrap();
    let (entered_wait, completed, worker) = waiting_receiver(receiver);
    entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
    tap.signal(
        tap.terminal.load(Ordering::Acquire),
        RecordingCompletion::Complete,
    );
    entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
    drop(entered_wait);
    assert!(matches!(
        completed.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    tap.unsubscribe();
    assert_eq!(
        completed
            .recv_timeout(DEADLOCK_TIMEOUT)
            .unwrap()
            .unwrap_err(),
        RecordingCompletion::Complete
    );
    worker.join().unwrap();
}

#[test]
fn publishing_wakes_a_waiting_receiver_with_the_original_frame() {
    let tap = RecordingTap::default();
    let receiver = tap.subscribe().unwrap();
    let (entered_wait, completed, worker) = waiting_receiver(receiver);
    entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
    drop(entered_wait);
    let expected = frame(73);
    tap.publish(&expected);
    let received = completed.recv_timeout(DEADLOCK_TIMEOUT).unwrap().unwrap();
    assert_eq!(received.frame_index, expected.frame_index);
    assert_eq!(received.timestamp, expected.timestamp);
    assert_eq!(received.clock_rate_hz, expected.clock_rate_hz);
    assert_eq!(received.ssrc, expected.ssrc);
    assert!(Arc::ptr_eq(&received.data, &expected.data));
    worker.join().unwrap();
}

#[test]
fn queued_data_and_close_between_empty_check_and_wait_obey_completion() {
    for completion in [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::Interrupted),
    ] {
        let tap = RecordingTap::default();
        let mut receiver = tap.subscribe().unwrap();
        let (release, wait_release) = mpsc::sync_channel(0);
        receiver.wait_release = Some(wait_release);
        let (entered_wait, completed, worker) = waiting_receiver(receiver);
        entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        drop(entered_wait);
        tap.publish(&frame(81));
        tap.close(completion);
        release.send(()).unwrap();
        drop(release);
        let result = completed.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        match completion {
            RecordingCompletion::Complete => assert_eq!(result.unwrap().frame_index, Some(81)),
            RecordingCompletion::Cut { .. } => assert_eq!(result.unwrap_err(), completion),
        }
        worker.join().unwrap();
    }
}

#[test]
fn subscription_contention_cuts_without_blocking_the_publisher_or_more_traffic() {
    for hold_before_block in [true, false] {
        let tap = Arc::new(RecordingTap::default());
        let mut receiver = tap.subscribe().unwrap();
        let (release, wait_release) = mpsc::sync_channel(0);
        if hold_before_block {
            receiver.wait_release = Some(wait_release);
        }
        let (entered_wait, completed, receiver_worker) = waiting_receiver(receiver);
        entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        drop(entered_wait);
        let subscription = tap.subscription.lock().unwrap();
        let publisher_tap = Arc::clone(&tap);
        let (published, publication_finished) = mpsc::sync_channel(1);
        let publisher = thread::spawn(move || {
            publisher_tap.publish(&frame(1));
            published.send(()).unwrap();
        });
        publication_finished.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
        if hold_before_block {
            release.send(()).unwrap();
        }
        drop(release);
        assert_eq!(
            completed
                .recv_timeout(DEADLOCK_TIMEOUT)
                .unwrap()
                .unwrap_err(),
            cut(RecordingCutReason::QueueOverflow)
        );
        assert!(subscription.sender.is_some());
        drop(subscription);
        publisher.join().unwrap();
        receiver_worker.join().unwrap();
    }
}

#[test]
fn graceful_close_drains_accepted_frames_but_every_cut_rejects_them() {
    let completions = [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::Discontinuity),
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ];
    for completion in completions {
        let tap = RecordingTap::default();
        let receiver = tap.subscribe().unwrap();
        tap.publish(&frame(1));
        tap.publish(&frame(2));
        tap.close(completion);
        tap.publish(&frame(3));
        if completion == RecordingCompletion::Complete {
            assert_eq!(receiver.recv().unwrap().frame_index, Some(1));
            assert_eq!(receiver.recv().unwrap().frame_index, Some(2));
        }
        assert_eq!(terminal(&receiver), completion);
    }
}

#[test]
fn terminal_reason_is_first_wins_even_when_close_is_repeated() {
    let completions = [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::Discontinuity),
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ];
    for first in completions {
        for second in completions {
            let tap = RecordingTap::default();
            let receiver = tap.subscribe().unwrap();
            tap.close(first);
            tap.close(second);
            tap.unsubscribe();
            assert_eq!(terminal(&receiver), first);
        }
    }
}

#[test]
fn old_receiver_lifetime_prevents_terminal_state_reset_on_resubscribe() {
    for completion in [
        RecordingCompletion::Complete,
        cut(RecordingCutReason::QueueOverflow),
        cut(RecordingCutReason::Interrupted),
    ] {
        let tap = RecordingTap::default();
        let receiver = tap.subscribe().unwrap();
        assert!(tap.subscribe().is_err());
        tap.close(completion);
        let old_status = tap.terminal.load(Ordering::Acquire);
        assert!(tap.subscribe().is_err());
        assert_eq!(tap.terminal.load(Ordering::Acquire), old_status);
        assert_eq!(terminal(&receiver), completion);
        assert!(tap.subscribe().is_err());
        drop(receiver);
        let replacement = tap.subscribe().unwrap();
        assert_ne!(tap.terminal.load(Ordering::Acquire), old_status);
        tap.publish(&frame(2));
        tap.unsubscribe();
        assert_eq!(replacement.recv().unwrap().frame_index, Some(2));
        assert_eq!(terminal(&replacement), RecordingCompletion::Complete);
    }
}

#[test]
fn stale_publisher_compare_exchange_cannot_cut_the_next_subscription() {
    let tap = RecordingTap::default();
    let receiver = tap.subscribe().unwrap();
    let stale_running_status = tap.terminal.load(Ordering::Acquire);
    tap.unsubscribe();
    drop(receiver);
    let receiver = tap.subscribe().unwrap();
    let current_status = tap.terminal.load(Ordering::Acquire);
    assert_ne!(current_status, stale_running_status);
    tap.signal(stale_running_status, cut(RecordingCutReason::QueueOverflow));
    assert_eq!(tap.terminal.load(Ordering::Acquire), current_status);
    assert_eq!(receiver.cut(), None);
    tap.publish(&frame(42));
    tap.unsubscribe();
    assert_eq!(receiver.recv().unwrap().frame_index, Some(42));
    assert_eq!(terminal(&receiver), RecordingCompletion::Complete);
}

#[test]
fn stale_wake_is_consumed_once_then_the_new_receiver_waits_for_its_own_signal() {
    let tap = RecordingTap::default();
    let previous = tap.subscribe().unwrap();
    tap.unsubscribe();
    assert_eq!(terminal(&previous), RecordingCompletion::Complete);
    drop(previous);
    let receiver = tap.subscribe().unwrap();
    let (entered_wait, completed, worker) = waiting_receiver(receiver);
    entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
    entered_wait.recv_timeout(DEADLOCK_TIMEOUT).unwrap();
    drop(entered_wait);
    assert!(matches!(
        tap.wake_receiver.try_lock(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    assert!(matches!(
        completed.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    tap.close(cut(RecordingCutReason::Interrupted));
    assert_eq!(
        completed
            .recv_timeout(DEADLOCK_TIMEOUT)
            .unwrap()
            .unwrap_err(),
        cut(RecordingCutReason::Interrupted)
    );
    worker.join().unwrap();
}

#[test]
fn wake_notifications_coalesce_and_overflow_does_not_drain_pending_frames() {
    let tap = RecordingTap::default();
    let receiver = tap.subscribe().unwrap();
    for index in 0..RECORDING_TAP_QUEUE_CAPACITY {
        tap.publish(&frame(index as u32));
    }
    {
        let wake = tap.wake_receiver.lock().unwrap();
        assert_eq!(wake.try_recv(), Ok(()));
        assert_eq!(wake.try_recv(), Err(mpsc::TryRecvError::Empty));
    }
    tap.publish(&frame(RECORDING_TAP_QUEUE_CAPACITY as u32));
    assert!(receiver.overflowed());
    assert_eq!(terminal(&receiver), cut(RecordingCutReason::QueueOverflow));
    tap.unsubscribe();
    assert_eq!(terminal(&receiver), cut(RecordingCutReason::QueueOverflow));
}

fn directory(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..128 {
        let directory = std::env::temp_dir().join(format!(
            "opennow-recording-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        match std::fs::create_dir(&directory) {
            Ok(()) => return directory,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("failed to create recording fixture directory: {error}"),
        }
    }
    panic!("exhausted recording fixture directory allocation attempts");
}

#[test]
fn recording_fixture_directories_are_exclusively_owned() {
    let first = directory("same-clock-tick");
    let second = directory("same-clock-tick");
    assert_ne!(first, second);
    std::fs::remove_dir_all(first).unwrap();
    std::fs::remove_dir_all(second).unwrap();
}

#[test]
fn overflow_before_any_mux_commit_does_not_create_a_saved_recording() {
    let tap = RecordingTap::default();
    let receiver = tap.subscribe().unwrap();
    for index in 0..=RECORDING_TAP_QUEUE_CAPACITY {
        tap.publish(&frame(index as u32));
    }
    let directory = directory("no-prefix-overflow");
    let output = directory.join("overflow.mkv");
    let error = crate::recording::record_matroska(&output, MediaStreamConfig::default(), receiver)
        .expect_err("queued frames are not a mux-committed prefix");
    assert_eq!(
        error,
        "recording ended before a decodable video keyframe arrived"
    );
    assert!(!output.exists());
    assert!(!directory.join(".overflow.mkv.part").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

struct CutAfterPayload {
    file: File,
    payload: Vec<u8>,
    triggered: Arc<AtomicBool>,
    tap: Arc<RecordingTap>,
    reason: RecordingCutReason,
}

impl Write for CutAfterPayload {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.file.write(bytes)?;
        if bytes[..written].ends_with(&self.payload) && !self.triggered.swap(true, Ordering::AcqRel)
        {
            match self.reason {
                RecordingCutReason::QueueOverflow => {
                    for index in 1..=RECORDING_TAP_QUEUE_CAPACITY + 1 {
                        self.tap.publish(&frame(index as u32));
                    }
                }
                reason => {
                    self.tap.publish(&frame(1));
                    self.tap.close(cut(reason));
                    self.tap.publish(&frame(2));
                }
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for CutAfterPayload {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

#[test]
fn recording_overflow_saves_only_the_mux_committed_video_prefix() {
    assert_mux_committed_prefix(RecordingCutReason::QueueOverflow);
}

#[test]
fn recording_interruption_saves_only_the_mux_committed_video_prefix() {
    assert_mux_committed_prefix(RecordingCutReason::Interrupted);
}

#[test]
fn recording_discontinuity_cut_saves_only_the_mux_committed_video_prefix() {
    assert_mux_committed_prefix(RecordingCutReason::Discontinuity);
}

fn assert_mux_committed_prefix(reason: RecordingCutReason) {
    use openh264::encoder::Encoder;
    use openh264::formats::{RgbSliceU8, YUVBuffer};
    use oxideav_core::{NullCodecResolver, ReadSeek, WriteSeek};
    use oxideav_mkv::avc::annexb_to_avcc;

    let tap = Arc::new(RecordingTap::default());
    let receiver = tap.subscribe().unwrap();
    let rgb = vec![64_u8; 32 * 32 * 3];
    let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (32, 32)));
    let encoded = Encoder::new().unwrap().encode(&yuv).unwrap().to_vec();
    let packetized = annexb_to_avcc(&encoded).packetized;
    assert!(!packetized.is_empty());
    let first = EncodedFrame {
        data: encoded.into(),
        ..frame(0)
    };
    tap.publish(&first);
    let directory = directory("committed-prefix");
    let output = directory.join("overflow.mkv");
    let triggered = Arc::new(AtomicBool::new(false));
    let writer_triggered = Arc::clone(&triggered);
    let writer_tap = Arc::clone(&tap);
    let expected_payload = packetized.clone();
    let worker_output = output.clone();
    let (finished, result) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let summary = crate::recording::record_matroska_with(
            &worker_output,
            MediaStreamConfig {
                width: 32,
                height: 32,
                fps: 30,
                ..MediaStreamConfig::default()
            },
            receiver,
            move |path| {
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|error| error.to_string())?;
                Ok(Box::new(CutAfterPayload {
                    file,
                    payload: expected_payload,
                    triggered: writer_triggered,
                    tap: writer_tap,
                    reason,
                }) as Box<dyn WriteSeek>)
            },
            |part, output| {
                crate::recording::publish_manual_recording(part, output)
                    .map_err(|error| error.to_string())
            },
        );
        let _ = finished.send(summary);
    });
    let result = result.recv_timeout(DEADLOCK_TIMEOUT);
    if result.is_err() {
        tap.close(cut(RecordingCutReason::Interrupted));
    }
    worker.join().unwrap();
    let summary = result
        .expect("recorder must finish after the payload-triggered cut without more input")
        .expect("a committed valid packet must survive a recording cut");
    assert!(triggered.load(Ordering::Acquire));
    assert_eq!(summary.completion, cut(reason));
    assert_eq!(summary.media.video_packets, 1);
    assert_eq!(summary.media.audio_packets, 0);
    assert!(!directory.join(".overflow.mkv.part").exists());
    let file: Box<dyn ReadSeek> = Box::new(File::open(&output).unwrap());
    let mut demuxer = oxideav_mkv::demux::open(file, &NullCodecResolver).unwrap();
    assert_eq!(demuxer.streams()[0].params.codec_id.as_str(), "h264");
    let saved = demuxer.next_packet().unwrap();
    assert_eq!(saved.data.as_slice(), packetized);
    assert_eq!(saved.pts, Some(0));
    assert!(demuxer.next_packet().is_err());
    drop(demuxer);
    std::fs::remove_dir_all(directory).unwrap();
}
