# GeForce NOW Session & API Management - Reverse Engineering Documentation

## 1. Authentication Flow

### OAuth 2.0 with PKCE
```
Endpoint:  https://login.nvidia.com/authorize
Token:     https://login.nvidia.com/token
Client ID: ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ
Scopes:    openid consent email tk_client age
IDP ID:    PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg
```

### PKCE Parameters
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
  "id_token": "jwt_token",
  "expires_in": 86400
}
```

### Authorization Headers
```
Native:  Authorization: GFNJWT {token}
Partner: Authorization: GFNPartnerJWT auth={token}
OAuth:   Authorization: Bearer {token}
```

---

## 2. Service URLs API

### Endpoint
```
GET https://pcs.geforcenow.com/v1/serviceUrls
```

### Response
Array of login providers with:
- `idp_id`: Identity provider ID
- `streaming_service_url`: Region-specific base URL

### Alliance Partners
- KDD, TWM, BPC/bro.game, etc.
- Custom streaming URLs from serviceUrls response

---

## 3. CloudMatch Session API

### Base URL
```
https://{zone}.cloudmatchbeta.nvidiagrid.net/v2/session
```

### Required Headers
```
User-Agent: Mozilla/5.0 ... NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173
Authorization: GFNJWT {token}
Content-Type: application/json
Origin: https://play.geforcenow.com
nv-client-id: {uuid}
nv-client-type: NATIVE
nv-client-version: 2.0.80.173
nv-client-streamer: NVIDIA-CLASSIC
nv-device-os: WINDOWS
nv-device-type: DESKTOP
x-device-id: {uuid}
```

---

## 4. Create Session

### POST /v2/session
```json
{
  "sessionRequestData": {
    "appId": "string",
    "internalTitle": "Game Name",
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
      "port": 443,
      "usage": 14,
      "protocol": 1,
      "resourcePath": "/nvst/"
    }],
    "iceServerConfiguration": {
      "iceServers": [
        {"urls": "turn:server:port", "username": "...", "credential": "..."}
      ]
    }
  },
  "requestStatus": {
    "statusCode": 1,
    "serverId": "NP-AMS-08"
  }
}
```

---

## 5. Session States

| Status | Description |
|--------|-------------|
| 1 | Launching / Setting up |
| 2 | Ready for streaming |
| 3 | Actively streaming |
| 6 | Initialization pending |

---

## 6. Poll Session

### GET /v2/session/{sessionId}
Same response format as create session.

---

## 7. Stop Session

### DELETE /v2/session/{sessionId}
Terminates the session.

---

## 8. Resume Session

### PUT /v2/session/{sessionId}
```json
{
  "action": 2,
  "data": "RESUME",
  "sessionRequestData": { ... }
}
```

---

## 9. Error Codes

### CloudMatch Status Codes
| Code | Description |
|------|-------------|
| 1 | Success |
| 2 | Forbidden |
| 3 | Timeout |
| 4 | Internal Error |
| 11 | Session Limit Exceeded |
| 14 | Auth Failure |
| 16 | Token Expired |
| 25 | Service Unavailable |
| 50 | Device Limit Reached |
| 51 | Zone At Capacity |
| 86 | Insufficient Playability |

### Unified Error Codes (i64)
```
15859712:    Success
3237093643:  Session Limit Exceeded
3237093648:  Token Expired
3237093657:  Service Unavailable
3237093682:  Device Session Limit
3237093715:  Max Session Limit
3237093718:  Insufficient Playability
```

---

## 10. WebSocket Signaling

### Connection URL
```
wss://{server_ip}:443/nvst/sign_in?peer_id={peer_name}&version=2

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

### Heartbeat (Every 5s)
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

## 11. nvstSdp Parameters

### FEC Settings
```
a=vqos.fec.rateDropWindow:10
a=vqos.fec.minRequiredFecPackets:2
a=vqos.fec.repairMinPercent:5
a=vqos.fec.repairMaxPercent:35
```

### Dynamic Quality Control
```
a=vqos.dfc.enable:1
a=vqos.dfc.decodeFpsAdjPercent:85
a=vqos.dfc.targetDownCooldownMs:250
a=vqos.dfc.minTargetFps:100
```

### Bitrate Control
```
a=video.initialBitrateKbps:25000
a=vqos.bw.maximumBitrateKbps:50000
a=vqos.bw.minimumBitrateKbps:5000
a=bwe.useOwdCongestionControl:1
```

### NACK Settings
```
a=video.enableRtpNack:1
a=video.rtpNackQueueLength:1024
a=video.rtpNackQueueMaxPackets:512
```

---

## 12. Game Library API

### GraphQL Endpoint
```
POST https://games.geforce.com/graphql

Persisted Query Hash: f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0
```

### Public Games List
```
GET https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json
```

### Game Images
```
https://cdn.cloudflare.steamstatic.com/steam/apps/{steam_id}/library_600x900.jpg
```

---

## 13. Subscription API

### Endpoint
```
GET https://mes.geforcenow.com/v4/subscriptions
?serviceName=gfn_pc&languageCode=en_US&vpcId={vpc_id}&userId={user_id}
```

---

## 14. Server Info

### GET /v2/serverInfo
```json
{
  "requestStatus": {
    "serverId": "NP-AMS-08"
  },
  "metaData": [
    {"key": "region_name", "value": "https://region.cloudmatchbeta.nvidiagrid.net/"}
  ]
}
```

---

## 15. Client Type Headers

### Native Client
```
nv-client-type: NATIVE
nv-client-streamer: NVIDIA-CLASSIC
```

### Browser/WebRTC
```
nv-client-type: BROWSER
nv-client-streamer: WEBRTC
```

---

## 16. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Session API | /v2/session POST | /v2/session POST | /v2/session POST |
| Auth Header | GFNJWT | GFNJWT | GFNJWT |
| WS Signaling | Custom JS | Native C++ | Tokio WebSocket |
| Session Polling | Callback-based | Polling loop | Async polling |
| Heartbeat | Every 5s | Every 5s | Every 5s |
| Alliance Partners | Full support | Full support | Full support |
