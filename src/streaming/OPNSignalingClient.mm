#include "OPNSignalingClient.h"

#import <Foundation/Foundation.h>

// ---------------------------------------------------------------------------
// Private delegate that fires on WebSocket open/close/error, mirroring the
// raw WebSocket "open" / "close" / "error" events in OpenNow's signaling.ts.
// ---------------------------------------------------------------------------
@interface _OPNWebSocketDelegate : NSObject <NSURLSessionDelegate, NSURLSessionWebSocketDelegate>
@property (nonatomic, copy) void (^onOpen)(NSString *protocol);
@property (nonatomic, copy) void (^onError)(NSError *error);
@property (nonatomic, copy) void (^onClose)(NSURLSessionWebSocketCloseCode code, NSString *reason);
@end

@implementation _OPNWebSocketDelegate
- (void)URLSession:(NSURLSession *)session webSocketTask:(NSURLSessionWebSocketTask *)webSocketTask didOpenWithProtocol:(NSString *)protocol {
    if (self.onOpen) self.onOpen(protocol);
}
- (void)URLSession:(NSURLSession *)session task:(NSURLSessionTask *)task didCompleteWithError:(NSError *)error {
    if (error && self.onError) self.onError(error);
}
- (void)URLSession:(NSURLSession *)session webSocketTask:(NSURLSessionWebSocketTask *)webSocketTask didCloseWithCode:(NSURLSessionWebSocketCloseCode)closeCode reason:(NSData *)reason {
    NSString *reasonStr = reason ? [[NSString alloc] initWithData:reason encoding:NSUTF8StringEncoding] : @"";
    if (self.onClose) self.onClose(closeCode, reasonStr);
}
@end

namespace OPN {

// ---------------------------------------------------------------------------
// Helper to build the sign-in URL (mirrors buildSignInUrl in signaling.ts).
// ---------------------------------------------------------------------------
static NSURL *BuildSignInUrl(const std::string &signalingServer,
                              const std::string &sessionId,
                              const std::string &signalingUrl,
                              const std::string &peerName) {
    NSString *host = [NSString stringWithUTF8String:signalingServer.c_str()];
    NSString *sessionIdObj = [NSString stringWithUTF8String:sessionId.c_str()];

    // Determine base URL — signaling.ts uses signalingUrl if set, otherwise wss://host:443/nvst/
    NSString *baseUrlStr;
    if (!signalingUrl.empty()) {
        baseUrlStr = [NSString stringWithUTF8String:signalingUrl.c_str()];
    } else {
        baseUrlStr = [host containsString:@":"]
            ? [NSString stringWithFormat:@"wss://%@/nvst/", host]
            : [NSString stringWithFormat:@"wss://%@:443/nvst/", host];
    }

    NSURLComponents *comp = [NSURLComponents componentsWithString:baseUrlStr];
    if (!comp) {
        comp = [NSURLComponents new];
        comp.scheme = @"wss";
        comp.host = host;
        comp.path = @"/nvst/";
    }

    comp.scheme = @"wss";

    // Ensure path ends with "sign_in" (matching signaling.ts logic)
    NSString *path = comp.path ?: @"/nvst/";
    if (![path hasSuffix:@"/"]) path = [path stringByAppendingString:@"/"];
    path = [path stringByAppendingString:@"sign_in"];
    comp.path = path;

    // Query params (matching signaling.ts)
    NSMutableArray *items = [NSMutableArray arrayWithArray:comp.queryItems ?: @[]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"peer_id" value:[NSString stringWithUTF8String:peerName.c_str()]]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"version" value:@"2"]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"peer_role" value:@"1"]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"pairing_id" value:sessionIdObj]];
    comp.queryItems = items;

    return comp.URL;
}

// ---------------------------------------------------------------------------
// SignalingClient implementation
// ---------------------------------------------------------------------------

SignalingClient::SignalingClient(const std::string &signalingServer,
                                  const std::string &sessionId,
                                  const std::string &signalingUrl)
    : m_signalingServer(signalingServer)
    , m_sessionId(sessionId)
    , m_signalingUrl(signalingUrl) {
}

SignalingClient::~SignalingClient() {
    Disconnect();
}

void SignalingClient::SetPeerResolution(const std::string &resolution) {
    if (!resolution.empty()) {
        m_peerResolution = resolution;
    }
}

bool SignalingClient::IsCurrentGeneration(int generation) const {
    return generation == m_connectionGeneration;
}

// ------ Connect (mirrors signaling.ts connect) ------
void SignalingClient::Connect(SignalingConnectCallback onConnect) {
    if (m_webSocketTask) {
        onConnect(true, "");
        return;
    }

    m_peerName = "peer-" + std::to_string(arc4random_uniform(1000000000));
    m_didOpen = false;
    NSURL *url = BuildSignInUrl(m_signalingServer, m_sessionId, m_signalingUrl, m_peerName);
    if (!url) {
        onConnect(false, "Failed to build signaling URL");
        return;
    }

    NSString *protocol = [NSString stringWithFormat:@"x-nv-sessionid.%s", m_sessionId.c_str()];

    int generation = ++m_connectionGeneration;
    NSURLSession *session = nil;
    _OPNWebSocketDelegate *delegate = nil;

    // --- completion handler for open (mirrors ws.once("open")) ---
    void (^onOpen)(NSString *) = ^(NSString *proto) {
        (void)proto;
        if (!IsCurrentGeneration(generation)) return;

        m_didOpen = true;  // <-- mark connected so onError skips the callback

        // Connection is open — now send peer_info and set up heartbeat
        // (mirrors signaling.ts: this.sendPeerInfo(); this.setupHeartbeat();)
        SendPeerInfo();
        SetupHeartbeat();

        // Signal connected (mirrors resolve() + emit("connected"))
        onConnect(true, "");
    };

    // --- completion handler for error (mirrors ws.once("error")) ---
    void (^onError)(NSError *) = ^(NSError *error) {
        if (!IsCurrentGeneration(generation)) return;
        // In OpenNow, ws.once("error") uses "once" — it only fires *instead* of
        // "open", never after.  If we already opened, this is a post-connection
        // transport error (e.g. socket reset), not a connection failure.
        if (m_didOpen) {
            NSLog(@"[Signaling] Post-connection error: %@", error);
            return;
        }
        std::string msg = [[NSString stringWithFormat:@"Signaling connect failed: %@", error.localizedDescription] UTF8String];
        onConnect(false, msg);
    };

    // --- completion handler for close (mirrors ws.on("close")) ---
    void (^onClose)(NSURLSessionWebSocketCloseCode, NSString *) = ^(NSURLSessionWebSocketCloseCode code, NSString *reason) {
        if (!IsCurrentGeneration(generation)) return;
        NSLog(@"[Signaling] WebSocket closed: code=%ld, reason=%@", (long)code, reason);
        ClearHeartbeat();
        m_webSocketTask = nullptr;
    };

    // Delegate object receives URLSession callbacks and forwards to blocks
    delegate = [[_OPNWebSocketDelegate alloc] init];
    delegate.onOpen = onOpen;
    delegate.onError = onError;
    delegate.onClose = onClose;

    NSOperationQueue *delegateQueue = [[NSOperationQueue alloc] init];
    delegateQueue.maxConcurrentOperationCount = 1;

    session = [NSURLSession sessionWithConfiguration:[NSURLSessionConfiguration defaultSessionConfiguration]
                                            delegate:delegate
                                       delegateQueue:delegateQueue];

    NSMutableURLRequest *req = [NSMutableURLRequest requestWithURL:url];
    [req setValue:protocol forHTTPHeaderField:@"Sec-WebSocket-Protocol"];
    [req setValue:@"https://play.geforcenow.com" forHTTPHeaderField:@"Origin"];
    [req setValue:@"Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36"
        forHTTPHeaderField:@"User-Agent"];

    NSURLSessionWebSocketTask *task = [session webSocketTaskWithRequest:req];
    m_webSocketTask = (__bridge_retained void *)task;
    m_urlSession = (__bridge_retained void *)session;
    m_delegate = (__bridge_retained void *)delegate;

    [task resume];

    // Timeout (mirrors signaling.ts's implicit WS timeout — 15s is reasonable)
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(15 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (!IsCurrentGeneration(generation)) return;
        if (m_webSocketTask) {
            NSURLSessionWebSocketTask *t = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;
            if (t.state != NSURLSessionTaskStateRunning) {
                [t cancelWithCloseCode:NSURLSessionWebSocketCloseCodeNormalClosure reason:nil];
                onConnect(false, "Signaling connection timeout");
            }
        }
    });
}

// ------ Disconnect (mirrors signaling.ts disconnect) ------
void SignalingClient::Disconnect() {
    m_connectionGeneration += 1;
    ClearHeartbeat();

    if (m_webSocketTask) {
        NSURLSessionWebSocketTask *task = (__bridge_transfer NSURLSessionWebSocketTask *)m_webSocketTask;
        [task cancelWithCloseCode:NSURLSessionWebSocketCloseCodeNormalClosure reason:nil];
        m_webSocketTask = nullptr;
    }
    if (m_urlSession) {
        NSURLSession *session = (__bridge_transfer NSURLSession *)m_urlSession;
        [session invalidateAndCancel];
        m_urlSession = nullptr;
    }
    if (m_delegate) {
        _OPNWebSocketDelegate *d = (__bridge_transfer _OPNWebSocketDelegate *)m_delegate;
        d.onOpen = nil;
        d.onError = nil;
        d.onClose = nil;
        m_delegate = nullptr;
    }
}

// ------ Heartbeat (mirrors signaling.ts setupHeartbeat / clearHeartbeat) ------
void SignalingClient::SetupHeartbeat() {
    ClearHeartbeat();

    // Start receive handler re-arm loop (mirrors message listener in signaling.ts)
    RearmReceiveHandler();

    // Proactive heartbeat timer — send {"hb":1} every 5s (mirrors setInterval in signaling.ts)
    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    if (timer) {
        dispatch_source_set_timer(timer, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC),
                                  5 * NSEC_PER_SEC, 0);
        __block SignalingClient *blockSelf = this;
        int generation = m_connectionGeneration;
        dispatch_source_set_event_handler(timer, ^{
            if (!blockSelf->IsCurrentGeneration(generation)) {
                dispatch_source_cancel(timer);
                return;
            }
            blockSelf->SendJson("{\"hb\":1}");
        });
        dispatch_resume(timer);
        m_heartbeatSource = (__bridge_retained void *)timer;
    }
}

void SignalingClient::ClearHeartbeat() {
    if (m_heartbeatSource) {
        dispatch_source_t timer = (__bridge_transfer dispatch_source_t)m_heartbeatSource;
        dispatch_source_cancel(timer);
        m_heartbeatSource = nullptr;
    }
}

// ------ Rearm: receive next message (mirrors the rearm pattern in signaling.ts) ------
void SignalingClient::RearmReceiveHandler() {
    if (!m_webSocketTask) return;
    NSURLSessionWebSocketTask *task = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;

    int generation = m_connectionGeneration;
    __block SignalingClient *blockSelf = this;

    [task receiveMessageWithCompletionHandler:^(NSURLSessionWebSocketMessage *msg, NSError *err) {
        if (!blockSelf->IsCurrentGeneration(generation)) return;

        if (err) {
            NSLog(@"[Signaling] Receive error: %@", err);
            return;
        }

        NSString *text = msg.string;
        if (text) {
            blockSelf->HandleMessage([text UTF8String]);
        }

        // Re-arm (mirrors signaling.ts: message handler stays registered)
        blockSelf->RearmReceiveHandler();
    }];
}

// ------ Send a JSON string over the WebSocket (mirrors signaling.ts sendJson) ------
void SignalingClient::SendJson(const std::string &json) {
    if (!m_webSocketTask) return;
    NSURLSessionWebSocketTask *task = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;
    [task sendMessage:[[NSURLSessionWebSocketMessage alloc] initWithString:[NSString stringWithUTF8String:json.c_str()]]
    completionHandler:^(NSError *){}];
}

// ------ Send peer_info (mirrors signaling.ts sendPeerInfo) ------
void SignalingClient::SendPeerInfo() {
    NSDictionary *info = @{
        @"ackid": @(++m_ackCounter),
        @"peer_info": @{
            @"browser": @"Chrome",
            @"browserVersion": @"131",
            @"connected": @YES,
            @"id": @(m_peerId),
            @"name": [NSString stringWithUTF8String:m_peerName.c_str()],
            @"peerRole": @0,
            @"resolution": [NSString stringWithUTF8String:m_peerResolution.c_str()],
            @"version": @2,
        },
    };
    NSData *jsonData = [NSJSONSerialization dataWithJSONObject:info options:0 error:nil];
    if (!jsonData) return;
    SendJson([[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding].UTF8String);
}

// ------ Handle incoming message (mirrors signaling.ts handleMessage) ------
void SignalingClient::HandleMessage(const std::string &text) {
    NSData *data = [[NSString stringWithUTF8String:text.c_str()] dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    if (![json isKindOfClass:[NSDictionary class]]) return;

    // --- peer_info (mirrors signaling.ts peer_info handling) ---
    NSDictionary *peerInfo = json[@"peer_info"];
    if ([peerInfo isKindOfClass:[NSDictionary class]]) {
        NSNumber *pid = peerInfo[@"id"];
        NSString *name = peerInfo[@"name"];
        if (pid && [name isKindOfClass:[NSString class]] && [name isEqualToString:[NSString stringWithUTF8String:m_peerName.c_str()]]) {
            m_peerId = pid.intValue;
            NSLog(@"[Signaling] Local peer id assigned: %d", m_peerId);
        }
    }

    // --- ack (mirrors signaling.ts ack handling) ---
    // OpenNow sends an ack for any ackid, unless the ackid is from our own peer_info
    if (json[@"ackid"]) {
        NSNumber *ourPid = peerInfo[@"id"];
        BOOL shouldAck = !ourPid || ourPid.intValue != m_peerId;
        if (shouldAck) {
            SendJson([NSString stringWithFormat:@"{\"ack\":%d}", [json[@"ackid"] intValue]].UTF8String);
        }
    }

    // --- ack response (just ignore) ---
    if (json[@"ack"]) return;

    // --- heartbeat (mirrors signaling.ts hb handling) ---
    if (json[@"hb"]) {
        SendJson("{\"hb\":1}");
        return;
    }

    // --- peer_msg (mirrors signaling.ts peer_msg handling) ---
    NSDictionary *peerMsg = json[@"peer_msg"];
    if (![peerMsg isKindOfClass:[NSDictionary class]]) return;

    NSString *msgStr = peerMsg[@"msg"];
    if (![msgStr isKindOfClass:[NSString class]]) return;

    // Set remote peer id from the from field (mirrors signaling.ts)
    NSNumber *fromId = peerMsg[@"from"];
    if (fromId) {
        m_remotePeerId = fromId.intValue;
        NSLog(@"[Signaling] Remote peer id: %d", m_remotePeerId);
    }

    // Parse inner payload
    NSData *msgData = [msgStr dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:msgData options:0 error:nil];
    if (![payload isKindOfClass:[NSDictionary class]]) return;

    // --- offer (mirrors signaling.ts offer handling) ---
    NSString *type = payload[@"type"];
    if ([type isEqualToString:@"offer"]) {
        NSString *sdp = payload[@"sdp"];
        NSLog(@"[Signaling] Offer received, sdp length=%lu, m_onOffer=%p",
              (unsigned long)sdp.length, (void*)&m_onOffer);
        if (sdp && m_onOffer) {
            m_onOffer([sdp UTF8String]);
        }
        return;
    }

    // --- ICE candidate (mirrors signaling.ts remote-ice handling) ---
    NSString *candidate = payload[@"candidate"];
    if (candidate) {
        IceCandidatePayload ice;
        ice.candidate = [candidate UTF8String];
        NSString *mid = payload[@"sdpMid"];
        if ([mid isKindOfClass:[NSString class]]) ice.sdpMid = [mid UTF8String];
        NSNumber *mli = payload[@"sdpMLineIndex"];
        if (mli) ice.sdpMLineIndex = mli.intValue;
        NSString *usernameFragment = payload[@"usernameFragment"];
        if (![usernameFragment isKindOfClass:[NSString class]]) usernameFragment = payload[@"ufrag"];
        if ([usernameFragment isKindOfClass:[NSString class]]) ice.usernameFragment = [usernameFragment UTF8String];
        if (m_onIceCandidate) {
            m_onIceCandidate(ice);
        }
        return;
    }
}

// ------ OnOffer / OnIceCandidate (mirrors signaling.ts onEvent registration) ------
void SignalingClient::OnOffer(SignalingOfferCallback cb) {
    m_onOffer = cb;
}

void SignalingClient::OnIceCandidate(SignalingIceCallback cb) {
    m_onIceCandidate = cb;
}

// ------ SendAnswer (mirrors signaling.ts sendAnswer) ------
void SignalingClient::SendAnswer(const SendAnswerRequest &answer) {
    if (!m_webSocketTask) return;
    NSLog(@"[Signaling] Sending answer SDP length=%zu nvstSdp length=%zu", answer.sdp.size(), answer.nvstSdp.size());

    NSMutableDictionary *answerDict = [NSMutableDictionary dictionary];
    answerDict[@"type"] = @"answer";
    answerDict[@"sdp"] = [NSString stringWithUTF8String:answer.sdp.c_str()];
    if (!answer.nvstSdp.empty()) {
        answerDict[@"nvstSdp"] = [NSString stringWithUTF8String:answer.nvstSdp.c_str()];
    }

    NSDictionary *peerMsg = @{
        @"peer_msg": @{
            @"from": @(m_peerId),
            @"to": @(m_remotePeerId),
            @"msg": [[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:answerDict options:0 error:nil] encoding:NSUTF8StringEncoding] ?: @"{}",
        },
        @"ackid": @(++m_ackCounter),
    };

    NSData *jsonData = [NSJSONSerialization dataWithJSONObject:peerMsg options:0 error:nil];
    if (!jsonData) return;
    SendJson([[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding].UTF8String);
}

// ------ SendIceCandidate (mirrors signaling.ts sendIceCandidate) ------
void SignalingClient::SendIceCandidate(const IceCandidatePayload &candidate) {
    if (!m_webSocketTask) return;

    NSString *mid = candidate.sdpMid.empty() ? nil : [NSString stringWithUTF8String:candidate.sdpMid.c_str()];

    NSMutableDictionary *candidateDict = [NSMutableDictionary dictionary];
    candidateDict[@"candidate"] = [NSString stringWithUTF8String:candidate.candidate.c_str()];
    candidateDict[@"sdpMid"] = mid ?: [NSNull null];
    candidateDict[@"sdpMLineIndex"] = @(candidate.sdpMLineIndex);
    if (!candidate.usernameFragment.empty()) {
        candidateDict[@"usernameFragment"] = [NSString stringWithUTF8String:candidate.usernameFragment.c_str()];
    }

    NSString *msgStr = [[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:candidateDict options:0 error:nil] encoding:NSUTF8StringEncoding];
    if (!msgStr) return;

    NSDictionary *peerMsg = @{
        @"peer_msg": @{
            @"from": @(m_peerId),
            @"to": @(m_remotePeerId),
            @"msg": msgStr,
        },
        @"ackid": @(++m_ackCounter),
    };

    NSData *jsonData = [NSJSONSerialization dataWithJSONObject:peerMsg options:0 error:nil];
    if (!jsonData) return;
    SendJson([[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding].UTF8String);
}

// ------ IsConnected (matches signaling.ts ws state check) ------
bool SignalingClient::IsConnected() const {
    if (!m_webSocketTask) return false;
    NSURLSessionWebSocketTask *task = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;
    return task.state == NSURLSessionTaskStateRunning;
}

} // namespace OPN
