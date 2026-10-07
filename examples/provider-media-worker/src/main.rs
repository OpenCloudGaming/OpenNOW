mod control;
mod media;

use control::InputState;
use opennow_media_protocol::wire::{
    ControlMessage, MediaHeader, WorkerBootstrap, read_control, write_control,
};
use opennow_media_protocol::{MAX_BOOTSTRAP_BYTES, MEDIA_PROTOCOL_VERSION};
use opennow_plugin_api::media::{AudioCodec, AudioFormat, MediaLimits};
use opennow_sdk_demo::authorization::{authorize_media, session_is_active};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

fn bootstrap(reader: impl BufRead) -> io::Result<WorkerBootstrap> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BOOTSTRAP_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > MAX_BOOTSTRAP_BYTES {
        return Err(io::Error::other("Bootstrap exceeds limit"));
    }
    let bootstrap: WorkerBootstrap = serde_json::from_slice(&bytes)
        .map_err(|_| io::Error::other("Invalid private bootstrap"))?;
    bootstrap.validate().map_err(io::Error::other)?;
    let audio = AudioFormat {
        codec: AudioCodec::Opus,
        sample_rate: 48000,
        channels: 2,
    };
    if bootstrap.accepted.video != opennow_sdk_demo::fixture_video()
        || bootstrap
            .accepted
            .audio
            .as_ref()
            .is_some_and(|accepted| accepted != &audio)
        || bootstrap.accepted.input.gamepad_slots > 4
        || (bootstrap.accepted.input.rumble && bootstrap.accepted.input.gamepad_slots == 0)
    {
        return Err(io::Error::other("Unsupported accepted media"));
    }
    Ok(bootstrap)
}

fn stdout_file() -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        Ok(File::from(io::stdout().as_fd().try_clone_to_owned()?))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        Ok(File::from(io::stdout().as_handle().try_clone_to_owned()?))
    }
}

fn write_media(
    stdout: &mut File,
    header: MediaHeader,
    payload: &[u8],
    limits: &MediaLimits,
) -> io::Result<()> {
    let encoded = header.encode();
    MediaHeader::decode(&encoded, limits).map_err(io::Error::other)?;
    stdout.write_all(&encoded)?;
    stdout.write_all(payload)
}

fn stream_media(
    bootstrap: &WorkerBootstrap,
    stop: &AtomicBool,
    keyframe: &AtomicBool,
) -> io::Result<()> {
    let mut fixture = media::Fixture::new()?;
    let mut stdout = stdout_file()?;
    let mut index = 0;
    let mut deadline = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let (header, payload) = fixture.video(
            index,
            bootstrap.attempt_generation,
            keyframe.swap(false, Ordering::AcqRel),
        )?;
        write_media(&mut stdout, header, &payload, &bootstrap.limits)?;
        if stop.load(Ordering::Acquire) {
            break;
        }
        if bootstrap.accepted.audio.is_some() {
            let (header, payload) = fixture.audio(index, bootstrap.attempt_generation)?;
            write_media(&mut stdout, header, &payload, &bootstrap.limits)?;
        }
        index += 1;
        deadline += Duration::from_millis(20);
        if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
            thread::sleep(delay);
        } else {
            deadline = Instant::now();
        }
    }
    Ok(())
}

fn run() -> io::Result<()> {
    let bootstrap = bootstrap(io::stdin().lock())?;
    let data_directory = PathBuf::from(
        std::env::var_os("OPENNOW_PLUGIN_DATA_DIR")
            .ok_or_else(|| io::Error::other("Host data directory is missing"))?,
    );
    let session = authorize_media(
        &data_directory,
        bootstrap.provider_bootstrap.expose_secret(),
        &bootstrap.accepted,
    )?;
    if session != bootstrap.binding.session
        || bootstrap.binding.source_id.as_str() != opennow_sdk_demo::PLUGIN_ID
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Media authorization does not match the host binding",
        ));
    }
    let mut socket = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, bootstrap.control_port)),
        Duration::from_secs(5),
    )?;
    socket.set_nodelay(true)?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    let maximum = bootstrap.limits.max_control_message_bytes as usize;
    write_control(
        &mut socket,
        &ControlMessage::Hello {
            version: MEDIA_PROTOCOL_VERSION,
            authentication: bootstrap.authentication.clone(),
            attempt_generation: bootstrap.attempt_generation,
        },
        maximum,
    )?;
    match read_control(&mut socket, maximum)? {
        ControlMessage::Attached { attempt_generation }
            if attempt_generation == bootstrap.attempt_generation => {}
        _ => return Err(io::Error::other("Expected matching host attachment")),
    }
    if !session_is_active(&data_directory, &session)? {
        return Err(io::Error::other("Session ended before attachment"));
    }
    write_control(
        &mut socket,
        &ControlMessage::Ready {
            attempt_generation: bootstrap.attempt_generation,
            input: bootstrap.accepted.input.clone(),
        },
        maximum,
    )?;
    socket.set_read_timeout(None)?;
    let shutdown = socket.try_clone()?;
    let stop = Arc::new(AtomicBool::new(false));
    let keyframe = Arc::new(AtomicBool::new(false));
    let (completed_tx, completed_rx) = mpsc::sync_channel(2);
    let control = {
        let stop = Arc::clone(&stop);
        let keyframe = Arc::clone(&keyframe);
        let completed = completed_tx.clone();
        let attempt = bootstrap.attempt_generation;
        let capabilities = bootstrap.accepted.input.clone();
        thread::spawn(move || {
            let result = (|| {
                let mut input = InputState::default();
                loop {
                    let message = match read_control(&mut socket, maximum) {
                        Ok(message) => message,
                        Err(_) if stop.load(Ordering::Acquire) => break,
                        Err(error) => return Err(error),
                    };
                    let (replies, stopping) =
                        input.apply(message, attempt, &capabilities, &keyframe)?;
                    for reply in replies {
                        write_control(&mut socket, &reply, maximum)?;
                    }
                    if stopping {
                        break;
                    }
                }
                write_control(
                    &mut socket,
                    &ControlMessage::Ended {
                        attempt_generation: attempt,
                    },
                    maximum,
                )
            })();
            let _ = completed.send(result);
            stop.store(true, Ordering::Release);
            let _ = socket.shutdown(Shutdown::Both);
        })
    };
    let media = {
        let stop = Arc::clone(&stop);
        let keyframe = Arc::clone(&keyframe);
        thread::spawn(move || {
            let result = stream_media(&bootstrap, &stop, &keyframe);
            let _ = completed_tx.send(result);
            stop.store(true, Ordering::Release);
        })
    };
    let result = loop {
        match completed_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err(io::Error::other("Worker threads terminated"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                match session_is_active(&data_directory, &session) {
                    Ok(true) => {}
                    Ok(false) => break Ok(()),
                    Err(error) => break Err(error),
                }
            }
        }
    };
    stop.store(true, Ordering::Release);
    let _ = shutdown.shutdown(Shutdown::Read);
    let deadline = Instant::now() + Duration::from_millis(250);
    while !control.is_finished() || !media.is_finished() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        if completed_rx.recv_timeout(remaining).is_err() {
            break;
        }
    }
    if control.is_finished() {
        let _ = control.join();
    }
    if media.is_finished() {
        let _ = media.join();
    }
    result
}

fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("provider-media-worker accepts only the host stdin bootstrap");
        std::process::exit(1);
    }
    if run().is_err() {
        eprintln!("Provider media worker terminated with an error");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::lease::WorkerBinding;
    use opennow_plugin_api::PluginId;
    use opennow_plugin_api::media::{AcceptedMedia, InputCapabilities, VideoEncoding};
    use opennow_plugin_api::provider::{
        AttemptId, OfferId, SecretBytes, SessionId, SessionKey, Text,
    };

    #[test]
    fn bootstrap_accepts_video_only_but_rejects_unimplemented_formats_and_oversize() {
        let mut value = WorkerBootstrap {
            version: MEDIA_PROTOCOL_VERSION,
            binding: WorkerBinding {
                lease_id: Text::new("worker-test-lease").unwrap(),
                source_id: PluginId::new(opennow_sdk_demo::PLUGIN_ID).unwrap(),
                session: SessionKey {
                    account: None,
                    remote_id: SessionId::new("worker-test-session").unwrap(),
                },
                attempt_id: AttemptId::new("worker-test-attempt").unwrap(),
            },
            attempt_generation: 1,
            control_port: 12345,
            authentication: SecretBytes::new(vec![0; 32]).unwrap(),
            accepted: AcceptedMedia {
                offer_id: OfferId::new("worker-test").unwrap(),
                runtime_epoch: 1,
                video: opennow_sdk_demo::fixture_video(),
                audio: None,
                input: InputCapabilities {
                    keyboard: true,
                    relative_mouse: false,
                    absolute_mouse: false,
                    text: false,
                    gamepad_slots: 0,
                    rumble: false,
                },
            },
            limits: MediaLimits {
                max_video_access_unit_bytes: 262144,
                max_audio_packet_bytes: 1275,
                max_buffered_video_bytes: 524288,
                max_buffered_video_frames: 2,
                max_buffered_audio_ms: 100,
                max_control_message_bytes: 4096,
                max_pending_input_events: 32,
            },
            provider_bootstrap: SecretBytes::new(vec![1]).unwrap(),
        };
        assert!(
            bootstrap(serde_json::to_vec(&value).unwrap().as_slice())
                .unwrap()
                .accepted
                .audio
                .is_none()
        );
        value.accepted.video.encoding = VideoEncoding::HevcAnnexB;
        assert!(bootstrap(serde_json::to_vec(&value).unwrap().as_slice()).is_err());
        value.accepted.video = opennow_sdk_demo::fixture_video();
        value.accepted.audio = Some(AudioFormat {
            codec: AudioCodec::Opus,
            sample_rate: 48000,
            channels: 1,
        });
        assert!(bootstrap(serde_json::to_vec(&value).unwrap().as_slice()).is_err());
        assert!(bootstrap(vec![b' '; MAX_BOOTSTRAP_BYTES + 1].as_slice()).is_err());
    }
}
