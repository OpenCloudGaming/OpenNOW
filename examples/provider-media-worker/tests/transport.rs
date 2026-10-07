mod support;

use opennow_media_protocol::lease::WorkerBinding;
use opennow_media_protocol::wire::*;
use opennow_media_protocol::{MEDIA_PROTOCOL_VERSION, SourceStamp};
use opennow_plugin_api::PluginId;
use opennow_plugin_api::provider::{AttemptId, SecretBytes, Text};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use support::Authorization;

const ATTEMPT: u64 = 7;
const MAXIMUM: usize = 4096;

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Worker {
    fn spawn(
        auth: &Authorization,
        mutate: impl FnOnce(&mut WorkerBootstrap),
    ) -> (Self, TcpListener) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut bootstrap = WorkerBootstrap {
            version: MEDIA_PROTOCOL_VERSION,
            binding: WorkerBinding {
                lease_id: Text::new("worker-test-lease").unwrap(),
                source_id: PluginId::new(opennow_sdk_demo::PLUGIN_ID).unwrap(),
                session: auth.session.clone(),
                attempt_id: AttemptId::new("worker-test-attempt").unwrap(),
            },
            attempt_generation: ATTEMPT,
            control_port: listener.local_addr().unwrap().port(),
            authentication: SecretBytes::new(vec![42; 32]).unwrap(),
            accepted: auth.prepared.accepted.clone(),
            limits: auth.offer.limits.clone(),
            provider_bootstrap: auth.prepared.bootstrap.clone(),
        };
        mutate(&mut bootstrap);
        let mut child = Command::new(env!("CARGO_BIN_EXE_provider-media-worker"))
            .env("OPENNOW_PLUGIN_DATA_DIR", auth.directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        serde_json::to_writer(&mut stdin, &bootstrap).unwrap();
        stdin.write_all(b"\n").unwrap();
        (Self(child), listener)
    }

    fn attach(&mut self, listener: TcpListener, attempt: u64) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        self.0.try_wait().unwrap().is_none(),
                        "worker exited before attachment"
                    );
                    assert!(Instant::now() < deadline, "attachment timeout");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        socket.set_nodelay(true).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert!(
            matches!(read_control(&mut socket, MAXIMUM).unwrap(), ControlMessage::Hello { version: MEDIA_PROTOCOL_VERSION, attempt_generation: ATTEMPT, authentication } if authentication.expose_secret() == [42; 32])
        );
        write_control(
            &mut socket,
            &ControlMessage::Attached {
                attempt_generation: attempt,
            },
            MAXIMUM,
        )
        .unwrap();
        socket
    }

    fn start(auth: &Authorization) -> (Self, TcpStream) {
        let (mut worker, listener) = Self::spawn(auth, |_| {});
        let mut socket = worker.attach(listener, ATTEMPT);
        assert!(
            matches!(read_control(&mut socket, MAXIMUM).unwrap(), ControlMessage::Ready { attempt_generation: ATTEMPT, input } if input == auth.prepared.accepted.input)
        );
        (worker, socket)
    }

    fn exit(&mut self, success: bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert_eq!(status.success(), success);
                return;
            }
            assert!(Instant::now() < deadline, "worker failed to exit");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn blocked_stdout(&self) {
        #[cfg(target_os = "linux")]
        {
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                let blocked = std::fs::read_dir(format!("/proc/{}/task", self.0.id()))
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|entry| {
                        std::fs::read_to_string(entry.path().join("wchan"))
                            .unwrap_or_default()
                            .contains("pipe_write")
                    });
                if blocked {
                    return;
                }
                assert!(Instant::now() < deadline, "writer did not block");
                thread::sleep(Duration::from_millis(20));
            }
        }
        #[cfg(not(target_os = "linux"))]
        thread::sleep(Duration::from_secs(6));
    }
}

fn ack(socket: &mut TcpStream, message: ControlMessage, sequence: u64, kind: AckKind) {
    write_control(socket, &message, MAXIMUM).unwrap();
    let response = read_control(socket, MAXIMUM).unwrap();
    assert!(
        matches!(response, ControlMessage::Ack { attempt_generation: ATTEMPT, sequence: received, kind: received_kind } if received == sequence && std::mem::discriminant(&received_kind) == std::mem::discriminant(&kind))
    );
}

fn key(sequence: u64) -> ControlMessage {
    ControlMessage::Input {
        attempt_generation: ATTEMPT,
        sequence,
        captured_us: 1,
        event: InputEvent::Key {
            virtual_key: 65,
            modifiers: 0,
            pressed: true,
        },
    }
}

#[test]
fn authorized_media_preserves_full_width_identity_and_keyframe_feedback() {
    let auth = Authorization::new(60000);
    let (mut worker, mut socket) = Worker::start(&auth);
    let mut stdout = worker.0.stdout.take().unwrap();
    let limits = auth.offer.limits.clone();
    let (tx, rx) = mpsc::sync_channel(4);
    thread::spawn(move || {
        loop {
            let mut bytes = [0; MEDIA_HEADER_BYTES];
            if stdout.read_exact(&mut bytes).is_err() {
                return;
            }
            let header = MediaHeader::decode(&bytes, &limits).unwrap();
            let mut payload = vec![0; header.payload_bytes as usize];
            if stdout.read_exact(&mut payload).is_err() {
                return;
            }
            if tx.send((header, payload)).is_err() {
                return;
            }
        }
    });
    let mut recovery = false;
    for index in 0..60u64 {
        let (video, bytes) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(video.track_id, VIDEO_TRACK_ID);
        assert_eq!(video.attempt_generation, ATTEMPT);
        assert_eq!(
            video.source,
            SourceStamp {
                sender_frame_id: Some(u32::MAX as u64 + 1000 + index),
                timestamp: 90000 + index * 1800,
                clock_rate_hz: 90000,
                ssrc: Some(1234)
            }
        );
        if cfg!(feature = "fault-injection") {
            assert_eq!(video.contiguous, (index + 1) % 50 != 0);
        } else {
            assert!(video.contiguous);
        }
        assert!(bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]));
        if index == 0 {
            assert!(video.keyframe);
        }
        write_control(
            &mut socket,
            &ControlMessage::FrameProgress {
                provenance: video.provenance(),
                stage: FrameStage::Presented,
                local_us: index * 20000,
            },
            MAXIMUM,
        )
        .unwrap();
        if index == 49 {
            assert!(!video.keyframe);
            ack(
                &mut socket,
                ControlMessage::Keyframe {
                    attempt_generation: ATTEMPT,
                    track_id: VIDEO_TRACK_ID,
                },
                0,
                AckKind::Keyframe,
            );
        }
        if (50..58).contains(&index) && video.keyframe {
            recovery = true;
        }
        let (audio, _) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(audio.track_id, AUDIO_TRACK_ID);
        assert_eq!(
            audio.source,
            SourceStamp {
                sender_frame_id: Some(u32::MAX as u64 + 2000 + index),
                timestamp: 48000 + index * 960,
                clock_rate_hz: 48000,
                ssrc: Some(5678)
            }
        );
    }
    assert!(recovery);
    ack(
        &mut socket,
        ControlMessage::Stop {
            attempt_generation: ATTEMPT,
            sequence: 5,
        },
        5,
        AckKind::Stop,
    );
    assert!(matches!(
        read_control(&mut socket, MAXIMUM).unwrap(),
        ControlMessage::Ended {
            attempt_generation: ATTEMPT
        }
    ));
    worker.exit(true);
}

#[test]
fn journal_authorization_rejects_forged_expired_and_mismatched_bootstraps() {
    for case in 0..3 {
        let auth = Authorization::new(60000);
        let (mut worker, listener) = Worker::spawn(&auth, |bootstrap| {
            if case == 2 {
                bootstrap.accepted.video.fps = 49;
                return;
            }
            let mut provider: serde_json::Value =
                serde_json::from_slice(bootstrap.provider_bootstrap.expose_secret()).unwrap();
            if case == 0 {
                provider["token"] = serde_json::json!("0".repeat(64));
            } else {
                provider["expiresAtMs"] = serde_json::json!(1);
            }
            bootstrap.provider_bootstrap =
                SecretBytes::new(serde_json::to_vec(&provider).unwrap()).unwrap();
        });
        worker.exit(false);
        assert!(listener.accept().is_err());
        let mut bytes = Vec::new();
        worker
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.is_empty());
    }
}

#[test]
fn wrong_attachment_attempt_produces_no_media() {
    let auth = Authorization::new(60000);
    let (mut worker, listener) = Worker::spawn(&auth, |_| {});
    let _socket = worker.attach(listener, ATTEMPT + 1);
    worker.exit(false);
    let mut bytes = Vec::new();
    worker
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.is_empty());
}

#[test]
fn valid_journal_grants_cannot_cross_host_session_or_source_bindings() {
    let mut auth = Authorization::new(60000);
    assert_eq!(
        opennow_sdk_demo::authorization::authorize_media(
            auth.directory.path(),
            auth.prepared.bootstrap.expose_secret(),
            &auth.prepared.accepted,
        )
        .unwrap(),
        auth.session,
    );
    let (second_session, second_prepared) = auth.another_session();
    assert_ne!(second_session, auth.session);
    assert_eq!(
        opennow_sdk_demo::authorization::authorize_media(
            auth.directory.path(),
            second_prepared.bootstrap.expose_secret(),
            &second_prepared.accepted,
        )
        .unwrap(),
        second_session,
    );
    for wrong_source in [false, true] {
        let (mut worker, listener) = Worker::spawn(&auth, |bootstrap| {
            bootstrap.provider_bootstrap = second_prepared.bootstrap.clone();
            bootstrap.accepted = second_prepared.accepted.clone();
            if wrong_source {
                bootstrap.binding.session = second_session.clone();
                bootstrap.binding.source_id = PluginId::new("org.opennow.different-demo").unwrap();
            }
        });
        worker.exit(false);
        assert!(listener.accept().is_err());
        let mut media = Vec::new();
        worker
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut media)
            .unwrap();
        assert!(media.is_empty());
    }
}

#[test]
fn active_worker_survives_expiry_rotation_and_control_restart_but_stops_on_terminal_journal() {
    let mut auth = Authorization::new(2000);
    let old_bootstrap = auth.prepared.bootstrap.clone();
    let old_accepted = auth.prepared.accepted.clone();
    let (mut worker, mut socket) = Worker::start(&auth);
    worker.blocked_stdout();
    thread::sleep(Duration::from_millis(2100));
    auth.restart_and_renew();
    assert!(
        opennow_sdk_demo::authorization::authorize_media(
            auth.directory.path(),
            old_bootstrap.expose_secret(),
            &old_accepted
        )
        .is_err()
    );
    ack(&mut socket, key(1), 1, AckKind::Input);
    ack(
        &mut socket,
        ControlMessage::Neutral {
            attempt_generation: ATTEMPT,
            sequence: 2,
        },
        2,
        AckKind::Neutral,
    );
    ack(
        &mut socket,
        ControlMessage::Keyframe {
            attempt_generation: ATTEMPT,
            track_id: VIDEO_TRACK_ID,
        },
        0,
        AckKind::Keyframe,
    );
    auth.stop();
    assert!(matches!(
        read_control(&mut socket, MAXIMUM).unwrap(),
        ControlMessage::Ended {
            attempt_generation: ATTEMPT
        }
    ));
    worker.exit(true);
}

#[test]
fn blocked_media_does_not_block_stop_and_unsupported_input_fails_closed() {
    for invalid in [false, true] {
        let auth = Authorization::new(60000);
        let (mut worker, mut socket) = Worker::start(&auth);
        worker.blocked_stdout();
        ack(&mut socket, key(1), 1, AckKind::Input);
        if invalid {
            write_control(
                &mut socket,
                &ControlMessage::Neutral {
                    attempt_generation: ATTEMPT + 1,
                    sequence: 2,
                },
                MAXIMUM,
            )
            .unwrap();
        } else {
            ack(
                &mut socket,
                ControlMessage::Stop {
                    attempt_generation: ATTEMPT,
                    sequence: 2,
                },
                2,
                AckKind::Stop,
            );
            assert!(matches!(
                read_control(&mut socket, MAXIMUM).unwrap(),
                ControlMessage::Ended {
                    attempt_generation: ATTEMPT
                }
            ));
        }
        worker.exit(!invalid);
    }
}

#[test]
fn optional_sender_and_ssrc_absence_survive_canonical_header() {
    let auth = Authorization::new(60000);
    for sender_frame_id in [None, Some(u32::MAX as u64 + 1234)] {
        let header = MediaHeader {
            attempt_generation: ATTEMPT,
            track_id: VIDEO_TRACK_ID,
            payload_bytes: 4,
            source: SourceStamp {
                sender_frame_id,
                timestamp: u32::MAX as u64 + 90000,
                clock_rate_hz: 90000,
                ssrc: None,
            },
            keyframe: true,
            contiguous: true,
        };
        assert_eq!(
            MediaHeader::decode(&header.encode(), &auth.offer.limits).unwrap(),
            header
        );
    }
}
