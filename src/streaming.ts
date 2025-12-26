// GFN WebRTC Streaming Implementation
// Based on analysis of official GFN browser client (WebSocket signaling + WebRTC)
// Reference: geronimo.log analysis showing wssignaling:1, WebRTC transport

import { invoke } from "@tauri-apps/api/core";

// Types
interface WebRtcConfig {
  session_id: string;
  signaling_url: string;
  ice_servers: IceServerConfig[];
  video_codec: string;
  audio_codec: string;
  max_bitrate_kbps: number;
}

interface IceServerConfig {
  urls: string[];
  username?: string;
  credential?: string;
}

interface StreamConnectionInfo {
  control_ip: string;
  control_port: number;
  stream_ip: string | null;
  stream_port: number;
  resource_path: string;
}

interface StreamingConnectionState {
  session_id: string;
  phase: string;
  server_ip: string | null;
  signaling_url: string | null;
  connection_info: StreamConnectionInfo | null;
  gpu_type: string | null;
  error: string | null;
}

// NVST Signaling Message Types (based on official client analysis)
interface NvstSignalingMessage {
  type: string;
  payload?: unknown;
  timestamp?: number;
  sequence?: number;
}

interface NvstAuthMessage {
  type: "auth";
  payload: {
    token: string;
    clientType: string;
    clientVersion: string;
    capabilities: string[];
  };
}

interface NvstOfferMessage {
  type: "offer";
  payload: {
    sdp: string;
    sessionId: string;
  };
}

interface NvstAnswerMessage {
  type: "answer";
  payload: {
    sdp: string;
  };
}

interface NvstIceCandidateMessage {
  type: "ice-candidate";
  payload: {
    candidate: string;
    sdpMid: string | null;
    sdpMLineIndex: number | null;
  };
}

// Streaming state
export interface StreamingState {
  connected: boolean;
  peerConnection: RTCPeerConnection | null;
  dataChannels: Map<string, RTCDataChannel>;
  videoElement: HTMLVideoElement | null;
  audioContext: AudioContext | null;
  signalingSocket: WebSocket | null;
  sessionId: string | null;
  stats: StreamingStats | null;
  retryCount: number;
  maxRetries: number;
  inputDebugLogged?: Set<string>;
}

export interface StreamingStats {
  fps: number;
  latency_ms: number;
  bitrate_kbps: number;
  packet_loss: number;
  resolution: string;
  codec: string;
  // Input latency stats (in ms)
  input_ipc_ms: number;      // Time to get mouse delta from Rust (IPC call)
  input_send_ms: number;     // Time to send input over WebRTC
  input_total_ms: number;    // Total input pipeline latency
  input_rate: number;        // Input events per second
}

// Global streaming state
let streamingState: StreamingState = {
  connected: false,
  peerConnection: null,
  dataChannels: new Map(),
  videoElement: null,
  audioContext: null,
  signalingSocket: null,
  sessionId: null,
  stats: null,
  retryCount: 0,
  maxRetries: 3,
};

// Signaling sequence counter
let signalingSeq = 0;

// Bitrate tracking for real-time calculation
let lastBytesReceived = 0;
let lastBytesTimestamp = 0;

/**
 * Initialize streaming with the given connection info
 *
 * GFN Browser Signaling Protocol (discovered from play.geforcenow.com):
 * - URL: wss://{stream_ip}/nvst/sign_in?peer_id=peer-{random}&version=2
 * - Auth: WebSocket subprotocol x-nv-sessionid.{session_id}
 * - Protocol: JSON peer messaging with ackid, peer_info, peer_msg
 */
export interface StreamingOptions {
  resolution: string; // "2560x1440" format
  fps: number;
}

export async function initializeStreaming(
  connectionState: StreamingConnectionState,
  accessToken: string,
  videoContainer: HTMLElement,
  options?: StreamingOptions
): Promise<void> {
  console.log("Initializing streaming with:", connectionState);

  if (!connectionState.connection_info) {
    throw new Error("No connection info available");
  }

  // Reset shared media stream to avoid leftover audio tracks
  sharedMediaStream = null;

  streamingState.sessionId = connectionState.session_id;
  streamingState.retryCount = 0;

  // Create video element
  const videoEl = createVideoElement();
  videoContainer.appendChild(videoEl);
  streamingState.videoElement = videoEl;

  // Create audio context for advanced audio handling
  try {
    streamingState.audioContext = new AudioContext();
  } catch (e) {
    console.warn("Failed to create AudioContext:", e);
  }

  // Get WebRTC config from backend
  const webrtcConfig = await invoke<WebRtcConfig>("get_webrtc_config", {
    sessionId: connectionState.session_id,
  });

  console.log("WebRTC config:", webrtcConfig);

  // Extract stream IP from signaling_url
  // Supports both formats:
  // - RTSP: rtsps://80-84-170-155.cloudmatchbeta.nvidiagrid.net:322
  // - WebSocket: wss://66-22-147-40.cloudmatchbeta.nvidiagrid.net:443/nvst/
  let streamIp: string | null = null;

  console.log("Connection state for streaming:", {
    signaling_url: connectionState.signaling_url,
    server_ip: connectionState.server_ip,
    connection_info: connectionState.connection_info,
  });

  if (connectionState.signaling_url) {
    try {
      // Parse URL to get hostname - supports rtsps://, rtsp://, wss://, ws://
      const urlMatch = connectionState.signaling_url.match(/(?:rtsps?|wss?):\/\/([^:/]+)/);
      if (urlMatch && urlMatch[1]) {
        streamIp = urlMatch[1];
        console.log("Extracted stream IP from signaling_url:", streamIp);
      }
    } catch (e) {
      console.warn("Failed to parse signaling_url:", e);
    }
  }

  // Fallback to other sources if signaling_url parsing failed
  if (!streamIp) {
    streamIp = connectionState.connection_info.stream_ip ||
               connectionState.connection_info.control_ip ||
               connectionState.server_ip;
    console.log("Using fallback stream IP:", streamIp, "from:",
      connectionState.connection_info.stream_ip ? "stream_ip" :
      connectionState.connection_info.control_ip ? "control_ip" : "server_ip");
  }

  if (!streamIp) {
    throw new Error("No stream server IP available");
  }

  const sessionId = connectionState.session_id;

  console.log("Stream IP:", streamIp);
  console.log("Session ID:", sessionId);

  // Parse resolution from options or use defaults
  let streamWidth = window.screen.width;
  let streamHeight = window.screen.height;
  if (options?.resolution) {
    const [w, h] = options.resolution.split('x').map(Number);
    if (w && h) {
      streamWidth = w;
      streamHeight = h;
      console.log(`Using requested resolution: ${streamWidth}x${streamHeight}`);
    }
  }

  // Parse FPS from options or use default
  let streamFps = 60; // Default FPS
  if (options?.fps && options.fps > 0) {
    streamFps = options.fps;
    console.log(`Using requested FPS: ${streamFps}`);
  }

  // Connect using the official GFN browser protocol
  await connectGfnBrowserSignaling(streamIp, sessionId, webrtcConfig, streamWidth, streamHeight, streamFps);
}

// GFN Browser Peer Protocol types
interface GfnPeerInfo {
  browser: string;
  browserVersion: string;
  connected: boolean;
  id: number;
  name: string;
  peer_role: number;
  resolution: string;
  version: number;
}

interface GfnPeerMessage {
  ackid?: number;
  peer_info?: GfnPeerInfo;
  peer_msg?: {
    from: number;
    to: number;
    msg: string;
  };
  hb?: number;
}

// Peer connection state for GFN protocol
let gfnPeerId = 2; // Client is always peer 2, server is peer 1
let gfnAckId = 0;
let heartbeatInterval: ReturnType<typeof setInterval> | null = null;
let isReconnect = false; // Track if this is a reconnection attempt

/**
 * Log detailed ICE debugging information
 */
async function logIceDebugInfo(pc: RTCPeerConnection): Promise<void> {
  try {
    const stats = await pc.getStats();
    console.log("=== ICE Debug Info ===");

    // Log all candidate pairs
    stats.forEach(report => {
      if (report.type === "candidate-pair") {
        console.log(`Candidate pair [${report.id}]:`);
        console.log(`  State: ${report.state}`);
        console.log(`  Nominated: ${report.nominated}`);
        console.log(`  Priority: ${report.priority}`);
        console.log(`  Local: ${report.localCandidateId}`);
        console.log(`  Remote: ${report.remoteCandidateId}`);
        if (report.currentRoundTripTime) {
          console.log(`  RTT: ${report.currentRoundTripTime * 1000}ms`);
        }
        if (report.requestsSent !== undefined) {
          console.log(`  Requests sent: ${report.requestsSent}`);
        }
        if (report.responsesReceived !== undefined) {
          console.log(`  Responses received: ${report.responsesReceived}`);
        }
      }
    });

    // Log local candidates
    console.log("--- Local candidates ---");
    stats.forEach(report => {
      if (report.type === "local-candidate") {
        console.log(`  ${report.candidateType}: ${report.address}:${report.port} (${report.protocol})`);
      }
    });

    // Log remote candidates
    console.log("--- Remote candidates ---");
    stats.forEach(report => {
      if (report.type === "remote-candidate") {
        console.log(`  ${report.candidateType}: ${report.address}:${report.port} (${report.protocol})`);
      }
    });

    console.log("=== End ICE Debug ===");
  } catch (e) {
    console.warn("Failed to get ICE debug info:", e);
  }
}

/**
 * Connect using the official GFN browser signaling protocol
 *
 * Protocol based on network capture from play.geforcenow.com:
 * - URL: wss://{server}/nvst/sign_in?peer_id=peer-{random}&version=2
 * - Auth: WebSocket subprotocol x-nv-sessionid.{session_id}
 * - Messages: JSON with ackid, peer_info, peer_msg fields
 */
async function connectGfnBrowserSignaling(
  serverIp: string,
  sessionId: string,
  config: WebRtcConfig,
  requestedWidth: number,
  requestedHeight: number,
  requestedFps: number
): Promise<void> {
  return new Promise((resolve, reject) => {
    // Generate random peer ID suffix (matching GFN browser format)
    const randomPeerId = Math.floor(Math.random() * 10000000000);
    const peerName = `peer-${randomPeerId}`;

    // Build signaling URL - exact format from GFN browser
    // First connection is normal, reconnections add &reconnect=1
    let signalingUrl = `wss://${serverIp}/nvst/sign_in?peer_id=${peerName}&version=2`;
    if (isReconnect) {
      signalingUrl += "&reconnect=1";
    }

    console.log("GFN Browser Signaling URL:", signalingUrl);
    console.log("Is reconnect:", isReconnect);
    console.log("Session ID for subprotocol:", sessionId);

    // Auth via WebSocket subprotocol: x-nv-sessionid.{session_id}
    const subprotocol = `x-nv-sessionid.${sessionId}`;

    let ws: WebSocket;
    try {
      ws = new WebSocket(signalingUrl, [subprotocol]);
    } catch (e) {
      console.error("Failed to create WebSocket:", e);
      reject(new Error(`WebSocket creation failed: ${e}`));
      return;
    }

    ws.binaryType = "arraybuffer";
    streamingState.signalingSocket = ws;
    gfnAckId = 0;

    let resolved = false;

    const connectionTimeout = setTimeout(() => {
      if (!resolved) {
        ws.close();
        reject(new Error("GFN signaling connection timeout"));
      }
    }, 15000);

    ws.onopen = () => {
      console.log("GFN WebSocket connected!");
      console.log("Accepted protocol:", ws.protocol);

      // Mark for reconnect on future attempts
      isReconnect = true;

      // Send peer_info immediately after connection (as seen in captures)
      const peerInfo: GfnPeerMessage = {
        ackid: ++gfnAckId,
        peer_info: {
          browser: "Chrome",
          browserVersion: navigator.userAgent.match(/Chrome\/(\d+)/)?.[1] || "131",
          connected: true,
          id: gfnPeerId,
          name: peerName,
          peer_role: 0, // 0 = client
          resolution: `${requestedWidth}x${requestedHeight}`,
          version: 2
        }
      };

      console.log("Sending peer_info:", JSON.stringify(peerInfo));
      ws.send(JSON.stringify(peerInfo));

      // Start heartbeat
      heartbeatInterval = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ hb: 1 }));
        }
      }, 5000);
    };

    ws.onmessage = async (event) => {
      const messageText = typeof event.data === "string"
        ? event.data
        : new TextDecoder().decode(event.data);

      console.log("GFN message received:", messageText.substring(0, 500));

      try {
        const message: GfnPeerMessage & { ack?: number } = JSON.parse(messageText);

        // CRITICAL: Send ack for any message with ackid (except our own echoes)
        if (message.ackid !== undefined) {
          // Don't ack our own peer_info echo (same id as us)
          const isOurEcho = message.peer_info?.id === gfnPeerId;
          if (!isOurEcho) {
            const ackResponse = { ack: message.ackid };
            console.log("Sending ack:", ackResponse);
            ws.send(JSON.stringify(ackResponse));
          }
        }

        // Handle heartbeat - respond with heartbeat
        if (message.hb !== undefined) {
          console.log("Heartbeat received, responding");
          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ hb: 1 }));
          }
          return;
        }

        // Handle ack responses to our messages
        if (message.ack !== undefined) {
          console.log("Received ack for our message:", message.ack);
          return;
        }

        // Handle server peer_info
        if (message.peer_info) {
          console.log("Server peer_info received:", message.peer_info);
          return;
        }

        // Handle peer messages (SDP offer, ICE candidates, etc.)
        if (message.peer_msg) {
          const peerMsg = message.peer_msg;
          console.log(`Peer message from ${peerMsg.from} to ${peerMsg.to}`);

          try {
            const innerMsg = JSON.parse(peerMsg.msg);

            if (innerMsg.type === "offer") {
              console.log("Received SDP offer, length:", innerMsg.sdp?.length);

              // Mark as resolved BEFORE processing - WebSocket may close during setup
              // and that's OK for ice-lite servers (signaling is complete)
              if (!resolved) {
                resolved = true;
                clearTimeout(connectionTimeout);
              }

              // Log full SDP to check for ICE candidates
              console.log("Full SDP offer:");
              console.log(innerMsg.sdp);

              // Check for server ICE candidates in SDP
              const candidateLines = innerMsg.sdp.match(/a=candidate:.*/g) || [];
              console.log("Server ICE candidates in SDP:", candidateLines.length);
              candidateLines.forEach((c: string) => console.log("  ", c));

              // Handle the SDP offer and create answer
              // This runs async - WebSocket may close during this, that's expected
              handleGfnSdpOffer(innerMsg.sdp, ws, config, serverIp, requestedWidth, requestedHeight, requestedFps)
                .then(() => {
                  console.log("SDP offer handled successfully");
                  resolve();
                })
                .catch((e) => {
                  console.error("Failed to handle SDP offer:", e);
                  reject(e);
                });
            } else if (innerMsg.type === "answer") {
              console.log("Received SDP answer (unexpected for client)");
            } else if (innerMsg.candidate !== undefined) {
              // ICE candidate from server (trickle ICE)
              console.log("Received trickle ICE candidate from server:", innerMsg.candidate);
              if (streamingState.peerConnection && innerMsg.candidate) {
                try {
                  await streamingState.peerConnection.addIceCandidate(
                    new RTCIceCandidate({
                      candidate: innerMsg.candidate,
                      sdpMid: innerMsg.sdpMid,
                      sdpMLineIndex: innerMsg.sdpMLineIndex
                    })
                  );
                  console.log("Added remote ICE candidate");
                } catch (e) {
                  console.warn("Failed to add ICE candidate:", e);
                }
              }
            } else if (innerMsg.type === "candidate") {
              // Alternative ICE candidate format
              console.log("Received ICE candidate (type=candidate):", JSON.stringify(innerMsg));
              if (streamingState.peerConnection && innerMsg.candidate) {
                try {
                  await streamingState.peerConnection.addIceCandidate(
                    new RTCIceCandidate({
                      candidate: innerMsg.candidate,
                      sdpMid: innerMsg.sdpMid || "0",
                      sdpMLineIndex: innerMsg.sdpMLineIndex ?? 0
                    })
                  );
                  console.log("Added remote ICE candidate (alt format)");
                } catch (e) {
                  console.warn("Failed to add ICE candidate (alt format):", e);
                }
              }
            } else {
              // Log any unhandled peer_msg types for debugging
              console.log("Unhandled peer_msg inner type:", JSON.stringify(innerMsg).substring(0, 300));
            }
          } catch (parseError) {
            console.log("peer_msg content is not JSON:", peerMsg.msg.substring(0, 100));
          }
        }

      } catch (e) {
        console.warn("Failed to parse GFN message:", e);
      }
    };

    ws.onerror = (error) => {
      console.error("GFN WebSocket error:", error);
      if (!resolved) {
        resolved = true;
        clearTimeout(connectionTimeout);
        if (heartbeatInterval) {
          clearInterval(heartbeatInterval);
          heartbeatInterval = null;
        }
        reject(new Error("GFN signaling connection failed"));
      }
    };

    ws.onclose = (event) => {
      console.log("GFN WebSocket closed:", event.code, event.reason);
      if (heartbeatInterval) {
        clearInterval(heartbeatInterval);
        heartbeatInterval = null;
      }

      if (!resolved) {
        resolved = true;
        clearTimeout(connectionTimeout);
        reject(new Error(`GFN signaling closed: ${event.code} ${event.reason}`));
      }
    };
  });
}

/**
 * Handle SDP offer from GFN server and send answer
 * Note: Server's ICE candidate will come via trickle ICE, not manually constructed
 */
async function handleGfnSdpOffer(
  serverSdp: string,
  ws: WebSocket,
  config: WebRtcConfig,
  serverIp: string,
  requestedWidth: number,
  requestedHeight: number,
  requestedFps: number
): Promise<void> {
  console.log("Setting up WebRTC with GFN SDP offer");
  console.log("SDP offer preview:", serverSdp.substring(0, 500));

  // Check for ice-lite in SDP
  // With ice-lite, the server sends its actual ICE candidate via trickle ICE
  // (the port in SDP m=audio line is NOT the actual port - server will send correct one)
  const isIceLite = serverSdp.includes("a=ice-lite");
  console.log("Server uses ice-lite:", isIceLite);

  // Log ICE servers for debugging
  console.log("ICE servers configuration:");
  config.ice_servers.forEach((s, i) => {
    console.log(`  [${i}] urls:`, s.urls);
    if (s.username) console.log(`      username: ${s.username}`);
    if (s.credential) console.log(`      has credential: yes`);
  });

  // Create RTCPeerConnection with proper configuration
  // Settings based on official GFN browser client analysis
  const pc = new RTCPeerConnection({
    iceServers: config.ice_servers.map((s) => ({
      urls: s.urls,
      username: s.username,
      credential: s.credential,
    })),
    bundlePolicy: "max-bundle",      // Bundle all media over single transport
    rtcpMuxPolicy: "require",        // Multiplex RTP and RTCP
    iceCandidatePoolSize: 2,         // Official client uses 2
  } as RTCConfiguration);

  streamingState.peerConnection = pc;

  // Set up event handlers
  pc.ontrack = handleTrack;

  pc.onicecandidate = (event) => {
    if (event.candidate) {
      console.log("Local ICE candidate:", event.candidate.candidate);
      console.log("  type:", event.candidate.type, "protocol:", event.candidate.protocol);

      // Send ICE candidate to server using GFN peer protocol
      const candidateMsg: GfnPeerMessage = {
        peer_msg: {
          from: gfnPeerId, // 2 = client
          to: 1,           // 1 = server
          msg: JSON.stringify({
            candidate: event.candidate.candidate,
            sdpMid: event.candidate.sdpMid,
            sdpMLineIndex: event.candidate.sdpMLineIndex
          })
        },
        ackid: ++gfnAckId
      };

      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(candidateMsg));
      }
    } else {
      console.log("ICE gathering complete - all candidates sent");
    }
  };

  pc.onicegatheringstatechange = () => {
    console.log("ICE gathering state:", pc.iceGatheringState);
  };

  pc.oniceconnectionstatechange = () => {
    console.log("ICE connection state:", pc.iceConnectionState);
    if (pc.iceConnectionState === "connected") {
      console.log("ICE connected! Media should start flowing.");
    } else if (pc.iceConnectionState === "completed") {
      console.log("ICE completed - connection fully established");
    } else if (pc.iceConnectionState === "failed") {
      console.error("ICE connection failed!");
      logIceDebugInfo(pc);
    } else if (pc.iceConnectionState === "disconnected") {
      console.warn("ICE disconnected - may reconnect or be checking...");
      // Log debug info to understand why
      logIceDebugInfo(pc);
    } else if (pc.iceConnectionState === "checking") {
      console.log("ICE checking - connectivity checks in progress");
      // Log candidate pairs being checked
      setTimeout(() => logIceDebugInfo(pc), 1000);
    }
  };

  pc.onsignalingstatechange = () => {
    console.log("Signaling state:", pc.signalingState);
  };

  pc.onconnectionstatechange = () => {
    console.log("Connection state:", pc.connectionState);
    if (pc.connectionState === "connected") {
      streamingState.connected = true;
      console.log("WebRTC fully connected!");
      startStatsCollection();

      // Connection is now fully established
      // Input channel should already be open from initial setup
    } else if (pc.connectionState === "failed") {
      console.error("WebRTC connection failed");
      streamingState.connected = false;
    } else if (pc.connectionState === "disconnected") {
      console.warn("WebRTC disconnected");
      streamingState.connected = false;
    }
  };

  // Set up handler for server-created data channels (control_channel, etc.)
  pc.ondatachannel = (event) => {
    const channel = event.channel;
    console.log("=== SERVER CREATED DATA CHANNEL ===");
    console.log("  Label:", channel.label);
    console.log("  ID:", channel.id);
    console.log("  Protocol:", channel.protocol);
    console.log("  ordered:", channel.ordered, "maxRetransmits:", channel.maxRetransmits, "maxPacketLifeTime:", channel.maxPacketLifeTime);
    channel.binaryType = "arraybuffer";

    channel.onopen = () => {
      console.log(`Data channel '${channel.label}' opened, readyState:`, channel.readyState);
      streamingState.dataChannels.set(channel.label, channel);

      // Also store with normalized names for easier lookup
      const lowerLabel = channel.label.toLowerCase();
      if (lowerLabel.includes("input") || lowerLabel.includes("ri_") || lowerLabel === "input_1") {
        console.log("SERVER input channel opened - storing as 'server_input'");
        streamingState.dataChannels.set("server_input", channel);
        // Also try using server's input channel
        // Don't overwrite 'input' if we already have a working client channel
        if (!streamingState.dataChannels.has("input") || !inputHandshakeComplete) {
          console.log("Using server's input channel as primary");
          streamingState.dataChannels.set("input", channel);
        }
      }
      if (lowerLabel.includes("control") || lowerLabel.includes("cc_")) {
        console.log("Storing as 'control' channel");
        streamingState.dataChannels.set("control", channel);
        console.log("Control channel ready");
        // Input channel was already created before SDP negotiation (per official GFN client)
      }
    };

    channel.onmessage = (e) => {
      const size = e.data instanceof ArrayBuffer ? e.data.byteLength : e.data.length;
      console.log(`Data channel '${channel.label}' message, size:`, size);

      // Decode control channel messages
      if (e.data instanceof ArrayBuffer && e.data.byteLength > 0) {
        const bytes = new Uint8Array(e.data);
        console.log(`  First 32 bytes: ${Array.from(bytes.slice(0, 32)).map(b => b.toString(16).padStart(2, '0')).join(' ')}`);

        // Try to decode as text (might be JSON)
        try {
          const text = new TextDecoder().decode(e.data);
          if (text.startsWith('{') || text.startsWith('[')) {
            const json = JSON.parse(text);
            console.log(`  JSON content:`, json);

            // Log specific message types for debugging
            if (json.videoStreamProgressEvent) {
              console.log("  Video progress event received");
            }
            if (json.customMessage) {
              console.log("  Custom message:", json.customMessage);
            }
            if (json.inputReady !== undefined) {
              console.log("  *** INPUT READY MESSAGE:", json.inputReady);
            }
          } else if (text.match(/^[\x20-\x7E\r\n\t]+$/)) {
            // Printable ASCII
            console.log(`  Text content:`, text.substring(0, 200));
          }
        } catch {
          // Binary data - decode header
          if (bytes.length >= 4) {
            const view = new DataView(e.data);
            const msgType = view.getUint16(0, true);
            const flags = view.getUint16(2, true);
            console.log(`  Binary msg type: 0x${msgType.toString(16)}, flags: 0x${flags.toString(16)}`);
          }

          // Check for handshake on server input channel
          if (bytes.length === 4 && bytes[0] === 0x0e) {
            console.log("  SERVER INPUT HANDSHAKE detected on", channel.label);
            console.log("  Responding to server input handshake...");
            const response = new Uint8Array([bytes[0], bytes[1], bytes[2], bytes[3]]);
            try {
              channel.send(response.buffer);
              console.log("  Server input handshake response sent");
              inputHandshakeComplete = true;
              streamStartTime = Date.now(); // Set stream start time
              // Use server's channel for input
              streamingState.dataChannels.set("input", channel);
              console.log("  Switched to server input channel for input events");
              console.log("  Stream start time set:", streamStartTime);
            } catch (err) {
              console.error("  Failed to send server input handshake:", err);
            }
          }
        }
      } else if (typeof e.data === 'string') {
        console.log(`  String content:`, e.data.substring(0, 200));
        // Try parsing as JSON
        try {
          const json = JSON.parse(e.data);
          console.log(`  Parsed JSON:`, json);
        } catch {}
      }
    };

    channel.onerror = (e) => console.error(`Data channel '${channel.label}' error:`, e);
    channel.onclose = () => console.log(`Data channel '${channel.label}' closed`);
  };

  // === CRITICAL: Create input channel BEFORE SDP negotiation (per official GFN client) ===
  // The official NVIDIA GFN web client creates data channels during RTCPeerConnection setup,
  // BEFORE calling setRemoteDescription. This ensures the server recognizes the channel
  // and sends the input handshake message when the SCTP connection is established.
  //
  // From vendor.08340f0978ba62aa.js analysis:
  //   const Wt = {ordered: true, reliable: true};
  //   this.cc = this.pc.createDataChannel("input_channel_v1", Wt);
  //   this.cc.binaryType = "arraybuffer";
  //   // ... then later setRemoteDescription is called
  console.log("Creating input_channel_v1 BEFORE SDP negotiation (per official GFN client)...");
  const inputChannel = pc.createDataChannel("input_channel_v1", {
    ordered: false,        // Unordered for lowest latency (mouse deltas don't need ordering)
    maxRetransmits: 0,     // No retransmits - if packet lost, next one will have updated position
  });
  inputChannel.binaryType = "arraybuffer";

  // Set up input channel handlers
  inputChannel.onopen = () => {
    console.log("=== INPUT CHANNEL OPENED ===");
    console.log("  Label:", inputChannel.label);
    console.log("  ID:", inputChannel.id);
    console.log("  ReadyState:", inputChannel.readyState);
    streamingState.dataChannels.set("input_channel_v1", inputChannel);
    streamingState.dataChannels.set("input", inputChannel);
    console.log("  Waiting for server handshake message...");
  };

  inputChannel.onmessage = (e) => {
    const size = e.data instanceof ArrayBuffer ? e.data.byteLength : 0;
    console.log("=== INPUT CHANNEL MESSAGE ===");
    console.log("  Size:", size, "bytes");

    if (e.data instanceof ArrayBuffer && size > 0) {
      const bytes = new Uint8Array(e.data);
      const view = new DataView(e.data);
      console.log("  Bytes:", Array.from(bytes.slice(0, 16)).map(b => b.toString(16).padStart(2, '0')).join(' '));

      // Check for version handshake (per GFN protocol)
      if (!inputHandshakeComplete && size >= 2) {
        const firstWord = view.getUint16(0, true); // Little endian

        if (firstWord === 526) {
          // New protocol format: 0x020E (526 LE) followed by version
          const version = size >= 4 ? view.getUint16(2, true) : 0;
          console.log(`  *** HANDSHAKE: New format (0x020E), version=${version}`);
          inputProtocolVersion = version;

          // CRITICAL: Send handshake response back to server
          // Echo the received bytes to acknowledge the handshake
          try {
            const response = new Uint8Array(bytes.slice(0, size));
            inputChannel.send(response.buffer);
            console.log("  *** HANDSHAKE RESPONSE SENT:", Array.from(response).map(b => b.toString(16).padStart(2, '0')).join(' '));
          } catch (err) {
            console.error("  Failed to send handshake response:", err);
          }

          inputHandshakeComplete = true;
          inputHandshakeAttempts++;
          streamStartTime = Date.now();
          console.log("  *** INPUT HANDSHAKE COMPLETE! Ready for input events.");
        } else {
          // Old format: first word is the version directly
          console.log(`  *** HANDSHAKE: Old format, version=${firstWord}`);
          inputProtocolVersion = firstWord;

          // CRITICAL: Send handshake response back to server
          try {
            const response = new Uint8Array(bytes.slice(0, size));
            inputChannel.send(response.buffer);
            console.log("  *** HANDSHAKE RESPONSE SENT:", Array.from(response).map(b => b.toString(16).padStart(2, '0')).join(' '));
          } catch (err) {
            console.error("  Failed to send handshake response:", err);
          }

          inputHandshakeComplete = true;
          inputHandshakeAttempts++;
          streamStartTime = Date.now();
          console.log("  *** INPUT HANDSHAKE COMPLETE! Ready for input events.");
        }
      } else {
        // Post-handshake message (ACK, cursor position, etc.)
        console.log("  Post-handshake message received");
      }
    }
  };

  inputChannel.onerror = (e) => console.error("Input channel error:", e);
  inputChannel.onclose = () => {
    console.log("Input channel closed");
    streamingState.dataChannels.delete("input");
    streamingState.dataChannels.delete("input_channel_v1");
  };

  console.log("Input channel created, state:", inputChannel.readyState);

  // NOTE: Don't add transceivers manually - the server's SDP offer already defines
  // the media sections. Adding transceivers would create duplicates and potentially
  // cause negotiation issues. The browser will automatically create transceivers
  // when setRemoteDescription processes the offer's m= lines.

  // Set remote description (server's SDP offer)
  const remoteDesc = new RTCSessionDescription({
    type: "offer",
    sdp: serverSdp,
  });

  await pc.setRemoteDescription(remoteDesc);
  console.log("Remote SDP offer set");

  // Log ice-lite server's ICE credentials for debugging
  if (isIceLite) {
    const iceUfragMatch = serverSdp.match(/a=ice-ufrag:(\S+)/);
    const icePwdMatch = serverSdp.match(/a=ice-pwd:(\S+)/);
    const iceUfrag = iceUfragMatch ? iceUfragMatch[1] : "";
    const icePwd = icePwdMatch ? icePwdMatch[1] : "";
    console.log("Server ICE credentials - ufrag:", iceUfrag, "pwd:", icePwd.substring(0, 8) + "...");
    console.log("Note: ice-lite server will send its candidate via trickle ICE, not in SDP");
  }

  // Create answer
  const answer = await pc.createAnswer({
    offerToReceiveVideo: true,
    offerToReceiveAudio: true,
  });

  // Modify SDP to prefer certain codecs if needed
  if (answer.sdp) {
    answer.sdp = preferCodec(answer.sdp, config.video_codec);
  }

  await pc.setLocalDescription(answer);
  console.log("Local SDP answer created");
  console.log("Answer SDP preview:", answer.sdp?.substring(0, 500));

  // Wait briefly for some ICE candidates to be gathered
  // Some ice-lite servers expect candidates in the answer SDP
  console.log("Waiting for initial ICE candidates...");
  await new Promise<void>((resolve) => {
    let candidateCount = 0;
    const checkCandidates = () => {
      // Check if we have at least one srflx candidate (public IP)
      const currentSdp = pc.localDescription?.sdp || "";
      const hasSrflx = currentSdp.includes("typ srflx");
      candidateCount++;

      if (hasSrflx || candidateCount > 10) {
        resolve();
      } else {
        setTimeout(checkCandidates, 100);
      }
    };
    // Start checking after a short delay
    setTimeout(checkCandidates, 50);
  });

  const currentSdp = pc.localDescription?.sdp || answer.sdp || "";
  console.log("Sending answer with gathered candidates, SDP length:", currentSdp.length);

  // Count candidates in our answer
  const ourCandidates = currentSdp.match(/a=candidate:.*/g) || [];
  console.log("Our candidates in answer SDP:", ourCandidates.length);
  ourCandidates.forEach(c => console.log("  ", c.substring(0, 80)));

  // Extract ICE credentials and DTLS fingerprint from our answer SDP
  // The server needs these in nvstSdp to complete the ICE/DTLS handshake
  const iceUfragMatch = currentSdp.match(/a=ice-ufrag:(\S+)/);
  const icePwdMatch = currentSdp.match(/a=ice-pwd:(\S+)/);
  const fingerprintMatch = currentSdp.match(/a=fingerprint:sha-256\s+(\S+)/);

  const iceUfrag = iceUfragMatch ? iceUfragMatch[1] : "";
  const icePwd = icePwdMatch ? icePwdMatch[1] : "";
  const fingerprint = fingerprintMatch ? fingerprintMatch[1] : "";

  console.log("Our ICE credentials - ufrag:", iceUfrag, "pwd:", icePwd.substring(0, 8) + "...");
  console.log("Our DTLS fingerprint:", fingerprint.substring(0, 30) + "...");

  // Use requested resolution for viewport dimensions (not screen dimensions)
  const viewportWidth = requestedWidth;
  const viewportHeight = requestedHeight;
  console.log(`nvstSdp viewport: ${viewportWidth}x${viewportHeight}`);

  console.log(`nvstSdp video.maxFPS: ${requestedFps}`);

  // Use bitrate from config (set by user in settings)
  const maxBitrateKbps = config.max_bitrate_kbps || 100000;
  const minBitrateKbps = Math.min(10000, maxBitrateKbps / 10); // 10% of max or 10 Mbps
  const initialBitrateKbps = Math.round(maxBitrateKbps * 0.5); // Start at 50%
  console.log(`Bitrate settings: max=${maxBitrateKbps}kbps, min=${minBitrateKbps}kbps, initial=${initialBitrateKbps}kbps`);

  // Build nvstSdp matching official GFN browser client format
  // Based on Wl function from vendor_beautified.js
  const isHighFps = requestedFps >= 120;
  const is120Fps = requestedFps === 120;
  const is240Fps = requestedFps >= 240;

  const nvstSdpString = [
    "v=0",
    "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
    "s=-",
    "t=0 0",
    `a=general.icePassword:${icePwd}`,
    `a=general.iceUserNameFragment:${iceUfrag}`,
    `a=general.dtlsFingerprint:${fingerprint}`,
    "m=video 0 RTP/AVP",
    "a=msid:fbc-video-0",
    // FEC settings
    "a=vqos.fec.rateDropWindow:10",
    "a=vqos.fec.minRequiredFecPackets:2",
    "a=vqos.fec.repairMinPercent:5",
    "a=vqos.fec.repairPercent:5",
    "a=vqos.fec.repairMaxPercent:35",
    // DRC settings - disable for high FPS, use DFC instead
    ...(isHighFps ? [
      "a=vqos.drc.enable:0",
      "a=vqos.dfc.enable:1",
      "a=vqos.dfc.decodeFpsAdjPercent:85",
      "a=vqos.dfc.targetDownCooldownMs:250",
      "a=vqos.dfc.dfcAlgoVersion:2",
      `a=vqos.dfc.minTargetFps:${is120Fps ? 100 : 60}`,
    ] : [
      "a=vqos.drc.minRequiredBitrateCheckEnabled:1",
    ]),
    // Video encoder settings
    "a=video.dx9EnableNv12:1",
    "a=video.dx9EnableHdr:1",
    "a=vqos.qpg.enable:1",
    "a=vqos.resControl.qp.qpg.featureSetting:7",
    "a=bwe.useOwdCongestionControl:1",
    "a=video.enableRtpNack:1",
    "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200",
    "a=vqos.drc.bitrateIirFilterFactor:18",
    "a=video.packetSize:1140",
    "a=packetPacing.minNumPacketsPerGroup:15",
    // High FPS (120+) optimizations from official GFN client
    ...(isHighFps ? [
      "a=bwe.iirFilterFactor:8",
      "a=video.encoderFeatureSetting:47",
      "a=video.encoderPreset:6",
      "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600",
      "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9",
      `a=video.fbcDynamicFpsGrabTimeoutMs:${is120Fps ? 6 : 18}`,
      `a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:${is120Fps ? 6000 : 12000}`,
    ] : []),
    // Ultra high FPS (240+) optimizations
    ...(is240Fps ? [
      "a=video.enableNextCaptureMode:1",
      "a=vqos.maxStreamFpsEstimate:240",
      "a=video.videoSplitEncodeStripsPerFrame:3",
      "a=video.updateSplitEncodeStateDynamically:1",
    ] : []),
    // Out of focus settings
    "a=vqos.adjustStreamingFpsDuringOutOfFocus:1",
    "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1",
    "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1",
    "a=vqos.resControl.cpmRtc.featureMask:3",
    // Packet pacing
    `a=packetPacing.numGroups:${is120Fps ? 3 : 5}`,
    "a=packetPacing.maxDelayUs:1000",
    "a=packetPacing.minNumPacketsFrame:10",
    // NACK settings
    "a=video.rtpNackQueueLength:1024",
    "a=video.rtpNackQueueMaxPackets:512",
    "a=video.rtpNackMaxPacketCount:25",
    // Resolution/quality settings
    "a=vqos.drc.qpMaxResThresholdAdj:4",
    "a=vqos.grc.qpMaxResThresholdAdj:4",
    "a=vqos.drc.iirFilterFactor:100",
    // Viewport and FPS
    `a=video.clientViewportWd:${viewportWidth}`,
    `a=video.clientViewportHt:${viewportHeight}`,
    `a=video.maxFPS:${requestedFps}`,
    // Bitrate settings
    `a=video.initialBitrateKbps:${initialBitrateKbps}`,
    `a=video.initialPeakBitrateKbps:${initialBitrateKbps}`,
    `a=vqos.bw.maximumBitrateKbps:${maxBitrateKbps}`,
    `a=vqos.bw.minimumBitrateKbps:${minBitrateKbps}`,
    // Encoder settings
    "a=video.maxNumReferenceFrames:4",
    "a=video.mapRtpTimestampsToFrames:1",
    "a=video.encoderCscMode:3",
    "a=video.scalingFeature1:0",
    "a=video.prefilterParams.prefilterModel:0",
    // Audio track
    "m=audio 0 RTP/AVP",
    "a=msid:audio",
    // Mic track
    "m=mic 0 RTP/AVP",
    "a=msid:mic",
    // Input/application track
    "m=application 0 RTP/AVP",
    "a=msid:input_1",
    "a=ri.partialReliableThresholdMs:300",
    ""
  ].join("\n");

  console.log("Built nvstSdp with ICE credentials and streaming params");

  // Send answer to server using GFN peer protocol
  // Include nvstSdp as seen in official browser traffic
  const answerMsg: GfnPeerMessage = {
    peer_msg: {
      from: gfnPeerId, // 2 = client
      to: 1,           // 1 = server
      msg: JSON.stringify({
        type: "answer",
        sdp: currentSdp,
        nvstSdp: nvstSdpString
      })
    },
    ackid: ++gfnAckId
  };

  if (ws.readyState === WebSocket.OPEN) {
    console.log("Sending SDP answer to server (with nvstSdp)");
    ws.send(JSON.stringify(answerMsg));
  } else {
    console.error("WebSocket not open, cannot send answer! State:", ws.readyState);
  }

  // For ice-lite servers that don't send trickle ICE candidates,
  // we need to construct the candidate manually from the server IP and SDP port
  console.log("Answer sent. Adding server ICE candidate manually for ice-lite...");

  // Extract port from the SDP (from m=audio or m=video line)
  const portMatch = serverSdp.match(/m=(?:audio|video)\s+(\d+)/);
  const serverPort = portMatch ? parseInt(portMatch[1], 10) : 47998;

  // Convert serverIp from hostname format to IP
  // Format: 80-250-101-43.cloudmatchbeta.nvidiagrid.net -> 80.250.101.43
  let serverIpAddress = serverIp;
  const ipMatch = serverIp.match(/^(\d+-\d+-\d+-\d+)\./);
  if (ipMatch) {
    serverIpAddress = ipMatch[1].replace(/-/g, ".");
    console.log("Converted server hostname to IP:", serverIpAddress);
  }

  // Extract ICE credentials from server SDP
  const serverUfragMatch = serverSdp.match(/a=ice-ufrag:(\S+)/);
  const serverUfrag = serverUfragMatch ? serverUfragMatch[1] : "";

  // Construct the ICE candidate
  // Format: candidate:foundation component protocol priority ip port typ type
  const candidateString = `candidate:1 1 udp 2130706431 ${serverIpAddress} ${serverPort} typ host`;
  console.log("Constructed server ICE candidate:", candidateString);

  try {
    await pc.addIceCandidate(new RTCIceCandidate({
      candidate: candidateString,
      sdpMid: "0",
      sdpMLineIndex: 0,
      usernameFragment: serverUfrag
    }));
    console.log("Successfully added server ICE candidate");
  } catch (e) {
    console.warn("Failed to add constructed ICE candidate:", e);
    // Try alternative format with different sdpMid values
    for (const mid of ["1", "2", "3"]) {
      try {
        await pc.addIceCandidate(new RTCIceCandidate({
          candidate: candidateString,
          sdpMid: mid,
          sdpMLineIndex: parseInt(mid, 10),
          usernameFragment: serverUfrag
        }));
        console.log(`Added server ICE candidate with sdpMid=${mid}`);
        break;
      } catch (e2) {
        // Continue trying
      }
    }
  }
}

/**
 * Create input data channel (FALLBACK FUNCTION)
 * NOTE: The primary input channel is now created BEFORE SDP negotiation in
 * handleGfnSdpOffer/handleSdpOffer. This function is kept as a fallback.
 */
function createInputDataChannel(pc: RTCPeerConnection): void {
  if (streamingState.dataChannels.has("input_channel_v1")) {
    console.log("Input channel already exists (created before SDP negotiation)");
    return;
  }
  console.warn("createInputDataChannel called as fallback - should have been created before SDP!");
}

/**
 * Try connecting to multiple signaling URLs until one works
 */
async function connectSignalingWithMultipleUrls(
  urls: string[],
  accessToken: string,
  config: WebRtcConfig
): Promise<void> {
  let lastError: Error | null = null;
  let urlIndex = 0;

  for (const url of urls) {
    urlIndex++;
    console.log(`Trying signaling URL ${urlIndex}/${urls.length}: ${url}`);

    try {
      await connectSignalingWithTimeout(url, accessToken, config, 5000);
      console.log(`Successfully connected to: ${url}`);
      return;
    } catch (e) {
      const error = e as Error;
      console.warn(`URL ${urlIndex} failed: ${error.message}`);
      lastError = error;
      // Continue to next URL immediately (no delay between URLs)
    }
  }

  throw lastError || new Error("Failed to connect to any signaling server");
}

/**
 * Connect to signaling with a timeout
 */
async function connectSignalingWithTimeout(
  url: string,
  accessToken: string,
  config: WebRtcConfig,
  timeoutMs: number
): Promise<void> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`Connection timeout after ${timeoutMs}ms`));
    }, timeoutMs);

    connectSignaling(url, accessToken, config)
      .then(() => {
        clearTimeout(timeout);
        resolve();
      })
      .catch((e) => {
        clearTimeout(timeout);
        reject(e);
      });
  });
}

/**
 * Create a video element for streaming
 */
function createVideoElement(): HTMLVideoElement {
  const video = document.createElement("video");
  video.id = "gfn-stream-video";
  video.autoplay = true;
  video.playsInline = true;
  video.muted = false; // Audio enabled
  video.controls = false; // No controls - this is a live stream
  video.disablePictureInPicture = true; // No PiP

  // === LOW LATENCY OPTIMIZATIONS ===
  // Hint for low-latency video decoding
  (video as any).latencyHint = "interactive";
  // Disable audio/video sync for lower latency (audio may drift slightly)
  (video as any).disableRemotePlayback = true;
  // Disable pitch correction for lower latency audio
  (video as any).preservesPitch = false;
  // Request hardware acceleration
  (video as any).mozPreservesPitch = false;

  video.style.cssText = `
    width: 100%;
    height: 100%;
    background: #000;
    object-fit: contain;
    pointer-events: auto;
  `;

  // Prevent pausing the stream
  video.onpause = () => {
    // Immediately resume if paused
    video.play().catch(() => {});
  };

  // Prevent seeking and keep at live edge
  video.onseeking = () => {
    // Reset to live edge immediately
    if (video.seekable.length > 0) {
      video.currentTime = video.seekable.end(video.seekable.length - 1);
    }
  };

  // Keep video at live edge - catch up if we fall behind
  let lastCheck = 0;
  const keepAtLiveEdge = () => {
    const now = performance.now();
    if (now - lastCheck > 1000) { // Check every second
      lastCheck = now;
      if (video.buffered.length > 0) {
        const bufferedEnd = video.buffered.end(video.buffered.length - 1);
        const lag = bufferedEnd - video.currentTime;
        // If we're more than 100ms behind live, catch up
        if (lag > 0.1) {
          video.currentTime = bufferedEnd;
          console.log(`Caught up to live edge (was ${(lag * 1000).toFixed(0)}ms behind)`);
        }
      }
    }
    if (video.parentNode) {
      requestAnimationFrame(keepAtLiveEdge);
    }
  };
  requestAnimationFrame(keepAtLiveEdge);

  // Handle video events
  video.onloadedmetadata = () => {
    console.log("Video metadata loaded:", video.videoWidth, "x", video.videoHeight);
  };

  video.onplay = () => {
    console.log("Video playback started");

    // === LOW LATENCY: Use requestVideoFrameCallback for precise frame timing ===
    // This fires exactly when a frame is presented, allowing tighter input sync
    if ('requestVideoFrameCallback' in video) {
      let frameCount = 0;
      let lastFrameTime = 0;
      let droppedFrames = 0;

      const onVideoFrame = (now: number, metadata: any) => {
        frameCount++;

        // Log frame timing stats periodically
        if (frameCount % 240 === 0) { // Every 240 frames (~1 second at 240fps)
          const fps = 1000 / (now - lastFrameTime);
          const totalFrames = metadata.presentedFrames || 0;
          const newDropped = (metadata.droppedVideoFrames || 0) - droppedFrames;
          droppedFrames = metadata.droppedVideoFrames || 0;

          if (newDropped > 0) {
            console.log(`Frame timing: ${fps.toFixed(1)}fps, dropped: ${newDropped} in last second`);
          }
        }
        lastFrameTime = now;

        // Keep at live edge - if processing delay detected, skip ahead
        if (metadata.processingDuration && metadata.processingDuration > 8) { // >8ms processing = lag
          console.log(`High processing delay: ${metadata.processingDuration.toFixed(1)}ms`);
        }

        // Continue callback loop
        (video as any).requestVideoFrameCallback(onVideoFrame);
      };

      (video as any).requestVideoFrameCallback(onVideoFrame);
      console.log("requestVideoFrameCallback enabled for low-latency frame sync");
    } else {
      console.log("requestVideoFrameCallback not supported");
    }
  };

  video.onerror = (e) => {
    console.error("Video error:", e);
  };

  // Note: Double-click fullscreen is handled in setupInputCapture for proper pointer lock integration

  return video;
}

/**
 * Connect to the WebSocket signaling server
 *
 * Authentication methods to try:
 * 1. WebSocket subprotocol with token
 * 2. Token in first message after connect
 * 3. Plain connection (server may use session-based auth)
 */
async function connectSignaling(
  url: string,
  accessToken: string,
  config: WebRtcConfig
): Promise<void> {
  return new Promise((resolve, reject) => {
    console.log("Opening WebSocket connection to:", url);

    let ws: WebSocket;

    try {
      // Try with GFNJWT subprotocol (some servers accept auth via subprotocol)
      // Format: ["GFNJWT-<token>"] or ["gfn", "v1"]
      ws = new WebSocket(url, ["gfn", "v1"]);
    } catch (e) {
      console.warn("WebSocket with subprotocol failed, trying plain:", e);
      try {
        ws = new WebSocket(url);
      } catch (e2) {
        reject(new Error(`Failed to create WebSocket: ${e2}`));
        return;
      }
    }

    ws.binaryType = "arraybuffer";
    streamingState.signalingSocket = ws;

    let resolved = false;
    let messageCount = 0;

    const connectionTimeout = setTimeout(() => {
      if (!resolved && ws.readyState !== WebSocket.OPEN) {
        ws.close();
        reject(new Error("WebSocket connection timeout"));
      }
    }, 10000);

    ws.onopen = async () => {
      console.log("WebSocket connected to:", url);
      console.log("Protocol:", ws.protocol || "(none)");

      try {
        // Send authentication message immediately
        await sendAuthMessage(ws, accessToken);

        // Also try RTSP-style OPTIONS request (in case server expects RTSP)
        sendRtspOptions(ws, accessToken);
      } catch (e) {
        console.warn("Auth message failed:", e);
      }
    };

    ws.onmessage = async (event) => {
      messageCount++;
      console.log(`Message ${messageCount} received, type:`, typeof event.data);

      try {
        const handled = await handleSignalingMessage(
          event.data,
          ws,
          accessToken,
          config,
          () => {
            if (!resolved) {
              resolved = true;
              clearTimeout(connectionTimeout);
              resolve();
            }
          },
          (error) => {
            if (!resolved) {
              resolved = true;
              clearTimeout(connectionTimeout);
              reject(error);
            }
          }
        );

        // If we got any valid response, consider it a success for connection
        if (handled && !resolved) {
          resolved = true;
          clearTimeout(connectionTimeout);
          resolve();
        }
      } catch (e) {
        console.error("Error handling signaling message:", e);
      }
    };

    ws.onerror = (error) => {
      console.error("WebSocket error on", url);
      if (!resolved) {
        resolved = true;
        clearTimeout(connectionTimeout);
        reject(new Error("WebSocket connection failed"));
      }
    };

    ws.onclose = (event) => {
      console.log("WebSocket closed:", event.code, event.reason || "(no reason)");
      streamingState.connected = false;

      if (!resolved) {
        resolved = true;
        clearTimeout(connectionTimeout);
        // Provide more specific error based on close code
        let errorMsg = "WebSocket closed";
        if (event.code === 1006) {
          errorMsg = "Connection failed (network error or server rejected)";
        } else if (event.code === 4001 || event.code === 4003) {
          errorMsg = "Authentication failed";
        }
        reject(new Error(`${errorMsg}: ${event.code}`));
      }
    };
  });
}

/**
 * Send RTSP OPTIONS request (in case server expects RTSP-over-WebSocket)
 */
function sendRtspOptions(ws: WebSocket, accessToken: string): void {
  const rtspRequest = [
    "OPTIONS * RTSP/1.0",
    "CSeq: 1",
    "X-GS-Version: 14.2",
    `Authorization: GFNJWT ${accessToken}`,
    "",
    "",
  ].join("\r\n");

  if (ws.readyState === WebSocket.OPEN) {
    console.log("Sending RTSP OPTIONS request");
    ws.send(rtspRequest);
  }
}

/**
 * Send authentication message to signaling server
 */
async function sendAuthMessage(ws: WebSocket, accessToken: string): Promise<void> {
  const authMsg: NvstAuthMessage = {
    type: "auth",
    payload: {
      token: accessToken,
      clientType: "BROWSER",
      clientVersion: "2.0.80.173",
      capabilities: [
        "webrtc",
        "h264",
        "av1",
        "opus",
        "datachannel",
      ],
    },
  };

  sendSignalingMessage(ws, authMsg);
  console.log("Auth message sent");
}

/**
 * Send signaling message with sequence number
 */
function sendSignalingMessage(ws: WebSocket, message: NvstSignalingMessage): void {
  const msgWithSeq = {
    ...message,
    sequence: signalingSeq++,
    timestamp: Date.now(),
  };

  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(msgWithSeq));
  } else {
    console.warn("WebSocket not open, cannot send message");
  }
}

/**
 * Handle incoming signaling message
 * Returns true if message was handled and streaming is ready
 */
async function handleSignalingMessage(
  data: string | ArrayBuffer,
  ws: WebSocket,
  accessToken: string,
  config: WebRtcConfig,
  resolve: () => void,
  reject: (error: Error) => void
): Promise<boolean> {
  const message = typeof data === "string" ? data : new TextDecoder().decode(data);

  // Try to parse as JSON
  let json: NvstSignalingMessage;
  try {
    json = JSON.parse(message);
  } catch {
    // Not JSON - might be binary control data or keep-alive
    console.log("Non-JSON signaling message:", message.substring(0, 100));
    return false;
  }

  console.log("Signaling message received:", json.type);

  switch (json.type) {
    case "auth-ack":
    case "authenticated":
      console.log("Authentication acknowledged");
      // Request streaming session
      sendSignalingMessage(ws, {
        type: "session-request",
        payload: {
          sessionId: streamingState.sessionId,
        },
      });
      return true;

    case "offer":
      // Server sent SDP offer
      const offerPayload = json.payload as { sdp: string; sessionId?: string };
      console.log("Received SDP offer, length:", offerPayload.sdp?.length);

      try {
        await handleSdpOffer(offerPayload.sdp, ws, accessToken, config);
        resolve();
      } catch (e) {
        reject(e as Error);
      }
      return true;

    case "ice-candidate":
      // Remote ICE candidate
      const candidatePayload = json.payload as {
        candidate: string;
        sdpMid: string | null;
        sdpMLineIndex: number | null;
      };

      if (streamingState.peerConnection && candidatePayload.candidate) {
        try {
          const candidate = new RTCIceCandidate({
            candidate: candidatePayload.candidate,
            sdpMid: candidatePayload.sdpMid,
            sdpMLineIndex: candidatePayload.sdpMLineIndex,
          });
          await streamingState.peerConnection.addIceCandidate(candidate);
          console.log("Added remote ICE candidate");
        } catch (e) {
          console.warn("Failed to add ICE candidate:", e);
        }
      }
      return true;

    case "error":
      const errorPayload = json.payload as { code?: string; message?: string };
      console.error("Signaling error:", errorPayload);
      reject(new Error(`Signaling error: ${errorPayload.message || "Unknown error"}`));
      return true;

    case "session-ready":
      console.log("Session ready for streaming");
      return true;

    case "ping":
      // Respond to keep-alive
      sendSignalingMessage(ws, { type: "pong" });
      return true;

    case "bye":
      console.log("Server requested disconnect");
      stopStreaming();
      return true;

    default:
      console.log("Unknown signaling message type:", json.type);
      return false;
  }
}

/**
 * Handle SDP offer and set up WebRTC
 */
async function handleSdpOffer(
  serverSdp: string,
  ws: WebSocket,
  accessToken: string,
  config: WebRtcConfig
): Promise<void> {
  console.log("Setting up WebRTC peer connection");

  // Create RTCPeerConnection with proper configuration
  const pc = new RTCPeerConnection({
    iceServers: config.ice_servers.map((s) => ({
      urls: s.urls,
      username: s.username,
      credential: s.credential,
    })),
    bundlePolicy: "max-bundle",
    rtcpMuxPolicy: "require",
    iceCandidatePoolSize: 2,
  });

  streamingState.peerConnection = pc;

  // Set up event handlers
  pc.ontrack = handleTrack;
  pc.onicecandidate = (event) => handleIceCandidate(event, ws);
  pc.oniceconnectionstatechange = () => {
    console.log("ICE connection state:", pc.iceConnectionState);
    if (pc.iceConnectionState === "failed") {
      console.error("ICE connection failed, attempting restart");
      pc.restartIce();
    }
  };
  pc.onconnectionstatechange = () => {
    console.log("Connection state:", pc.connectionState);
    if (pc.connectionState === "connected") {
      streamingState.connected = true;
      console.log("WebRTC connected!");
      startStatsCollection();
    } else if (pc.connectionState === "failed" || pc.connectionState === "disconnected") {
      streamingState.connected = false;
    }
  };

  // === CRITICAL: Create input channel BEFORE SDP negotiation (per official GFN client) ===
  // The official GFN web client creates data channels during RTCPeerConnection setup,
  // BEFORE calling setRemoteDescription.
  console.log("Creating input_channel_v1 BEFORE SDP negotiation...");
  const inputChannel = pc.createDataChannel("input_channel_v1", {
    ordered: false,        // Unordered for lowest latency
    maxRetransmits: 0,     // No retransmits for lowest latency
  });
  inputChannel.binaryType = "arraybuffer";

  inputChannel.onopen = () => {
    console.log("=== INPUT CHANNEL OPENED ===");
    console.log("  Label:", inputChannel.label);
    console.log("  ID:", inputChannel.id);
    streamingState.dataChannels.set("input_channel_v1", inputChannel);
    streamingState.dataChannels.set("input", inputChannel);
    console.log("  Waiting for server handshake...");
  };

  inputChannel.onmessage = (e) => {
    const size = e.data instanceof ArrayBuffer ? e.data.byteLength : 0;
    console.log("=== INPUT CHANNEL MESSAGE ===", "Size:", size, "bytes");

    if (e.data instanceof ArrayBuffer && size > 0) {
      const view = new DataView(e.data);
      if (!inputHandshakeComplete && size >= 2) {
        const firstWord = view.getUint16(0, true);
        if (firstWord === 526) {
          const version = size >= 4 ? view.getUint16(2, true) : 0;
          console.log(`  *** HANDSHAKE: New format (0x020E), version=${version}`);
          inputProtocolVersion = version;
        } else {
          console.log(`  *** HANDSHAKE: Old format, version=${firstWord}`);
          inputProtocolVersion = firstWord;
        }
        inputHandshakeComplete = true;
        inputHandshakeAttempts++;
        streamStartTime = Date.now();
        console.log("  *** INPUT HANDSHAKE COMPLETE!");
      }
    }
  };

  inputChannel.onerror = (e) => console.error("Input channel error:", e);
  inputChannel.onclose = () => {
    console.log("Input channel closed");
    streamingState.dataChannels.delete("input");
    streamingState.dataChannels.delete("input_channel_v1");
  };

  console.log("Input channel created, state:", inputChannel.readyState);

  // Add transceiver for video to ensure we can receive
  pc.addTransceiver("video", { direction: "recvonly" });
  pc.addTransceiver("audio", { direction: "recvonly" });

  // Set remote description (server's SDP offer)
  const remoteDesc = new RTCSessionDescription({
    type: "offer",
    sdp: serverSdp,
  });

  await pc.setRemoteDescription(remoteDesc);
  console.log("Remote description set");

  // Create answer
  const answer = await pc.createAnswer({
    offerToReceiveVideo: true,
    offerToReceiveAudio: true,
  });

  // Modify SDP to prefer certain codecs if needed
  if (answer.sdp) {
    answer.sdp = preferCodec(answer.sdp, config.video_codec);
  }

  await pc.setLocalDescription(answer);
  console.log("Local description set");

  // Wait for ICE gathering to complete (or timeout)
  await waitForIceGathering(pc);

  // Send answer to server
  const answerMsg: NvstAnswerMessage = {
    type: "answer",
    payload: {
      sdp: pc.localDescription?.sdp || answer.sdp || "",
    },
  };
  sendSignalingMessage(ws, answerMsg);
  console.log("Answer sent to server");
}

/**
 * Wait for ICE gathering to complete
 */
async function waitForIceGathering(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") {
    return;
  }

  return new Promise((resolve) => {
    const checkState = () => {
      if (pc.iceGatheringState === "complete") {
        pc.removeEventListener("icegatheringstatechange", checkState);
        resolve();
      }
    };

    pc.addEventListener("icegatheringstatechange", checkState);

    // Timeout after 5 seconds
    setTimeout(() => {
      pc.removeEventListener("icegatheringstatechange", checkState);
      resolve();
    }, 5000);
  });
}

/**
 * Modify SDP to force a specific video codec by removing all other codecs
 */
function preferCodec(sdp: string, codec: string): string {
  // Map user codec selection to actual SDP codec name
  const codecMap: Record<string, string> = {
    H264: "H264",
    H265: "H265",
    HEVC: "H265",   // HEVC is the same as H265
    AV1: "AV1",
  };

  const preferredCodec = codecMap[codec.toUpperCase()] || "H264";
  console.log("Forcing SDP to use codec:", preferredCodec);

  const lines = sdp.split("\r\n");
  const result: string[] = [];

  let inVideoSection = false;
  const codecPayloads: Map<string, string[]> = new Map(); // codec name -> payload types
  const payloadToCodec: Map<string, string> = new Map(); // payload type -> codec name

  // First pass: collect codec information
  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
    } else if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }

    if (inVideoSection) {
      // Parse rtpmap lines to get codec -> payload mapping
      const rtpmapMatch = line.match(/^a=rtpmap:(\d+)\s+([^\/]+)/);
      if (rtpmapMatch) {
        const pt = rtpmapMatch[1];
        const codecName = rtpmapMatch[2].toUpperCase();
        if (!codecPayloads.has(codecName)) {
          codecPayloads.set(codecName, []);
        }
        codecPayloads.get(codecName)!.push(pt);
        payloadToCodec.set(pt, codecName);
      }
    }
  }

  // Get preferred codec payload types
  const preferredPayloads = codecPayloads.get(preferredCodec) || [];
  const preferredPayloadSet = new Set(preferredPayloads);

  if (preferredPayloads.length === 0) {
    console.log("Preferred codec not found in SDP, returning original. Available:", Array.from(codecPayloads.keys()).join(", "));
    return sdp;
  }

  console.log("Available codecs:", Array.from(codecPayloads.entries()).map(([k, v]) => `${k}:${v.join(",")}`).join(" | "));
  console.log("Keeping only payload types:", preferredPayloads.join(", "));

  // Second pass: rebuild SDP keeping only preferred codec
  inVideoSection = false;
  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      // Rewrite m=video line to only include preferred codec payload types
      const parts = line.split(" ");
      const header = parts.slice(0, 3); // m=video, port, proto
      const payloadTypes = parts.slice(3);

      // Keep only preferred codec payloads
      const filtered = payloadTypes.filter(pt => preferredPayloadSet.has(pt));

      if (filtered.length > 0) {
        result.push([...header, ...filtered].join(" "));
        console.log("Filtered m=video line to payloads:", filtered.join(", "));
      } else {
        result.push(line); // Fallback to original if filter removed everything
      }
      continue;
    } else if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }

    if (inVideoSection) {
      // Filter out rtpmap, fmtp, rtcp-fb lines for non-preferred codecs
      const ptMatch = line.match(/^a=(?:rtpmap|fmtp|rtcp-fb):(\d+)/);
      if (ptMatch) {
        const pt = ptMatch[1];
        if (!preferredPayloadSet.has(pt)) {
          // Skip this line - it's for a codec we don't want
          continue;
        }
      }
    }

    result.push(line);
  }

  return result.join("\r\n");
}

/**
 * Handle incoming media track
 */
// Shared media stream for all tracks
let sharedMediaStream: MediaStream | null = null;

function handleTrack(event: RTCTrackEvent): void {
  console.log("Track received:", event.track.kind, event.track.id, "readyState:", event.track.readyState);
  console.log("Track settings:", JSON.stringify(event.track.getSettings()));

  // === LOW LATENCY: Minimize jitter buffer ===
  if (event.receiver) {
    try {
      // Set minimum jitter buffer delay for lowest latency
      // This may cause more frame drops but reduces latency
      if ('jitterBufferTarget' in event.receiver) {
        (event.receiver as any).jitterBufferTarget = 0; // Minimum buffering
        console.log("Set jitterBufferTarget to 0 for low latency");
      }
      // Also try playoutDelayHint if available
      if ('playoutDelayHint' in event.receiver) {
        (event.receiver as any).playoutDelayHint = 0;
        console.log("Set playoutDelayHint to 0 for low latency");
      }
    } catch (e) {
      console.log("Could not set jitter buffer target:", e);
    }
  }

  // Get or create the shared MediaStream
  let stream: MediaStream;
  if (event.streams && event.streams[0]) {
    stream = event.streams[0];
    sharedMediaStream = stream;
    console.log("Using stream from event, tracks:", stream.getTracks().map(t => t.kind).join(", "));
  } else {
    // Track arrived without a stream - create one or add to existing
    if (!sharedMediaStream) {
      sharedMediaStream = new MediaStream();
      console.log("Created new MediaStream for orphan track");
    }
    sharedMediaStream.addTrack(event.track);
    stream = sharedMediaStream;
    console.log("Added orphan track to shared stream");
  }

  // Always ensure video element has the stream
  if (streamingState.videoElement) {
    if (!streamingState.videoElement.srcObject) {
      console.log("Setting srcObject on video element");
      streamingState.videoElement.srcObject = stream;
    } else if (streamingState.videoElement.srcObject !== stream) {
      // Different stream - add tracks to existing
      const existingStream = streamingState.videoElement.srcObject as MediaStream;
      if (!existingStream.getTracks().find(t => t.id === event.track.id)) {
        existingStream.addTrack(event.track);
        console.log("Added track to existing video srcObject");
      }
    }

    // Log current stream state
    const currentStream = streamingState.videoElement.srcObject as MediaStream;
    console.log("Video element stream tracks:", currentStream?.getTracks().map(t => `${t.kind}:${t.readyState}`).join(", "));
  }

  if (event.track.kind === "video") {
    console.log("Video track details - enabled:", event.track.enabled, "muted:", event.track.muted);

    if (streamingState.videoElement) {
      // Ensure video plays
      streamingState.videoElement.play().catch((e) => {
        console.warn("Video autoplay blocked:", e);
        const clickHandler = () => {
          streamingState.videoElement?.play();
          document.removeEventListener("click", clickHandler);
        };
        document.addEventListener("click", clickHandler);
      });

      // Log video element state
      console.log("Video element state - readyState:", streamingState.videoElement.readyState,
        "networkState:", streamingState.videoElement.networkState,
        "paused:", streamingState.videoElement.paused,
        "videoWidth:", streamingState.videoElement.videoWidth,
        "videoHeight:", streamingState.videoElement.videoHeight);
    }
  } else if (event.track.kind === "audio") {
    console.log("Audio track details - enabled:", event.track.enabled, "muted:", event.track.muted);
    // Audio is played through the video element's srcObject - no need for separate AudioContext
    // Using AudioContext would cause double audio playback
  }

  // Handle track end
  event.track.onended = () => {
    console.log("Track ended:", event.track.kind, event.track.id);
  };

  event.track.onmute = () => {
    console.log("Track muted:", event.track.kind, event.track.id);
  };

  event.track.onunmute = () => {
    console.log("Track unmuted:", event.track.kind, event.track.id);
  };
}

/**
 * Handle ICE candidate
 */
function handleIceCandidate(event: RTCPeerConnectionIceEvent, ws: WebSocket): void {
  if (event.candidate) {
    console.log("Local ICE candidate:", event.candidate.candidate.substring(0, 50));

    // Send ICE candidate to server
    const candidateMsg: NvstIceCandidateMessage = {
      type: "ice-candidate",
      payload: {
        candidate: event.candidate.candidate,
        sdpMid: event.candidate.sdpMid,
        sdpMLineIndex: event.candidate.sdpMLineIndex,
      },
    };

    sendSignalingMessage(ws, candidateMsg);
  } else {
    console.log("ICE gathering complete");
  }
}

/**
 * Create data channels for input
 */
function createDataChannels(pc: RTCPeerConnection): void {
  // Match official GFN client data channel configuration
  // From logs: control_channel_reliable, input_channel_partially_reliable

  // Control channel - reliable, ordered (SCTP stream 0)
  const controlChannel = pc.createDataChannel("control_channel_reliable", {
    ordered: true,
    id: 0,  // Explicit SCTP stream ID
  });
  controlChannel.binaryType = "arraybuffer";
  controlChannel.onopen = () => {
    console.log("Control channel open");
    // Send initial control handshake if needed
  };
  controlChannel.onerror = (e) => console.error("Control channel error:", e);
  controlChannel.onclose = () => console.log("Control channel closed");
  controlChannel.onmessage = (e) => {
    console.log("Control channel message received, size:", (e.data as ArrayBuffer).byteLength);
  };
  streamingState.dataChannels.set("control", controlChannel);

  // Input channel - unreliable for lowest latency
  const inputChannel = pc.createDataChannel("input_channel_partially_reliable", {
    ordered: false,          // Unordered for lowest latency
    maxRetransmits: 0,       // No retransmits - stale input is useless
    id: 6,  // SCTP stream 6 per logs
  });
  inputChannel.binaryType = "arraybuffer";
  inputChannel.onopen = () => console.log("Input channel open");
  inputChannel.onerror = (e) => console.error("Input channel error:", e);
  inputChannel.onclose = () => console.log("Input channel closed");
  streamingState.dataChannels.set("input", inputChannel);

  // Custom message channel - reliable (SCTP stream 2)
  const customChannel = pc.createDataChannel("custom_message_on_sctp_private_reliable", {
    ordered: true,
    id: 2,
  });
  customChannel.binaryType = "arraybuffer";
  customChannel.onopen = () => console.log("Custom message channel open");
  customChannel.onmessage = (e) => {
    console.log("Custom message received, size:", (e.data as ArrayBuffer).byteLength);
  };
  streamingState.dataChannels.set("custom", customChannel);

  // Also handle incoming data channels from server
  pc.ondatachannel = (event) => {
    const channel = event.channel;
    console.log("Incoming data channel:", channel.label, "id:", channel.id);
    channel.binaryType = "arraybuffer";
    channel.onopen = () => console.log(`Server channel '${channel.label}' open`);
    channel.onmessage = (e) => {
      console.log(`Server channel '${channel.label}' message, size:`, (e.data as ArrayBuffer).byteLength);
    };
    streamingState.dataChannels.set(channel.label, channel);
  };
}

/**
 * Handle stats message from server
 */
function handleStatsMessage(event: MessageEvent): void {
  try {
    const data = new Uint8Array(event.data as ArrayBuffer);
    // Parse server-side stats (format TBD based on NVST protocol)
    console.log("Server stats received, bytes:", data.length);
  } catch (e) {
    console.warn("Failed to parse stats message:", e);
  }
}

/**
 * Start periodic stats collection
 */
function startStatsCollection(): void {
  const collectStats = async () => {
    if (!streamingState.connected || !streamingState.peerConnection) {
      return;
    }

    const stats = await getStreamingStats();
    if (stats) {
      streamingState.stats = stats;
      // Could emit an event here for UI updates
    }

    // Continue collecting
    if (streamingState.connected) {
      setTimeout(collectStats, 1000);
    }
  };

  collectStats();
}

/**
 * Send input event over data channel
 *
 * Strategy: Use binary protocol on input channel (primary), with JSON as fallback
 */
// Track input channel handshake state
let inputHandshakeComplete = false;
let inputHandshakeAttempts = 0;
let inputProtocolVersion = 0; // GFN input protocol version from server handshake

// Input event counter for debugging
let inputEventCount = 0;
let lastInputLogTime = 0;

// Prefer binary protocol over JSON - only use JSON if binary isn't working
let preferBinaryInput = true;

/**
 * Get the best available input channel
 */
function getBestInputChannel(): RTCDataChannel | null {
  // Priority order for input channels:
  // 1. Server-created input channel (labeled with "input" or "ri_")
  // 2. Client-created input channel
  // 3. Fall back to control channel for JSON input

  // Try server's input channel first
  const serverInput = streamingState.dataChannels.get("server_input");
  if (serverInput && serverInput.readyState === "open") {
    return serverInput;
  }

  // Try primary input channel
  const inputChannel = streamingState.dataChannels.get("input");
  if (inputChannel && inputChannel.readyState === "open") {
    return inputChannel;
  }

  // Try input_channel_v1
  const inputV1 = streamingState.dataChannels.get("input_channel_v1");
  if (inputV1 && inputV1.readyState === "open") {
    return inputV1;
  }

  // Search through all channels for one containing 'input'
  for (const [name, channel] of streamingState.dataChannels.entries()) {
    if (name.toLowerCase().includes("input") && channel.readyState === "open") {
      return channel;
    }
  }

  return null;
}

export function sendInputEvent(event: InputEvent): void {
  const inputChannel = getBestInputChannel();
  const controlChannel = streamingState.dataChannels.get("control");

  // Need at least one channel
  if (!inputChannel && !controlChannel) {
    return;
  }

  // Initialize debug logging set
  if (!streamingState.inputDebugLogged) {
    streamingState.inputDebugLogged = new Set();
  }

  inputEventCount++;

  // Log input state only on first input (avoid GC pauses from logging)
  if (inputEventCount === 1) {
    console.log("=== FIRST INPUT EVENT ===");
    console.log(`  Input channel: ${inputChannel?.label || 'none'} (${inputChannel?.readyState || 'n/a'})`);
    console.log(`  Handshake complete: ${inputHandshakeComplete}`);
    console.log("=========================");
  }

  try {
    // Primary: Send binary format on input channel
    if (inputChannel && inputChannel.readyState === "open" && preferBinaryInput) {
      const encoded = encodeInputEvent(event);

      // Only send if we have valid data
      if (encoded.byteLength > 0) {
        // For protocol version > 2, prepend 10-byte header:
        // [0x23 (1 byte)][timestamp (8 bytes BE)][0x22 wrapper (1 byte)]
        // The 0x22 (34) byte is a single-event wrapper required by v3 protocol
        let finalPacket: ArrayBuffer;
        if (inputProtocolVersion > 2) {
          const now = Date.now();
          const relativeMs = streamStartTime > 0 ? now - streamStartTime : now;
          const timestampUs = BigInt(relativeMs) * BigInt(1000);

          // Create packet with 10-byte header prefix (9-byte v3 header + 1-byte wrapper)
          finalPacket = new ArrayBuffer(10 + encoded.byteLength);
          const headerView = new DataView(finalPacket);
          const packetBytes = new Uint8Array(finalPacket);

          // Header byte 0: type marker 0x23 (35)
          headerView.setUint8(0, 0x23);
          // Header bytes 1-8: timestamp in microseconds (BE)
          headerView.setBigUint64(1, timestampUs, false);
          // Header byte 9: single event wrapper 0x22 (34)
          headerView.setUint8(9, 0x22);
          // Copy original payload after header
          packetBytes.set(new Uint8Array(encoded), 10);
        } else {
          finalPacket = encoded;
        }

        inputChannel.send(finalPacket);

        // Log first of each type
        if (!streamingState.inputDebugLogged.has(event.type + "_binary")) {
          streamingState.inputDebugLogged.add(event.type + "_binary");
          const bytes = new Uint8Array(finalPacket);
          console.log(`First binary input (${event.type}):`);
          console.log(`  Channel: ${inputChannel.label}`);
          console.log(`  Protocol version: ${inputProtocolVersion}`);
          console.log(`  Bytes: ${bytes.length}${inputProtocolVersion > 2 ? ' (includes 10-byte v3 header)' : ''}`);
          console.log(`  Hex: ${Array.from(bytes.slice(0, 45)).map(b => b.toString(16).padStart(2, '0')).join(' ')}${bytes.length > 45 ? '...' : ''}`);
        }
      }
    }

    // Fallback: Send JSON format on control channel if binary isn't preferred or failed
    if (controlChannel && controlChannel.readyState === "open" && !preferBinaryInput) {
      const jsonMsg = encodeInputAsJson(event);
      if (jsonMsg) {
        controlChannel.send(jsonMsg);

        if (!streamingState.inputDebugLogged.has(event.type + "_json")) {
          streamingState.inputDebugLogged.add(event.type + "_json");
          console.log(`First JSON input (${event.type}): ${jsonMsg.substring(0, 80)}...`);
        }
      }
    }
  } catch (e) {
    console.error("Failed to send input event:", e);
  }
}

/**
 * Force initialize input handshake (call this if input isn't working)
 */
export function forceInputHandshake(): void {
  const inputChannel = getBestInputChannel();
  if (!inputChannel) {
    console.error("No input channel available for handshake");
    return;
  }

  console.log("Forcing input handshake on channel:", inputChannel.label);

  // Send handshake initiation: [0x0e, version_major, version_minor, flags]
  // Version 14.0 based on GFN client analysis
  const handshake = new Uint8Array([0x0e, 0x0e, 0x00, 0x00]);
  try {
    inputChannel.send(handshake.buffer);
    console.log("Handshake sent:", Array.from(handshake).map(b => b.toString(16).padStart(2, '0')).join(' '));
    inputHandshakeAttempts++;
  } catch (e) {
    console.error("Failed to send handshake:", e);
  }
}

/**
 * Check if input is ready
 */
export function isInputReady(): boolean {
  return inputHandshakeComplete && getBestInputChannel() !== null;
}

/**
 * Get input debug info
 */
export function getInputDebugInfo(): object {
  return {
    handshakeComplete: inputHandshakeComplete,
    handshakeAttempts: inputHandshakeAttempts,
    eventCount: inputEventCount,
    streamStartTime,
    inputChannel: getBestInputChannel()?.label || null,
    inputChannelState: getBestInputChannel()?.readyState || null,
    channels: Array.from(streamingState.dataChannels.entries()).map(([name, ch]) => ({
      name,
      label: ch.label,
      state: ch.readyState,
      id: ch.id
    }))
  };
}

/**
 * Encode input event as JSON (matching GFN web client format)
 */
function encodeInputAsJson(event: InputEvent): string | null {
  const timestamp = Date.now();

  switch (event.type) {
    case "mouse_move": {
      const data = event.data as MouseMoveData;
      // Format matching GFN web client
      return JSON.stringify({
        inputEvent: {
          eventName: "mouseMove",
          movementX: data.dx,
          movementY: data.dy,
          timestamp
        }
      });
    }

    case "mouse_button": {
      const data = event.data as MouseButtonData;
      return JSON.stringify({
        inputEvent: {
          eventName: data.pressed ? "mouseDown" : "mouseUp",
          button: data.button,
          timestamp
        }
      });
    }

    case "mouse_wheel": {
      const data = event.data as MouseWheelData;
      return JSON.stringify({
        inputEvent: {
          eventName: "wheel",
          deltaX: data.deltaX,
          deltaY: data.deltaY,
          timestamp
        }
      });
    }

    case "key": {
      const data = event.data as KeyData;
      return JSON.stringify({
        inputEvent: {
          eventName: data.pressed ? "keyDown" : "keyUp",
          keyCode: data.keyCode,
          scanCode: data.scanCode,
          modifiers: data.modifiers,
          timestamp
        }
      });
    }

    default:
      return null;
  }
}

/**
 * Input event types
 */
export interface InputEvent {
  type: "mouse_move" | "mouse_button" | "mouse_wheel" | "key";
  data: MouseMoveData | MouseButtonData | MouseWheelData | KeyData;
}

interface MouseMoveData {
  dx: number;
  dy: number;
  absolute?: boolean;
  x?: number;
  y?: number;
}

interface MouseButtonData {
  button: number;
  pressed: boolean;
}

interface MouseWheelData {
  deltaX: number;
  deltaY: number;
}

interface KeyData {
  keyCode: number;
  scanCode: number;
  pressed: boolean;
  modifiers: number;
}

// GFN Input Protocol Constants (from vendor.js analysis)
// Type: Little Endian, Data fields: Big Endian, Timestamp: 8B Big Endian microseconds
const GFN_INPUT_KEY_DOWN = 3;
const GFN_INPUT_KEY_UP = 4;
const GFN_INPUT_MOUSE_ABS = 5;
const GFN_INPUT_MOUSE_REL = 7;
const GFN_INPUT_MOUSE_BUTTON_DOWN = 8;
const GFN_INPUT_MOUSE_BUTTON_UP = 9;
const GFN_INPUT_MOUSE_WHEEL = 10;

// GFN Modifier flags (from vendor.js mS function)
const GFN_MOD_SHIFT = 1;
const GFN_MOD_CTRL = 2;
const GFN_MOD_ALT = 4;
const GFN_MOD_META = 8;

// Browser code to Windows Virtual Key code mapping (from vendor.js so map)
const CODE_TO_VK: Record<string, number> = {
  "Escape": 27, "Digit0": 48, "Digit1": 49, "Digit2": 50, "Digit3": 51,
  "Digit4": 52, "Digit5": 53, "Digit6": 54, "Digit7": 55, "Digit8": 56, "Digit9": 57,
  "Minus": 189, "Equal": 187, "Backspace": 8, "Tab": 9,
  "KeyQ": 81, "KeyW": 87, "KeyE": 69, "KeyR": 82, "KeyT": 84, "KeyY": 89,
  "KeyU": 85, "KeyI": 73, "KeyO": 79, "KeyP": 80,
  "BracketLeft": 219, "BracketRight": 221, "Enter": 13,
  "ControlLeft": 162, "ControlRight": 163,
  "KeyA": 65, "KeyS": 83, "KeyD": 68, "KeyF": 70, "KeyG": 71, "KeyH": 72,
  "KeyJ": 74, "KeyK": 75, "KeyL": 76,
  "Semicolon": 186, "Quote": 222, "Backquote": 192,
  "ShiftLeft": 160, "ShiftRight": 161,
  "Backslash": 220, "IntlBackslash": 226,
  "KeyZ": 90, "KeyX": 88, "KeyC": 67, "KeyV": 86, "KeyB": 66, "KeyN": 78, "KeyM": 77,
  "Comma": 188, "Period": 190, "Slash": 191,
  "NumpadMultiply": 106, "NumpadDivide": 111, "NumpadSubtract": 109,
  "NumpadAdd": 107, "NumpadEnter": 13, "NumpadDecimal": 110,
  "Numpad0": 96, "Numpad1": 97, "Numpad2": 98, "Numpad3": 99, "Numpad4": 100,
  "Numpad5": 101, "Numpad6": 102, "Numpad7": 103, "Numpad8": 104, "Numpad9": 105,
  "AltLeft": 164, "AltRight": 165, "Space": 32, "CapsLock": 20,
  "F1": 112, "F2": 113, "F3": 114, "F4": 115, "F5": 116, "F6": 117,
  "F7": 118, "F8": 119, "F9": 120, "F10": 121, "F11": 122, "F12": 123,
  "F13": 124, "F14": 125, "F15": 126, "F16": 127, "F17": 128, "F18": 129,
  "F19": 130, "F20": 131, "F21": 132, "F22": 133, "F23": 134, "F24": 135,
  "Pause": 19, "ScrollLock": 145, "NumLock": 144, "PrintScreen": 42,
  "Home": 36, "End": 35, "PageUp": 33, "PageDown": 34,
  "ArrowUp": 38, "ArrowDown": 40, "ArrowLeft": 37, "ArrowRight": 39,
  "Insert": 45, "Delete": 46,
  "MetaLeft": 91, "MetaRight": 92, "OSLeft": 91, "OSRight": 92,
  "ContextMenu": 93,
  // International keys
  "IntlRo": 194, "IntlYen": 193, "KanaMode": 233,
  "Lang1": 21, "Lang2": 25, "Convert": 234, "NonConvert": 235,
};

// Get Windows Virtual Key code from browser event
function getVirtualKeyCode(e: KeyboardEvent): number {
  // First try to map from e.code
  if (e.code && CODE_TO_VK[e.code] !== undefined) {
    return CODE_TO_VK[e.code];
  }
  // Fallback to keyCode (deprecated but still works)
  return e.keyCode;
}

// Get GFN modifier flags from browser event
function getModifierFlags(e: KeyboardEvent): number {
  let flags = 0;
  // Only include modifier flags if the key itself isn't a modifier
  if (e.shiftKey && !e.code.startsWith("Shift")) flags |= GFN_MOD_SHIFT;
  if (e.ctrlKey && !e.code.startsWith("Control")) flags |= GFN_MOD_CTRL;
  if (e.altKey && !e.code.startsWith("Alt")) flags |= GFN_MOD_ALT;
  if (e.metaKey && !e.code.startsWith("Meta") && !e.code.startsWith("OS")) flags |= GFN_MOD_META;
  return flags;
}

// Use wrapper byte in packets (0xFF) - set to false to try without
const USE_WRAPPER_BYTE = false;

// Stream start time for relative timestamps
let streamStartTime = 0;

/**
 * Encode input event for GFN protocol (from deobfuscated vendor.js)
 *
 * Format discovered from vendor.js:
 * - Event type: 4 bytes, LITTLE ENDIAN
 * - Data fields: BIG ENDIAN
 * - Timestamp: 8 bytes, BIG ENDIAN, in MICROSECONDS (ms * 1000)
 *
 * Mouse Relative (22 bytes): Gc function
 *   [type 4B LE][dx 2B BE][dy 2B BE][reserved 2B][reserved 4B][timestamp 8B BE μs]
 *
 * Mouse Button (18 bytes): xc function
 *   [type 4B LE][button 1B][pad 1B][reserved 4B][timestamp 8B BE μs]
 *
 * Keyboard (18 bytes): Yc function
 *   [type 4B LE][keycode 2B BE][modifiers 2B BE][reserved 2B][timestamp 8B BE μs]
 */
function encodeInputEvent(event: InputEvent): ArrayBuffer {
  // Timestamp in microseconds (ms * 1000), relative to stream start
  const now = Date.now();
  const relativeMs = streamStartTime > 0 ? now - streamStartTime : now;
  const timestampUs = BigInt(relativeMs) * BigInt(1000);

  switch (event.type) {
    case "mouse_move": {
      const data = event.data as MouseMoveData;

      // Check if we should use absolute positioning
      if (data.absolute && data.x !== undefined && data.y !== undefined) {
        // Mouse Absolute (Gc with absolute=true): 26 bytes
        // From GFN vendor.js analysis:
        // [type 4B LE][x 2B BE][y 2B BE][reserved 2B BE][refWidth 2B BE][refHeight 2B BE][reserved 4B][timestamp 8B BE]
        const buffer = new ArrayBuffer(26);
        const view = new DataView(buffer);
        view.setUint32(0, GFN_INPUT_MOUSE_ABS, true);   // Type 5, LE
        view.setUint16(4, data.x, false);               // Absolute X, BE (0-65535)
        view.setUint16(6, data.y, false);               // Absolute Y, BE (0-65535)
        view.setUint16(8, 0, false);                    // Reserved, BE
        view.setUint16(10, 65535, false);               // Reference width, BE
        view.setUint16(12, 65535, false);               // Reference height, BE
        view.setUint32(14, 0, false);                   // Reserved
        view.setBigUint64(18, timestampUs, false);      // Timestamp μs, BE
        return buffer;
      }

      // Mouse Relative (Gc with absolute=false): 22 bytes
      // [type 4B LE][dx 2B BE][dy 2B BE][reserved 2B BE][reserved 4B][timestamp 8B BE]
      const buffer = new ArrayBuffer(22);
      const view = new DataView(buffer);
      view.setUint32(0, GFN_INPUT_MOUSE_REL, true);   // Type 7, LE
      view.setInt16(4, data.dx, false);               // Delta X, BE (signed)
      view.setInt16(6, data.dy, false);               // Delta Y, BE (signed)
      view.setUint16(8, 0, false);                    // Reserved, BE
      view.setUint32(10, 0, false);                   // Reserved
      view.setBigUint64(14, timestampUs, false);      // Timestamp μs, BE
      return buffer;
    }

    case "mouse_button": {
      // Mouse Button (xc): 18 bytes
      // [type 4B LE][button 1B][pad 1B][reserved 4B][timestamp 8B BE]
      // Button mapping: ja(button) = button + 1, so 0→1 (left), 1→2 (right), 2→3 (middle)
      const data = event.data as MouseButtonData;
      const eventType = data.pressed ? GFN_INPUT_MOUSE_BUTTON_DOWN : GFN_INPUT_MOUSE_BUTTON_UP;
      const gfnButton = data.button + 1;  // GFN uses 1-based button indices
      const buffer = new ArrayBuffer(18);
      const view = new DataView(buffer);
      const bytes = new Uint8Array(buffer);
      view.setUint32(0, eventType, true);             // Type 8 or 9, LE
      bytes[4] = gfnButton;                           // Button as uint8 (1=left, 2=right, 3=middle)
      bytes[5] = 0;                                   // Padding
      view.setUint32(6, 0);                           // Reserved
      view.setBigUint64(10, timestampUs, false);      // Timestamp μs, BE
      return buffer;
    }

    case "mouse_wheel": {
      // Mouse Wheel (Lc): 22 bytes
      // [type 4B LE][horiz 2B BE][vert 2B BE][reserved 2B BE][reserved 4B][timestamp 8B BE]
      const data = event.data as MouseWheelData;
      // GFN expects wheel delta as multiples of 120, negated
      const wheelDelta = Math.round(data.deltaY / Math.abs(data.deltaY || 1) * -120);
      const buffer = new ArrayBuffer(22);
      const view = new DataView(buffer);
      view.setUint32(0, GFN_INPUT_MOUSE_WHEEL, true); // Type 10, LE
      view.setInt16(4, 0, false);                     // Horizontal wheel, BE
      view.setInt16(6, wheelDelta, false);            // Vertical wheel, BE
      view.setUint16(8, 0, false);                    // Reserved, BE
      view.setUint32(10, 0);                          // Reserved
      view.setBigUint64(14, timestampUs, false);      // Timestamp μs, BE
      return buffer;
    }

    case "key": {
      // Keyboard (Yc): 18 bytes
      // [type 4B LE][keycode 2B BE][modifiers 2B BE][reserved 2B BE][timestamp 8B BE]
      const data = event.data as KeyData;
      const eventType = data.pressed ? GFN_INPUT_KEY_DOWN : GFN_INPUT_KEY_UP;
      const buffer = new ArrayBuffer(18);
      const view = new DataView(buffer);
      view.setUint32(0, eventType, true);             // Type 3 or 4, LE
      view.setUint16(4, data.keyCode, false);         // Key code, BE
      view.setUint16(6, data.modifiers, false);       // Modifiers, BE
      view.setUint16(8, data.scanCode || 0, false);   // Reserved (or scancode), BE
      view.setBigUint64(10, timestampUs, false);      // Timestamp μs, BE
      return buffer;
    }

    default:
      return new ArrayBuffer(0);
  }
}

// Input capture mode - can be 'pointerlock' (FPS games) or 'absolute' (desktop/menu)
let inputCaptureMode: 'pointerlock' | 'absolute' = 'absolute';

// Track if input is active (video element is focused/active)
let inputCaptureActive = false;

// Platform detection
const isMacOS = navigator.platform.toUpperCase().includes("MAC") ||
  navigator.userAgent.toUpperCase().includes("MAC");
const isWindows = navigator.platform.toUpperCase().includes("WIN") ||
  navigator.userAgent.toUpperCase().includes("WIN");

// Track if we're using native cursor capture (bypasses browser pointer lock)
// This is used on macOS (Core Graphics) and Windows (Win32 ClipCursor)
let nativeCursorCaptured = false;

// Windows high-frequency mouse polling state
let mousePollingActive = false;
let mousePollingInterval: number | null = null;
let mousePollingChannel: MessageChannel | null = null;

// Input latency tracking
const INPUT_LATENCY_SAMPLES = 100; // Rolling average over last 100 samples
let ipcLatencySamples: number[] = [];
let sendLatencySamples: number[] = [];
let totalLatencySamples: number[] = [];
let inputEventTimestamps: number[] = []; // For calculating events per second
let lastInputStatsLog = 0;

// Get average from samples array
function getAverage(samples: number[]): number {
  if (samples.length === 0) return 0;
  return samples.reduce((a, b) => a + b, 0) / samples.length;
}

// Add sample to rolling buffer
function addSample(samples: number[], value: number): void {
  samples.push(value);
  if (samples.length > INPUT_LATENCY_SAMPLES) {
    samples.shift();
  }
}

// Calculate input events per second
function getInputRate(): number {
  const now = performance.now();
  // Remove timestamps older than 1 second
  while (inputEventTimestamps.length > 0 && now - inputEventTimestamps[0] > 1000) {
    inputEventTimestamps.shift();
  }
  return inputEventTimestamps.length;
}

// Export input latency stats for getStreamingStats
export function getInputLatencyStats(): { ipc: number; send: number; total: number; rate: number } {
  return {
    ipc: Math.round(getAverage(ipcLatencySamples) * 100) / 100,
    send: Math.round(getAverage(sendLatencySamples) * 100) / 100,
    total: Math.round(getAverage(totalLatencySamples) * 100) / 100,
    rate: getInputRate(),
  };
}

// Start high-frequency mouse polling on Windows
// Uses native 1000Hz polling thread + MessageChannel for minimal latency scheduling
const startMousePolling = async () => {
  if (!isWindows || mousePollingActive) return;

  try {
    const started = await invoke<boolean>("start_mouse_polling");
    if (started) {
      mousePollingActive = true;
      console.log("High-frequency mouse polling started (1000Hz native + MessageChannel polling)");

      // Use MessageChannel for tighter scheduling (bypasses 4ms timer clamping)
      mousePollingChannel = new MessageChannel();
      let lastPollTime = performance.now();
      const MIN_POLL_INTERVAL = 1; // Minimum 1ms between polls

      const pollMouse = () => {
        if (!mousePollingActive || !nativeCursorCaptured) {
          // Schedule next check even when inactive to allow resumption
          if (mousePollingActive) {
            mousePollingChannel?.port2.postMessage(null);
          }
          return;
        }

        const now = performance.now();
        const elapsed = now - lastPollTime;

        // Throttle to prevent overwhelming the IPC
        if (elapsed < MIN_POLL_INTERVAL) {
          mousePollingChannel?.port2.postMessage(null);
          return;
        }

        lastPollTime = now;
        const pipelineStart = now;

        // Get accumulated deltas from native polling thread (non-blocking)
        const ipcStart = performance.now();
        invoke<[number, number]>("get_accumulated_mouse_delta").then(([dx, dy]) => {
          const ipcEnd = performance.now();
          const ipcTime = ipcEnd - ipcStart;

          if (dx !== 0 || dy !== 0) {
            // Track IPC latency
            addSample(ipcLatencySamples, ipcTime);

            // Track input event timestamp for rate calculation
            inputEventTimestamps.push(performance.now());

            // Send input and measure send time
            const sendStart = performance.now();
            sendInputEvent({
              type: "mouse_move",
              data: { dx, dy },
            });
            const sendEnd = performance.now();
            const sendTime = sendEnd - sendStart;
            addSample(sendLatencySamples, sendTime);

            // Total pipeline latency
            const totalTime = sendEnd - pipelineStart;
            addSample(totalLatencySamples, totalTime);

            // Log stats periodically (every 5 seconds)
            const logNow = performance.now();
            if (logNow - lastInputStatsLog > 5000) {
              lastInputStatsLog = logNow;
              const stats = getInputLatencyStats();
              console.log(`[Input Stats] IPC: ${stats.ipc.toFixed(2)}ms | Send: ${stats.send.toFixed(2)}ms | Total: ${stats.total.toFixed(2)}ms | Rate: ${stats.rate}/s`);
            }
          }

          // Schedule next poll immediately after IPC completes
          if (mousePollingActive) {
            mousePollingChannel?.port2.postMessage(null);
          }
        }).catch(() => {
          // Schedule next poll even on error
          if (mousePollingActive) {
            mousePollingChannel?.port2.postMessage(null);
          }
        });
      };

      mousePollingChannel.port1.onmessage = pollMouse;
      // Start the polling loop
      mousePollingChannel.port2.postMessage(null);
    }
  } catch (e) {
    console.error("Failed to start mouse polling:", e);
  }
};

// Stop high-frequency mouse polling
const stopMousePolling = async () => {
  mousePollingActive = false;
  if (mousePollingInterval !== null) {
    clearInterval(mousePollingInterval);
    mousePollingInterval = null;
  }
  if (mousePollingChannel !== null) {
    mousePollingChannel.port1.close();
    mousePollingChannel.port2.close();
    mousePollingChannel = null;
  }
  try {
    await invoke("stop_mouse_polling");
    console.log("High-frequency mouse polling stopped");
  } catch (e) {
    // Ignore errors on stop
  }
};

/**
 * Set input capture mode
 * - 'pointerlock': Use pointer lock for relative mouse (FPS games)
 * - 'absolute': Send absolute coordinates without pointer lock (menus, desktop)
 *
 * On macOS and Windows, we use native OS APIs via Tauri commands to capture the cursor,
 * bypassing the browser's pointer lock which has issues (ESC exits, permission prompts).
 */
export async function setInputCaptureMode(mode: 'pointerlock' | 'absolute'): Promise<void> {
  const platform = isMacOS ? "macOS" : isWindows ? "Windows" : "other";
  console.log("Setting input capture mode:", mode, `(${platform})`);
  inputCaptureMode = mode;

  // On macOS and Windows, use native Tauri cursor capture
  if (isMacOS || isWindows) {
    try {
      if (mode === 'pointerlock') {
        // Use native cursor capture via Tauri
        // macOS: Core Graphics (CGAssociateMouseAndMouseCursorPosition)
        // Windows: Win32 (ClipCursor + ShowCursor)
        const captured = await invoke<boolean>("capture_cursor");
        if (captured) {
          nativeCursorCaptured = true;
          inputCaptureActive = true;
          // Also hide cursor via CSS as backup (prevents webview cursor flicker)
          const video = document.getElementById("gfn-stream-video") as HTMLVideoElement;
          const container = document.getElementById("streaming-container");
          if (video) video.style.cursor = 'none';
          if (container) container.style.cursor = 'none';
          document.body.style.cursor = 'none';
          // Start high-frequency mouse polling on Windows
          if (isWindows) {
            await startMousePolling();
          }
          console.log(`${platform}: Native cursor capture enabled`);
        } else {
          console.warn(`${platform}: Native cursor capture not available, falling back to CSS`);
          // Fallback to CSS cursor hiding
          const video = document.getElementById("gfn-stream-video") as HTMLVideoElement;
          const container = document.getElementById("streaming-container");
          if (video) video.style.cursor = 'none';
          if (container) container.style.cursor = 'none';
          document.body.style.cursor = 'none';
          nativeCursorCaptured = true;
          inputCaptureActive = true;
        }
      } else {
        // Stop mouse polling first
        if (isWindows) {
          await stopMousePolling();
        }
        // Release native cursor capture
        await invoke<boolean>("release_cursor");
        nativeCursorCaptured = false;
        // Restore cursor style
        const video = document.getElementById("gfn-stream-video") as HTMLVideoElement;
        const container = document.getElementById("streaming-container");
        if (video) video.style.cursor = 'default';
        if (container) container.style.cursor = 'default';
        document.body.style.cursor = 'default';
        console.log(`${platform}: Native cursor capture released`);
      }
    } catch (e) {
      console.error(`${platform} cursor capture error:`, e);
    }
  }
}

/**
 * Suspend cursor capture temporarily (e.g., when window loses focus)
 * This releases native cursor capture so user can interact with other apps
 */
export async function suspendCursorCapture(): Promise<void> {
  if (!nativeCursorCaptured) return;

  try {
    // Stop mouse polling first
    if (isWindows) {
      await stopMousePolling();
    }
    await invoke<boolean>("release_cursor");
    console.log("Native cursor capture suspended (window blur)");
  } catch (e) {
    console.error("Failed to suspend cursor capture:", e);
  }
}

/**
 * Resume cursor capture (e.g., when window regains focus)
 * This re-enables native cursor capture if we were in pointer lock mode
 */
export async function resumeCursorCapture(): Promise<void> {
  if (!nativeCursorCaptured) return;
  if (inputCaptureMode !== 'pointerlock') return;

  try {
    const captured = await invoke<boolean>("capture_cursor");
    if (captured) {
      // Restart mouse polling on Windows
      if (isWindows) {
        await startMousePolling();
      }
      console.log("Native cursor capture resumed (window focus)");
    }
  } catch (e) {
    console.error("Failed to resume cursor capture:", e);
  }
}

/**
 * Check if we're currently using native cursor capture
 */
export function isNativeCursorCaptured(): boolean {
  return nativeCursorCaptured;
}

/**
 * Get current input capture mode
 */
export function getInputCaptureMode(): 'pointerlock' | 'absolute' {
  return inputCaptureMode;
}

/**
 * Set up input capture on the video element
 */
export function setupInputCapture(videoElement: HTMLVideoElement): () => void {
  // Track if we have pointer lock
  let hasPointerLock = false;

  // Get video element bounds for absolute mouse calculations
  const getVideoBounds = () => videoElement.getBoundingClientRect();

  // Convert page coordinates to video-relative coordinates (0-65535 range for GFN)
  const toAbsoluteCoords = (pageX: number, pageY: number) => {
    const bounds = getVideoBounds();
    const relX = Math.max(0, Math.min(1, (pageX - bounds.left) / bounds.width));
    const relY = Math.max(0, Math.min(1, (pageY - bounds.top) / bounds.height));
    // GFN uses 16-bit absolute coordinates (0-65535)
    return {
      x: Math.round(relX * 65535),
      y: Math.round(relY * 65535),
    };
  };

  // Check if mouse is over video element
  const isMouseOverVideo = (e: MouseEvent) => {
    const bounds = getVideoBounds();
    return (
      e.clientX >= bounds.left &&
      e.clientX <= bounds.right &&
      e.clientY >= bounds.top &&
      e.clientY <= bounds.bottom
    );
  };

  // Check if pointerrawupdate is supported (lower latency than pointermove)
  const supportsRawUpdate = "onpointerrawupdate" in videoElement;

  // Mouse move handler - uses pointerrawupdate for lowest latency when available
  const handleMouseMove = (e: MouseEvent | PointerEvent) => {
    // In pointer lock mode, require pointer lock OR native cursor capture
    if (inputCaptureMode === 'pointerlock') {
      const canSendRelative = hasPointerLock || (nativeCursorCaptured && inputCaptureActive);

      if (canSendRelative) {
        // Windows with high-frequency polling: skip browser events, polling handles it
        if (isWindows && mousePollingActive) {
          return; // Mouse input handled by 1000Hz native polling thread
        }

        // macOS native or browser pointer lock: use movementX/movementY
        // pointerrawupdate gives us individual events, no need to coalesce
        if (e.type === "pointerrawupdate") {
          if (e.movementX !== 0 || e.movementY !== 0) {
            sendInputEvent({
              type: "mouse_move",
              data: { dx: e.movementX, dy: e.movementY },
            });
          }
        } else {
          // For regular pointermove, get all coalesced events
          const events = (e as PointerEvent).getCoalescedEvents?.() || [e];
          for (const evt of events) {
            if (evt.movementX !== 0 || evt.movementY !== 0) {
              sendInputEvent({
                type: "mouse_move",
                data: { dx: evt.movementX, dy: evt.movementY },
              });
            }
          }
        }
      }
    } else {
      // In absolute mode, send if over video or input is captured
      if (inputCaptureActive || isMouseOverVideo(e)) {
        const coords = toAbsoluteCoords(e.clientX, e.clientY);
        sendInputEvent({
          type: "mouse_move",
          data: {
            dx: e.movementX,
            dy: e.movementY,
            absolute: true,
            x: coords.x,
            y: coords.y,
          },
        });
      }
    }
  };

  // Mouse button down
  const handleMouseDown = (e: MouseEvent) => {
    // On macOS, also capture when cursor is hidden (pointerlock workaround)
    const nativeCapture = nativeCursorCaptured;
    const shouldCapture = hasPointerLock || nativeCapture || (inputCaptureMode === 'absolute' && isMouseOverVideo(e));

    if (shouldCapture) {
      // Activate input capture on click
      if (!inputCaptureActive && inputCaptureMode === 'absolute') {
        inputCaptureActive = true;
        videoElement.focus();
        console.log("Input capture activated (absolute mode)");
      }

      sendInputEvent({
        type: "mouse_button",
        data: { button: e.button, pressed: true },
      });
      e.preventDefault();
    }
  };

  // Mouse button up
  const handleMouseUp = (e: MouseEvent) => {
    const nativeCapture = nativeCursorCaptured;
    const shouldCapture = hasPointerLock || nativeCapture || inputCaptureActive;

    if (shouldCapture) {
      sendInputEvent({
        type: "mouse_button",
        data: { button: e.button, pressed: false },
      });
      e.preventDefault();
    }
  };

  // Mouse wheel
  const handleWheel = (e: WheelEvent) => {
    const nativeCapture = nativeCursorCaptured;
    const shouldCapture = hasPointerLock || nativeCapture || (inputCaptureMode === 'absolute' && (inputCaptureActive || isMouseOverVideo(e)));

    if (shouldCapture) {
      sendInputEvent({
        type: "mouse_wheel",
        data: { deltaX: e.deltaX, deltaY: e.deltaY },
      });
      e.preventDefault();
    }
  };

  // Keyboard handlers
  const handleKeyDown = (e: KeyboardEvent) => {
    const nativeCapture = nativeCursorCaptured;
    const shouldCapture = hasPointerLock || nativeCapture || inputCaptureActive;

    if (shouldCapture) {
      // ESC is sent to the game like any other key - fullscreen exit is handled separately
      // Use proper GFN key code mapping and modifier flags
      const vkCode = getVirtualKeyCode(e);
      const modifiers = getModifierFlags(e);

      sendInputEvent({
        type: "key",
        data: {
          keyCode: vkCode,
          scanCode: 0, // GFN uses scanCode field for other purposes
          pressed: true,
          modifiers,
        },
      });

      e.preventDefault();
    }
  };

  const handleKeyUp = (e: KeyboardEvent) => {
    const nativeCapture = nativeCursorCaptured;
    const shouldCapture = hasPointerLock || nativeCapture || inputCaptureActive;

    if (shouldCapture) {
      // Use proper GFN key code mapping and modifier flags
      const vkCode = getVirtualKeyCode(e);
      const modifiers = getModifierFlags(e);

      sendInputEvent({
        type: "key",
        data: {
          keyCode: vkCode,
          scanCode: 0,
          pressed: false,
          modifiers,
        },
      });

      e.preventDefault();
    }
  };

  // Helper to request pointer lock with keyboard lock (Windows only)
  // Keyboard lock must be acquired BEFORE pointer lock to capture Escape key
  const requestPointerLockWithKeyboardLock = async () => {
    // On Windows, lock the Escape key first to prevent Chrome from exiting pointer lock
    if (!isMacOS && navigator.keyboard?.lock) {
      try {
        await navigator.keyboard.lock(["Escape"]);
        console.log("Keyboard lock enabled (Escape key captured)");
      } catch (e) {
        console.warn("Keyboard lock failed:", e);
      }
    }

    // Now request pointer lock
    try {
      // Use unadjustedMovement for raw mouse input without OS acceleration
      await (videoElement as any).requestPointerLock({ unadjustedMovement: true });
    } catch {
      // Fallback if unadjustedMovement not supported
      videoElement.requestPointerLock();
    }
  };

  // Click handler - either get pointer lock or activate absolute capture
  const handleClick = (e: MouseEvent) => {
    if (inputCaptureMode === 'pointerlock') {
      // On macOS/Windows, we use native cursor capture, not browser pointer lock
      // Only request browser pointer lock on other platforms
      if (!hasPointerLock && !nativeCursorCaptured && !(isMacOS || isWindows)) {
        requestPointerLockWithKeyboardLock();
      }
    } else {
      // Absolute mode - just activate capture
      if (!inputCaptureActive) {
        inputCaptureActive = true;
        videoElement.focus();
        console.log("Input capture activated (click)");
      }
    }
  };

  // Context menu handler - prevent default when captured
  const handleContextMenu = (e: MouseEvent) => {
    if (hasPointerLock || inputCaptureActive) {
      e.preventDefault();
    }
  };

  // Pointer lock change
  const handlePointerLockChange = () => {
    hasPointerLock = document.pointerLockElement === videoElement;
    console.log("Pointer lock:", hasPointerLock);
    if (hasPointerLock) {
      inputCaptureActive = true;
    } else {
      // Release keyboard lock when pointer lock is released
      if (!isMacOS && navigator.keyboard?.unlock) {
        navigator.keyboard.unlock();
        console.log("Keyboard lock released");
      }
    }

    // Hide/show main app UI based on pointer lock state
    const appHeader = document.getElementById("app-header");
    const statusBar = document.getElementById("status-bar");
    const streamHeader = document.querySelector(".stream-header") as HTMLElement;

    if (hasPointerLock) {
      // Hide main app UI when mouse is locked
      if (appHeader) appHeader.style.display = "none";
      if (statusBar) statusBar.style.display = "none";
      if (streamHeader) streamHeader.style.display = "none";
    } else {
      // Show main app UI when mouse is unlocked
      if (appHeader) appHeader.style.display = "";
      if (statusBar) statusBar.style.display = "";
      if (streamHeader) streamHeader.style.display = "";
    }
  };

  // Pointer lock error
  const handlePointerLockError = () => {
    // On macOS/Windows, we use native cursor capture, so ignore browser pointer lock errors
    if (isMacOS || isWindows) return;

    console.error("Pointer lock error - falling back to absolute mode");
    hasPointerLock = false;
    // Fall back to absolute mode if pointer lock fails
    if (inputCaptureMode === 'pointerlock') {
      inputCaptureMode = 'absolute';
      inputCaptureActive = true;
    }
  };

  // Blur handler - deactivate capture when window loses focus
  const handleBlur = () => {
    if (inputCaptureActive && !hasPointerLock) {
      // Keep capture active but note the window lost focus
      console.log("Window blurred, input capture paused");
    }
  };

  // Fullscreen change handler - auto switch between pointer lock and absolute mode
  const handleFullscreenChange = () => {
    const isFullscreen = !!(
      document.fullscreenElement ||
      (document as any).webkitFullscreenElement ||
      (document as any).mozFullScreenElement ||
      (document as any).msFullscreenElement
    );

    console.log("Fullscreen changed:", isFullscreen);

    if (isFullscreen) {
      // Entering fullscreen - switch to pointer lock mode
      inputCaptureMode = 'pointerlock';
      inputCaptureActive = true;

      // Request pointer lock after a small delay (fullscreen transition needs to complete)
      // On macOS/Windows, we use native cursor capture instead of browser pointer lock
      if (!(isMacOS || isWindows)) {
        setTimeout(() => {
          if (!hasPointerLock) {
            console.log("Requesting pointer lock for fullscreen");
            requestPointerLockWithKeyboardLock();
          }
        }, 100);
      }
    } else {
      // Exiting fullscreen - switch to absolute mode
      if (hasPointerLock) {
        document.exitPointerLock();
      }
      inputCaptureMode = 'absolute';
      inputCaptureActive = true;
      console.log("Switched to absolute mode (windowed)");
    }
  };

  // Make video element focusable
  videoElement.tabIndex = 0;

  // Add event listeners
  videoElement.addEventListener("click", handleClick);
  videoElement.addEventListener("contextmenu", handleContextMenu);
  document.addEventListener("pointerlockchange", handlePointerLockChange);
  document.addEventListener("pointerlockerror", handlePointerLockError);
  document.addEventListener("fullscreenchange", handleFullscreenChange);
  document.addEventListener("webkitfullscreenchange", handleFullscreenChange);
  document.addEventListener("mozfullscreenchange", handleFullscreenChange);
  document.addEventListener("MSFullscreenChange", handleFullscreenChange);
  // Use pointerrawupdate for lowest latency when available, fallback to pointermove
  if (supportsRawUpdate) {
    document.addEventListener("pointerrawupdate", handleMouseMove as EventListener);
    console.log("Using pointerrawupdate for low-latency mouse input");
  } else {
    document.addEventListener("pointermove", handleMouseMove as EventListener);
    console.log("Using pointermove for mouse input (pointerrawupdate not supported)");
  }
  document.addEventListener("mousedown", handleMouseDown);
  document.addEventListener("mouseup", handleMouseUp);
  document.addEventListener("wheel", handleWheel, { passive: false });
  document.addEventListener("keydown", handleKeyDown, { passive: false });
  document.addEventListener("keyup", handleKeyUp, { passive: false });
  window.addEventListener("blur", handleBlur);

  // Start in absolute mode - immediately active
  console.log("Input capture set up in", inputCaptureMode, "mode");
  console.log("Double-click video to enter fullscreen with pointer lock");
  if (inputCaptureMode === 'absolute') {
    // Auto-activate after a short delay to allow video to render
    setTimeout(() => {
      inputCaptureActive = true;
      videoElement.focus();
      console.log("Input capture auto-activated");
    }, 500);
  }

  // Return cleanup function
  return () => {
    inputCaptureActive = false;
    videoElement.removeEventListener("click", handleClick);
    videoElement.removeEventListener("contextmenu", handleContextMenu);
    document.removeEventListener("pointerlockchange", handlePointerLockChange);
    document.removeEventListener("pointerlockerror", handlePointerLockError);
    document.removeEventListener("fullscreenchange", handleFullscreenChange);
    document.removeEventListener("webkitfullscreenchange", handleFullscreenChange);
    document.removeEventListener("mozfullscreenchange", handleFullscreenChange);
    document.removeEventListener("MSFullscreenChange", handleFullscreenChange);
    if (supportsRawUpdate) {
      document.removeEventListener("pointerrawupdate", handleMouseMove as EventListener);
    } else {
      document.removeEventListener("pointermove", handleMouseMove as EventListener);
    }
    document.removeEventListener("mousedown", handleMouseDown);
    document.removeEventListener("mouseup", handleMouseUp);
    document.removeEventListener("wheel", handleWheel);
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("keyup", handleKeyUp);
    window.removeEventListener("blur", handleBlur);

    if (document.pointerLockElement === videoElement) {
      document.exitPointerLock();
    }

    // Exit fullscreen if active (cross-browser)
    const fullscreenElement = document.fullscreenElement ||
      (document as any).webkitFullscreenElement ||
      (document as any).mozFullScreenElement ||
      (document as any).msFullscreenElement;

    if (fullscreenElement) {
      if (document.exitFullscreen) {
        document.exitFullscreen().catch(() => {});
      } else if ((document as any).webkitExitFullscreen) {
        (document as any).webkitExitFullscreen();
      } else if ((document as any).mozCancelFullScreen) {
        (document as any).mozCancelFullScreen();
      } else if ((document as any).msExitFullscreen) {
        (document as any).msExitFullscreen();
      }
    }
  };
}

/**
 * Get streaming statistics
 */
export async function getStreamingStats(): Promise<StreamingStats | null> {
  if (!streamingState.peerConnection) {
    return null;
  }

  const stats = await streamingState.peerConnection.getStats();
  let fps = 0;
  let latency = 0;
  let bitrate = 0;
  let packetLoss = 0;
  let resolution = "";
  let codec = "";

  stats.forEach((report) => {
    if (report.type === "inbound-rtp" && report.kind === "video") {
      fps = report.framesPerSecond || 0;
      resolution = `${report.frameWidth || 0}x${report.frameHeight || 0}`;

      if (report.packetsLost !== undefined && report.packetsReceived) {
        packetLoss = report.packetsLost / (report.packetsReceived + report.packetsLost);
      }
    }

    if (report.type === "candidate-pair" && report.state === "succeeded") {
      latency = report.currentRoundTripTime ? report.currentRoundTripTime * 1000 : 0;
    }

    if (report.type === "codec" && report.mimeType?.includes("video")) {
      codec = report.mimeType.replace("video/", "");
      // Normalize HEVC to H265 for display consistency
      if (codec.toUpperCase() === "HEVC") {
        codec = "H265";
      }
    }
  });

  // Calculate real-time bitrate from bytes received over time
  const videoStats = Array.from(stats.values()).find(
    (s) => s.type === "inbound-rtp" && s.kind === "video"
  );
  if (videoStats && videoStats.bytesReceived !== undefined) {
    const now = Date.now();
    const currentBytes = videoStats.bytesReceived;

    if (lastBytesTimestamp > 0 && lastBytesReceived > 0) {
      const timeDelta = (now - lastBytesTimestamp) / 1000; // seconds
      const bytesDelta = currentBytes - lastBytesReceived;

      if (timeDelta > 0 && bytesDelta >= 0) {
        // Calculate kbps: (bytes * 8 bits/byte) / 1000 / seconds
        bitrate = Math.round((bytesDelta * 8) / 1000 / timeDelta);
      }
    }

    // Update tracking for next calculation
    lastBytesReceived = currentBytes;
    lastBytesTimestamp = now;
  }

  // Get input latency stats
  const inputStats = getInputLatencyStats();

  const currentStats: StreamingStats = {
    fps,
    latency_ms: Math.round(latency),
    bitrate_kbps: bitrate,
    packet_loss: packetLoss,
    resolution,
    codec,
    input_ipc_ms: inputStats.ipc,
    input_send_ms: inputStats.send,
    input_total_ms: inputStats.total,
    input_rate: inputStats.rate,
  };

  streamingState.stats = currentStats;
  return currentStats;
}

/**
 * Stop streaming and clean up resources
 */
export function stopStreaming(): void {
  console.log("Stopping streaming");

  // Clear heartbeat interval
  if (heartbeatInterval) {
    clearInterval(heartbeatInterval);
    heartbeatInterval = null;
  }

  // Close WebSocket
  if (streamingState.signalingSocket) {
    // Send bye message before closing
    if (streamingState.signalingSocket.readyState === WebSocket.OPEN) {
      sendSignalingMessage(streamingState.signalingSocket, { type: "bye" });
    }
    streamingState.signalingSocket.close(1000, "User requested stop");
    streamingState.signalingSocket = null;
  }

  // Close data channels
  streamingState.dataChannels.forEach((channel) => {
    channel.close();
  });
  streamingState.dataChannels.clear();

  // Close peer connection
  if (streamingState.peerConnection) {
    streamingState.peerConnection.close();
    streamingState.peerConnection = null;
  }

  // Close audio context
  if (streamingState.audioContext) {
    streamingState.audioContext.close();
    streamingState.audioContext = null;
  }

  // Remove video element
  if (streamingState.videoElement) {
    streamingState.videoElement.srcObject = null;
    streamingState.videoElement.remove();
    streamingState.videoElement = null;
  }

  // Reset state
  streamingState.connected = false;
  streamingState.sessionId = null;
  streamingState.stats = null;
  streamingState.retryCount = 0;
  streamingState.inputDebugLogged = undefined;
  signalingSeq = 0;
  gfnAckId = 0;
  sharedMediaStream = null;
  isReconnect = false; // Reset for fresh session
  inputHandshakeComplete = false;
  inputHandshakeAttempts = 0;
  inputProtocolVersion = 0;
  streamStartTime = 0;
  inputEventCount = 0;
  lastInputLogTime = 0;
  inputCaptureActive = false;

  // Reset bitrate tracking
  lastBytesReceived = 0;
  lastBytesTimestamp = 0;
}

/**
 * Check if streaming is active
 */
export function isStreamingActive(): boolean {
  return streamingState.connected && streamingState.peerConnection !== null;
}

/**
 * Get current streaming state
 */
export function getStreamingState(): StreamingState {
  return { ...streamingState };
}

/**
 * Set video quality during stream
 */
export function setStreamingQuality(quality: {
  maxBitrate?: number;
  maxFramerate?: number;
}): void {
  if (!streamingState.peerConnection) {
    return;
  }

  const senders = streamingState.peerConnection.getSenders();
  senders.forEach((sender) => {
    if (sender.track?.kind === "video") {
      const params = sender.getParameters();
      if (params.encodings && params.encodings.length > 0) {
        if (quality.maxBitrate) {
          params.encodings[0].maxBitrate = quality.maxBitrate * 1000;
        }
        if (quality.maxFramerate) {
          params.encodings[0].maxFramerate = quality.maxFramerate;
        }
        sender.setParameters(params);
      }
    }
  });
}

/**
 * Toggle fullscreen mode
 */
export function toggleFullscreen(): void {
  if (!streamingState.videoElement) {
    return;
  }

  // Cross-browser fullscreen check
  const fullscreenElement = document.fullscreenElement ||
    (document as any).webkitFullscreenElement ||
    (document as any).mozFullScreenElement ||
    (document as any).msFullscreenElement;

  const element = streamingState.videoElement;

  if (fullscreenElement) {
    // Exit fullscreen - cross-browser
    if (document.exitFullscreen) {
      document.exitFullscreen();
    } else if ((document as any).webkitExitFullscreen) {
      (document as any).webkitExitFullscreen();
    } else if ((document as any).mozCancelFullScreen) {
      (document as any).mozCancelFullScreen();
    } else if ((document as any).msExitFullscreen) {
      (document as any).msExitFullscreen();
    }
  } else {
    // Enter fullscreen - cross-browser (with Safari/WebKit support for macOS)
    if (element.requestFullscreen) {
      element.requestFullscreen();
    } else if ((element as any).webkitRequestFullscreen) {
      (element as any).webkitRequestFullscreen();
    } else if ((element as any).mozRequestFullScreen) {
      (element as any).mozRequestFullScreen();
    } else if ((element as any).msRequestFullscreen) {
      (element as any).msRequestFullscreen();
    }
  }
}

/**
 * Set audio volume (0-1)
 */
export function setVolume(volume: number): void {
  if (streamingState.videoElement) {
    streamingState.videoElement.volume = Math.max(0, Math.min(1, volume));
  }
}

/**
 * Toggle audio mute
 */
export function toggleMute(): boolean {
  if (streamingState.videoElement) {
    streamingState.videoElement.muted = !streamingState.videoElement.muted;
    return streamingState.videoElement.muted;
  }
  return false;
}
