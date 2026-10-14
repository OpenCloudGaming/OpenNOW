use crate::sources::gfn::push::decrypt::{EceKeyPair, decrypt_web_push, derive_content_keys};
use crate::sources::gfn::push::protocol::{
    CheckinRequest, FrameReader, MCS_VERSION, STREAM_ACK_EXTENSION_ID, TAG_DATA_MESSAGE_STANZA,
    TAG_HEARTBEAT_ACK, TAG_HEARTBEAT_PING, TAG_IQ_STANZA, TAG_LOGIN_RESPONSE,
    decode_checkin_response, decode_frame, encode_checkin_request, encode_heartbeat_ack,
    encode_heartbeat_ping, encode_login_request, encode_stream_ack,
};
use crate::sources::gfn::push::registration::{
    HttpMethod, HttpRequest, HttpResponse, PushEndpoints, PushHttp, PushIdentity, Registration,
    RegistrationClient, checkin_and_register, pns_register,
};
use crate::sources::gfn::push::store::PushStateStore;
use crate::sources::gfn::push::transport::{PushTransport, PushTransportFactory};
use crate::sources::gfn::push::{MAXIMUM_CHANGED_IDS, MAXIMUM_FRAME_BYTES, PushError};
use crate::sources::gfn::push::{PushEvent, PushOwner, PushOwnerConfig, PushOwnerDeps, PushScope};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

fn scope() -> PushScope {
    PushScope {
        user_id: "user-1".into(),
        provider_id: "nvidia".into(),
        generation: 7,
    }
}

fn identity() -> PushIdentity {
    PushIdentity {
        project_id: "project".into(),
        api_key: "api-key".into(),
        sender_id: "123456789012".into(),
        app_id: "app".into(),
        firebase_app_id: "1:1:web:abc".into(),
        vapid_key: Some("vapid".into()),
    }
}

fn endpoints() -> PushEndpoints {
    PushEndpoints {
        checkin: "https://push.test/checkin".into(),
        c2dm: "https://push.test/c2dm".into(),
        fis: "https://push.test/fis/v1".into(),
        fcm_registration: "https://push.test/fcm/v1".into(),
        fcm_send: "https://push.test/fcm/send".into(),
        mcs_host: "push.test".into(),
        mcs_port: 5228,
        pns: "https://push.test/pns/v1".into(),
        pns_client_id: "client-id".into(),
    }
}

struct FakeHttp {
    responses: Mutex<Vec<HttpResponse>>,
    requests: Mutex<Vec<(String, String, Vec<u8>)>>,
}

impl FakeHttp {
    fn new(responses: Vec<HttpResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        })
    }
}

impl PushHttp for FakeHttp {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, PushError> {
        self.requests.lock().unwrap().push((
            request.url.clone(),
            match request.method {
                HttpMethod::Get => "GET".into(),
                HttpMethod::Post => "POST".into(),
            },
            request.body.clone(),
        ));
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(PushError::new("push_http_failed", "No scripted response"));
        }
        Ok(responses.remove(0))
    }
}

struct MemoryStore(Mutex<BTreeMap<String, Registration>>);

impl MemoryStore {
    fn new(existing: Option<Registration>) -> Arc<Self> {
        let mut map = BTreeMap::new();
        if let Some(registration) = existing {
            map.insert(scope().account_key(), registration);
        }
        Arc::new(Self(Mutex::new(map)))
    }
}

impl PushStateStore for MemoryStore {
    fn load(&self, account: &str) -> Result<Option<Registration>, PushError> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }

    fn save(&self, account: &str, registration: &Registration) -> Result<(), PushError> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_owned(), registration.clone());
        Ok(())
    }

    fn clear(&self, account: &str) -> Result<(), PushError> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct HeartbeatPing {
    last_received: Instant,
    sent: Instant,
}

#[derive(Clone, Copy, Default)]
struct HeartbeatTiming {
    last_received: Option<Instant>,
    ping: Option<HeartbeatPing>,
}

struct ScriptedFactory {
    connections: Mutex<Vec<Vec<Vec<u8>>>>,
    sent: Arc<Mutex<Vec<Vec<u8>>>>,
    connects: Arc<AtomicUsize>,
    fail_first: AtomicUsize,
    active: Arc<AtomicUsize>,
    maximum_active: Arc<AtomicUsize>,
    spacing_millis: Arc<AtomicUsize>,
    heartbeat: Arc<Mutex<HeartbeatTiming>>,
}

struct TrackedTransport {
    chunks: Vec<Vec<u8>>,
    sent: Arc<Mutex<Vec<Vec<u8>>>>,
    active: Arc<AtomicUsize>,
    spacing_millis: Arc<AtomicUsize>,
    heartbeat: Arc<Mutex<HeartbeatTiming>>,
}

impl PushTransport for TrackedTransport {
    fn send(&mut self, frame: &[u8], _timeout: Duration) -> Result<(), PushError> {
        if frame.first() == Some(&TAG_HEARTBEAT_PING) {
            let mut heartbeat = self.heartbeat.lock().unwrap();
            if heartbeat.ping.is_none()
                && let Some(last_received) = heartbeat.last_received
            {
                heartbeat.ping = Some(HeartbeatPing {
                    last_received,
                    sent: Instant::now(),
                });
            }
        }
        self.sent.lock().unwrap().push(frame.to_vec());
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> Result<Vec<u8>, PushError> {
        if self.chunks.is_empty() {
            std::thread::sleep(Duration::from_millis(5));
            return Ok(Vec::new());
        }
        let spacing = self.spacing_millis.load(Ordering::SeqCst);
        if spacing > 0 {
            std::thread::sleep(Duration::from_millis(spacing as u64));
        }
        let chunk = self.chunks.remove(0);
        if !chunk.is_empty() {
            self.heartbeat.lock().unwrap().last_received = Some(Instant::now());
        }
        Ok(chunk)
    }

    fn close(&mut self) {}
}

impl Drop for TrackedTransport {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl PushTransportFactory for ScriptedFactory {
    fn connect(
        &self,
        _host: &str,
        _port: u16,
        _timeout: Duration,
    ) -> Result<Box<dyn PushTransport>, PushError> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        if self.fail_first.load(Ordering::SeqCst) > 0 {
            self.fail_first.fetch_sub(1, Ordering::SeqCst);
            return Err(PushError::new(
                "push_transport_failed",
                "The push connection failed",
            ));
        }
        let mut connections = self.connections.lock().unwrap();
        if connections.is_empty() {
            return Err(PushError::new(
                "push_transport_failed",
                "No scripted connection",
            ));
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        Ok(Box::new(TrackedTransport {
            chunks: connections.remove(0),
            sent: Arc::clone(&self.sent),
            active: Arc::clone(&self.active),
            spacing_millis: Arc::clone(&self.spacing_millis),
            heartbeat: Arc::clone(&self.heartbeat),
        }))
    }
}

struct BlockingFactory {
    released: Mutex<bool>,
    wake: Condvar,
    connects: Arc<AtomicUsize>,
    connections: Mutex<Vec<Vec<Vec<u8>>>>,
    sent: Arc<Mutex<Vec<Vec<u8>>>>,
    active: Arc<AtomicUsize>,
    maximum_active: Arc<AtomicUsize>,
}

impl Drop for BlockingFactory {
    fn drop(&mut self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

impl PushTransportFactory for BlockingFactory {
    fn connect(
        &self,
        _host: &str,
        _port: u16,
        _timeout: Duration,
    ) -> Result<Box<dyn PushTransport>, PushError> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        let mut released = self.released.lock().unwrap();
        while !*released {
            let (state, timeout) = self
                .wake
                .wait_timeout(released, Duration::from_millis(20))
                .unwrap();
            released = state;
            if timeout.timed_out() {
                return Err(PushError::new(
                    "push_transport_failed",
                    "The push connection timed out",
                ));
            }
        }
        drop(released);
        let mut connections = self.connections.lock().unwrap();
        if connections.is_empty() {
            return Err(PushError::new(
                "push_transport_failed",
                "No scripted connection",
            ));
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        Ok(Box::new(TrackedTransport {
            chunks: connections.remove(0),
            sent: Arc::clone(&self.sent),
            active: Arc::clone(&self.active),
            spacing_millis: Arc::new(AtomicUsize::new(0)),
            heartbeat: Arc::new(Mutex::new(HeartbeatTiming::default())),
        }))
    }
}

fn registration() -> Registration {
    let keys = EceKeyPair::generate().unwrap();
    Registration {
        android_id: 42,
        security_token: 7,
        gcm_token: "gcm".into(),
        fcm_token: "fcm".into(),
        endpoint: "https://push.test/send/1".into(),
        private_key: keys.private_key(),
        public_key: keys.public_key().to_vec(),
        auth_secret: keys.auth_secret().to_vec(),
        received_persistent_ids: Vec::new(),
        created_at_seconds: 1,
        fingerprint: String::new(),
    }
}

fn fingerprint_for(scope: &PushScope) -> String {
    PushOwnerConfig::bounded(endpoints(), identity(), "nvidia".into(), "device".into())
        .registration_fingerprint(scope)
}

fn rfc_input_key(keys: &EceKeyPair, server_public: &[u8]) -> [u8; 32] {
    let server_secret =
        p256::SecretKey::from_slice(&decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"))
            .unwrap();
    let user_public = p256::PublicKey::from_sec1_bytes(keys.public_key()).unwrap();
    let shared =
        p256::ecdh::diffie_hellman(server_secret.to_nonzero_scalar(), user_public.as_affine());
    let mut key_info = Vec::new();
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(keys.public_key());
    key_info.extend_from_slice(server_public);
    let mut input_key = [0_u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(keys.auth_secret()), shared.raw_secret_bytes())
        .expand(&key_info, &mut input_key)
        .unwrap();
    input_key
}

fn derive_test_input_key(keys: &EceKeyPair, server_public: &[u8]) -> [u8; 32] {
    let mut server_scalar = [0_u8; 32];
    server_scalar[31] = 9;
    let server_secret = p256::SecretKey::from_slice(&server_scalar).unwrap();
    let user_public = p256::PublicKey::from_sec1_bytes(keys.public_key()).unwrap();
    let shared =
        p256::ecdh::diffie_hellman(server_secret.to_nonzero_scalar(), user_public.as_affine());
    let mut key_info = Vec::new();
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(keys.public_key());
    key_info.extend_from_slice(server_public);
    let mut input_key = [0_u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(keys.auth_secret()), shared.raw_secret_bytes())
        .expand(&key_info, &mut input_key)
        .unwrap();
    input_key
}

fn encrypt_record(
    content_key: &[u8; 16],
    base_nonce: &[u8; 12],
    sequence: u64,
    record: &[u8],
) -> Vec<u8> {
    let cipher = Aes128Gcm::new_from_slice(content_key).unwrap();
    let mut nonce = *base_nonce;
    for (target, byte) in nonce[4..].iter_mut().zip(sequence.to_be_bytes().iter()) {
        *target ^= *byte;
    }
    cipher.encrypt(Nonce::from_slice(&nonce), record).unwrap()
}

fn aes128gcm_body(salt: &[u8; 16], server_public: &[u8], records: &[u8]) -> Vec<u8> {
    aes128gcm_body_with_record_size(salt, server_public, 4096, records)
}

fn aes128gcm_body_with_record_size(
    salt: &[u8; 16],
    server_public: &[u8],
    record_size: u32,
    records: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(salt);
    body.extend_from_slice(&record_size.to_be_bytes());
    body.push(server_public.len() as u8);
    body.extend_from_slice(server_public);
    body.extend_from_slice(records);
    body
}

fn data_stanza(
    keys: &EceKeyPair,
    fields: &[(&str, String)],
    persistent_id: &str,
    immediate_ack: bool,
) -> Vec<u8> {
    let mut data = serde_json::Map::new();
    for (key, value) in fields {
        data.insert((*key).to_owned(), serde_json::Value::String(value.clone()));
    }
    let payload = serde_json::json!({ "data": data }).to_string();
    let (ciphertext, server_public, salt) = legacy_encryption(keys, payload.as_bytes());
    let mut stanza = Vec::new();
    write_string(3, "from", &mut stanza);
    write_string(5, "category", &mut stanza);
    write_app_data(
        "crypto-key",
        &format!("dh={}", URL_SAFE_NO_PAD.encode(&server_public)),
        &mut stanza,
    );
    write_app_data(
        "encryption",
        &format!("salt={}", URL_SAFE_NO_PAD.encode(salt)),
        &mut stanza,
    );
    write_string(9, persistent_id, &mut stanza);
    write_bytes(21, &ciphertext, &mut stanza);
    if immediate_ack {
        write_tag(24, 0, &mut stanza);
        write_varint(1, &mut stanza);
    }
    stanza
}

fn data_frame(keys: &EceKeyPair, fields: &[(&str, String)], persistent_id: &str) -> Vec<u8> {
    server_frame(
        TAG_DATA_MESSAGE_STANZA,
        &data_stanza(keys, fields, persistent_id, false),
    )
}

fn legacy_encryption(keys: &EceKeyPair, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>, [u8; 16]) {
    let mut server_scalar = [0_u8; 32];
    server_scalar[31] = 9;
    let server_secret = p256::SecretKey::from_slice(&server_scalar).unwrap();
    let server_public = p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(
        &server_secret.public_key(),
        false,
    )
    .as_bytes()
    .to_vec();
    let salt = [3_u8; 16];
    let user_public = p256::PublicKey::from_sec1_bytes(keys.public_key()).unwrap();
    let shared =
        p256::ecdh::diffie_hellman(server_secret.to_nonzero_scalar(), user_public.as_affine());
    let mut input_key = [0_u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(keys.auth_secret()), shared.raw_secret_bytes())
        .expand(b"Content-Encoding: auth\0", &mut input_key)
        .unwrap();
    let mut context = Vec::new();
    context.extend_from_slice(&[0, 65]);
    context.extend_from_slice(keys.public_key());
    context.extend_from_slice(&[0, 65]);
    context.extend_from_slice(&server_public);
    let mut key_info = b"Content-Encoding: aesgcm\0P-256\0".to_vec();
    key_info.extend_from_slice(&context);
    let mut nonce_info = b"Content-Encoding: nonce\0P-256\0".to_vec();
    nonce_info.extend_from_slice(&context);
    let prk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&salt), &input_key);
    let mut content_key = [0_u8; 16];
    let mut nonce = [0_u8; 12];
    prk.expand(&key_info, &mut content_key).unwrap();
    prk.expand(&nonce_info, &mut nonce).unwrap();
    let mut record = vec![0, 0];
    record.extend_from_slice(plaintext);
    let cipher = Aes128Gcm::new_from_slice(&content_key).unwrap();
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), record.as_slice())
        .unwrap();
    (ciphertext, server_public, salt)
}

fn aes128gcm_encryption(keys: &EceKeyPair, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>, [u8; 16]) {
    let mut server_scalar = [0_u8; 32];
    server_scalar[31] = 9;
    let server_secret = p256::SecretKey::from_slice(&server_scalar).unwrap();
    let server_public = p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(
        &server_secret.public_key(),
        false,
    )
    .as_bytes()
    .to_vec();
    let salt = [3_u8; 16];
    let input_key = derive_test_input_key(keys, &server_public);
    let (content_key, base_nonce) = derive_content_keys(&input_key, &salt).unwrap();
    let mut record = plaintext.to_vec();
    record.push(2);
    let ciphertext = encrypt_record(&content_key, &base_nonce, 0, &record);
    (ciphertext, server_public, salt)
}

fn aes128gcm_data_frame(
    keys: &EceKeyPair,
    fields: &[(&str, String)],
    persistent_id: &str,
) -> Vec<u8> {
    let mut data = serde_json::Map::new();
    for (key, value) in fields {
        data.insert((*key).to_owned(), serde_json::Value::String(value.clone()));
    }
    let payload = serde_json::json!({ "data": data }).to_string();
    let mut server_scalar = [0_u8; 32];
    server_scalar[31] = 9;
    let server_secret = p256::SecretKey::from_slice(&server_scalar).unwrap();
    let server_public = p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(
        &server_secret.public_key(),
        false,
    )
    .as_bytes()
    .to_vec();
    let salt = [5_u8; 16];
    let input_key = derive_test_input_key(keys, &server_public);
    let (content_key, base_nonce) = derive_content_keys(&input_key, &salt).unwrap();
    let mut record = payload.into_bytes();
    record.push(2);
    let ciphertext = encrypt_record(&content_key, &base_nonce, 0, &record);
    let mut stanza = Vec::new();
    write_string(3, "from", &mut stanza);
    write_string(5, "category", &mut stanza);
    write_string(9, persistent_id, &mut stanza);
    write_bytes(
        21,
        &aes128gcm_body(&salt, &server_public, &ciphertext),
        &mut stanza,
    );
    server_frame(TAG_DATA_MESSAGE_STANZA, &stanza)
}

fn server_frame(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 6);
    frame.push(tag);
    write_varint(payload.len() as u64, &mut frame);
    frame.extend_from_slice(payload);
    frame
}

fn write_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn write_tag(field: u64, wire: u64, out: &mut Vec<u8>) {
    write_varint((field << 3) | wire, out);
}

fn write_bytes(field: u64, value: &[u8], out: &mut Vec<u8>) {
    write_tag(field, 2, out);
    write_varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

fn write_string(field: u64, value: &str, out: &mut Vec<u8>) {
    write_bytes(field, value.as_bytes(), out);
}

fn write_app_data(key: &str, value: &str, out: &mut Vec<u8>) {
    let mut nested = Vec::new();
    write_string(1, key, &mut nested);
    write_string(2, value, &mut nested);
    write_bytes(7, &nested, out);
}

fn checkin_body(android_id: u64, security_token: u64) -> Vec<u8> {
    let mut body = Vec::new();
    write_tag(7, 1, &mut body);
    body.extend_from_slice(&android_id.to_le_bytes());
    write_tag(8, 1, &mut body);
    body.extend_from_slice(&security_token.to_le_bytes());
    body
}

fn push_http(responses: Vec<HttpResponse>) -> Arc<FakeHttp> {
    FakeHttp::new(responses)
}

fn responses_for_registration() -> Vec<HttpResponse> {
    vec![
        HttpResponse {
            status: 200,
            body: checkin_body(42, 7),
        },
        HttpResponse {
            status: 200,
            body: b"token=gcm-token".to_vec(),
        },
        HttpResponse {
            status: 200,
            body: br#"{"authToken":{"token":"fis-token"}}"#.to_vec(),
        },
        HttpResponse {
            status: 200,
            body: br#"{"token":"fcm-token"}"#.to_vec(),
        },
        HttpResponse {
            status: 204,
            body: Vec::new(),
        },
    ]
}

struct OwnerHarness {
    owner: PushOwner,
    factory: Arc<ScriptedFactory>,
    store: Arc<MemoryStore>,
    desired: Arc<Mutex<Option<PushScope>>>,
    config: PushOwnerConfig,
    http: Arc<FakeHttp>,
}

fn owner_harness(
    http: Arc<FakeHttp>,
    connections: Vec<Vec<Vec<u8>>>,
    sink: Arc<dyn Fn(PushEvent, u64) + Send + Sync>,
    initial: Option<PushScope>,
    existing: Option<Registration>,
) -> OwnerHarness {
    tuned_owner_harness(http, connections, sink, initial, existing, |_| {})
}

fn tuned_owner_harness(
    http: Arc<FakeHttp>,
    connections: Vec<Vec<Vec<u8>>>,
    sink: Arc<dyn Fn(PushEvent, u64) + Send + Sync>,
    initial: Option<PushScope>,
    existing: Option<Registration>,
    tune: impl FnOnce(&mut PushOwnerConfig),
) -> OwnerHarness {
    let store = MemoryStore::new(existing);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let spacing_millis = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(ScriptedFactory {
        connections: Mutex::new(connections),
        sent,
        connects: Arc::new(AtomicUsize::new(0)),
        fail_first: AtomicUsize::new(0),
        active: Arc::clone(&active),
        maximum_active: Arc::clone(&maximum_active),
        spacing_millis: Arc::clone(&spacing_millis),
        heartbeat: Arc::new(Mutex::new(HeartbeatTiming::default())),
    });
    let desired = Arc::new(Mutex::new(initial));
    let scope_source = Arc::clone(&desired);
    let worker_scope = Arc::clone(&scope_source);
    let token_source = Arc::new(move |scope: &PushScope| {
        let current = worker_scope.lock().unwrap().clone()?;
        (current.account_key() == scope.account_key()).then(|| "gfnjwt".to_owned())
    });
    let mut config = PushOwnerConfig {
        connect_timeout: Duration::from_secs(1),
        read_timeout: Duration::from_millis(20),
        login_timeout: Duration::from_secs(2),
        heartbeat_interval: Duration::from_secs(30),
        heartbeat_ack_timeout: Duration::from_millis(500),
        minimum_heartbeat_interval: Duration::from_millis(100),
        maximum_heartbeat_interval: Duration::from_secs(2),
        coalescing_window: Duration::from_millis(30),
        backoff_initial: Duration::from_millis(10),
        backoff_maximum: Duration::from_millis(40),
        stop_deadline: Duration::from_secs(2),
        ..PushOwnerConfig::bounded(endpoints(), identity(), "nvidia".into(), "device".into())
    };
    tune(&mut config);
    let http_dep: Arc<dyn PushHttp> = http.clone();
    let owner = PushOwner::new(
        config.clone(),
        PushOwnerDeps {
            http: http_dep,
            transport: factory.clone(),
            store: store.clone(),
            sink,
            scope_source: Arc::new(move || scope_source.lock().unwrap().clone()),
            token_source,
        },
    );
    OwnerHarness {
        owner,
        factory,
        store,
        desired,
        config,
        http,
    }
}

impl OwnerHarness {
    fn config(&self) -> PushOwnerConfig {
        self.config.clone()
    }
}

fn shutdown(harness: &mut OwnerHarness) {
    *harness.desired.lock().unwrap() = None;
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.owner.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!harness.owner.is_running());
    assert!(harness.owner.shutdown(Duration::from_secs(2)));
    assert_eq!(harness.owner.worker_count(), 0);
}

#[test]
fn encryption_round_trip_decrypts_the_payload() {
    let keys = EceKeyPair::generate().unwrap();
    let (ciphertext, server_public, salt) = legacy_encryption(&keys, b"{\"data\":{}}");
    let crypto_key = format!("dh={}", URL_SAFE_NO_PAD.encode(&server_public));
    let encryption = format!("salt={}", URL_SAFE_NO_PAD.encode(salt));
    let plaintext = decrypt_web_push(
        &keys,
        None,
        Some(&crypto_key),
        Some(&encryption),
        &ciphertext,
    )
    .unwrap();
    assert_eq!(plaintext, b"{\"data\":{}}");
    let (aes128_body, aes128_public, aes128_salt) = aes128gcm_encryption(&keys, b"{\"data\":{}}");
    let body = aes128gcm_body(&aes128_salt, &aes128_public, &aes128_body);
    let plaintext = decrypt_web_push(&keys, None, None, None, &body).unwrap();
    assert_eq!(plaintext, b"{\"data\":{}}");
}

#[test]
fn decryption_matches_the_rfc8291_fixed_vector() {
    let keys = EceKeyPair::from_parts(
        &decode("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94"),
        &decode("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
        &decode("BTBZMqHH6r4Tts7J_aSIgg"),
    )
    .unwrap();
    let body = decode(
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
    );
    let plaintext = decrypt_web_push(&keys, None, None, None, &body).unwrap();
    assert_eq!(plaintext, b"When I grow up, I want to be a watermelon");
}

#[test]
fn decryption_rejects_a_tampered_rfc8291_body() {
    let keys = EceKeyPair::from_parts(
        &decode("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94"),
        &decode("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
        &decode("BTBZMqHH6r4Tts7J_aSIgg"),
    )
    .unwrap();
    let mut body = decode(
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
    );
    let last = body.len() - 1;
    body[last] ^= 0x40;
    assert_eq!(
        decrypt_web_push(&keys, None, None, None, &body)
            .unwrap_err()
            .code,
        "push_decrypt_failed"
    );
}

#[test]
fn multi_record_payloads_use_the_documented_sequence_nonce() {
    let keys = EceKeyPair::from_parts(
        &decode("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94"),
        &decode("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
        &decode("BTBZMqHH6r4Tts7J_aSIgg"),
    )
    .unwrap();
    let salt = decode("DGv6ra1nlYgDCS1FRnbzlw");
    let salt: [u8; 16] = salt.try_into().unwrap();
    let server_public = decode(
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
    );
    let input_key = rfc_input_key(&keys, &server_public);
    assert_eq!(
        input_key.to_vec(),
        decode("S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg"),
        "the RFC key derivation must reproduce the published IKM"
    );
    let (content_key, base_nonce) = derive_content_keys(&input_key, &salt).unwrap();
    assert_eq!(
        content_key.to_vec(),
        decode("oIhVW04MRdy2XN9CiKLxTg"),
        "the RFC key derivation must reproduce the published CEK"
    );
    assert_eq!(
        base_nonce.to_vec(),
        decode("4h_95klXJ5E_qnoN"),
        "the RFC key derivation must reproduce the published nonce"
    );
    let first = encrypt_record(
        &content_key,
        &base_nonce,
        0,
        b"When I grow up, I want to be a watermelon\x02",
    );
    assert_eq!(
        first,
        decode("8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ")
    );
    let mut records = Vec::new();
    records.extend_from_slice(&encrypt_record(&content_key, &base_nonce, 0, b"a\x01"));
    records.extend_from_slice(&encrypt_record(&content_key, &base_nonce, 1, b"b\x02"));
    let body = aes128gcm_body_with_record_size(&salt, &server_public, 18, &records);
    let plaintext = decrypt_web_push(&keys, None, None, None, &body).unwrap();
    assert_eq!(plaintext, b"ab");
}

#[test]
fn decryption_honors_a_declared_aes128gcm_content_encoding() {
    let keys = EceKeyPair::generate().unwrap();
    let (ciphertext, server_public, salt) = aes128gcm_encryption(&keys, b"{\"data\":{}}");
    let body = aes128gcm_body(&salt, &server_public, &ciphertext);
    let misleading = format!("dh={}", URL_SAFE_NO_PAD.encode([1_u8; 65]));
    let misleading_salt = format!("salt={}", URL_SAFE_NO_PAD.encode([2_u8; 16]));
    let plaintext = decrypt_web_push(
        &keys,
        Some("aes128gcm"),
        Some(&misleading),
        Some(&misleading_salt),
        &body,
    )
    .unwrap();
    assert_eq!(plaintext, b"{\"data\":{}}");
}

#[test]
fn decryption_honors_a_declared_aesgcm_content_encoding() {
    let keys = EceKeyPair::generate().unwrap();
    let (ciphertext, server_public, salt) = legacy_encryption(&keys, b"{\"data\":{}}");
    let crypto_key = format!("dh={}", URL_SAFE_NO_PAD.encode(&server_public));
    let encryption = format!("salt={}", URL_SAFE_NO_PAD.encode(salt));
    let plaintext = decrypt_web_push(
        &keys,
        Some("aesgcm"),
        Some(&crypto_key),
        Some(&encryption),
        &ciphertext,
    )
    .unwrap();
    assert_eq!(plaintext, b"{\"data\":{}}");
    assert_eq!(
        decrypt_web_push(&keys, Some("aesgcm"), None, Some(&encryption), &ciphertext)
            .err()
            .map(|error| error.code),
        Some("push_decrypt_invalid")
    );
}

#[test]
fn decryption_rejects_a_declared_unsupported_content_encoding() {
    let keys = EceKeyPair::generate().unwrap();
    let (ciphertext, server_public, salt) = aes128gcm_encryption(&keys, b"{\"data\":{}}");
    let body = aes128gcm_body(&salt, &server_public, &ciphertext);
    assert_eq!(
        decrypt_web_push(&keys, Some("br"), None, None, &body)
            .err()
            .map(|error| error.code),
        Some("push_decrypt_unsupported")
    );
}

#[test]
fn decryption_rejects_an_unsupported_record_size() {
    let keys = EceKeyPair::generate().unwrap();
    let (ciphertext, server_public, salt) = aes128gcm_encryption(&keys, b"{\"data\":{}}");
    let mut body = aes128gcm_body(&salt, &server_public, &ciphertext);
    body[16..20].copy_from_slice(&0_u32.to_be_bytes());
    assert_eq!(
        decrypt_web_push(&keys, None, None, None, &body)
            .unwrap_err()
            .code,
        "push_decrypt_invalid"
    );
}

#[test]
fn stored_key_material_must_match_its_public_key() {
    let first = EceKeyPair::generate().unwrap();
    let second = EceKeyPair::generate().unwrap();
    let error = match EceKeyPair::from_parts(
        &first.private_key(),
        second.public_key(),
        first.auth_secret(),
    ) {
        Ok(_) => panic!("an unrelated public key was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.code, "push_key_mismatch");
    EceKeyPair::from_parts(
        &first.private_key(),
        first.public_key(),
        first.auth_secret(),
    )
    .unwrap();
    let malformed = EceKeyPair::from_parts(&first.private_key(), &[0_u8; 5], first.auth_secret());
    assert_eq!(
        malformed.err().map(|error| error.code),
        Some("push_key_invalid")
    );
    let unrelated = EceKeyPair::from_parts(&first.private_key(), &[0_u8; 65], first.auth_secret());
    assert_eq!(
        unrelated.err().map(|error| error.code),
        Some("push_key_mismatch")
    );
}

#[test]
fn config_updates_restart_the_session_without_a_second_worker() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new()), session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(harness.owner.worker_count(), 1);
    let mut updated = harness.config();
    updated.coalescing_window = Duration::from_millis(10);
    harness.owner.update_config(updated);
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        harness.factory.connects.load(Ordering::SeqCst) >= 2,
        "config update did not restart the session"
    );
    assert_eq!(
        harness.owner.worker_count(),
        1,
        "config update spawned a second worker"
    );
    assert!(harness.factory.maximum_active.load(Ordering::SeqCst) <= 1);
    shutdown(&mut harness);
}

#[test]
fn a_continuous_stream_never_postpones_a_coalesced_event() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut frames: Vec<Vec<u8>> = session_chunks(Vec::new());
    for index in 0..60 {
        frames.push(data_frame(
            &keys,
            &[
                ("messageType", "\"LIBRARY_CHANGE\"".into()),
                ("changedIds", format!("[\"app-{index}\"]")),
            ],
            &format!("p{index}"),
        ));
    }
    let connections = vec![frames];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.factory.spacing_millis.store(5, Ordering::SeqCst);
    harness.owner.update_config(PushOwnerConfig {
        coalescing_window: Duration::from_millis(40),
        ..harness.config()
    });
    harness.owner.start().unwrap();
    let event = events_rx
        .recv_timeout(Duration::from_millis(200))
        .expect("a coalescing window must close while messages keep arriving");
    match event {
        PushEvent::Library { changed_ids } => assert!(!changed_ids.is_empty()),
        other => panic!("unexpected event {other:?}"),
    }
    let mut events = 1;
    while events_rx.recv_timeout(Duration::from_millis(80)).is_ok() {
        events += 1;
    }
    assert!(events >= 2, "later windows must keep closing, saw {events}");
    shutdown(&mut harness);
}

#[test]
fn a_provider_mismatch_never_reaches_the_token_source() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut other_provider = scope();
    other_provider.provider_id = "alliance".into();
    *harness.desired.lock().unwrap() = Some(other_provider);
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.owner.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !harness.owner.is_running(),
        "a foreign provider must stop the session"
    );
    let connects = harness.factory.connects.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        harness.factory.connects.load(Ordering::SeqCst),
        connects,
        "a foreign provider must not open a new push session"
    );
    shutdown(&mut harness);
}

#[test]
fn configuration_snapshots_are_atomic_with_their_generation() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let owner = Arc::new(harness.owner);
    let reader = {
        let owner = Arc::clone(&owner);
        std::thread::spawn(move || {
            let mut highest = 0_u64;
            for _ in 0..200_000 {
                let (config, generation) = owner.config();
                assert!(
                    generation >= highest,
                    "config generations must never move backwards"
                );
                highest = generation;
                assert_eq!(
                    config.coalescing_window,
                    expected_window(generation),
                    "torn configuration snapshot at generation {generation}"
                );
            }
        })
    };
    let writer = {
        let owner = Arc::clone(&owner);
        std::thread::spawn(move || {
            for index in 1..=2000_u64 {
                let (mut config, generation) = owner.config();
                assert_eq!(config.coalescing_window, expected_window(generation));
                config.coalescing_window = Duration::from_millis(index % 97 + 1);
                owner.update_config(config);
            }
        })
    };
    let _ = writer.join();
    let _ = reader.join();
    let mut harness = OwnerHarness {
        owner: Arc::try_unwrap(owner).ok().expect("owner"),
        factory: harness.factory,
        store: harness.store,
        desired: harness.desired,
        config: harness.config,
        http: harness.http,
    };
    shutdown(&mut harness);
}

fn expected_window(generation: u64) -> Duration {
    if generation == 1 {
        Duration::from_millis(30)
    } else {
        Duration::from_millis(((generation - 1) % 97) + 1)
    }
}

fn login_refusal_frame(code: u64) -> Vec<u8> {
    let mut refusal = Vec::new();
    write_tag(1, 0, &mut refusal);
    write_varint(code, &mut refusal);
    let mut response = Vec::new();
    write_bytes(3, &refusal, &mut response);
    server_frame(TAG_LOGIN_RESPONSE, &response)
}

fn login_response_frame(heartbeat_interval_ms: Option<i32>) -> Vec<u8> {
    let mut payload = Vec::new();
    write_string(1, "login-id", &mut payload);
    if let Some(interval) = heartbeat_interval_ms {
        let mut config = Vec::new();
        write_tag(3, 0, &mut config);
        write_varint(interval as u64, &mut config);
        write_bytes(7, &config, &mut payload);
    }
    server_frame(TAG_LOGIN_RESPONSE, &payload)
}

fn session_chunks(extra: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut chunks = vec![vec![MCS_VERSION], login_response_frame(None)];
    chunks.extend(extra);
    chunks
}

fn checkin_requests(harness: &OwnerHarness) -> usize {
    harness
        .http
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(url, _, _)| url == &endpoints().checkin)
        .count()
}

#[test]
fn a_login_refusal_refreshes_the_registration_without_minting_a_new_identity() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "stale-fcm-token".into();
    let refused = login_refusal_frame(2);
    let connections = vec![vec![vec![MCS_VERSION], refused], vec![vec![MCS_VERSION]]];
    let mut responses = responses_for_registration();
    responses[3] = HttpResponse {
        status: 200,
        body: br#"{"token":"refreshed-fcm-token"}"#.to_vec(),
    };
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(responses),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while harness.factory.connects.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        harness.factory.connects.load(Ordering::SeqCst) >= 2,
        "the refused registration must reconnect"
    );
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("a refreshed registration must be stored");
    assert_eq!(
        stored.fcm_token, "refreshed-fcm-token",
        "the cached credentials must be replaced"
    );
    assert_eq!(
        (stored.android_id, stored.security_token),
        (42, 7),
        "a login refusal must retain the stored device identity"
    );
    let requests = harness.http.requests.lock().unwrap().clone();
    let checkin = requests
        .iter()
        .find(|(url, _, _)| url == &endpoints().checkin)
        .expect("the refresh must present the stored identity at check-in");
    assert_eq!(
        checkin.2,
        encode_checkin_request(&CheckinRequest {
            android_id: Some(42),
            security_token: Some(7),
        }),
        "the refresh must re-use the stored device identity"
    );
    shutdown(&mut harness);
}

#[test]
fn a_zero_login_error_code_is_not_a_rejection() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "kept-fcm-token".into();
    let zero_error = login_refusal_frame(0);
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![vec![vec![MCS_VERSION], zero_error]],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        checkin_requests(&harness),
        0,
        "a zero login error code must not trigger a registration refresh"
    );
    assert_eq!(
        harness.factory.connects.load(Ordering::SeqCst),
        1,
        "a zero login error code must not restart the session"
    );
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("a zero login error code must keep the stored registration");
    assert_eq!(stored.fcm_token, "kept-fcm-token");
    assert!(harness.owner.is_running(), "the session must stay active");
    shutdown(&mut harness);
}

#[test]
fn repeated_login_refusals_keep_refreshing_without_minting_a_new_identity() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "stale-fcm-token".into();
    let mut responses = Vec::new();
    for index in 0..3 {
        let mut round = responses_for_registration();
        round[3] = HttpResponse {
            status: 200,
            body: format!("{{\"token\":\"refreshed-fcm-token-{index}\"}}").into_bytes(),
        };
        responses.extend(round);
    }
    let mut harness = owner_harness(
        push_http(responses),
        vec![
            vec![vec![MCS_VERSION], login_refusal_frame(2)],
            vec![vec![MCS_VERSION], login_refusal_frame(7)],
            vec![vec![MCS_VERSION], login_refusal_frame(13)],
        ],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while checkin_requests(&harness) < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        checkin_requests(&harness) >= 3,
        "a repeatedly refused registration must keep being refreshed"
    );
    let requests = harness.http.requests.lock().unwrap().clone();
    let checkins: Vec<&(String, String, Vec<u8>)> = requests
        .iter()
        .filter(|(url, _, _)| url == &endpoints().checkin)
        .collect();
    for (_, _, body) in &checkins {
        assert_eq!(
            body,
            &encode_checkin_request(&CheckinRequest {
                android_id: Some(42),
                security_token: Some(7),
            }),
            "a refresh must retain the stored device identity"
        );
    }
    if let Some(stored) = harness.store.load(&scope().account_key()).unwrap() {
        assert_ne!(
            stored.fcm_token, "stale-fcm-token",
            "a refused registration must never return to the cache"
        );
        assert_eq!(
            stored.android_id, 42,
            "the device identity must be retained"
        );
    }
    shutdown(&mut harness);
}

#[test]
fn corrupt_key_material_is_refreshed_without_a_session_attempt() {
    let mismatched = EceKeyPair::generate().unwrap();
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = mismatched.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "corrupt-fcm-token".into();
    let mut responses = responses_for_registration();
    responses[3] = HttpResponse {
        status: 200,
        body: br#"{"token":"fresh-fcm-token"}"#.to_vec(),
    };
    let mut harness = owner_harness(
        push_http(responses),
        vec![session_chunks(Vec::new()), session_chunks(Vec::new())],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("unusable key material must be replaced");
    assert_eq!(stored.fcm_token, "fresh-fcm-token");
    assert!(
        EceKeyPair::from_parts(&stored.private_key, &stored.public_key, &stored.auth_secret)
            .is_ok(),
        "the refreshed registration must carry loadable key material"
    );
    assert_eq!(
        harness.factory.connects.load(Ordering::SeqCst),
        1,
        "unusable key material must never open a push session"
    );
    shutdown(&mut harness);
}

#[test]
fn a_rejected_device_identity_is_cleared_before_the_next_check_in() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.created_at_seconds = existing.created_at_seconds.saturating_sub(8 * 24 * 60 * 60);
    let mut responses = vec![HttpResponse {
        status: 401,
        body: Vec::new(),
    }];
    let mut round = responses_for_registration();
    round[0] = HttpResponse {
        status: 200,
        body: checkin_body(84, 9),
    };
    responses.extend(round);
    let mut harness = owner_harness(
        push_http(responses),
        vec![session_chunks(Vec::new())],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while checkin_requests(&harness) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    while harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .is_none_or(|stored| stored.fcm_token != "fcm-token")
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let requests = harness.http.requests.lock().unwrap().clone();
    let checkins: Vec<&(String, String, Vec<u8>)> = requests
        .iter()
        .filter(|(url, _, _)| url == &endpoints().checkin)
        .collect();
    assert!(
        checkins.len() >= 2,
        "a rejected device identity must be re-checked-in"
    );
    assert_eq!(
        checkins[0].2,
        encode_checkin_request(&CheckinRequest {
            android_id: Some(42),
            security_token: Some(7),
        }),
        "the first check-in must present the stored device identity"
    );
    assert_eq!(
        checkins[1].2,
        encode_checkin_request(&CheckinRequest {
            android_id: None,
            security_token: None,
        }),
        "a rejected device identity must not be presented again"
    );
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("a fresh registration must be stored");
    assert_eq!(stored.fcm_token, "fcm-token");
    assert_eq!(
        (stored.android_id, stored.security_token),
        (84, 9),
        "a rejected device identity must be replaced by the fresh identity"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        harness.owner.is_running(),
        "the fresh registration must open a push session"
    );
    assert_eq!(harness.factory.connects.load(Ordering::SeqCst), 1);
    shutdown(&mut harness);
}

#[test]
fn transient_failures_never_discard_a_stored_registration() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "kept-fcm-token".into();
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.factory.fail_first.store(3, Ordering::SeqCst);
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while harness.factory.connects.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("a transient failure must keep the stored registration");
    assert_eq!(stored.fcm_token, "kept-fcm-token");
    shutdown(&mut harness);
}

#[test]
fn registration_reads_enforce_a_real_byte_limit() {
    use crate::sources::gfn::push::registration::{MAXIMUM_RESPONSE_BYTES, read_bounded};
    let exact = vec![b'a'; MAXIMUM_RESPONSE_BYTES as usize];
    let read = read_bounded(std::io::Cursor::new(exact.clone()), MAXIMUM_RESPONSE_BYTES).unwrap();
    assert_eq!(read.len(), exact.len());
    let oversized = vec![b'a'; MAXIMUM_RESPONSE_BYTES as usize + 1];
    assert_eq!(
        read_bounded(std::io::Cursor::new(oversized), MAXIMUM_RESPONSE_BYTES)
            .err()
            .map(|error| error.code),
        Some("push_http_failed")
    );
    let unbounded = vec![b'a'; 4 * 1024 * 1024];
    assert!(read_bounded(std::io::Cursor::new(unbounded), MAXIMUM_RESPONSE_BYTES).is_err());
}

struct LocalHttpServer {
    url: String,
    _handle: std::thread::JoinHandle<()>,
}

fn serve_local_http(responses: Vec<Vec<u8>>) -> LocalHttpServer {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for response in responses {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            loop {
                match socket.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        request.extend_from_slice(&chunk[..count]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = socket.write_all(&response);
            let _ = socket.shutdown(std::net::Shutdown::Write);
        }
    });
    LocalHttpServer {
        url: format!("http://{address}"),
        _handle: handle,
    }
}

fn execute_local(
    url: &str,
    timeout: Duration,
) -> Result<crate::sources::gfn::push::registration::HttpResponse, PushError> {
    use crate::sources::gfn::push::registration::{HttpMethod, HttpRequest, ReqwestPushHttp};
    ReqwestPushHttp::new()?
        .execute(&HttpRequest {
            url: url.to_owned(),
            method: HttpMethod::Get,
            headers: Vec::new(),
            body: Vec::new(),
            timeout,
        })
        .map_err(|error| PushError::new(error.code, error.message))
}

fn expect_request_error<T>(result: Result<T, PushError>) -> PushError {
    match result {
        Ok(_) => panic!("the push request must fail"),
        Err(error) => error,
    }
}

#[test]
fn an_oversized_close_delimited_response_is_rejected_locally() {
    use crate::sources::gfn::push::registration::MAXIMUM_RESPONSE_BYTES;
    let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    response.extend(std::iter::repeat_n(
        b'a',
        MAXIMUM_RESPONSE_BYTES as usize + 1,
    ));
    let server = serve_local_http(vec![response]);
    let error = expect_request_error(execute_local(&server.url, Duration::from_secs(5)));
    assert_eq!(error.code, "push_http_failed");
}

#[test]
fn an_oversized_chunked_response_is_rejected_locally() {
    use crate::sources::gfn::push::registration::MAXIMUM_RESPONSE_BYTES;
    let chunk = vec![b'a'; 40 * 1024];
    let header = format!("{:x}\r\n", chunk.len());
    let mut response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    response.extend_from_slice(header.as_bytes());
    response.extend_from_slice(&chunk);
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(header.as_bytes());
    response.extend_from_slice(&chunk);
    response.extend_from_slice(b"\r\n0\r\n\r\n");
    assert!(
        (2 * chunk.len()) as u64 > MAXIMUM_RESPONSE_BYTES,
        "the scripted body must exceed the accepted size"
    );
    let server = serve_local_http(vec![response]);
    let error = expect_request_error(execute_local(&server.url, Duration::from_secs(5)));
    assert_eq!(error.code, "push_http_failed");
}

#[test]
fn a_response_at_the_byte_limit_is_accepted_locally() {
    use crate::sources::gfn::push::registration::MAXIMUM_RESPONSE_BYTES;
    let body = vec![b'a'; MAXIMUM_RESPONSE_BYTES as usize];
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    let server = serve_local_http(vec![response]);
    let response = execute_local(&server.url, Duration::from_secs(5))
        .expect("a response at the limit must be accepted");
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), body.len());
}

#[test]
fn a_malformed_installation_response_is_rejected_locally() {
    use crate::sources::gfn::push::registration::firebase_installation_token;
    let server = serve_local_http(vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{".to_vec(),
    ]);
    let mut endpoints = endpoints();
    endpoints.fis = server.url.clone();
    let error = expect_request_error(firebase_installation_token(
        &RegistrationClient {
            http: &crate::sources::gfn::push::registration::ReqwestPushHttp::new().unwrap(),
            endpoints: &endpoints,
            timeout: Duration::from_secs(5),
        },
        &identity(),
        "fid",
    ));
    assert_eq!(error.code, "push_fis_incomplete");
}

#[test]
fn a_stalled_push_response_honors_the_request_deadline() {
    use std::io::Read as _;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let mut chunk = [0_u8; 1024];
        let _ = socket.read(&mut chunk);
        std::thread::sleep(Duration::from_millis(900));
    });
    let started = Instant::now();
    let error = expect_request_error(execute_local(
        &format!("http://{address}"),
        Duration::from_millis(300),
    ));
    assert_eq!(error.code, "push_http_failed");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the request deadline was ignored: {:?}",
        started.elapsed()
    );
    let _ = handle.join();
}

#[test]
fn tls_transport_honors_a_short_connect_deadline() {
    let started = Instant::now();
    let error = match crate::sources::gfn::push::transport::TlsPushTransportFactory::new().connect(
        "10.255.255.1",
        5228,
        Duration::from_millis(300),
    ) {
        Ok(_) => panic!("an unroutable push address was reported as connected"),
        Err(error) => error,
    };
    assert_eq!(error.code, "push_transport_failed");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the connect deadline was ignored: {:?}",
        started.elapsed()
    );
}

#[test]
fn frame_reader_accepts_the_versioned_first_frame_and_split_input() {
    let mut reader = FrameReader::new();
    reader.extend(&[MCS_VERSION, TAG_LOGIN_RESPONSE, 3, 1, 2, 3]);
    let frame = reader.next_frame(MAXIMUM_FRAME_BYTES).unwrap().unwrap();
    assert_eq!(frame.0, TAG_LOGIN_RESPONSE);
    assert_eq!(frame.1, vec![1, 2, 3]);
    let mut split = FrameReader::new();
    split.extend(&[MCS_VERSION, TAG_HEARTBEAT_PING]);
    assert!(split.next_frame(MAXIMUM_FRAME_BYTES).unwrap().is_none());
    split.extend(&[0]);
    assert_eq!(
        split.next_frame(MAXIMUM_FRAME_BYTES).unwrap().unwrap().0,
        TAG_HEARTBEAT_PING
    );
}

#[test]
fn frame_reader_rejects_unsupported_versions_and_oversized_frames() {
    let mut reader = FrameReader::new();
    reader.extend(&[7, TAG_LOGIN_RESPONSE, 0]);
    assert_eq!(
        reader.next_frame(MAXIMUM_FRAME_BYTES).unwrap_err().code,
        "push_protocol_version"
    );
    let mut oversize = FrameReader::new();
    let mut frame = vec![MCS_VERSION, TAG_DATA_MESSAGE_STANZA];
    write_varint((MAXIMUM_FRAME_BYTES + 1) as u64, &mut frame);
    oversize.extend(&frame);
    assert_eq!(
        oversize.next_frame(MAXIMUM_FRAME_BYTES).unwrap_err().code,
        "push_protocol_frame"
    );
}

#[test]
fn login_request_uses_the_documented_wire_prefix() {
    let login = encode_login_request(42, 7, &["persistent".into()]);
    assert_eq!(login[0], MCS_VERSION);
    assert_eq!(login[1], 2);
    assert_eq!(login[3], 0x0a);
    let login_text = String::from_utf8_lossy(&login);
    assert!(login_text.contains("mcs.android.com"));
    assert!(login_text.contains("android-2a"));
    assert!(login_text.contains("persistent"));
    assert!(login_text.contains("new_vc"));
    assert!(
        login.windows(2).any(|window| window == [0x80, 0x01]),
        "auth_service field is missing"
    );
}

#[test]
fn heartbeat_frames_carry_the_last_stream_id_received() {
    let ack = encode_heartbeat_ack(Some(9));
    assert_eq!(ack, vec![1, 2, 0x10, 9]);
    let ping = encode_heartbeat_ping(Some(3));
    assert_eq!(ping, vec![0, 2, 0x10, 3]);
    assert_eq!(encode_heartbeat_ack(None), vec![1, 0]);
    assert_eq!(encode_heartbeat_ping(None), vec![0, 0]);
}

#[test]
fn checkin_request_and_response_follow_the_public_contract() {
    let request = encode_checkin_request(&crate::sources::gfn::push::protocol::CheckinRequest {
        android_id: None,
        security_token: None,
    });
    assert_eq!(request[0], 0x22);
    assert!(String::from_utf8_lossy(&request).contains("63.0.3234.0"));
    let decoded = decode_checkin_response(&checkin_body(42, 7)).unwrap();
    assert_eq!(decoded.android_id, Some(42));
    assert_eq!(decoded.security_token, Some(7));
}

#[test]
fn data_message_decodes_app_data_and_payload() {
    let mut stanza = Vec::new();
    write_string(3, "from", &mut stanza);
    write_string(5, "category", &mut stanza);
    write_app_data("crypto-key", "dh=abc", &mut stanza);
    write_string(9, "persistent", &mut stanza);
    write_bytes(21, &[1, 2, 3], &mut stanza);
    let frame = server_frame(TAG_DATA_MESSAGE_STANZA, &stanza);
    let mut reader = FrameReader::new();
    reader.extend(&[MCS_VERSION]);
    reader.extend(&frame);
    let (tag, payload) = reader.next_frame(MAXIMUM_FRAME_BYTES).unwrap().unwrap();
    match decode_frame(tag, &payload).unwrap() {
        crate::sources::gfn::push::protocol::Frame::DataMessage(message) => {
            assert_eq!(message.from, "from");
            assert_eq!(message.app_data[0].0, "crypto-key");
            assert_eq!(message.persistent_id.as_deref(), Some("persistent"));
            assert_eq!(message.raw_data.as_deref(), Some(&[1_u8, 2, 3][..]));
        }
        _ => panic!("expected a data message"),
    }
}

#[test]
fn registration_flow_uses_the_documented_steps_and_never_logs_tokens() {
    let http = push_http(responses_for_registration());
    let registration = checkin_and_register(
        &RegistrationClient {
            http: http.as_ref(),
            endpoints: &endpoints(),
            timeout: Duration::from_secs(5),
        },
        &identity(),
        None,
        "gfnjwt",
        "device",
        &mut || true,
    )
    .unwrap();
    assert_eq!(registration.android_id, 42);
    assert_eq!(registration.gcm_token, "gcm-token");
    assert_eq!(registration.fcm_token, "fcm-token");
    assert_eq!(
        registration.endpoint,
        "https://push.test/fcm/send/gcm-token"
    );
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[0].0, "https://push.test/checkin");
    assert_eq!(requests[1].0, "https://push.test/c2dm");
    let c2dm = String::from_utf8_lossy(&requests[1].2).to_string();
    assert!(c2dm.contains("app=org.chromium.linux"));
    assert!(c2dm.contains("X-subtype=app"));
    assert!(c2dm.contains("device=42"));
    assert!(c2dm.contains("sender=123456789012"));
    assert_eq!(
        requests[2].0,
        "https://push.test/fis/v1/projects/project/installations"
    );
    assert_eq!(
        requests[3].0,
        "https://push.test/fcm/v1/projects/project/registrations"
    );
    assert_eq!(requests[4].0, "https://push.test/pns/v1/registrations");
    assert!(String::from_utf8_lossy(&requests[3].2).contains("fcm/send/gcm-token"));
}

#[test]
fn registration_omits_the_sender_parameter_when_unconfigured() {
    let http = push_http(responses_for_registration());
    let mut identity = identity();
    identity.sender_id = String::new();
    checkin_and_register(
        &RegistrationClient {
            http: http.as_ref(),
            endpoints: &endpoints(),
            timeout: Duration::from_secs(5),
        },
        &identity,
        None,
        "gfnjwt",
        "device",
        &mut || true,
    )
    .unwrap();
    let requests = http.requests.lock().unwrap();
    let c2dm = String::from_utf8_lossy(&requests[1].2);
    assert!(!c2dm.contains("sender="));
    assert!(c2dm.contains("X-subtype=app"));
}

#[test]
fn generated_installation_ids_follow_the_public_format() {
    for _ in 0..32 {
        let http = push_http(responses_for_registration());
        checkin_and_register(
            &RegistrationClient {
                http: http.as_ref(),
                endpoints: &endpoints(),
                timeout: Duration::from_secs(5),
            },
            &identity(),
            None,
            "gfnjwt",
            "device",
            &mut || true,
        )
        .unwrap();
        let requests = http.requests.lock().unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[2].2).unwrap();
        let fid = body["fid"].as_str().unwrap();
        assert_eq!(fid.len(), 22, "fid {fid}");
        assert!(fid.starts_with(['c', 'd', 'e', 'f']), "fid {fid}");
        assert!(
            fid.chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || character == '-'
                    || character == '_')
        );
    }
}

#[test]
fn pns_registration_validates_only_documented_statuses() {
    let http = push_http(vec![HttpResponse {
        status: 204,
        body: Vec::new(),
    }]);
    pns_register(
        &RegistrationClient {
            http: http.as_ref(),
            endpoints: &endpoints(),
            timeout: Duration::from_secs(5),
        },
        "token",
        "device",
        &[],
        "jwt",
    )
    .unwrap();
    let http = push_http(vec![HttpResponse {
        status: 500,
        body: Vec::new(),
    }]);
    assert_eq!(
        pns_register(
            &RegistrationClient {
                http: http.as_ref(),
                endpoints: &endpoints(),
                timeout: Duration::from_secs(5),
            },
            "token",
            "device",
            &[],
            "jwt",
        )
        .unwrap_err()
        .code,
        "push_pns_refused"
    );
}

#[test]
fn pns_unregistration_validates_only_documented_statuses() {
    let http = push_http(vec![HttpResponse {
        status: 204,
        body: Vec::new(),
    }]);
    crate::sources::gfn::push::registration::pns_unregister(
        &RegistrationClient {
            http: http.as_ref(),
            endpoints: &endpoints(),
            timeout: Duration::from_secs(5),
        },
        &["previous".to_owned()],
        "jwt",
    )
    .unwrap();
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[0].0, "https://push.test/pns/v1/unregister");
    let http = push_http(vec![HttpResponse {
        status: 500,
        body: Vec::new(),
    }]);
    assert_eq!(
        crate::sources::gfn::push::registration::pns_unregister(
            &RegistrationClient {
                http: http.as_ref(),
                endpoints: &endpoints(),
                timeout: Duration::from_secs(5),
            },
            &["previous".to_owned()],
            "jwt",
        )
        .unwrap_err()
        .code,
        "push_pns_refused"
    );
}

#[test]
fn production_edges_construct_without_reaching_the_network() {
    crate::sources::gfn::push::registration::ReqwestPushHttp::new().unwrap();
    let _factory = crate::sources::gfn::push::transport::TlsPushTransportFactory::new();
    let _store = crate::sources::gfn::push::store::RegistrationStore::default_service();
}

#[test]
fn owner_emits_coalesced_events_for_one_connection() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let frame_one = data_frame(
        &keys,
        &[
            ("messageType", "\"LIBRARY_CHANGE\"".into()),
            ("changedIds", "[\"app-1\"]".into()),
        ],
        "p1",
    );
    let frame_two = data_frame(
        &keys,
        &[
            ("messageType", "\"LIBRARY_CHANGE\"".into()),
            ("changedIds", "[\"app-2\"]".into()),
        ],
        "p2",
    );
    let connections = vec![vec![
        vec![MCS_VERSION],
        login_response_frame(None),
        frame_one,
        frame_two,
    ]];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let event = events_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("an event should arrive");
    match event {
        PushEvent::Library { changed_ids } => {
            assert_eq!(changed_ids, vec!["app-1".to_owned(), "app-2".to_owned()]);
        }
        other => panic!("unexpected event {other:?}"),
    }
    assert!(events_rx.recv_timeout(Duration::from_millis(80)).is_err());
    shutdown(&mut harness);
    assert_eq!(harness.factory.connects.load(Ordering::SeqCst), 1);
    assert!(
        harness
            .store
            .load(&scope().account_key())
            .unwrap()
            .is_some()
    );
}

#[test]
fn owner_accepts_aes128gcm_payloads_without_legacy_headers() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let frame = aes128gcm_data_frame(
        &keys,
        &[("messageType", "\"FAVORITES_CHANGE\"".into())],
        "p1",
    );
    let connections = vec![vec![vec![MCS_VERSION], login_response_frame(None), frame]];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let event = events_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("an aes128gcm event should arrive");
    assert!(matches!(event, PushEvent::Favorites { .. }));
    shutdown(&mut harness);
}

#[test]
fn owner_rejects_out_of_scope_events_and_stops() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let frame = data_frame(
        &keys,
        &[("messageType", "\"FAVORITES_CHANGE\"".into())],
        "p1",
    );
    let connections = vec![vec![vec![MCS_VERSION], login_response_frame(None), frame]];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        None,
        Some(existing),
    );
    harness.owner.start().unwrap();
    assert!(events_rx.recv_timeout(Duration::from_millis(120)).is_err());
    assert_eq!(harness.factory.connects.load(Ordering::SeqCst), 0);
    shutdown(&mut harness);
}

#[test]
fn owner_reconnects_after_a_failed_connection() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let frame = data_frame(
        &keys,
        &[("messageType", "\"SUBSCRIPTION_CHANGE\"".into())],
        "p1",
    );
    let connections = vec![vec![vec![MCS_VERSION], login_response_frame(None), frame]];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.factory.fail_first.store(1, Ordering::SeqCst);
    harness.owner.start().unwrap();
    let event = events_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("an event should arrive after reconnecting");
    assert!(matches!(event, PushEvent::Subscription { .. }));
    shutdown(&mut harness);
    assert!(harness.factory.connects.load(Ordering::SeqCst) >= 2);
}

#[test]
fn owner_registers_when_no_registration_is_stored() {
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let http = push_http(responses_for_registration());
    let mut harness = owner_harness(
        http.clone(),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        None,
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .is_none()
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("registration should persist");
    assert_eq!(stored.fcm_token, "fcm-token");
    assert!(!stored.fingerprint.is_empty());
    assert_eq!(http.requests.lock().unwrap().len(), 5);
    shutdown(&mut harness);
}

#[test]
fn owner_replaces_an_expired_registration() {
    let http = push_http(responses_for_registration());
    let mut expired = registration();
    expired.created_at_seconds = 1;
    expired.fingerprint = fingerprint_for(&scope());
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        http.clone(),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(expired),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(http.requests.lock().unwrap().len(), 5);
    shutdown(&mut harness);
}

#[test]
fn owner_never_reuses_another_account_registration() {
    let http = push_http(responses_for_registration());
    let mut first = registration();
    first.created_at_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    first.fingerprint = fingerprint_for(&scope());
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        http.clone(),
        vec![session_chunks(Vec::new()), session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(first),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(http.requests.lock().unwrap().len(), 0);
    let second = PushScope {
        user_id: "user-2".into(),
        provider_id: "nvidia".into(),
        generation: 8,
    };
    *harness.desired.lock().unwrap() = Some(second.clone());
    let deadline = Instant::now() + Duration::from_secs(3);
    while http.requests.lock().unwrap().len() < 5 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let requests = http.requests.lock().unwrap().len();
    assert_eq!(
        requests, 5,
        "the second account must register its own token"
    );
    assert!(harness.store.load(&second.account_key()).unwrap().is_some());
    assert!(events_rx.recv_timeout(Duration::from_millis(50)).is_err());
    shutdown(&mut harness);
}

#[test]
fn the_registration_fingerprint_covers_the_registration_identity() {
    let base = PushOwnerConfig::bounded(endpoints(), identity(), "nvidia".into(), "device".into());
    let baseline = base.registration_fingerprint(&scope());
    let mut changes: Vec<(&str, PushOwnerConfig)> = Vec::new();
    let mut rotated = base.clone();
    rotated.identity.project_id = "other-project".into();
    changes.push(("project", rotated));
    let mut rotated = base.clone();
    rotated.identity.api_key = "other-key".into();
    changes.push(("api key", rotated));
    let mut rotated = base.clone();
    rotated.identity.sender_id = "999999999999".into();
    changes.push(("sender", rotated));
    let mut rotated = base.clone();
    rotated.identity.app_id = "other-app".into();
    changes.push(("application", rotated));
    let mut rotated = base.clone();
    rotated.identity.firebase_app_id = "1:2:web:other".into();
    changes.push(("firebase application", rotated));
    let mut rotated = base.clone();
    rotated.identity.vapid_key = Some("rotated-vapid".into());
    changes.push(("vapid key", rotated));
    let mut rotated = base.clone();
    rotated.endpoints.pns_client_id = "other-client".into();
    changes.push(("client id", rotated));
    let mut rotated = base.clone();
    rotated.device_id = "other-device".into();
    changes.push(("device", rotated));
    for (field, rotated) in changes {
        assert_ne!(
            baseline,
            rotated.registration_fingerprint(&scope()),
            "a changed {field} must invalidate the stored registration"
        );
    }
    assert_eq!(
        baseline,
        base.registration_fingerprint(&scope()),
        "the fingerprint must stay stable for one configuration"
    );
}

#[test]
fn owner_rapid_scope_changes_keep_one_worker_and_one_connection() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration();
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fingerprint = fingerprint_for(&scope());
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let connections: Vec<Vec<Vec<u8>>> = (0..40).map(|_| session_chunks(Vec::new())).collect();
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    for _ in 0..40 {
        *harness.desired.lock().unwrap() = None;
        *harness.desired.lock().unwrap() = Some(scope());
        assert_eq!(harness.owner.worker_count(), 1);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    shutdown(&mut harness);
    assert_eq!(harness.factory.active.load(Ordering::SeqCst), 0);
    assert!(harness.factory.maximum_active.load(Ordering::SeqCst) <= 1);
}

#[test]
fn owner_cancels_a_blocked_connect_within_its_slice() {
    let released = Arc::new(Mutex::new(false));
    let wake = Arc::new(Condvar::new());
    let connects = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let desired = Arc::new(Mutex::new(Some(scope())));
    let source_scope = Arc::clone(&desired);
    let mut owner = PushOwner::new(
        PushOwnerConfig {
            connect_timeout: Duration::from_millis(150),
            read_timeout: Duration::from_millis(20),
            login_timeout: Duration::from_secs(2),
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_ack_timeout: Duration::from_millis(500),
            coalescing_window: Duration::from_millis(30),
            backoff_initial: Duration::from_millis(10),
            backoff_maximum: Duration::from_millis(40),
            stop_deadline: Duration::from_millis(300),
            ..PushOwnerConfig::bounded(endpoints(), identity(), "nvidia".into(), "device".into())
        },
        PushOwnerDeps {
            http: push_http(Vec::new()),
            transport: Arc::new(BlockingFactory {
                released: Mutex::new(false),
                wake: Condvar::new(),
                connects: Arc::clone(&connects),
                connections: Mutex::new(vec![session_chunks(Vec::new())]),
                sent: Arc::new(Mutex::new(Vec::new())),
                active: Arc::clone(&active),
                maximum_active: Arc::clone(&maximum_active),
            }),
            store: MemoryStore::new(Some(registration_with_current_age(&scope()))),
            sink: Arc::new(|_: PushEvent, _: u64| {}),
            scope_source: Arc::new(move || source_scope.lock().unwrap().clone()),
            token_source: Arc::new(|_: &PushScope| Some("gfnjwt".to_owned())),
        },
    );
    owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        connects.load(Ordering::SeqCst) >= 1,
        "the connect must be attempted"
    );
    assert!(owner.worker_count() <= 1);
    *desired.lock().unwrap() = None;
    *released.lock().unwrap() = true;
    wake.notify_all();
    assert!(owner.shutdown(Duration::from_secs(3)));
    assert_eq!(owner.worker_count(), 0);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(maximum_active.load(Ordering::SeqCst) <= 1);
}
#[test]
fn changed_ids_stay_bounded() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let ids: Vec<String> = (0..120).map(|index| format!("\"app-{index}\"")).collect();
    let frame = data_frame(
        &keys,
        &[
            ("messageType", "\"LIBRARY_CHANGE\"".into()),
            ("changedIds", format!("[{}]", ids.join(","))),
        ],
        "p1",
    );
    let connections = vec![vec![vec![MCS_VERSION], login_response_frame(None), frame]];
    let (events_tx, events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let event = events_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("an event should arrive");
    match event {
        PushEvent::Library { changed_ids } => {
            assert_eq!(changed_ids.len(), MAXIMUM_CHANGED_IDS);
        }
        other => panic!("unexpected event {other:?}"),
    }
    shutdown(&mut harness);
}

#[test]
fn encode_stream_ack_matches_the_documented_mcs_wire_shape() {
    let mut extension = Vec::new();
    write_tag(1, 0, &mut extension);
    write_varint(STREAM_ACK_EXTENSION_ID, &mut extension);
    write_tag(2, 2, &mut extension);
    write_varint(0, &mut extension);
    let mut payload = Vec::new();
    write_tag(2, 0, &mut payload);
    write_varint(1, &mut payload);
    write_tag(3, 2, &mut payload);
    write_varint(0, &mut payload);
    write_tag(7, 2, &mut payload);
    write_varint(extension.len() as u64, &mut payload);
    payload.extend_from_slice(&extension);
    write_tag(10, 0, &mut payload);
    write_varint(6, &mut payload);
    let mut expected = vec![TAG_IQ_STANZA];
    write_varint(payload.len() as u64, &mut expected);
    expected.extend_from_slice(&payload);
    assert_eq!(encode_stream_ack(Some(6)), expected);
    let mut payload_without_stream_id = payload.clone();
    payload_without_stream_id.truncate(payload_without_stream_id.len() - 2);
    let mut without_stream_id = vec![TAG_IQ_STANZA];
    write_varint(
        payload_without_stream_id.len() as u64,
        &mut without_stream_id,
    );
    without_stream_id.extend_from_slice(&payload_without_stream_id);
    assert_eq!(encode_stream_ack(None), without_stream_id);
}

#[test]
fn the_session_advertises_every_received_stream_id_and_acknowledges_pings() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut chunks = session_chunks(Vec::new());
    chunks.push(server_frame(TAG_HEARTBEAT_ACK, &[]));
    chunks.push(server_frame(5, &[]));
    chunks.push(server_frame(TAG_IQ_STANZA, &[]));
    chunks.push(data_frame(
        &keys,
        &[("messageType", "\"LIBRARY_CHANGE\"".into())],
        "",
    ));
    chunks.push(server_frame(TAG_HEARTBEAT_PING, &[]));
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![chunks],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.sent.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let sent = harness.factory.sent.lock().unwrap().clone();
    let ack = sent
        .iter()
        .find(|frame| frame.first() == Some(&TAG_HEARTBEAT_ACK))
        .expect("the inbound ping must be acknowledged");
    assert_eq!(
        ack,
        &vec![TAG_HEARTBEAT_ACK, 2, 0x10, 6],
        "the acknowledgement must advertise every received stream id"
    );
    shutdown(&mut harness);
}

#[test]
fn a_burst_of_persistent_messages_triggers_one_stream_ack() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut chunks = session_chunks(Vec::new());
    for index in 0..10 {
        chunks.push(data_frame(
            &keys,
            &[("messageType", "\"LIBRARY_CHANGE\"".into())],
            &format!("p{index}"),
        ));
    }
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![chunks],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.sent.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let sent = harness.factory.sent.lock().unwrap().clone();
    let acks: Vec<&Vec<u8>> = sent
        .iter()
        .filter(|frame| frame.first() == Some(&TAG_IQ_STANZA))
        .collect();
    assert_eq!(
        acks.len(),
        1,
        "a burst of ten persistent messages must produce exactly one stream acknowledgement"
    );
    shutdown(&mut harness);
}

#[test]
fn an_immediate_ack_message_is_acknowledged_immediately() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let immediate = server_frame(
        TAG_DATA_MESSAGE_STANZA,
        &data_stanza(
            &keys,
            &[("messageType", "\"LIBRARY_CHANGE\"".into())],
            "p-immediate",
            true,
        ),
    );
    let mut chunks = session_chunks(Vec::new());
    chunks.push(immediate);
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![chunks],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.sent.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let sent = harness.factory.sent.lock().unwrap().clone();
    assert!(
        sent.iter()
            .any(|frame| frame.first() == Some(&TAG_IQ_STANZA)),
        "an immediate-ack message must be acknowledged without waiting for the burst threshold"
    );
    shutdown(&mut harness);
}

#[test]
fn a_silent_connection_reconnects_without_replacing_the_registration() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    existing.fcm_token = "kept-fcm-token".into();
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new()), session_chunks(Vec::new())],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while harness.factory.connects.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        harness.factory.connects.load(Ordering::SeqCst) >= 2,
        "a silent connection must be re-established after the heartbeat deadline"
    );
    assert_eq!(
        checkin_requests(&harness),
        0,
        "a heartbeat timeout must not re-register the device identity"
    );
    let stored = harness
        .store
        .load(&scope().account_key())
        .unwrap()
        .expect("the registration must survive a heartbeat timeout");
    assert_eq!(stored.fcm_token, "kept-fcm-token");
    shutdown(&mut harness);
}

#[test]
fn resumed_traffic_clears_the_heartbeat_deadline() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut chunks = session_chunks(Vec::new());
    chunks.push(server_frame(TAG_HEARTBEAT_PING, &[]));
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![chunks],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.sent.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.factory.connects.load(Ordering::SeqCst), 1);
    assert_eq!(checkin_requests(&harness), 0);
    shutdown(&mut harness);
}

#[test]
fn the_negotiated_heartbeat_interval_is_clamped_and_honored() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut chunks = vec![vec![MCS_VERSION], login_response_frame(Some(50))];
    chunks.push(server_frame(TAG_HEARTBEAT_PING, &[]));
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![chunks],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !harness
        .factory
        .sent
        .lock()
        .unwrap()
        .iter()
        .any(|frame| frame.first() == Some(&TAG_HEARTBEAT_PING))
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let sent = harness.factory.sent.lock().unwrap().clone();
    assert!(
        sent.iter()
            .any(|frame| frame.first() == Some(&TAG_HEARTBEAT_PING)),
        "the negotiated interval must still produce a keep-alive"
    );
    let heartbeat = *harness.factory.heartbeat.lock().unwrap();
    let ping = heartbeat
        .ping
        .expect("the keep-alive must be observed with its preceding receive");
    let elapsed = ping.sent.saturating_duration_since(ping.last_received);
    assert!(
        elapsed >= Duration::from_millis(100),
        "an out-of-range negotiated interval must be clamped to the configured bound: {elapsed:?}"
    );
    shutdown(&mut harness);
}

#[test]
fn a_missing_login_response_bounds_the_session() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![vec![vec![MCS_VERSION]], session_chunks(Vec::new())],
        Arc::new(|_: PushEvent, _: u64| {}),
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while harness.factory.connects.load(Ordering::SeqCst) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        harness.factory.connects.load(Ordering::SeqCst),
        2,
        "a session without a login response must be bounded and retried"
    );
    assert_eq!(checkin_requests(&harness), 0);
    shutdown(&mut harness);
}

#[test]
fn heartbeat_ping_triggers_an_acknowledgement() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let connections = vec![vec![
        vec![MCS_VERSION],
        server_frame(TAG_HEARTBEAT_PING, &[]),
    ]];
    let sent = Arc::new(Mutex::new(Vec::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let factory = Arc::new(ScriptedFactory {
        connections: Mutex::new(connections),
        sent: Arc::clone(&sent),
        connects: Arc::new(AtomicUsize::new(0)),
        fail_first: AtomicUsize::new(0),
        active,
        maximum_active,
        spacing_millis: Arc::new(AtomicUsize::new(0)),
        heartbeat: Arc::new(Mutex::new(HeartbeatTiming::default())),
    });
    let store = MemoryStore::new(Some(existing));
    let desired = Arc::new(Mutex::new(Some(scope())));
    let token_scope = Arc::clone(&desired);
    let token_source = Arc::new(move |scope: &PushScope| {
        let current = token_scope.lock().unwrap().clone()?;
        (current.account_key() == scope.account_key()).then(|| "gfnjwt".to_owned())
    });
    let source_scope = Arc::clone(&desired);
    let sink = Arc::new(|_: PushEvent, _: u64| {});
    let config = PushOwnerConfig {
        connect_timeout: Duration::from_secs(1),
        read_timeout: Duration::from_millis(20),
        login_timeout: Duration::from_secs(2),
        heartbeat_interval: Duration::from_secs(30),
        heartbeat_ack_timeout: Duration::from_millis(500),
        minimum_heartbeat_interval: Duration::from_millis(100),
        maximum_heartbeat_interval: Duration::from_secs(2),
        coalescing_window: Duration::from_millis(30),
        backoff_initial: Duration::from_millis(10),
        backoff_maximum: Duration::from_millis(40),
        stop_deadline: Duration::from_secs(2),
        ..PushOwnerConfig::bounded(endpoints(), identity(), "nvidia".into(), "device".into())
    };
    let mut owner = PushOwner::new(
        config,
        PushOwnerDeps {
            http: push_http(Vec::new()),
            transport: factory,
            store,
            sink,
            scope_source: Arc::new(move || source_scope.lock().unwrap().clone()),
            token_source,
        },
    );
    owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while sent.lock().unwrap().len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let frames = sent.lock().unwrap().clone();
    let ack = frames.last().expect("an acknowledgement should be sent");
    assert_eq!(ack, &vec![1, 2, 0x10, 1]);
    *desired.lock().unwrap() = None;
    let deadline = Instant::now() + Duration::from_secs(2);
    while owner.is_running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!owner.is_running());
    assert!(owner.shutdown(Duration::from_secs(2)));
    assert_eq!(owner.worker_count(), 0);
}

#[test]
fn message_types_map_only_to_the_supported_invalidation_events() {
    let expected: Vec<(&str, bool)> = vec![
        ("FAVORITES_CHANGE", true),
        ("LIBRARY_CHANGE", true),
        ("LINKEDACCOUNT_CHANGE", true),
        ("SUBSCRIPTION_CHANGE", true),
        ("PLATFORM_SYNC_CHANGE", true),
        ("SESSION_CHANGE", false),
        ("CAMPAIGN_CHANGE", false),
        ("KV_STORE_CHANGE", false),
    ];
    let mut seen = 0;
    for (name, supported) in expected {
        let mapped = matches!(
            crate::sources::gfn::push::MessageType::parse(name),
            Some(
                crate::sources::gfn::push::MessageType::LibraryChange
                    | crate::sources::gfn::push::MessageType::FavoritesChange
                    | crate::sources::gfn::push::MessageType::SubscriptionChange
                    | crate::sources::gfn::push::MessageType::LinkedAccountChange
                    | crate::sources::gfn::push::MessageType::PlatformSyncChange
            )
        );
        assert_eq!(mapped, supported, "{name}");
        seen += 1;
    }
    assert_eq!(seen, 8);
}

#[test]
fn stale_generation_cancels_the_worker_between_reconnects() {
    let keys = EceKeyPair::generate().unwrap();
    let mut existing = registration_with_current_age(&scope());
    existing.private_key = keys.private_key();
    existing.public_key = keys.public_key().to_vec();
    existing.auth_secret = keys.auth_secret().to_vec();
    let connections = vec![session_chunks(Vec::new())];
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        connections,
        sink,
        Some(scope()),
        Some(existing),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    shutdown(&mut harness);
}

#[test]
fn pausing_an_owner_keeps_one_worker_and_resumes_cleanly() {
    let (events_tx, _events_rx) = mpsc::channel();
    let sink = Arc::new(move |event: PushEvent, _generation: u64| {
        let _ = events_tx.send(event);
    });
    let mut harness = owner_harness(
        push_http(Vec::new()),
        vec![session_chunks(Vec::new())],
        sink,
        Some(scope()),
        Some(registration_with_current_age(&scope())),
    );
    harness.owner.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while harness.factory.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.owner.stop();
    assert!(harness.owner.wait_idle(Duration::from_secs(2)));
    assert_eq!(
        harness.owner.worker_count(),
        1,
        "pausing must keep the single worker"
    );
    harness.owner.start().unwrap();
    assert_eq!(
        harness.owner.worker_count(),
        1,
        "resuming must never add a worker"
    );
    assert!(harness.factory.maximum_active.load(Ordering::SeqCst) <= 1);
    shutdown(&mut harness);
}

fn registration_with_current_age(scope: &PushScope) -> Registration {
    let mut registration = registration();
    registration.created_at_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    registration.fingerprint = fingerprint_for(scope);
    registration
}

fn decode(value: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD
        .decode(value.trim().replace(char::is_whitespace, ""))
        .unwrap()
}
