use super::*;

fn command(value: Value) -> Command {
    serde_json::from_value(value).expect("command")
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
        completed,
        thread: thread::spawn(move || {
            worker_completed.store(true, Ordering::Release);
            events
                .send(event("recording-state", json!({"state":"failed"})))
                .unwrap();
            assert!(receiver.recv().is_err());
            Err("terminal recording failure".to_owned())
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
        stopped.unwrap_err(),
        "recording ended before a decodable video keyframe arrived"
    );
}
