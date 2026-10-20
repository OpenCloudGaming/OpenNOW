use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use opennow_streamer_transport::nvst::logical_ice_addr;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceCreds, Input, Output, RtcConfig};

const CONTROL_RELIABLE_LABEL: &str = "control_channel_reliable";
const MAX_STDIN_LINE_BYTES: u64 = 4 * 1024;
const MAX_PENDING_COMMANDS: usize = 32;
const PROBE_RECEIVE_WAIT: Duration = Duration::from_millis(1);
const MAX_COMMANDS_PER_ITERATION: usize = 8;
const INPUT_PROTOCOL_VERSION: u16 = 3;
const INPUT_VERSION_COMMAND_CODE: u16 = 0x020e;

fn input_version_command() -> Vec<u8> {
    let mut command = Vec::with_capacity(6);
    command.extend_from_slice(&INPUT_VERSION_COMMAND_CODE.to_le_bytes());
    command.extend_from_slice(&2_u16.to_le_bytes());
    command.extend_from_slice(&INPUT_PROTOCOL_VERSION.to_le_bytes());
    command
}

struct Options {
    bind_ip: std::net::IpAddr,
    remote_ip: std::net::IpAddr,
    port: u16,
    remote_port: u16,
    local_ufrag: String,
    local_password: String,
    remote_ufrag: String,
    remote_password: String,
    timeout: Duration,
}

fn parse_options() -> Result<Options, String> {
    let mut bind_ip = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let mut remote_ip = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let mut port = None;
    let mut remote_port = None;
    let mut local_ufrag = None;
    let mut local_password = None;
    let mut remote_ufrag = None;
    let mut remote_password = None;
    let mut timeout_ms = 120_000_u64;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("missing {argument}"))
        };
        match argument.as_str() {
            "--bind-ip" => bind_ip = value()?.parse().map_err(|_| "bad bind ip")?,
            "--remote-ip" => remote_ip = value()?.parse().map_err(|_| "bad remote ip")?,
            "--port" => port = Some(value()?.parse::<u16>().map_err(|_| "bad port")?),
            "--remote-port" => {
                remote_port = Some(value()?.parse::<u16>().map_err(|_| "bad remote port")?)
            }
            "--local-ufrag" => local_ufrag = Some(value()?),
            "--local-password" => local_password = Some(value()?),
            "--remote-ufrag" => remote_ufrag = Some(value()?),
            "--remote-password" => remote_password = Some(value()?),
            "--timeout-ms" => timeout_ms = value()?.parse::<u64>().map_err(|_| "bad timeout")?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Options {
        bind_ip,
        remote_ip,
        port: port.ok_or("missing --port")?,
        remote_port: remote_port.ok_or("missing --remote-port")?,
        local_ufrag: local_ufrag.ok_or("missing --local-ufrag")?,
        local_password: local_password.ok_or("missing --local-password")?,
        remote_ufrag: remote_ufrag.ok_or("missing --remote-ufrag")?,
        remote_password: remote_password.ok_or("missing --remote-password")?,
        timeout: Duration::from_millis(timeout_ms),
    })
}

fn fingerprint_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn payload_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

enum PeerCommand {
    Quit,
    Cursor { id: u32 },
    Rumble { low_id: u8, low: u8, high: u8 },
}

fn parse_peer_command(line: &str) -> Option<PeerCommand> {
    let mut parts = line.split_whitespace();
    match parts.next()? {
        "QUIT" => Some(PeerCommand::Quit),
        "CURSOR" => Some(PeerCommand::Cursor {
            id: parts.next()?.parse().ok()?,
        }),
        "RUMBLE" => Some(PeerCommand::Rumble {
            low_id: parts.next()?.parse().ok()?,
            low: parts.next()?.parse().ok()?,
            high: parts.next()?.parse().ok()?,
        }),
        _ => None,
    }
}

fn system_cursor_command(id: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&id.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend_from_slice(&0x010f_u16.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

fn sony_rumble_command(low_id: u8, low: u8, high: u8) -> Vec<u8> {
    let mut payload = Vec::with_capacity(13);
    payload.extend_from_slice(&0x11_u32.to_le_bytes());
    payload.push(low_id);
    payload.push(4);
    payload.push(0);
    payload.extend_from_slice(&[5, 0x01, 0, 0, low, high]);
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend_from_slice(&0x0206_u16.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

fn emit(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

fn watch_stdin(
    commands: std::sync::mpsc::SyncSender<PeerCommand>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut buffer = String::new();
        let mut discarding_line = false;
        loop {
            buffer.clear();
            let read = {
                let mut bounded = stdin.lock().take(MAX_STDIN_LINE_BYTES);
                bounded.read_line(&mut buffer)
            };
            match read {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if !buffer.ends_with('\n') {
                        discarding_line = true;
                        continue;
                    }
                    if discarding_line {
                        discarding_line = false;
                        continue;
                    }
                    match parse_peer_command(buffer.trim()) {
                        Some(PeerCommand::Quit) => break,
                        Some(command) => {
                            if commands.try_send(command).is_err() {
                                continue;
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Release);
    });
}

fn run() -> Result<(), String> {
    let options = parse_options()?;
    let socket = UdpSocket::bind(SocketAddr::new(options.bind_ip, options.port))
        .map_err(|error| error.to_string())?;
    opennow_socket_wait::prepare(&socket, PROBE_RECEIVE_WAIT).map_err(|error| error.to_string())?;
    let physical_local = socket.local_addr().map_err(|error| error.to_string())?;
    if physical_local.ip().is_unspecified() {
        return Err("--bind-ip must name the physical local address".to_owned());
    }
    let physical_remote = SocketAddr::new(options.remote_ip, options.remote_port);
    let local_candidate = logical_ice_addr(physical_local, 2);
    let remote_candidate = logical_ice_addr(physical_remote, 1);

    let mut rtc = RtcConfig::new()
        .set_fingerprint_verification(false)
        .build(Instant::now());
    rtc.direct_api().set_local_ice_credentials(IceCreds {
        ufrag: options.local_ufrag,
        pass: options.local_password,
    });
    rtc.direct_api().set_remote_ice_credentials(IceCreds {
        ufrag: options.remote_ufrag,
        pass: options.remote_password,
    });
    rtc.add_local_candidate(
        Candidate::host(local_candidate, "udp").map_err(|error| error.to_string())?,
    );
    rtc.add_remote_candidate(
        Candidate::host(remote_candidate, "udp").map_err(|error| error.to_string())?,
    );
    rtc.direct_api().set_ice_controlling(false);
    let placeholder = rtc.direct_api().local_dtls_fingerprint().clone();
    rtc.direct_api().set_remote_fingerprint(placeholder);
    rtc.direct_api()
        .start_dtls(false)
        .map_err(|error| error.to_string())?;
    rtc.direct_api().start_sctp(false);
    emit(&format!(
        "READY {}",
        fingerprint_hex(&rtc.direct_api().local_dtls_fingerprint().bytes)
    ));
    emit(&format!(
        "ADDRESSES physicalLocal={physical_local} physicalRemote={physical_remote} logicalLocal={local_candidate} logicalRemote={remote_candidate}"
    ));

    let deadline = Instant::now() + options.timeout;
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (command_sender, command_receiver) = std::sync::mpsc::sync_channel(MAX_PENDING_COMMANDS);
    watch_stdin(command_sender, stop.clone());
    let mut control_channel: Option<str0m::channel::ChannelId> = None;
    let mut buffer = [0_u8; 65_536];
    let mut payloads = 0_u64;
    let mut connected = false;
    let mut reported_error = false;
    let mut dropped_datagrams = 0_u64;
    while Instant::now() < deadline && !stop.load(std::sync::atomic::Ordering::Acquire) {
        let now = Instant::now();
        for _ in 0..MAX_COMMANDS_PER_ITERATION {
            let Ok(command) = command_receiver.try_recv() else {
                break;
            };
            match command {
                PeerCommand::Quit => {}
                PeerCommand::Cursor { id } => {
                    let sent = control_channel.and_then(|id| rtc.channel(id)).is_some_and(
                        |mut channel| {
                            channel
                                .write(true, &system_cursor_command(id))
                                .unwrap_or(false)
                        },
                    );
                    emit(&format!("SENT CURSOR id={id} sent={sent}"));
                }
                PeerCommand::Rumble { low_id, low, high } => {
                    let sent = control_channel.and_then(|id| rtc.channel(id)).is_some_and(
                        |mut channel| {
                            channel
                                .write(true, &sony_rumble_command(low_id, low, high))
                                .unwrap_or(false)
                        },
                    );
                    emit(&format!(
                        "SENT RUMBLE low_id={low_id} low={} high={} sent={sent}",
                        u16::from(low) << 8,
                        u16::from(high) << 8
                    ));
                }
            }
        }
        let _ = rtc.handle_input(Input::Timeout(now));
        loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(_)) => break,
                Ok(Output::Transmit(transmit)) => {
                    let _ = socket.send_to(transmit.contents.as_ref(), physical_remote);
                }
                Ok(Output::Event(event)) => match event {
                    Event::Connected => {
                        if !connected {
                            connected = true;
                            emit("CONNECTED");
                        }
                    }
                    Event::ChannelOpen(id, label) => {
                        if label == CONTROL_RELIABLE_LABEL {
                            control_channel = Some(id);
                            if let Some(mut channel) = rtc.channel(id) {
                                let _ = channel.write(true, &input_version_command());
                            }
                        }
                        emit(&format!("OPEN {id:?} {label}"));
                    }
                    Event::ChannelData(data) => {
                        payloads += 1;
                        emit(&format!("DATA {payloads} {}", payload_hex(&data.data)));
                    }
                    _ => {}
                },
                Err(error) => {
                    if !reported_error {
                        reported_error = true;
                        emit(&format!("ERROR {error}"));
                    }
                    break;
                }
            }
        }
        if let Ok((length, source)) =
            opennow_socket_wait::recv_from(&socket, &mut buffer, PROBE_RECEIVE_WAIT)
        {
            if source != physical_remote {
                continue;
            }
            let contents = match buffer[..length].try_into() {
                Ok(contents) => contents,
                Err(error) => {
                    if dropped_datagrams == 0 {
                        emit(&format!(
                            "DROPPED source={source} length={length} error={error}"
                        ));
                    }
                    dropped_datagrams += 1;
                    continue;
                }
            };
            let receive = Receive {
                proto: Protocol::Udp,
                source: remote_candidate,
                destination: local_candidate,
                contents,
            };
            if let Err(error) = rtc.handle_input(Input::Receive(now, receive)) {
                if !reported_error {
                    reported_error = true;
                    emit(&format!("ERROR {error}"));
                }
            }
        }
    }
    emit(&format!("DONE {payloads}"));
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            emit(&format!("ERROR {error}"));
            ExitCode::FAILURE
        }
    }
}
