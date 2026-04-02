use std::{
    collections::HashSet,
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{mpsc::Sender, Arc},
    thread,
};

use anyhow::{anyhow, Context};
use bytes::Bytes;
use opus::{Channels, Decoder as OpusDecoder};
use rtp::{
    codecs::{
        h264::H264Packet,
        h265::{H265Packet, H265Payload},
    },
    packetizer::Depacketizer,
};
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;

use crate::messages::StreamerMessage;

#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub y_plane: Vec<u8>,
    pub u_plane: Vec<u8>,
    pub v_plane: Vec<u8>,
}

#[derive(Clone)]
pub struct AudioFrame {
    pub samples: Vec<i16>,
    pub channels: u8,
    pub sample_rate: u32,
}

#[derive(Clone)]
pub enum MediaEvent {
    Video(VideoFrame),
    Audio(AudioFrame),
}

#[derive(Clone)]
pub struct MediaPipeline {
    event_tx: Sender<MediaEvent>,
    log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
    video_settings: VideoSettings,
}

#[derive(Clone)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

impl MediaPipeline {
    pub fn new(
        event_tx: Sender<MediaEvent>,
        log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
        video_settings: VideoSettings,
    ) -> Self {
        Self {
            event_tx,
            log_tx,
            video_settings,
        }
    }

    pub async fn attach_video_track(
        &self,
        track: Arc<webrtc::track::track_remote::TrackRemote>,
    ) -> anyhow::Result<()> {
        let codec = track.codec().capability;
        let mime = codec.mime_type.to_lowercase();
        let event_tx = self.event_tx.clone();
        let log_tx = self.log_tx.clone();
        let settings = self.video_settings.clone();
        tokio::spawn(async move {
            if let Err(error) =
                run_video_track(track, codec, settings, event_tx, log_tx.clone()).await
            {
                let _ = log_tx
                    .send(StreamerMessage::Error {
                        message: format!("video pipeline failed: {error:#}"),
                    })
                    .await;
            }
        });
        let _ = self
            .log_tx
            .send(StreamerMessage::Log {
                level: "info".into(),
                message: format!("attached video track {mime}"),
            })
            .await;
        Ok(())
    }

    pub async fn attach_audio_track(
        &self,
        track: Arc<webrtc::track::track_remote::TrackRemote>,
    ) -> anyhow::Result<()> {
        let codec = track.codec().capability;
        let event_tx = self.event_tx.clone();
        let log_tx = self.log_tx.clone();
        tokio::spawn(async move {
            if let Err(error) = run_audio_track(track, codec, event_tx, log_tx.clone()).await {
                let _ = log_tx
                    .send(StreamerMessage::Error {
                        message: format!("audio pipeline failed: {error:#}"),
                    })
                    .await;
            }
        });
        Ok(())
    }
}

async fn run_video_track(
    track: Arc<webrtc::track::track_remote::TrackRemote>,
    codec: RTCRtpCodecCapability,
    settings: VideoSettings,
    event_tx: Sender<MediaEvent>,
    log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
) -> anyhow::Result<()> {
    let codec_name = codec.mime_type.to_lowercase();
    let ffmpeg_demuxer = if codec_name.contains("h265") || codec_name.contains("hevc") {
        "hevc"
    } else if codec_name.contains("h264") {
        "h264"
    } else {
        return Err(anyhow!(
            "unsupported video codec for MVP decode path: {}",
            codec.mime_type
        ));
    };

    let mut decoder = FfmpegVideoDecoder::spawn(
        ffmpeg_demuxer,
        settings.width,
        settings.height,
        event_tx,
        log_tx.clone(),
    )?;
    let mut h264 = H264Packet::default();
    let mut h265 = H265Assembler::default();
    loop {
        let (packet, _) = track.read_rtp().await.context("read_rtp video")?;
        let payload = if ffmpeg_demuxer == "h264" {
            match h264.depacketize(&packet.payload) {
                Ok(bytes) if !bytes.is_empty() => bytes.as_ref().to_vec(),
                Ok(_) => Vec::new(),
                Err(error) => {
                    let _ = log_tx
                        .send(StreamerMessage::Log {
                            level: "warn".into(),
                            message: format!("h264 depacketize: {error}"),
                        })
                        .await;
                    Vec::new()
                }
            }
        } else {
            match h265.push(packet.payload.clone()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let _ = log_tx
                        .send(StreamerMessage::Log {
                            level: "warn".into(),
                            message: format!("h265 depacketize: {error}"),
                        })
                        .await;
                    Vec::new()
                }
            }
        };
        if !payload.is_empty() {
            decoder.write(&payload)?;
        }
    }
}

async fn run_audio_track(
    track: Arc<webrtc::track::track_remote::TrackRemote>,
    codec: RTCRtpCodecCapability,
    event_tx: Sender<MediaEvent>,
    _log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
) -> anyhow::Result<()> {
    if !codec.mime_type.to_lowercase().contains("opus") {
        return Err(anyhow!(
            "unsupported audio codec for MVP decode path: {}",
            codec.mime_type
        ));
    }
    let sample_rate = codec.clock_rate.max(48_000);
    let channels = if codec.channels == 0 {
        2
    } else {
        codec.channels as usize
    };
    let mut depacketizer = rtp::codecs::opus::OpusPacket::default();
    let mut decoder = OpusDecoder::new(
        sample_rate,
        if channels > 1 {
            Channels::Stereo
        } else {
            Channels::Mono
        },
    )?;
    let mut pcm = vec![0_i16; 960 * channels * 6];
    loop {
        let (packet, _) = track.read_rtp().await.context("read_rtp audio")?;
        let opus = depacketizer
            .depacketize(&packet.payload)
            .context("depacketize opus")?;
        let frame_samples = decoder
            .decode(&opus, &mut pcm, false)
            .context("decode opus")?;
        if frame_samples > 0 {
            let used = frame_samples * channels;
            let samples = pcm[..used].to_vec();
            let _ = event_tx.send(MediaEvent::Audio(AudioFrame {
                samples,
                channels: channels as u8,
                sample_rate,
            }));
        }
    }
}

struct FfmpegVideoDecoder {
    ffmpeg: PathBuf,
    demuxer: String,
    width: u32,
    height: u32,
    event_tx: Sender<MediaEvent>,
    log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
    candidates: Vec<DecoderCandidate>,
    current_index: usize,
    stdin: ChildStdin,
    child: Child,
}

impl FfmpegVideoDecoder {
    fn spawn(
        demuxer: &str,
        width: u32,
        height: u32,
        event_tx: Sender<MediaEvent>,
        log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
    ) -> anyhow::Result<Self> {
        let ffmpeg = resolve_ffmpeg_binary()?;
        let candidates = build_decoder_candidates(&ffmpeg, demuxer);
        let (child, stdin) = spawn_decoder_process(
            &ffmpeg,
            &candidates[0],
            demuxer,
            width,
            height,
            event_tx.clone(),
            log_tx.clone(),
        )?;
        Ok(Self {
            ffmpeg,
            demuxer: demuxer.to_string(),
            width,
            height,
            event_tx,
            log_tx,
            candidates,
            current_index: 0,
            stdin,
            child,
        })
    }

    fn write(&mut self, payload: &[u8]) -> anyhow::Result<()> {
        loop {
            self.ensure_backend()?;
            match self.stdin.write_all(payload) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    self.log_blocking(
                        "warn",
                        format!(
                            "decoder backend {} write failed: {error}",
                            self.candidates[self.current_index].name
                        ),
                    );
                    if !self.advance_backend(None)? {
                        return Err(error).context("write ffmpeg stdin");
                    }
                }
            }
        }
    }

    fn ensure_backend(&mut self) -> anyhow::Result<()> {
        if let Some(status) = self.child.try_wait().context("try_wait ffmpeg decoder")? {
            self.log_blocking(
                "warn",
                format!(
                    "decoder backend {} exited early with {}",
                    self.candidates[self.current_index].name,
                    format_exit_status(status)
                ),
            );
            if !self.advance_backend(Some(status))? {
                return Err(anyhow!("all decoder backends exhausted"));
            }
        }
        Ok(())
    }

    fn advance_backend(&mut self, _status: Option<ExitStatus>) -> anyhow::Result<bool> {
        while self.current_index + 1 < self.candidates.len() {
            self.current_index += 1;
            let candidate = self.candidates[self.current_index].clone();
            match spawn_decoder_process(
                &self.ffmpeg,
                &candidate,
                &self.demuxer,
                self.width,
                self.height,
                self.event_tx.clone(),
                self.log_tx.clone(),
            ) {
                Ok((child, stdin)) => {
                    self.child = child;
                    self.stdin = stdin;
                    return Ok(true);
                }
                Err(error) => {
                    self.log_blocking(
                        "warn",
                        format!(
                            "decoder backend {} failed to start: {error:#}",
                            candidate.name
                        ),
                    );
                }
            }
        }
        Ok(false)
    }

    fn log_blocking(&self, level: &str, message: String) {
        send_streamer_log(&self.log_tx, level, message);
    }
}

#[derive(Clone)]
struct DecoderCandidate {
    name: String,
    input_args: Vec<String>,
    output_args: Vec<String>,
}

fn spawn_decoder_process(
    ffmpeg: &Path,
    candidate: &DecoderCandidate,
    demuxer: &str,
    width: u32,
    height: u32,
    event_tx: Sender<MediaEvent>,
    log_tx: tokio::sync::mpsc::Sender<StreamerMessage>,
) -> anyhow::Result<(Child, ChildStdin)> {
    let mut command = Command::new(ffmpeg);
    command
        .args(&candidate.input_args)
        .arg("-f")
        .arg(demuxer)
        .arg("-i")
        .arg("pipe:0")
        .args(&candidate.output_args)
        .arg("-an")
        .arg("-sn")
        .arg("-dn")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-f")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("spawn ffmpeg decoder backend {}", candidate.name))?;

    let y_size = (width * height) as usize;
    let uv_size = ((width / 2) * (height / 2)) as usize;
    let frame_size = y_size + uv_size + uv_size;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("missing ffmpeg stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("missing ffmpeg stderr"))?;
    let backend_name = candidate.name.clone();
    send_streamer_log(
        &log_tx,
        "info",
        format!("starting decoder backend {}", backend_name),
    );
    thread::spawn(move || {
        let mut buffer = vec![0_u8; frame_size];
        while stdout.read_exact(&mut buffer).is_ok() {
            let y_plane = buffer[..y_size].to_vec();
            let u_plane = buffer[y_size..(y_size + uv_size)].to_vec();
            let v_plane = buffer[(y_size + uv_size)..].to_vec();
            let _ = event_tx.send(MediaEvent::Video(VideoFrame {
                width,
                height,
                y_plane,
                u_plane,
                v_plane,
            }));
        }
    });
    thread::spawn(move || {
        let mut stderr_buf = String::new();
        let _ = stderr.read_to_string(&mut stderr_buf);
        if !stderr_buf.trim().is_empty() {
            send_streamer_log(
                &log_tx,
                "stderr",
                format!("[{}] {}", backend_name, stderr_buf.trim()),
            );
        }
    });
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("missing ffmpeg stdin"))?;
    Ok((child, stdin))
}

fn send_streamer_log(
    log_tx: &tokio::sync::mpsc::Sender<StreamerMessage>,
    level: &str,
    message: String,
) {
    let _ = log_tx.try_send(StreamerMessage::Log {
        level: level.to_string(),
        message,
    });
}

fn build_decoder_candidates(ffmpeg: &Path, demuxer: &str) -> Vec<DecoderCandidate> {
    let hwaccels = query_hwaccels(ffmpeg);
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if hwaccels.contains("d3d11va") || hwaccels.contains("qsv") || hwaccels.is_empty() {
            candidates.push(DecoderCandidate {
                name: "windows-d3d11va-copyback".into(),
                input_args: base_ffmpeg_input_args(&[
                    "-hwaccel",
                    "d3d11va",
                    "-hwaccel_output_format",
                    "d3d11",
                ]),
                output_args: ffmpeg_output_args(&["-vf", "hwdownload,format=yuv420p"]),
            });
        }
        if hwaccels.contains("qsv") {
            candidates.push(DecoderCandidate {
                name: "windows-qsv-copyback".into(),
                input_args: base_ffmpeg_input_args(&[
                    "-hwaccel",
                    "qsv",
                    "-hwaccel_output_format",
                    "qsv",
                ]),
                output_args: ffmpeg_output_args(&["-vf", "hwdownload,format=yuv420p"]),
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push(DecoderCandidate {
            name: "macos-videotoolbox-copyback".into(),
            input_args: base_ffmpeg_input_args(&["-hwaccel", "videotoolbox"]),
            output_args: ffmpeg_output_args(&["-vf", "format=yuv420p"]),
        });
    }

    #[cfg(target_os = "linux")]
    {
        if hwaccels.contains("vulkan") {
            candidates.push(DecoderCandidate {
                name: "linux-vulkan-copyback".into(),
                input_args: base_ffmpeg_input_args(&[
                    "-init_hw_device",
                    "vulkan=vk:0",
                    "-filter_hw_device",
                    "vk",
                    "-hwaccel",
                    "vulkan",
                    "-hwaccel_output_format",
                    "vulkan",
                    "-extra_hw_frames",
                    "8",
                ]),
                output_args: ffmpeg_output_args(&["-vf", "hwdownload,format=yuv420p"]),
            });
        }
        if hwaccels.contains("vaapi") {
            if let Some(render_node) = linux_render_node() {
                candidates.push(DecoderCandidate {
                    name: "linux-vaapi-copyback".into(),
                    input_args: base_ffmpeg_input_args(&[
                        "-vaapi_device",
                        render_node.as_str(),
                        "-hwaccel",
                        "vaapi",
                        "-hwaccel_output_format",
                        "vaapi",
                        "-extra_hw_frames",
                        "8",
                    ]),
                    output_args: ffmpeg_output_args(&["-vf", "hwdownload,format=yuv420p"]),
                });
            }
        }
        if hwaccels.contains("cuda") {
            candidates.push(DecoderCandidate {
                name: "linux-cuda-copyback".into(),
                input_args: base_ffmpeg_input_args(&[
                    "-hwaccel",
                    "cuda",
                    "-hwaccel_output_format",
                    "cuda",
                    "-extra_hw_frames",
                    "8",
                ]),
                output_args: ffmpeg_output_args(&["-vf", "hwdownload,format=yuv420p"]),
            });
        }
        if hwaccels.contains("drm") {
            candidates.push(DecoderCandidate {
                name: "linux-drm-copyback".into(),
                input_args: base_ffmpeg_input_args(&["-hwaccel", "drm"]),
                output_args: ffmpeg_output_args(&["-vf", "format=yuv420p"]),
            });
        }
    }

    candidates.push(DecoderCandidate {
        name: if demuxer == "av1" {
            "software-dav1d".into()
        } else {
            format!("software-{demuxer}")
        },
        input_args: base_ffmpeg_input_args(&[]),
        output_args: if demuxer == "av1" {
            ffmpeg_output_args(&["-c:v", "libdav1d"])
        } else {
            ffmpeg_output_args(&[])
        },
    });

    if let Ok(force_backend) = env::var("OPENNOW_STREAMER_DECODER_BACKEND") {
        let force_backend = force_backend.to_lowercase();
        candidates.sort_by_key(|candidate| {
            if candidate.name.to_lowercase().contains(&force_backend) {
                0
            } else {
                1
            }
        });
    }

    candidates
}

fn base_ffmpeg_input_args(extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "-loglevel".into(),
        "warning".into(),
        "-fflags".into(),
        "nobuffer".into(),
        "-flags".into(),
        "low_delay".into(),
        "-flags2".into(),
        "fast".into(),
        "-probesize".into(),
        "32".into(),
        "-analyzeduration".into(),
        "0".into(),
        "-thread_queue_size".into(),
        "4".into(),
    ];
    args.extend(extra.iter().map(|value| value.to_string()));
    args
}

fn ffmpeg_output_args(extra: &[&str]) -> Vec<String> {
    let mut args = vec!["-threads".into(), "1".into()];
    args.extend(extra.iter().map(|value| value.to_string()));
    args
}

fn query_hwaccels(ffmpeg: &Path) -> HashSet<String> {
    Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-hwaccels")
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .skip(1)
                .map(|line| line.trim().to_lowercase())
                .filter(|line| !line.is_empty())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn linux_render_node() -> Option<String> {
    ["/dev/dri/renderD128", "/dev/dri/renderD129"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .map(ToString::to_string)
}

#[cfg(not(target_os = "linux"))]
fn linux_render_node() -> Option<String> {
    None
}

fn format_exit_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "terminated by signal".into(),
    }
}

#[derive(Default)]
struct H265Assembler {
    packet: H265Packet,
    fu_buffer: Vec<u8>,
}

impl H265Assembler {
    fn push(&mut self, payload: Bytes) -> anyhow::Result<Vec<u8>> {
        self.packet.depacketize(&payload)?;
        match self.packet.payload() {
            H265Payload::H265SingleNALUnitPacket(packet) => {
                let mut out = vec![0, 0, 0, 1];
                out.extend_from_slice(&packet.payload_header().0.to_be_bytes());
                out.extend_from_slice(&packet.payload());
                Ok(out)
            }
            H265Payload::H265AggregationPacket(packet) => {
                let mut out = Vec::new();
                if let Some(first) = packet.first_unit() {
                    out.extend_from_slice(&[0, 0, 0, 1]);
                    let nal: Bytes = first.nal_unit();
                    out.extend_from_slice(nal.as_ref());
                }
                for unit in packet.other_units() {
                    out.extend_from_slice(&[0, 0, 0, 1]);
                    let nal: Bytes = unit.nal_unit();
                    out.extend_from_slice(nal.as_ref());
                }
                Ok(out)
            }
            H265Payload::H265FragmentationUnitPacket(packet) => {
                if packet.fu_header().s() {
                    self.fu_buffer.clear();
                    self.fu_buffer.extend_from_slice(&[0, 0, 0, 1]);
                    let header = packet.payload_header();
                    let reconstructed0 = ((header.f() as u8) << 7)
                        | ((packet.fu_header().fu_type() & 0x3F) << 1)
                        | (header.layer_id() & 0x01);
                    let reconstructed1 = ((header.layer_id() as u8) << 3) | (header.tid() & 0x07);
                    self.fu_buffer.push(reconstructed0);
                    self.fu_buffer.push(reconstructed1);
                }
                self.fu_buffer.extend_from_slice(&packet.payload());
                if packet.fu_header().e() {
                    Ok(std::mem::take(&mut self.fu_buffer))
                } else {
                    Ok(Vec::new())
                }
            }
            H265Payload::H265PACIPacket(packet) => {
                let mut out = vec![0, 0, 0, 1];
                out.extend_from_slice(&packet.payload());
                Ok(out)
            }
        }
    }
}

fn resolve_ffmpeg_binary() -> anyhow::Result<PathBuf> {
    if let Ok(path) = env::var("OPENNOW_FFMPEG_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    let exe = env::current_exe().context("current_exe")?;
    let suffix = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    let candidates = [
        exe.parent().map(|p| p.join(format!("ffmpeg{suffix}"))),
        exe.parent()
            .and_then(Path::parent)
            .map(|p| p.join("bin").join(format!("ffmpeg{suffix}"))),
        Some(PathBuf::from(format!("ffmpeg{suffix}"))),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|candidate| {
            candidate.exists() || candidate == &PathBuf::from(format!("ffmpeg{suffix}"))
        })
        .ok_or_else(|| anyhow!("unable to locate bundled ffmpeg runtime"))
}
