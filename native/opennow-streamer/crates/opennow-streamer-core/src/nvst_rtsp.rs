use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use opennow_streamer_platform::MediaStreamConfig;
use opennow_streamer_protocol::SessionContext;
use opennow_streamer_transport::nvst::MAX_NVST_VIDEO_PEER_PORTS;
use opennow_streamer_transport::{ReservedNvstBundle, nvst_video_packet_size};
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{HeaderValue, Uri};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket, connect};

#[path = "nvst_rtsp_color.rs"]
mod color;
#[path = "nvst_rtsp_transport_diagnostics.rs"]
mod transport_diagnostics;
use color::announce_color_lines;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
// A rig whose video streamer is still starting answers SETUP with 200 but no
// Transport peer yet. Re-sweep on a bounded pace then instead of failing in
// under a second; pure rejections still fail immediately. Worst case adds 9s
// inside the shared 20s budget above.
const SETUP_PEER_RETRY_ROUNDS: u32 = 3;
const SETUP_PEER_RETRY_DELAY: Duration = Duration::from_secs(3);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const CONTROL_PING_EXPIRY: Duration = Duration::from_secs(5);
const CONTROL_IO_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_REQUEST_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTROL_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_STREAM_BITRATE_MBPS: u64 = 200;
// GeForce NOW 2.0.87.131 reports video[0].timeoutLengthMs=8000 and
// video[0].sendFrameTimeoutMs=7000. Waiting sixty seconds left a dead Mjolnir media leg on screen
// while audio/control remained alive; use the official receiver timeout so the existing bounded
// transport recovery runs promptly.
pub(crate) const VIDEO_TIMEOUT_MS: u64 = 8_000;
const VIDEO_STARTUP_TIMEOUT_MS: u64 = if cfg!(windows) {
    60_000
} else {
    VIDEO_TIMEOUT_MS
};

#[derive(Debug)]
pub struct NvstRtspError {
    pub code: &'static str,
    pub message: String,
}

impl NvstRtspError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

struct RtspResponse {
    status: u16,
    status_text: String,
    headers: HashMap<String, String>,
    body: String,
}

struct SweepFailure {
    error: NvstRtspError,
    peerless_200: Option<Box<RtspResponse>>,
}

struct RtspClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    cseq: u64,
    buffer: String,
}

struct VideoSetup {
    response: RtspResponse,
    peer: (String, u16, u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum QosVersionOffer {
    #[default]
    Missing,
    Malformed,
    Version(u8),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct VideoQosOffers {
    feedback: QosVersionOffer,
    timings: QosVersionOffer,
    blob_stats: QosVersionOffer,
}

impl VideoQosOffers {
    fn uses_v5_timings(&self) -> bool {
        matches!(self.timings, QosVersionOffer::Version(5..))
            && matches!(self.blob_stats, QosVersionOffer::Version(9..))
    }

    fn validate(&self) -> Result<(), NvstRtspError> {
        for (offer, minimum) in [(self.feedback, 7), (self.timings, 5), (self.blob_stats, 9)] {
            match offer {
                QosVersionOffer::Malformed => {
                    return Err(NvstRtspError::new(
                        "nvst-qos-version-invalid",
                        "Server advertised a malformed QoS version",
                    ));
                }
                QosVersionOffer::Version(version) if version < minimum => {
                    return Err(NvstRtspError::new(
                        "nvst-qos-version-unsupported",
                        "Server requires an older QoS wire format that is not implemented",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_pacing(&self, sdp: &str) -> Result<(), NvstRtspError> {
        if self.uses_v5_timings()
            && [
                ("video[0].framePacing.mode", "2"),
                ("video[0].framePacing.feedbackMode", "0"),
            ]
            .into_iter()
            .any(|(name, expected)| sdp_attribute(sdp, name).is_some_and(|value| value != expected))
        {
            return Err(NvstRtspError::new(
                "nvst-qos-pacing-unsupported",
                "Server pacing mode conflicts with the negotiated QoS timings version",
            ));
        }
        Ok(())
    }

    fn record(&mut self, parameter: &str) {
        let parameter = parameter.trim();
        let parameter = parameter.strip_prefix("a=").unwrap_or(parameter);
        let (name, raw_version) = parameter.split_once(['=', ':']).unwrap_or((parameter, ""));
        let offer = match name.trim() {
            name if name.eq_ignore_ascii_case("nv-video-qos-feedback-version") => {
                &mut self.feedback
            }
            name if name.eq_ignore_ascii_case("nv-video-qos-timings-version") => &mut self.timings,
            name if name.eq_ignore_ascii_case("nv-video-qos-blob-stats-version") => {
                &mut self.blob_stats
            }
            _ => return,
        };
        let raw_version = raw_version.trim();
        *offer = if matches!(offer, QosVersionOffer::Missing)
            && !raw_version.is_empty()
            && raw_version.bytes().all(|byte| byte.is_ascii_digit())
        {
            raw_version
                .parse::<u8>()
                .map_or(QosVersionOffer::Malformed, QosVersionOffer::Version)
        } else {
            QosVersionOffer::Malformed
        };
    }

    fn add_to_handoff(&self, handoff: &mut Value) {
        if let QosVersionOffer::Version(version) = self.feedback {
            handoff["qosFeedbackVersion"] = json!(version);
        }
        if let QosVersionOffer::Version(version) = self.timings {
            handoff["qosTimingsVersion"] = json!(version);
        }
        if let QosVersionOffer::Version(version) = self.blob_stats {
            handoff["qosBlobStatsVersion"] = json!(version);
        }
    }
}

#[derive(Clone, Default)]
struct NvstControlPing {
    sample: Arc<Mutex<Option<(Instant, Duration)>>>,
}

impl NvstControlPing {
    #[cfg(test)]
    fn ping_ms(&self, now: Instant) -> Option<f64> {
        let mut sample = self.sample.lock().ok()?;
        let (received_at, elapsed) = (*sample)?;
        if now.checked_duration_since(received_at)? >= CONTROL_PING_EXPIRY {
            *sample = None;
            return None;
        }
        Some(elapsed.as_secs_f64() * 1000.0)
    }

    fn record(&self, sent_at: Instant, received_at: Instant) {
        if let Ok(mut sample) = self.sample.lock() {
            *sample = received_at
                .checked_duration_since(sent_at)
                .map(|elapsed| (received_at, elapsed));
        }
    }

    fn clear(&self) {
        if let Ok(mut sample) = self.sample.lock() {
            *sample = None;
        }
    }
}

impl RtspClient {
    #[allow(clippy::too_many_arguments)]
    fn setup_video(
        &mut self,
        control: &str,
        target: &str,
        headers: &[(&str, String)],
        client_port: u16,
        bundle_video_peer: Option<(String, u16, u16)>,
    ) -> Result<VideoSetup, NvstRtspError> {
        self.setup_video_with_retry(
            control,
            target,
            headers,
            client_port,
            SETUP_PEER_RETRY_ROUNDS,
            SETUP_PEER_RETRY_DELAY,
            bundle_video_peer,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn setup_video_with_retry(
        &mut self,
        control: &str,
        target: &str,
        headers: &[(&str, String)],
        client_port: u16,
        max_peer_retries: u32,
        peer_retry_delay: Duration,
        bundle_video_peer: Option<(String, u16, u16)>,
    ) -> Result<VideoSetup, NvstRtspError> {
        // A rig whose video streamer is still starting answers SETUP with 200
        // but no Transport peer yet. Re-sweep on a bounded pace then: the
        // official client negotiates through progress callbacks instead of
        // one fast burst. Pure rejections (400/404/459+) mean the forms are
        // wrong for this server, so those still fail immediately.
        let candidates = video_setup_candidates(control, target);
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let mut headers = headers.to_vec();
        headers.push(("Transport", String::new()));
        let transport_index = headers.len() - 1;
        let mut round = 0u32;
        let mut peerless_200_seen = false;
        loop {
            match self.setup_video_sweep(
                &candidates,
                &mut headers,
                transport_index,
                client_port,
                &deadline,
            ) {
                Ok(setup) => return Ok(setup),
                Err(failure) => {
                    if let Some(response) = failure.peerless_200 {
                        peerless_200_seen = true;
                        if let Some(peer) = bundle_video_peer.clone() {
                            opennow_streamer_protocol::log::log_line(
                                "WARN",
                                "rtsps",
                                "video-peer-fallback source=cloudmatch-bundle",
                            );
                            return Ok(VideoSetup {
                                response: *response,
                                peer,
                            });
                        }
                    }
                    let mut error = failure.error;
                    if error.code != "missing-video-peer" {
                        if error.code == "nvst-rtsp-failed" && peerless_200_seen {
                            error.code = "missing-video-peer";
                        } else {
                            return Err(error);
                        }
                    }
                    if round >= max_peer_retries {
                        return Err(error);
                    }
                    round += 1;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(error);
                    }
                    let sleep_for = peer_retry_delay.min(remaining);
                    opennow_streamer_protocol::log::log_line(
                        "INFO",
                        "rtsps",
                        &format!(
                            "video-setup-retry round={round}/{max_peer_retries} sleep_ms={}",
                            sleep_for.as_millis(),
                        ),
                    );
                    std::thread::sleep(sleep_for);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn setup_video_sweep(
        &mut self,
        candidates: &[String],
        headers: &mut [(&str, String)],
        transport_index: usize,
        client_port: u16,
        deadline: &Instant,
    ) -> Result<VideoSetup, SweepFailure> {
        let mut last_status = 0;
        for transport in [
            String::new(),
            format!(
                "unicast;X-GS-ClientPort={client_port}-{}",
                client_port.saturating_add(1)
            ),
        ] {
            let transport_form = if transport.is_empty() {
                "empty"
            } else {
                "client-udp"
            };
            headers[transport_index].1 = transport;
            for (index, candidate) in candidates.iter().enumerate() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(SweepFailure {
                        error: NvstRtspError::new(
                            "nvst-rtsp-timeout",
                            "RTSPS video SETUP timed out",
                        ),
                        peerless_200: None,
                    });
                }
                let response = self
                    .request_with_timeout("SETUP", candidate, headers, "", remaining)
                    .map_err(|error| SweepFailure {
                        error,
                        peerless_200: None,
                    })?;
                let transport = header_value(&response, "transport");
                let peer = transport
                    .and_then(parse_video_peer)
                    .filter(|(ip, _, _)| ip.parse::<IpAddr>().is_ok());
                opennow_streamer_protocol::log::log_line(
                    "INFO",
                    "rtsps",
                    &format!(
                        "video-setup candidate={}/{} transport_form={transport_form} status={} transport_present={} video_peer_valid={} ping_version_present={} ping_payload_present={}",
                        index + 1,
                        candidates.len(),
                        response.status,
                        transport.is_some(),
                        peer.is_some(),
                        header_value(&response, "x-nv-ping").is_some(),
                        header_value(&response, "x-nv-ping-payload").is_some(),
                    ),
                );
                last_status = response.status;
                match response.status {
                    200 => {
                        if let Some(peer) = peer {
                            return Ok(VideoSetup { response, peer });
                        }
                        // 200 without a peer: stop sweeping immediately. Live
                        // alliance rigs accept the first SETUP but poison the
                        // session once further forms are tried (later rounds
                        // degrade to pure 400s and ANNOUNCE is then rejected),
                        // while a single SETUP followed by ANNOUNCE succeeds.
                        // Every observed working session peers on its first
                        // 200, so stopping here changes nothing for them.
                        if let Some(transport) = transport.filter(|t| !t.trim().is_empty()) {
                            opennow_streamer_protocol::log::log_line(
                                "WARN",
                                "rtsps",
                                &format!(
                                    "video-setup-transport candidate={} {}",
                                    index + 1,
                                    transport_diagnostics::summarize(transport),
                                ),
                            );
                        }
                        return Err(SweepFailure {
                            error: NvstRtspError::new(
                                "missing-video-peer",
                                format!(
                                    "SETUP stopped after a peerless 200 without a usable NVST video peer (last status {last_status})",
                                ),
                            ),
                            peerless_200: Some(Box::new(response)),
                        });
                    }
                    400 | 404 | 459 | 460 | 461 => {}
                    _ => {
                        return Err(SweepFailure {
                            error: NvstRtspError::new(
                                "nvst-rtsp-failed",
                                format!("SETUP failed with status {}", response.status),
                            ),
                            peerless_200: None,
                        });
                    }
                }
            }
        }
        Err(SweepFailure {
            error: NvstRtspError::new(
                "nvst-rtsp-failed",
                format!(
                    "SETUP did not return a usable NVST video peer after {} URI forms and 2 Transport forms (last status {last_status})",
                    candidates.len(),
                ),
            ),
            peerless_200: None,
        })
    }

    fn connect(endpoint: &str, session_id: &str) -> Result<(Self, String), NvstRtspError> {
        let (wss, target) = rtsp_endpoint_urls(endpoint)?;
        let mut request = wss
            .into_client_request()
            .map_err(|error| NvstRtspError::new("nvst-connect-failed", error.to_string()))?;
        request.headers_mut().insert(
            "x-nv-sessionid",
            HeaderValue::from_str(session_id).map_err(|_| {
                NvstRtspError::new("invalid-session", "Invalid NVST session identity")
            })?,
        );
        request
            .headers_mut()
            .insert("content-length", HeaderValue::from_static("0"));
        let (mut socket, _) = connect_with_retry(|| connect(request.clone()))?;
        set_io_timeout(&mut socket, REQUEST_TIMEOUT);
        Ok((
            Self {
                socket,
                cseq: 0,
                buffer: String::new(),
            },
            target,
        ))
    }

    fn request(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: &str,
    ) -> Result<RtspResponse, NvstRtspError> {
        self.request_with_timeout(method, uri, headers, body, REQUEST_TIMEOUT)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: &str,
        timeout: Duration,
    ) -> Result<RtspResponse, NvstRtspError> {
        let mut stage = opennow_streamer_protocol::log::Stage::begin("rtsps.request");
        let deadline = Instant::now() + timeout;
        set_io_timeout(&mut self.socket, timeout);
        self.socket.set_config(|config| {
            config.max_message_size = Some(MAX_REQUEST_RESPONSE_BYTES);
            config.max_frame_size = Some(MAX_REQUEST_RESPONSE_BYTES);
        });
        self.send_request(method, uri, headers, body)?;
        opennow_streamer_protocol::log::log_line(
            "INFO",
            "rtsps",
            &format!(
                "request method={method} cseq={} timeout_ms={} body_bytes={}",
                self.cseq,
                timeout.as_millis(),
                body.len()
            ),
        );
        loop {
            if self.buffer.len() > MAX_REQUEST_RESPONSE_BYTES {
                return Err(NvstRtspError::new(
                    "nvst-rtsp-failed",
                    format!(
                        "RTSPS {method} response exceeds request buffer limit: {} bytes (limit {MAX_REQUEST_RESPONSE_BYTES})",
                        self.buffer.len()
                    ),
                ));
            }
            if Instant::now() >= deadline {
                return Err(NvstRtspError::new(
                    "nvst-rtsp-timeout",
                    format!("RTSPS {method} timed out"),
                ));
            }
            let response = match take_rtsp_response(&mut self.buffer, self.cseq) {
                Ok(response) => response,
                Err(error)
                    if method == "TEARDOWN" && error.code == "nvst-rtsp-sequence-mismatch" =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(response) = response {
                opennow_streamer_protocol::log::log_line(
                    "INFO",
                    "rtsps",
                    &format!(
                        "response method={method} cseq={} status={} body_bytes={}",
                        self.cseq,
                        response.status,
                        response.body.len()
                    ),
                );
                stage.complete();
                return Ok(response);
            }
            set_io_timeout(
                &mut self.socket,
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(1)),
            );
            match self.socket.read() {
                Ok(Message::Text(text)) => self.buffer.push_str(text.as_str()),
                Ok(Message::Binary(bytes)) => {
                    self.buffer.push_str(&String::from_utf8_lossy(&bytes))
                }
                Ok(Message::Ping(bytes)) => {
                    let _ = self.socket.send(Message::Pong(bytes));
                }
                Ok(Message::Close(_)) => {
                    return Err(NvstRtspError::new(
                        "nvst-rtsp-failed",
                        "RTSPS control channel closed",
                    ));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) => {
                    return Err(NvstRtspError::new("nvst-rtsp-failed", error.to_string()));
                }
            }
        }
    }

    fn send_request(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, String)],
        body: &str,
    ) -> Result<u64, NvstRtspError> {
        self.cseq += 1;
        let mut request = format!(
            "{method} {uri} RTSP/1.0\r\nCSeq: {}\r\nRequest-Id: {}\r\n",
            self.cseq, self.cseq
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        if !body.is_empty() {
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        request.push_str("\r\n");
        request.push_str(body);
        self.socket
            .send(Message::Text(request.into()))
            .map_err(|error| NvstRtspError::new("nvst-rtsp-failed", error.to_string()))?;
        Ok(self.cseq)
    }
}

fn connect_with_retry<T>(
    mut attempt: impl FnMut() -> Result<T, tungstenite::Error>,
) -> Result<T, NvstRtspError> {
    for retry in 0..3 {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let transient = matches!(&error, tungstenite::Error::Io(_))
                    || matches!(&error, tungstenite::Error::Http(response) if response.status().is_server_error());
                let failure = rtsp_connect_error(&error);
                opennow_streamer_protocol::log::log_line(
                    "WARN",
                    "rtsp",
                    &format!(
                        "signaling-connect-failed attempt={}/3 code={}",
                        retry + 1,
                        failure.code
                    ),
                );
                if !transient || retry == 2 {
                    return Err(failure);
                }
            }
        }
    }
    Err(NvstRtspError::new(
        "nvst-connect-failed",
        "RTSPS connection attempts exhausted",
    ))
}

fn rtsp_connect_error(error: &tungstenite::Error) -> NvstRtspError {
    if let tungstenite::Error::Http(response) = error {
        let status = response.status().as_u16();
        return NvstRtspError::new(
            if status == 403 {
                "nvst-signaling-forbidden"
            } else if status == 503 {
                "nvst-service-unavailable"
            } else {
                "nvst-connect-failed"
            },
            if status == 503 {
                "The GeForce NOW RTSPS service is temporarily unavailable (HTTP 503). The media connection was not established; this is not a video decoder error.".to_owned()
            } else {
                format!("Could not open RTSPS control channel: HTTP {status}")
            },
        );
    }
    NvstRtspError::new(
        "nvst-connect-failed",
        format!("Could not open RTSPS control channel: {error}"),
    )
}

pub struct PreparedNvstRtspSession {
    control_ping: NvstControlPing,
    client: Option<RtspClient>,
    target: String,
    common_headers: Vec<(&'static str, String)>,
    rtsp_session: String,
    disable_play: bool,
    announce_body: String,
    announced: bool,
    owns_session: bool,
    pub handoff: Value,
    pub media_config: MediaStreamConfig,
}

impl PreparedNvstRtspSession {
    pub fn announce(&mut self) -> Result<(), NvstRtspError> {
        if self.announced {
            return Ok(());
        }
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| NvstRtspError::new("nvst-rtsp-failed", "RTSPS client is unavailable"))?;
        let mut announce_headers = self.common_headers.clone();
        announce_headers.push(("Session", self.rtsp_session.clone()));
        announce_headers.push(("Content-Type", "application/sdp".to_owned()));
        let announce = client.request(
            "ANNOUNCE",
            &self.target,
            &announce_headers,
            &self.announce_body,
        )?;
        ensure_rtsp_ok("ANNOUNCE", &announce)?;
        self.announced = true;
        Ok(())
    }

    pub fn finish(mut self) -> Result<ActiveNvstRtspSession, NvstRtspError> {
        self.announce()?;

        if !self.disable_play {
            let mut play_headers = self.common_headers.clone();
            play_headers.push(("Session", self.rtsp_session.clone()));
            let play = self
                .client
                .as_mut()
                .ok_or_else(|| {
                    NvstRtspError::new("nvst-rtsp-failed", "RTSPS client is unavailable")
                })?
                .request("PLAY", &self.target, &play_headers, "")?;
            if play.status != 200 && play.status != 455 {
                return Err(NvstRtspError::new(
                    "nvst-rtsp-failed",
                    format!("PLAY failed: {} {}", play.status, play.status_text),
                ));
            }
        }

        let client = self
            .client
            .take()
            .ok_or_else(|| NvstRtspError::new("nvst-rtsp-failed", "RTSPS client is unavailable"))?;
        self.owns_session = false;
        ActiveNvstRtspSession::spawn(client, self.control_ping.clone())
    }
}

impl Drop for PreparedNvstRtspSession {
    fn drop(&mut self) {
        if !self.owns_session {
            return;
        }
        let Some(client) = self.client.as_mut() else {
            return;
        };
        set_io_timeout(&mut client.socket, CONTROL_IO_TIMEOUT);
        let mut headers = self.common_headers.clone();
        headers.push(("Session", self.rtsp_session.clone()));
        let _ = client.request_with_timeout(
            "TEARDOWN",
            &self.target,
            &headers,
            "",
            Duration::from_secs(1),
        );
        let _ = client.socket.close(None);
    }
}

enum Control {
    Shutdown,
}

pub struct ActiveNvstRtspSession {
    control: Sender<Control>,
    worker: Option<JoinHandle<()>>,
}

impl ActiveNvstRtspSession {
    fn spawn(mut client: RtspClient, control_ping: NvstControlPing) -> Result<Self, NvstRtspError> {
        set_io_timeout(&mut client.socket, CONTROL_IO_TIMEOUT);
        client.socket.set_config(|config| {
            config.max_message_size = Some(MAX_CONTROL_RESPONSE_BYTES);
            config.max_frame_size = Some(MAX_CONTROL_RESPONSE_BYTES);
        });
        let (control, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("opennow-nvst-rtsps".to_owned())
            .spawn(move || {
                let mut last_ping = Instant::now();
                let mut outstanding: Option<(u64, Instant)> = None;
                let mut ping_sequence = 0u64;
                loop {
                    if receiver.try_recv().is_ok() {
                        control_ping.clear();
                        let _ = client.socket.close(None);
                        break;
                    }
                    let now = Instant::now();
                    if outstanding.is_some_and(|(_, sent_at)| {
                        now.duration_since(sent_at) >= KEEPALIVE_INTERVAL
                    }) {
                        outstanding = None;
                        control_ping.clear();
                    }
                    if outstanding.is_none() && now.duration_since(last_ping) >= KEEPALIVE_INTERVAL
                    {
                        ping_sequence = ping_sequence.wrapping_add(1);
                        if client
                            .socket
                            .send(Message::Ping(ping_sequence.to_be_bytes().to_vec().into()))
                            .is_err()
                        {
                            break;
                        }
                        outstanding = Some((ping_sequence, now));
                        last_ping = now;
                    }
                    match client.socket.read() {
                        Ok(Message::Text(text)) => client.buffer.push_str(text.as_str()),
                        Ok(Message::Binary(bytes)) => {
                            client.buffer.push_str(&String::from_utf8_lossy(&bytes));
                        }
                        Ok(Message::Ping(bytes)) => {
                            if client.socket.send(Message::Pong(bytes)).is_err() {
                                break;
                            }
                        }
                        Ok(Message::Pong(bytes)) => {
                            if let Some((sequence, sent_at)) = outstanding {
                                if bytes.as_ref() == sequence.to_be_bytes() {
                                    outstanding = None;
                                    control_ping.record(sent_at, Instant::now());
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(error))
                            if matches!(
                                error.kind(),
                                ErrorKind::WouldBlock | ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                    if client.buffer.len() > MAX_CONTROL_RESPONSE_BYTES {
                        break;
                    }
                    while !client.buffer.is_empty() {
                        match take_rtsp_response(&mut client.buffer, client.cseq) {
                            Ok(Some(_)) => {}
                            Ok(None) => break,
                            Err(error) if error.code == "nvst-rtsp-sequence-mismatch" => {}
                            Err(_) => {
                                control_ping.clear();
                                return;
                            }
                        }
                    }
                }
                control_ping.clear();
            })
            .map_err(|error| NvstRtspError::new("nvst-control-failed", error.to_string()))?;
        Ok(Self {
            control,
            worker: Some(worker),
        })
    }

    pub fn shutdown(&mut self) {
        let _ = self.control.send(Control::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for ActiveNvstRtspSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn prepare_owned_nvst(
    context: &SessionContext,
    bundle: &mut ReservedNvstBundle,
) -> Result<PreparedNvstRtspSession, NvstRtspError> {
    ensure_tls_crypto_provider()?;
    let endpoints = context
        .session
        .extra
        .get("rtspsEndpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(Value::as_str)
        .collect::<Vec<_>>();
    let session_id = context.session.session_id.trim();
    if session_id.is_empty() {
        return Err(NvstRtspError::new(
            "invalid-session",
            "NVST negotiation requires a session ID",
        ));
    }

    try_signaling_endpoints(&endpoints, |endpoint| {
        prepare_on_endpoint(context, bundle, session_id, endpoint)
    })
}

fn try_signaling_endpoints<T>(
    endpoints: &[Option<&str>],
    mut handshake: impl FnMut(&str) -> Result<T, NvstRtspError>,
) -> Result<T, NvstRtspError> {
    if endpoints.is_empty() {
        return Err(NvstRtspError::new(
            "missing-rtsps-endpoint",
            "CloudMatch did not provide an RTSPS endpoint for NVST",
        ));
    }
    let mut failures = Vec::new();
    for (index, endpoint) in endpoints.iter().enumerate() {
        opennow_streamer_protocol::log::log_line(
            "INFO",
            "rtsps",
            &format!(
                "signaling-attempt endpoint={}/{}",
                index + 1,
                endpoints.len()
            ),
        );
        let attempt = endpoint
            .ok_or_else(|| {
                NvstRtspError::new(
                    "invalid-rtsps-endpoint",
                    "RTSPS endpoint is not a URL string",
                )
            })
            .and_then(&mut handshake);
        match attempt {
            Ok(session) => return Ok(session),
            Err(error) => {
                opennow_streamer_protocol::log::log_line(
                    "WARN",
                    "rtsps",
                    &format!(
                        "signaling-failed endpoint={}/{} code={}",
                        index + 1,
                        endpoints.len(),
                        error.code
                    ),
                );
                if error.code == "nvst-signaling-forbidden" {
                    return Err(error);
                }
                failures.push((index + 1, error));
            }
        }
    }
    let summary = failures
        .iter()
        .map(|(index, error)| format!("{index}:{}", error.code))
        .collect::<Vec<_>>()
        .join(", ");
    let (_, last) = failures.pop().ok_or_else(|| {
        NvstRtspError::new(
            "missing-rtsps-endpoint",
            "CloudMatch did not provide an RTSPS endpoint for NVST",
        )
    })?;
    Err(NvstRtspError::new(
        last.code,
        format!(
            "All {} NVST signaling endpoints failed ({}); last: {}",
            endpoints.len(),
            summary,
            last.message,
        ),
    ))
}

fn prepare_on_endpoint(
    context: &SessionContext,
    bundle: &mut ReservedNvstBundle,
    session_id: &str,
    endpoint: &str,
) -> Result<PreparedNvstRtspSession, NvstRtspError> {
    let client_port = bundle
        .local_addr()
        .map_err(|error| NvstRtspError::new("nvst-bind-failed", error.to_string()))?
        .port();
    let mjolnir_port = bundle
        .mjolnir_local_addr()
        .map_err(|error| NvstRtspError::new("nvst-bind-failed", error.to_string()))?
        .port();
    let identity = bundle.identity();

    let mut connect_stage = opennow_streamer_protocol::log::Stage::begin("rtsps.connect");
    let (mut client, target) = RtspClient::connect(endpoint, session_id)?;
    connect_stage.complete();
    drop(connect_stage);
    let host = target
        .strip_prefix("rtsps://")
        .or_else(|| target.strip_prefix("rtsp://"))
        .unwrap_or(&target)
        .to_owned();
    let common_headers = vec![
        ("X-GS-Version", "14.2".to_owned()),
        ("Host", host),
        ("x-nv-sessionid", session_id.to_owned()),
    ];

    let options = client.request("OPTIONS", &target, &common_headers, "")?;
    ensure_rtsp_ok("OPTIONS", &options)?;
    let mut describe_headers = common_headers.clone();
    describe_headers.push(("Accept", "application/sdp".to_owned()));
    describe_headers.push(("x-nv-abtesting", "2".to_owned()));
    let describe = client.request("DESCRIBE", &target, &describe_headers, "")?;
    ensure_rtsp_ok("DESCRIBE", &describe)?;
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "rtsps",
        &format!("describe-shape {}", describe_media_shape(&describe.body),),
    );
    let stream = super::media_stream_config(context);
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "nvst-color",
        &format!(
            "color={} hdr={} announce={:?}",
            stream.color_quality.protocol_name(),
            stream.hdr,
            announce_color_lines(stream),
        ),
    );

    let rtsp_session = header_value(&describe, "session")
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            NvstRtspError::new("nvst-rtsp-failed", "DESCRIBE did not include a session")
        })?
        .to_owned();
    let video_control = media_control(&describe.body, "video").ok_or_else(|| {
        NvstRtspError::new(
            "missing-video-control",
            "DESCRIBE did not include a video control stream",
        )
    })?;
    let video_qos_offers = video_qos_offers(&describe.body);
    video_qos_offers.validate()?;
    video_qos_offers.validate_pacing(&describe.body)?;
    let described_ping_version = sdp_attribute(&describe.body, "general.pingVersion")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(6);
    let remote_ufrag = sdp_attribute(&describe.body, "general.iceUserNameFragmentV2")
        .or_else(|| sdp_attribute(&describe.body, "general.iceUsernameFragment"));
    let remote_password = sdp_attribute(&describe.body, "general.icePasswordV2")
        .or_else(|| sdp_attribute(&describe.body, "general.iceUsernamePwd"));
    let remote_fingerprint = sdp_attribute(&describe.body, "general.dtlsFingerprintV2")
        .or_else(|| sdp_attribute(&describe.body, "general.dtlsFingerprint"));
    let disable_play = sdp_attribute(&describe.body, "general.disablePlay").as_deref() == Some("1");
    let native_bundle = sdp_attribute(&describe.body, "general.nativeRtcOnBundlePort");
    if native_bundle.as_deref() != Some("1") {
        return Err(NvstRtspError::new(
            "nvst-legacy-transport-unsupported",
            "This seat requires the retired multi-socket NVST transport",
        ));
    }
    let rtcp_on_sctp = sdp_attribute(&describe.body, "general.rtcpOnSctp").as_deref() == Some("1");
    let hid_device_mask = sdp_attribute(&describe.body, "ri.hidDeviceMask")
        .as_deref()
        .map(parse_hid_device_mask)
        .unwrap_or(0);
    let microphone_available = negotiate_microphone(context, &describe.body);

    let mut setup_headers = common_headers.clone();
    setup_headers.push(("Session", rtsp_session.clone()));
    setup_headers.push(("x-nv-ping", described_ping_version.to_string()));
    // CloudMatch bundle peer doubles as the video peer when rigs omit
    // Transport (validated here; the substitution itself is logged where it
    // happens inside setup_video).
    let bundle_video_peer = context
        .session
        .media_connection_info
        .as_ref()
        .and_then(|media| {
            let port = u16::try_from(media.port).ok().filter(|port| *port != 0)?;
            media.ip.parse::<IpAddr>().ok()?;
            Some((media.ip.clone(), port, port))
        });
    let VideoSetup {
        response: setup,
        peer: (video_peer_ip, video_peer_port, video_peer_port_end),
    } = client.setup_video(
        &video_control,
        &target,
        &setup_headers,
        mjolnir_port,
        bundle_video_peer,
    )?;
    let (bundle_peer_ip, bundle_peer_port) = context
        .session
        .media_connection_info
        .as_ref()
        .map(|media| (media.ip.as_str(), media.port))
        .unwrap_or((&video_peer_ip, u32::from(video_peer_port)));
    let bundle_peer_port = u16::try_from(bundle_peer_port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            NvstRtspError::new("invalid-media-peer", "NVST media peer port is invalid")
        })?;
    let bundle_peer = bundle_peer_ip
        .parse::<IpAddr>()
        .map(|ip| std::net::SocketAddr::new(ip, bundle_peer_port))
        .map_err(|_| {
            NvstRtspError::new("invalid-media-peer", "NVST media peer is not an IP address")
        })?;
    let local_address = bundle
        .advertised_local_address_for(bundle_peer)
        .map_err(|error| {
            NvstRtspError::new(
                "nvst-bind-failed",
                format!("Could not select the local route to the NVST media peer: {error}"),
            )
        })?;
    let video_peer_ip_parsed = video_peer_ip.parse().map_err(|_| {
        NvstRtspError::new("invalid-media-peer", "NVST video peer is not an IP address")
    })?;
    let video_packet_size = nvst_video_packet_size(video_peer_ip_parsed)
        .map_err(|error| NvstRtspError::new("nvst-video-mtu-invalid", error.to_string()))?;
    let video_packet_size = measured_path_packet_size(context, video_peer_ip_parsed)
        .map_or(video_packet_size, |measured| {
            video_packet_size.min(measured)
        });
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "transport",
        &format!(
            "NVST media route local={local_address} bundlePort={client_port} videoPort={mjolnir_port} bundlePeer={bundle_peer} videoPeer={video_peer_ip}:{video_peer_port}"
        ),
    );
    let setup_ping_payload = header_value(&setup, "x-nv-ping-payload").map(ToOwned::to_owned);
    let ping_version = header_value(&setup, "x-nv-ping")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(described_ping_version);
    if ping_version == 6 && setup_ping_payload.is_none() {
        return Err(NvstRtspError::new(
            "missing-ice-credentials",
            "SETUP selected ping version 6 without an X-Nv-Ping-Payload",
        ));
    }
    let remote_ufrag = resolve_remote_ufrag(
        setup_ping_payload.as_deref(),
        remote_ufrag.as_deref(),
        ping_version,
    )
    .ok_or_else(|| {
        NvstRtspError::new(
            "missing-ice-credentials",
            "NVST negotiation did not provide a remote ICE username fragment",
        )
    })?;
    let ping_payload = setup_ping_payload.unwrap_or_else(|| "PING".to_owned());
    let remote_password = remote_password.ok_or_else(|| {
        NvstRtspError::new(
            "missing-ice-credentials",
            "DESCRIBE did not return NVST ICE credentials",
        )
    })?;
    let (key, key_id) = match runtime_key(&describe.body) {
        Some(value) => value,
        None => random_runtime_key()?,
    };
    let salt = format!("{key_id:024X}");
    let codec = negotiated_codec(context);
    let srtp_profile =
        advertised_srtp_profile(&setup, &describe.body).unwrap_or("AEAD_AES_256_GCM_8");

    let mut handoff = json!({
        "clientUdpPort":client_port,
        "packetSize":video_packet_size,
        "mjolnirUdpPort":mjolnir_port,
        "videoPeerIp":video_peer_ip,
        "videoPeerPort":video_peer_port,
        "videoPeerPortEnd":video_peer_port_end,
        "srtpAesKeyHex":key,
        "srtpKeyId":key_id,
        "srtpSaltHex":salt,
        "srtpProfile":srtp_profile,
        "pingPayload":ping_payload,
        "pingVersion":ping_version,
        "localIceUsernameFragment":identity.ice_username_fragment,
        "localIcePassword":identity.ice_password,
        "remoteIceUsernameFragment":remote_ufrag,
        "remoteIcePassword":remote_password,
        "localDtlsFingerprint":identity.dtls_fingerprint,
        "remoteDtlsFingerprint":remote_fingerprint,
        "rtcpOnSctp":rtcp_on_sctp,
        "hidDeviceMask":hid_device_mask,
        "microphoneOnBundle":microphone_available,
        "codec":codec,
        "audioTrack":{"payloadType":111,"codec":"opus","clockRateHz":48000,"channels":2,"mid":"0"},
        "timeoutMs":VIDEO_TIMEOUT_MS,
        "startupTimeoutMs":VIDEO_STARTUP_TIMEOUT_MS
    });
    video_qos_offers.add_to_handoff(&mut handoff);
    set_bundle_natt_username(&mut handoff, &describe.body, ping_version);
    if let Some(media) = context.session.media_connection_info.as_ref() {
        handoff["bundlePeerIp"] = json!(media.ip);
        handoff["bundlePeerPort"] = json!(media.port);
    }

    // Only log transport shape, never SDP, runtime keys, or ICE credentials.
    opennow_streamer_protocol::log::log_line(
        "INFO",
        "nvst-handoff",
        &format!(
            "video_local_port={mjolnir_port} bundle_local_port={client_port} video_peer_port={video_peer_port} video_peer_port_end={video_peer_port_end} bundle_peer_port={} same_peer_host={} ping_version={ping_version} ping_bytes={} legacy_ping_payload={} srtp_profile={srtp_profile} rtcp_on_sctp={rtcp_on_sctp} sockets_retained=true reachability=unverified video_startup_timeout_ms={VIDEO_STARTUP_TIMEOUT_MS} video_idle_timeout_ms={VIDEO_TIMEOUT_MS}",
            context
                .session
                .media_connection_info
                .as_ref()
                .map_or(u32::from(video_peer_port), |media| media.port),
            context
                .session
                .media_connection_info
                .as_ref()
                .is_none_or(|media| media.ip == video_peer_ip),
            ping_payload.len(),
            ping_payload == "PING"
        ),
    );

    let announce_body = omit_server_announce_attributes(
        &build_announce(
            context,
            AnnounceParams {
                stream,
                key: handoff["srtpAesKeyHex"].as_str().unwrap_or_default(),
                key_id,
                port: client_port,
                address: &local_address,
                ufrag: handoff["localIceUsernameFragment"]
                    .as_str()
                    .unwrap_or_default(),
                password: handoff["localIcePassword"].as_str().unwrap_or_default(),
                fingerprint: handoff["localDtlsFingerprint"].as_str().unwrap_or_default(),
                video_port: video_peer_port,
                video_packet_size,
                rtcp_on_sctp,
                microphone_available,
                qos_timings_v5: video_qos_offers.uses_v5_timings(),
            },
        ),
        &describe.body,
    );
    Ok(PreparedNvstRtspSession {
        control_ping: NvstControlPing::default(),
        client: Some(client),
        target,
        common_headers,
        rtsp_session,
        disable_play,
        announce_body,
        announced: false,
        owns_session: true,
        handoff,
        media_config: stream,
    })
}

fn ensure_tls_crypto_provider() -> Result<(), NvstRtspError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        return Err(NvstRtspError::new(
            "tls-provider-unavailable",
            "Could not initialize the TLS crypto provider",
        ));
    }
    Ok(())
}

struct AnnounceParams<'a> {
    stream: MediaStreamConfig,
    key: &'a str,
    key_id: u32,
    port: u16,
    address: &'a str,
    ufrag: &'a str,
    password: &'a str,
    fingerprint: &'a str,
    video_port: u16,
    video_packet_size: usize,
    rtcp_on_sctp: bool,
    microphone_available: bool,
    qos_timings_v5: bool,
}

fn negotiate_microphone(context: &SessionContext, describe: &str) -> bool {
    context
        .settings
        .get("microphoneMode")
        .and_then(Value::as_str)
        == Some("voice-activity")
        && sdp_attribute(describe, "general.rtcMicOnNativeBundle").as_deref() == Some("1")
}

fn advertised_bitrate_kbps(settings: &Value) -> u64 {
    let mbps = settings
        .get("maxBitrateMbps")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(75.0)
        .clamp(0.22, MAX_STREAM_BITRATE_MBPS as f64);
    (mbps * 1000.0).round() as u64
}

fn omit_server_announce_attributes(announce: &str, describe: &str) -> String {
    const SERVER_NEGOTIATED: &[&str] = &[
        "video[0].updateSplitEncodeStateDynamically",
        "video[0].enableRtpNack",
        "video[0].rtpNackQueueLength",
        "video[0].rtpNackQueueMaxPackets",
        "video[0].rtpNackMaxPacketCount",
        "video[0].framePacing.mode",
        "video[0].framePacing.feedbackMode",
        "video[0].initialBitrateKbps",
        "video[0].initialPeakBitrateKbps",
        "video[0].mapRtpTimestampsToFrames",
        "vqos[0].fec.enable",
        "vqos[0].fec.rateDropWindow",
        "vqos[0].fec.minRequiredFecPackets",
        "vqos[0].fec.repairPercent",
        "vqos[0].fec.repairMinPercent",
        "vqos[0].fec.repairMaxPercent",
        "vqos[0].bllFec.enable",
        "vqos[0].drc.enable",
        "vqos[0].dfc.adjustResAndFps",
        "vqos[0].calculateAvgVideoStreamingBitrate",
        "vqos[0].bw.minimumBitrateKbps",
        "vqos[0].drc.bitrateIirFilterFactor",
        "vqos[0].resControl.bitrateIirFilterFactor",
        "packetPacing.version",
        "packetPacing.mode",
        "packetPacing.numGroups",
        "packetPacing.maxDelayUs",
        "packetPacing.minNumPacketsFrame",
        "packetPacing.minNumPacketsPerGroup",
        "packetPacing.enableAccurateSleep",
        "packetPacing.enableSmoothTransition",
        "packetPacing.allowFpsBasedToggle",
        "ri.partialReliableThresholdMs",
        "ri.timestampsEnabled",
        "ri.useMultipleGamepads",
        "ri.usePartiallyReliableUdpChannel",
        "ri.enablePartiallyReliableTransferGamepad",
        "ri.enablePartiallyReliableTransferHid",
        "bwe.useOwdCongestionControl",
        "general.rtspWebSocketPerConnection",
        "general.pingIntervalBeforeConnectionMs",
        "general.pingIntervalAfterConnectionMs",
        "runtime.audioSrtp",
        "runtime.micSrtp",
        "runtime.videoSrtp",
        "general.nativeRtcOnBundlePort",
        "general.rtcVideoOnNativeBundle",
        "general.rtcAudioOnNativeBundle",
        "general.rtcDataChannelOnNativeBundle",
        "general.enableUnifiedSocket",
        "general.rtcpOnSctp",
    ];
    let server_values: HashMap<String, &str> = describe
        .split("||")
        .next()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim().strip_prefix("a=")?.split_once(':')?;
            let name = name.strip_prefix("x-nv-").unwrap_or(name);
            let value = value.trim();
            (!value.is_empty()).then(|| (name.to_ascii_lowercase(), value))
        })
        .collect();
    let mut filtered = String::with_capacity(announce.len());
    for line in announce.split_inclusive("\r\n") {
        let Some((name, value)) = line
            .strip_prefix("a=x-nv-")
            .and_then(|attribute| attribute.split_once(':'))
        else {
            filtered.push_str(line);
            continue;
        };
        let server_value = server_values.get(&name.to_ascii_lowercase());
        if name == "vqos[0].bw.maximumBitrateKbps"
            && let Some(server_value) = server_value
        {
            if let (Ok(requested), Ok(limit)) =
                (value.trim().parse::<u64>(), server_value.parse::<u64>())
                && limit > 0
            {
                filtered.push_str(&format!("a=x-nv-{name}:{}\r\n", requested.min(limit)));
            } else {
                filtered.push_str(line);
            }
            continue;
        }
        if SERVER_NEGOTIATED.contains(&name) && server_value.is_some() {
            continue;
        }
        filtered.push_str(line);
    }
    filtered
}

fn build_announce(context: &SessionContext, params: AnnounceParams<'_>) -> String {
    let (width, height) = resolution(context);
    let fps = negotiated_fps(context);
    let bitrate = advertised_bitrate_kbps(&context.settings);
    let minimum_bitrate = bitrate.min(1_000);
    let codec = negotiated_codec(context);
    let format = if codec.eq_ignore_ascii_case("AV1") {
        2
    } else if codec.eq_ignore_ascii_case("H265") || codec.eq_ignore_ascii_case("HEVC") {
        1
    } else {
        0
    };
    let dynamic_streaming_mode = negotiated_dynamic_streaming_mode(context);
    let adjust_res_and_fps = negotiated_adjustment_enabled(dynamic_streaming_mode);
    let mut lines = vec![
        "v=0".to_owned(),
        "o=unknown 0 14 IN IPv4 127.0.0.1".to_owned(),
        "s=NVIDIA Streaming Client".to_owned(),
        format!("a=x-nv-video[0].clientViewportWd:{width}"),
        format!("a=x-nv-video[0].clientViewportHt:{height}"),
        "a=x-nv-video[0].maxCodecProfile:3".to_owned(),
        "a=x-nv-video[0].maxCodecLevel:51".to_owned(),
        "a=x-nv-video[0].maxH264Profile:3".to_owned(),
        "a=x-nv-video[0].maxH264Level:51".to_owned(),
        "a=x-nv-video[0].updateSplitEncodeStateDynamically:1".to_owned(),
        format!("a=x-nv-video[0].packetSize:{}", params.video_packet_size),
        "a=x-nv-video[0].enableRtpNack:1".to_owned(),
        "a=x-nv-video[0].rtpNackQueueLength:2048".to_owned(),
        "a=x-nv-video[0].rtpNackQueueMaxPackets:1024".to_owned(),
        "a=x-nv-video[0].rtpNackMaxPacketCount:64".to_owned(),
        format!(
            "a=x-nv-video[0].framePacing.mode:{}",
            if params.qos_timings_v5 { 2 } else { 1 }
        ),
        format!(
            "a=x-nv-video[0].framePacing.feedbackMode:{}",
            if params.qos_timings_v5 { 0 } else { 1 }
        ),
        "a=x-nv-video[0].maxNumReferenceFrames:0".to_owned(),
        "a=x-nv-video[0].prefilterParams.prefilterMode:0".to_owned(),
        "a=x-nv-video[0].prefilterParams.prefilterModel:4".to_owned(),
        "a=x-nv-video[0].prefilterParams.denoiseLevel:0".to_owned(),
        "a=x-nv-video[0].prefilterParams.sharpnessLevel:0".to_owned(),
        "a=x-nv-video[0].encoderCscMode:2".to_owned(),
        "a=x-nv-video[0].encoderHdrCscMode:4".to_owned(),
        "a=x-nv-video[0].mapRtpTimestampsToFrames:0".to_owned(),
        format!("a=x-nv-video[0].maxFPS:{fps}"),
        format!("a=x-nv-video[0].initialBitrateKbps:{bitrate}"),
        format!("a=x-nv-video[0].initialPeakBitrateKbps:{bitrate}"),
        format!("a=x-nv-vqos[0].bitStreamFormat:{format}"),
        "a=x-nv-vqos[0].fec.enable:1".to_owned(),
        "a=x-nv-vqos[0].fec.rateDropWindow:10".to_owned(),
        "a=x-nv-vqos[0].fec.minRequiredFecPackets:2".to_owned(),
        "a=x-nv-vqos[0].fec.repairPercent:20".to_owned(),
        "a=x-nv-vqos[0].fec.repairMinPercent:20".to_owned(),
        "a=x-nv-vqos[0].fec.repairMaxPercent:35".to_owned(),
        "a=x-nv-vqos[0].bllFec.enable:0".to_owned(),
        "a=x-nv-vqos[0].drc.enable:0".to_owned(),
        format!("a=x-nv-vqos[0].dfc.adjustResAndFps:{adjust_res_and_fps}"),
        "a=x-nv-vqos[0].calculateAvgVideoStreamingBitrate:1".to_owned(),
        format!("a=x-nv-vqos[0].bw.maximumBitrateKbps:{bitrate}"),
        format!("a=x-nv-vqos[0].bw.minimumBitrateKbps:{minimum_bitrate}"),
        "a=x-nv-vqos[0].drc.bitrateIirFilterFactor:128".to_owned(),
        "a=x-nv-vqos[0].resControl.bitrateIirFilterFactor:128".to_owned(),
        format!("a=x-nv-vqos[0].dynamicStreamingMode:{dynamic_streaming_mode}"),
        "a=x-nv-packetPacing.version:3".to_owned(),
        "a=x-nv-packetPacing.mode:1".to_owned(),
        "a=x-nv-packetPacing.numGroups:5".to_owned(),
        format!(
            "a=x-nv-packetPacing.maxDelayUs:{}",
            if fps >= 100 { 4000 } else { 2000 }
        ),
        "a=x-nv-packetPacing.minNumPacketsFrame:10".to_owned(),
        "a=x-nv-packetPacing.minNumPacketsPerGroup:15".to_owned(),
        "a=x-nv-packetPacing.enableAccurateSleep:1".to_owned(),
        "a=x-nv-packetPacing.enableSmoothTransition:1".to_owned(),
        "a=x-nv-packetPacing.allowFpsBasedToggle:1".to_owned(),
        "a=x-nv-ri.partialReliableThresholdMs:300".to_owned(),
        "a=x-nv-ri.timestampsEnabled:1".to_owned(),
        "a=x-nv-ri.useMultipleGamepads:1".to_owned(),
        "a=x-nv-ri.usePartiallyReliableUdpChannel:0".to_owned(),
        "a=x-nv-ri.enablePartiallyReliableTransferGamepad:255".to_owned(),
        "a=x-nv-ri.enablePartiallyReliableTransferHid:-1".to_owned(),
        "a=x-nv-bwe.useOwdCongestionControl:1".to_owned(),
        "a=x-nv-general.rtspWebSocketPerConnection:1".to_owned(),
        "a=x-nv-general.pingIntervalBeforeConnectionMs:20".to_owned(),
        "a=x-nv-general.pingIntervalAfterConnectionMs:100".to_owned(),
        "a=x-nv-runtime.audioSrtp:0".to_owned(),
        "a=x-nv-runtime.micSrtp:0".to_owned(),
        "a=x-nv-runtime.mouseCursorCapture:3".to_owned(),
        "a=x-nv-runtime.mimicRemoteCursor:0".to_owned(),
        "a=x-nv-runtime.videoSrtp:1".to_owned(),
        format!("a=x-nv-runtime.encryptionKey:{}", params.key),
        format!("a=x-nv-runtime.encryptionKeyId:{}", params.key_id),
        "a=x-nv-general.clientPorts.video:0".to_owned(),
        "a=x-nv-general.clientPorts.audio:0".to_owned(),
        "a=x-nv-general.clientPorts.mic:0".to_owned(),
        "a=x-nv-general.clientPorts.control:0".to_owned(),
        "a=x-nv-general.clientPorts.bundle:0".to_owned(),
        "a=x-nv-general.clientPorts.session:0".to_owned(),
        format!("a=x-nv-general.clientPorts.localAddress:{}", params.address),
        "a=x-nv-general.clientPorts.useReserved:1".to_owned(),
        "a=x-nv-general.clientPorts.fallbackDynamic:1".to_owned(),
        format!("a=x-nv-general.clientBundlePort:{}", params.port),
        "a=x-nv-general.nativeRtcOnBundlePort:1".to_owned(),
        "a=x-nv-general.rtcVideoOnNativeBundle:0".to_owned(),
        "a=x-nv-general.rtcAudioOnNativeBundle:1".to_owned(),
        "a=x-nv-general.rtcDataChannelOnNativeBundle:1".to_owned(),
        "a=x-nv-general.enableUnifiedSocket:0".to_owned(),
        format!(
            "a=x-nv-general.rtcpOnSctp:{}",
            u8::from(params.rtcp_on_sctp)
        ),
        format!("a=x-nv-general.iceUserNameFragmentV2:{}", params.ufrag),
        format!("a=x-nv-general.icePasswordV2:{}", params.password),
        format!("a=x-nv-general.dtlsFingerprintV2:{}", params.fingerprint),
        "a=ice-options:trickle".to_owned(),
        format!("a=ice-ufrag:{}", params.ufrag),
        format!("a=ice-pwd:{}", params.password),
        format!("a=fingerprint:sha-256 {}", params.fingerprint),
        "a=setup:actpass".to_owned(),
        format!(
            "a=candidate:1 1 udp 2122260223 {} {} typ host",
            params.address, params.port
        ),
    ];
    if params.microphone_available {
        lines.push("a=x-nv-general.rtcMicOnNativeBundle:1".to_owned());
        lines.push("a=x-nv-mic.micSsrcConfig.senderSsrc:1".to_owned());
    }
    lines.extend(announce_color_lines(params.stream));
    lines.extend([
        "t=0 0".to_owned(),
        format!("m=video {}", params.video_port),
        "c=IN IP4 0.0.0.0".to_owned(),
        "i=DeviceString, DeviceName".to_owned(),
        String::new(),
    ]);
    lines.join("\r\n")
}

fn resolution(context: &SessionContext) -> (u64, u64) {
    let value = context
        .session
        .extra
        .get("negotiatedStreamProfile")
        .and_then(|profile| profile.get("resolution"))
        .and_then(Value::as_str)
        .or_else(|| context.settings.get("resolution").and_then(Value::as_str))
        .unwrap_or("1920x1080");
    value
        .split_once(['x', 'X'])
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .unwrap_or((1920, 1080))
}

fn negotiated_fps(context: &SessionContext) -> u64 {
    context
        .session
        .extra
        .get("negotiatedStreamProfile")
        .and_then(|profile| profile.get("fps"))
        .and_then(Value::as_u64)
        .or_else(|| context.settings.get("fps").and_then(Value::as_u64))
        .unwrap_or(60)
        .clamp(30, u64::from(super::MAX_STREAM_FPS))
}

fn measured_path_packet_size(context: &SessionContext, peer: IpAddr) -> Option<usize> {
    let datagram = context
        .session
        .extra
        .get("networkTest")?
        .get("measuredDatagramBytes")?
        .as_u64()?;
    opennow_streamer_transport::measured_video_packet_size(usize::try_from(datagram).ok()?, peer)
}

fn negotiated_codec(context: &SessionContext) -> String {
    context
        .session
        .extra
        .get("negotiatedStreamProfile")
        .and_then(|profile| profile.get("codec"))
        .and_then(Value::as_str)
        .or_else(|| context.settings.get("codec").and_then(Value::as_str))
        .unwrap_or("H264")
        .to_ascii_uppercase()
}

fn negotiated_dynamic_streaming_mode(context: &SessionContext) -> u8 {
    // The dynamic quality policy is RTSP-only in the official client: it never
    // travels in CloudMatch requestedStreamingFeatures, so a negotiated value can
    // only come from a server-finalized override. Otherwise the live client
    // setting decides, matching the official Data Saver behavior at ANNOUNCE time.
    if let Some(policy) = context
        .session
        .extra
        .get("negotiatedStreamProfile")
        .and_then(|profile| profile.get("dynamicStreamingMode"))
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| *value <= 3)
    {
        return policy;
    }
    u8::from(
        context
            .settings
            .get("saveBandwidth")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

fn negotiated_adjustment_enabled(policy: u8) -> u8 {
    u8::from(policy != 0)
}

fn advertised_srtp_profile<'a>(response: &'a RtspResponse, sdp: &'a str) -> Option<&'a str> {
    const PROFILES: [&str; 8] = [
        "AEAD_AES_128_GCM_8",
        "AEAD_AES_256_GCM_8",
        "AEAD_AES_128_GCM",
        "AEAD_AES_256_GCM",
        "AES_CM_128_HMAC_SHA1_32",
        "AES_CM_128_HMAC_SHA1_80",
        "AES_CM_256_HMAC_SHA1_32",
        "AES_CM_256_HMAC_SHA1_80",
    ];
    response
        .headers
        .iter()
        .filter(|(name, _)| {
            name.eq_ignore_ascii_case("transport")
                || name.to_ascii_lowercase().contains("srtp")
                || name.to_ascii_lowercase().contains("crypto")
        })
        .map(|(_, value)| value.as_str())
        .chain(sdp.lines())
        .find_map(|value| {
            let upper = value.to_ascii_uppercase();
            PROFILES.into_iter().find(|profile| {
                upper
                    .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                    .any(|token| token == *profile)
            })
        })
}

fn ensure_rtsp_ok(step: &str, response: &RtspResponse) -> Result<(), NvstRtspError> {
    if response.status == 200 {
        Ok(())
    } else {
        let failure = NvstRtspError::new(
            "nvst-rtsp-failed",
            format!(
                "{step} failed: {} {}",
                response.status, response.status_text
            ),
        );
        opennow_streamer_protocol::log::log_line("WARN", "rtsp", &failure.message);
        Err(failure)
    }
}

fn header_value<'a>(response: &'a RtspResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .get(&name.to_ascii_lowercase())
        .map(String::as_str)
}

fn take_rtsp_response(
    buffer: &mut String,
    expected_cseq: u64,
) -> Result<Option<RtspResponse>, NvstRtspError> {
    // Some Bifrost seats put an extra blank CRLF block between WebSocket-carried
    // RTSP responses. It is transport padding, not an empty RTSP response. Drop
    // only leading line separators while waiting for the next status line.
    let status_start = buffer
        .find(|character: char| character != '\r' && character != '\n')
        .unwrap_or(buffer.len());
    if status_start > 0 {
        buffer.drain(..status_start);
    }
    let Some(header_end) = buffer.find("\r\n\r\n").or_else(|| buffer.find("\n\n")) else {
        return Ok(None);
    };
    let separator = if buffer[header_end..].starts_with("\r\n\r\n") {
        4
    } else {
        2
    };
    let header_text = &buffer[..header_end];
    let content_length = header_text
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    let total = (header_end + separator)
        .checked_add(content_length)
        .ok_or_else(|| NvstRtspError::new("nvst-rtsp-failed", "Invalid RTSPS content length"))?;
    if total > MAX_REQUEST_RESPONSE_BYTES {
        return Err(NvstRtspError::new(
            "nvst-rtsp-failed",
            format!(
                "RTSPS response exceeds request buffer limit: {total} bytes (limit {MAX_REQUEST_RESPONSE_BYTES})"
            ),
        ));
    }
    if buffer.len() < total {
        return Ok(None);
    }
    if !buffer.is_char_boundary(total) {
        return Err(NvstRtspError::new(
            "nvst-rtsp-failed",
            "Invalid RTSPS body encoding",
        ));
    }
    let raw = buffer[..total].to_owned();
    buffer.drain(..total);
    let (head, body) = raw.split_at(header_end + separator);
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let _ = parts.next();
    let status = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            let printable = status_line
                .chars()
                .take(120)
                .map(|character| {
                    if character.is_ascii_graphic() || character == ' ' {
                        character
                    } else {
                        '�'
                    }
                })
                .collect::<String>();
            NvstRtspError::new(
                "nvst-rtsp-failed",
                format!("Invalid RTSPS status line: {printable:?}"),
            )
        })?;
    let status_text = parts.next().unwrap_or_default().trim().to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let response_cseq = headers
        .get("cseq")
        .and_then(|value| value.parse::<u64>().ok());
    let response_request_id = headers
        .get("request-id")
        .and_then(|value| value.parse::<u64>().ok());
    let sequence_matches = if headers.contains_key("cseq") {
        response_cseq == Some(expected_cseq)
    } else {
        response_request_id == Some(expected_cseq)
    };
    if !sequence_matches {
        return Err(NvstRtspError::new(
            "nvst-rtsp-sequence-mismatch",
            format!(
                "RTSPS response sequence mismatch: expected {expected_cseq}, CSeq={}, Request-Id={}",
                response_cseq
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "missing".to_owned()),
                response_request_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "missing".to_owned()),
            ),
        ));
    }
    Ok(Some(RtspResponse {
        status,
        status_text,
        headers,
        body: body.to_owned(),
    }))
}

fn rtsp_endpoint_urls(endpoint: &str) -> Result<(String, String), NvstRtspError> {
    if endpoint.starts_with("rtsp://") {
        return Err(NvstRtspError::new(
            "nvst-raw-rtsp-unsupported",
            "Raw RTSP signaling is not supported by the NVST WebSocket client",
        ));
    }
    let translated = endpoint
        .strip_prefix("rtsps://")
        .map(|rest| format!("https://{rest}"))
        .ok_or_else(|| {
            NvstRtspError::new(
                "invalid-rtsps-endpoint",
                "Unsupported RTSPS endpoint scheme",
            )
        })?;
    let parsed = translated
        .parse::<Uri>()
        .map_err(|_| NvstRtspError::new("invalid-rtsps-endpoint", "Invalid RTSPS endpoint"))?;
    let host = parsed.host().ok_or_else(|| {
        NvstRtspError::new("invalid-rtsps-endpoint", "RTSPS endpoint has no host")
    })?;
    let authority = parsed.authority().ok_or_else(|| {
        NvstRtspError::new("invalid-rtsps-endpoint", "RTSPS endpoint has no authority")
    })?;
    if authority.as_str().contains('@')
        || endpoint.contains('#')
        || endpoint.chars().any(char::is_control)
    {
        return Err(NvstRtspError::new(
            "invalid-rtsps-endpoint",
            "RTSPS endpoint contains invalid authority or characters",
        ));
    }
    if host.starts_with('[')
        && host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .is_none_or(|host| host.parse::<std::net::Ipv6Addr>().is_err())
    {
        return Err(NvstRtspError::new(
            "invalid-rtsps-endpoint",
            "RTSPS endpoint IPv6 host is invalid",
        ));
    }
    let explicit_port = authority.as_str().strip_prefix(host).unwrap_or_default();
    let port = if explicit_port.is_empty() {
        322
    } else {
        explicit_port
            .strip_prefix(':')
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or_else(|| {
                NvstRtspError::new("invalid-rtsps-endpoint", "RTSPS endpoint port is invalid")
            })?
    };
    if port == 0 {
        return Err(NvstRtspError::new(
            "invalid-rtsps-endpoint",
            "RTSPS endpoint port is zero",
        ));
    }
    Ok((
        format!("wss://{host}:{port}/rtsp"),
        format!("rtsps://{host}:{port}"),
    ))
}

/// Shape-only DESCRIBE summary for diagnosing peerless SETUP responses.
/// Logs media sections with ports, control shape classes, and connection
/// presence. Never logs addresses, ports of the connection line, control
/// values, or any SDP attribute values (ICE credentials, fingerprints).
fn describe_media_shape(sdp: &str) -> String {
    let mut sections = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut connection_present = false;
    let mut control_count = 0u32;
    let mut video_control_shape = "absent";
    let mut in_video = false;
    for line in sdp
        .split("||")
        .next()
        .unwrap_or_default()
        .split(";;")
        .next()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
    {
        if let Some(media) = line.strip_prefix("m=") {
            if let Some((media, port)) = current.take() {
                sections.push(format!("{media}/{port}"));
            }
            let mut parts = media.split_whitespace();
            let kind = parts.next().unwrap_or("");
            let media = if kind.eq_ignore_ascii_case("video") {
                "video"
            } else if kind.eq_ignore_ascii_case("audio") {
                "audio"
            } else if kind.eq_ignore_ascii_case("application") {
                "application"
            } else {
                "other"
            };
            current = Some((
                media.to_owned(),
                parts
                    .next()
                    .and_then(|port| port.parse::<u16>().ok())
                    .map_or_else(|| "?".to_owned(), |port| port.to_string()),
            ));
            in_video = media == "video";
        } else if line.starts_with("c=") {
            connection_present = true;
        } else if let Some(value) = line.strip_prefix("a=control:") {
            control_count += 1;
            if in_video && video_control_shape == "absent" {
                let lower = value.to_ascii_lowercase();
                video_control_shape = if value == "*" || value.is_empty() {
                    "empty"
                } else if lower.starts_with("rtsps://") || lower.starts_with("rtsp://") {
                    "absolute-url"
                } else if lower.starts_with("streamid=") {
                    "relative-streamid"
                } else {
                    "other"
                };
            }
        }
    }
    if let Some((media, port)) = current.take() {
        sections.push(format!("{media}/{port}"));
    }
    format!(
        "sections=[{}] connection_present={connection_present} control_count={control_count} video_control={video_control_shape}",
        sections.join(","),
    )
}

fn media_control(sdp: &str, kind: &str) -> Option<String> {
    let mut current = "";
    for line in sdp
        .split("||")
        .next()?
        .split(";;")
        .next()?
        .lines()
        .map(str::trim)
    {
        if let Some(media) = line.strip_prefix("m=") {
            current = media.split_whitespace().next().unwrap_or("");
        } else if current.eq_ignore_ascii_case(kind)
            && let Some(value) = line.strip_prefix("a=control:")
            && value != "*"
            && !value.is_empty()
        {
            return Some(value.to_owned());
        }
    }
    None
}

fn video_qos_offers(sdp: &str) -> VideoQosOffers {
    let mut offers = VideoQosOffers::default();
    let mut video_formats = Vec::new();
    let mut in_video = false;
    for line in sdp
        .split("||")
        .next()
        .unwrap_or_default()
        .split(";;")
        .next()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
    {
        if let Some(media) = line.strip_prefix("m=") {
            let mut parts = media.split_whitespace();
            if in_video {
                break;
            }
            in_video = parts
                .next()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("video"));
            if in_video {
                video_formats = parts.skip(2).collect();
            }
        } else if in_video {
            if let Some(fmtp) = line.strip_prefix("a=fmtp:") {
                let mut parts = fmtp.splitn(2, char::is_whitespace);
                if parts
                    .next()
                    .is_some_and(|format| video_formats.contains(&format))
                {
                    for parameter in parts.next().unwrap_or_default().split(';') {
                        offers.record(parameter);
                    }
                }
            } else if line.starts_with("a=") {
                offers.record(line);
            }
        }
    }
    offers
}

fn parse_hid_device_mask(value: &str) -> u32 {
    let trimmed = value.trim();
    let (radix, digits) = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        (16, hex)
    } else if trimmed
        .chars()
        .any(|character| character.is_ascii_hexdigit())
        && trimmed
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        (16, trimmed)
    } else {
        (10, trimmed)
    };
    u32::from_str_radix(digits, radix).unwrap_or(0)
}

fn sdp_attribute(sdp: &str, name: &str) -> Option<String> {
    let candidates = [
        format!("a=x-nv-{name}:").to_ascii_lowercase(),
        format!("a={name}:").to_ascii_lowercase(),
    ];
    sdp.split("||")
        .next()?
        .split(";;")
        .flat_map(str::lines)
        .map(str::trim)
        .filter_map(|line| {
            let lower = line.to_ascii_lowercase();
            candidates.iter().find_map(|prefix| {
                lower.strip_prefix(prefix).and_then(|_| {
                    line.get(prefix.len()..)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
            })
        })
        .last()
}

fn official_video_setup_control(control: &str) -> String {
    let lower = control.to_ascii_lowercase();
    if lower.starts_with("streamid=video/") && control.matches('/').count() == 1 {
        format!("{control}/0")
    } else {
        control.to_owned()
    }
}

fn video_setup_candidates(control: &str, target: &str) -> Vec<String> {
    let mut candidates = vec![official_video_setup_control(control)];
    if candidates[0] != control {
        candidates.push(control.to_owned());
    }
    for index in 0..candidates.len() {
        let control = &candidates[index];
        let lower = control.to_ascii_lowercase();
        if lower.starts_with("rtsps://") || lower.starts_with("rtsp://") {
            continue;
        }
        let absolute = format!(
            "{}/{}",
            target.trim_end_matches('/'),
            control.trim_start_matches('/')
        );
        if !candidates.contains(&absolute) {
            candidates.push(absolute);
        }
    }
    candidates
}

fn parse_video_peer(transport: &str) -> Option<(String, u16, u16)> {
    let mut ip = None;
    let mut port = None;
    let mut port_end = None;
    for part in transport.split([';', ',']) {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("source") {
            ip = Some(value.trim().to_owned());
        } else if name.eq_ignore_ascii_case("X-GS-ServerPort") {
            let (first, last) = value
                .trim()
                .split_once('-')
                .unwrap_or((value.trim(), value.trim()));
            let first = first.parse::<u16>().ok().filter(|port| *port != 0)?;
            port = Some(first);
            port_end = Some(
                last.parse::<u16>()
                    .ok()
                    .filter(|last| *last >= first && *last - first < MAX_NVST_VIDEO_PEER_PORTS)
                    .unwrap_or(first),
            );
        }
    }
    Some((ip?, port?, port_end?))
}

fn increment_hex(value: &str) -> Option<String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = value.as_bytes().to_vec();
    let mut carry = true;
    for byte in bytes.iter_mut().rev() {
        if !carry {
            break;
        }
        let digit = (*byte as char).to_digit(16)?;
        if digit == 15 {
            *byte = b'0';
        } else {
            *byte = char::from_digit(digit + 1, 16)?.to_ascii_lowercase() as u8;
            carry = false;
        }
    }
    if carry {
        bytes.insert(0, b'1');
    }
    String::from_utf8(bytes).ok()
}

fn resolve_remote_ufrag(
    ping_payload: Option<&str>,
    described_ufrag: Option<&str>,
    ping_version: u8,
) -> Option<String> {
    if let Some(payload) = ping_payload {
        if let Some(incremented) = increment_hex(payload) {
            return Some(incremented);
        }
        if payload.eq_ignore_ascii_case("PING") || ping_version == 6 {
            return Some(payload.to_owned());
        }
    }
    described_ufrag.map(ToOwned::to_owned)
}

fn bundle_natt_username(sdp: &str) -> Option<String> {
    let ufrag = sdp_attribute(sdp, "general.iceUsernameFragment")
        .or_else(|| sdp_attribute(sdp, "general.iceUserNameFragmentV2"))?;
    let port = sdp_attribute(sdp, "general.serverBundlePort")?
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)?;
    let port_text = port.to_string();
    if ufrag.is_empty()
        || ufrag.len() + port_text.len() > 256
        || !ufrag
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b':')
    {
        return None;
    }
    Some(format!("{ufrag}{port_text}"))
}

fn set_bundle_natt_username(handoff: &mut Value, sdp: &str, ping_version: u8) {
    if ping_version != 6 {
        return;
    }
    if let Some(username) = bundle_natt_username(sdp) {
        handoff["bundleNattRemoteUsername"] = json!(username);
    }
}

fn runtime_key(sdp: &str) -> Option<(String, u32)> {
    let key = sdp_attribute(sdp, "runtime.encryptionKey")?;
    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let raw = sdp_attribute(sdp, "runtime.encryptionKeyId")?
        .parse::<i64>()
        .ok()?;
    Some((key.to_ascii_uppercase(), raw as u32))
}

fn random_runtime_key() -> Result<(String, u32), NvstRtspError> {
    let mut key = [0_u8; 32];
    let mut id = [0_u8; 4];
    getrandom::fill(&mut key).map_err(|error| {
        NvstRtspError::new(
            "randomness-unavailable",
            format!("Could not generate the NVST runtime key: {error}"),
        )
    })?;
    getrandom::fill(&mut id).map_err(|error| {
        NvstRtspError::new(
            "randomness-unavailable",
            format!("Could not generate the NVST runtime key ID: {error}"),
        )
    })?;
    Ok((
        key.iter().map(|byte| format!("{byte:02X}")).collect(),
        u32::from_be_bytes(id),
    ))
}

fn set_io_timeout(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, timeout: Duration) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
            let _ = stream.set_write_timeout(Some(timeout));
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.get_mut().set_read_timeout(Some(timeout));
            let _ = stream.get_mut().set_write_timeout(Some(timeout));
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "nvst_rtsp_control_ping_tests.rs"]
mod control_ping_tests;

#[cfg(test)]
#[path = "nvst_rtsp_tls_tests.rs"]
mod tls_tests;

#[cfg(test)]
#[path = "nvst_rtsp_setup_tests.rs"]
mod setup_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stream_config(context: &SessionContext) -> MediaStreamConfig {
        super::super::media_stream_config(context)
    }

    fn announce_color_format(context: &SessionContext) -> (u8, u8) {
        let stream = stream_config(context);
        (
            stream.color_quality.bit_depth(),
            if stream.color_quality.is_444() { 3 } else { 1 },
        )
    }

    #[test]
    fn video_setup_retains_only_bounded_advertised_port_ranges() {
        for (ports, expected) in [
            ("5004", (5004, 5004)),
            ("5004-5005", (5004, 5005)),
            ("5004-5019", (5004, 5019)),
            ("65534-65535", (65534, 65535)),
            ("65535", (65535, 65535)),
            ("5005-5004", (5005, 5005)),
            ("5004-5020", (5004, 5004)),
            ("5004-65536", (5004, 5004)),
            ("5004-invalid", (5004, 5004)),
            ("5004-5005-5006", (5004, 5004)),
        ] {
            assert_eq!(
                parse_video_peer(&format!(
                    "unicast;X-GS-ServerPort={ports};source=192.0.2.10"
                )),
                Some(("192.0.2.10".to_owned(), expected.0, expected.1)),
                "{ports}"
            );
        }
        for transport in [
            "source=192.0.2.10",
            "X-GS-ServerPort=5004-5005",
            "source=192.0.2.10;X-GS-ServerPort=0-1",
            "source=192.0.2.10;X-GS-ServerPort=65536",
            "source=192.0.2.10;X-GS-ServerPort=invalid",
        ] {
            assert_eq!(parse_video_peer(transport), None, "{transport}");
        }
    }

    #[test]
    fn http_503_is_a_control_service_failure_not_a_decoder_failure() {
        let response = tungstenite::http::Response::builder()
            .status(503)
            .body(Some(b"private response body".to_vec()))
            .unwrap();
        let error = rtsp_connect_error(&tungstenite::Error::Http(Box::new(response)));
        assert_eq!(error.code, "nvst-service-unavailable");
        assert!(error.message.contains("HTTP 503"));
        assert!(!error.message.contains("private response body"));
    }

    #[test]
    fn forbidden_upgrade_stops_signaling_fallback() {
        let response = tungstenite::http::Response::builder()
            .status(403)
            .body(Some(b"private response body".to_vec()))
            .unwrap();
        let failure = rtsp_connect_error(&tungstenite::Error::Http(Box::new(response)));
        assert_eq!(failure.code, "nvst-signaling-forbidden");
        assert!(!failure.message.contains("private response body"));
        let mut attempted = Vec::new();
        let error = try_signaling_endpoints(&[Some("first"), Some("second")], |endpoint| {
            attempted.push(endpoint.to_owned());
            Err::<(), _>(NvstRtspError::new(failure.code, failure.message.clone()))
        })
        .unwrap_err();
        assert_eq!(attempted, ["first"]);
        assert_eq!(error.code, "nvst-signaling-forbidden");
        let mut attempts = 0;
        let error = connect_with_retry::<()>(|| {
            attempts += 1;
            let response = tungstenite::http::Response::builder()
                .status(403)
                .body(None)
                .unwrap();
            Err(tungstenite::Error::Http(Box::new(response)))
        })
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.code, "nvst-signaling-forbidden");
    }

    #[test]
    fn transient_connection_retries_are_bounded_per_endpoint() {
        let mut attempts = 0;
        let result = connect_with_retry(|| {
            attempts += 1;
            if attempts < 3 {
                Err(tungstenite::Error::Io(std::io::Error::from(
                    ErrorKind::ConnectionRefused,
                )))
            } else {
                Ok(42)
            }
        })
        .unwrap();
        assert_eq!(result, 42);
        assert_eq!(attempts, 3);
        let mut attempts = 0;
        let error = connect_with_retry::<()>(|| {
            attempts += 1;
            Err(tungstenite::Error::Io(std::io::Error::from(
                ErrorKind::ConnectionRefused,
            )))
        })
        .unwrap_err();
        assert_eq!(attempts, 3);
        assert_eq!(error.code, "nvst-connect-failed");
    }

    #[test]
    fn signaling_attempts_every_endpoint_in_order_with_a_fresh_handshake() {
        let mut attempted = Vec::new();
        let session = try_signaling_endpoints(
            &[None, Some("unavailable"), Some("working"), Some("unused")],
            |endpoint| {
                attempted.push(endpoint.to_owned());
                match endpoint {
                    "unavailable" => {
                        Err(NvstRtspError::new("nvst-service-unavailable", "HTTP 503"))
                    }
                    _ => Ok(endpoint.to_owned()),
                }
            },
        )
        .unwrap();
        assert_eq!(session, "working");
        assert_eq!(attempted, ["unavailable", "working"]);

        let error = try_signaling_endpoints::<()>(&[None, Some("unavailable")], |endpoint| {
            Err(NvstRtspError::new(
                if endpoint == "invalid" {
                    "invalid-rtsps-endpoint"
                } else {
                    "nvst-service-unavailable"
                },
                "failed",
            ))
        })
        .unwrap_err();
        assert_eq!(error.code, "nvst-service-unavailable");
        assert!(
            error
                .message
                .contains("1:invalid-rtsps-endpoint, 2:nvst-service-unavailable")
        );
        assert_eq!(
            try_signaling_endpoints::<()>(&[], |_| Ok(()))
                .unwrap_err()
                .code,
            "missing-rtsps-endpoint"
        );
    }

    fn context() -> SessionContext {
        serde_json::from_value(json!({
            "session": {
                "sessionId": "session",
                "serverIp": "seat.nvidiagrid.net",
                "rtspsEndpoints": ["rtsps://seat.nvidiagrid.net:322/session"],
                "iceServers": [],
                "negotiatedStreamProfile": {
                    "codec": "AV1",
                    "fps": 120,
                    "colorQuality": "10bit_444"
                }
            },
            "settings": {
                "transportMode": "nvst",
                "codec": "H264",
                "resolution": "2560x1440",
                "fps": 60,
                "maxBitrateMbps": 75
            },
            "shortcuts": {}
        }))
        .expect("context")
    }

    #[test]
    fn rtsps_supports_tls12_and_tls13_without_legacy_versions() {
        let versions: Vec<_> = rustls::DEFAULT_VERSIONS
            .iter()
            .map(|version| version.version)
            .collect();
        assert_eq!(
            versions,
            vec![
                rustls::ProtocolVersion::TLSv1_3,
                rustls::ProtocolVersion::TLSv1_2
            ]
        );
    }

    #[test]
    fn installs_a_process_level_tls_crypto_provider() {
        ensure_tls_crypto_provider().expect("TLS provider");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn announce_carries_the_documented_top_tier_frame_rate() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"] = json!({"codec":"AV1", "fps":360});
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].maxFPS:360"));
        assert!(sdp.contains("a=x-nv-packetPacing.maxDelayUs:4000"));

        let mut runaway = context();
        runaway.session.extra["negotiatedStreamProfile"] = json!({"codec":"AV1", "fps":600});
        let sdp = build_announce(
            &runaway,
            AnnounceParams {
                stream: stream_config(&runaway),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].maxFPS:360"));
        assert!(!sdp.contains("a=x-nv-video[0].maxFPS:600"));
    }

    #[test]
    fn announce_with_v5_timings_does_not_request_legacy_pacing_feedback() {
        let value = context();
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: true,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].framePacing.mode:2\r\n"));
        assert!(sdp.contains("a=x-nv-video[0].framePacing.feedbackMode:0\r\n"));
    }

    #[test]
    fn owned_announce_matches_current_official_bundle_baseline() {
        let value = context();
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].maxFPS:120"));
        assert!(sdp.contains("a=x-nv-video[0].bitDepth:10"));
        assert!(sdp.contains("a=x-nv-video[0].chromaFormat:1"));
        assert!(sdp.contains("a=x-nv-video[0].maxCodecProfile:3"));
        assert!(sdp.contains("a=x-nv-video[0].maxCodecLevel:51"));
        assert!(sdp.contains("a=x-nv-video[0].maxH264Level:51"));
        assert!(!sdp.contains("a=x-nv-video[0].videoSplitEncodeStripsPerFrame:"));
        assert!(!sdp.contains("a=x-nv-vqos[0].grc.enable:"));
        assert!(!sdp.contains("a=x-nv-clientSupportHevc:"));
        assert!(sdp.contains("a=x-nv-video[0].encoderCscMode:2"));
        assert!(sdp.contains("a=x-nv-vqos[0].bitStreamFormat:2"));
        assert!(sdp.contains("a=x-nv-general.clientBundlePort:49006"));
        assert!(sdp.contains("a=x-nv-general.rtcDataChannelOnNativeBundle:1"));
        assert!(sdp.contains("a=x-nv-runtime.encryptionKey:"));
        assert!(sdp.contains("m=video 5004"));
    }

    #[test]
    fn announce_preserves_the_route_selected_video_packet_size() {
        for video_packet_size in [1280, 1216, 1200, 1136] {
            let sdp = build_announce(
                &context(),
                AnnounceParams {
                    stream: stream_config(&context()),
                    key: &"01".repeat(32),
                    key_id: 7,
                    port: 49006,
                    address: "192.0.2.10",
                    ufrag: "abcd",
                    password: "abcdefghijklmnopqrstuv",
                    fingerprint: "AA:BB",
                    video_port: 5004,
                    video_packet_size,
                    rtcp_on_sctp: true,
                    microphone_available: false,
                    qos_timings_v5: false,
                },
            );
            assert_eq!(
                sdp_attribute(&sdp, "video[0].packetSize"),
                Some(video_packet_size.to_string())
            );
        }
    }

    #[test]
    fn announce_uses_the_measured_authenticated_path_when_it_is_tighter() {
        let mut context = context();
        context.session.extra.insert(
            "networkTest".to_owned(),
            json!({"sessionId":"nt-1", "measuredDatagramBytes":1_200}),
        );
        let peer: IpAddr = "192.0.2.1".parse().unwrap();
        let packet_size = measured_path_packet_size(&context, peer).expect("measured packet size");
        assert_eq!(packet_size, 1_168);
        assert!(packet_size < nvst_video_packet_size(peer).unwrap());

        let sdp = build_announce(
            &context,
            AnnounceParams {
                stream: stream_config(&context),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: packet_size,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert_eq!(
            sdp_attribute(&sdp, "video[0].packetSize"),
            Some(packet_size.to_string())
        );
    }

    #[test]
    fn announce_ignores_an_absent_or_unusable_measurement() {
        let base = context();
        let peer: IpAddr = "192.0.2.1".parse().unwrap();
        assert_eq!(measured_path_packet_size(&base, peer), None);

        for measured in [0_u64, 1, 12] {
            let mut context = context();
            context.session.extra.insert(
                "networkTest".to_owned(),
                json!({"measuredDatagramBytes":measured}),
            );
            assert_eq!(
                measured_path_packet_size(&context, peer),
                None,
                "{measured}"
            );
        }
    }

    #[test]
    fn announce_prefers_a_finalized_policy_but_falls_back_to_live_settings() {
        // The official client never sends dynamicStreamingMode to CloudMatch, so a
        // negotiated value can only be a server-finalized override. Otherwise the
        // live saveBandwidth setting decides, matching the official RTSP-only policy.
        for (profile, save, policy, adjust) in [
            (None, false, 0, 0),
            (None, true, 1, 1),
            (Some(1), false, 1, 1),
            (Some(2), false, 2, 1),
            (Some(3), false, 3, 1),
            (Some(7), true, 1, 1),
            (Some(7), false, 0, 0),
        ] {
            let mut value = context();
            value.settings["saveBandwidth"] = json!(save);
            if let Some(profile) = profile {
                value.session.extra["negotiatedStreamProfile"]["dynamicStreamingMode"] =
                    json!(profile);
            }
            let sdp = build_announce(
                &value,
                AnnounceParams {
                    stream: stream_config(&value),
                    key: &"01".repeat(32),
                    key_id: 7,
                    port: 49006,
                    address: "192.0.2.10",
                    ufrag: "abcd",
                    password: "abcdefghijklmnopqrstuv",
                    fingerprint: "AA:BB",
                    video_port: 5004,
                    video_packet_size: 1280,
                    rtcp_on_sctp: true,
                    microphone_available: false,
                    qos_timings_v5: false,
                },
            );
            assert!(sdp.contains(&format!("a=x-nv-vqos[0].dynamicStreamingMode:{policy}\r\n")));
            assert!(sdp.contains(&format!("a=x-nv-vqos[0].dfc.adjustResAndFps:{adjust}\r\n")));
            assert!(sdp.contains("a=x-nv-vqos[0].drc.enable:0\r\n"));
        }
    }

    #[test]
    fn announce_dynamic_range_follows_accepted_hdr_not_saved_intent() {
        for (accepted, requested, hdr) in [
            (json!(true), false, true),
            (json!(false), true, false),
            (Value::Null, true, false),
        ] {
            let mut value = context();
            value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H265");
            value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_420");
            value.session.extra["negotiatedStreamProfile"]["enableHdr"] = accepted;
            value.settings["enableHdr"] = json!(requested);
            let sdp = build_announce(
                &value,
                AnnounceParams {
                    stream: stream_config(&value),
                    key: &"01".repeat(32),
                    key_id: 7,
                    port: 49006,
                    address: "192.0.2.10",
                    ufrag: "abcd",
                    password: "abcdefghijklmnopqrstuv",
                    fingerprint: "AA:BB",
                    video_port: 5004,
                    video_packet_size: 1280,
                    rtcp_on_sctp: true,
                    microphone_available: false,
                    qos_timings_v5: false,
                },
            );
            // HDR carries an explicit :1; SDR omits the line, like the official client.
            assert_eq!(sdp.contains("a=x-nv-video[0].dynamicRangeMode:1\r\n"), hdr);
            assert!(!sdp.contains("a=x-nv-video[0].dynamicRangeMode:0"));
            assert!(sdp.contains("a=x-nv-video[0].bitDepth:10\r\n"));
            assert!(sdp.contains("a=x-nv-video[0].chromaFormat:1\r\n"));
        }
    }

    #[test]
    fn owned_announce_preserves_the_configured_200_mbps_ceiling() {
        let mut value = context();
        value.settings["maxBitrateMbps"] = json!(200);
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].initialBitrateKbps:200000"));
        assert!(sdp.contains("a=x-nv-video[0].initialPeakBitrateKbps:200000"));
        assert!(sdp.contains("a=x-nv-vqos[0].bw.maximumBitrateKbps:200000"));
        assert!(sdp.contains("a=x-nv-vqos[0].bw.minimumBitrateKbps:1000"));
    }

    #[test]
    fn owned_announce_can_request_220_kbps_without_a_1_mbps_floor() {
        let mut value = context();
        value.settings["maxBitrateMbps"] = json!(0.22);
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: false,
                qos_timings_v5: false,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].initialBitrateKbps:220"));
        assert!(sdp.contains("a=x-nv-vqos[0].bw.maximumBitrateKbps:220"));
        assert!(sdp.contains("a=x-nv-vqos[0].bw.minimumBitrateKbps:220"));
    }

    #[test]
    fn microphone_announce_requires_request_and_server_offer() {
        let mut value = context();
        assert!(!negotiate_microphone(
            &value,
            "a=x-nv-general.rtcMicOnNativeBundle:1\r\n"
        ));
        for mode in ["disabled", "voice-activity", "push-to-talk"] {
            value.settings["microphoneMode"] = json!(mode);
            for offer in [
                "",
                "a=x-nv-general.rtcMicOnNativeBundle:0\r\n",
                "a=x-nv-general.rtcMicOnNativeBundle:1\r\n",
            ] {
                let available = negotiate_microphone(&value, offer);
                assert_eq!(available, mode == "voice-activity" && offer.contains(":1"));
                let sdp = build_announce(
                    &value,
                    AnnounceParams {
                        stream: stream_config(&value),
                        key: &"01".repeat(32),
                        key_id: 7,
                        port: 49006,
                        address: "192.0.2.10",
                        ufrag: "abcd",
                        password: "abcdefghijklmnopqrstuv",
                        fingerprint: "AA:BB",
                        video_port: 5004,
                        video_packet_size: 1280,
                        rtcp_on_sctp: true,
                        microphone_available: available,
                        qos_timings_v5: false,
                    },
                );
                assert_eq!(
                    sdp.contains("a=x-nv-general.rtcMicOnNativeBundle:1\r\n"),
                    available
                );
                assert_eq!(
                    sdp.contains("a=x-nv-mic.micSsrcConfig.senderSsrc:1\r\n"),
                    available
                );
                assert_eq!(sdp.contains("rtcMicOnNativeBundle"), available);
                assert!(sdp.contains("a=x-nv-general.rtcAudioOnNativeBundle:1\r\n"));
            }
        }
    }

    #[test]
    fn h264_announce_stays_eight_bit_420() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H264");
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(announce_color_format(&value), (8, 1));
    }

    #[test]
    fn av1_announce_stays_420_but_preserves_ten_bit_depth() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(announce_color_format(&value), (10, 1));
    }

    #[test]
    fn accepted_hevc_color_preserves_nvst_depth_and_chroma_enum_space() {
        for (color, format) in [
            ("8bit_420", (8, 1)),
            ("8bit_444", (8, 3)),
            ("10bit_420", (10, 1)),
            ("10bit_444", (10, 3)),
        ] {
            let mut value = context();
            value.settings["colorQuality"] = json!("8bit_420");
            value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H265");
            value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!(color);
            assert_eq!(announce_color_format(&value), format);
        }
    }

    #[test]
    fn h265_announce_supports_ten_bit_444() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H265");
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(announce_color_format(&value), (10, 3));
    }

    #[test]
    fn full_announce_always_states_depth_chroma_and_hdr_explicitly() {
        for (codec, color, hdr, depth, chroma) in [
            ("H265", "10bit_444", false, "10", "3"),
            ("H265", "10bit_420", false, "10", "1"),
            ("H264", "8bit_420", false, "8", "1"),
        ] {
            let mut value = context();
            value.session.extra["negotiatedStreamProfile"]["codec"] = json!(codec);
            value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!(color);
            let sdp = build_announce(
                &value,
                AnnounceParams {
                    stream: stream_config(&value),
                    key: &"01".repeat(32),
                    key_id: 7,
                    port: 49006,
                    address: "192.0.2.10",
                    ufrag: "abcd",
                    password: "abcdefghijklmnopqrstuv",
                    fingerprint: "AA:BB",
                    video_port: 5004,
                    video_packet_size: 1280,
                    rtcp_on_sctp: true,
                    microphone_available: false,
                    qos_timings_v5: false,
                },
            );
            assert_eq!(
                sdp_attribute(&sdp, "video[0].bitDepth"),
                Some(depth.to_owned())
            );
            assert_eq!(
                sdp_attribute(&sdp, "video[0].chromaFormat"),
                Some(chroma.to_owned())
            );
            // SDR never carries a dynamic-range line; HDR always carries :1.
            assert_eq!(
                sdp_attribute(&sdp, "video[0].dynamicRangeMode"),
                hdr.then(|| "1".to_owned())
            );
            for field in ["bitDepth", "chromaFormat"] {
                assert_eq!(
                    sdp.matches(&format!("a=x-nv-video[0].{field}:")).count(),
                    1,
                    "{codec}/{color}"
                );
            }
            // Encoder identity the seat reads before initializing.
            for line in [
                "a=x-nv-video[0].maxCodecProfile:3",
                "a=x-nv-video[0].maxCodecLevel:51",
                "a=x-nv-video[0].maxH264Profile:3",
                "a=x-nv-video[0].maxH264Level:51",
            ] {
                assert!(sdp.contains(line), "{line}");
            }
        }
    }

    #[test]
    fn announce_does_not_echo_server_config_or_invent_server_owned_values() {
        let value = context();
        let sdp = build_announce(
            &value,
            AnnounceParams {
                stream: stream_config(&value),
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                video_packet_size: 1280,
                rtcp_on_sctp: true,
                microphone_available: true,
                qos_timings_v5: false,
            },
        );
        for server_value in ["0", "23"] {
            let server_bitrate = if server_value == "0" { 100_000 } else { 50_000 };
            let describe = format!(
                "v=0\r\na=x-nv-video[0].framePacing.mode:2\r\na=x-nv-vqos[0].fec.repairMinPercent:5\r\na=x-nv-vqos[0].grc.enable:{server_value}\r\na=x-nv-vqos[0].bw.maximumBitrateKbps:{server_bitrate}\r\na=x-nv-video[0].maxCodecLevel:61\r\na=x-nv-video[0].chromaFormat:0\r\na=x-nv-video[0].packetSize:1408\r\n;;v=0\r\na=x-nv-video[0].framePacing.feedbackMode:0\r\na=x-nv-packetPacing.maxDelayUs:1000\r\na=x-nv-packetPacing.minNumPacketsPerGroup:0\r\n||v=0\r\na=x-nv-vqos[0].fec.enable:0\r\n"
            );
            assert_eq!(
                sdp_attribute(&describe, "video[0].framePacing.feedbackMode"),
                Some("0".to_owned())
            );
            let announce = omit_server_announce_attributes(&sdp, &describe);
            for field in [
                "video[0].framePacing.mode",
                "video[0].framePacing.feedbackMode",
                "vqos[0].fec.repairMinPercent",
                "vqos[0].grc.enable",
                "packetPacing.maxDelayUs",
                "packetPacing.minNumPacketsPerGroup",
            ] {
                assert_eq!(sdp_attribute(&announce, field), None, "{field}");
            }
            assert_eq!(
                sdp_attribute(&announce, "vqos[0].fec.enable"),
                Some("1".to_owned())
            );
            assert_eq!(
                sdp_attribute(&announce, "video[0].maxCodecLevel"),
                Some("51".to_owned())
            );
            assert_eq!(
                sdp_attribute(&announce, "video[0].chromaFormat"),
                Some("1".to_owned())
            );
            assert_eq!(
                sdp_attribute(&announce, "vqos[0].bw.maximumBitrateKbps"),
                Some(75_000_u64.min(server_bitrate).to_string())
            );
            for (field, value) in [
                ("video[0].packetSize", "1280"),
                ("video[0].maxFPS", "120"),
                ("general.iceUserNameFragmentV2", "abcd"),
                ("general.rtcMicOnNativeBundle", "1"),
                ("mic.micSsrcConfig.senderSsrc", "1"),
            ] {
                assert_eq!(sdp_attribute(&announce, field).as_deref(), Some(value));
            }
            for server_owned in [
                "video[0].videoSplitEncodeStripsPerFrame",
                "video[0].adaptiveQuantization.spatialAQStrength",
                "vqos[0].grc.enable",
                "aqos.redundancyLevel",
                "general.enetControlChannel.mtuSize",
            ] {
                assert_eq!(
                    sdp_attribute(&announce, server_owned),
                    None,
                    "{server_owned}"
                );
            }
        }
        for invalid_cap in ["0", "not-a-number"] {
            let describe = format!("a=x-nv-vqos[0].bw.maximumBitrateKbps:{invalid_cap}\r\n");
            let announce = omit_server_announce_attributes(&sdp, &describe);
            assert_eq!(
                sdp_attribute(&announce, "vqos[0].bw.maximumBitrateKbps"),
                Some("75000".to_owned()),
                "{invalid_cap}"
            );
        }
    }

    #[test]
    fn rtsp_parser_waits_for_body_and_checks_cseq() {
        let mut buffer = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 4\r\n\r\ntest".to_owned();
        let response = take_rtsp_response(&mut buffer, 3).unwrap().unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "test");
        assert!(buffer.is_empty());
    }

    #[test]
    fn rtsp_parser_accepts_matching_request_id_when_setup_omits_cseq() {
        let mut buffer = "RTSP/1.0 200 OK\r\nRequest-Id: 3\r\nContent-Length: 0\r\n\r\n".to_owned();
        assert!(take_rtsp_response(&mut buffer, 3).unwrap().is_some());

        let mut uncorrelated = "RTSP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n".to_owned();
        assert!(take_rtsp_response(&mut uncorrelated, 3).is_err());
    }

    #[test]
    fn rtsp_parser_ignores_blank_transport_padding_before_next_response() {
        let mut buffer =
            "\r\n\r\nRTSP/1.0 200 OK\r\nCSeq: 4\r\nRequest-Id: 4\r\nContent-Length: 0\r\n\r\n"
                .to_owned();
        let response = take_rtsp_response(&mut buffer, 4).unwrap().unwrap();
        assert_eq!(response.status, 200);
        assert!(buffer.is_empty());
    }

    #[test]
    fn official_setup_preserves_the_relative_video_control_target() {
        assert_eq!(
            official_video_setup_control("streamid=video/0"),
            "streamid=video/0/0"
        );
        assert_eq!(
            official_video_setup_control("streamid=video/0/0"),
            "streamid=video/0/0"
        );
    }

    #[test]
    fn describe_features_override_main_without_reading_upstream_offer() {
        let describe = "v=0\r\na=x-nv-general.disablePlay:0\r\na=x-nv-general.nativeRtcOnBundlePort:0\r\nm=video 5004\r\na=control:streamid=video/0\r\n;;v=0\r\na=x-nv-general.nativeRtcOnBundlePort:1\r\na=x-nv-general.disablePlay:1\r\nm=video 6000\r\na=control:wrong-control\r\n||v=0\r\na=x-nv-general.disablePlay:0\r\n";
        assert_eq!(
            sdp_attribute(describe, "general.nativeRtcOnBundlePort").as_deref(),
            Some("1")
        );
        assert_eq!(
            sdp_attribute(describe, "general.disablePlay").as_deref(),
            Some("1")
        );
        assert_eq!(
            media_control(describe, "video").as_deref(),
            Some("streamid=video/0")
        );
        assert_eq!(
            media_control("v=0\r\n||m=video 5004\r\na=control:upstream\r\n", "video"),
            None
        );
        assert_eq!(sdp_attribute("v=0\r\n", "general.disablePlay"), None);
    }

    #[test]
    fn describe_video_qos_offers_parse_official_fmtp_and_attributes() {
        let describe = "v=0\r\na=nv-video-qos-feedback-version:99\r\n\
            m=audio 5006 RTP/SAVPF 111\r\na=fmtp:111 nv-video-qos-feedback-version=99\r\n\
            m=video 5004 RTP/SAVPF 96 97\r\n\
            a=fmtp:111 nv-video-qos-feedback-version=99\r\n\
            a=fmtp:96 packetization-mode=1; nv-video-qos-feedback-version=7; nv-video-qos-timings-version=5\r\n\
            a=nv-video-qos-blob-stats-version:9\r\n\
            m=application 6000 UDP/DTLS/SCTP webrtc-datachannel\r\n\
            a=nv-video-qos-blob-stats-version:99\r\n";
        let offers = video_qos_offers(describe);
        assert_eq!(
            offers,
            VideoQosOffers {
                feedback: QosVersionOffer::Version(7),
                timings: QosVersionOffer::Version(5),
                blob_stats: QosVersionOffer::Version(9),
            }
        );
        let mut handoff = json!({});
        offers.add_to_handoff(&mut handoff);
        assert_eq!(
            handoff,
            json!({"qosFeedbackVersion":7,"qosTimingsVersion":5,"qosBlobStatsVersion":9})
        );
        assert!(offers.uses_v5_timings());
        offers
            .validate_pacing(
                "m=video 5004 RTP/SAVPF 96\r\na=x-nv-video[0].framePacing.mode:2\r\na=x-nv-video[0].framePacing.feedbackMode:0\r\n",
            )
            .unwrap();
        assert_eq!(
            offers
                .validate_pacing(
                    "m=video 5004 RTP/SAVPF 96\r\na=x-nv-video[0].framePacing.feedbackMode:1\r\n"
                )
                .unwrap_err()
                .code,
            "nvst-qos-pacing-unsupported"
        );
    }

    #[test]
    fn describe_video_qos_offers_retain_lower_and_higher_versions() {
        for (feedback, timings, blob_stats) in [(5, 3, 8), (8, 6, 10), (0, 255, 9)] {
            let describe = format!(
                "v=0\r\nm=video 5004 RTP/SAVPF 96\r\n\
                 a=fmtp:96 nv-video-qos-feedback-version={feedback};nv-video-qos-timings-version={timings};nv-video-qos-blob-stats-version={blob_stats}\r\n"
            );
            let offers = video_qos_offers(&describe);
            assert_eq!(offers.feedback, QosVersionOffer::Version(feedback));
            assert_eq!(offers.timings, QosVersionOffer::Version(timings));
            assert_eq!(offers.blob_stats, QosVersionOffer::Version(blob_stats));
            if feedback < 7 || timings < 5 || blob_stats < 9 {
                assert_eq!(
                    offers.validate().unwrap_err().code,
                    "nvst-qos-version-unsupported"
                );
            } else {
                offers.validate().unwrap();
            }
            let mut handoff = json!({});
            offers.add_to_handoff(&mut handoff);
            assert_eq!(handoff["qosFeedbackVersion"], feedback);
            assert_eq!(handoff["qosTimingsVersion"], timings);
            assert_eq!(handoff["qosBlobStatsVersion"], blob_stats);
        }
    }

    #[test]
    fn describe_video_qos_offers_distinguish_malformed_from_missing() {
        for malformed in [
            "",
            "-1",
            "+7",
            "7x",
            "256",
            "999999999999999999999999999999",
        ] {
            let describe = format!(
                "m=video 5004 RTP/SAVPF 96\r\n\
                 a=fmtp:96 nv-video-qos-feedback-version={malformed};nv-video-qos-timings-version=5\r\n"
            );
            let offers = video_qos_offers(&describe);
            assert_eq!(offers.feedback, QosVersionOffer::Malformed, "{malformed}");
            assert_eq!(
                offers.validate().unwrap_err().code,
                "nvst-qos-version-invalid"
            );
            assert_eq!(offers.timings, QosVersionOffer::Version(5));
            assert_eq!(offers.blob_stats, QosVersionOffer::Missing);
            let mut handoff = json!({});
            offers.add_to_handoff(&mut handoff);
            assert_eq!(handoff, json!({"qosTimingsVersion":5}));
        }
        let offers = video_qos_offers(
            "m=video 5004 RTP/SAVPF 96\r\na=nv-video-qos-feedback-version\r\n\
             a=nv-video-qos-timings-version:5\r\na=nv-video-qos-timings-version:6\r\n",
        );
        assert_eq!(offers.feedback, QosVersionOffer::Malformed);
        assert_eq!(offers.timings, QosVersionOffer::Malformed);
        assert_eq!(offers.blob_stats, QosVersionOffer::Missing);
    }

    #[test]
    fn describe_video_qos_offers_read_only_first_describe_video_section() {
        let describe = "v=0\r\nm=video 5004 RTP/SAVPF 96\r\n\
            a=fmtp:96 nv-video-qos-feedback-version=7\r\n\
            m=video 5006 RTP/SAVPF 97\r\na=nv-video-qos-timings-version:5\r\n\
            ;;v=0\r\nm=video 6000 RTP/SAVPF 96\r\na=nv-video-qos-blob-stats-version:9\r\n\
            ||v=0\r\nm=video 7000 RTP/SAVPF 96\r\na=nv-video-qos-timings-version:8\r\n";
        assert_eq!(
            video_qos_offers(describe),
            VideoQosOffers {
                feedback: QosVersionOffer::Version(7),
                timings: QosVersionOffer::Missing,
                blob_stats: QosVersionOffer::Missing,
            }
        );
        assert_eq!(
            video_qos_offers(
                "v=0\r\n||m=video 5004 RTP/SAVPF 96\r\na=nv-video-qos-feedback-version:7\r\n"
            ),
            VideoQosOffers::default()
        );
    }

    #[test]
    fn describe_qos_log_shape_excludes_offers_and_credentials() {
        let describe = "v=0\r\nm=video 5004 RTP/SAVPF 96\r\n\
            a=fmtp:96 nv-video-qos-feedback-version=7;password=secret-credential\r\n\
            a=nv-video-qos-timings-version:5\r\n\
            a=x-nv-general.icePasswordV2:secret-credential\r\n";
        let shape = describe_media_shape(describe);
        assert_eq!(
            shape,
            "sections=[video/5004] connection_present=false control_count=0 video_control=absent"
        );
        assert!(!shape.contains("nv-video-qos"));
        assert!(!shape.contains("secret-credential"));
        let malformed = describe_media_shape(
            "m=secret-credential 5004 RTP/SAVPF 96\r\nm=video secret-credential RTP/SAVPF 96\r\n",
        );
        assert_eq!(
            malformed,
            "sections=[other/5004,video/?] connection_present=false control_count=0 video_control=absent"
        );
        assert!(!malformed.contains("secret-credential"));
    }

    #[test]
    fn describe_shape_reports_sections_and_controls_without_values() {
        let shape = describe_media_shape(
            "v=0\r\no=- 0 0 IN IP4 203.0.113.9\r\nc=IN IP4 203.0.113.9\r\n\
             m=video 5004 RTP/SAVPF 96\r\na=control:streamid=video/0\r\n\
             a=x-nv-general.iceUserNameFragment:secret\r\n\
             m=audio 5006 RTP/SAVPF 111\r\na=control:*\r\n",
        );
        assert_eq!(
            shape,
            "sections=[video/5004,audio/5006] connection_present=true \
             control_count=2 video_control=relative-streamid"
        );
        assert!(!shape.contains("203.0.113.9"));
        assert!(!shape.contains("secret"));
        assert!(!shape.contains("streamid=video/0"));
        let disabled = describe_media_shape("v=0\r\nm=video 0 RTP/SAVPF 96\r\n");
        assert!(disabled.contains("video/0"));
        assert!(disabled.contains("video_control=absent"));
    }

    #[test]
    fn endpoint_urls_preserve_one_ipv6_bracket_pair_and_the_selected_port() {
        for (endpoint, port) in [
            ("rtsps://[2001:4860:4860::8888]:48322/session", 48322),
            ("rtsps://[2001:4860:4860::8888]/session", 322),
        ] {
            let (wss, target) = rtsp_endpoint_urls(endpoint).unwrap();
            let authority = format!("[2001:4860:4860::8888]:{port}");
            assert_eq!(wss, format!("wss://{authority}/rtsp"));
            assert_eq!(target, format!("rtsps://{authority}"));
            let request = wss.into_client_request().unwrap();
            assert_eq!(request.headers()["host"], authority);
            assert_eq!(request.uri().host(), Some("[2001:4860:4860::8888]"));
            assert_eq!(request.uri().port_u16(), Some(port));
            let target = target.parse::<Uri>().unwrap();
            assert_eq!(target.authority().unwrap().as_str(), authority);
        }
    }

    #[test]
    fn endpoint_urls_preserve_dns_and_ipv4_behavior() {
        for (endpoint, authority) in [
            (
                "rtsps://seat.nvidiagrid.net/session",
                "seat.nvidiagrid.net:322",
            ),
            ("rtsps://8.8.8.8:48322/session", "8.8.8.8:48322"),
        ] {
            assert_eq!(
                rtsp_endpoint_urls(endpoint).unwrap(),
                (
                    format!("wss://{authority}/rtsp"),
                    format!("rtsps://{authority}"),
                )
            );
        }
    }

    #[test]
    fn endpoint_urls_reject_malformed_authorities_not_partner_or_private_hosts() {
        for endpoint in [
            "rtsps://[seat.nvidiagrid.net]:322",
            "rtsps://[8.8.8.8]:322",
            "rtsps://[[2001:4860:4860::8888]]:322",
            "https://partner.example:322",
            "rtsp://partner.example:322",
            "rtsps://user@partner.example:322",
            "rtsps://partner.example:65536",
            "rtsps://partner.example:0",
            "rtsps://partner.example:wrong",
            "rtsps://partner.example:322/#fragment",
        ] {
            assert!(rtsp_endpoint_urls(endpoint).is_err(), "{endpoint}");
        }
        for endpoint in [
            "rtsps://partner.example:322",
            "rtsps://127.0.0.1:322",
            "rtsps://10.0.0.8:322",
            "rtsps://[::1]:322",
        ] {
            assert!(rtsp_endpoint_urls(endpoint).is_ok(), "{endpoint}");
        }
        assert_eq!(
            rtsp_endpoint_urls("rtsp://partner.example:322")
                .unwrap_err()
                .code,
            "nvst-raw-rtsp-unsupported"
        );
    }

    #[test]
    fn endpoint_urls_accept_ipv4_mapped_public_addresses() {
        assert_eq!(
            rtsp_endpoint_urls("rtsps://[::ffff:8.8.8.8]:48322/session").unwrap(),
            (
                "wss://[::ffff:8.8.8.8]:48322/rtsp".to_owned(),
                "rtsps://[::ffff:8.8.8.8]:48322".to_owned(),
            )
        );
    }

    #[test]
    fn ping_identity_increment_preserves_width_and_carry() {
        assert_eq!(increment_hex("00ff").as_deref(), Some("0100"));
        assert_eq!(increment_hex("ffff").as_deref(), Some("10000"));
        assert_eq!(increment_hex("PING"), None);
        assert_eq!(
            resolve_remote_ufrag(Some("00ff"), Some("described"), 6).as_deref(),
            Some("0100")
        );
        assert_eq!(
            resolve_remote_ufrag(None, Some("described"), 5).as_deref(),
            Some("described")
        );
    }

    #[test]
    fn bundle_natt_username_uses_server_ufrag_and_decimal_bundle_port() {
        let sdp = "a=x-nv-general.iceUsernameFragment:server\r\na=x-nv-general.serverBundlePort:47998\r\n;;a=x-nv-general.serverBundlePort:47999\r\n||a=x-nv-general.serverBundlePort:1234\r\n";
        assert_eq!(bundle_natt_username(sdp).as_deref(), Some("server47999"));
        assert_ne!(
            bundle_natt_username(sdp).as_deref(),
            increment_hex("server47998").as_deref()
        );
        assert_eq!(bundle_natt_username("a=x-nv-general.iceUserNameFragmentV2:next\r\na=x-nv-general.serverBundlePort:5004\r\n").as_deref(), Some("next5004"));
        for invalid in [
            "a=x-nv-general.iceUsernameFragment:server\r\n",
            "a=x-nv-general.serverBundlePort:47999\r\n",
            "a=x-nv-general.iceUsernameFragment:server\r\na=x-nv-general.serverBundlePort:0\r\n",
            "a=x-nv-general.iceUsernameFragment:server\r\na=x-nv-general.serverBundlePort:65536\r\n",
            "a=x-nv-general.iceUsernameFragment:bad:value\r\na=x-nv-general.serverBundlePort:47999\r\n",
        ] {
            assert_eq!(bundle_natt_username(invalid), None);
        }
        let oversized = format!(
            "a=x-nv-general.iceUsernameFragment:{}\r\na=x-nv-general.serverBundlePort:47999\r\n",
            "a".repeat(252)
        );
        assert_eq!(bundle_natt_username(&oversized), None);
    }

    #[test]
    fn bundle_natt_handoff_keeps_the_ice_fragment_distinct() {
        let describe = "v=0\r\na=x-nv-general.iceUsernameFragment:a1b2\r\na=x-nv-general.serverBundlePort:48000\r\n;;a=x-nv-general.serverBundlePort:48001\r\n||a=x-nv-general.serverBundlePort:47000\r\n";
        let ice_fragment = resolve_remote_ufrag(
            Some("a1b247998"),
            sdp_attribute(describe, "general.iceUsernameFragment").as_deref(),
            6,
        )
        .unwrap();
        let mut handoff = json!({"remoteIceUsernameFragment": ice_fragment});
        set_bundle_natt_username(&mut handoff, describe, 6);
        assert_eq!(handoff["remoteIceUsernameFragment"], "a1b247999");
        assert_eq!(handoff["bundleNattRemoteUsername"], "a1b248001");

        let mut missing = json!({"remoteIceUsernameFragment": "a1b247999"});
        set_bundle_natt_username(
            &mut missing,
            "a=x-nv-general.iceUsernameFragment:a1b2\r\n",
            6,
        );
        assert_eq!(missing["remoteIceUsernameFragment"], "a1b247999");
        assert!(missing.get("bundleNattRemoteUsername").is_none());

        let mut legacy = json!({"remoteIceUsernameFragment": "a1b2"});
        set_bundle_natt_username(&mut legacy, describe, 5);
        assert_eq!(legacy["remoteIceUsernameFragment"], "a1b2");
        assert!(legacy.get("bundleNattRemoteUsername").is_none());
    }
}
