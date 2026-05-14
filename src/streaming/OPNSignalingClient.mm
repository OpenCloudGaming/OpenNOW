#include "OPNSignalingClient.h"

#import <Foundation/Foundation.h>





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




static NSURL *BuildSignInUrl(const std::string &signalingServer,
                              const std::string &sessionId,
                              const std::string &signalingUrl,
                              const std::string &peerName) {
    NSString *host = [NSString stringWithUTF8String:signalingServer.c_str()];
    NSString *sessionIdObj = [NSString stringWithUTF8String:sessionId.c_str()];


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


    NSString *path = comp.path ?: @"/nvst/";
    if (![path hasSuffix:@"/"]) path = [path stringByAppendingString:@"/"];
    path = [path stringByAppendingString:@"sign_in"];
    comp.path = path;


    NSMutableArray *items = [NSMutableArray arrayWithArray:comp.queryItems ?: @[]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"peer_id" value:[NSString stringWithUTF8String:peerName.c_str()]]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"version" value:@"2"]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"peer_role" value:@"1"]];
    [items addObject:[NSURLQueryItem queryItemWithName:@"pairing_id" value:sessionIdObj]];
    comp.queryItems = items;

    return comp.URL;
}





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


    void (^onOpen)(NSString *) = ^(NSString *proto) {
        (void)proto;
        if (!IsCurrentGeneration(generation)) return;

        m_didOpen = true;



        SendPeerInfo();
        SetupHeartbeat();


        onConnect(true, "");
    };


    void (^onError)(NSError *) = ^(NSError *error) {
        if (!IsCurrentGeneration(generation)) return;



        if (m_didOpen) {
            NSLog(@"[Signaling] Post-connection error: %@", error);
            return;
        }
        std::string msg = [[NSString stringWithFormat:@"Signaling connect failed: %@", error.localizedDescription] UTF8String];
        onConnect(false, msg);
    };


    void (^onClose)(NSURLSessionWebSocketCloseCode, NSString *) = ^(NSURLSessionWebSocketCloseCode code, NSString *reason) {
        if (!IsCurrentGeneration(generation)) return;
        NSLog(@"[Signaling] WebSocket closed: code=%ld, reason=%@", (long)code, reason);
        ClearHeartbeat();
        m_webSocketTask = nullptr;
    };


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


void SignalingClient::SetupHeartbeat() {
    ClearHeartbeat();


    RearmReceiveHandler();


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


        blockSelf->RearmReceiveHandler();
    }];
}


void SignalingClient::SendJson(const std::string &json) {
    if (!m_webSocketTask) return;
    NSURLSessionWebSocketTask *task = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;
    [task sendMessage:[[NSURLSessionWebSocketMessage alloc] initWithString:[NSString stringWithUTF8String:json.c_str()]]
    completionHandler:^(NSError *){}];
}


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


void SignalingClient::HandleMessage(const std::string &text) {
    NSData *data = [[NSString stringWithUTF8String:text.c_str()] dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *json = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    if (![json isKindOfClass:[NSDictionary class]]) return;


    NSDictionary *peerInfo = json[@"peer_info"];
    if ([peerInfo isKindOfClass:[NSDictionary class]]) {
        NSNumber *pid = peerInfo[@"id"];
        NSString *name = peerInfo[@"name"];
        if (pid && [name isKindOfClass:[NSString class]] && [name isEqualToString:[NSString stringWithUTF8String:m_peerName.c_str()]]) {
            m_peerId = pid.intValue;
            NSLog(@"[Signaling] Local peer id assigned: %d", m_peerId);
        }
    }



    if (json[@"ackid"]) {
        NSNumber *ourPid = peerInfo[@"id"];
        BOOL shouldAck = !ourPid || ourPid.intValue != m_peerId;
        if (shouldAck) {
            SendJson([NSString stringWithFormat:@"{\"ack\":%d}", [json[@"ackid"] intValue]].UTF8String);
        }
    }


    if (json[@"ack"]) return;


    if (json[@"hb"]) {
        SendJson("{\"hb\":1}");
        return;
    }


    NSDictionary *peerMsg = json[@"peer_msg"];
    if (![peerMsg isKindOfClass:[NSDictionary class]]) return;

    NSString *msgStr = peerMsg[@"msg"];
    if (![msgStr isKindOfClass:[NSString class]]) return;


    NSNumber *fromId = peerMsg[@"from"];
    if (fromId) {
        m_remotePeerId = fromId.intValue;
        NSLog(@"[Signaling] Remote peer id: %d", m_remotePeerId);
    }


    NSData *msgData = [msgStr dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:msgData options:0 error:nil];
    if (![payload isKindOfClass:[NSDictionary class]]) return;


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


void SignalingClient::OnOffer(SignalingOfferCallback cb) {
    m_onOffer = cb;
}

void SignalingClient::OnIceCandidate(SignalingIceCallback cb) {
    m_onIceCandidate = cb;
}


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


bool SignalingClient::IsConnected() const {
    if (!m_webSocketTask) return false;
    NSURLSessionWebSocketTask *task = (__bridge NSURLSessionWebSocketTask *)m_webSocketTask;
    return task.state == NSURLSessionTaskStateRunning;
}

}
