use super::*;

fn command(value: Value) -> Command {
    serde_json::from_value(value).expect("command")
}

#[test]
fn recovery_stop_preserves_recording_and_restarts_when_terminal_events_are_dropped() {
    use openh264::encoder::Encoder;
    use openh264::formats::{RgbSliceU8, YUVBuffer};

    for native_already_stopped in [false, true] {
        let (host, runtime) = opennow_streamer_platform::create_test_runtime();
        let (events, received) = std::sync::mpsc::sync_channel(1);
        events.send(json!({"type":"queue-full-fixture"})).unwrap();
        let mut engine =
            Engine::with_embedded_media_runtime(EventSender::bounded(events), runtime.clone());
        let directory = std::env::temp_dir().join(format!(
            "opennow-recording-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let rgb = vec![64_u8; 64 * 64 * 3];
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (64, 64)));
        let data = Encoder::new().unwrap().encode(&yuv).unwrap().to_vec();
        let first_path = directory.join("first.mkv");
        let next_path = directory.join("next.mkv");
        let mut original = Vec::new();
        let (empty, _) = engine.handle(command(
            json!({"id":"empty-stop", "type":"stop", "recovery":true}),
        ));
        assert!(empty[0]["recording"].is_null());
        for (segment, path) in [&first_path, &next_path].into_iter().enumerate() {
            let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
            let session = runtime
                .start(
                    feedback,
                    MediaStreamConfig {
                        width: 64,
                        height: 64,
                        ..MediaStreamConfig::default()
                    },
                )
                .unwrap();
            let sink = session.sink();
            engine.media_session = Some(session);
            lock_lifecycle(&engine.lifecycle).state = State::Connected;
            let request_id = format!("segment-{segment}");
            let (started, _) = engine.handle(command(json!({
                "id":request_id, "type":"recording-start", "outputPath":path
            })));
            assert_eq!(started[0]["type"], "recording-started");
            sink.push(EncodedFrame {
                mid: "video".to_owned(),
                codec: MediaCodec::H264,
                data: Arc::from(data.clone()),
                frame_index: Some(10 + segment as u32),
                timestamp: 90_000 + segment as u64 * 3_000,
                clock_rate_hz: 90_000,
                keyframe: true,
                contiguous: true,
                ssrc: Some(7),
            });
            let partial = path.with_file_name(format!(
                ".{}.part",
                path.file_name().unwrap().to_str().unwrap()
            ));
            let deadline = Instant::now() + Duration::from_secs(5);
            while !partial.exists() {
                assert!(
                    Instant::now() < deadline,
                    "recording did not open its first video frame"
                );
                thread::sleep(Duration::from_millis(1));
            }
            if segment == 0 && native_already_stopped {
                engine.stop_media_resources();
            }
            let (stopped, running) = engine.handle(command(json!({
                "id":format!("transport-stop-{segment}"), "type":"stop", "recovery":segment == 0
            })));
            assert!(running);
            assert_eq!(stopped[0]["type"], "ok");
            let terminal = &stopped[0]["recording"];
            assert_eq!(terminal["requestId"], request_id);
            assert_eq!(terminal["state"], "saved", "{terminal}");
            assert_eq!(terminal["path"], path.to_string_lossy().as_ref());
            assert_eq!(terminal["videoPackets"], 1);
            assert_eq!(
                terminal["completion"],
                if segment == 0 {
                    json!({"kind":"cut", "reason":"interrupted"})
                } else {
                    json!({"kind":"complete"})
                }
            );
            assert!(engine.recording_worker.is_none());
            assert!(path.is_file());
            if segment == 0 {
                original = std::fs::read(path).unwrap();
                assert!(original.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]));
            } else {
                assert_eq!(std::fs::read(&first_path).unwrap(), original);
            }
        }
        assert_eq!(received.try_recv().unwrap()["type"], "queue-full-fixture");
        assert!(received.try_recv().is_err());
        let (repeated, _) = engine.handle(command(
            json!({"id":"repeated-stop", "type":"stop", "recovery":true}),
        ));
        assert_eq!(repeated[0]["recording"]["requestId"], "segment-1");
        assert_eq!(
            repeated[0]["recording"]["completion"],
            json!({"kind":"complete"})
        );
        runtime.shutdown();
        host.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn transport_stop_distinguishes_empty_recordings_and_storage_failures() {
    for (recovery, invalid_storage) in [(false, false), (true, false), (true, true)] {
        let (host, runtime) = opennow_streamer_platform::create_test_runtime();
        let (events, _received) = std::sync::mpsc::channel();
        let mut engine = Engine::with_media_runtime(events, runtime.clone());
        let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
        engine.media_session = Some(
            runtime
                .start(feedback, MediaStreamConfig::default())
                .unwrap(),
        );
        lock_lifecycle(&engine.lifecycle).state = State::Connected;
        let directory = std::env::temp_dir().join(format!(
            "opennow-empty-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = if invalid_storage {
            directory.join("missing/empty.mkv")
        } else {
            directory.join("empty.mkv")
        };
        let (started, _) = engine.handle(command(json!({
            "id":"empty-recording", "type":"recording-start", "outputPath":path
        })));
        assert_eq!(started[0]["type"], "recording-started");
        if invalid_storage {
            engine.stop_media_resources();
        }
        let (stopped, _) = engine.handle(command(json!({
            "id":"transport-stop", "type":"stop", "recovery":recovery
        })));
        let terminal = &stopped[0]["recording"];
        assert_eq!(terminal["state"], "failed");
        assert_eq!(terminal["requestId"], "empty-recording");
        assert!(!terminal["message"].as_str().unwrap().is_empty());
        assert!(terminal["path"].is_null());
        assert_eq!(
            terminal["completion"],
            if invalid_storage {
                Value::Null
            } else if recovery {
                json!({"kind":"cut", "reason":"interrupted"})
            } else {
                json!({"kind":"complete"})
            }
        );
        assert!(!path.exists());
        runtime.shutdown();
        host.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn recording_cut_preserves_a_prefix_and_emits_one_completion() {
    let (host, runtime) = opennow_streamer_platform::create_test_runtime();
    let (events, received) = std::sync::mpsc::channel();
    let mut engine = Engine::with_media_runtime(events, runtime.clone());
    let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
    let session = runtime
        .start(feedback, MediaStreamConfig::default())
        .unwrap();
    let sink = session.sink();
    engine.media_session = Some(session);
    lock_lifecycle(&engine.lifecycle).state = State::Connected;
    let directory = std::env::temp_dir().join(format!(
        "opennow-cut-completion-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let output = directory.join("cut.mkv");
    let (started, _) = engine.handle(command(json!({
        "id":"cut-start", "type":"recording-start", "outputPath":output
    })));
    assert_eq!(started[0]["type"], "recording-started");
    let first_frame = EncodedFrame {
        mid: "video".to_owned(),
        codec: MediaCodec::H264,
        data: Arc::from([
            0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28, 0xde, 0xad, 0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80, 0,
            0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x10,
        ]),
        frame_index: Some(10),
        timestamp: 90_000,
        clock_rate_hz: 90_000,
        keyframe: true,
        contiguous: true,
        ssrc: Some(7),
    };
    sink.push(first_frame.clone());
    sink.push(EncodedFrame {
        mid: "video".to_owned(),
        codec: MediaCodec::H264,
        data: Arc::from([0, 0, 0, 1, 0x41]),
        frame_index: Some(11),
        timestamp: 93_000,
        clock_rate_hz: 90_000,
        keyframe: false,
        contiguous: false,
        ssrc: Some(7),
    });
    let completion = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completion["type"], "recording-state");
    assert_eq!(completion["state"], "saved");
    assert_eq!(completion["completion"]["kind"], "cut");
    assert_eq!(completion["completion"]["reason"], "discontinuity");
    assert_eq!(completion["requestId"], "cut-start");
    assert_eq!(completion["videoPackets"], 1);
    assert!(output.is_file());
    let original = std::fs::read(&output).unwrap();
    let (stopped, _) = engine.handle(command(json!({"id":"first-stop", "type":"recording-stop"})));
    assert_eq!(stopped[0]["type"], "recording-stopped");
    assert_eq!(stopped[0]["requestId"], "cut-start");
    assert_eq!(stopped[0]["completion"], completion["completion"]);
    let (repeated, _) = engine.handle(command(
        json!({"id":"second-stop", "type":"recording-stop"}),
    ));
    assert_eq!(repeated[0]["type"], "recording-not-active");
    assert!(received.try_recv().is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    let next_output = directory.join("next.mkv");
    let (restarted, _) = engine.handle(command(json!({
        "id":"next-start", "type":"recording-start", "outputPath":next_output
    })));
    assert_eq!(restarted[0]["type"], "recording-started");
    sink.push(first_frame);
    engine.stop("test complete");
    let saved = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(saved["state"], "saved");
    assert_eq!(saved["completion"]["kind"], "complete");
    assert_eq!(saved["requestId"], "next-start");
    assert!(next_output.is_file());
    runtime.shutdown();
    host.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recording_can_restart_after_finished_worker_failure() {
    let (host, runtime) = opennow_streamer_platform::create_test_runtime();
    let (events, received) = std::sync::mpsc::channel();
    let mut engine = Engine::with_media_runtime(events, runtime.clone());
    let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
    let session = runtime
        .start(feedback, MediaStreamConfig::default())
        .unwrap();
    let sink = session.sink();
    engine.media_session = Some(session);
    lock_lifecycle(&engine.lifecycle).state = State::Connected;
    let directory = std::env::temp_dir().join(format!(
        "opennow-recording-retry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let (started, _) = engine.handle(command(json!({
        "id":"first", "type":"recording-start", "outputPath":directory.join("first.mkv")
    })));
    assert_eq!(started[0]["type"], "recording-started");
    let (busy, _) = engine.handle(command(json!({
        "id":"busy", "type":"recording-start", "outputPath":directory.join("busy.mkv")
    })));
    assert_eq!(busy[0]["code"], "recording-already-active");
    sink.push(EncodedFrame {
        mid: "video".to_owned(),
        codec: MediaCodec::H264,
        data: Arc::from([0, 0, 0, 1, 0x41]),
        frame_index: Some(2),
        timestamp: 90_000,
        clock_rate_hz: 90_000,
        keyframe: false,
        contiguous: false,
        ssrc: None,
    });
    let completion = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completion["type"], "recording-state");
    assert_eq!(completion["state"], "failed");
    assert_eq!(
        completion["message"],
        "recording stopped because the video stream was discontinuous"
    );
    assert_eq!(completion["completion"]["kind"], "cut");
    assert_eq!(completion["completion"]["reason"], "discontinuity");
    assert!(
        engine
            .recording_worker
            .as_ref()
            .unwrap()
            .completed
            .load(Ordering::Acquire)
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !engine
        .recording_worker
        .as_ref()
        .unwrap()
        .thread
        .is_finished()
    {
        assert!(Instant::now() < deadline, "recording worker did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    let output = directory.join("retry.mkv");
    let (retried, _) = engine.handle(command(json!({
        "id":"retry", "type":"recording-start", "outputPath":output
    })));
    assert_eq!(retried[0]["type"], "recording-started");
    sink.push(EncodedFrame {
        mid: "video".to_owned(),
        codec: MediaCodec::H264,
        data: Arc::from([
            0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28, 0xde, 0xad, 0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80, 0,
            0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x10,
        ]),
        frame_index: Some(3),
        timestamp: 91_500,
        clock_rate_hz: 90_000,
        keyframe: true,
        contiguous: true,
        ssrc: None,
    });
    let (stopped, _) = engine.handle(command(json!({"id":"stop", "type":"recording-stop"})));
    assert_eq!(stopped[0]["type"], "recording-stopped");
    assert_eq!(stopped[0]["videoPackets"], 1);
    let completion = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completion["state"], "saved");
    assert!(output.is_file());
    assert!(engine.recording_worker.is_none());
    engine.stop("test complete");
    runtime.shutdown();
    host.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recording_retry_reaps_terminal_worker_before_thread_exit() {
    let (host, runtime) = opennow_streamer_platform::create_test_runtime();
    let (events, received) = std::sync::mpsc::channel();
    let mut engine = Engine::with_media_runtime(events, runtime.clone());
    let (feedback, _feedback_receiver) = std::sync::mpsc::channel();
    let session = runtime
        .start(feedback, MediaStreamConfig::default())
        .unwrap();
    let (_, receiver) = session.control().subscribe_recording().unwrap();
    engine.media_session = Some(session);
    lock_lifecycle(&engine.lifecycle).state = State::Connected;
    let completed = Arc::new(AtomicBool::new(false));
    let worker_completed = Arc::clone(&completed);
    let events = engine.events.clone();
    engine.recording_worker = Some(RecordingWorker {
        request_id: "terminal-fixture".to_owned(),
        completed,
        thread: thread::spawn(move || {
            worker_completed.store(true, Ordering::Release);
            events
                .send(event("recording-state", json!({"state":"failed"})))
                .unwrap();
            assert!(receiver.recv().is_err());
            Err("terminal recording failure".to_owned().into())
        }),
    });
    let completion = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completion["state"], "failed");
    assert!(
        !engine
            .recording_worker
            .as_ref()
            .unwrap()
            .thread
            .is_finished()
    );
    let path = std::env::temp_dir().join(format!(
        "opennow-recording-terminal-retry-{}.mkv",
        std::process::id()
    ));
    let (retried, _) = engine.handle(command(json!({
        "id":"retry", "type":"recording-start", "outputPath":path
    })));
    let stopped = engine.stop_recording_inner();
    engine.stop("test complete");
    runtime.shutdown();
    host.join().unwrap();
    assert_eq!(retried[0]["type"], "recording-started");
    assert_eq!(
        stopped.unwrap()["message"],
        "recording ended before a decodable video keyframe arrived"
    );
}
