use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use opennow_streamer_protocol::SessionContext;
use opennow_streamer_transport::ReservedNvstBundle;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{HeaderValue, Uri};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket, connect};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);
const MAX_STREAM_BITRATE_MBPS: u64 = 200;
// GeForce NOW 2.0.87.131 reports video[0].timeoutLengthMs=8000 and
// video[0].sendFrameTimeoutMs=7000. Waiting sixty seconds left a dead Mjolnir media leg on screen
// while audio/control remained alive; use the official receiver timeout so the existing bounded
// transport recovery runs promptly.
const VIDEO_TIMEOUT_MS: u64 = 8_000;

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

struct RtspClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    cseq: u64,
    buffer: String,
}

impl RtspClient {
    fn connect(endpoint: &str, session_id: &str) -> Result<(Self, String), NvstRtspError> {
        let translated = endpoint
            .replacen("rtsps://", "https://", 1)
            .replacen("rtsp://", "http://", 1);
        let parsed = translated
            .parse::<Uri>()
            .map_err(|_| NvstRtspError::new("invalid-rtsps-endpoint", "Invalid RTSPS endpoint"))?;
        let host = parsed.host().ok_or_else(|| {
            NvstRtspError::new("invalid-rtsps-endpoint", "RTSPS endpoint has no host")
        })?;
        if !trusted_nvst_host(host) {
            return Err(NvstRtspError::new(
                "untrusted-rtsps-endpoint",
                "Refusing an untrusted RTSPS endpoint",
            ));
        }
        let port = parsed.port_u16().unwrap_or(322);
        let authority_host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let wss = format!("wss://{authority_host}:{port}/rtsp");
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
        let (mut socket, _) = connect(request).map_err(|error| {
            let failure = rtsp_connect_error(&error);
            opennow_streamer_protocol::log::log_line("WARN", "rtsp", &failure.message);
            failure
        })?;
        set_read_timeout(&mut socket, REQUEST_TIMEOUT);
        Ok((
            Self {
                socket,
                cseq: 0,
                buffer: String::new(),
            },
            format!("rtsps://{host}:{port}"),
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
        self.cseq += 1;
        eprintln!(
            "NVST RTSPS request method={method} cseq={} uri={uri}",
            self.cseq
        );
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
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(response) = take_rtsp_response(&mut self.buffer, self.cseq)? {
                eprintln!(
                    "NVST RTSPS response method={method} cseq={} status={} {} responseCseq={} requestId={}",
                    self.cseq,
                    response.status,
                    response.status_text,
                    response
                        .headers
                        .get("cseq")
                        .map(String::as_str)
                        .unwrap_or("missing"),
                    response
                        .headers
                        .get("request-id")
                        .map(String::as_str)
                        .unwrap_or("missing")
                );
                return Ok(response);
            }
            if Instant::now() >= deadline {
                return Err(NvstRtspError::new(
                    "nvst-rtsp-timeout",
                    format!("RTSPS {method} timed out"),
                ));
            }
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
}

fn rtsp_connect_error(error: &tungstenite::Error) -> NvstRtspError {
    if let tungstenite::Error::Http(response) = error {
        let status = response.status().as_u16();
        return NvstRtspError::new(
            if status == 503 {
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
    client: Option<RtspClient>,
    target: String,
    common_headers: Vec<(&'static str, String)>,
    rtsp_session: String,
    disable_play: bool,
    announce_body: String,
    announced: bool,
    owns_session: bool,
    pub handoff: Value,
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
        ActiveNvstRtspSession::spawn(
            client,
            self.target.clone(),
            self.common_headers.clone(),
            self.rtsp_session.clone(),
        )
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
        set_read_timeout(&mut client.socket, Duration::from_millis(100));
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
    fn spawn(
        mut client: RtspClient,
        target: String,
        common_headers: Vec<(&'static str, String)>,
        rtsp_session: String,
    ) -> Result<Self, NvstRtspError> {
        set_read_timeout(&mut client.socket, Duration::from_millis(100));
        let (control, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("opennow-nvst-rtsps".to_owned())
            .spawn(move || {
                let mut last_ping = Instant::now();
                loop {
                    if receiver.try_recv().is_ok() {
                        let mut headers = common_headers.clone();
                        headers.push(("Session", rtsp_session.clone()));
                        let _ = client.request_with_timeout(
                            "TEARDOWN",
                            &target,
                            &headers,
                            "",
                            Duration::from_secs(1),
                        );
                        let _ = client.socket.close(None);
                        break;
                    }
                    if last_ping.elapsed() >= KEEPALIVE_INTERVAL {
                        if client
                            .socket
                            .send(Message::Ping(Vec::new().into()))
                            .is_err()
                        {
                            break;
                        }
                        last_ping = Instant::now();
                    }
                    match client.socket.read() {
                        Ok(Message::Ping(bytes)) => {
                            let _ = client.socket.send(Message::Pong(bytes));
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
                }
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
    let endpoint = context
        .session
        .extra
        .get("rtspsEndpoints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|value| value.starts_with("rtsps://") || value.starts_with("rtsp://"))
        .ok_or_else(|| {
            NvstRtspError::new(
                "missing-rtsps-endpoint",
                "CloudMatch did not provide an RTSPS endpoint for NVST",
            )
        })?;
    let session_id = context.session.session_id.trim();
    if session_id.is_empty() {
        return Err(NvstRtspError::new(
            "invalid-session",
            "NVST negotiation requires a session ID",
        ));
    }

    let client_port = bundle
        .local_addr()
        .map_err(|error| NvstRtspError::new("nvst-bind-failed", error.to_string()))?
        .port();
    let mjolnir_port = bundle
        .mjolnir_local_addr()
        .map_err(|error| NvstRtspError::new("nvst-bind-failed", error.to_string()))?
        .port();
    let identity = bundle.identity();

    let (mut client, target) = RtspClient::connect(endpoint, session_id)?;
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
    let video_setup = official_video_setup_control(&video_control);
    let described_ping_version = sdp_attribute(&describe.body, "general.pingVersion")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(6);
    let remote_ufrag = sdp_attribute(&describe.body, "general.iceUserNameFragmentV2")
        .or_else(|| sdp_attribute(&describe.body, "general.iceUsernameFragment"));
    let remote_password = sdp_attribute(&describe.body, "general.icePasswordV2")
        .or_else(|| sdp_attribute(&describe.body, "general.iceUsernamePwd"));
    let remote_fingerprint = sdp_attribute(&describe.body, "general.dtlsFingerprintV2")
        .or_else(|| sdp_attribute(&describe.body, "general.dtlsFingerprint"));
    let disable_play = sdp_attribute(&describe.body, "general.disablePlay").as_deref() != Some("0");
    let native_bundle = sdp_attribute(&describe.body, "general.nativeRtcOnBundlePort");
    if native_bundle.as_deref() != Some("1") {
        return Err(NvstRtspError::new(
            "nvst-legacy-transport-unsupported",
            "This seat requires the retired multi-socket NVST transport",
        ));
    }
    let rtcp_on_sctp = sdp_attribute(&describe.body, "general.rtcpOnSctp").as_deref() == Some("1");

    let mut setup_headers = common_headers.clone();
    setup_headers.push(("Session", rtsp_session.clone()));
    setup_headers.push(("x-nv-ping", described_ping_version.to_string()));
    setup_headers.push(("Transport", String::new()));
    let setup = client.request("SETUP", &video_setup, &setup_headers, "")?;
    ensure_rtsp_ok("SETUP", &setup)?;
    let transport = header_value(&setup, "transport").unwrap_or_default();
    let (video_peer_ip, video_peer_port) = parse_video_peer(transport).ok_or_else(|| {
        NvstRtspError::new(
            "missing-video-peer",
            "SETUP did not return the NVST video peer",
        )
    })?;
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
        "packetSize":1280,
        "mjolnirUdpPort":mjolnir_port,
        "videoPeerIp":video_peer_ip,
        "videoPeerPort":video_peer_port,
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
        "codec":codec,
        "audioTrack":{"payloadType":111,"codec":"opus","clockRateHz":48000,"channels":2,"mid":"0"},
        "timeoutMs":VIDEO_TIMEOUT_MS
    });
    if let Some(media) = context.session.media_connection_info.as_ref() {
        handoff["bundlePeerIp"] = json!(media.ip);
        handoff["bundlePeerPort"] = json!(media.port);
    }

    let announce_body = build_announce(
        context,
        AnnounceParams {
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
            rtcp_on_sctp,
        },
    );
    Ok(PreparedNvstRtspSession {
        client: Some(client),
        target,
        common_headers,
        rtsp_session,
        disable_play,
        announce_body,
        announced: false,
        owns_session: true,
        handoff,
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
    key: &'a str,
    key_id: u32,
    port: u16,
    address: &'a str,
    ufrag: &'a str,
    password: &'a str,
    fingerprint: &'a str,
    video_port: u16,
    rtcp_on_sctp: bool,
}

fn build_announce(context: &SessionContext, params: AnnounceParams<'_>) -> String {
    let (width, height) = resolution(context);
    let fps = negotiated_fps(context);
    let bitrate = context
        .settings
        .get("maxBitrateMbps")
        .and_then(Value::as_u64)
        .unwrap_or(75)
        .clamp(1, MAX_STREAM_BITRATE_MBPS)
        * 1000;
    let codec = negotiated_codec(context);
    let format = if codec.eq_ignore_ascii_case("AV1") {
        2
    } else if codec.eq_ignore_ascii_case("H265") || codec.eq_ignore_ascii_case("HEVC") {
        1
    } else {
        0
    };
    let (bit_depth, chroma_format) = negotiated_color_format(context, &codec);
    let mut lines = vec![
        "v=0".to_owned(),
        "o=unknown 0 14 IN IPv4 127.0.0.1".to_owned(),
        "s=NVIDIA Streaming Client".to_owned(),
        format!("a=x-nv-video[0].clientViewportWd:{width}"),
        format!("a=x-nv-video[0].clientViewportHt:{height}"),
        "a=x-nv-video[0].videoSplitEncodeStripsPerFrame:64".to_owned(),
        "a=x-nv-video[0].updateSplitEncodeStateDynamically:1".to_owned(),
        "a=x-nv-video[0].packetSize:1280".to_owned(),
        "a=x-nv-video[0].enableRtpNack:1".to_owned(),
        "a=x-nv-video[0].rtpNackQueueLength:2048".to_owned(),
        "a=x-nv-video[0].rtpNackQueueMaxPackets:1024".to_owned(),
        "a=x-nv-video[0].rtpNackMaxPacketCount:64".to_owned(),
        "a=x-nv-video[0].framePacing.mode:1".to_owned(),
        "a=x-nv-video[0].framePacing.feedbackMode:1".to_owned(),
        "a=x-nv-video[0].framePacing.pid.minTargetFrameTimeUs:7936".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.spatialAQSetting:7".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.temporalAQSetting:0".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.spatialAQStrength:12".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.qpThresholdAdjPercent:2".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.saqAdaptMinQpThresholdPercent:40".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.saqAdaptMaxQpThresholdPercent:100".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.saqAdaptDecayStrengthX100:250".to_owned(),
        "a=x-nv-video[0].adaptiveQuantization.perfAdjEnablement:1".to_owned(),
        "a=x-nv-video[0].enableAv1RcPrecisionFactor:1".to_owned(),
        // Match the native Geronimo/NVST profile. CloudMatch wire values are
        // 0/1, while ANNOUNCE carries the real 8/10-bit depth and 0/1 chroma.
        "a=x-nv-video[0].maxNumReferenceFrames:0".to_owned(),
        "a=x-nv-video[0].dynamicRangeMode:0".to_owned(),
        format!("a=x-nv-video[0].bitDepth:{bit_depth}"),
        format!("a=x-nv-video[0].chromaFormat:{chroma_format}"),
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
        "a=x-nv-vqos[0].grc.enable:7".to_owned(),
        "a=x-nv-vqos[0].drc.enable:0".to_owned(),
        "a=x-nv-vqos[0].dfc.adjustResAndFps:0".to_owned(),
        "a=x-nv-vqos[0].calculateAvgVideoStreamingBitrate:1".to_owned(),
        format!("a=x-nv-vqos[0].bw.maximumBitrateKbps:{bitrate}"),
        "a=x-nv-vqos[0].bw.minimumBitrateKbps:1000".to_owned(),
        "a=x-nv-vqos[0].drc.bitrateIirFilterFactor:128".to_owned(),
        "a=x-nv-vqos[0].resControl.bitrateIirFilterFactor:128".to_owned(),
        "a=x-nv-vqos[0].dynamicStreamingMode:0".to_owned(),
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
        "a=x-nv-aqos.enableRedundancy:1".to_owned(),
        "a=x-nv-aqos.redundancyLevel:2".to_owned(),
        "a=x-nv-bwe.useOwdCongestionControl:1".to_owned(),
        "a=x-nv-general.rtspWebSocketPerConnection:1".to_owned(),
        "a=x-nv-general.enetControlChannel.mtuSize:1191".to_owned(),
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
        "a=x-nv-general.rtcMicOnNativeBundle:1".to_owned(),
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
        "t=0 0".to_owned(),
        format!("m=video {}", params.video_port),
        "c=IN IP4 0.0.0.0".to_owned(),
        "i=DeviceString, DeviceName".to_owned(),
        String::new(),
    ];
    if format != 2 {
        lines.insert(
            26,
            format!("a=x-nv-clientSupportHevc:{}", u8::from(format == 1)),
        );
    }
    lines.join("\r\n")
}

fn negotiated_color_format(context: &SessionContext, codec: &str) -> (u8, u8) {
    if codec.eq_ignore_ascii_case("H264") {
        return (8, 0);
    }
    let quality = context
        .session
        .extra
        .get("negotiatedStreamProfile")
        .and_then(|profile| profile.get("colorQuality"))
        .and_then(Value::as_str)
        .or_else(|| context.settings.get("colorQuality").and_then(Value::as_str))
        .unwrap_or("8bit_420")
        .to_ascii_lowercase();
    let bit_depth = if quality.starts_with("10bit") { 10 } else { 8 };
    let chroma_format = if codec.eq_ignore_ascii_case("AV1") {
        0
    } else if quality.ends_with("444") {
        1
    } else {
        0
    };
    (bit_depth, chroma_format)
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
        .clamp(30, 240)
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
    let total = header_end + separator + content_length;
    if buffer.len() < total {
        return Ok(None);
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
            "nvst-rtsp-failed",
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

fn trusted_nvst_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "nvidiagrid.net" || host.ends_with(".nvidiagrid.net") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| match ip {
        IpAddr::V4(ip) => {
            !ip.is_private() && !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
        }
        IpAddr::V6(ip) => !ip.is_loopback() && !ip.is_unicast_link_local() && !ip.is_unspecified(),
    })
}

fn media_control(sdp: &str, kind: &str) -> Option<String> {
    let mut current = "";
    for line in sdp.lines().map(str::trim) {
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

fn sdp_attribute(sdp: &str, name: &str) -> Option<String> {
    let candidates = [
        format!("a=x-nv-{name}:").to_ascii_lowercase(),
        format!("a={name}:").to_ascii_lowercase(),
    ];
    sdp.lines().map(str::trim).find_map(|line| {
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
}

fn official_video_setup_control(control: &str) -> String {
    let lower = control.to_ascii_lowercase();
    if lower.starts_with("streamid=video/") && control.matches('/').count() == 1 {
        format!("{control}/0")
    } else {
        control.to_owned()
    }
}

fn parse_video_peer(transport: &str) -> Option<(String, u16)> {
    let mut ip = None;
    let mut port = None;
    for part in transport.split([';', ',']) {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case("source") {
            ip = Some(value.trim().to_owned());
        } else if name.eq_ignore_ascii_case("X-GS-ServerPort") {
            port = value.trim().split('-').next()?.parse().ok();
        }
    }
    Some((ip?, port?))
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

fn set_read_timeout(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, timeout: Duration) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.get_mut().set_read_timeout(Some(timeout));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn installs_a_process_level_tls_crypto_provider() {
        ensure_tls_crypto_provider().expect("TLS provider");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn owned_announce_matches_current_official_bundle_baseline() {
        let value = context();
        let sdp = build_announce(
            &value,
            AnnounceParams {
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                rtcp_on_sctp: true,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].maxFPS:120"));
        assert!(sdp.contains("a=x-nv-video[0].bitDepth:10"));
        assert!(sdp.contains("a=x-nv-video[0].chromaFormat:0"));
        assert!(sdp.contains("a=x-nv-video[0].encoderCscMode:2"));
        assert!(sdp.contains("a=x-nv-vqos[0].bitStreamFormat:2"));
        assert!(sdp.contains("a=x-nv-general.clientBundlePort:49006"));
        assert!(sdp.contains("a=x-nv-general.rtcDataChannelOnNativeBundle:1"));
        assert!(sdp.contains("a=x-nv-runtime.encryptionKey:"));
        assert!(sdp.contains("m=video 5004"));
    }

    #[test]
    fn owned_announce_preserves_the_configured_200_mbps_ceiling() {
        let mut value = context();
        value.settings["maxBitrateMbps"] = json!(200);
        let sdp = build_announce(
            &value,
            AnnounceParams {
                key: &"01".repeat(32),
                key_id: 7,
                port: 49006,
                address: "192.0.2.10",
                ufrag: "abcd",
                password: "abcdefghijklmnopqrstuv",
                fingerprint: "AA:BB",
                video_port: 5004,
                rtcp_on_sctp: true,
            },
        );
        assert!(sdp.contains("a=x-nv-video[0].initialBitrateKbps:200000"));
        assert!(sdp.contains("a=x-nv-video[0].initialPeakBitrateKbps:200000"));
        assert!(sdp.contains("a=x-nv-vqos[0].bw.maximumBitrateKbps:200000"));
    }

    #[test]
    fn h264_announce_stays_eight_bit_420() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H264");
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(negotiated_color_format(&value, "H264"), (8, 0));
    }

    #[test]
    fn av1_announce_stays_420_but_preserves_ten_bit_depth() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(negotiated_color_format(&value, "AV1"), (10, 0));
    }

    #[test]
    fn h265_announce_supports_ten_bit_444() {
        let mut value = context();
        value.session.extra["negotiatedStreamProfile"]["codec"] = json!("H265");
        value.session.extra["negotiatedStreamProfile"]["colorQuality"] = json!("10bit_444");
        assert_eq!(negotiated_color_format(&value, "H265"), (10, 1));
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
    fn endpoint_policy_rejects_local_and_private_addresses() {
        assert!(trusted_nvst_host("seat.nvidiagrid.net"));
        assert!(trusted_nvst_host("8.8.8.8"));
        assert!(!trusted_nvst_host("localhost"));
        assert!(!trusted_nvst_host("127.0.0.1"));
        assert!(!trusted_nvst_host("10.0.0.8"));
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
}
