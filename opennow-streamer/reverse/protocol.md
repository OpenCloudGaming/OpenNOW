# GeForce NOW WebRTC & RTP Protocol - Reverse Engineering Documentation

## 1. Authentication Flow

### OAuth 2.0 with PKCE
```
Endpoint: https://login.nvidia.com/authorize
Token:    https://login.nvidia.com/token
Client ID: ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ
Scopes:    openid consent email tk_client age
```

### Request Parameters
```json
{
  "response_type": "code",
  "device_id": "sha256(hostname + username + 'opennow-streamer')",
  "scope": "openid consent email tk_client age",
  "client_id": "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ",
  "redirect_uri": "http://localhost:{port}",
  "code_challenge": "sha256_base64(verifier)",
  "code_challenge_method": "S256",
  "idp_id": "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg"
}
```

### Token Response
```json
{
  "access_token": "...",
  "refresh_token": "...",
  "id_token": "...",
  "expires_in": 86400
}
```

### Authorization Header
```
Authorization: GFNJWT {token}
```

---

## 2. CloudMatch Session API

### Base URL
```
https://{zone}.cloudmatchbeta.nvidiagrid.net/v2/session
```

### Required Headers
```
Authorization: GFNJWT {token}
Content-Type: application/json
nv-client-id: {uuid}
nv-client-type: NATIVE
nv-client-version: 2.0.80.173
nv-client-streamer: NVIDIA-CLASSIC
nv-device-os: WINDOWS
nv-device-type: DESKTOP
x-device-id: {device-id}
Origin: https://play.geforcenow.com
```

### Create Session (POST /v2/session)
```json
{
  "sessionRequestData": {
    "appId": "string",
    "internalTitle": "Game Title",
    "clientIdentification": "GFN-PC",
    "deviceHashId": "{uuid}",
    "clientVersion": "30.0",
    "clientPlatformName": "windows",
    "clientRequestMonitorSettings": [{
      "widthInPixels": 1920,
      "heightInPixels": 1080,
      "framesPerSecond": 60
    }],
    "metaData": [
      {"key": "GSStreamerType", "value": "WebRTC"},
      {"key": "wssignaling", "value": "1"}
    ],
    "requestedStreamingFeatures": {
      "reflex": false,
      "trueHdr": false
    }
  }
}
```

### Session Response
```json
{
  "session": {
    "sessionId": "string",
    "status": 2,
    "gpuType": "RTX_A5000",
    "connectionInfo": [{
      "ip": "server_ip",
      "port": 47998,
      "usage": 14
    }]
  },
  "requestStatus": {
    "statusCode": 1,
    "serverId": "NP-AMS-08"
  }
}
```

### Session States
- **1**: Setting up / Launching
- **2**: Ready for streaming
- **3**: Already streaming
- **6**: Initialization pending

---

## 3. WebSocket Signaling

### Connection
```
URL: wss://{server_ip}:443/nvst/sign_in?peer_id={peer_name}&version=2
Subprotocol: x-nv-sessionid.{sessionId}
```

### Peer Info Message
```json
{
  "ackid": 1,
  "peer_info": {
    "id": 2,
    "name": "peer-{random}",
    "browser": "Chrome",
    "browserVersion": "131",
    "connected": true,
    "peerRole": 0,
    "resolution": "1920x1080",
    "version": 2
  }
}
```

### Heartbeat (every 5s)
```json
{"hb": 1}
```

### Acknowledgment
```json
{"ack": 1}
```

### SDP Offer (from server)
```json
{
  "ackid": 2,
  "peer_msg": {
    "from": 1,
    "to": 2,
    "msg": "{\"type\":\"offer\",\"sdp\":\"v=0\\r\\no=...\"}"
  }
}
```

### SDP Answer (to server)
```json
{
  "ackid": 3,
  "peer_msg": {
    "from": 2,
    "to": 1,
    "msg": "{\"type\":\"answer\",\"sdp\":\"...\",\"nvstSdp\":\"v=0\\r\\n...\"}"
  }
}
```

### ICE Candidate
```json
{
  "ackid": 4,
  "peer_msg": {
    "from": 2,
    "to": 1,
    "msg": "{\"candidate\":\"candidate:...\",\"sdpMid\":\"...\",\"sdpMLineIndex\":0}"
  }
}
```

---

## 4. SDP (Session Description Protocol)

### ICE-Lite Detection
```
a=ice-lite
```
When server is ice-lite:
- Client MUST respond with `a=setup:active`
- Client initiates DTLS ClientHello

### nvstSdp Attributes

**FEC Settings:**
```
a=vqos.fec.rateDropWindow:10
a=vqos.fec.minRequiredFecPackets:2
a=vqos.fec.repairMinPercent:5
a=vqos.fec.repairMaxPercent:35
```

**Dynamic Quality Control:**
```
a=vqos.dfc.enable:1
a=vqos.dfc.decodeFpsAdjPercent:85
a=vqos.dfc.targetDownCooldownMs:250
a=vqos.dfc.minTargetFps:100
```

**Bitrate Control:**
```
a=video.initialBitrateKbps:25000
a=vqos.bw.maximumBitrateKbps:50000
a=vqos.bw.minimumBitrateKbps:5000
a=bwe.useOwdCongestionControl:1
```

**NACK Settings:**
```
a=video.enableRtpNack:1
a=video.rtpNackQueueLength:1024
a=video.rtpNackQueueMaxPackets:512
```

---

## 5. RTP Protocol

### Video Payload Types
- **96**: H.264
- **127**: H.265/HEVC
- **98**: AV1

### Audio Payload Types
- **111**: Opus (stereo)
- **100**: Multiopus (up to 8 channels)

### Clock Rates
- Video: 90000 Hz
- Audio: 48000 Hz

---

## 6. RTCP Feedback

### PLI (Picture Loss Indication)
```rust
let pli = PictureLossIndication {
    sender_ssrc: 0,
    media_ssrc: VIDEO_SSRC,
};
peer_connection.write_rtcp(&[Box::new(pli)]).await?
```

### NACK (Negative Acknowledgment)
```
Bitmask of missing sequence numbers
Requests retransmission of specific packets
```

---

## 7. Data Channels

### Channel Names
| Channel | Ordered | Reliable | Purpose |
|---------|---------|----------|---------|
| input_channel_v1 | Yes | Yes | Keyboard, handshake |
| input_channel_partially_reliable | No | No (8ms) | Mouse movement |
| cursor_channel | Yes | Yes | Cursor updates |
| control_channel | Yes | Yes | Control messages |

### Handshake Protocol
```
Server → Client: [0x0e, major, minor, flags]
Client → Server: Echo same bytes
```

---

## 8. DTLS/TLS Security

### Handshake States
1. ice-gathering
2. ice-connected
3. dtls-connecting
4. dtls-connected
5. peer-connected

### Certificate Handling
- Self-signed certificates accepted
- `danger_accept_invalid_certs(true)`

---

## 9. Comparison

| Feature | Web Client | OpenNow | Official Client |
|---------|-----------|---------|-----------------|
| WebRTC | Browser | webrtc-rs | libwebrtc |
| Signaling | JavaScript | Tokio WS | Native C++ |
| ICE | Browser | Manual | libwebrtc |
| DTLS | Browser | webrtc-rs | Native |
| Data Channels | Browser | webrtc-rs | Native |
