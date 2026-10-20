//! Social account, friends, presence and co-op lobby client (`social.v1`).
//!
//! The core owns the social token: it is stored with the same rules as GFN
//! sessions, attached to backend requests, and never returned to the shell or
//! written to diagnostics. One WebSocket thread keeps the backend connection,
//! and one job thread refreshes data the socket announces.

mod backend;
mod socket;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use crate::credential_vault::TokenVault;
use backend::{Backend, Call, Failure};
use reqwest::Method;
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::fmt;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

/// Social backend used while the `socialBackendUrl` setting is empty: the
/// production Railway deployment. Saved settings keep `""`, so changing this
/// constant moves every install that did not choose its own backend.
pub const DEFAULT_SOCIAL_BACKEND_URL: &str = "https://opennow-social-production.up.railway.app";

pub const METHODS: &[&str] = &[
    "social.state.get",
    "social.link.start",
    "social.link.cancel",
    "social.unlink",
    "social.settings.update",
    "social.friends.get",
    "social.users.lookup",
    "social.friends.request",
    "social.friends.accept",
    "social.friends.decline",
    "social.friends.cancel",
    "social.friends.remove",
    "social.friends.block",
    "social.friends.unblock",
    "social.presence.set",
    "coop.iceServers.get",
    "coop.lobby.create",
    "coop.lobby.update",
    "coop.lobby.regenerateCode",
    "coop.lobby.close",
    "coop.lobby.leave",
    "coop.report",
    "coop.lobby.invite",
    "coop.invite.cancel",
    "coop.invite.accept",
    "coop.invite.decline",
    "coop.invites.get",
    "coop.join.code",
    "coop.join.ask",
    "coop.join.approved",
    "coop.joinRequest.respond",
    "coop.member.kick",
    "coop.member.slot",
    "coop.signal.send",
];

const TOKEN_SERVICE: &str = "app.opennow.social";
const TOKEN_FALLBACK_DIR: &str = "fallback-social";
const ACCOUNT_KEY: &str = "discord-account";
const MAXIMUM_OUTBOUND_FRAMES: usize = 256;
const MAXIMUM_SIGNAL_PAYLOAD_BYTES: usize = 16 * 1024;
const MAXIMUM_ID_BYTES: usize = 128;
const MAXIMUM_BACKEND_URL_BYTES: usize = 2_048;
const GUEST_TOKEN_LIFETIME: Duration = Duration::from_secs(23 * 60 * 60 + 30 * 60);
const ICE_REFRESH_MARGIN_SECONDS: u64 = 10 * 60;
const DEFAULT_LINK_LIFETIME: Duration = Duration::from_secs(10 * 60);
const MAXIMUM_LINK_LIFETIME: Duration = Duration::from_secs(15 * 60);
const ACTIVITIES: &[&str] = &[
    "online",
    "idle",
    "browsing_library",
    "browsing_store",
    "in_settings",
    "in_queue",
    "playing",
    "in_lobby",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialError {
    pub code: String,
    pub message: String,
}

impl SocialError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn not_linked() -> Self {
        Self::new(
            "social_not_linked",
            "Link a Discord account in OpenNOW to use Friends and co-op",
        )
    }

    fn offline() -> Self {
        Self::new(
            "social_offline",
            "OpenNOW is not connected to the social service right now",
        )
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_params", message)
    }
}

/// A bearer token. It is deliberately not `Debug`-printable or serializable.
#[derive(Clone, PartialEq, Eq)]
struct Token(String);

impl Token {
    fn parse(value: &Value) -> Option<Self> {
        let text = value.as_str()?;
        (!text.is_empty() && text.len() <= 512 && text.bytes().all(|byte| byte.is_ascii_graphic()))
            .then(|| Self(text.to_owned()))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token(<redacted>)")
    }
}

#[derive(Clone, PartialEq)]
struct Config {
    base: Url,
    proxy: Value,
}

impl Config {
    fn from_settings(settings: &Value) -> Self {
        let configured = settings["socialBackendUrl"]
            .as_str()
            .and_then(|raw| normalize_backend_url(raw).ok())
            .filter(|url| !url.is_empty());
        let base = configured
            .and_then(|url| Url::parse(&url).ok())
            .unwrap_or_else(|| {
                Url::parse(DEFAULT_SOCIAL_BACKEND_URL).expect("default URL is valid")
            });
        Self {
            base,
            proxy: json!({
                "sessionProxyEnabled": settings["sessionProxyEnabled"],
                "sessionProxyUrl": settings["sessionProxyUrl"],
            }),
        }
    }
}

/// Validates the `socialBackendUrl` setting. Empty selects the built-in
/// backend. Otherwise the value must be an https origin; http is accepted only
/// for a loopback host so a local backend can be used during development.
pub fn normalize_backend_url(raw: &str) -> Result<String, String> {
    const INVALID: &str = "socialBackendUrl must be an https:// origin such as https://social.example.com (http:// is allowed only for localhost)";
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > MAXIMUM_BACKEND_URL_BYTES {
        return Err(INVALID.to_owned());
    }
    let url = Url::parse(trimmed).map_err(|_| INVALID.to_owned())?;
    let loopback = match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    let scheme_allowed = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    if !scheme_allowed
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(INVALID.to_owned());
    }
    Ok(url.origin().ascii_serialization())
}

#[derive(Clone, Copy, Debug)]
struct Timing {
    ping: Duration,
    silence: Duration,
    auth: Duration,
    connect: Duration,
    presence: Duration,
    backoff_base: Duration,
    backoff_max: Duration,
    tick: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            ping: Duration::from_secs(20),
            silence: Duration::from_secs(60),
            auth: Duration::from_secs(5),
            connect: Duration::from_secs(10),
            presence: Duration::from_secs(1),
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            tick: Duration::from_millis(100),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Connection {
    Unlinked,
    Offline,
    Connecting,
    Connected,
}

impl Connection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unlinked => "unlinked",
            Self::Offline => "offline",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
        }
    }
}

struct Account {
    token: Token,
    user: Value,
    settings: Value,
}

struct Guest {
    token: Token,
    id: String,
    display_name: String,
    expires: Instant,
}

struct LinkAttempt {
    login_id: String,
    poll_secret: Token,
    deadline: Instant,
    interval: Duration,
    committing: bool,
}

#[derive(Default)]
struct Presence {
    latest: Option<String>,
    dirty: bool,
    last_sent: Option<Instant>,
}

impl Presence {
    fn take_due(&mut self, now: Instant, interval: Duration) -> Option<String> {
        let due = self.dirty
            && self
                .last_sent
                .is_none_or(|sent| now.duration_since(sent) >= interval);
        if !due {
            return None;
        }
        self.dirty = false;
        self.last_sent = Some(now);
        self.latest.clone()
    }
}

struct IceCache {
    generation: u64,
    expires: Instant,
    value: Value,
}

#[derive(Default)]
struct Jobs {
    account: bool,
    friends: bool,
}

struct State {
    config: Config,
    account: Option<Account>,
    guest: Option<Guest>,
    connection: Connection,
    generation: u64,
    link: Option<LinkAttempt>,
    presence: Presence,
    outbound: VecDeque<String>,
    ice: Option<IceCache>,
    jobs: Jobs,
    shutdown: bool,
}

impl State {
    fn socket_token(&self) -> Option<(Token, bool)> {
        if let Some(account) = &self.account {
            return Some((account.token.clone(), true));
        }
        self.guest
            .as_ref()
            .filter(|guest| guest.expires > Instant::now())
            .map(|guest| (guest.token.clone(), false))
    }

    fn snapshot(&self) -> Value {
        json!({
            "connection": self.connection.as_str(),
            "account": self.account.as_ref().map(|account| json!({
                "linked": true,
                "user": account.user,
                "settings": account.settings,
            })),
            "guest": self.guest.as_ref().map(|guest| json!({
                "id": guest.id,
                "displayName": guest.display_name,
            })),
        })
    }
}

#[derive(Clone, Copy)]
enum Auth {
    Account,
    Lobby,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    output: Sender<Value>,
    vault: TokenVault,
    account_key: String,
    backend: Backend,
    timing: Timing,
}

pub struct SocialService {
    shared: Arc<Shared>,
}

impl SocialService {
    pub fn new(data_dir: &Path, output: Sender<Value>, settings: &Value) -> Result<Self, String> {
        Self::start(
            TokenVault::new(data_dir, TOKEN_SERVICE, TOKEN_FALLBACK_DIR),
            account_key(data_dir),
            output,
            settings,
            Timing::default(),
        )
    }

    fn start(
        vault: TokenVault,
        account_key: String,
        output: Sender<Value>,
        settings: &Value,
        timing: Timing,
    ) -> Result<Self, String> {
        let account = match vault.load(&account_key) {
            Ok(Some(encoded)) => decode_account(&encoded),
            Ok(None) => None,
            Err(_) => {
                eprintln!("social: the saved account token could not be read");
                None
            }
        };
        let linked = account.is_some();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                config: Config::from_settings(settings),
                account,
                guest: None,
                connection: if linked {
                    Connection::Connecting
                } else {
                    Connection::Unlinked
                },
                generation: 0,
                link: None,
                presence: Presence::default(),
                outbound: VecDeque::new(),
                ice: None,
                jobs: Jobs::default(),
                shutdown: false,
            }),
            wake: Condvar::new(),
            output,
            vault,
            account_key,
            backend: Backend::new()?,
            timing,
        });
        let socket_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("opennow-social-ws".to_owned())
            .spawn(move || socket::run(&socket_shared))
            .map_err(|error| error.to_string())?;
        let job_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("opennow-social-jobs".to_owned())
            .spawn(move || run_jobs(&job_shared))
            .map_err(|error| error.to_string())?;
        Ok(Self { shared })
    }

    /// Applies `socialBackendUrl` and proxy settings; a change reconnects.
    pub fn configure(&self, settings: &Value) {
        let config = Config::from_settings(settings);
        let mut state = self.shared.lock();
        if state.config != config {
            state.config = config;
            self.shared.invalidate_connection(&mut state);
        }
    }

    pub fn handle(&self, method: &str, params: &Value) -> Result<Value, SocialError> {
        let shared = &self.shared;
        let id = |key: &str| id_param(params, key);
        match method {
            "social.state.get" => Ok(shared.lock().snapshot()),
            "social.link.start" => link_start(shared),
            "social.link.cancel" => {
                shared.link_cancel(id("loginId")?);
                Ok(json!({}))
            }
            "social.unlink" => shared.unlink(),
            "social.settings.update" => {
                let generation = shared.lock().generation;
                let result = shared.call(
                    Auth::Account,
                    Call::new(
                        Method::PATCH,
                        &["v1", "me", "settings"],
                        Some(settings_patch(params)?),
                    ),
                )?;
                let mut state = shared.lock();
                if state.generation == generation
                    && let Some(account) = &mut state.account
                    && result["settings"].is_object()
                {
                    account.settings = result["settings"].clone();
                    shared.emit_account(&state, None);
                }
                Ok(result)
            }
            "social.friends.get" => shared.call(Auth::Account, Call::get(&["v1", "friends"])),
            "social.users.lookup" => {
                let username = username_param(params)?;
                let path = ["v1", "users", "lookup"];
                let mut call = Call::get(&path);
                call.query = Some(("username", username));
                shared.call(Auth::Account, call)
            }
            "social.friends.request" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "friends", "requests"],
                    json!({"username": username_param(params)?}),
                ),
            ),
            "social.friends.accept" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "friends", "requests", id("requestId")?, "accept"],
                    json!({}),
                ),
            ),
            "social.friends.decline" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "friends", "requests", id("requestId")?, "decline"],
                    json!({}),
                ),
            ),
            "social.friends.cancel" => shared.call(
                Auth::Account,
                Call::delete(&["v1", "friends", "requests", id("requestId")?]),
            ),
            "social.friends.remove" => shared.call(
                Auth::Account,
                Call::delete(&["v1", "friends", id("userId")?]),
            ),
            "social.friends.block" => shared.call(
                Auth::Account,
                Call::post(&["v1", "blocks"], json!({"userId": id("userId")?})),
            ),
            "social.friends.unblock" => shared.call(
                Auth::Account,
                Call::delete(&["v1", "blocks", id("userId")?]),
            ),
            "social.presence.set" => {
                let frame = presence_frame(params)?;
                let mut state = shared.lock();
                state.presence.latest = Some(frame);
                state.presence.dirty = true;
                Ok(json!({}))
            }
            "coop.iceServers.get" => shared.ice_servers(),
            "coop.lobby.create" => {
                let body = json!({
                    "game": game_param(&params["game"])?,
                    "maxGuests": max_guests_param(&params["maxGuests"])?,
                });
                shared.call(Auth::Account, Call::post(&["v1", "lobbies"], body))
            }
            "coop.lobby.update" => {
                let mut body = Map::new();
                match &params["state"] {
                    Value::Null => {}
                    Value::String(state) if state == "streaming" => {
                        body.insert("state".to_owned(), json!("streaming"));
                    }
                    _ => return Err(SocialError::invalid("state may only be \"streaming\"")),
                }
                if !params["maxGuests"].is_null() {
                    body.insert(
                        "maxGuests".to_owned(),
                        json!(max_guests_param(&params["maxGuests"])?),
                    );
                }
                if body.is_empty() {
                    return Err(SocialError::invalid(
                        "coop.lobby.update requires state or maxGuests",
                    ));
                }
                shared.call(
                    Auth::Account,
                    Call::new(
                        Method::PATCH,
                        &["v1", "lobbies", id("lobbyId")?],
                        Some(Value::Object(body)),
                    ),
                )
            }
            "coop.lobby.regenerateCode" => shared.call(
                Auth::Account,
                Call::post(&["v1", "lobbies", id("lobbyId")?, "code"], json!({})),
            ),
            "coop.lobby.close" => shared.call(
                Auth::Account,
                Call::delete(&["v1", "lobbies", id("lobbyId")?]),
            ),
            "coop.lobby.leave" => shared.call(
                Auth::Lobby,
                Call::post(&["v1", "lobbies", id("lobbyId")?, "leave"], json!({})),
            ),
            "coop.report" => shared.call(
                Auth::Lobby,
                Call::post(
                    &["v1", "lobbies", report_id(params, "lobbyId")?, "reports"],
                    report_body(params)?,
                ),
            ),
            "coop.lobby.invite" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "lobbies", id("lobbyId")?, "invites"],
                    json!({"userId": id("userId")?}),
                ),
            ),
            "coop.invite.cancel" => shared.call(
                Auth::Account,
                Call::delete(&["v1", "invites", id("inviteId")?]),
            ),
            "coop.invite.accept" => shared.call(
                Auth::Account,
                Call::post(&["v1", "invites", id("inviteId")?, "accept"], json!({})),
            ),
            "coop.invite.decline" => shared.call(
                Auth::Account,
                Call::post(&["v1", "invites", id("inviteId")?, "decline"], json!({})),
            ),
            "coop.invites.get" => shared.call(Auth::Account, Call::get(&["v1", "invites"])),
            "coop.join.code" => {
                let code = code_param(params)?;
                let display_name = guest_name_param(params)?;
                if shared.lock().account.is_none() {
                    shared.ensure_guest(&display_name)?;
                }
                shared.call(
                    Auth::Lobby,
                    Call::post(&["v1", "lobbies", "join"], json!({"code": code})),
                )
            }
            "coop.join.ask" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "lobbies", id("lobbyId")?, "join-requests"],
                    json!({}),
                ),
            ),
            "coop.join.approved" => shared.call(
                Auth::Account,
                Call::post(
                    &["v1", "join-requests", id("requestId")?, "join"],
                    json!({}),
                ),
            ),
            "coop.joinRequest.respond" => {
                let approve = params["approve"]
                    .as_bool()
                    .ok_or_else(|| SocialError::invalid("approve must be a boolean"))?;
                let action = if approve { "approve" } else { "decline" };
                shared.call(
                    Auth::Account,
                    Call::post(
                        &["v1", "join-requests", id("requestId")?, action],
                        json!({}),
                    ),
                )
            }
            "coop.member.kick" => shared.call(
                Auth::Account,
                Call::post(
                    &[
                        "v1",
                        "lobbies",
                        id("lobbyId")?,
                        "members",
                        id("userId")?,
                        "kick",
                    ],
                    json!({}),
                ),
            ),
            "coop.member.slot" => {
                let slot = params["slot"]
                    .as_u64()
                    .filter(|slot| (1..=3).contains(slot))
                    .ok_or_else(|| SocialError::invalid("slot must be 1, 2 or 3"))?;
                shared.call(
                    Auth::Account,
                    Call::post(
                        &[
                            "v1",
                            "lobbies",
                            id("lobbyId")?,
                            "members",
                            id("userId")?,
                            "slot",
                        ],
                        json!({"slot": slot}),
                    ),
                )
            }
            "coop.signal.send" => shared.send_signal(params),
            _ => Err(SocialError::new(
                "method_not_found",
                format!("Unknown core method: {method}"),
            )),
        }
    }
}

impl Drop for SocialService {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.shutdown = true;
        state.link = None;
        self.shared.wake.notify_all();
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("social state poisoned")
    }

    fn emit(&self, name: &str, payload: Value) {
        let _ = self
            .output
            .send(json!({"type":"event", "name":name, "payload":payload}));
    }

    fn emit_account(&self, state: &State, link: Option<(&str, &str)>) {
        let mut payload = state.snapshot();
        if let Some((login_id, status)) = link {
            payload["link"] = json!({"loginId": login_id, "status": status});
        } else if let Some(attempt) = &state.link {
            payload["link"] = json!({"loginId": attempt.login_id, "status": "pending"});
        }
        self.emit("social.account.changed", payload);
    }

    fn set_connection(&self, state: &mut State, connection: Connection) {
        if state.connection != connection {
            state.connection = connection;
            self.emit(
                "social.connection.changed",
                json!({"state": connection.as_str()}),
            );
        }
    }

    /// Ends the current socket generation: queued frames and cached ICE
    /// credentials belong to the old identity or backend.
    fn invalidate_connection(&self, state: &mut State) {
        state.generation += 1;
        state.ice = None;
        state.outbound.clear();
        self.wake.notify_all();
    }

    fn tokens_changed(&self, state: &mut State, link: Option<(&str, &str)>) {
        self.invalidate_connection(state);
        if state.account.is_none() && state.socket_token().is_none() {
            self.set_connection(state, Connection::Unlinked);
        }
        self.emit_account(state, link);
    }

    fn call(&self, auth: Auth, call: Call<'_>) -> Result<Value, SocialError> {
        let (config, token, guest) = {
            let state = self.lock();
            let (token, guest) = match (auth, &state.account, &state.guest) {
                (_, Some(account), _) => (account.token.clone(), false),
                (Auth::Lobby, None, Some(guest)) if guest.expires > Instant::now() => {
                    (guest.token.clone(), true)
                }
                _ => return Err(SocialError::not_linked()),
            };
            (state.config.clone(), token, guest)
        };
        match self.backend.send(&config, call, Some(&token)) {
            Ok(value) => Ok(value),
            Err(Failure {
                status: Some(401), ..
            }) => {
                self.token_rejected(&token, guest);
                Err(SocialError::not_linked())
            }
            Err(failure) => Err(failure.error),
        }
    }

    fn token_rejected(&self, token: &Token, guest: bool) {
        let mut state = self.lock();
        if guest {
            if state
                .guest
                .as_ref()
                .is_some_and(|item| &item.token == token)
            {
                state.guest = None;
                self.tokens_changed(&mut state, None);
            }
            return;
        }
        if state
            .account
            .as_ref()
            .is_some_and(|item| &item.token == token)
        {
            if self.vault.delete(&self.account_key).is_err() {
                eprintln!("social: the rejected account token could not be removed");
            }
            state.account = None;
            self.tokens_changed(&mut state, None);
        }
    }

    fn unlink(&self) -> Result<Value, SocialError> {
        let token = {
            let mut state = self.lock();
            if let Some(attempt) = state.link.take() {
                self.emit_account(&state, Some((&attempt.login_id, "cancelled")));
                self.wake.notify_all();
            }
            match &state.account {
                Some(account) => account.token.clone(),
                None => return Ok(json!({})),
            }
        };
        match self.call(
            Auth::Account,
            Call::post(&["v1", "auth", "logout"], json!({})),
        ) {
            Ok(_) => {}
            Err(error) if error.code == "social_not_linked" => return Ok(json!({})),
            Err(error) => return Err(error),
        }
        let mut state = self.lock();
        if !state
            .account
            .as_ref()
            .is_some_and(|item| item.token == token)
        {
            return Ok(json!({}));
        }
        if self.vault.delete(&self.account_key).is_err() {
            eprintln!(
                "social: the signed-out account's token could not be removed from the OS credential store"
            );
        }
        state.account = None;
        self.tokens_changed(&mut state, None);
        Ok(json!({}))
    }

    fn link_cancel(&self, login_id: &str) {
        let mut state = self.lock();
        if state
            .link
            .as_ref()
            .is_some_and(|attempt| attempt.login_id == login_id && !attempt.committing)
        {
            state.link = None;
            self.emit_account(&state, Some((login_id, "cancelled")));
            self.wake.notify_all();
        }
    }

    fn ensure_guest(&self, display_name: &str) -> Result<(), SocialError> {
        let config = {
            let state = self.lock();
            if state
                .guest
                .as_ref()
                .is_some_and(|guest| guest.expires > Instant::now())
            {
                return Ok(());
            }
            state.config.clone()
        };
        let result = self
            .backend
            .send(
                &config,
                Call::post(
                    &["v1", "auth", "guest"],
                    json!({"displayName": display_name}),
                ),
                None,
            )
            .map_err(|failure| failure.error)?;
        let invalid = || {
            SocialError::new(
                "invalid_upstream_response",
                "The social service returned an invalid guest session",
            )
        };
        let token = Token::parse(&result["token"]).ok_or_else(invalid)?;
        let id = result["guest"]["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(invalid)?
            .to_owned();
        let display_name = result["guest"]["displayName"]
            .as_str()
            .unwrap_or(display_name)
            .chars()
            .take(64)
            .collect();
        let mut state = self.lock();
        if state.account.is_none() {
            state.guest = Some(Guest {
                token,
                id,
                display_name,
                expires: Instant::now() + GUEST_TOKEN_LIFETIME,
            });
            self.tokens_changed(&mut state, None);
        }
        Ok(())
    }

    fn ice_servers(&self) -> Result<Value, SocialError> {
        let generation = {
            let state = self.lock();
            if let Some(cache) = &state.ice
                && cache.generation == state.generation
                && cache.expires > Instant::now()
            {
                return Ok(cache.value.clone());
            }
            state.generation
        };
        let value = self.call(Auth::Lobby, Call::get(&["v1", "ice-servers"]))?;
        let usable = value["ttlSeconds"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(ICE_REFRESH_MARGIN_SECONDS);
        let mut state = self.lock();
        if usable > 0 && state.generation == generation {
            state.ice = Some(IceCache {
                generation,
                expires: Instant::now() + Duration::from_secs(usable),
                value: value.clone(),
            });
        }
        Ok(value)
    }

    fn send_signal(&self, params: &Value) -> Result<Value, SocialError> {
        let lobby_id = id_param(params, "lobbyId")?;
        let to = id_param(params, "to")?;
        let payload = &params["payload"];
        if !payload.is_object() {
            return Err(SocialError::invalid("payload must be an object"));
        }
        if serde_json::to_vec(payload).map_or(usize::MAX, |bytes| bytes.len())
            > MAXIMUM_SIGNAL_PAYLOAD_BYTES
        {
            return Err(SocialError::invalid("payload must be at most 16 KiB"));
        }
        let frame =
            json!({"type":"signal", "lobbyId":lobby_id, "to":to, "payload":payload}).to_string();
        let mut state = self.lock();
        if state.connection != Connection::Connected {
            return Err(SocialError::offline());
        }
        if state.outbound.len() >= MAXIMUM_OUTBOUND_FRAMES {
            return Err(SocialError::new(
                "busy",
                "Too many co-op messages are waiting to be sent",
            ));
        }
        state.outbound.push_back(frame);
        Ok(json!({}))
    }

    fn request_jobs(&self, state: &mut State, account: bool, friends: bool) {
        state.jobs.account |= account;
        state.jobs.friends |= friends;
        self.wake.notify_all();
    }

    /// Socket authenticated: publish the state, replay presence, and refetch
    /// what may have changed while disconnected.
    fn connected(&self, state: &mut State, ready: &Value, account: bool) {
        self.set_connection(state, Connection::Connected);
        if !account {
            return;
        }
        if let Some(current) = &mut state.account
            && ready["user"].is_object()
            && current.user != ready["user"]
        {
            current.user = ready["user"].clone();
            self.persist_account(current);
            self.emit_account(state, None);
        }
        state.presence.dirty = state.presence.latest.is_some();
        self.request_jobs(state, true, true);
    }

    fn handle_frame(&self, text: &str) {
        let Ok(Value::Object(mut frame)) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let Some(Value::String(kind)) = frame.remove("type") else {
            return;
        };
        let name = match kind.as_str() {
            "presence.update" => "social.presence.changed",
            "invite.received" => "coop.invite.received",
            "invite.updated" => "coop.invite.updated",
            "joinRequest.received" => "coop.joinRequest.received",
            "joinRequest.updated" => "coop.joinRequest.updated",
            "lobby.updated" => "coop.lobby.updated",
            "lobby.ended" => "coop.lobby.ended",
            "lobby.kicked" => "coop.lobby.kicked",
            "signal" => "coop.signal",
            "friends.changed" => {
                let mut state = self.lock();
                self.request_jobs(&mut state, false, true);
                return;
            }
            "error" => {
                log_error_frame(&Value::Object(frame));
                return;
            }
            _ => return,
        };
        self.emit(name, Value::Object(frame));
    }
}

fn link_start(shared: &Arc<Shared>) -> Result<Value, SocialError> {
    let config = shared.lock().config.clone();
    let result = shared
        .backend
        .send(
            &config,
            Call::post(&["v1", "auth", "discord", "start"], json!({})),
            None,
        )
        .map_err(|failure| failure.error)?;
    let invalid = || {
        SocialError::new(
            "invalid_upstream_response",
            "The social service returned an invalid Discord sign-in",
        )
    };
    let login_id = result["loginId"]
        .as_str()
        .filter(|id| valid_id(id))
        .ok_or_else(invalid)?
        .to_owned();
    let poll_secret = Token::parse(&result["pollSecret"]).ok_or_else(invalid)?;
    let login_url = result["loginUrl"]
        .as_str()
        .filter(|url| url.len() <= 4_096)
        .and_then(|url| Url::parse(url).ok())
        .filter(|url| url.scheme() == "https")
        .ok_or_else(invalid)?;
    let expires_at = result["expiresAt"].as_str().unwrap_or_default().to_owned();
    let lifetime = chrono::DateTime::parse_from_rfc3339(&expires_at)
        .ok()
        .and_then(|expires| {
            (expires.with_timezone(&chrono::Utc) - chrono::Utc::now())
                .to_std()
                .ok()
        })
        .map_or(DEFAULT_LINK_LIFETIME, |lifetime| {
            lifetime.min(MAXIMUM_LINK_LIFETIME)
        });
    let interval = Duration::from_millis(
        result["intervalMs"]
            .as_u64()
            .unwrap_or(2_000)
            .clamp(250, 10_000),
    );
    {
        let mut state = shared.lock();
        if let Some(previous) = state.link.take() {
            shared.emit_account(&state, Some((&previous.login_id, "cancelled")));
        }
        state.link = Some(LinkAttempt {
            login_id: login_id.clone(),
            poll_secret,
            deadline: Instant::now() + lifetime,
            interval,
            committing: false,
        });
        shared.wake.notify_all();
        shared.emit_account(&state, None);
    }
    let poller = Arc::clone(shared);
    let poll_id = login_id.clone();
    thread::Builder::new()
        .name("opennow-social-link".to_owned())
        .spawn(move || poll_link(&poller, &poll_id))
        .map_err(|error| SocialError::new("social_link_failed", error.to_string()))?;
    Ok(json!({"loginId": login_id, "loginUrl": login_url.as_str(), "expiresAt": expires_at}))
}

/// Polls one Discord sign-in until it completes, is denied, expires, or is
/// cancelled (removed from the state). Completion enters a commit fence after
/// which cancellation no longer applies.
fn poll_link(shared: &Shared, login_id: &str) {
    let current = |state: &State| {
        state
            .link
            .as_ref()
            .is_some_and(|attempt| attempt.login_id == login_id)
    };
    loop {
        let (config, secret, deadline) = {
            let mut state = shared.lock();
            let Some(attempt) = state.link.as_ref().filter(|_| current(&state)) else {
                return;
            };
            let wake_at = (Instant::now() + attempt.interval).min(attempt.deadline);
            while current(&state) && !state.shutdown {
                let remaining = wake_at.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                state = shared
                    .wake
                    .wait_timeout(state, remaining)
                    .expect("social state poisoned")
                    .0;
            }
            if state.shutdown || !current(&state) {
                return;
            }
            let attempt = state.link.as_ref().expect("current link");
            (
                state.config.clone(),
                attempt.poll_secret.clone(),
                attempt.deadline,
            )
        };
        if Instant::now() >= deadline {
            finish_link(shared, login_id, "expired");
            return;
        }
        let result = shared.backend.send(
            &config,
            Call::post(
                &["v1", "auth", "discord", "poll"],
                json!({"loginId": login_id, "pollSecret": secret.expose()}),
            ),
            None,
        );
        let value = match result {
            Ok(value) => value,
            Err(failure) if failure.transient => continue,
            Err(_) => {
                finish_link(shared, login_id, "expired");
                return;
            }
        };
        match value["status"].as_str() {
            Some("complete") => {
                let (Some(token), true) =
                    (Token::parse(&value["token"]), value["user"].is_object())
                else {
                    continue;
                };
                complete_link(shared, login_id, token, value["user"].clone());
                return;
            }
            Some(status @ ("denied" | "expired")) => {
                finish_link(shared, login_id, status);
                return;
            }
            _ => {}
        }
    }
}

fn finish_link(shared: &Shared, login_id: &str, status: &str) {
    let mut state = shared.lock();
    if state
        .link
        .as_ref()
        .is_some_and(|attempt| attempt.login_id == login_id && !attempt.committing)
    {
        state.link = None;
        shared.emit_account(&state, Some((login_id, status)));
    }
}

fn complete_link(shared: &Shared, login_id: &str, token: Token, user: Value) {
    {
        let mut state = shared.lock();
        let Some(attempt) = state
            .link
            .as_mut()
            .filter(|attempt| attempt.login_id == login_id)
        else {
            return;
        };
        attempt.committing = true;
    }
    let account = Account {
        token,
        user,
        settings: Value::Null,
    };
    shared.persist_account(&account);
    let mut state = shared.lock();
    state.link = None;
    if state.shutdown {
        return;
    }
    state.account = Some(account);
    state.guest = None;
    shared.tokens_changed(&mut state, Some((login_id, "complete")));
    shared.request_jobs(&mut state, true, false);
}

/// The OS credential store is shared by every OpenNOW profile, so the entry is
/// scoped to the data directory like the per-profile GFN account index.
fn account_key(data_dir: &Path) -> String {
    use sha2::Digest as _;
    let digest = format!(
        "{:x}",
        sha2::Sha256::digest(data_dir.as_os_str().as_encoded_bytes())
    );
    format!("{ACCOUNT_KEY}:{}", &digest[..16])
}

impl Shared {
    fn persist_account(&self, account: &Account) {
        persist_account(&self.vault, &self.account_key, account);
    }
}

fn persist_account(vault: &TokenVault, key: &str, account: &Account) {
    let encoded = json!({"token": account.token.expose(), "user": account.user}).to_string();
    match vault.save(key, &encoded) {
        Ok("local-file") => {
            eprintln!(
                "social: the OS credential store is unavailable; the account token is saved in a private local file"
            );
        }
        Ok(_) => {}
        Err(_) => eprintln!("social: the account token could not be saved; it stays in memory"),
    }
}

fn decode_account(encoded: &str) -> Option<Account> {
    let value = serde_json::from_str::<Value>(encoded).ok()?;
    let token = Token::parse(&value["token"])?;
    value["user"].is_object().then(|| Account {
        token,
        user: value["user"].clone(),
        settings: Value::Null,
    })
}

fn run_jobs(shared: &Shared) {
    loop {
        let (account, generation) = {
            let mut state = shared.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if state.jobs.account || state.jobs.friends {
                    break;
                }
                state = shared.wake.wait(state).expect("social state poisoned");
            }
            let account = state.jobs.account;
            if account {
                state.jobs.account = false;
            } else {
                state.jobs.friends = false;
            }
            (account, state.generation)
        };
        if !account {
            if let Ok(friends) = shared.call(Auth::Account, Call::get(&["v1", "friends"])) {
                shared.emit("social.friends.changed", friends);
            }
            continue;
        }
        let Ok(me) = shared.call(Auth::Account, Call::get(&["v1", "me"])) else {
            continue;
        };
        let mut state = shared.lock();
        if state.generation != generation {
            continue;
        }
        let Some(current) = &mut state.account else {
            continue;
        };
        let user_changed = me["user"].is_object() && current.user != me["user"];
        let settings_changed = me["settings"].is_object() && current.settings != me["settings"];
        if user_changed {
            current.user = me["user"].clone();
            shared.persist_account(current);
        }
        if settings_changed {
            current.settings = me["settings"].clone();
        }
        if user_changed || settings_changed {
            shared.emit_account(&state, None);
        }
    }
}

/// The backend answers a frame it cannot accept with
/// `{"type":"error","error":{code,message}}`; only the stable code is logged.
fn log_error_frame(frame: &Value) {
    let code = frame["error"]["code"]
        .as_str()
        .filter(|code| {
            code.len() <= 64
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
        .unwrap_or("unknown");
    eprintln!("social: the backend rejected a socket frame: {code}");
}

const MAXIMUM_REPORT_ID_BYTES: usize = 64;
const MAXIMUM_REPORT_REASON_CHARS: usize = 256;
const MAXIMUM_REPORT_MILLISECONDS: u64 = 10 * 60 * 1000;

fn report_id<'a>(params: &'a Value, key: &str) -> Result<&'a str, SocialError> {
    id_param(params, key)
        .ok()
        .filter(|id| id.len() <= MAXIMUM_REPORT_ID_BYTES)
        .ok_or_else(|| {
            SocialError::invalid(format!(
                "{key} must be an identifier of at most 64 characters"
            ))
        })
}

/// `coop.report` body: every field is present, nullable fields are null when
/// absent, and the body never carries more than these bounded values.
fn report_body(params: &Value) -> Result<Value, SocialError> {
    let choice = |key: &str, allowed: &[&str], nullable: bool| -> Result<Value, SocialError> {
        match &params[key] {
            Value::Null if nullable => Ok(Value::Null),
            Value::String(value) if allowed.contains(&value.as_str()) => Ok(json!(value)),
            _ => Err(SocialError::invalid(format!(
                "{key} must be one of {}{}",
                allowed.join(", "),
                if nullable { " or null" } else { "" }
            ))),
        }
    };
    let optional_id = |key: &str| -> Result<Value, SocialError> {
        match &params[key] {
            Value::Null => Ok(Value::Null),
            _ => report_id(params, key).map(|id| json!(id)),
        }
    };
    let milliseconds = |key: &str| -> Result<Value, SocialError> {
        match &params[key] {
            Value::Null => Ok(Value::Null),
            value => value
                .as_u64()
                .filter(|value| *value < MAXIMUM_REPORT_MILLISECONDS)
                .map(|value| json!(value))
                .ok_or_else(|| {
                    SocialError::invalid(format!(
                        "{key} must be a non-negative integer below 600000"
                    ))
                }),
        }
    };
    let reason = match &params["reason"] {
        Value::Null => Value::Null,
        value => json!(bounded_text(value, "reason", MAXIMUM_REPORT_REASON_CHARS)?),
    };
    Ok(json!({
        "event": choice("event", &["first_frame", "ended"], false)?,
        "path": choice("path", &["direct", "relay"], true)?,
        "relayId": optional_id("relayId")?,
        "codec": optional_id("codec")?,
        "decoder": choice("decoder", &["hardware", "software"], true)?,
        "connectMs": milliseconds("connectMs")?,
        "firstFrameMs": milliseconds("firstFrameMs")?,
        "reason": reason,
    }))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAXIMUM_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn id_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, SocialError> {
    params[key]
        .as_str()
        .filter(|value| valid_id(value))
        .ok_or_else(|| SocialError::invalid(format!("{key} must be a valid identifier")))
}

fn bounded_text<'a>(
    value: &'a Value,
    key: &str,
    maximum_chars: usize,
) -> Result<&'a str, SocialError> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| {
            !text.is_empty()
                && text.chars().count() <= maximum_chars
                && !text.chars().any(char::is_control)
        })
        .ok_or_else(|| {
            SocialError::invalid(format!(
                "{key} must be a non-empty string of at most {maximum_chars} characters"
            ))
        })
}

fn username_param(params: &Value) -> Result<&str, SocialError> {
    bounded_text(&params["username"], "username", 64)
}

fn guest_name_param(params: &Value) -> Result<String, SocialError> {
    match &params["guestDisplayName"] {
        Value::Null => Ok("Guest".to_owned()),
        value => bounded_text(value, "guestDisplayName", 32).map(str::to_owned),
    }
}

fn code_param(params: &Value) -> Result<&str, SocialError> {
    params["code"]
        .as_str()
        .map(str::trim)
        .filter(|code| {
            !code.is_empty()
                && code.len() <= 16
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        .ok_or_else(|| SocialError::invalid("code must look like K7Q-4MX"))
}

fn max_guests_param(value: &Value) -> Result<u64, SocialError> {
    value
        .as_u64()
        .filter(|count| (1..=3).contains(count))
        .ok_or_else(|| SocialError::invalid("maxGuests must be 1, 2 or 3"))
}

fn game_param(value: &Value) -> Result<Value, SocialError> {
    let invalid = || SocialError::invalid("game must contain appId, title and artUrl");
    if !value.is_object() {
        return Err(invalid());
    }
    let app_id = bounded_text(&value["appId"], "game.appId", 128)?;
    let title = bounded_text(&value["title"], "game.title", 256)?;
    let art_url = match &value["artUrl"] {
        Value::Null => Value::Null,
        Value::String(url)
            if url.len() <= 2_048 && Url::parse(url).is_ok_and(|url| url.scheme() == "https") =>
        {
            json!(url)
        }
        _ => return Err(invalid()),
    };
    Ok(json!({"appId": app_id, "title": title, "artUrl": art_url}))
}

fn settings_patch(params: &Value) -> Result<Value, SocialError> {
    let object = params
        .as_object()
        .filter(|object| !object.is_empty())
        .ok_or_else(|| {
            SocialError::invalid("social.settings.update requires at least one setting")
        })?;
    for (key, value) in object {
        let valid = match key.as_str() {
            "presenceVisibility" | "joinRequests" => {
                matches!(value.as_str(), Some("friends" | "nobody"))
            }
            "shareActivity" | "botDmInvites" => value.is_boolean(),
            _ => {
                return Err(SocialError::invalid(format!(
                    "Unknown social setting: {key}"
                )));
            }
        };
        if !valid {
            return Err(SocialError::invalid(format!("{key} has an invalid value")));
        }
    }
    Ok(params.clone())
}

fn presence_frame(params: &Value) -> Result<String, SocialError> {
    let activity = params["activity"]
        .as_str()
        .filter(|activity| ACTIVITIES.contains(activity))
        .ok_or_else(|| SocialError::invalid("activity is not a known activity state"))?;
    let game = match &params["game"] {
        Value::Null => Value::Null,
        game => game_param(game)?,
    };
    let since = match &params["since"] {
        Value::Null => Value::Null,
        value => json!(bounded_text(value, "since", 64)?),
    };
    let queue_position = match &params["queuePosition"] {
        Value::Null => Value::Null,
        value => json!(
            value
                .as_u64()
                .filter(|position| *position <= 1_000_000)
                .ok_or_else(|| SocialError::invalid(
                    "queuePosition must be a non-negative integer"
                ))?
        ),
    };
    let lobby_id = match &params["lobbyId"] {
        Value::Null => Value::Null,
        _ => json!(id_param(params, "lobbyId")?),
    };
    Ok(json!({
        "type": "presence.set",
        "activity": activity,
        "game": game,
        "since": since,
        "queuePosition": queue_position,
        "lobbyId": lobby_id,
    })
    .to_string())
}
